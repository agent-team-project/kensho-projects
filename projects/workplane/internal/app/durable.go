package app

import (
	"bytes"
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"

	_ "github.com/lib/pq"
)

var (
	ErrNoOutbox      = errors.New("no eligible outbox record")
	ErrInjectedCrash = errors.New("injected outbox worker crash")
	ErrOutOfOrder    = errors.New("delivery is ahead of an unpublished earlier event")
)

type OutboxFault string

const (
	OutboxFaultNone         OutboxFault = ""
	OutboxFaultAfterClaim   OutboxFault = "after-claim"
	OutboxFaultAfterPublish OutboxFault = "after-publish"
)

type OutboxEnvelope struct {
	Sequence         int64           `json:"sequence"`
	EventID          string          `json:"event_id"`
	EventType        string          `json:"event_type"`
	SchemaVersion    int             `json:"schema_version"`
	OrganizationID   string          `json:"organization_id"`
	AggregateType    string          `json:"aggregate_type"`
	AggregateID      string          `json:"aggregate_id"`
	AggregateVersion int64           `json:"aggregate_version"`
	CommandID        string          `json:"command_id"`
	RequestID        string          `json:"request_id"`
	ActorKind        string          `json:"actor_kind"`
	ActorID          string          `json:"actor_id"`
	PrincipalID      *string         `json:"principal_id"`
	OccurredAt       string          `json:"occurred_at"`
	Payload          json.RawMessage `json:"payload"`
}

type DeliveryResult struct {
	Consumer       string    `json:"consumer"`
	Worker         string    `json:"worker"`
	EventID        string    `json:"event_id"`
	EventSequence  int64     `json:"event_sequence"`
	Attempt        int       `json:"attempt"`
	LeaseUntil     time.Time `json:"lease_until"`
	Published      bool      `json:"published"`
	Duplicate      bool      `json:"duplicate"`
	Checkpoint     int64     `json:"checkpoint"`
	Acknowledged   bool      `json:"acknowledged"`
	PayloadSHA256  string    `json:"payload_sha256"`
	InjectedFault  string    `json:"injected_fault,omitempty"`
	CheckpointTime time.Time `json:"checkpoint_time,omitempty"`
}

type ReplayReport struct {
	RunID             string `json:"run_id"`
	LastSequence      int64  `json:"last_sequence"`
	Projects          int    `json:"projects"`
	Decisions         int    `json:"decisions"`
	Activity          int    `json:"activity"`
	Planning          int    `json:"planning"`
	LiveChecksum      string `json:"live_checksum"`
	RebuiltChecksum   string `json:"rebuilt_checksum"`
	ActiveHeadUpdated bool   `json:"active_head_updated"`
}

type ReplayFailure struct {
	RunID         string `json:"run_id"`
	Code          string `json:"code"`
	Sequence      int64  `json:"sequence"`
	EventID       string `json:"event_id"`
	EventType     string `json:"event_type"`
	SchemaVersion int    `json:"schema_version"`
	Detail        string `json:"detail"`
}

func (failure *ReplayFailure) Error() string {
	return fmt.Sprintf("replay %s stopped at sequence %d event %s: %s", failure.Code, failure.Sequence, failure.EventID, failure.Detail)
}

type IntegrityFinding struct {
	Code      string `json:"code"`
	Sequence  int64  `json:"sequence,omitempty"`
	EventID   string `json:"event_id,omitempty"`
	Consumer  string `json:"consumer,omitempty"`
	Aggregate string `json:"aggregate,omitempty"`
	Detail    string `json:"detail"`
}

type DurableStore struct {
	db *sql.DB
}

func NewDurableStore(databaseURL string) (*DurableStore, error) {
	db, err := sql.Open("postgres", databaseURL)
	if err != nil {
		return nil, fmt.Errorf("open durable database: %w", err)
	}
	db.SetMaxOpenConns(8)
	db.SetMaxIdleConns(2)
	db.SetConnMaxLifetime(30 * time.Minute)
	return &DurableStore{db: db}, nil
}

func (store *DurableStore) Ping(ctx context.Context) error { return store.db.PingContext(ctx) }
func (store *DurableStore) Close() error                   { return store.db.Close() }

func (store *DurableStore) EnsureConsumer(ctx context.Context, consumer string) error {
	var enabled bool
	err := store.db.QueryRowContext(ctx, `SELECT enabled FROM outbox_consumers WHERE name=$1`, consumer).Scan(&enabled)
	if err == nil && enabled {
		return nil
	}
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return fmt.Errorf("read outbox consumer: %w", err)
	}
	tx, err := store.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `INSERT INTO outbox_consumers (name) VALUES ($1)
		ON CONFLICT (name) DO UPDATE SET enabled=true WHERE NOT outbox_consumers.enabled`, consumer); err != nil {
		return fmt.Errorf("register outbox consumer: %w", err)
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO outbox_delivery_state (consumer_name,event_id)
		SELECT $1,event_id FROM outbox_records ON CONFLICT DO NOTHING`, consumer); err != nil {
		return fmt.Errorf("backfill outbox consumer: %w", err)
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO consumer_checkpoints (consumer_name) VALUES ($1)
		ON CONFLICT DO NOTHING`, consumer); err != nil {
		return fmt.Errorf("initialize consumer checkpoint: %w", err)
	}
	return tx.Commit()
}

