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
	"io"
	"sort"
	"strings"
	"time"

	"github.com/lib/pq"
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
	Review            int    `json:"review"`
	Work              int    `json:"work"`
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
	Projects        []Project            `json:"projects"`
	Decisions       []Decision           `json:"decisions"`
	Deliverables    []Deliverable        `json:"deliverables"`
	Forecasts       []Forecast           `json:"forecasts"`
	ForecastHeads   []ForecastHead       `json:"forecast_heads"`
	Targets         []Target             `json:"targets"`
	TargetHeads     []TargetHead         `json:"target_heads"`
	Deadlines       []Deadline           `json:"deadlines"`
	DeadlineHeads   []DeadlineHead       `json:"deadline_heads"`
	Evidence        []Evidence           `json:"evidence"`
	Gates           []Gate               `json:"gates"`
	Verdicts        []Verdict            `json:"verdicts"`
	Findings        []Finding            `json:"findings"`
	FindingActions  []FindingAction      `json:"finding_actions"`
	Submissions     []Submission         `json:"submissions"`
	SubmissionHeads []SubmissionHead     `json:"submission_heads"`
	WorkItems       []WorkItem           `json:"work_items"`
	Dependencies    []WorkItemDependency `json:"dependencies"`
	Activity        []eventProjection    `json:"activity"`
}

func (snapshot projectionSnapshot) planningCount() int {
	return len(snapshot.Deliverables) + len(snapshot.Forecasts) + len(snapshot.ForecastHeads) +
		len(snapshot.Targets) + len(snapshot.TargetHeads) + len(snapshot.Deadlines) + len(snapshot.DeadlineHeads)
}

func (snapshot projectionSnapshot) reviewCount() int {
	return len(snapshot.Evidence) + len(snapshot.Gates) + len(snapshot.Verdicts) + len(snapshot.Findings) +
		len(snapshot.FindingActions) + len(snapshot.Submissions) + len(snapshot.SubmissionHeads)
}

func (snapshot projectionSnapshot) workCount() int {
	return len(snapshot.WorkItems) + len(snapshot.Dependencies)
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
	if err := writeReplayReview(ctx, tx, runID, rebuilt); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	if err := writeReplayWork(ctx, tx, runID, rebuilt); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	finished := time.Now().UTC()
	if _, err := tx.ExecContext(ctx, `UPDATE projection_replay_runs SET status='succeeded',finished_at=$2,last_sequence=$3,
		projects_count=$4,decisions_count=$5,activity_count=$6,planning_count=$7,review_count=$8,work_count=$9,live_checksum=$10,rebuilt_checksum=$11 WHERE id=$1`,
		runID, finished, lastSequence, len(rebuilt.Projects), len(rebuilt.Decisions), len(rebuilt.Activity), rebuilt.planningCount(), rebuilt.reviewCount(), rebuilt.workCount(), liveChecksum, rebuiltChecksum); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO projection_heads
		(name,run_id,last_sequence,checksum,projects_count,decisions_count,activity_count,planning_count,review_count,work_count,updated_at)
		VALUES ('m1-canonical',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
		ON CONFLICT (name) DO UPDATE SET run_id=EXCLUDED.run_id,last_sequence=EXCLUDED.last_sequence,
		checksum=EXCLUDED.checksum,projects_count=EXCLUDED.projects_count,decisions_count=EXCLUDED.decisions_count,
		activity_count=EXCLUDED.activity_count,planning_count=EXCLUDED.planning_count,review_count=EXCLUDED.review_count,
		work_count=EXCLUDED.work_count,updated_at=EXCLUDED.updated_at`,
		runID, lastSequence, rebuiltChecksum, len(rebuilt.Projects), len(rebuilt.Decisions), len(rebuilt.Activity), rebuilt.planningCount(), rebuilt.reviewCount(), rebuilt.workCount(), finished); err != nil {
		_ = tx.Rollback()
		return ReplayReport{}, err
	}
	if err := tx.Commit(); err != nil {
		return ReplayReport{}, fmt.Errorf("commit replay generation: %w", err)
	}
	return ReplayReport{RunID: runID, LastSequence: lastSequence, Projects: len(rebuilt.Projects),
		Decisions: len(rebuilt.Decisions), Activity: len(rebuilt.Activity), Planning: rebuilt.planningCount(), Review: rebuilt.reviewCount(), Work: rebuilt.workCount(), LiveChecksum: liveChecksum,
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

func writeReplayReview(ctx context.Context, tx *sql.Tx, runID string, snapshot projectionSnapshot) error {
	type row struct {
		kind, id, organizationID, projectID string
		value                               any
	}
	rows := make([]row, 0, snapshot.reviewCount())
	for _, item := range snapshot.Evidence {
		rows = append(rows, row{"evidence", item.ID, item.OrganizationID, item.ProjectID, item})
	}
	for _, item := range snapshot.Gates {
		rows = append(rows, row{"gate", item.ID, item.OrganizationID, item.ProjectID, item})
	}
	for _, item := range snapshot.Verdicts {
		rows = append(rows, row{"verdict", item.ID, item.OrganizationID, item.ProjectID, item})
	}
	for _, item := range snapshot.Findings {
		rows = append(rows, row{"finding", item.ID, item.OrganizationID, item.ProjectID, item})
	}
	for _, item := range snapshot.FindingActions {
		rows = append(rows, row{"finding-action", item.ID, item.OrganizationID, item.ProjectID, item})
	}
	for _, item := range snapshot.Submissions {
		rows = append(rows, row{"submission", item.ID, item.OrganizationID, item.ProjectID, item})
	}
	projects := make(map[string]Project, len(snapshot.Projects))
	for _, project := range snapshot.Projects {
		projects[project.ID] = project
	}
	for _, item := range snapshot.SubmissionHeads {
		for _, submission := range snapshot.Submissions {
			if submission.ID == item.SubmissionID {
				rows = append(rows, row{"submission-head", item.DeliverableID, submission.OrganizationID, submission.ProjectID, item})
				break
			}
		}
	}
	for _, item := range rows {
		if projects[item.projectID].ID == "" {
			return fmt.Errorf("review projection %s:%s references unknown project %s", item.kind, item.id, item.projectID)
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_review_projections
			(run_id,kind,projection_id,organization_id,project_id,projection) VALUES ($1,$2,$3,$4,$5,$6)`,
			runID, item.kind, item.id, item.organizationID, item.projectID, canonicalJSON(item.value)); err != nil {
			return fmt.Errorf("write replay review %s:%s: %w", item.kind, item.id, err)
		}
	}
	return nil
}