func (store *DurableStore) RunOutboxOnce(ctx context.Context, consumer, worker string, lease time.Duration, at time.Time, fault OutboxFault) (DeliveryResult, error) {
	if lease <= 0 || lease > 5*time.Minute {
		return DeliveryResult{}, fmt.Errorf("lease must be within (0,5m]")
	}
	if err := store.EnsureConsumer(ctx, consumer); err != nil {
		return DeliveryResult{}, err
	}
	at = at.UTC()
	leaseUntil := at.Add(lease)
	result := DeliveryResult{Consumer: consumer, Worker: worker, LeaseUntil: leaseUntil}
	var payload, storedHash []byte
	claim, err := store.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelReadCommitted})
	if err != nil {
		return result, err
	}
	err = claim.QueryRowContext(ctx, `WITH candidate AS (
		SELECT state.consumer_name,state.event_id
		FROM outbox_delivery_state state
		JOIN outbox_records record ON record.event_id=state.event_id
		WHERE state.consumer_name=$1 AND state.delivered_at IS NULL
		  AND record.available_at<=$2
		  AND (state.lease_until IS NULL OR state.lease_until<=$2)
		ORDER BY record.event_sequence
		FOR UPDATE OF state SKIP LOCKED
		LIMIT 1
	), claimed AS (
		UPDATE outbox_delivery_state state
		SET attempts=state.attempts+1,lease_owner=$3,lease_until=$4,updated_at=$2
		FROM candidate
		WHERE state.consumer_name=candidate.consumer_name AND state.event_id=candidate.event_id
		RETURNING state.event_id,state.attempts
	)
	SELECT claimed.event_id,record.event_sequence,claimed.attempts,record.payload,record.payload_sha256
	FROM claimed JOIN outbox_records record ON record.event_id=claimed.event_id`, consumer, at, worker, leaseUntil).Scan(
		&result.EventID, &result.EventSequence, &result.Attempt, &payload, &storedHash)
	if errors.Is(err, sql.ErrNoRows) {
		_ = claim.Rollback()
		return result, ErrNoOutbox
	}
	if err != nil {
		_ = claim.Rollback()
		return result, fmt.Errorf("claim outbox record: %w", err)
	}
	if err := claim.Commit(); err != nil {
		return result, fmt.Errorf("commit outbox lease: %w", err)
	}
	result.PayloadSHA256 = hex.EncodeToString(storedHash)
	if fault == OutboxFaultAfterClaim {
		result.InjectedFault = string(fault)
		return result, ErrInjectedCrash
	}
	publish, err := store.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelReadCommitted})
	if err != nil {
		return result, err
	}
	inserted, err := publish.ExecContext(ctx, `INSERT INTO consumer_deliveries
		(consumer_name,event_id,event_sequence,payload_sha256,first_published_at)
		VALUES ($1,$2,$3,$4,$5) ON CONFLICT (consumer_name,event_id) DO NOTHING`,
		consumer, result.EventID, result.EventSequence, storedHash, at)
	if err != nil {
		_ = publish.Rollback()
		return result, fmt.Errorf("publish outbox record: %w", err)
	}
	rows, err := inserted.RowsAffected()
	if err != nil {
		_ = publish.Rollback()
		return result, err
	}
	result.Published = true
	result.Duplicate = rows == 0
	if _, err := publish.ExecContext(ctx, `INSERT INTO outbox_delivery_attempts
		(consumer_name,event_id,attempt,lease_owner,published_at,duplicate,payload_sha256)
		VALUES ($1,$2,$3,$4,$5,$6,$7)`, consumer, result.EventID, result.Attempt, worker, at, result.Duplicate, storedHash); err != nil {
		_ = publish.Rollback()
		return result, fmt.Errorf("record publish attempt: %w", err)
	}
	if err := publish.Commit(); err != nil {
		return result, fmt.Errorf("commit publish effect: %w", err)
	}
	if fault == OutboxFaultAfterPublish {
		result.InjectedFault = string(fault)
		return result, ErrInjectedCrash
	}
	// The checkpoint row lock serializes workers for this consumer, while the
	// unique delivery effect and monotonic trigger protect identity/order. The
	// commit-horizon lock waits for every transaction that could still commit a
	// lower identity value. The following read-committed predicate can therefore
	// distinguish a committed missing delivery from a permanently aborted
	// sequence gap without making domain commands participate in SSI.
	checkpoint, err := store.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelReadCommitted})
	if err != nil {
		return result, err
	}
	defer checkpoint.Rollback()
	var previous int64
	if err := checkpoint.QueryRowContext(ctx, `SELECT last_sequence FROM consumer_checkpoints
		WHERE consumer_name=$1 FOR UPDATE`, consumer).Scan(&previous); err != nil {
		return result, fmt.Errorf("lock checkpoint: %w", err)
	}
	if _, err := checkpoint.ExecContext(ctx, `SELECT lock_domain_event_commit_horizon()`); err != nil {
		return result, fmt.Errorf("establish domain event commit horizon: %w", err)
	}
	var missingEarlier bool
	if err := checkpoint.QueryRowContext(ctx, `SELECT EXISTS (
		SELECT 1 FROM outbox_records earlier
		WHERE earlier.event_sequence<$2 AND NOT EXISTS (
			SELECT 1 FROM consumer_deliveries delivered
			WHERE delivered.consumer_name=$1 AND delivered.event_id=earlier.event_id
		)
	)`, consumer, result.EventSequence).Scan(&missingEarlier); err != nil {
		return result, fmt.Errorf("check earlier deliveries: %w", err)
	}
	if missingEarlier {
		released, err := checkpoint.ExecContext(ctx, `UPDATE outbox_delivery_state
			SET lease_owner=NULL,lease_until=NULL,updated_at=$4
			WHERE consumer_name=$1 AND event_id=$2 AND lease_owner=$3 AND delivered_at IS NULL`,
			consumer, result.EventID, worker, at)
		if err != nil {
			return result, fmt.Errorf("release out-of-order outbox lease: %w", err)
		}
		rows, err := released.RowsAffected()
		if err != nil || rows != 1 {
			return result, fmt.Errorf("outbox lease lost before out-of-order release")
		}
		if err := checkpoint.Commit(); err != nil {
			return result, fmt.Errorf("commit out-of-order lease release: %w", err)
		}
		return result, ErrOutOfOrder
	}
	result.Checkpoint = previous
	if result.EventSequence > previous {
		if _, err := checkpoint.ExecContext(ctx, `UPDATE consumer_checkpoints
			SET last_sequence=$2,last_event_id=$3,updated_at=$4 WHERE consumer_name=$1`,
			consumer, result.EventSequence, result.EventID, at); err != nil {
			return result, fmt.Errorf("advance checkpoint: %w", err)
		}
		result.Checkpoint = result.EventSequence
		result.CheckpointTime = at
	}
	updated, err := checkpoint.ExecContext(ctx, `UPDATE outbox_delivery_state
		SET delivered_at=$4,lease_owner=NULL,lease_until=NULL,updated_at=$4
		WHERE consumer_name=$1 AND event_id=$2 AND lease_owner=$3 AND delivered_at IS NULL`,
		consumer, result.EventID, worker, at)
	if err != nil {
		return result, fmt.Errorf("acknowledge outbox record: %w", err)
	}
	rows, err = updated.RowsAffected()
	if err != nil || rows != 1 {
		return result, fmt.Errorf("outbox lease lost before acknowledgement")
	}
	result.Acknowledged = true
	if err := checkpoint.Commit(); err != nil {
		return result, fmt.Errorf("commit checkpoint and acknowledgement: %w", err)
	}
	return result, nil
}

func (store *DurableStore) RunOutboxLoop(ctx context.Context, consumer, worker string, lease, poll time.Duration) error {
	for {
		_, err := store.RunOutboxOnce(ctx, consumer, worker, lease, time.Now().UTC(), OutboxFaultNone)
		switch {
		case err == nil:
			continue
		case errors.Is(err, ErrNoOutbox):
			timer := time.NewTimer(poll)
			select {
			case <-ctx.Done():
				timer.Stop()
				return nil
			case <-timer.C:
			}
		case errors.Is(err, ErrOutOfOrder):
			continue
		default:
			return err
		}
	}
}

type eventProjection struct {
	Sequence         int64   `json:"sequence"`
	EventID          string  `json:"event_id"`
	EventType        string  `json:"event_type"`
	SchemaVersion    int     `json:"schema_version"`
	OrganizationID   string  `json:"organization_id"`
	AggregateType    string  `json:"aggregate_type"`
	AggregateID      string  `json:"aggregate_id"`
	AggregateVersion int64   `json:"aggregate_version"`
	ActorKind        string  `json:"actor_kind"`
	ActorID          string  `json:"actor_id"`
	PrincipalID      *string `json:"principal_id"`
	CommandID        string  `json:"command_id"`
	RequestID        string  `json:"request_id"`
	OccurredAt       string  `json:"occurred_at"`
}

type eventRow struct {
	Projection eventProjection
	Payload    json.RawMessage
}

type projectionSnapshot struct {
	Projects      []Project         `json:"projects"`
	Decisions     []Decision        `json:"decisions"`
	Deliverables  []Deliverable     `json:"deliverables"`
	Forecasts     []Forecast        `json:"forecasts"`
	ForecastHeads []ForecastHead    `json:"forecast_heads"`
	Targets       []Target          `json:"targets"`
	TargetHeads   []TargetHead      `json:"target_heads"`
	Deadlines     []Deadline        `json:"deadlines"`
	DeadlineHeads []DeadlineHead    `json:"deadline_heads"`
	Activity      []eventProjection `json:"activity"`
}

func (snapshot projectionSnapshot) planningCount() int {
	return len(snapshot.Deliverables) + len(snapshot.Forecasts) + len(snapshot.ForecastHeads) +
		len(snapshot.Targets) + len(snapshot.TargetHeads) + len(snapshot.Deadlines) + len(snapshot.DeadlineHeads)
}

type databaseQueryer interface {
	QueryContext(context.Context, string, ...any) (*sql.Rows, error)
	QueryRowContext(context.Context, string, ...any) *sql.Row
}