func writeReplayWork(ctx context.Context, tx *sql.Tx, runID string, snapshot projectionSnapshot) error {
	projects := make(map[string]Project, len(snapshot.Projects))
	workProjects := make(map[string]string, len(snapshot.WorkItems))
	for _, project := range snapshot.Projects {
		projects[project.ID] = project
	}
	for _, item := range snapshot.WorkItems {
		project := projects[item.ProjectID]
		if project.ID == "" || project.OrganizationID != item.OrganizationID {
			return fmt.Errorf("work projection %s references unknown project %s", item.ID, item.ProjectID)
		}
		workProjects[item.ID] = item.ProjectID
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_work_projections
			(run_id,kind,projection_id,organization_id,project_id,projection) VALUES ($1,'work-item',$2,$3,$4,$5)`,
			runID, item.ID, item.OrganizationID, item.ProjectID, canonicalJSON(item)); err != nil {
			return fmt.Errorf("write replay work item %s: %w", item.ID, err)
		}
	}
	for _, dependency := range snapshot.Dependencies {
		projectID := workProjects[dependency.SourceWorkItemID]
		if projectID == "" || workProjects[dependency.TargetWorkItemID] == "" {
			return fmt.Errorf("dependency projection %s references an unknown work item", dependency.ID)
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO replay_work_projections
			(run_id,kind,projection_id,organization_id,project_id,projection) VALUES ($1,'dependency',$2,$3,$4,$5)`,
			runID, dependency.ID, dependency.OrganizationID, projectID, canonicalJSON(dependency)); err != nil {
			return fmt.Errorf("write replay dependency %s: %w", dependency.ID, err)
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

func replayEvidenceAttributions(ids []string, projectID string, records map[string]Evidence) ([]Evidence, bool) {
	if len(ids) == 0 {
		return nil, false
	}
	selected := make([]Evidence, 0, len(ids))
	seen := make(map[string]bool, len(ids))
	for _, id := range ids {
		record := records[id]
		if record.ID == "" || record.ProjectID != projectID || seen[id] {
			return nil, false
		}
		seen[id] = true
		selected = append(selected, record)
	}
	return selected, true
}

func replayEvidenceSelection(ids []string, projectID string, records map[string]Evidence) ([]Evidence, bool) {
	selected, valid := replayEvidenceAttributions(ids, projectID, records)
	if !valid {
		return nil, false
	}
	for _, item := range selected {
		for _, candidate := range records {
			if candidate.SupersedesID != nil && *candidate.SupersedesID == item.ID {
				return nil, false
			}
		}
	}
	return selected, true
}

func replayDeliverableGates(deliverableID string, records map[string]Gate) []Gate {
	result := make([]Gate, 0)
	for _, gate := range records {
		if gate.DeliverableID == deliverableID {
			result = append(result, gate)
		}
	}
	return result
}

func replayOpenFindings(deliverableID string, blockingOnly bool, records map[string]Finding) int {
	count := 0
	for _, finding := range records {
		if finding.DeliverableID == deliverableID && finding.State == "open" && (!blockingOnly || finding.Blocking) {
			count++
		}
	}
	return count
}

func replayDecision(id string, decisions []Decision) (Decision, bool) {
	for _, decision := range decisions {
		if decision.ID == id {
			return decision, true
		}
	}
	return Decision{}, false
}

func replayEvidenceTargetExists(support EvidenceSupport, projectID string, deliverables map[string]Deliverable, gates map[string]Gate,
	findings map[string]Finding, decisions []Decision) bool {
	switch support.TargetType {
	case "deliverable":
		return deliverables[support.TargetID].ProjectID == projectID
	case "gate":
		return gates[support.TargetID].ProjectID == projectID
	case "finding":
		return findings[support.TargetID].ProjectID == projectID
	case "decision":
		decision, exists := replayDecision(support.TargetID, decisions)
		return exists && decision.ProjectID == projectID
	default:
		return false
	}
}

func sameWorkIdentity(left, right WorkItemRecord) bool {
	return left.ID == right.ID && left.OrganizationID == right.OrganizationID && left.ProjectID == right.ProjectID &&
		left.CreatedBy == right.CreatedBy && left.CreatedAt == right.CreatedAt
}

func sameWorkContent(left, right WorkItemRecord) bool {
	return left.Title == right.Title && left.Description == right.Description && left.State == right.State &&
		left.Priority == right.Priority && equalOptionalString(left.DeliverableID, right.DeliverableID) &&
		equalOptionalString(left.AssigneeID, right.AssigneeID)
}

var workItemRecordMembers = []string{
	"id", "organization_id", "project_id", "deliverable_id", "title", "description", "state",
	"priority", "assignee_id", "version", "created_by", "created_at", "updated_at",
}

func requireJSONMembers(value []byte, expected ...string) (map[string]json.RawMessage, error) {
	var members map[string]json.RawMessage
	if err := json.Unmarshal(value, &members); err != nil {
		return nil, err
	}
	if len(members) != len(expected) {
		return nil, fmt.Errorf("payload has %d members, expected %d", len(members), len(expected))
	}
	for _, name := range expected {
		if _, ok := members[name]; !ok {
			return nil, fmt.Errorf("payload member %q is required", name)
		}
	}
	return members, nil
}

func canonicalTimestamp(value string) bool {
	parsed, err := time.Parse(timeFormat, value)
	return err == nil && parsed.UTC().Format(timeFormat) == value
}

func canonicalWorkState(value string) bool {
	return value == "open" || value == "in_progress" || value == "in_review" || value == "done" || value == "cancelled"
}

func canonicalOptionalUUID(value *string) bool {
	return value == nil || uuidPattern.MatchString(*value)
}

func canonicalWorkRecord(item WorkItemRecord) bool {
	if !uuidPattern.MatchString(item.ID) || !uuidPattern.MatchString(item.OrganizationID) ||
		!uuidPattern.MatchString(item.ProjectID) || !uuidPattern.MatchString(item.CreatedBy) ||
		!canonicalOptionalUUID(item.DeliverableID) || !canonicalOptionalUUID(item.AssigneeID) ||
		!validText(item.Title, 1, 200) || strings.TrimSpace(item.Title) != item.Title ||
		!validText(item.Description, 1, 4000) || strings.TrimSpace(item.Description) != item.Description ||
		!canonicalWorkState(item.State) || !validWorkPriority(item.Priority) || item.Version < 1 ||
		!canonicalTimestamp(item.CreatedAt) || !canonicalTimestamp(item.UpdatedAt) {
		return false
	}
	created, _ := time.Parse(timeFormat, item.CreatedAt)
	updated, _ := time.Parse(timeFormat, item.UpdatedAt)
	return !updated.Before(created)
}

func canonicalEvidenceIDs(values []string) bool {
	if values == nil || len(values) > 64 {
		return false
	}
	normalized, ok := normalizeUUIDList(values, 0, 64)
	if !ok {
		return false
	}
	for index := range values {
		if values[index] != normalized[index] {
			return false
		}
	}
	return true
}

func decodeCanonicalWorkEvent(value []byte, eventType string) (WorkItemEvent, error) {
	members, err := requireJSONMembers(value, "work_item", "command", "reason", "evidence_ids", "finding_id", "batch")
	if err != nil {
		return WorkItemEvent{}, err
	}
	if _, err := requireJSONMembers(members["work_item"], workItemRecordMembers...); err != nil {
		return WorkItemEvent{}, fmt.Errorf("work_item: %w", err)
	}
	var event WorkItemEvent
	if err := decodeStrictJSON(value, &event); err != nil {
		return WorkItemEvent{}, err
	}
	if !canonicalWorkRecord(event.WorkItem) || !canonicalEvidenceIDs(event.EvidenceIDs) ||
		strings.TrimSpace(event.Reason) != event.Reason || !canonicalOptionalUUID(event.FindingID) {
		return WorkItemEvent{}, errors.New("work item payload members are not canonical")
	}
	expectedCommand := map[string]string{
		"work_item.created": "create", "work_item.updated": "update", "work_item.assigned": "assign",
		"work_item.started": "start", "work_item.review_requested": "request_review", "work_item.bounced": "bounce",
		"work_item.accepted": "accept", "work_item.cancelled": "cancel",
	}[eventType]
	if expectedCommand == "" || event.Command != expectedCommand {
		return WorkItemEvent{}, errors.New("work item command does not match its event type")
	}
	switch event.Command {
	case "create", "update", "assign":
		if event.Reason != "" || len(event.EvidenceIDs) != 0 || event.FindingID != nil || event.Batch {
			return WorkItemEvent{}, errors.New("work item mutation metadata is not canonical")
		}
	case "request_review", "accept":
		if !validText(event.Reason, 1, 4000) || len(event.EvidenceIDs) == 0 || event.FindingID != nil {
			return WorkItemEvent{}, errors.New("review transition metadata is not canonical")
		}
	case "bounce":
		if !validText(event.Reason, 1, 4000) || len(event.EvidenceIDs) != 0 || event.FindingID == nil {
			return WorkItemEvent{}, errors.New("bounce transition metadata is not canonical")
		}
	case "start", "cancel":
		if !validText(event.Reason, 1, 4000) || len(event.EvidenceIDs) != 0 || event.FindingID != nil {
			return WorkItemEvent{}, errors.New("lifecycle transition metadata is not canonical")
		}
	}
	return event, nil
}

func canonicalDependencyRecord(dependency WorkItemDependency) bool {
	return uuidPattern.MatchString(dependency.ID) && uuidPattern.MatchString(dependency.OrganizationID) &&
		uuidPattern.MatchString(dependency.SourceWorkItemID) && uuidPattern.MatchString(dependency.TargetWorkItemID) &&
		(dependency.Kind == "blocks" || dependency.Kind == "relates" || dependency.Kind == "caused-by") &&
		dependency.Version == 1 && uuidPattern.MatchString(dependency.CreatedBy) && canonicalTimestamp(dependency.CreatedAt)
}

func decodeCanonicalDependencyEvent(value []byte, eventType string) (WorkDependencyEvent, error) {
	members, err := requireJSONMembers(value, "dependency", "source", "removed")
	if err != nil {
		return WorkDependencyEvent{}, err
	}
	if _, err := requireJSONMembers(members["dependency"], "id", "organization_id", "source_work_item_id",
		"target_work_item_id", "kind", "version", "created_by", "created_at"); err != nil {
		return WorkDependencyEvent{}, fmt.Errorf("dependency: %w", err)
	}
	if _, err := requireJSONMembers(members["source"], workItemRecordMembers...); err != nil {
		return WorkDependencyEvent{}, fmt.Errorf("source: %w", err)
	}
	var event WorkDependencyEvent
	if err := decodeStrictJSON(value, &event); err != nil {
		return WorkDependencyEvent{}, err
	}
	if !canonicalDependencyRecord(event.Dependency) || !canonicalWorkRecord(event.Source) ||
		(eventType == "dependency.added" && event.Removed) || (eventType == "dependency.removed" && !event.Removed) {
		return WorkDependencyEvent{}, errors.New("dependency payload members are not canonical")
	}
	return event, nil
}

func replayBlockingPath(dependencies map[string]WorkItemDependency, organizationID, fromID, toID string) bool {
	seen := map[string]bool{fromID: true}
	queue := []string{fromID}
	for len(queue) > 0 {
		current := queue[0]
		queue = queue[1:]
		for _, dependency := range dependencies {
			if dependency.OrganizationID != organizationID || dependency.Kind != "blocks" || dependency.SourceWorkItemID != current {
				continue
			}
			if dependency.TargetWorkItemID == toID {
				return true
			}
			if !seen[dependency.TargetWorkItemID] {
				seen[dependency.TargetWorkItemID] = true
				queue = append(queue, dependency.TargetWorkItemID)
			}
		}
	}
	return false
}

func replayWorkItemViewWithFinalStates(item WorkItemRecord, records map[string]WorkItemRecord,
	dependencies map[string]WorkItemDependency, gates map[string]Gate, finalStates map[string]string) WorkItem {
	reasons := make([]string, 0, 2)
	for _, dependency := range dependencies {
		if dependency.Kind != "blocks" || dependency.SourceWorkItemID != item.ID {
			continue
		}
		target, exists := records[dependency.TargetWorkItemID]
		state := target.State
		if replacement, ok := finalStates[target.ID]; ok {
			state = replacement
		}
		if !exists || (state != "done" && state != "cancelled") {
			reasons = append(reasons, "dependency")
			break
		}
	}
	if item.DeliverableID != nil {
		for _, gate := range gates {
			if gate.DeliverableID == *item.DeliverableID && gate.Hard && gate.State != "passed" {
				reasons = append(reasons, "hard_gate")
				break
			}
		}
	}
	return WorkItem{WorkItemRecord: item, Blocked: len(reasons) > 0, BlockingReasons: reasons}
}

func replayWorkItemView(item WorkItemRecord, records map[string]WorkItemRecord,
	dependencies map[string]WorkItemDependency, gates map[string]Gate) WorkItem {
	return replayWorkItemViewWithFinalStates(item, records, dependencies, gates, nil)
}

func sortWorkSnapshot(snapshot *projectionSnapshot) {
	sort.Slice(snapshot.WorkItems, func(i, j int) bool {
		if snapshot.WorkItems[i].ProjectID != snapshot.WorkItems[j].ProjectID {
			return snapshot.WorkItems[i].ProjectID < snapshot.WorkItems[j].ProjectID
		}
		return snapshot.WorkItems[i].ID < snapshot.WorkItems[j].ID
	})
	sort.Slice(snapshot.Dependencies, func(i, j int) bool {
		if snapshot.Dependencies[i].SourceWorkItemID != snapshot.Dependencies[j].SourceWorkItemID {
			return snapshot.Dependencies[i].SourceWorkItemID < snapshot.Dependencies[j].SourceWorkItemID
		}
		if snapshot.Dependencies[i].TargetWorkItemID != snapshot.Dependencies[j].TargetWorkItemID {
			return snapshot.Dependencies[i].TargetWorkItemID < snapshot.Dependencies[j].TargetWorkItemID
		}
		return snapshot.Dependencies[i].ID < snapshot.Dependencies[j].ID
	})
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
	evidence := make(map[string]Evidence)
	gates := make(map[string]Gate)
	verdicts := make(map[string]Verdict)
	verdictHeads := make(map[string]string)
	findings := make(map[string]Finding)
	findingActions := make(map[string]FindingAction)
	submissions := make(map[string]Submission)
	submissionHeads := make(map[string]SubmissionHead)
	workItems := make(map[string]WorkItemRecord)
	dependencies := make(map[string]WorkItemDependency)
	activity := make([]eventProjection, 0, len(events))
	versions := make(map[string]int64)
	// A batch is one atomic command even though the immutable ledger retains one
	// versioned event per work aggregate. Precompute its declared endpoint
	// states so blocking validation is independent of input/event order. Every
	// event is still validated sequentially below; a malformed later member
	// therefore fails the complete replay and cannot advance the active head.
	type workTransitionCommand struct {
		Count, BatchCount                    int
		OrganizationID, ProjectID, ActorKind string
		ActorID, RequestID, OccurredAt       string
		PrincipalID                          *string
		Consistent, Initialized              bool
	}
	batchFinalStates := make(map[string]map[string]string)
	transitionCommands := make(map[string]workTransitionCommand)
	for _, event := range events {
		switch event.Projection.EventType {
		case "work_item.started", "work_item.review_requested", "work_item.bounced", "work_item.accepted", "work_item.cancelled":
			workEvent, err := decodeCanonicalWorkEvent(event.Payload, event.Projection.EventType)
			if err != nil {
				continue
			}
			item := event.Projection
			command := transitionCommands[item.CommandID]
			if !command.Initialized {
				command = workTransitionCommand{OrganizationID: item.OrganizationID, ProjectID: workEvent.WorkItem.ProjectID,
					ActorKind: item.ActorKind, ActorID: item.ActorID, PrincipalID: item.PrincipalID, RequestID: item.RequestID,
					OccurredAt: item.OccurredAt, Consistent: true, Initialized: true}
			} else if command.OrganizationID != item.OrganizationID || command.ProjectID != workEvent.WorkItem.ProjectID ||
				command.ActorKind != item.ActorKind || command.ActorID != item.ActorID ||
				!equalOptionalString(command.PrincipalID, item.PrincipalID) || command.RequestID != item.RequestID ||
				command.OccurredAt != item.OccurredAt {
				command.Consistent = false
			}
			command.Count++
			if workEvent.Batch {
				command.BatchCount++
				states := batchFinalStates[event.Projection.CommandID]
				if states == nil {
					states = make(map[string]string)
					batchFinalStates[event.Projection.CommandID] = states
				}
				states[workEvent.WorkItem.ID] = workEvent.WorkItem.State
			}
			transitionCommands[item.CommandID] = command
		}
	}
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
		case "evidence.created", "evidence.superseded":
			var record Evidence
			if err := decodeStrictJSON(event.Payload, &record); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[record.ProjectID]
			if !exists || record.ProjectID != item.AggregateID || record.OrganizationID != item.OrganizationID ||
				record.ProducedBy != item.ActorID || record.ProducerKind != item.ActorKind ||
				!equalOptionalString(record.PrincipalID, item.PrincipalID) || len(record.IntegrityDigest) != 64 || len(record.Supports) == 0 {
				return failure("invalid_event_payload", "evidence identity, attribution, digest, or support contract is invalid")
			}
			for _, support := range record.Supports {
				if !replayEvidenceTargetExists(support, record.ProjectID, deliverables, gates, findings, decisions) {
					return failure("invalid_event_payload", "evidence support references an unknown project target")
				}
			}
			if item.EventType == "evidence.created" {
				if record.SupersedesID != nil || evidence[record.ID].ID != "" {
					return failure("invalid_event_payload", "initial evidence conflicts with existing history")
				}
			} else if record.SupersedesID == nil || evidence[*record.SupersedesID].ID == "" || evidence[record.ID].ID != "" ||
				evidence[*record.SupersedesID].ProjectID != record.ProjectID || evidence[*record.SupersedesID].OrganizationID != record.OrganizationID {
				return failure("invalid_event_payload", "evidence supersession does not extend an existing project chain")
			}
			for _, prior := range evidence {
				if record.SupersedesID != nil && prior.SupersedesID != nil && *prior.SupersedesID == *record.SupersedesID {
					return failure("invalid_event_payload", "evidence supersession chain branches")
				}
			}
			evidence[record.ID] = record
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "gate.created":
			var gate Gate
			if err := decodeStrictJSON(event.Payload, &gate); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[gate.ProjectID]
			deliverable := deliverables[gate.DeliverableID]
			if !exists || deliverable.ProjectID != gate.ProjectID || gate.ProjectID != item.AggregateID ||
				gate.OrganizationID != item.OrganizationID || gate.CreatedBy != item.ActorID || gate.State != "pending" ||
				gate.Version != 1 || len(gate.RequiredEvidence) == 0 || gates[gate.ID].ID != "" {
				return failure("invalid_event_payload", "gate identity or evidence contract is invalid")
			}
			gates[gate.ID] = gate
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "gate.verdict_recorded":
			var result GateVerdictEvent
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := gates[result.Gate.ID]
			project, exists := projects[result.Gate.ProjectID]
			deliverable := deliverables[result.Gate.DeliverableID]
			submission := submissions[submissionHeads[result.Gate.DeliverableID].SubmissionID]
			selectedEvidence, evidenceValid := replayEvidenceSelection(result.Verdict.EvidenceIDs, result.Gate.ProjectID, evidence)
			submittedEvidence, submittedEvidenceValid := replayEvidenceAttributions(submission.EvidenceIDs, result.Gate.ProjectID, evidence)
			validTransition := prior.ID != "" && result.Gate.Version == prior.Version+1 &&
				((prior.State == "pending" && (result.Gate.State == "passed" || result.Gate.State == "failed")) ||
					(prior.State == "failed" && result.Gate.State == "passed"))
			validResult := (result.Verdict.Result == "pass" && result.Gate.State == "passed") ||
				(result.Verdict.Result == "fail" && result.Gate.State == "failed")
			validHistory := (verdictHeads[result.Gate.ID] == "" && result.Verdict.SupersedesID == nil) ||
				(verdictHeads[result.Gate.ID] != "" && result.Verdict.SupersedesID != nil && *result.Verdict.SupersedesID == verdictHeads[result.Gate.ID])
			if !exists || !validTransition || result.Gate.ProjectID != item.AggregateID || result.Gate.OrganizationID != item.OrganizationID ||
				result.Verdict.GateID != result.Gate.ID || result.Verdict.ProjectID != result.Gate.ProjectID ||
				result.Verdict.ReviewerID != item.ActorID || result.Verdict.ReviewerKind != item.ActorKind ||
				!equalOptionalString(result.Verdict.PrincipalID, item.PrincipalID) || verdicts[result.Verdict.ID].ID != "" ||
				deliverable.State != "submitted" || submission.ID == "" || !evidenceValid || !submittedEvidenceValid || !evidenceMeetsGate(selectedEvidence, prior) ||
				!validResult || !validHistory || (prior.IndependenceRequired && !independentReviewer(
				Actor{ID: item.ActorID, Kind: item.ActorKind, PrincipalID: item.PrincipalID}, deliverable, submission, submittedEvidence, selectedEvidence)) {
				return failure("invalid_event_payload", "gate verdict transition or attribution is invalid")
			}
			gates[result.Gate.ID] = result.Gate
			verdicts[result.Verdict.ID] = result.Verdict
			verdictHeads[result.Gate.ID] = result.Verdict.ID
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "finding.created":
			var finding Finding
			if err := decodeStrictJSON(event.Payload, &finding); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			project, exists := projects[finding.ProjectID]
			verdict := verdicts[finding.VerdictID]
			if !exists || finding.ProjectID != item.AggregateID || finding.OrganizationID != item.OrganizationID ||
				finding.CreatedBy != item.ActorID || finding.State != "open" || finding.Version != 1 ||
				verdict.Result != "fail" || verdict.GateID != finding.GateID || verdict.DeliverableID != finding.DeliverableID ||
				findings[finding.ID].ID != "" {
				return failure("invalid_event_payload", "finding identity, origin verdict, or attribution is invalid")
			}
			findings[finding.ID] = finding
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "finding.resolved", "finding.withdrawn":
			var result FindingActionResult
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := findings[result.Finding.ID]
			project, exists := projects[result.Finding.ProjectID]
			selectedEvidence, evidenceValid := replayEvidenceSelection(result.Action.EvidenceIDs, result.Finding.ProjectID, evidence)
			evidenceLinked := evidenceValid
			for _, record := range selectedEvidence {
				evidenceLinked = evidenceLinked && (evidenceSupports(record, "finding", result.Finding.ID) ||
					evidenceSupports(record, "deliverable", result.Finding.DeliverableID))
			}
			expectedState, expectedAction := "resolved", "resolve"
			if item.EventType == "finding.withdrawn" {
				expectedState, expectedAction = "withdrawn", "withdraw"
			}
			if !exists || prior.State != "open" || result.Finding.State != expectedState || result.Finding.Version != prior.Version+1 ||
				result.Action.Action != expectedAction || result.Action.FindingID != result.Finding.ID ||
				result.Action.ProjectID != result.Finding.ProjectID || result.Action.DeliverableID != result.Finding.DeliverableID ||
				len(result.Action.EvidenceIDs) == 0 ||
				result.Action.ActorID != item.ActorID || result.Action.ActorKind != item.ActorKind ||
				!equalOptionalString(result.Action.PrincipalID, item.PrincipalID) || findingActions[result.Action.ID].ID != "" ||
				!evidenceLinked {
				return failure("invalid_event_payload", "finding disposition history is invalid")
			}
			findings[result.Finding.ID] = result.Finding
			findingActions[result.Action.ID] = result.Action
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "deliverable.submitted", "deliverable.resubmitted":
			var result SubmissionResult
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := deliverables[result.Deliverable.ID]
			project, exists := projects[result.Deliverable.ProjectID]
			selectedEvidence, evidenceValid := replayEvidenceSelection(result.Submission.EvidenceIDs, result.Deliverable.ProjectID, evidence)
			gateContractValid := evidenceValid && evidenceMeetsDeliverableContract(
				selectedEvidence, result.Deliverable.ID, replayDeliverableGates(result.Deliverable.ID, gates))
			expectedPrior, expectedKind := "ready", "submit"
			resolutionValid := true
			if item.EventType == "deliverable.resubmitted" {
				expectedPrior, expectedKind = "bounced", "resubmit"
				resolutionValid = replayOpenFindings(result.Deliverable.ID, false, findings) == 0
				linkedResolution := false
				for _, action := range findingActions {
					if action.DeliverableID != result.Deliverable.ID || action.Action != "resolve" {
						continue
					}
					for _, selectedID := range result.Submission.EvidenceIDs {
						for _, actionID := range action.EvidenceIDs {
							linkedResolution = linkedResolution || selectedID == actionID
						}
					}
				}
				resolutionValid = resolutionValid && linkedResolution
			}
			if !exists || prior.State != expectedPrior || result.Deliverable.State != "submitted" ||
				result.Deliverable.Version != prior.Version+1 || result.Submission.Kind != expectedKind ||
				result.Submission.DeliverableID != result.Deliverable.ID || result.Submission.ProjectID != result.Deliverable.ProjectID ||
				result.Submission.OrganizationID != item.OrganizationID || result.Submission.SubmittedBy != item.ActorID ||
				result.Submission.SubmitterKind != item.ActorKind || !equalOptionalString(result.Submission.PrincipalID, item.PrincipalID) ||
				len(result.Submission.EvidenceIDs) == 0 || submissions[result.Submission.ID].ID != "" ||
				!gateContractValid || !resolutionValid {
				return failure("invalid_event_payload", "deliverable submission transition is invalid")
			}
			deliverables[result.Deliverable.ID] = result.Deliverable
			submissions[result.Submission.ID] = result.Submission
			submissionHeads[result.Deliverable.ID] = SubmissionHead{DeliverableID: result.Deliverable.ID, SubmissionID: result.Submission.ID}
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "deliverable.bounced", "deliverable.accepted":
			var result DeliverableReviewEvent
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := deliverables[result.Deliverable.ID]
			project, exists := projects[result.Deliverable.ProjectID]
			expectedState := "bounced"
			validDetail := result.FindingID != nil && findings[*result.FindingID].DeliverableID == result.Deliverable.ID &&
				findings[*result.FindingID].State == "open" && strings.TrimSpace(findings[*result.FindingID].Detail) != ""
			if item.EventType == "deliverable.accepted" {
				expectedState, validDetail = "accepted", result.FindingID == nil
				gateCount, incompleteHard := 0, 0
				for _, gate := range gates {
					if gate.DeliverableID == result.Deliverable.ID {
						gateCount++
						if gate.Hard && gate.State != "passed" {
							incompleteHard++
						}
					}
				}
				validDetail = validDetail && gateCount > 0 && incompleteHard == 0 && replayOpenFindings(result.Deliverable.ID, false, findings) == 0
			}
			if !exists || prior.State != "submitted" || result.Deliverable.State != expectedState ||
				result.Deliverable.Version != prior.Version+1 || !validDetail || strings.TrimSpace(result.Rationale) == "" {
				return failure("invalid_event_payload", "deliverable review transition is invalid")
			}
			deliverables[result.Deliverable.ID] = result.Deliverable
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "deliverable.cancelled":
			var result DeliverableCancellation
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := deliverables[result.Deliverable.ID]
			project, exists := projects[result.Deliverable.ProjectID]
			validPrior := prior.State == "draft" || prior.State == "ready"
			if !exists || !validPrior || prior.Required || result.Deliverable.Required ||
				result.Deliverable.ProjectID != item.AggregateID || result.Deliverable.OrganizationID != item.OrganizationID ||
				result.Deliverable.State != "cancelled" || result.Deliverable.Version != prior.Version+1 ||
				strings.TrimSpace(result.Reason) == "" {
				return failure("invalid_event_payload", "optional deliverable cancellation is invalid")
			}
			deliverables[result.Deliverable.ID] = result.Deliverable
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "deliverable.waived":
			var result DeliverableWaiverResult
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := deliverables[result.Deliverable.ID]
			project, exists := projects[result.Deliverable.ProjectID]
			validPrior := prior.State == "ready" || prior.State == "submitted" || prior.State == "bounced"
			decision, decisionExists := replayDecision(result.Decision.ID, decisions)
			selectedEvidence, evidenceValid := replayEvidenceSelection(result.Decision.Evidence, result.Deliverable.ProjectID, evidence)
			waiverContractValid := evidenceValid && evidenceMeetsDeliverableContract(
				selectedEvidence, result.Deliverable.ID, replayDeliverableGates(result.Deliverable.ID, gates))
			for _, gate := range gates {
				if gate.DeliverableID == result.Deliverable.ID && gate.Hard && gate.State != "passed" {
					waiverContractValid = false
				}
			}
			waiverContractValid = waiverContractValid && replayOpenFindings(result.Deliverable.ID, true, findings) == 0
			if !exists || !validPrior || result.Deliverable.State != "waived" || result.Deliverable.Version != prior.Version+1 ||
				result.Deliverable.WaiverDecisionID == nil || *result.Deliverable.WaiverDecisionID != result.Decision.ID ||
				!decisionExists || decision.Kind != "waiver" || decision.ActorKind != "human" || decision.ActorID != item.ActorID ||
				!bytes.Equal(canonicalJSON(decision), canonicalJSON(result.Decision)) || !waiverContractValid {
				return failure("invalid_event_payload", "deliverable waiver decision is invalid")
			}
			deliverables[result.Deliverable.ID] = result.Deliverable
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "gate.waived":
			var result GateWaiverResult
			if err := decodeStrictJSON(event.Payload, &result); err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			prior := gates[result.Gate.ID]
			project, exists := projects[result.Gate.ProjectID]
			decision, decisionExists := replayDecision(result.Decision.ID, decisions)
			selectedEvidence, evidenceValid := replayEvidenceSelection(result.Decision.Evidence, result.Gate.ProjectID, evidence)
			waiverContractValid := evidenceValid && evidenceMeetsGate(selectedEvidence, prior) &&
				replayOpenFindings(result.Gate.DeliverableID, true, findings) == 0
			for _, gate := range gates {
				if gate.DeliverableID == result.Gate.DeliverableID && gate.Hard && gate.State != "passed" {
					waiverContractValid = false
				}
			}
			if !exists || prior.Hard || (prior.State != "pending" && prior.State != "failed") || result.Gate.State != "waived" ||
				result.Gate.Version != prior.Version+1 || result.Gate.WaiverDecisionID == nil || *result.Gate.WaiverDecisionID != result.Decision.ID ||
				!decisionExists || decision.Kind != "waiver" || decision.ActorKind != "human" || decision.ActorID != item.ActorID ||
				!bytes.Equal(canonicalJSON(decision), canonicalJSON(result.Decision)) || !waiverContractValid {
				return failure("invalid_event_payload", "soft gate waiver decision is invalid")
			}
			gates[result.Gate.ID] = result.Gate
			project.Version = item.AggregateVersion
			projects[project.ID] = project
		case "work_item.created", "work_item.updated", "work_item.assigned", "work_item.started",
			"work_item.review_requested", "work_item.bounced", "work_item.accepted", "work_item.cancelled":
			workEvent, err := decodeCanonicalWorkEvent(event.Payload, item.EventType)
			if err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			work := workEvent.WorkItem
			project, projectExists := projects[work.ProjectID]
			if item.AggregateType != "work_item" || !projectExists || work.ID != item.AggregateID ||
				work.OrganizationID != item.OrganizationID || project.OrganizationID != work.OrganizationID ||
				work.Version != item.AggregateVersion || work.UpdatedAt != item.OccurredAt {
				return failure("invalid_event_payload", "work item identity, project, version, or required event members are invalid")
			}
			prior, exists := workItems[work.ID]
			if item.EventType == "work_item.created" {
				if exists || workEvent.Command != "create" || work.State != "open" || work.Version != 1 ||
					work.CreatedBy != item.ActorID || !validText(work.Title, 1, 200) || !validText(work.Description, 1, 4000) ||
					!validWorkPriority(work.Priority) || work.AssigneeID != nil || work.CreatedAt != item.OccurredAt {
					return failure("invalid_event_payload", "work item creation payload is invalid")
				}
				if work.DeliverableID != nil && deliverables[*work.DeliverableID].ProjectID != work.ProjectID {
					return failure("invalid_event_payload", "work item deliverable is outside its project")
				}
				workItems[work.ID] = work
				break
			}
			if !exists || !sameWorkIdentity(prior, work) || work.Version != prior.Version+1 ||
				work.CreatedAt != prior.CreatedAt || work.UpdatedAt < prior.UpdatedAt {
				return failure("invalid_event_payload", "work item update rewrites identity or skips a version")
			}
			expectedCommand := map[string]string{
				"work_item.updated": "update", "work_item.assigned": "assign", "work_item.started": "start",
				"work_item.review_requested": "request_review", "work_item.bounced": "bounce",
				"work_item.accepted": "accept", "work_item.cancelled": "cancel",
			}[item.EventType]
			if workEvent.Command != expectedCommand {
				return failure("invalid_event_payload", "work item event command does not match its type")
			}
			if item.EventType != "work_item.created" && item.EventType != "work_item.updated" && item.EventType != "work_item.assigned" {
				command := transitionCommands[item.CommandID]
				if !command.Initialized || !command.Consistent || (command.Count > 1 && command.BatchCount != command.Count) ||
					(workEvent.Batch && command.BatchCount != command.Count) {
					return failure("invalid_event_payload", "work item batch marker is inconsistent with its command group")
				}
			}
			switch item.EventType {
			case "work_item.updated":
				if work.State != prior.State || !equalOptionalString(work.AssigneeID, prior.AssigneeID) ||
					(prior.DeliverableID != nil && work.DeliverableID == nil) {
					return failure("invalid_event_payload", "work item update changed lifecycle or assignment")
				}
				if work.DeliverableID != nil && deliverables[*work.DeliverableID].ProjectID != work.ProjectID {
					return failure("invalid_event_payload", "work item update linked a cross-project deliverable")
				}
			case "work_item.assigned":
				if work.State != prior.State || work.AssigneeID == nil || work.Title != prior.Title || work.Description != prior.Description ||
					work.Priority != prior.Priority || !equalOptionalString(work.DeliverableID, prior.DeliverableID) {
					return failure("invalid_event_payload", "assignment changed non-assignment fields")
				}
			default:
				target, _, transitionValid := workTransitionTarget(prior.State, workEvent.Command)
				if !transitionValid || work.State != target || work.Title != prior.Title || work.Description != prior.Description ||
					work.Priority != prior.Priority || !equalOptionalString(work.DeliverableID, prior.DeliverableID) ||
					!equalOptionalString(work.AssigneeID, prior.AssigneeID) || !validText(workEvent.Reason, 1, 4000) {
					return failure("invalid_event_payload", "work item transition does not match the fixed lifecycle")
				}
				if workEvent.Command == "start" && prior.AssigneeID == nil {
					return failure("invalid_event_payload", "work item started without an assignee")
				}
				if workEvent.Command == "request_review" || workEvent.Command == "accept" {
					if len(workEvent.EvidenceIDs) == 0 {
						return failure("invalid_event_payload", "review transition lacks evidence")
					}
					for _, evidenceID := range workEvent.EvidenceIDs {
						if evidence[evidenceID].ProjectID != work.ProjectID {
							return failure("invalid_event_payload", "review transition references unavailable evidence")
						}
					}
				}
				if workEvent.Command == "bounce" {
					if workEvent.FindingID == nil || prior.DeliverableID == nil {
						return failure("invalid_event_payload", "bounce lacks an actionable finding")
					}
					finding := findings[*workEvent.FindingID]
					if finding.ProjectID != work.ProjectID || finding.DeliverableID != *prior.DeliverableID || finding.State != "open" || !finding.Blocking {
						return failure("invalid_event_payload", "bounce finding is not open and actionable")
					}
				}
				if workEvent.Command == "start" || workEvent.Command == "request_review" || workEvent.Command == "accept" {
					finalStates := map[string]string(nil)
					if workEvent.Batch {
						finalStates = batchFinalStates[item.CommandID]
					}
					if replayWorkItemViewWithFinalStates(prior, workItems, dependencies, gates, finalStates).Blocked {
						return failure("invalid_event_payload", "blocked work item advanced")
					}
				}
			}
			workItems[work.ID] = work
		case "dependency.added", "dependency.removed":
			dependencyEvent, err := decodeCanonicalDependencyEvent(event.Payload, item.EventType)
			if err != nil {
				return failure("invalid_event_payload", err.Error())
			}
			dependency, source := dependencyEvent.Dependency, dependencyEvent.Source
			priorSource, sourceExists := workItems[source.ID]
			target, targetExists := workItems[dependency.TargetWorkItemID]
			if item.AggregateType != "work_item" || !sourceExists || !targetExists || source.ID != item.AggregateID ||
				dependency.SourceWorkItemID != source.ID || source.OrganizationID != item.OrganizationID ||
				dependency.OrganizationID != item.OrganizationID || target.OrganizationID != item.OrganizationID ||
				source.UpdatedAt != item.OccurredAt ||
				source.Version != item.AggregateVersion || source.Version != priorSource.Version+1 ||
				!sameWorkIdentity(priorSource, source) || !sameWorkContent(priorSource, source) ||
				dependency.Version != 1 || (dependency.Kind != "blocks" && dependency.Kind != "relates" && dependency.Kind != "caused-by") {
				return failure("invalid_event_payload", "dependency event identity, endpoint, kind, or source version is invalid")
			}
			if item.EventType == "dependency.added" {
				if dependencyEvent.Removed || dependency.SourceWorkItemID == dependency.TargetWorkItemID || dependencies[dependency.ID].ID != "" ||
					dependency.CreatedBy != item.ActorID || dependency.CreatedAt != item.OccurredAt {
					return failure("invalid_event_payload", "dependency add is duplicate or self-referential")
				}
				for _, existing := range dependencies {
					if existing.SourceWorkItemID == dependency.SourceWorkItemID && existing.TargetWorkItemID == dependency.TargetWorkItemID {
						return failure("invalid_event_payload", "dependency ordered pair is duplicated")
					}
				}
				if dependency.Kind == "blocks" && replayBlockingPath(dependencies, dependency.OrganizationID, dependency.TargetWorkItemID, dependency.SourceWorkItemID) {
					return failure("dependency_cycle", "blocking dependency closes a cycle")
				}
				dependencies[dependency.ID] = dependency
			} else {
				prior, exists := dependencies[dependency.ID]
				if !dependencyEvent.Removed || !exists || !bytes.Equal(canonicalJSON(prior), canonicalJSON(dependency)) {
					return failure("invalid_event_payload", "dependency removal does not reference the current immutable edge")
				}
				delete(dependencies, dependency.ID)
			}
			workItems[source.ID] = source
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
	for _, item := range evidence {
		snapshot.Evidence = append(snapshot.Evidence, item)
	}
	for _, item := range gates {
		snapshot.Gates = append(snapshot.Gates, item)
	}
	for _, item := range verdicts {
		snapshot.Verdicts = append(snapshot.Verdicts, item)
	}
	for _, item := range findings {
		snapshot.Findings = append(snapshot.Findings, item)
	}
	for _, item := range findingActions {
		snapshot.FindingActions = append(snapshot.FindingActions, item)
	}
	for _, item := range submissions {
		snapshot.Submissions = append(snapshot.Submissions, item)
	}
	for _, item := range submissionHeads {
		snapshot.SubmissionHeads = append(snapshot.SubmissionHeads, item)
	}
	for _, item := range workItems {
		snapshot.WorkItems = append(snapshot.WorkItems, replayWorkItemView(item, workItems, dependencies, gates))
	}
	for _, item := range dependencies {
		snapshot.Dependencies = append(snapshot.Dependencies, item)
	}
	sortPlanningSnapshot(&snapshot)
	sortReviewSnapshot(&snapshot)
	sortWorkSnapshot(&snapshot)
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
	if err := loadLiveReview(ctx, queryer, &snapshot); err != nil {
		return projectionSnapshot{}, err
	}
	if err := loadLiveWork(ctx, queryer, &snapshot); err != nil {
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

func loadLiveReview(ctx context.Context, queryer databaseQueryer, snapshot *projectionSnapshot) error {
	evidenceRows, err := queryer.QueryContext(ctx, selectEvidence+` ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for evidenceRows.Next() {
		item, err := scanEvidence(evidenceRows)
		if err != nil {
			_ = evidenceRows.Close()
			return err
		}
		snapshot.Evidence = append(snapshot.Evidence, item)
	}
	if err := evidenceRows.Err(); err != nil {
		_ = evidenceRows.Close()
		return fmt.Errorf("scan live evidence: %w", err)
	}
	if err := evidenceRows.Close(); err != nil {
		return err
	}
	for index := range snapshot.Evidence {
		if err := loadEvidenceSupports(ctx, queryer, &snapshot.Evidence[index]); err != nil {
			return err
		}
	}

	gateRows, err := queryer.QueryContext(ctx, selectGate+` ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for gateRows.Next() {
		item, err := scanGate(gateRows)
		if err != nil {
			_ = gateRows.Close()
			return err
		}
		snapshot.Gates = append(snapshot.Gates, item)
	}
	if err := gateRows.Err(); err != nil {
		_ = gateRows.Close()
		return fmt.Errorf("scan live gates: %w", err)
	}
	if err := gateRows.Close(); err != nil {
		return err
	}
	for index := range snapshot.Gates {
		if err := loadGateRequirements(ctx, queryer, &snapshot.Gates[index]); err != nil {
			return err
		}
	}

	verdictRows, err := queryer.QueryContext(ctx, selectVerdict+` ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for verdictRows.Next() {
		item, err := scanVerdict(verdictRows)
		if err != nil {
			_ = verdictRows.Close()
			return err
		}
		snapshot.Verdicts = append(snapshot.Verdicts, item)
	}
	if err := verdictRows.Err(); err != nil {
		_ = verdictRows.Close()
		return fmt.Errorf("scan live verdicts: %w", err)
	}
	if err := verdictRows.Close(); err != nil {
		return err
	}

	findingRows, err := queryer.QueryContext(ctx, selectFinding+` ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for findingRows.Next() {
		item, err := scanFinding(findingRows)
		if err != nil {
			_ = findingRows.Close()
			return err
		}
		snapshot.Findings = append(snapshot.Findings, item)
	}
	if err := findingRows.Err(); err != nil {
		_ = findingRows.Close()
		return fmt.Errorf("scan live findings: %w", err)
	}
	if err := findingRows.Close(); err != nil {
		return err
	}

	actionRows, err := queryer.QueryContext(ctx, `SELECT id,organization_id,project_id,deliverable_id,finding_id,action::text,
		evidence_ids,rationale,actor_id,actor_kind::text,principal_id,created_at FROM finding_actions ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for actionRows.Next() {
		var item FindingAction
		var evidenceIDs pq.StringArray
		var principal sql.NullString
		var created time.Time
		if err := actionRows.Scan(&item.ID, &item.OrganizationID, &item.ProjectID, &item.DeliverableID, &item.FindingID,
			&item.Action, &evidenceIDs, &item.Rationale, &item.ActorID, &item.ActorKind, &principal, &created); err != nil {
			_ = actionRows.Close()
			return err
		}
		item.EvidenceIDs = []string(evidenceIDs)
		if principal.Valid {
			item.PrincipalID = &principal.String
		}
		item.CreatedAt = created.UTC().Format(timeFormat)
		snapshot.FindingActions = append(snapshot.FindingActions, item)
	}
	if err := actionRows.Err(); err != nil {
		_ = actionRows.Close()
		return fmt.Errorf("scan live finding actions: %w", err)
	}
	if err := actionRows.Close(); err != nil {
		return err
	}

	submissionRows, err := queryer.QueryContext(ctx, `SELECT id,organization_id,project_id,deliverable_id,kind::text,evidence_ids,
		note,submitted_by,submitter_kind::text,principal_id,created_at FROM deliverable_submissions ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	for submissionRows.Next() {
		item, err := scanSubmission(submissionRows)
		if err != nil {
			_ = submissionRows.Close()
			return err
		}
		snapshot.Submissions = append(snapshot.Submissions, item)
	}
	if err := submissionRows.Err(); err != nil {
		_ = submissionRows.Close()
		return fmt.Errorf("scan live submissions: %w", err)
	}
	if err := submissionRows.Close(); err != nil {
		return err
	}

	headRows, err := queryer.QueryContext(ctx, `SELECT deliverable_id,submission_id FROM deliverable_submission_heads ORDER BY deliverable_id`)
	if err != nil {
		return err
	}
	for headRows.Next() {
		var item SubmissionHead
		if err := headRows.Scan(&item.DeliverableID, &item.SubmissionID); err != nil {
			_ = headRows.Close()
			return err
		}
		snapshot.SubmissionHeads = append(snapshot.SubmissionHeads, item)
	}
	if err := headRows.Err(); err != nil {
		_ = headRows.Close()
		return fmt.Errorf("scan live submission heads: %w", err)
	}
	if err := headRows.Close(); err != nil {
		return err
	}
	sortReviewSnapshot(snapshot)
	return nil
}

func loadLiveWork(ctx context.Context, queryer databaseQueryer, snapshot *projectionSnapshot) error {
	rows, err := queryer.QueryContext(ctx, selectWorkItem+` ORDER BY project_id,id`)
	if err != nil {
		return err
	}
	records := make([]WorkItemRecord, 0)
	for rows.Next() {
		item, err := scanWorkItem(rows)
		if err != nil {
			_ = rows.Close()
			return err
		}
		records = append(records, item)
	}
	if err := rows.Err(); err != nil {
		_ = rows.Close()
		return fmt.Errorf("scan live work items: %w", err)
	}
	if err := rows.Close(); err != nil {
		return err
	}
	dependencyRows, err := queryer.QueryContext(ctx, selectWorkDependency+` ORDER BY source_work_item_id,target_work_item_id,id`)
	if err != nil {
		return err
	}
	for dependencyRows.Next() {
		item, err := scanWorkDependency(dependencyRows)
		if err != nil {
			_ = dependencyRows.Close()
			return err
		}
		snapshot.Dependencies = append(snapshot.Dependencies, item)
	}
	if err := dependencyRows.Err(); err != nil {
		_ = dependencyRows.Close()
		return fmt.Errorf("scan live work dependencies: %w", err)
	}
	if err := dependencyRows.Close(); err != nil {
		return err
	}
	for _, item := range records {
		view, err := workItemView(ctx, queryer, item, nil)
		if err != nil {
			return err
		}
		snapshot.WorkItems = append(snapshot.WorkItems, view)
	}
	sortWorkSnapshot(snapshot)
	return nil
}

func snapshotChecksum(snapshot projectionSnapshot) string {
	digest := sha256.Sum256(canonicalJSON(snapshot))
	return hex.EncodeToString(digest[:])
}

func (store *DurableStore) ActiveProjectionHead(ctx context.Context) (ReplayReport, error) {
	var report ReplayReport
	err := store.db.QueryRowContext(ctx, `SELECT run_id,last_sequence,projects_count,decisions_count,activity_count,planning_count,review_count,work_count,checksum
		FROM projection_heads WHERE name='m1-canonical'`).Scan(&report.RunID, &report.LastSequence,
		&report.Projects, &report.Decisions, &report.Activity, &report.Planning, &report.Review, &report.Work, &report.RebuiltChecksum)
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
	reviewOrphans, err := queryer.QueryContext(ctx, `
		SELECT kind,id,project_id FROM (
			SELECT 'evidence' kind,evidence.id::text id,evidence.project_id
			FROM evidence LEFT JOIN domain_events event
			  ON event.event_type IN ('evidence.created','evidence.superseded') AND event.payload->>'id'=evidence.id::text
			WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'gate',gate.id::text,gate.project_id FROM review_gates gate LEFT JOIN domain_events event
			  ON event.event_type='gate.created' AND event.payload->>'id'=gate.id::text WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'verdict',verdict.id::text,verdict.project_id FROM review_verdicts verdict LEFT JOIN domain_events event
			  ON event.event_type='gate.verdict_recorded' AND event.payload->'verdict'->>'id'=verdict.id::text WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'finding',finding.id::text,finding.project_id FROM review_findings finding LEFT JOIN domain_events event
			  ON event.event_type='finding.created' AND event.payload->>'id'=finding.id::text WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'finding-action',action.id::text,action.project_id FROM finding_actions action LEFT JOIN domain_events event
			  ON event.event_type IN ('finding.resolved','finding.withdrawn') AND event.payload->'action'->>'id'=action.id::text WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'submission',submission.id::text,submission.project_id FROM deliverable_submissions submission LEFT JOIN domain_events event
			  ON event.event_type IN ('deliverable.submitted','deliverable.resubmitted') AND event.payload->'submission'->>'id'=submission.id::text WHERE event.event_id IS NULL
		) orphan ORDER BY kind,id`)
	if err != nil {
		return nil, err
	}
	for reviewOrphans.Next() {
		var kind, id, projectID string
		if err := reviewOrphans.Scan(&kind, &id, &projectID); err != nil {
			_ = reviewOrphans.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "review_projection_without_event", Aggregate: "project:" + projectID,
			Detail: fmt.Sprintf("%s projection %s lacks its immutable event", kind, id)})
	}
	if err := reviewOrphans.Err(); err != nil {
		_ = reviewOrphans.Close()
		return nil, fmt.Errorf("scan review/event coupling: %w", err)
	}
	if err := reviewOrphans.Close(); err != nil {
		return nil, err
	}
	workOrphans, err := queryer.QueryContext(ctx, `
		SELECT kind,id,aggregate FROM (
			SELECT 'work-item' kind,work.id::text id,'work_item:' || work.id::text aggregate
			FROM work_items work LEFT JOIN domain_events event
			  ON event.aggregate_type='work_item' AND event.aggregate_id=work.id
			  AND event.event_type IN ('work_item.created','work_item.updated','work_item.assigned','work_item.started',
			    'work_item.review_requested','work_item.bounced','work_item.accepted','work_item.cancelled')
			WHERE event.event_id IS NULL
			UNION ALL
			SELECT 'dependency',dependency.id::text,'work_item:' || dependency.source_work_item_id::text
			FROM work_item_dependencies dependency LEFT JOIN domain_events event
			  ON event.event_type='dependency.added'
			  AND event.payload->'dependency'->>'id'=dependency.id::text
			WHERE event.event_id IS NULL
		) orphan ORDER BY kind,id`)
	if err != nil {
		return nil, err
	}
	for workOrphans.Next() {
		var kind, id, aggregate string
		if err := workOrphans.Scan(&kind, &id, &aggregate); err != nil {
			_ = workOrphans.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "work_projection_without_event", Aggregate: aggregate,
			Detail: fmt.Sprintf("%s projection %s lacks its immutable event", kind, id)})
	}
	if err := workOrphans.Err(); err != nil {
		_ = workOrphans.Close()
		return nil, fmt.Errorf("scan work/event coupling: %w", err)
	}
	if err := workOrphans.Close(); err != nil {
		return nil, err
	}
	cycleRows, err := queryer.QueryContext(ctx, `WITH RECURSIVE paths(organization_id,start_id,current_id,path,cycle) AS (
		SELECT organization_id,source_work_item_id,target_work_item_id,
		  ARRAY[source_work_item_id,target_work_item_id],source_work_item_id=target_work_item_id
		FROM work_item_dependencies WHERE kind='blocks'
		UNION ALL
		SELECT path.organization_id,path.start_id,dependency.target_work_item_id,
		  path.path || dependency.target_work_item_id,dependency.target_work_item_id=ANY(path.path)
		FROM paths path JOIN work_item_dependencies dependency
		  ON dependency.organization_id=path.organization_id AND dependency.source_work_item_id=path.current_id
		WHERE dependency.kind='blocks' AND NOT path.cycle
	) SELECT DISTINCT organization_id,start_id FROM paths WHERE cycle ORDER BY organization_id,start_id`)
	if err != nil {
		return nil, err
	}
	for cycleRows.Next() {
		var organizationID, startID string
		if err := cycleRows.Scan(&organizationID, &startID); err != nil {
			_ = cycleRows.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "dependency_cycle", Aggregate: "work_item:" + startID,
			Detail: fmt.Sprintf("organization %s contains a persisted blocking cycle", organizationID)})
	}
	if err := cycleRows.Err(); err != nil {
		_ = cycleRows.Close()
		return nil, fmt.Errorf("scan blocking dependency cycles: %w", err)
	}
	if err := cycleRows.Close(); err != nil {
		return nil, err
	}
	integrityRows, err := queryer.QueryContext(ctx, `SELECT id,project_id FROM evidence
		WHERE integrity_digest IS DISTINCT FROM digest(convert_to(integrity_material::text,'UTF8'),'sha256') ORDER BY project_id,id`)
	if err != nil {
		return nil, err
	}
	for integrityRows.Next() {
		var id, projectID string
		if err := integrityRows.Scan(&id, &projectID); err != nil {
			_ = integrityRows.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "evidence_digest_mismatch", Aggregate: "project:" + projectID,
			Detail: fmt.Sprintf("evidence %s digest differs from its canonical material", id)})
	}
	if err := integrityRows.Err(); err != nil {
		_ = integrityRows.Close()
		return nil, fmt.Errorf("scan evidence digest integrity: %w", err)
	}
	_ = integrityRows.Close()
	reviewReferences, err := queryer.QueryContext(ctx, `
		SELECT kind,id,project_id,evidence_id FROM (
			SELECT 'verdict' kind,verdict.id::text id,verdict.project_id,selected.evidence_id
			FROM review_verdicts verdict CROSS JOIN LATERAL unnest(verdict.evidence_ids) selected(evidence_id)
			LEFT JOIN evidence ON evidence.id=selected.evidence_id AND evidence.project_id=verdict.project_id
			WHERE evidence.id IS NULL
			UNION ALL
			SELECT 'finding-action',action.id::text,action.project_id,selected.evidence_id
			FROM finding_actions action CROSS JOIN LATERAL unnest(action.evidence_ids) selected(evidence_id)
			LEFT JOIN evidence ON evidence.id=selected.evidence_id AND evidence.project_id=action.project_id
			WHERE evidence.id IS NULL
			UNION ALL
			SELECT 'submission',submission.id::text,submission.project_id,selected.evidence_id
			FROM deliverable_submissions submission CROSS JOIN LATERAL unnest(submission.evidence_ids) selected(evidence_id)
			LEFT JOIN evidence ON evidence.id=selected.evidence_id AND evidence.project_id=submission.project_id
			WHERE evidence.id IS NULL
		) invalid ORDER BY kind,id,evidence_id`)
	if err != nil {
		return nil, err
	}
	for reviewReferences.Next() {
		var kind, id, projectID, evidenceID string
		if err := reviewReferences.Scan(&kind, &id, &projectID, &evidenceID); err != nil {
			_ = reviewReferences.Close()
			return nil, err
		}
		findings = append(findings, IntegrityFinding{Code: "review_evidence_reference_invalid", Aggregate: "project:" + projectID,
			Detail: fmt.Sprintf("%s %s references missing or cross-project evidence %s", kind, id, evidenceID)})
	}
	if err := reviewReferences.Err(); err != nil {
		_ = reviewReferences.Close()
		return nil, fmt.Errorf("scan review evidence references: %w", err)
	}
	_ = reviewReferences.Close()

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
			UNION ALL
			SELECT 'submission',submission.project_id,head.submission_id::text FROM deliverable_submission_heads head
			JOIN deliverable_submissions submission ON submission.id=head.submission_id
			WHERE submission.deliverable_id<>head.deliverable_id
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
			'deliverable.created','deliverable.revised','forecast.created','forecast.superseded','target.changed','deadline.changed',
			'evidence.created','evidence.superseded','gate.created','gate.verdict_recorded','gate.waived',
			'finding.created','finding.resolved','finding.withdrawn','deliverable.submitted','deliverable.bounced',
			'deliverable.resubmitted','deliverable.accepted','deliverable.cancelled','deliverable.waived',
			'work_item.created','work_item.updated','work_item.assigned','work_item.started','work_item.review_requested',
			'work_item.bounced','work_item.accepted','work_item.cancelled','dependency.added','dependency.removed'
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
	var trailing any
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		if err == nil {
			return errors.New("multiple JSON values")
		}
		return err
	}
	return nil
}

func equalOptionalString(left, right *string) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return *left == *right
}