func (store *DurableStore) Replay(ctx context.Context) (ReplayReport, error) {
	runID, err := newUUID()
	if err != nil {
		return ReplayReport{}, err
	}
	started := time.Now().UTC()
	if _, err := store.db.ExecContext(ctx, `INSERT INTO projection_replay_runs (id,status,started_at) VALUES ($1,'building',$2)`, runID, started); err != nil {
		return ReplayReport{}, fmt.Errorf("start replay run: %w", err)
	}
	tx, err := store.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable, ReadOnly: false})
	if err != nil {
		return ReplayReport{}, err
	}
	if _, err := tx.ExecContext(ctx, `SELECT pg_advisory_xact_lock(hashtextextended('workplane-projection-replay',0))`); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	events, err := loadEventRows(ctx, tx)
	if err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	live, err := loadLiveSnapshot(ctx, tx, events)
	if err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	rebuilt, failure := rebuildSnapshot(runID, events)
	if failure != nil {
		_ = tx.Rollback()
		return ReplayReport{}, store.recordReplayFailure(ctx, failure)
	}
	liveChecksum := snapshotChecksum(live)
	rebuiltChecksum := snapshotChecksum(rebuilt)
	lastSequence := int64(0)
	if len(events) > 0 {
		lastSequence = events[len(events)-1].Projection.Sequence
	}
	if liveChecksum != rebuiltChecksum {
		failure = &ReplayFailure{RunID: runID, Code: "checksum_mismatch", Sequence: lastSequence,
			Detail: fmt.Sprintf("live checksum %s differs from rebuilt checksum %s", liveChecksum, rebuiltChecksum)}
		_ = tx.Rollback()
		return ReplayReport{}, store.recordReplayFailure(ctx, failure)
	}
	for _, project := range rebuilt.Projects {
		encoded := canonicalJSON(project)
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_project_projections
			(run_id,organization_id,project_id,projection) VALUES ($1,$2,$3,$4)`, runID, project.OrganizationID, project.ID, encoded); err != nil {
			_ = tx.Rollback()
			return ReplayReport{}, fmt.Errorf("write replay project: %w", err)
		}
	}
	for _, decision := range rebuilt.Decisions {
		encoded := canonicalJSON(decision)
		var orgID string
		if err := tx.QueryRowContext(ctx, `SELECT organization_id FROM projects WHERE id=$1`, decision.ProjectID).Scan(&orgID); err != nil {
			_ = tx.Rollback()
			return ReplayReport{}, err
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_decision_projections
			(run_id,organization_id,decision_id,project_id,projection) VALUES ($1,$2,$3,$4,$5)`,
			runID, orgID, decision.ID, decision.ProjectID, encoded); err != nil {
			_ = tx.Rollback()
			return ReplayReport{}, fmt.Errorf("write replay decision: %w", err)
		}
	}
	for _, activity := range rebuilt.Activity {
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_activity_projections
			(run_id,sequence,event_id,projection) VALUES ($1,$2,$3,$4)`,
			runID, activity.Sequence, activity.EventID, canonicalJSON(activity)); err != nil {
			_ = tx.Rollback()
			return ReplayReport{}, fmt.Errorf("write replay activity: %w", err)
		}
	}
	if err := writeReplayPlanning(ctx, tx, runID, rebuilt); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	finished := time.Now().UTC()
	if _, err := tx.ExecContext(ctx, `UPDATE projection_replay_runs SET status='succeeded',finished_at=$2,last_sequence=$3,
		projects_count=$4,decisions_count=$5,activity_count=$6,planning_count=$7,live_checksum=$8,rebuilt_checksum=$9 WHERE id=$1`,
		runID, finished, lastSequence, len(rebuilt.Projects), len(rebuilt.Decisions), len(rebuilt.Activity), rebuilt.planningCount(), liveChecksum, rebuiltChecksum); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO projection_heads
		(name,run_id,last_sequence,checksum,projects_count,decisions_count,activity_count,planning_count,updated_at)
		VALUES ('m1-canonical',$1,$2,$3,$4,$5,$6,$7,$8)
		ON CONFLICT (name) DO UPDATE SET run_id=EXCLUDED.run_id,last_sequence=EXCLUDED.last_sequence,
		checksum=EXCLUDED.checksum,projects_count=EXCLUDED.projects_count,decisions_count=EXCLUDED.decisions_count,
		activity_count=EXCLUDED.activity_count,planning_count=EXCLUDED.planning_count,updated_at=EXCLUDED.updated_at`,
		runID, lastSequence, rebuiltChecksum, len(rebuilt.Projects), len(rebuilt.Decisions), len(rebuilt.Activity), rebuilt.planningCount(), finished); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	if err := tx.Commit(); err != nil {
		return ReplayReport{}, fmt.Errorf("commit replay generation: %w", err)
	}
	return ReplayReport{RunID: runID, LastSequence: lastSequence, Projects: len(rebuilt.Projects),
		Decisions: len(rebuilt.Decisions), Activity: len(rebuilt.Activity), Planning: rebuilt.planningCount(), LiveChecksum: liveChecksum,
		RebuiltChecksum: rebuiltChecksum, ActiveHeadUpdated: true}, nil
}

func writeReplayPlanning(ctx context.Context, tx *sql.Tx, runID string, snapshot projectionSnapshot) error {
	organizations := make(map[string]string, len(snapshot.Projects))
	for _, project := range snapshot.Projects {
		organizations[project.ID] = project.OrganizationID
	}
	type row struct {
		kind, id, projectID string
		value               any
	}
	rows := make([]row, 0, snapshot.planningCount())
	for _, item := range snapshot.Deliverables {
		rows = append(rows, row{"deliverable", item.ID, item.ProjectID, item})
	}
	for _, item := range snapshot.Forecasts {
		rows = append(rows, row{"forecast", item.ID, item.ProjectID, item})
	}
	for _, item := range snapshot.ForecastHeads {
		id := "project"
		if item.DeliverableID != nil {
			id = *item.DeliverableID
		}
		rows = append(rows, row{"forecast-head", id, item.ProjectID, item})
	}
	for _, item := range snapshot.Targets {
		rows = append(rows, row{"target", item.ID, item.ProjectID, item})
	}
	for _, item := range snapshot.TargetHeads {
		rows = append(rows, row{"target-head", item.ProjectID, item.ProjectID, item})
	}
	for _, item := range snapshot.Deadlines {
		rows = append(rows, row{"deadline", item.ID, item.ProjectID, item})
	}
	for _, item := range snapshot.DeadlineHeads {
		rows = append(rows, row{"deadline-head", item.ProjectID, item.ProjectID, item})
	}
	for _, item := range rows {
		organizationID := organizations[item.projectID]
		if organizationID == "" {
			return fmt.Errorf("planning projection %s:%s references unknown project %s", item.kind, item.id, item.projectID)
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_planning_projections
			(run_id,kind,projection_id,organization_id,project_id,projection) VALUES ($1,$2,$3,$4,$5,$6)`,
			runID, item.kind, item.id, organizationID, item.projectID, canonicalJSON(item.value)); err != nil {
			return fmt.Errorf("write replay planning %s:%s: %w", item.kind, item.id, err)
		}
	}
	return nil
}

func (store *DurableStore) recordReplayFailure(ctx context.Context, failure *ReplayFailure) error {
	if err := store.persistReplayFailure(ctx, failure); err != nil {
		return errors.Join(failure, fmt.Errorf("persist replay failure: %w", err))
	}
	return failure
}

func (store *DurableStore) persistReplayFailure(ctx context.Context, failure *ReplayFailure) error {
	result, err := store.db.ExecContext(ctx, `UPDATE projection_replay_runs SET status='failed',finished_at=$2,
		last_sequence=$3,failed_sequence=$3,failed_event_id=NULLIF($4,'')::uuid,failure_code=$5,failure_detail=$6 WHERE id=$1`,
		failure.RunID, time.Now().UTC(), failure.Sequence, failure.EventID, failure.Code, failure.Detail)
	if err != nil {
		return err
	}
	updated, err := result.RowsAffected()
	if err != nil {
		return err
	}
	if updated != 1 {
		return fmt.Errorf("updated %d replay run rows, want 1", updated)
	}
	return nil
}

func rebuildSnapshot(runID string, events []eventRow) (projectionSnapshot, *ReplayFailure) {
	projects := make(map[string]Project)
	decisions := make([]Decision, 0)
	deliverables := make(map[string]Deliverable)
	forecasts := make(map[string]Forecast)
	forecastHeads := make(map[string]ForecastHead)
	pendingForecasts := make(map[string]ForecastSupersession)
	pendingForecastCommands := make(map[string]string)
	targets := make(map[string]Target)
	targetHeads := make(map[string]TargetHead)
	deadlines := make(map[string]Deadline)
	deadlineHeads := make(map[string]DeadlineHead)
	activity := make([]eventProjection, 0, len(events))
	versions := make(map[string]int64)
	for _, event := range events {
		item := event.Projection
		failure := func(code, detail string) (projectionSnapshot, *ReplayFailure) {
			return projectionSnapshot{}, &ReplayFailure{RunID: runID, Code: code, Sequence: item.Sequence,
				EventID: item.EventID, EventType: item.EventType, SchemaVersion: item.SchemaVersion, Detail: detail}
		}
		if item.SchemaVersion != 1 {
			return failure("unknown_event_schema", fmt.Sprintf("schema version %d is not supported", item.SchemaVersion))
		}
		aggregate := item.AggregateType + ":" + item.AggregateID
		expected := versions[aggregate] + 1
		if item.AggregateVersion != expected {
			return failure("aggregate_version_integrity", fmt.Sprintf("aggregate version %d, expected %d", item.AggregateVersion, expected))
		}
		versions[aggregate] = item.AggregateVersion
		switch item.EventType {
		case "project.created":
			var project Project
			if err := decodeStrictJSON(event.Payload, &project); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			if project.ID != item.AggregateID || project.OrganizationID != item.OrganizationID || project.Version != item.AggregateVersion {
				return failure("invalid_event_payload", "project identity/version does not match its envelope")
			}
			if _, exists := projects[project.ID]; exists {
				return failure("duplicate_projection_identity", "project already exists")
			}
			projects[project.ID] = project
		case "decision.recorded":
			var decision Decision
			if err := decodeStrictJSON(event.Payload, &decision); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[decision.ProjectID]
			if !exists || decision.ProjectID != item.AggregateID {
				return failure("invalid_event_payload", "decision references an unknown project")
			}
			if decision.ActorID != item.ActorID || decision.ActorKind != item.ActorKind || !equalOptionalString(decision.PrincipalID, item.PrincipalID) {
				return failure("invalid_event_payload", "decision attribution does not match its envelope")
			}
			project.Version = item.AggregateVersion
			projects[project.ID] = project
			decisions = append(decisions, decision)
		case "project.activated", "project.held", "project.resumed":
			var transition ProjectTransition
			if err := decodeStrictJSON(event.Payload, &transition); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior, exists := projects[item.AggregateID]
			validTransition := exists && transition.Project.Mode == prior.Mode
			switch item.EventType {
			case "project.activated":
				validTransition = validTransition && prior.State == "proposed" && transition.Project.State == "active" &&
					forecastHeads[forecastScopeKey(prior.ID, nil)].ForecastID != ""
			case "project.held":
				validTransition = validTransition && prior.State == "active" && transition.Project.State == "held"
			case "project.resumed":
				validTransition = validTransition && prior.State == "held" && transition.Project.State == "active"
			}
			if !validTransition || transition.Project.ID != item.AggregateID ||
				transition.Project.OrganizationID != item.OrganizationID || transition.Project.Version != item.AggregateVersion || strings.TrimSpace(transition.Reason) == "" {
				return failure("invalid_event_payload", "project transition identity, version, or reason is invalid")
			}
			projects[item.AggregateID] = transition.Project
		case "project.promoted":
			var promotion PromotionEvent
			if err := decodeStrictJSON(event.Payload, &promotion); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior, exists := projects[item.AggregateID]
			required := false
			for _, deliverable := range deliverables {
				required = required || (deliverable.ProjectID == item.AggregateID && deliverable.Required && deliverable.State != "waived" && deliverable.State != "cancelled")
			}
			if !exists || prior.Mode != "exploration" || prior.State != "active" ||
				forecastHeads[forecastScopeKey(prior.ID, nil)].ForecastID == "" || !required || promotion.Project.ID != item.AggregateID ||
				promotion.Project.OrganizationID != item.OrganizationID || promotion.Project.Mode != "exploitation" ||
				promotion.Project.State != "active" || promotion.Project.Version != item.AggregateVersion ||
				strings.TrimSpace(promotion.ResidualUncertainty) == "" || strings.TrimSpace(promotion.PriorityRationale) == "" {
				return failure("invalid_event_payload", "promotion contract is incomplete")
			}
			projects[item.AggregateID] = promotion.Project
		case "deliverable.created", "deliverable.revised":
			var deliverable Deliverable
			if err := decodeStrictJSON(event.Payload, &deliverable); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[deliverable.ProjectID]
			if !exists || deliverable.ProjectID != item.AggregateID || deliverable.OrganizationID != item.OrganizationID {
				return failure("invalid_event_payload", "deliverable references an unknown project")
			}
			prior, priorExists := deliverables[deliverable.ID]
			invalidRevision := item.EventType == "deliverable.revised" && (!priorExists || deliverable.Version != prior.Version+1 ||
				deliverable.ProjectID != prior.ProjectID || deliverable.OrganizationID != prior.OrganizationID ||
				deliverable.CreatedBy != prior.CreatedBy || deliverable.CreatedAt != prior.CreatedAt)
			invalidAttribution := item.EventType == "deliverable.created" && deliverable.CreatedBy != item.ActorID
			if invalidAttribution || deliverable.Version < 1 ||
				(item.EventType == "deliverable.created" && (priorExists || deliverable.Version != 1)) || invalidRevision {
				return failure("invalid_event_payload", "deliverable create/revision identity is inconsistent")
			}
			deliverables[deliverable.ID] = deliverable
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "forecast.superseded":
			var supersession ForecastSupersession
			if err := decodeStrictJSON(event.Payload, &supersession); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[supersession.ProjectID]
			prior := forecasts[supersession.SupersededID]
			if !exists || supersession.ProjectID != item.AggregateID || prior.ID == "" || supersession.SupersedingID == "" ||
				prior.ProjectID != supersession.ProjectID || !equalOptionalString(prior.DeliverableID, supersession.DeliverableID) ||
				forecastHeads[forecastScopeKey(supersession.ProjectID, supersession.DeliverableID)].ForecastID != supersession.SupersededID ||
				pendingForecasts[supersession.SupersedingID].SupersedingID != "" {
				return failure("invalid_event_payload", "forecast supersession references an unknown scope")
			}
			pendingForecasts[supersession.SupersedingID] = supersession
			pendingForecastCommands[supersession.SupersedingID] = item.CommandID
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "forecast.created":
			var forecast Forecast
			if err := decodeStrictJSON(event.Payload, &forecast); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[forecast.ProjectID]
			if !exists || forecast.ProjectID != item.AggregateID || forecast.OrganizationID != item.OrganizationID {
				return failure("invalid_event_payload", "forecast references an unknown project")
			}
			if forecast.DeliverableID != nil {
				if deliverables[*forecast.DeliverableID].ProjectID != forecast.ProjectID {
					return failure("invalid_event_payload", "deliverable forecast scope is invalid")
				}
			}
			if forecast.CreatedBy != item.ActorID {
				return failure("invalid_event_payload", "forecast attribution does not match its envelope")
			}
			key := forecastScopeKey(forecast.ProjectID, forecast.DeliverableID)
			pending, hasPending := pendingForecasts[forecast.ID]
			if forecast.SupersedesID == nil {
				if hasPending || forecastHeads[key].ForecastID != "" {
					return failure("invalid_event_payload", "initial forecast conflicts with existing scope history")
				}
			} else if !hasPending || pending.SupersededID != *forecast.SupersedesID ||
				!equalOptionalString(pending.DeliverableID, forecast.DeliverableID) || pendingForecastCommands[forecast.ID] != item.CommandID {
				return failure("invalid_event_payload", "forecast creation does not complete its supersession event")
			}
			if _, duplicate := forecasts[forecast.ID]; duplicate {
				return failure("duplicate_projection_identity", "forecast already exists")
			}
			forecasts[forecast.ID] = forecast
			forecastHeads[key] = ForecastHead{ProjectID: forecast.ProjectID, DeliverableID: forecast.DeliverableID, ForecastID: forecast.ID}
			delete(pendingForecasts, forecast.ID)
			delete(pendingForecastCommands, forecast.ID)
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "target.changed":
			var target Target
			if err := decodeStrictJSON(event.Payload, &target); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[target.ProjectID]
			prior := targetHeads[target.ProjectID]
			validHistory := (prior.TargetID == "" && target.SupersedesID == nil) ||
				(prior.TargetID != "" && target.SupersedesID != nil && *target.SupersedesID == prior.TargetID)
			if !exists || !validHistory || target.CreatedBy != item.ActorID || targets[target.ID].ID != "" ||
				target.ProjectID != item.AggregateID || target.OrganizationID != item.OrganizationID {
				return failure("invalid_event_payload", "target references an unknown project")
			}
			targets[target.ID] = target
			targetHeads[target.ProjectID] = TargetHead{ProjectID: target.ProjectID, TargetID: target.ID}
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "deadline.changed":
			var deadline Deadline
			if err := decodeStrictJSON(event.Payload, &deadline); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[deadline.ProjectID]
			prior := deadlineHeads[deadline.ProjectID]
			validHistory := (prior.DeadlineID == "" && deadline.SupersedesID == nil) ||
				(prior.DeadlineID != "" && deadline.SupersedesID != nil && *deadline.SupersedesID == prior.DeadlineID)
			if !exists || !validHistory || deadline.CreatedBy != item.ActorID || deadlines[deadline.ID].ID != "" ||
				deadline.ProjectID != item.AggregateID || deadline.OrganizationID != item.OrganizationID {
				return failure("invalid_event_payload", "deadline references an unknown project")
			}
			deadlines[deadline.ID] = deadline
			deadlineHeads[deadline.ProjectID] = DeadlineHead{ProjectID: deadline.ProjectID, DeadlineID: deadline.ID}
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		default:
			return failure("unknown_event_schema", "event type is not registered in replay v1")
		}
		activity = append(activity, item)
	}
	if len(pendingForecasts) != 0 {
		return projectionSnapshot{}, &ReplayFailure{RunID: runID, Code: "invalid_event_payload", Detail: "forecast supersession event lacks its forecast creation event"}
	}
	projectList := make([]Project, 0, len(projects))
	for _, project := range projects {
		projectList = append(projectList, project)
	}
	sort.Slice(projectList, func(i, j int) bool {
		if projectList[i].OrganizationID != projectList[j].OrganizationID {
			return projectList[i].OrganizationID < projectList[j].OrganizationID
		}
		return projectList[i].ID < projectList[j].ID
	})
	sort.Slice(decisions, func(i, j int) bool {
		if decisions[i].ProjectID != decisions[j].ProjectID {
			return decisions[i].ProjectID < decisions[j].ProjectID
		}
		return decisions[i].ID < decisions[j].ID
	})
	snapshot := projectionSnapshot{Projects: projectList, Decisions: decisions, Activity: activity}
	for _, item := range deliverables {
		snapshot.Deliverables = append(snapshot.Deliverables, item)
	}
	for _, item := range forecasts {
		snapshot.Forecasts = append(snapshot.Forecasts, item)
	}
	for _, item := range forecastHeads {
		snapshot.ForecastHeads = append(snapshot.ForecastHeads, item)
	}
	for _, item := range targets {
		snapshot.Targets = append(snapshot.Targets, item)
	}
	for _, item := range targetHeads {
		snapshot.TargetHeads = append(snapshot.TargetHeads, item)
	}
	for _, item := range deadlines {
		snapshot.Deadlines = append(snapshot.Deadlines, item)
	}
	for _, item := range deadlineHeads {
		snapshot.DeadlineHeads = append(snapshot.DeadlineHeads, item)
	}
	sortPlanningSnapshot(&snapshot)
	return snapshot, nil
}

func loadEventRows(ctx context.Context, queryer databaseQueryer) ([]eventRow, error) {
	rows, err := queryer.QueryContext(ctx, `SELECT sequence,event_id,event_type,schema_version,organization_id,
		aggregate_type,aggregate_id,aggregate_version,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload
		FROM domain_events ORDER BY sequence`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	events := make([]eventRow, 0)
	for rows.Next() {
		var item eventRow
		var occurredAt time.Time
		if err := rows.Scan(&item.Projection.Sequence, &item.Projection.EventID, &item.Projection.EventType,
			&item.Projection.SchemaVersion, &item.Projection.OrganizationID, &item.Projection.AggregateType,
			&item.Projection.AggregateID, &item.Projection.AggregateVersion, &item.Projection.ActorKind,
			&item.Projection.ActorID, &item.Projection.PrincipalID, &item.Projection.CommandID,
			&item.Projection.RequestID, &occurredAt, &item.Payload); err != nil {
			return nil, err
		}
		item.Projection.OccurredAt = occurredAt.UTC().Format(timeFormat)
		events = append(events, item)
	}
	return events, rows.Err()
}

func loadLiveSnapshot(ctx context.Context, queryer databaseQueryer, events []eventRow) (projectionSnapshot, error) {
	rows, err := queryer.QueryContext(ctx, selectProject+` ORDER BY organization_id,id`)
	if err != nil {
		return projectionSnapshot{}, err
	}
	projects := make([]Project, 0)
	for rows.Next() {
		project, err := scanProject(rows)
		if err != nil {
			_ = rows.Close()
			return projectionSnapshot{}, err
		}
		projects = append(projects, project)
	}
	if err := rows.Close(); err != nil {
		return projectionSnapshot{}, err
	}
	decisionRows, err := queryer.QueryContext(ctx, `SELECT d.id,d.project_id,d.actor_id,principal.kind,event.principal_id,d.recorded_at,
		d.kind,d.question,d.choice,d.alternatives,d.rationale,d.evidence,d.consequences
		FROM decisions d
		JOIN principals principal ON principal.id=d.actor_id
		LEFT JOIN LATERAL (
			SELECT ledger.event_id,ledger.principal_id
			FROM domain_events ledger
			WHERE ledger.event_type='decision.recorded' AND ledger.payload->>'id'=d.id::text
			ORDER BY ledger.sequence
			LIMIT 1
		) event ON true
		ORDER BY d.project_id,d.id`)
	if err != nil {
		return projectionSnapshot{}, err
	}
	defer decisionRows.Close()
	decisions := make([]Decision, 0)
	for decisionRows.Next() {
		var decision Decision
		var recordedAt time.Time
		var alternatives, evidence, consequences []byte
		var principalID sql.NullString
		if err := decisionRows.Scan(&decision.ID, &decision.ProjectID, &decision.ActorID, &decision.ActorKind,
			&principalID, &recordedAt, &decision.Kind, &decision.Question, &decision.Choice,
			&alternatives, &decision.Rationale, &evidence, &consequences); err != nil {
			return projectionSnapshot{}, err
		}
		if principalID.Valid {
			decision.PrincipalID = &principalID.String
		}
		decision.RecordedAt = recordedAt.UTC().Format(timeFormat)
		if err := json.Unmarshal(alternatives, &decision.Alternatives); err != nil {
			return projectionSnapshot{}, err
		}
		if err := json.Unmarshal(evidence, &decision.Evidence); err != nil {
			return projectionSnapshot{}, err
		}
		if err := json.Unmarshal(consequences, &decision.Consequences); err != nil {
			return projectionSnapshot{}, err
		}
		decisions = append(decisions, decision)
	}
	if err := decisionRows.Err(); err != nil {
		return projectionSnapshot{}, err
	}
	snapshot := projectionSnapshot{Projects: projects, Decisions: decisions}
	if err := loadLivePlanning(ctx, queryer, &snapshot); err != nil {
		return projectionSnapshot{}, err
	}
	activity := make([]eventProjection, len(events))
	for index, event := range events {
		activity[index] = event.Projection
	}
	snapshot.Activity = activity
	return snapshot, nil
}

func loadLivePlanning(ctx context.Context, queryer databaseQueryer, snapshot *projectionSnapshot) error {
	deliverableRows, err := queryer.QueryContext(ctx, selectDeliverable+` ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for deliverableRows.Next() {
		item, err := scanDeliverable(deliverableRows)
		if err != nil {
			_ = deliverableRows.Close()
			return err
		}
		snapshot.Deliverables = append(snapshot.Deliverables, item)
	}
	if err := deliverableRows.Err(); err != nil {
		_ = deliverableRows.Close()
		return fmt.Errorf("scan live deliverables: %w", err)
	}
	if err := deliverableRows.Close(); err != nil {
		return err
	}

	forecastRows, err := queryer.QueryContext(ctx, selectForecast+` ORDER BY project_id,deliverable_id NULLS FIRST,id`)
	if err != nil {
		return err
	}
	for forecastRows.Next() {
		item, err := scanForecast(forecastRows)
		if err != nil {
			_ = forecastRows.Close()
			return err
		}
		snapshot.Forecasts = append(snapshot.Forecasts, item)
	}
	if err := forecastRows.Err(); err != nil {
		_ = forecastRows.Close()
		return fmt.Errorf("scan live forecasts: %w", err)
	}
	if err := forecastRows.Close(); err != nil {
		return err
	}

	headRows, err := queryer.QueryContext(ctx, `SELECT project_id,deliverable_id,forecast_id FROM forecast_heads ORDER BY project_id,scope_key`)
	if err != nil {
		return err
	}
	for headRows.Next() {
		var item ForecastHead
		var deliverableID sql.NullString
		if err := headRows.Scan(&item.ProjectID, &deliverableID, &item.ForecastID); err != nil {
			_ = headRows.Close()
			return err
		}
		if deliverableID.Valid {
			item.DeliverableID = &deliverableID.String
		}
		snapshot.ForecastHeads = append(snapshot.ForecastHeads, item)
	}
	if err := headRows.Err(); err != nil {
		_ = headRows.Close()
		return fmt.Errorf("scan live forecast heads: %w", err)
	}
	if err := headRows.Close(); err != nil {
		return err
	}

	targetRows, err := queryer.QueryContext(ctx, `SELECT id,organization_id,project_id,target_at,reason,supersedes_id,created_by,created_at FROM project_targets ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for targetRows.Next() {
		var item Target
		var targetAt, created time.Time
		var supersedes sql.NullString
		if err := targetRows.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &targetAt, &item.Reason, &supersedes, &item.CreatedBy, &created); err != nil {
			_ = targetRows.Close()
			return err
		}
		if supersedes.Valid {
			item.SupersedesID = &supersedes.String
		}
		item.TargetAt, item.CreatedAt = targetAt.UTC().Format(timeFormat), created.UTC().Format(timeFormat)
		snapshot.Targets = append(snapshot.Targets, item)
	}
	if err := targetRows.Err(); err != nil {
		_ = targetRows.Close()
		return fmt.Errorf("scan live targets: %w", err)
	}
	if err := targetRows.Close(); err != nil {
		return err
	}

	targetHeadRows, err := queryer.QueryContext(ctx, `SELECT project_id,target_id FROM project_target_heads ORDER BY project_id`)
	if err != nil {
		return err
	}
	for targetHeadRows.Next() {
		var item TargetHead
		if err := targetHeadRows.Scan(&item.ProjectID, &item.TargetID); err != nil {
			_ = targetHeadRows.Close()
			return err
		}
		snapshot.TargetHeads = append(snapshot.TargetHeads, item)
	}
	if err := targetHeadRows.Err(); err != nil {
		_ = targetHeadRows.Close()
		return fmt.Errorf("scan live target heads: %w", err)
	}
	if err := targetHeadRows.Close(); err != nil {
		return err
	}

	deadlineRows, err := queryer.QueryContext(ctx, `SELECT id,organization_id,project_id,deadline_at,source,description,supersedes_id,created_by,created_at FROM project_deadlines ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for deadlineRows.Next() {
		var item Deadline
		var deadlineAt, created time.Time
		var supersedes sql.NullString
		if err := deadlineRows.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &deadlineAt, &item.Source, &item.Description, &supersedes, &item.CreatedBy, &created); err != nil {
			_ = deadlineRows.Close()
			return err
		}
		if supersedes.Valid {
			item.SupersedesID = &supersedes.String
		}
		item.DeadlineAt, item.CreatedAt = deadlineAt.UTC().Format(timeFormat), created.UTC().Format(timeFormat)
		snapshot.Deadlines = append(snapshot.Deadlines, item)
	}
	if err := deadlineRows.Err(); err != nil {
		_ = deadlineRows.Close()
		return fmt.Errorf("scan live deadlines: %w", err)
	}
	if err := deadlineRows.Close(); err != nil {
		return err
	}

	deadlineHeadRows, err := queryer.QueryContext(ctx, `SELECT project_id,deadline_id FROM project_deadline_heads ORDER BY project_id`)
	if err != nil {
		return err
	}
	for deadlineHeadRows.Next() {
		var item DeadlineHead
		if err := deadlineHeadRows.Scan(&item.ProjectID, &item.DeadlineID); err != nil {
			_ = deadlineHeadRows.Close()
			return err
		}
		snapshot.DeadlineHeads = append(snapshot.DeadlineHeads, item)
	}
	if err := deadlineHeadRows.Err(); err != nil {
		_ = deadlineHeadRows.Close()
		return fmt.Errorf("scan live deadline heads: %w", err)
	}
	if err := deadlineHeadRows.Close(); err != nil {
		return err
	}
	sortPlanningSnapshot(snapshot)
	return nil
}

func snapshotChecksum(snapshot projectionSnapshot) string {
	digest := sha256.Sum256(canonicalJSON(snapshot))
	return hex.EncodeToString(digest[:])
}

func (store *DurableStore) ActiveProjectionHead(ctx context.Context) (ReplayReport, error) {
	var report ReplayReport
	err := store.db.QueryRowContext(ctx, `SELECT run_id,last_sequence,projects_count,decisions_count,activity_count,planning_count,checksum
		FROM projection_heads WHERE name='m1-canonical'`).Scan(&report.RunID, &report.LastSequence,
		&report.Projects, &report.Decisions, &report.Activity, &report.Planning, &report.RebuiltChecksum)
	if err != nil {
		return ReplayReport{}, err
	}
	report.ActiveHeadUpdated = true
	return report, nil
}

func (store *DurableStore) Doctor(ctx context.Context) ([]IntegrityFinding, error) {
	return doctor(ctx, store.db)
}

func DoctorTx(ctx context.Context, tx *sql.Tx) ([]IntegrityFinding, error) {
	return doctor(ctx, tx)
}

func doctor(ctx context.Context, queryer databaseQueryer) ([]IntegrityFinding, error) {
	findings := make([]IntegrityFinding, 0)
	versionRows, err := queryer.QueryContext(ctx, `SELECT aggregate_type,aggregate_id,aggregate_version,expected
		FROM (SELECT aggregate_type,aggregate_id,aggregate_version,
			row_number() OVER (PARTITION BY aggregate_type,aggregate_id ORDER BY aggregate_version,sequence) expected
			FROM domain_events) ranked WHERE aggregate_version<>expected ORDER BY aggregate_type,aggregate_id,aggregate_version`)
	if err != nil {
		return nil, err
	}
	for versionRows.Next() {
		var aggregateType, aggregateID string
		var actual, expected int64
		if err := versionRows.Scan(&aggregateType, &aggregateID, &actual, &expected); err != nil {
			_ = versionRows.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "event_version_gap", Aggregate: aggregateType + ":" + aggregateID,
			Detail: fmt.Sprintf("aggregate version %d appears where %d is required", actual, expected)})
	}
	if err := versionRows.Err(); err != nil {
		_ = versionRows.Close()
		return nil, fmt.Errorf("scan event version integrity: %w", err)
	}
	_ = versionRows.Close()
	duplicateRows, err := queryer.QueryContext(ctx, `SELECT aggregate_type,aggregate_id,aggregate_version,count(*)
		FROM domain_events GROUP BY aggregate_type,aggregate_id,aggregate_version HAVING count(*)>1
		ORDER BY aggregate_type,aggregate_id,aggregate_version`)
	if err != nil {
		return nil, err
	}
	for duplicateRows.Next() {
		var aggregateType, aggregateID string
		var version int64
		var count int
		if err := duplicateRows.Scan(&aggregateType, &aggregateID, &version, &count); err != nil {
			_ = duplicateRows.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "duplicate_aggregate_version", Aggregate: aggregateType + ":" + aggregateID,
			Detail: fmt.Sprintf("aggregate version %d occurs %d times", version, count)})
	}
	if err := duplicateRows.Err(); err != nil {
		_ = duplicateRows.Close()
		return nil, fmt.Errorf("scan duplicate aggregate versions: %w", err)
	}
	_ = duplicateRows.Close()
	missingRows, err := queryer.QueryContext(ctx, `SELECT event.sequence,event.event_id FROM domain_events event
		LEFT JOIN outbox_records record ON record.event_id=event.event_id AND record.event_sequence=event.sequence
		WHERE record.event_id IS NULL ORDER BY event.sequence`)
	if err != nil {
		return nil, err
	}
	for missingRows.Next() {
		var finding IntegrityFinding
		finding.Code = "event_without_outbox"
		finding.Detail = "committed event lacks its corresponding outbox record"
		if err := missingRows.Scan(&finding.Sequence, &finding.EventID); err != nil {
			_ = missingRows.Close()
			return nil, err
		}
		findings = append(findings, finding)
	}
	if err := missingRows.Err(); err != nil {
		_ = missingRows.Close()
		return nil, fmt.Errorf("scan event/outbox coupling: %w", err)
	}
	_ = missingRows.Close()
	orphanDecisionRows, err := queryer.QueryContext(ctx, `SELECT decision.id,decision.project_id
		FROM decisions decision
		LEFT JOIN domain_events event ON event.event_type='decision.recorded' AND event.payload->>'id'=decision.id::text
		WHERE event.event_id IS NULL ORDER BY decision.project_id,decision.id`)
	if err != nil {
		return nil, err
	}
	for orphanDecisionRows.Next() {
		var decisionID, projectID string
		if err := orphanDecisionRows.Scan(&decisionID, &projectID); err != nil {
			_ = orphanDecisionRows.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "decision_without_event", Aggregate: "project:" + projectID,
			Detail: fmt.Sprintf("decision projection %s lacks its decision.recorded event", decisionID)})
	}
	if err := orphanDecisionRows.Err(); err != nil {
		_ = orphanDecisionRows.Close()
		return nil, fmt.Errorf("scan decision/event coupling: %w", err)
	}
	_ = orphanDecisionRows.Close()
	planningOrphans, err := queryer.QueryContext(ctx, `
		SELECT kind,id,project_id FROM (
			SELECT 'deliverable' kind,deliverable.id::text id,deliverable.project_id
			FROM deliverables deliverable LEFT JOIN domain_events event
			  ON event.event_type IN ('deliverable.created','deliverable.revised') AND event.payload->>'id'=deliverable.id::text
			WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'forecast',forecast.id::text,forecast.project_id
			FROM forecasts forecast LEFT JOIN domain_events event
			  ON event.event_type='forecast.created' AND event.payload->>'id'=forecast.id::text
			WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'target',target.id::text,target.project_id
			FROM project_targets target LEFT JOIN domain_events event
			  ON event.event_type='target.changed' AND event.payload->>'id'=target.id::text
			WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'deadline',deadline.id::text,deadline.project_id
			FROM project_deadlines deadline LEFT JOIN domain_events event
			  ON event.event_type='deadline.changed' AND event.payload->>'id'=deadline.id::text
			WHERE event.event_id IS NULL
		) orphan ORDER BY kind,id`)
	if err != nil {
		return nil, err
	}
	for planningOrphans.Next() {
		var kind, id, projectID string
		if err := planningOrphans.Scan(&kind, &id, &projectID); err != nil {
			_ = planningOrphans.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "planning_projection_without_event", Aggregate: "project:" + projectID,
			Detail: fmt.Sprintf("%s projection %s lacks its immutable event", kind, id)})
	}
	if err := planningOrphans.Err(); err != nil {
		_ = planningOrphans.Close()
		return nil, fmt.Errorf("scan planning/event coupling: %w", err)
	}
	if err := planningOrphans.Close(); err != nil {
		return nil, err
	}

	headMismatches, err := queryer.QueryContext(ctx, `
		SELECT kind,project_id,id FROM (
			SELECT 'forecast' kind,head.project_id,head.forecast_id::text id FROM forecast_heads head
			JOIN forecasts forecast ON forecast.id=head.forecast_id
			WHERE forecast.project_id<>head.project_id OR forecast.deliverable_id IS DISTINCT FROM head.deliverable_id
			UNION ALL
			SELECT 'target',head.project_id,head.target_id::text FROM project_target_heads head
			JOIN project_targets target ON target.id=head.target_id WHERE target.project_id<>head.project_id
			UNION ALL
			SELECT 'deadline',head.project_id,head.deadline_id::text FROM project_deadline_heads head
			JOIN project_deadlines deadline ON deadline.id=head.deadline_id WHERE deadline.project_id<>head.project_id
		) mismatch ORDER BY kind,project_id`)
	if err != nil {
		return nil, err
	}
	for headMismatches.Next() {
		var kind, projectID, id string
		if err := headMismatches.Scan(&kind, &projectID, &id); err != nil {
			_ = headMismatches.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "planning_head_mismatch", Aggregate: "project:" + projectID,
			Detail: fmt.Sprintf("%s current head %s belongs to another scope", kind, id)})
	}
	if err := headMismatches.Err(); err != nil {
		_ = headMismatches.Close()
		return nil, fmt.Errorf("scan planning head integrity: %w", err)
	}
	if err := headMismatches.Close(); err != nil {
		return nil, err
	}
	outboxRows, err := queryer.QueryContext(ctx, `SELECT record.event_sequence,record.event_id,
		record.payload_sha256<>digest(convert_to(record.payload::text,'UTF8'),'sha256'),
		record.payload IS DISTINCT FROM domain_event_outbox_envelope(event),
		record.id IS DISTINCT FROM event.event_id OR record.event_sequence IS DISTINCT FROM event.sequence
			OR record.topic<>'domain-events' OR record.available_at IS DISTINCT FROM event.occurred_at
			OR record.created_at IS DISTINCT FROM event.occurred_at,
		EXISTS (
			SELECT 1 FROM consumer_deliveries delivery
			WHERE delivery.event_id=record.event_id AND (
				delivery.event_sequence IS DISTINCT FROM event.sequence
				OR delivery.payload_sha256 IS DISTINCT FROM digest(convert_to(domain_event_outbox_envelope(event)::text,'UTF8'),'sha256')
			)
		) OR EXISTS (
			SELECT 1 FROM outbox_delivery_attempts attempt
			WHERE attempt.event_id=record.event_id
				AND attempt.payload_sha256 IS DISTINCT FROM digest(convert_to(domain_event_outbox_envelope(event)::text,'UTF8'),'sha256')
		)
		FROM outbox_records record JOIN domain_events event ON event.event_id=record.event_id ORDER BY record.event_sequence`)
	if err != nil {
		return nil, err
	}
	for outboxRows.Next() {
		var sequence int64
		var eventID string
		var hashMismatch, envelopeMismatch, metadataMismatch, deliveryMismatch bool
		if err := outboxRows.Scan(&sequence, &eventID, &hashMismatch, &envelopeMismatch, &metadataMismatch, &deliveryMismatch); err != nil {
			_ = outboxRows.Close()
			return nil, err
		}
		if hashMismatch || envelopeMismatch || metadataMismatch || deliveryMismatch {
			findings = append(findings, IntegrityFinding{Code: "outbox_payload_mismatch", Sequence: sequence, EventID: eventID,
				Detail: "outbox metadata, complete canonical ledger envelope, checksum, or delivered hash differs from the event ledger"})
		}
	}
	if err := outboxRows.Err(); err != nil {
		_ = outboxRows.Close()
		return nil, fmt.Errorf("scan outbox integrity: %w", err)
	}
	_ = outboxRows.Close()
	unknownRows, err := queryer.QueryContext(ctx, `SELECT sequence,event_id,event_type,schema_version FROM domain_events
		WHERE schema_version<>1 OR event_type NOT IN (
			'project.created','decision.recorded','project.activated','project.held','project.resumed','project.promoted',
			'deliverable.created','deliverable.revised','forecast.created','forecast.superseded','target.changed','deadline.changed'
		) ORDER BY sequence`)
	if err != nil {
		return nil, err
	}
	for unknownRows.Next() {
		var finding IntegrityFinding
		var eventType string
		var schemaVersion int
		finding.Code = "unknown_event_schema"
		if err := unknownRows.Scan(&finding.Sequence, &finding.EventID, &eventType, &schemaVersion); err != nil {
			_ = unknownRows.Close()
			return nil, err
		}
		finding.Detail = fmt.Sprintf("event %s schema version %d is not replayable", eventType, schemaVersion)
		findings = append(findings, finding)
	}
	if err := unknownRows.Err(); err != nil {
		_ = unknownRows.Close()
		return nil, fmt.Errorf("scan unknown event schemas: %w", err)
	}
	_ = unknownRows.Close()
	checkpointRows, err := queryer.QueryContext(ctx, `SELECT checkpoint.consumer_name,checkpoint.last_sequence,checkpoint.last_event_id,
		COALESCE(max(delivery.event_sequence),0)
		FROM consumer_checkpoints checkpoint
		LEFT JOIN consumer_deliveries delivery ON delivery.consumer_name=checkpoint.consumer_name
		GROUP BY checkpoint.consumer_name,checkpoint.last_sequence,checkpoint.last_event_id ORDER BY checkpoint.consumer_name`)
	if err != nil {
		return nil, err
	}
	type checkpointValue struct {
		consumer              string
		checkpoint, delivered int64
		eventID               sql.NullString
	}
	checkpointValues := make([]checkpointValue, 0)
	for checkpointRows.Next() {
		var value checkpointValue
		if err := checkpointRows.Scan(&value.consumer, &value.checkpoint, &value.eventID, &value.delivered); err != nil {
			_ = checkpointRows.Close()
			return nil, err
		}
		checkpointValues = append(checkpointValues, value)
	}
	if err := checkpointRows.Err(); err != nil {
		_ = checkpointRows.Close()
		return nil, fmt.Errorf("scan checkpoint integrity: %w", err)
	}
	_ = checkpointRows.Close()
	for _, value := range checkpointValues {
		if value.checkpoint < value.delivered {
			findings = append(findings, IntegrityFinding{Code: "stale_checkpoint", Consumer: value.consumer,
				Detail: fmt.Sprintf("checkpoint %d trails published effect %d", value.checkpoint, value.delivered)})
		} else if value.checkpoint > value.delivered {
			findings = append(findings, IntegrityFinding{Code: "checkpoint_ahead", Consumer: value.consumer,
				Detail: fmt.Sprintf("checkpoint %d exceeds published effect %d", value.checkpoint, value.delivered)})
		}
		if value.checkpoint > 0 {
			var expected string
			if err := queryer.QueryRowContext(ctx, `SELECT event_id FROM outbox_records WHERE event_sequence=$1`, value.checkpoint).Scan(&expected); err != nil || !value.eventID.Valid || expected != value.eventID.String {
				findings = append(findings, IntegrityFinding{Code: "checkpoint_identity_mismatch", Consumer: value.consumer,
					Detail: fmt.Sprintf("checkpoint event identity does not match sequence %d", value.checkpoint)})
			}
		}
	}
	events, err := loadEventRows(ctx, queryer)
	if err != nil {
		return nil, err
	}
	live, err := loadLiveSnapshot(ctx, queryer, events)
	if err != nil {
		return nil, err
	}
	var headChecksum string
	if err := queryer.QueryRowContext(ctx, `SELECT checksum FROM projection_heads WHERE name='m1-canonical'`).Scan(&headChecksum); err == nil {
		current := snapshotChecksum(live)
		if current != headChecksum {
			findings = append(findings, IntegrityFinding{Code: "projection_checksum_drift",
				Detail: fmt.Sprintf("active checksum %s differs from live checksum %s", headChecksum, current)})
		}
	} else if !errors.Is(err, sql.ErrNoRows) {
		return nil, err
	}
	sort.Slice(findings, func(i, j int) bool {
		if findings[i].Code != findings[j].Code {
			return findings[i].Code < findings[j].Code
		}
		if findings[i].Sequence != findings[j].Sequence {
			return findings[i].Sequence < findings[j].Sequence
		}
		return findings[i].Detail < findings[j].Detail
	})
	return findings, nil
}

func canonicalizeJSON(value []byte) ([]byte, error) {
	var decoded any
	if err := json.Unmarshal(value, &decoded); err != nil {
		return nil, err
	}
	return json.Marshal(decoded)
}

func decodeStrictJSON(value []byte, target any) error {
	decoder := json.NewDecoder(bytes.NewReader(value))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(target); err != nil {
		return err
	}
	if decoder.More() {
		return errors.New("multiple JSON values")
	}
	return nil
}

func equalOptionalString(left, right *string) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return *left == *right
}
