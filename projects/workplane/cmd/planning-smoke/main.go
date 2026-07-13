package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/app"
	_ "github.com/lib/pq"
)

const (
	organizationID     = "00000000-0000-4000-8000-000000000010"
	humanID            = "00000000-0000-4000-8000-000000000001"
	agentID            = "00000000-0000-4000-8000-000000000002"
	agentToken         = "wpa_local_walking_slice_agent_token_00000000000000000001"
	reviewerHumanID    = "00000000-0000-4000-8000-000000000031"
	reviewerAgentID    = "00000000-0000-4000-8000-000000000032"
	reviewerSessionID  = "00000000-0000-4000-8000-000000000033"
	reviewerSession    = "m2d-reviewer-session-token-000000000000000000000001"
	reviewerCSRF       = "m2d-reviewer-csrf-token-0000000000000000000000001"
	reviewerAgentToken = "wpa_m2d_reviewer_agent_token_000000000000000000000001"
	publicOrigin       = "http://localhost:8080"
)

type snapshot struct {
	Status  int               `json:"status"`
	Body    json.RawMessage   `json:"body"`
	Headers map[string]string `json:"headers"`
}

type session struct {
	CSRF string `json:"csrf_token"`
}
type project struct {
	ID      string `json:"id"`
	Mode    string `json:"mode"`
	State   string `json:"state"`
	Version int64  `json:"version"`
}
type deliverable struct {
	ID                 string   `json:"id"`
	OrganizationID     string   `json:"organization_id"`
	ProjectID          string   `json:"project_id"`
	Title              string   `json:"title"`
	Description        string   `json:"description"`
	Required           bool     `json:"required"`
	Weight             int64    `json:"weight"`
	State              string   `json:"state"`
	AcceptanceCriteria []string `json:"acceptance_criteria"`
	Version            int64    `json:"version"`
	CreatedBy          string   `json:"created_by"`
	WaiverDecisionID   *string  `json:"waiver_decision_id"`
}
type promotion struct {
	Project      project       `json:"project"`
	Deliverables []deliverable `json:"deliverables"`
}
type forecast struct {
	ID             string   `json:"id"`
	OrganizationID string   `json:"organization_id"`
	ProjectID      string   `json:"project_id"`
	DeliverableID  *string  `json:"deliverable_id"`
	Scope          string   `json:"scope"`
	P50At          string   `json:"p50_at"`
	P90At          string   `json:"p90_at"`
	ReviewAfter    string   `json:"review_after"`
	Basis          string   `json:"basis"`
	Assumptions    []string `json:"assumptions"`
	ReasonCodes    []string `json:"reason_codes"`
	Impact         string   `json:"impact"`
	SupersedesID   *string  `json:"supersedes_id"`
	CreatedBy      string   `json:"created_by"`
	Current        bool     `json:"current"`
	Stale          bool     `json:"stale"`
	AttentionOnly  bool     `json:"attention_only"`
}
type target struct {
	ID            string `json:"id"`
	ProjectID     string `json:"project_id"`
	TargetAt      string `json:"target_at"`
	Reason        string `json:"reason"`
	CreatedBy     string `json:"created_by"`
	Current       bool   `json:"current"`
	Missed        bool   `json:"missed"`
	AttentionOnly bool   `json:"attention_only"`
}
type deadline struct {
	ID            string `json:"id"`
	ProjectID     string `json:"project_id"`
	DeadlineAt    string `json:"deadline_at"`
	Source        string `json:"source"`
	Description   string `json:"description"`
	CreatedBy     string `json:"created_by"`
	Current       bool   `json:"current"`
	Passed        bool   `json:"passed"`
	AttentionOnly bool   `json:"attention_only"`
}
type readDenyCase struct {
	Label string
	Path  string
}
type counts struct {
	Projects, Deliverables, Forecasts, Targets, Deadlines, Evidence, Gates, Verdicts, Findings, FindingActions,
	Submissions, Decisions, Events, Outbox, Idempotency int
}

type evidence struct {
	ID              string  `json:"id"`
	Kind            string  `json:"kind"`
	Claim           string  `json:"claim"`
	IntegrityDigest string  `json:"integrity_digest"`
	ProducedBy      string  `json:"produced_by"`
	ProducerKind    string  `json:"producer_kind"`
	PrincipalID     *string `json:"principal_id"`
	SupersedesID    *string `json:"supersedes_id"`
}

type gate struct {
	ID                   string `json:"id"`
	Kind                 string `json:"kind"`
	State                string `json:"state"`
	Hard                 bool   `json:"hard"`
	IndependenceRequired bool   `json:"independence_required"`
	Version              int64  `json:"version"`
}

type finding struct {
	ID       string `json:"id"`
	State    string `json:"state"`
	Blocking bool   `json:"blocking"`
	Version  int64  `json:"version"`
}

type verdictResult struct {
	Gate    gate `json:"gate"`
	Verdict struct {
		ID           string  `json:"id"`
		Result       string  `json:"result"`
		ReviewerID   string  `json:"reviewer_id"`
		SupersedesID *string `json:"supersedes_id"`
	} `json:"verdict"`
	Findings []finding `json:"findings"`
}

type submissionResult struct {
	Deliverable deliverable `json:"deliverable"`
	Submission  struct {
		ID          string `json:"id"`
		Kind        string `json:"kind"`
		SubmittedBy string `json:"submitted_by"`
	} `json:"submission"`
}

type findingActionResult struct {
	Finding finding `json:"finding"`
	Action  struct {
		ID      string `json:"id"`
		Action  string `json:"action"`
		ActorID string `json:"actor_id"`
	} `json:"action"`
}

type sseEvent struct {
	Kind string
	ID   string
	Data json.RawMessage
}

type sseClient struct {
	response *http.Response
	reader   *bufio.Reader
}

type runner struct {
	base, artifacts, csrf                      string
	human, agent, reviewerHuman, reviewerAgent *http.Client
	db                                         *sql.DB
	checks                                     []string
}

func main() {
	jar, err := cookiejar.New(nil)
	check(err)
	db, err := sql.Open("postgres", envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	check(err)
	defer db.Close()
	run := &runner{
		base: envOr("WORKPLANE_API_BASE", "http://api:8080"), artifacts: envOr("WORKPLANE_EVIDENCE_DIR", "/evidence"),
		human: &http.Client{Jar: jar, Timeout: 10 * time.Second}, agent: &http.Client{Timeout: 10 * time.Second}, db: db,
	}
	check(os.MkdirAll(run.artifacts, 0o755))
	check(run.execute(context.Background()))
	fmt.Printf("M2C planning and M2D evidence-review spines passed: %d load-bearing checks\n", len(run.checks))
}

func (run *runner) execute(ctx context.Context) error {
	login := run.call(run.human, http.MethodPost, "/api/v1/session/login", map[string]any{
		"email": "human@workplane.local", "password": "walking-slice-password",
	}, nil)
	if err := expect(login, http.StatusOK, ""); err != nil {
		return err
	}
	var authenticated session
	if err := json.Unmarshal(login.Body, &authenticated); err != nil {
		return err
	}
	run.csrf = authenticated.CSRF
	if len(run.csrf) < 32 {
		return errors.New("login omitted CSRF token")
	}
	_, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET scopes=ARRAY[
		'project.create','project.read','project.activate','project.hold','project.resume','project.promote',
		'project.reforecast','project.target.write','project.deadline.write','decision.record',
		'deliverable.read','deliverable.edit','deliverable.reforecast','deliverable.submit','review.request',
		'evidence.read','evidence.create','evidence.supersede','finding.resolve','deliverable.waive','gate.soft_waive',
		'realtime.subscribe','event.subscribe'
	],project_ids=ARRAY[]::uuid[],revoked_at=NULL WHERE token_prefix=$1`, agentToken[:16])
	if err != nil {
		return err
	}

	humanProject, humanPromotion, humanTypes, err := run.parityFlow(ctx, run.human, "human", run.humanHeaders(), "m2c-human")
	if err != nil {
		return err
	}
	agentProject, agentPromotion, agentTypes, err := run.parityFlow(ctx, run.agent, "agent", run.agentHeaders(), "m2c-agent")
	if err != nil {
		return err
	}
	if !reflect.DeepEqual(humanTypes, agentTypes) || humanProject.Version != agentProject.Version ||
		len(humanPromotion.Deliverables) != 1 || len(agentPromotion.Deliverables) != 1 {
		return fmt.Errorf("human/agent planning parity diverged: human=%v agent=%v", humanTypes, agentTypes)
	}
	run.write("m2c-human-agent-parity.json", map[string]any{
		"event_types": humanTypes, "project_version": humanProject.Version,
		"actors": []string{"human", "agent"}, "excluded": []string{"generated ids", "timestamps", "attributable actor identity"},
	})
	run.checks = append(run.checks, "human-agent-planning-field-and-event-parity")
	if err := run.idempotentDecisionReauthorizationExpectedDeny(ctx); err != nil {
		return err
	}

	projectID := humanProject.ID
	version := humanProject.Version
	omissionBefore := run.counts(ctx)
	omittedRequired := deliverableInput("Omitted requiredness must fail")
	delete(omittedRequired, "required")
	omitted := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deliverables", omittedRequired,
		run.versionedHuman("m2c-required-omitted-01", version))
	if err := expect(omitted, http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}
	if got := run.counts(ctx); got != omissionBefore {
		return fmt.Errorf("omitted deliverable required field left residue: before=%+v after=%+v", omissionBefore, got)
	}

	optionalInput := deliverableInput("Explicit false remains valid")
	optionalInput["required"] = false
	const crossProjectKey = "m2c-cross-project-idempotency-01"
	createdOptional := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deliverables", optionalInput,
		run.versionedHuman(crossProjectKey, version))
	if err := expect(createdOptional, http.StatusCreated, ""); err != nil {
		return err
	}
	var optionalDeliverable deliverable
	if err := json.Unmarshal(createdOptional.Body, &optionalDeliverable); err != nil {
		return err
	}
	if optionalDeliverable.ProjectID != projectID || optionalDeliverable.Required {
		return fmt.Errorf("explicit false deliverable did not preserve target/requiredness: %+v", optionalDeliverable)
	}
	version = etagVersion(createdOptional)
	afterFirstTarget := run.counts(ctx)
	crossProject := run.call(run.human, http.MethodPost, "/api/v1/projects/"+agentProject.ID+"/deliverables", optionalInput,
		run.versionedHuman(crossProjectKey, agentProject.Version))
	if err := expect(crossProject, http.StatusConflict, "idempotency_conflict"); err != nil {
		return err
	}
	if bytes.Contains(crossProject.Body, []byte(optionalDeliverable.ID)) || bytes.Contains(crossProject.Body, []byte(projectID)) {
		return fmt.Errorf("cross-project idempotency conflict disclosed the first target: %s", crossProject.Body)
	}
	if got := run.counts(ctx); got != afterFirstTarget {
		return fmt.Errorf("cross-project idempotency conflict wrote rows: before=%+v after=%+v", afterFirstTarget, got)
	}
	run.write("m2c-required-and-idempotency.json", map[string]any{
		"omitted_required": "invalid-request-no-write", "explicit_false": "created",
		"cross_project_reuse": "idempotency-conflict-no-disclosure-no-write",
	})
	run.checks = append(run.checks, "required-boolean-presence-and-cross-project-idempotency-target-binding")

	before := run.counts(ctx)
	missingReason := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/hold", map[string]any{}, run.versionedHuman("m2c-hold-missing-reason", version))
	if err := expect(missingReason, http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("missing lifecycle reason wrote rows: before=%+v after=%+v", before, got)
	}

	fault := run.versionedHuman("m2c-hold-fault-0001", version)
	fault["X-Workplane-Fault"] = "after-project"
	if err := expect(run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/hold", map[string]any{"reason": "Pause for review"}, fault), http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return err
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("hold fault left residue: before=%+v after=%+v", before, got)
	}
	if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/hold",
		map[string]any{"reason": "Pause for review"}, run.versionedHuman("m2c-hold-event-fault", version), "after-lifecycle-event"); err != nil {
		return err
	}
	version, err = run.concurrentHold(projectID, version)
	if err != nil {
		return err
	}
	resume := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/resume", map[string]any{"reason": "Review complete"}, run.versionedHuman("m2c-resume-commit-01", version))
	if err := expect(resume, http.StatusOK, ""); err != nil {
		return err
	}
	version = etagVersion(resume)
	run.checks = append(run.checks, "hold-resume-reason-version-and-fault-atomicity")

	deliverableID := humanPromotion.Deliverables[0].ID
	revision := deliverableInput("Revised M2C deliverable")
	for index, boundary := range []string{"after-deliverable", "after-deliverable-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPatch, "/api/v1/deliverables/"+deliverableID, revision,
			run.versionedHuman(fmt.Sprintf("m2c-revise-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	version, err = run.crossDeliverableRevisionIdempotency(ctx, deliverableID, optionalDeliverable.ID, version)
	if err != nil {
		return err
	}
	version, err = run.concurrentDeliverableRevision(deliverableID, version)
	if err != nil {
		return err
	}
	crossActorRevision := run.call(run.agent, http.MethodPatch, "/api/v1/deliverables/"+deliverableID,
		deliverableInput("Cross-actor M2C revision"), merge(run.agentHeaders(), map[string]string{
			"Idempotency-Key": "m2c-cross-actor-revise-01", "If-Match": fmt.Sprintf(`"%d"`, version),
		}))
	if err := expect(crossActorRevision, http.StatusOK, ""); err != nil {
		return err
	}
	version = etagVersion(crossActorRevision)
	var revised deliverable
	if err := json.Unmarshal(crossActorRevision.Body, &revised); err != nil {
		return err
	}
	var revisionActor, stableCreator string
	if err := run.db.QueryRowContext(ctx, `SELECT actor_id::text,payload->>'created_by' FROM domain_events
		WHERE event_type='deliverable.revised' AND payload->>'id'=$1 ORDER BY sequence DESC LIMIT 1`, deliverableID).
		Scan(&revisionActor, &stableCreator); err != nil {
		return err
	}
	if revisionActor != agentID || stableCreator != humanID || revised.ID != deliverableID {
		return fmt.Errorf("cross-actor revision attribution diverged: actor=%s creator=%s deliverable=%+v", revisionActor, stableCreator, revised)
	}
	run.checks = append(run.checks, "cross-actor-deliverable-revision-attribution-and-replay")
	version, optionalDeliverable, err = run.publicDeliverableProjectionMatrix(ctx, projectID, revised, optionalDeliverable, version)
	if err != nil {
		return err
	}
	for index, boundary := range []string{"after-deliverable", "after-deliverable-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deliverables",
			deliverableInput("Optional fault canary"), run.versionedHuman(fmt.Sprintf("m2c-create-del-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE deliverables SET state='accepted' WHERE id=$1`, deliverableID); err != nil {
		return err
	}
	acceptedBefore := run.counts(ctx)
	acceptedEdit := run.call(run.human, http.MethodPatch, "/api/v1/deliverables/"+deliverableID, revision, run.versionedHuman("m2c-accepted-edit-deny", version))
	if err := expect(acceptedEdit, http.StatusConflict, "invariant_violation"); err != nil {
		return err
	}
	if got := run.counts(ctx); got != acceptedBefore {
		return fmt.Errorf("accepted edit left residue")
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE deliverables SET state='ready' WHERE id=$1`, deliverableID); err == nil {
		return errors.New("PostgreSQL accepted-deliverable trigger allowed edit")
	}
	// Test fixtures may only bypass the immutable-state trigger by disabling it in
	// the same transaction, and restore the prior state before replay.
	if _, err := run.db.ExecContext(ctx, `ALTER TABLE deliverables DISABLE TRIGGER deliverable_immutable_state`); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE deliverables SET state='ready' WHERE id=$1`, deliverableID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `ALTER TABLE deliverables ENABLE TRIGGER deliverable_immutable_state`); err != nil {
		return err
	}
	if err := run.terminalDeliverableImmutability(ctx, projectID); err != nil {
		return err
	}
	run.write("m2c-deliverables.json", map[string]any{
		"stable_id": deliverableID, "cross_actor_revision": "creator-stable-event-reviser-attributed",
		"terminal_states": []string{"accepted", "waived", "cancelled"}, "protected_operations": []string{"update", "delete"},
		"public_get_list": "two-revised-deliverables-exact-fields", "private_nonmember_read": "not-found-no-residue",
	})
	run.checks = append(run.checks, "deliverable-stable-revision-and-terminal-update-delete-protection")

	forecastPath := "/api/v1/deliverables/" + deliverableID + "/forecasts"
	version, err = run.crossDeliverableReforecastIdempotency(ctx, deliverableID, optionalDeliverable.ID, version)
	if err != nil {
		return err
	}
	run.write("m2c-leaf-idempotency.json", map[string]any{
		"revision_cross_deliverable_reuse":   "idempotency-conflict-no-disclosure-no-write",
		"reforecast_cross_deliverable_reuse": "idempotency-conflict-no-disclosure-no-write",
	})
	run.checks = append(run.checks, "complete-deliverable-leaf-idempotency-target-binding")
	for index, boundary := range []string{"after-forecast", "after-forecast-superseded-event", "after-forecast-created-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, forecastPath, forecastInput(36, 72, -1),
			run.versionedHuman(fmt.Sprintf("m2c-del-forecast-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	secondForecast := run.call(run.human, http.MethodPost, forecastPath, forecastInput(36, 72, -1), run.versionedHuman("m2c-deliverable-forecast-2", version))
	if err := expect(secondForecast, http.StatusCreated, ""); err != nil {
		return err
	}
	version = etagVersion(secondForecast)
	history := run.call(run.human, http.MethodGet, forecastPath, nil, nil)
	if err := expect(history, http.StatusOK, ""); err != nil {
		return err
	}
	var forecasts []forecast
	if err := json.Unmarshal(history.Body, &forecasts); err != nil {
		return err
	}
	if len(forecasts) != 2 || forecasts[1].SupersedesID == nil || !forecasts[1].Current || forecasts[0].Current ||
		!forecasts[1].Stale || !forecasts[1].AttentionOnly {
		return fmt.Errorf("invalid immutable forecast history: %+v", forecasts)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE forecasts SET basis='tampered' WHERE id=$1`, forecasts[0].ID); err == nil || !strings.Contains(err.Error(), "append-only") {
		return fmt.Errorf("forecast history update did not fail closed: %v", err)
	}
	run.checks = append(run.checks, "deliverable-forecast-scope-current-and-immutable-history")
	projectHistoryID, err := run.publicProjectForecastProjectionMatrix(ctx)
	if err != nil {
		return err
	}

	stateBeforeDates := "active"
	for index, boundary := range []string{"after-target", "after-target-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
			"target_at": time.Now().UTC().Add(-time.Hour).Format(time.RFC3339), "reason": "Fault rollback canary",
		}, run.versionedHuman(fmt.Sprintf("m2c-target-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	targetAt := time.Now().UTC().Add(-time.Hour).Truncate(time.Second)
	const targetReason = "Intent remains visible after miss"
	targetResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
		"target_at": targetAt.Format(time.RFC3339), "reason": targetReason,
	}, run.versionedHuman("m2c-target-past-0001", version))
	if err := expect(targetResponse, http.StatusCreated, ""); err != nil {
		return err
	}
	var targetProjection target
	if err := json.Unmarshal(targetResponse.Body, &targetProjection); err != nil {
		return fmt.Errorf("decode public target projection: %w", err)
	}
	if err := run.assertTargetProjection(ctx, projectID, targetAt, targetReason, targetProjection); err != nil {
		return err
	}
	version = etagVersion(targetResponse)
	for index, boundary := range []string{"after-deadline", "after-deadline-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deadline", map[string]any{
			"deadline_at": time.Now().UTC().Add(-30 * time.Minute).Format(time.RFC3339), "source": "contract", "description": "Fault rollback canary",
		}, run.versionedHuman(fmt.Sprintf("m2c-deadline-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	deadlineAt := time.Now().UTC().Add(-30 * time.Minute).Truncate(time.Second)
	const deadlineDescription = "External constraint only"
	deadlineResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deadline", map[string]any{
		"deadline_at": deadlineAt.Format(time.RFC3339), "source": "contract", "description": deadlineDescription,
	}, run.versionedHuman("m2c-deadline-past-01", version))
	if err := expect(deadlineResponse, http.StatusCreated, ""); err != nil {
		return err
	}
	var deadlineProjection deadline
	if err := json.Unmarshal(deadlineResponse.Body, &deadlineProjection); err != nil {
		return fmt.Errorf("decode public deadline projection: %w", err)
	}
	if err := run.assertDeadlineProjection(ctx, projectID, deadlineAt, deadlineDescription, deadlineProjection); err != nil {
		return err
	}
	version = etagVersion(deadlineResponse)
	read := run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectID, nil, nil)
	if err := expect(read, http.StatusOK, ""); err != nil {
		return err
	}
	var current project
	if err := json.Unmarshal(read.Body, &current); err != nil {
		return err
	}
	if current.State != stateBeforeDates || version != current.Version {
		return fmt.Errorf("date command transitioned state or lost version: %+v version=%d", current, version)
	}
	run.write("m2c-forecasts.json", map[string]any{
		"deliverable_history_count": len(forecasts), "project_history_count": 2, "project_history_project_id": projectHistoryID,
		"current_per_scope": 1, "history_update": "denied", "project_history_read": "exact-fields-and-private-nonmember-deny",
		"target_id": targetProjection.ID, "target_at": targetProjection.TargetAt, "target_missed": targetProjection.Missed,
		"deadline_id": deadlineProjection.ID, "deadline_at": deadlineProjection.DeadlineAt, "deadline_source": deadlineProjection.Source,
		"attention_only": targetProjection.AttentionOnly && deadlineProjection.AttentionOnly, "state": current.State,
	})
	run.checks = append(run.checks, "target-deadline-distinction-and-attention-only")

	if err := run.concurrentForecastConflict(projectID, version); err != nil {
		return err
	}
	latest := run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectID, nil, nil)
	_ = json.Unmarshal(latest.Body, &current)
	version = current.Version
	run.checks = append(run.checks, "real-postgresql-conflicting-forecast-write")

	if err := run.authorityDenies(ctx, humanProject.ID, agentProject.ID, version); err != nil {
		return err
	}
	version, err = run.realtimePlanningResume(ctx, projectID, version)
	if err != nil {
		return err
	}
	if err := run.reviewFlow(ctx); err != nil {
		return fmt.Errorf("M2D evidence review: %w", err)
	}

	store, err := app.NewDurableStore(envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	if err != nil {
		return err
	}
	defer store.Close()
	replay, err := store.Replay(ctx)
	if err != nil {
		return fmt.Errorf("planning replay: %w", err)
	}
	if replay.LiveChecksum != replay.RebuiltChecksum || replay.Planning == 0 || replay.Review == 0 {
		return fmt.Errorf("planning replay mismatch: %+v", replay)
	}
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return err
	}
	findings, err := store.Doctor(ctx)
	if err != nil || len(findings) != 0 {
		return fmt.Errorf("planning integrity findings=%+v err=%v", findings, err)
	}
	run.write("m2c-durability.json", map[string]any{"replay": replay, "doctor_findings": 0, "realtime_delivery": "complete", "counts": run.counts(ctx)})
	run.checks = append(run.checks, "empty-replay-checksum-doctor-and-realtime-delivery")

	run.write("m2c-lifecycle.json", map[string]any{"nonterminal_states": []string{"proposed", "active", "held"}, "promotion_mode": "exploitation", "final_version": version, "terminal_commands": "absent"})
	run.write("m2c-summary.json", map[string]any{"result": "pass", "checks": run.checks, "human_project_id": humanProject.ID, "agent_project_id": agentProject.ID})
	return nil
}

func (run *runner) reviewFlow(ctx context.Context) error {
	if err := run.bootstrapReviewers(ctx); err != nil {
		return err
	}
	accepted, err := run.acceptanceReviewFlow(ctx)
	if err != nil {
		return err
	}
	waived, err := run.waiverReviewFlow(ctx)
	if err != nil {
		return err
	}
	if err := run.assertReviewLedger(ctx); err != nil {
		return err
	}
	run.write("m2d-summary.json", map[string]any{
		"result": "pass", "checks": run.checks, "accepted_project_id": accepted, "waived_project_id": waived,
	})
	return nil
}

func (run *runner) bootstrapReviewers(ctx context.Context) error {
	now := time.Now().UTC()
	tx, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	statements := []struct {
		query string
		args  []any
	}{
		{`INSERT INTO principals (id,kind,display_name,status,human_principal_id,created_at)
			VALUES ($1,'human','M2D independent reviewer','active',NULL,$2)`, []any{reviewerHumanID, now}},
		{`INSERT INTO principals (id,kind,display_name,status,human_principal_id,created_at)
			VALUES ($1,'agent','M2D independent reviewer agent','active',$2,$3)`, []any{reviewerAgentID, reviewerHumanID, now}},
		{`INSERT INTO organization_memberships (organization_id,principal_id,role,created_at)
			VALUES ($1,$2,'member',$3)`, []any{organizationID, reviewerHumanID, now}},
		{`INSERT INTO organization_memberships (organization_id,principal_id,role,created_at)
			VALUES ($1,$2,'member',$3)`, []any{organizationID, reviewerAgentID, now}},
		{`INSERT INTO human_sessions (id,principal_id,token_hash,csrf_hash,expires_at,created_at)
			VALUES ($1,$2,$3,$4,$5,$6)`, []any{reviewerSessionID, reviewerHumanID, m2dKeyedHash(reviewerSession), m2dKeyedHash(reviewerCSRF), now.Add(time.Hour), now}},
		{`INSERT INTO agent_tokens (id,agent_id,organization_id,token_prefix,token_hash,scopes,project_ids,expires_at,created_by,created_at)
			VALUES ('00000000-0000-4000-8000-000000000034',$1,$2,$3,$4,
			ARRAY['project.read','deliverable.read','evidence.read','review.verdict','finding.withdraw','realtime.subscribe','event.subscribe'],
			ARRAY[]::uuid[],$5,$6,$7)`, []any{reviewerAgentID, organizationID, reviewerAgentToken[:16], m2dKeyedHash(reviewerAgentToken), now.Add(time.Hour), reviewerHumanID, now}},
	}
	for _, statement := range statements {
		if _, err := tx.ExecContext(ctx, statement.query, statement.args...); err != nil {
			return err
		}
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	jar, err := cookiejar.New(nil)
	if err != nil {
		return err
	}
	baseURL, err := url.Parse(run.base)
	if err != nil {
		return err
	}
	jar.SetCookies(baseURL, []*http.Cookie{{Name: "workplane_session", Value: reviewerSession, Path: "/"}})
	run.reviewerHuman = &http.Client{Jar: jar, Timeout: 10 * time.Second}
	run.reviewerAgent = &http.Client{Timeout: 10 * time.Second}
	return nil
}

func (run *runner) grantReviewers(ctx context.Context, projectID string) error {
	_, err := run.db.ExecContext(ctx, `INSERT INTO project_memberships (project_id,principal_id,role,created_at)
		VALUES ($1,$2,'reviewer',CURRENT_TIMESTAMP),($1,$3,'reviewer',CURRENT_TIMESTAMP)`, projectID, reviewerHumanID, reviewerAgentID)
	return err
}

func (run *runner) acceptanceReviewFlow(ctx context.Context) (string, error) {
	projectItem, promoted, _, err := run.parityFlow(ctx, run.human, "human", run.humanHeaders(), "m2d-acceptance")
	if err != nil {
		return "", err
	}
	if err := run.grantReviewers(ctx, projectItem.ID); err != nil {
		return "", err
	}
	deliverableItem := promoted.Deliverables[0]
	version := projectItem.Version
	claim := "The exact-head M2D evidence and review smoke passes"
	gateInput := reviewGateInput("Exact-head evidence", true, claim)
	for index, boundary := range []string{"after-gate", "after-gate-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/gates", gateInput,
			run.reviewHeaders(run.humanHeaders(), fmt.Sprintf("m2d-gate-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return "", err
	}
	initialStream, err := run.openSSE("")
	if err != nil {
		return "", err
	}
	streamHead, streamErr := initialStream.next(3 * time.Second)
	initialStream.close()
	if streamErr != nil || streamHead.Kind != "ready" || streamHead.ID == "" {
		return "", fmt.Errorf("M2D SSE head=%+v err=%v", streamHead, streamErr)
	}
	gateExpectedVersion := version
	createdGate := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/gates", gateInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-gate-create", gateExpectedVersion))
	if err := expect(createdGate, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var gateItem gate
	if err := json.Unmarshal(createdGate.Body, &gateItem); err != nil {
		return "", err
	}
	if !gateItem.Hard || !gateItem.IndependenceRequired || gateItem.State != "pending" {
		return "", fmt.Errorf("invalid hard independent gate: %+v", gateItem)
	}
	gateRetry := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/gates", gateInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-gate-create", gateExpectedVersion))
	if err := expect(gateRetry, http.StatusCreated, ""); err != nil || !bytes.Equal(gateRetry.Body, createdGate.Body) ||
		gateRetry.Headers["X-Request-ID"] != createdGate.Headers["X-Request-ID"] {
		return "", fmt.Errorf("M2D gate idempotent retry diverged: err=%v first=%+v retry=%+v", err, createdGate, gateRetry)
	}
	version = etagVersion(createdGate)
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return "", err
	}
	resumedStream, err := run.openSSE(streamHead.ID)
	if err != nil {
		return "", err
	}
	defer resumedStream.close()
	resumeReady, readyErr := resumedStream.next(3 * time.Second)
	streamEvent, eventErr := resumedStream.next(3 * time.Second)
	resumedStream.close()
	var streamed struct {
		EventType   string `json:"event_type"`
		AggregateID string `json:"aggregate_id"`
	}
	if readyErr != nil || resumeReady.Kind != "ready" || eventErr != nil || streamEvent.Kind != "domain-event" ||
		json.Unmarshal(streamEvent.Data, &streamed) != nil || streamed.EventType != "gate.created" || streamed.AggregateID != projectItem.ID {
		return "", fmt.Errorf("M2D resumed SSE ready=%+v event=%+v envelope=%+v ready_err=%v event_err=%v", resumeReady, streamEvent, streamed, readyErr, eventErr)
	}

	initialInput := reviewEvidenceInput("Initial exact-head evidence", claim, []map[string]string{
		{"target_type": "deliverable", "target_id": deliverableItem.ID}, {"target_type": "gate", "target_id": gateItem.ID},
	})
	for index, boundary := range []string{"after-evidence", "after-evidence-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/evidence", initialInput,
			run.reviewHeaders(run.humanHeaders(), fmt.Sprintf("m2d-evidence-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	createdEvidence := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/evidence", initialInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-evidence-create", version))
	if err := expect(createdEvidence, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var initial evidence
	if err := json.Unmarshal(createdEvidence.Body, &initial); err != nil {
		return "", err
	}
	if len(initial.IntegrityDigest) != 64 || initial.ProducedBy != humanID || initial.ProducerKind != "human" {
		return "", fmt.Errorf("invalid evidence provenance or digest: %+v", initial)
	}
	version = etagVersion(createdEvidence)
	correctedInput := reviewEvidenceInput("Corrected exact-head evidence", claim, []map[string]string{
		{"target_type": "deliverable", "target_id": deliverableItem.ID}, {"target_type": "gate", "target_id": gateItem.ID},
	})
	correctedResponse := run.call(run.agent, http.MethodPost, "/api/v1/evidence/"+initial.ID+"/supersede", correctedInput,
		run.reviewHeaders(run.agentHeaders(), "m2d-evidence-correction", version))
	if err := expect(correctedResponse, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var corrected evidence
	if err := json.Unmarshal(correctedResponse.Body, &corrected); err != nil {
		return "", err
	}
	if corrected.SupersedesID == nil || *corrected.SupersedesID != initial.ID || corrected.ProducerKind != "agent" ||
		corrected.PrincipalID == nil || *corrected.PrincipalID != humanID {
		return "", fmt.Errorf("invalid evidence correction chain/provenance: %+v", corrected)
	}
	version = etagVersion(correctedResponse)
	staleBefore := run.counts(ctx)
	staleSubmit := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/submit",
		reviewSubmissionInput(initial.ID, "Stale evidence must be rejected"), run.reviewHeaders(run.humanHeaders(), "m2d-stale-submit", version))
	if err := expect(staleSubmit, http.StatusConflict, "invariant_violation"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != staleBefore {
		return "", fmt.Errorf("stale submission left residue: before=%+v after=%+v", staleBefore, after)
	}
	for index, boundary := range []string{"after-submission", "after-submission-event"} {
		if err := run.faultNoResidue(ctx, run.agent, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/submit",
			reviewSubmissionInput(corrected.ID, "Current exact-head evidence"),
			run.reviewHeaders(run.agentHeaders(), fmt.Sprintf("m2d-submit-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	submittedVersion, submitResponses := run.concurrentReviewMutation(
		[]*http.Client{run.human, run.agent}, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/submit",
		reviewSubmissionInput(corrected.ID, "Current exact-head evidence"),
		[]map[string]string{run.humanHeaders(), run.agentHeaders()}, "m2d-concurrent-submit", version, http.StatusOK)
	if submittedVersion == 0 {
		return "", fmt.Errorf("concurrent submission did not serialize: %+v", submitResponses)
	}
	var submittedResponse snapshot
	for _, response := range submitResponses {
		if response.Status == http.StatusOK {
			submittedResponse = response
		}
	}
	var submitted submissionResult
	if err := json.Unmarshal(submittedResponse.Body, &submitted); err != nil {
		return "", err
	}
	if submitted.Deliverable.State != "submitted" || !map[string]bool{humanID: true, agentID: true}[submitted.Submission.SubmittedBy] {
		return "", fmt.Errorf("invalid submission attribution/state: %+v", submitted)
	}
	version = submittedVersion

	failInput := reviewVerdictInput("fail", corrected.ID, true)
	selfBefore := run.counts(ctx)
	selfReview := run.call(run.agent, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", failInput,
		run.reviewHeaders(run.agentHeaders(), "m2d-self-review-deny", version))
	if err := expect(selfReview, http.StatusForbidden, "forbidden"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != selfBefore {
		return "", fmt.Errorf("effective-principal self-review deny left residue: before=%+v after=%+v", selfBefore, after)
	}
	for index, boundary := range []string{"after-verdict", "after-verdict-event", "after-finding", "after-finding-event"} {
		if err := run.faultNoResidue(ctx, run.reviewerAgent, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", failInput,
			run.reviewHeaders(run.reviewerAgentHeaders(), fmt.Sprintf("m2d-verdict-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	failedResponse := run.call(run.reviewerAgent, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", failInput,
		run.reviewHeaders(run.reviewerAgentHeaders(), "m2d-verdict-fail", version))
	if err := expect(failedResponse, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var failed verdictResult
	if err := json.Unmarshal(failedResponse.Body, &failed); err != nil {
		return "", err
	}
	if failed.Gate.State != "failed" || len(failed.Findings) != 1 || !failed.Findings[0].Blocking || failed.Verdict.ReviewerID != reviewerAgentID {
		return "", fmt.Errorf("invalid failed verdict/finding: %+v", failed)
	}
	version = etagVersion(failedResponse)
	findingItem := failed.Findings[0]
	duplicateBefore := run.counts(ctx)
	duplicateFailure := run.call(run.reviewerHuman, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", failInput,
		run.reviewHeaders(run.reviewerHumanHeaders(), "m2d-duplicate-failure", version))
	if err := expect(duplicateFailure, http.StatusConflict, "invariant_violation"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != duplicateBefore {
		return "", fmt.Errorf("duplicate failure verdict left residue: before=%+v after=%+v", duplicateBefore, after)
	}

	bounceInput := map[string]any{"finding_id": findingItem.ID, "rationale": "The exact-head evidence requires correction"}
	for index, boundary := range []string{"after-deliverable-review", "after-deliverable-review-event"} {
		if err := run.faultNoResidue(ctx, run.reviewerHuman, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/bounce", bounceInput,
			run.reviewHeaders(run.reviewerHumanHeaders(), fmt.Sprintf("m2d-bounce-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	bounced := run.call(run.reviewerHuman, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/bounce", bounceInput,
		run.reviewHeaders(run.reviewerHumanHeaders(), "m2d-bounce", version))
	if err := expect(bounced, http.StatusOK, ""); err != nil {
		return "", err
	}
	version = etagVersion(bounced)

	resolutionInput := reviewEvidenceInput("Finding resolution evidence", claim, []map[string]string{
		{"target_type": "deliverable", "target_id": deliverableItem.ID}, {"target_type": "gate", "target_id": gateItem.ID},
		{"target_type": "finding", "target_id": findingItem.ID},
	})
	resolutionResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/evidence", resolutionInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-resolution-evidence", version))
	if err := expect(resolutionResponse, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var resolution evidence
	if err := json.Unmarshal(resolutionResponse.Body, &resolution); err != nil {
		return "", err
	}
	version = etagVersion(resolutionResponse)
	incompleteBefore := run.counts(ctx)
	incompleteResubmit := run.call(run.agent, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/resubmit",
		reviewSubmissionInput(resolution.ID, "An open finding must still block resubmission"),
		run.reviewHeaders(run.agentHeaders(), "m2d-open-finding-resubmit-deny", version))
	if err := expect(incompleteResubmit, http.StatusConflict, "invariant_violation"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != incompleteBefore {
		return "", fmt.Errorf("open-finding resubmit deny left residue: before=%+v after=%+v", incompleteBefore, after)
	}
	dispositionInput := map[string]any{"evidence_ids": []string{resolution.ID}, "rationale": "The corrected evidence closes the actionable finding"}
	for index, boundary := range []string{"after-finding-action", "after-finding-action-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/findings/"+findingItem.ID+"/resolve", dispositionInput,
			run.reviewHeaders(run.humanHeaders(), fmt.Sprintf("m2d-resolve-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	resolvedVersion, responses := run.concurrentReviewMutation(
		[]*http.Client{run.human, run.agent}, http.MethodPost, "/api/v1/findings/"+findingItem.ID+"/resolve", dispositionInput,
		[]map[string]string{run.humanHeaders(), run.agentHeaders()}, "m2d-concurrent-resolve", version, http.StatusOK)
	if resolvedVersion == 0 {
		return "", fmt.Errorf("concurrent finding resolution did not serialize: %+v", responses)
	}
	version = resolvedVersion

	resubmittedResponse := run.call(run.agent, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/resubmit",
		reviewSubmissionInput(resolution.ID, "Resolution evidence is linked to the finding"), run.reviewHeaders(run.agentHeaders(), "m2d-resubmit", version))
	if err := expect(resubmittedResponse, http.StatusOK, ""); err != nil {
		return "", err
	}
	var resubmitted submissionResult
	if err := json.Unmarshal(resubmittedResponse.Body, &resubmitted); err != nil {
		return "", err
	}
	if resubmitted.Submission.Kind != "resubmit" || resubmitted.Deliverable.State != "submitted" {
		return "", fmt.Errorf("invalid resubmission: %+v", resubmitted)
	}
	version = etagVersion(resubmittedResponse)
	passedResponse := run.call(run.reviewerHuman, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts",
		reviewVerdictInput("pass", resolution.ID, false), run.reviewHeaders(run.reviewerHumanHeaders(), "m2d-verdict-pass", version))
	if err := expect(passedResponse, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var passed verdictResult
	if err := json.Unmarshal(passedResponse.Body, &passed); err != nil {
		return "", err
	}
	if passed.Gate.State != "passed" || passed.Verdict.SupersedesID == nil || *passed.Verdict.SupersedesID != failed.Verdict.ID {
		return "", fmt.Errorf("pass verdict did not supersede failure: %+v", passed)
	}
	version = etagVersion(passedResponse)
	reorderedBefore := run.counts(ctx)
	reorderedFailure := run.call(run.reviewerAgent, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", failInput,
		run.reviewHeaders(run.reviewerAgentHeaders(), "m2d-reordered-failure", version))
	if err := expect(reorderedFailure, http.StatusConflict, "invariant_violation"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != reorderedBefore {
		return "", fmt.Errorf("reordered failure verdict left residue: before=%+v after=%+v", reorderedBefore, after)
	}
	for index, boundary := range []string{"after-deliverable-review", "after-deliverable-review-event"} {
		if err := run.faultNoResidue(ctx, run.reviewerHuman, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/approve",
			map[string]any{"note": "All hard gates pass and findings are closed"},
			run.reviewHeaders(run.reviewerHumanHeaders(), fmt.Sprintf("m2d-approve-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	approvedVersion, approveResponses := run.concurrentReviewMutation(
		[]*http.Client{run.reviewerHuman, run.reviewerAgent}, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/approve",
		map[string]any{"note": "All hard gates pass and findings are closed"},
		[]map[string]string{run.reviewerHumanHeaders(), run.reviewerAgentHeaders()}, "m2d-concurrent-approve", version, http.StatusOK)
	if approvedVersion == 0 {
		return "", fmt.Errorf("concurrent approval did not serialize: %+v", approveResponses)
	}

	listedEvidence := run.call(run.agent, http.MethodGet, "/api/v1/projects/"+projectItem.ID+"/evidence", nil, run.agentHeaders())
	if err := expect(listedEvidence, http.StatusOK, ""); err != nil {
		return "", err
	}
	var evidenceHistory []evidence
	if err := json.Unmarshal(listedEvidence.Body, &evidenceHistory); err != nil || len(evidenceHistory) != 3 {
		return "", fmt.Errorf("evidence history mismatch: count=%d err=%v", len(evidenceHistory), err)
	}
	listedGates := run.call(run.agent, http.MethodGet, "/api/v1/deliverables/"+deliverableItem.ID+"/gates", nil, run.agentHeaders())
	listedVerdicts := run.call(run.agent, http.MethodGet, "/api/v1/gates/"+gateItem.ID+"/verdicts", nil, run.agentHeaders())
	listedFindings := run.call(run.agent, http.MethodGet, "/api/v1/deliverables/"+deliverableItem.ID+"/findings", nil, run.agentHeaders())
	if err := expect(listedGates, http.StatusOK, ""); err != nil {
		return "", err
	}
	if err := expect(listedVerdicts, http.StatusOK, ""); err != nil {
		return "", err
	}
	if err := expect(listedFindings, http.StatusOK, ""); err != nil {
		return "", err
	}
	var gateProjection []gate
	var verdictHistory []struct {
		ID           string  `json:"id"`
		Result       string  `json:"result"`
		SupersedesID *string `json:"supersedes_id"`
	}
	var findingProjection []finding
	if err := json.Unmarshal(listedGates.Body, &gateProjection); err != nil || len(gateProjection) != 1 || gateProjection[0].State != "passed" {
		return "", fmt.Errorf("gate projection mismatch: items=%+v err=%v", gateProjection, err)
	}
	if err := json.Unmarshal(listedVerdicts.Body, &verdictHistory); err != nil || len(verdictHistory) != 2 || verdictHistory[1].Result != "pass" ||
		verdictHistory[1].SupersedesID == nil || *verdictHistory[1].SupersedesID != verdictHistory[0].ID {
		return "", fmt.Errorf("verdict history mismatch: items=%+v err=%v", verdictHistory, err)
	}
	if err := json.Unmarshal(listedFindings.Body, &findingProjection); err != nil || len(findingProjection) != 1 || findingProjection[0].State != "resolved" {
		return "", fmt.Errorf("finding projection mismatch: items=%+v err=%v", findingProjection, err)
	}
	activityResponse := run.call(run.agent, http.MethodGet, "/api/v1/projects/"+projectItem.ID+"/activity", nil, run.agentHeaders())
	if err := expect(activityResponse, http.StatusOK, ""); err != nil {
		return "", err
	}
	var activity []app.Activity
	if err := json.Unmarshal(activityResponse.Body, &activity); err != nil {
		return "", err
	}
	activityKinds := make(map[string]bool, len(activity))
	for _, event := range activity {
		activityKinds[event.EventType] = true
	}
	for _, eventType := range []string{"evidence.created", "evidence.superseded", "gate.created", "gate.verdict_recorded", "finding.created",
		"finding.resolved", "deliverable.submitted", "deliverable.bounced", "deliverable.resubmitted", "deliverable.accepted"} {
		if !activityKinds[eventType] {
			return "", fmt.Errorf("public activity omitted M2D event family %s", eventType)
		}
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE evidence SET claim='rewritten' WHERE id=$1`, initial.ID); err == nil {
		return "", errors.New("database allowed immutable evidence update")
	}
	if _, err := run.db.ExecContext(ctx, `DELETE FROM evidence WHERE id=$1`, initial.ID); err == nil {
		return "", errors.New("database allowed immutable evidence delete")
	}
	run.write("m2d-evidence.json", map[string]any{
		"history_count": len(evidenceHistory), "digest": initial.IntegrityDigest, "supersession": []string{initial.ID, corrected.ID},
		"attribution": []string{"human", "agent-delegated-human"}, "stale_submission": "denied-no-residue", "database_mutation": "denied",
	})
	run.write("m2d-review-workflow.json", map[string]any{
		"deliverable_id": deliverableItem.ID, "states": []string{"ready", "submitted", "bounced", "submitted", "accepted"},
		"gate_states": []string{"pending", "failed", "passed"}, "verdict_supersession": []string{failed.Verdict.ID, passed.Verdict.ID},
		"finding_id": findingItem.ID, "finding_state": "resolved", "project_version": approvedVersion,
	})
	run.write("m2d-authority-atomicity.json", map[string]any{
		"effective_principal_self_review": "forbidden-no-residue", "human_agent_resolution_race": "one-winner-one-version-conflict",
		"human_agent_submission_race": "one-winner-one-version-conflict", "human_agent_approval_race": "one-winner-one-version-conflict",
		"duplicate_reordered_verdicts": "denied-no-residue", "idempotent_retry": "exact", "fault_boundaries": "all-rollback",
	})
	run.checks = append(run.checks,
		"immutable-attributable-evidence-and-linear-supersession",
		"submit-bounce-resolve-resubmit-approve-review-cycle",
		"immutable-verdict-history-with-later-pass",
		"effective-principal-separation-and-postgresql-atomicity",
		"public-human-agent-evidence-review-parity")
	return projectItem.ID, nil
}

func (run *runner) waiverReviewFlow(ctx context.Context) (string, error) {
	projectItem, promoted, _, err := run.parityFlow(ctx, run.human, "human", run.humanHeaders(), "m2d-waiver")
	if err != nil {
		return "", err
	}
	if err := run.grantReviewers(ctx, projectItem.ID); err != nil {
		return "", err
	}
	deliverableItem := promoted.Deliverables[0]
	version := projectItem.Version
	claim := "Waiver policy evidence is current"
	createGate := func(name string, hard bool, key string) (gate, error) {
		response := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/gates",
			reviewGateInput(name, hard, claim), run.reviewHeaders(run.humanHeaders(), key, version))
		if err := expect(response, http.StatusCreated, ""); err != nil {
			return gate{}, err
		}
		var item gate
		if err := json.Unmarshal(response.Body, &item); err != nil {
			return gate{}, err
		}
		version = etagVersion(response)
		return item, nil
	}
	hardGate, err := createGate("Non-waivable security gate", true, "m2d-hard-gate")
	if err != nil {
		return "", err
	}
	softGate, err := createGate("Human policy gate", false, "m2d-soft-gate")
	if err != nil {
		return "", err
	}
	evidenceInput := reviewEvidenceInput("Waiver decision evidence", claim, []map[string]string{
		{"target_type": "deliverable", "target_id": deliverableItem.ID}, {"target_type": "gate", "target_id": hardGate.ID},
		{"target_type": "gate", "target_id": softGate.ID},
	})
	evidenceResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/evidence", evidenceInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-waiver-evidence", version))
	if err := expect(evidenceResponse, http.StatusCreated, ""); err != nil {
		return "", err
	}
	var item evidence
	if err := json.Unmarshal(evidenceResponse.Body, &item); err != nil {
		return "", err
	}
	version = etagVersion(evidenceResponse)
	submit := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/submit",
		reviewSubmissionInput(item.ID, "Evidence satisfies hard and soft gate contracts"), run.reviewHeaders(run.humanHeaders(), "m2d-waiver-submit", version))
	if err := expect(submit, http.StatusOK, ""); err != nil {
		return "", err
	}
	version = etagVersion(submit)
	waiverInput := reviewWaiverInput(item.ID)
	hardBefore := run.counts(ctx)
	hardWaiver := run.call(run.human, http.MethodPost, "/api/v1/gates/"+hardGate.ID+"/waive", waiverInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-hard-waiver-deny", version))
	if err := expect(hardWaiver, http.StatusConflict, "invariant_violation"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != hardBefore {
		return "", fmt.Errorf("hard gate waiver deny left residue: before=%+v after=%+v", hardBefore, after)
	}
	hardPass := run.call(run.reviewerHuman, http.MethodPost, "/api/v1/gates/"+hardGate.ID+"/verdicts",
		reviewVerdictInput("pass", item.ID, false), run.reviewHeaders(run.reviewerHumanHeaders(), "m2d-hard-pass", version))
	if err := expect(hardPass, http.StatusCreated, ""); err != nil {
		return "", err
	}
	version = etagVersion(hardPass)
	agentBefore := run.counts(ctx)
	agentGateWaiver := run.call(run.agent, http.MethodPost, "/api/v1/gates/"+softGate.ID+"/waive", waiverInput,
		run.reviewHeaders(run.agentHeaders(), "m2d-agent-gate-waiver-deny", version))
	if err := expect(agentGateWaiver, http.StatusForbidden, "forbidden"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != agentBefore {
		return "", fmt.Errorf("agent soft-gate waiver deny left residue: before=%+v after=%+v", agentBefore, after)
	}
	for index, boundary := range []string{"after-decision", "after-waiver-decision-event", "after-gate-waiver-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/gates/"+softGate.ID+"/waive", waiverInput,
			run.reviewHeaders(run.humanHeaders(), fmt.Sprintf("m2d-gate-waiver-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	softWaiver := run.call(run.human, http.MethodPost, "/api/v1/gates/"+softGate.ID+"/waive", waiverInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-soft-waiver", version))
	if err := expect(softWaiver, http.StatusOK, ""); err != nil {
		return "", err
	}
	version = etagVersion(softWaiver)
	agentBefore = run.counts(ctx)
	agentDeliverableWaiver := run.call(run.agent, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/waive", waiverInput,
		run.reviewHeaders(run.agentHeaders(), "m2d-agent-deliverable-waiver-deny", version))
	if err := expect(agentDeliverableWaiver, http.StatusForbidden, "forbidden"); err != nil {
		return "", err
	}
	if after := run.counts(ctx); after != agentBefore {
		return "", fmt.Errorf("agent deliverable waiver deny left residue: before=%+v after=%+v", agentBefore, after)
	}
	for index, boundary := range []string{"after-decision", "after-waiver-decision-event", "after-deliverable-waiver-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/waive", waiverInput,
			run.reviewHeaders(run.humanHeaders(), fmt.Sprintf("m2d-deliverable-waiver-fault-%02d", index), version), boundary); err != nil {
			return "", err
		}
	}
	deliverableWaiver := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/waive", waiverInput,
		run.reviewHeaders(run.humanHeaders(), "m2d-deliverable-waiver", version))
	if err := expect(deliverableWaiver, http.StatusOK, ""); err != nil {
		return "", err
	}
	var storedGateState, storedDeliverableState string
	var waiverDecisions int
	if err := run.db.QueryRowContext(ctx, `SELECT
		(SELECT state::text FROM review_gates WHERE id=$1),(SELECT state::text FROM deliverables WHERE id=$2),
		(SELECT count(*) FROM decisions WHERE project_id=$3 AND kind='waiver')`, softGate.ID, deliverableItem.ID, projectItem.ID).
		Scan(&storedGateState, &storedDeliverableState, &waiverDecisions); err != nil {
		return "", err
	}
	if storedGateState != "waived" || storedDeliverableState != "waived" || waiverDecisions != 2 {
		return "", fmt.Errorf("waiver persistence mismatch: gate=%s deliverable=%s decisions=%d", storedGateState, storedDeliverableState, waiverDecisions)
	}
	run.write("m2d-waivers.json", map[string]any{
		"hard_gate": "never-waivable", "soft_gate": storedGateState, "deliverable": storedDeliverableState,
		"agent_policy_attempts": "forbidden-no-residue", "human_universal_decisions": waiverDecisions,
		"preconditions": []string{"hard-gates-passed", "no-blocking-findings", "current-linked-evidence", "rationale", "residual-risk"},
	})
	run.checks = append(run.checks, "hard-gate-nonwaiver-and-human-only-policy-decisions", "atomic-soft-gate-and-deliverable-waivers")
	return projectItem.ID, nil
}

func (run *runner) assertReviewLedger(ctx context.Context) error {
	required := []string{"evidence.created", "evidence.superseded", "gate.created", "gate.verdict_recorded", "finding.created",
		"finding.resolved", "deliverable.submitted", "deliverable.bounced", "deliverable.resubmitted", "deliverable.accepted", "gate.waived", "deliverable.waived"}
	for _, eventType := range required {
		var events, outbox int
		if err := run.db.QueryRowContext(ctx, `SELECT count(*),(SELECT count(*) FROM outbox_records record
			JOIN domain_events event ON event.event_id=record.event_id WHERE event.event_type=$1)
			FROM domain_events WHERE event_type=$1`, eventType).Scan(&events, &outbox); err != nil {
			return err
		}
		if events == 0 || events != outbox {
			return fmt.Errorf("review event/outbox mismatch for %s: events=%d outbox=%d", eventType, events, outbox)
		}
	}
	run.write("m2d-ledger-realtime.json", map[string]any{
		"required_event_families": required, "ledger_outbox": "one-to-one", "public_sse_resume_event": "gate.created",
		"replay_and_realtime": "verified-by-durability-gate",
	})
	run.checks = append(run.checks, "review-event-ledger-outbox-and-realtime-families")
	return nil
}

func (run *runner) concurrentReviewMutation(clients []*http.Client, method, path string, input any, auth []map[string]string,
	prefix string, version int64, successStatus int) (int64, []snapshot) {
	responses := make(chan snapshot, 2)
	var wait sync.WaitGroup
	for index := 0; index < 2; index++ {
		wait.Add(1)
		go func(index int) {
			defer wait.Done()
			responses <- run.call(clients[index], method, path, input,
				run.reviewHeaders(auth[index], fmt.Sprintf("%s-%02d", prefix, index), version))
		}(index)
	}
	wait.Wait()
	close(responses)
	results := make([]snapshot, 0, 2)
	for response := range responses {
		results = append(results, response)
	}
	winner, err := concurrentWinner(results, successStatus)
	if err != nil {
		return 0, results
	}
	return winner, results
}

func reviewEvidenceInput(title, claim string, supports []map[string]string) map[string]any {
	return map[string]any{"kind": "test-run", "title": title, "claim": claim, "source": "Workplane exact-head smoke",
		"content": "Deterministic PostgreSQL-backed smoke result", "uri": "evidence://workplane/m2d/exact-head",
		"metadata": map[string]string{"gate": "smoke", "track": "M2D"}, "supports": supports}
}

func reviewGateInput(name string, hard bool, claim string) map[string]any {
	return map[string]any{"name": name, "kind": "review", "hard": hard, "independence_required": true,
		"required_evidence": []map[string]string{{"kind": "test-run", "claim": claim}}}
}

func reviewSubmissionInput(evidenceID, note string) map[string]any {
	return map[string]any{"evidence_ids": []string{evidenceID}, "note": note}
}

func reviewVerdictInput(result, evidenceID string, withFinding bool) map[string]any {
	findings := []map[string]any{}
	if withFinding {
		findings = append(findings, map[string]any{"title": "Exact-head evidence correction required", "detail": "The submitted evidence must be corrected and rerun.", "blocking": true})
	}
	return map[string]any{"result": result, "evidence_ids": []string{evidenceID}, "findings": findings}
}

func reviewWaiverInput(evidenceID string) map[string]any {
	return map[string]any{"question": "Should policy permit this bounded waiver?", "choice": "Waive with recorded residual risk",
		"alternatives": []string{"Keep pending"}, "rationale": "The hard evidence contract passes and the remaining policy risk is bounded.",
		"evidence_ids": []string{evidenceID}, "consequences": []string{"Retain immutable audit history"},
		"residual_risk": "A human must revisit the policy judgment if the evidence changes."}
}

func (run *runner) reviewHeaders(auth map[string]string, key string, version int64) map[string]string {
	if len(key) < 16 {
		key += strings.Repeat("0", 16-len(key))
	}
	return merge(auth, map[string]string{"Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, version)})
}

func (run *runner) reviewerHumanHeaders() map[string]string {
	return map[string]string{"Origin": publicOrigin, "X-CSRF-Token": reviewerCSRF}
}

func (run *runner) reviewerAgentHeaders() map[string]string {
	return map[string]string{"Authorization": "Bearer " + reviewerAgentToken}
}

func m2dKeyedHash(value string) []byte {
	mac := hmac.New(sha256.New, []byte("local-only-key-material-32-bytes-minimum-change-me"))
	_, _ = mac.Write([]byte(value))
	return mac.Sum(nil)
}

func (run *runner) parityFlow(ctx context.Context, client *http.Client, actor string, auth map[string]string, prefix string) (project, promotion, []string, error) {
	createHeaders := clone(auth)
	createHeaders["Idempotency-Key"] = prefix + "-create-0000001"
	created := run.call(client, http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput(actor+" M2C parity"), createHeaders)
	if err := expect(created, http.StatusCreated, ""); err != nil {
		return project{}, promotion{}, nil, err
	}
	var item project
	if err := json.Unmarshal(created.Body, &item); err != nil {
		return project{}, promotion{}, nil, err
	}
	if item.Version != 1 || item.State != "proposed" {
		return project{}, promotion{}, nil, fmt.Errorf("invalid created project: %+v", item)
	}
	activationBefore := run.counts(ctx)
	blockedActivation := run.call(client, http.MethodPost, "/api/v1/projects/"+item.ID+"/activate",
		map[string]any{"reason": "Forecast is intentionally absent"}, merge(auth, map[string]string{"Idempotency-Key": prefix + "-activate-no-forecast", "If-Match": `"1"`}))
	if err := expect(blockedActivation, http.StatusConflict, "invariant_violation"); err != nil {
		return project{}, promotion{}, nil, err
	}
	if run.counts(ctx) != activationBefore {
		return project{}, promotion{}, nil, errors.New("missing-forecast activation left residue")
	}

	bad := forecastInput(48, 24, 1)
	invalid := run.call(client, http.MethodPost, "/api/v1/projects/"+item.ID+"/forecasts", bad, merge(auth, map[string]string{"Idempotency-Key": prefix + "-bad-forecast-01", "If-Match": `"1"`}))
	if err := expect(invalid, http.StatusBadRequest, "invalid_request"); err != nil {
		return project{}, promotion{}, nil, err
	}
	for index, boundary := range []string{"after-forecast", "after-forecast-created-event"} {
		if err := run.faultNoResidue(ctx, client, http.MethodPost, "/api/v1/projects/"+item.ID+"/forecasts", forecastInput(24, 48, 1),
			merge(auth, map[string]string{"Idempotency-Key": fmt.Sprintf("%s-forecast-fault-%02d", prefix, index), "If-Match": `"1"`}), boundary); err != nil {
			return project{}, promotion{}, nil, err
		}
	}
	committed := run.call(client, http.MethodPost, "/api/v1/projects/"+item.ID+"/forecasts", forecastInput(24, 48, 1), merge(auth, map[string]string{"Idempotency-Key": prefix + "-forecast-commit", "If-Match": `"1"`}))
	if err := expect(committed, http.StatusCreated, ""); err != nil {
		return project{}, promotion{}, nil, err
	}
	retry := run.call(client, http.MethodPost, "/api/v1/projects/"+item.ID+"/forecasts", forecastInput(24, 48, 1), merge(auth, map[string]string{"Idempotency-Key": prefix + "-forecast-commit", "If-Match": `"1"`}))
	if !bytes.Equal(committed.Body, retry.Body) || committed.Headers["X-Request-ID"] != retry.Headers["X-Request-ID"] {
		return project{}, promotion{}, nil, errors.New("forecast retry was not exact")
	}
	version := etagVersion(committed)
	activated := run.call(client, http.MethodPost, "/api/v1/projects/"+item.ID+"/activate", map[string]any{"reason": "Complete exploration contract"}, merge(auth, map[string]string{"Idempotency-Key": prefix + "-activate-0001", "If-Match": fmt.Sprintf(`"%d"`, version)}))
	if err := expect(activated, http.StatusOK, ""); err != nil {
		return project{}, promotion{}, nil, err
	}
	version = etagVersion(activated)
	for index, boundary := range []string{"after-decision", "after-decision-event", "after-deliverable", "after-deliverable-event", "after-project", "after-promotion-event"} {
		if err := run.faultNoResidue(ctx, client, http.MethodPost, "/api/v1/projects/"+item.ID+"/promote", promotionInput(),
			merge(auth, map[string]string{"Idempotency-Key": fmt.Sprintf("%s-promote-fault-%02d", prefix, index), "If-Match": fmt.Sprintf(`"%d"`, version)}), boundary); err != nil {
			return project{}, promotion{}, nil, err
		}
	}
	promoted := run.call(client, http.MethodPost, "/api/v1/projects/"+item.ID+"/promote", promotionInput(), merge(auth, map[string]string{"Idempotency-Key": prefix + "-promote-00001", "If-Match": fmt.Sprintf(`"%d"`, version)}))
	if err := expect(promoted, http.StatusOK, ""); err != nil {
		return project{}, promotion{}, nil, err
	}
	var result promotion
	if err := json.Unmarshal(promoted.Body, &result); err != nil {
		return project{}, promotion{}, nil, err
	}
	if result.Project.Mode != "exploitation" || result.Project.State != "active" || len(result.Deliverables) != 1 {
		return project{}, promotion{}, nil, fmt.Errorf("invalid promotion: %+v", result)
	}
	var grouped int
	if err := run.db.QueryRowContext(ctx, `SELECT count(DISTINCT command_id) FROM domain_events WHERE aggregate_id=$1 AND aggregate_version BETWEEN 4 AND 6`, item.ID).Scan(&grouped); err != nil || grouped != 1 {
		return project{}, promotion{}, nil, fmt.Errorf("promotion command group count=%d err=%v", grouped, err)
	}
	rows, err := run.db.QueryContext(ctx, `SELECT event_type FROM domain_events WHERE aggregate_id=$1 ORDER BY aggregate_version`, item.ID)
	if err != nil {
		return project{}, promotion{}, nil, err
	}
	defer rows.Close()
	types := []string{}
	for rows.Next() {
		var value string
		if err := rows.Scan(&value); err != nil {
			return project{}, promotion{}, nil, err
		}
		types = append(types, value)
	}
	return result.Project, result, types, nil
}

func (run *runner) idempotentDecisionReauthorizationExpectedDeny(ctx context.Context) error {
	created := run.call(run.agent, http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects",
		projectInput("Decision reauthorization fixture"), merge(run.agentHeaders(), map[string]string{
			"Idempotency-Key": "m2c-reauthorization-project-01",
		}))
	if err := expect(created, http.StatusCreated, ""); err != nil {
		return fmt.Errorf("decision reauthorization project: %w", err)
	}
	var item project
	if err := json.Unmarshal(created.Body, &item); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `INSERT INTO project_memberships (project_id,principal_id,role,created_at)
		VALUES ($1,$2,'owner',CURRENT_TIMESTAMP)
		ON CONFLICT (project_id,principal_id) DO UPDATE SET role='owner'`, item.ID, humanID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='member'
		WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}

	input := decisionInput()
	headers := run.versionedHuman("m2c-demoted-decision-retry-01", item.Version)
	first := run.call(run.human, http.MethodPost, "/api/v1/projects/"+item.ID+"/decisions", input, headers)
	if err := expect(first, http.StatusCreated, ""); err != nil {
		return fmt.Errorf("authorized decision before demotion: %w", err)
	}
	var recorded struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(first.Body, &recorded); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE project_memberships SET role='observer'
		WHERE project_id=$1 AND principal_id=$2`, item.ID, humanID); err != nil {
		return err
	}

	before := run.counts(ctx)
	retry := run.call(run.human, http.MethodPost, "/api/v1/projects/"+item.ID+"/decisions", input, headers)
	if err := expect(retry, http.StatusForbidden, "forbidden"); err != nil {
		return fmt.Errorf("demoted owner idempotent decision retry expected deny: %w", err)
	}
	if recorded.ID == "" || bytes.Contains(retry.Body, []byte(recorded.ID)) || bytes.Equal(retry.Body, first.Body) {
		return fmt.Errorf("demoted owner retry disclosed the stored decision response: %s", retry.Body)
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("demoted owner retry left residue: before=%+v after=%+v", before, got)
	}
	var decisions, events, outbox, idempotency int
	if err := run.db.QueryRowContext(ctx, `SELECT
		(SELECT count(*) FROM decisions WHERE project_id=$1),
		(SELECT count(*) FROM domain_events WHERE aggregate_id=$1),
		(SELECT count(*) FROM outbox_records record JOIN domain_events event ON event.event_id=record.event_id WHERE event.aggregate_id=$1),
		(SELECT count(*) FROM idempotency_results WHERE actor_id=$2 AND operation_id='recordDecision' AND idempotency_key=$3)`,
		item.ID, humanID, "m2c-demoted-decision-retry-01").Scan(&decisions, &events, &outbox, &idempotency); err != nil {
		return err
	}
	if decisions != 1 || events != 2 || outbox != 2 || idempotency != 1 {
		return fmt.Errorf("demoted owner retry changed accepted residue: decisions=%d events=%d outbox=%d idempotency=%d",
			decisions, events, outbox, idempotency)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='owner'
		WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	run.write("m2c-idempotent-reauthorization.json", map[string]any{
		"actor_relation": "non-creator-project-owner", "role_change": "owner-to-observer",
		"exact_retry":     "forbidden-no-disclosure-no-additional-residue",
		"accepted_counts": map[string]int{"decisions": decisions, "events": events, "outbox": outbox, "idempotency": idempotency},
	})
	run.checks = append(run.checks, "current-project-role-authorized-before-idempotency-replay")
	return nil
}

func (run *runner) faultNoResidue(ctx context.Context, client *http.Client, method, path string, input any, headers map[string]string, boundary string) error {
	before := run.counts(ctx)
	faultHeaders := clone(headers)
	faultHeaders["X-Workplane-Fault"] = boundary
	response := run.call(client, method, path, input, faultHeaders)
	if err := expect(response, http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return fmt.Errorf("fault %s: %w", boundary, err)
	}
	if after := run.counts(ctx); after != before {
		return fmt.Errorf("fault %s left residue: before=%+v after=%+v", boundary, before, after)
	}
	return nil
}

func concurrentWinner(responses []snapshot, successStatus int) (int64, error) {
	statuses := make([]int, 0, len(responses))
	var version int64
	for _, response := range responses {
		statuses = append(statuses, response.Status)
		switch response.Status {
		case successStatus:
			version = etagVersion(response)
		case http.StatusConflict:
			if err := expect(response, http.StatusConflict, "version_conflict"); err != nil {
				return 0, err
			}
		}
	}
	sort.Ints(statuses)
	if !reflect.DeepEqual(statuses, []int{successStatus, http.StatusConflict}) || version == 0 {
		return 0, fmt.Errorf("concurrent mutation statuses=%v", statuses)
	}
	return version, nil
}

func (run *runner) concurrentHold(projectID string, version int64) (int64, error) {
	responses := make(chan snapshot, 2)
	var wait sync.WaitGroup
	for index := 0; index < 2; index++ {
		wait.Add(1)
		go func(index int) {
			defer wait.Done()
			responses <- run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/hold",
				map[string]any{"reason": fmt.Sprintf("Concurrent hold %d", index)}, run.versionedHuman(fmt.Sprintf("m2c-concurrent-hold-%02d", index), version))
		}(index)
	}
	wait.Wait()
	close(responses)
	results := make([]snapshot, 0, 2)
	for response := range responses {
		results = append(results, response)
	}
	return concurrentWinner(results, http.StatusOK)
}

func (run *runner) concurrentDeliverableRevision(deliverableID string, version int64) (int64, error) {
	responses := make(chan snapshot, 2)
	var wait sync.WaitGroup
	for index := 0; index < 2; index++ {
		wait.Add(1)
		go func(index int) {
			defer wait.Done()
			responses <- run.call(run.human, http.MethodPatch, "/api/v1/deliverables/"+deliverableID,
				deliverableInput(fmt.Sprintf("Concurrent M2C revision %d", index)), run.versionedHuman(fmt.Sprintf("m2c-concurrent-revise-%02d", index), version))
		}(index)
	}
	wait.Wait()
	close(responses)
	results := make([]snapshot, 0, 2)
	for response := range responses {
		results = append(results, response)
	}
	return concurrentWinner(results, http.StatusOK)
}

func (run *runner) publicDeliverableProjectionMatrix(
	ctx context.Context,
	projectID string,
	first, second deliverable,
	version int64,
) (int64, deliverable, error) {
	firstExpected := deliverable{
		ID: first.ID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Cross-actor M2C revision", Description: "Observable planning-contract outcome",
		Required: true, Weight: 1000, State: "ready",
		AcceptanceCriteria: []string{"Exact-head smoke evidence passes"}, Version: 4, CreatedBy: humanID,
	}
	if err := assertDeliverableProjection("cross-actor revised write", first, firstExpected); err != nil {
		return 0, deliverable{}, err
	}

	secondInput := deliverableInput("Revised optional public projection")
	secondInput["required"] = false
	secondInput["acceptance_criteria"] = []string{"Public get and list preserve the revision"}
	revisedSecondResponse := run.call(run.human, http.MethodPatch, "/api/v1/deliverables/"+second.ID, secondInput,
		run.versionedHuman("m2c-public-deliverable-revise-01", version))
	if err := expect(revisedSecondResponse, http.StatusOK, ""); err != nil {
		return 0, deliverable{}, fmt.Errorf("revise second public deliverable fixture: %w", err)
	}
	var revisedSecond deliverable
	if err := json.Unmarshal(revisedSecondResponse.Body, &revisedSecond); err != nil {
		return 0, deliverable{}, err
	}
	version = etagVersion(revisedSecondResponse)
	secondExpected := deliverable{
		ID: second.ID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Revised optional public projection", Description: "Observable planning-contract outcome",
		Required: false, Weight: 1000, State: "ready",
		AcceptanceCriteria: []string{"Public get and list preserve the revision"}, Version: 2, CreatedBy: humanID,
	}
	if err := assertDeliverableProjection("optional revised write", revisedSecond, secondExpected); err != nil {
		return 0, deliverable{}, err
	}

	beforeReads := run.counts(ctx)
	for _, expected := range []deliverable{firstExpected, secondExpected} {
		response := run.call(run.human, http.MethodGet, "/api/v1/deliverables/"+expected.ID, nil, nil)
		if err := expect(response, http.StatusOK, ""); err != nil {
			return 0, deliverable{}, fmt.Errorf("public get deliverable %s: %w", expected.ID, err)
		}
		var actual deliverable
		if err := json.Unmarshal(response.Body, &actual); err != nil {
			return 0, deliverable{}, err
		}
		if err := assertDeliverableProjection("public get deliverable", actual, expected); err != nil {
			return 0, deliverable{}, err
		}
	}
	listedResponse := run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectID+"/deliverables", nil, nil)
	if err := expect(listedResponse, http.StatusOK, ""); err != nil {
		return 0, deliverable{}, fmt.Errorf("public list deliverables: %w", err)
	}
	var listed []deliverable
	if err := json.Unmarshal(listedResponse.Body, &listed); err != nil {
		return 0, deliverable{}, err
	}
	if len(listed) != 2 {
		return 0, deliverable{}, fmt.Errorf("public deliverable list length=%d want=2: %+v", len(listed), listed)
	}
	byID := make(map[string]deliverable, len(listed))
	for _, item := range listed {
		byID[item.ID] = item
	}
	for _, expected := range []deliverable{firstExpected, secondExpected} {
		actual, ok := byID[expected.ID]
		if !ok {
			return 0, deliverable{}, fmt.Errorf("public deliverable list omitted stable id %s", expected.ID)
		}
		if err := assertDeliverableProjection("public list deliverable", actual, expected); err != nil {
			return 0, deliverable{}, err
		}
	}

	if err := run.privateProjectReadExpectedDenies(ctx, projectID, []readDenyCase{
		{Label: "public get deliverable private nonmember", Path: "/api/v1/deliverables/" + firstExpected.ID},
		{Label: "public list deliverables private nonmember", Path: "/api/v1/projects/" + projectID + "/deliverables"},
	}); err != nil {
		return 0, deliverable{}, err
	}
	if afterReads := run.counts(ctx); afterReads != beforeReads {
		return 0, deliverable{}, fmt.Errorf("public deliverable reads/denies left residue: before=%+v after=%+v", beforeReads, afterReads)
	}
	run.checks = append(run.checks, "public-deliverable-get-list-exact-projection-and-authority")
	return version, secondExpected, nil
}

func assertDeliverableProjection(label string, actual, expected deliverable) error {
	if actual.ID != expected.ID || actual.OrganizationID != expected.OrganizationID || actual.ProjectID != expected.ProjectID ||
		actual.Title != expected.Title || actual.Description != expected.Description || actual.Required != expected.Required ||
		actual.Weight != expected.Weight || actual.State != expected.State || !reflect.DeepEqual(actual.AcceptanceCriteria, expected.AcceptanceCriteria) ||
		actual.Version != expected.Version || actual.CreatedBy != expected.CreatedBy {
		return fmt.Errorf("%s mismatch: got=%+v want=%+v", label, actual, expected)
	}
	return nil
}

func (run *runner) crossDeliverableRevisionIdempotency(ctx context.Context, firstID, secondID string, version int64) (int64, error) {
	const key = "m2c-cross-deliverable-revise-target-01"
	input := deliverableInput("Leaf-target-bound M2C revision")
	headers := run.versionedHuman(key, version)
	first := run.call(run.human, http.MethodPatch, "/api/v1/deliverables/"+firstID, input, headers)
	if err := expect(first, http.StatusOK, ""); err != nil {
		return 0, fmt.Errorf("first leaf-bound revision: %w", err)
	}
	var revised deliverable
	if err := json.Unmarshal(first.Body, &revised); err != nil {
		return 0, err
	}
	if revised.ID != firstID {
		return 0, fmt.Errorf("first leaf-bound revision targeted %s, want %s", revised.ID, firstID)
	}
	var beforeTitle string
	var beforeVersion int64
	if err := run.db.QueryRowContext(ctx, `SELECT title,version FROM deliverables WHERE id=$1`, secondID).Scan(&beforeTitle, &beforeVersion); err != nil {
		return 0, err
	}
	afterFirst := run.counts(ctx)
	conflict := run.call(run.human, http.MethodPatch, "/api/v1/deliverables/"+secondID, input, headers)
	if err := expect(conflict, http.StatusConflict, "idempotency_conflict"); err != nil {
		return 0, fmt.Errorf("cross-deliverable revision reuse: %w", err)
	}
	if bytes.Contains(conflict.Body, []byte(firstID)) || bytes.Contains(conflict.Body, first.Body) {
		return 0, fmt.Errorf("cross-deliverable revision conflict disclosed the first response: %s", conflict.Body)
	}
	if got := run.counts(ctx); got != afterFirst {
		return 0, fmt.Errorf("cross-deliverable revision conflict wrote rows: before=%+v after=%+v", afterFirst, got)
	}
	var afterTitle string
	var afterVersion int64
	if err := run.db.QueryRowContext(ctx, `SELECT title,version FROM deliverables WHERE id=$1`, secondID).Scan(&afterTitle, &afterVersion); err != nil {
		return 0, err
	}
	if afterTitle != beforeTitle || afterVersion != beforeVersion {
		return 0, fmt.Errorf("cross-deliverable revision conflict mutated second target: title=%q/%q version=%d/%d", beforeTitle, afterTitle, beforeVersion, afterVersion)
	}
	return etagVersion(first), nil
}

func (run *runner) crossDeliverableReforecastIdempotency(ctx context.Context, firstID, secondID string, version int64) (int64, error) {
	const key = "m2c-cross-deliverable-reforecast-target-01"
	input := forecastInput(24, 48, -1)
	headers := run.versionedHuman(key, version)
	first := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+firstID+"/forecasts", input, headers)
	if err := expect(first, http.StatusCreated, ""); err != nil {
		return 0, fmt.Errorf("first leaf-bound reforecast: %w", err)
	}
	var created forecast
	if err := json.Unmarshal(first.Body, &created); err != nil {
		return 0, err
	}
	if created.DeliverableID == nil || *created.DeliverableID != firstID {
		return 0, fmt.Errorf("first leaf-bound reforecast targeted %+v, want %s", created.DeliverableID, firstID)
	}
	var beforeForecasts, beforeHeads int
	if err := run.db.QueryRowContext(ctx, `SELECT
		(SELECT count(*) FROM forecasts WHERE deliverable_id=$1),
		(SELECT count(*) FROM forecast_heads WHERE deliverable_id=$1)`, secondID).Scan(&beforeForecasts, &beforeHeads); err != nil {
		return 0, err
	}
	afterFirst := run.counts(ctx)
	conflict := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+secondID+"/forecasts", input, headers)
	if err := expect(conflict, http.StatusConflict, "idempotency_conflict"); err != nil {
		return 0, fmt.Errorf("cross-deliverable reforecast reuse: %w", err)
	}
	if bytes.Contains(conflict.Body, []byte(firstID)) || bytes.Contains(conflict.Body, []byte(created.ID)) || bytes.Contains(conflict.Body, first.Body) {
		return 0, fmt.Errorf("cross-deliverable reforecast conflict disclosed the first response: %s", conflict.Body)
	}
	if got := run.counts(ctx); got != afterFirst {
		return 0, fmt.Errorf("cross-deliverable reforecast conflict wrote rows: before=%+v after=%+v", afterFirst, got)
	}
	var afterForecasts, afterHeads int
	if err := run.db.QueryRowContext(ctx, `SELECT
		(SELECT count(*) FROM forecasts WHERE deliverable_id=$1),
		(SELECT count(*) FROM forecast_heads WHERE deliverable_id=$1)`, secondID).Scan(&afterForecasts, &afterHeads); err != nil {
		return 0, err
	}
	if afterForecasts != beforeForecasts || afterHeads != beforeHeads {
		return 0, fmt.Errorf("cross-deliverable reforecast conflict mutated second target: forecasts=%d/%d heads=%d/%d", beforeForecasts, afterForecasts, beforeHeads, afterHeads)
	}
	return etagVersion(first), nil
}

func (run *runner) terminalDeliverableImmutability(ctx context.Context, projectID string) error {
	index := 0
	for _, state := range []string{"accepted", "waived", "cancelled"} {
		for _, operation := range []string{"update", "delete"} {
			index++
			tx, err := run.db.BeginTx(ctx, nil)
			if err != nil {
				return err
			}
			id := fmt.Sprintf("00000000-0000-4000-9000-%012d", index)
			_, err = tx.ExecContext(ctx, `INSERT INTO deliverables
				(id,organization_id,project_id,title,description,required,weight,state,acceptance_criteria,version,created_by,created_at,updated_at)
				VALUES ($1,$2,$3,$4,'Terminal immutability fixture',true,1,$5::deliverable_state,
				'["PostgreSQL rejects mutation"]'::jsonb,1,$6,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)`,
				id, organizationID, projectID, "Terminal "+state+" "+operation, state, humanID)
			if err != nil {
				_ = tx.Rollback()
				return err
			}
			switch operation {
			case "update":
				_, err = tx.ExecContext(ctx, `UPDATE deliverables SET title=title || ' changed' WHERE id=$1`, id)
			case "delete":
				_, err = tx.ExecContext(ctx, `DELETE FROM deliverables WHERE id=$1`, id)
			}
			_ = tx.Rollback()
			if err == nil || !strings.Contains(err.Error(), "immutable state") {
				return fmt.Errorf("PostgreSQL allowed %s of %s deliverable: %v", operation, state, err)
			}
		}
	}
	return nil
}

func (run *runner) concurrentForecastConflict(projectID string, version int64) error {
	type result struct{ response snapshot }
	responses := make(chan result, 2)
	var wait sync.WaitGroup
	for index := 0; index < 2; index++ {
		wait.Add(1)
		go func(index int) {
			defer wait.Done()
			headers := run.versionedHuman(fmt.Sprintf("m2c-concurrent-forecast-%02d", index), version)
			responses <- result{run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/forecasts", forecastInput(72+index, 96+index, 1), headers)}
		}(index)
	}
	wait.Wait()
	close(responses)
	statuses := []int{}
	for value := range responses {
		statuses = append(statuses, value.response.Status)
	}
	sort.Ints(statuses)
	if !reflect.DeepEqual(statuses, []int{http.StatusCreated, http.StatusConflict}) {
		return fmt.Errorf("concurrent forecast statuses=%v", statuses)
	}
	return nil
}

func (run *runner) publicProjectForecastProjectionMatrix(ctx context.Context) (string, error) {
	createdProjectResponse := run.call(run.human, http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects",
		projectInput("Public project forecast history fixture"), merge(run.humanHeaders(), map[string]string{
			"Idempotency-Key": "m2c-public-project-history-create-01",
		}))
	if err := expect(createdProjectResponse, http.StatusCreated, ""); err != nil {
		return "", fmt.Errorf("create public project forecast fixture: %w", err)
	}
	var historyProject project
	if err := json.Unmarshal(createdProjectResponse.Body, &historyProject); err != nil {
		return "", err
	}
	if historyProject.ID == "" || historyProject.Version != 1 {
		return "", fmt.Errorf("invalid public project forecast fixture: %+v", historyProject)
	}

	base := time.Now().UTC().Truncate(time.Second)
	firstInput := map[string]any{
		"p50_at": base.Add(24 * time.Hour).Format(time.RFC3339), "p90_at": base.Add(48 * time.Hour).Format(time.RFC3339),
		"review_after": base.Add(2 * time.Hour).Format(time.RFC3339), "basis": "Initial public project forecast",
		"assumptions": []string{"The projection remains project-scoped"}, "reason_codes": []string{"new-evidence"},
		"impact": "Establish immutable history",
	}
	firstResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+historyProject.ID+"/forecasts", firstInput,
		run.versionedHuman("m2c-public-project-history-first-01", historyProject.Version))
	if err := expect(firstResponse, http.StatusCreated, ""); err != nil {
		return "", fmt.Errorf("create initial public project forecast: %w", err)
	}
	var first forecast
	if err := json.Unmarshal(firstResponse.Body, &first); err != nil {
		return "", err
	}
	firstExpected := forecast{
		ID: first.ID, OrganizationID: organizationID, ProjectID: historyProject.ID, Scope: "project",
		P50At: projectionTime(base.Add(24 * time.Hour)), P90At: projectionTime(base.Add(48 * time.Hour)), ReviewAfter: projectionTime(base.Add(2 * time.Hour)),
		Basis: firstInput["basis"].(string), Assumptions: firstInput["assumptions"].([]string),
		ReasonCodes: firstInput["reason_codes"].([]string), Impact: firstInput["impact"].(string),
		CreatedBy: humanID, Current: true, Stale: false, AttentionOnly: true,
	}
	if first.ID == "" {
		return "", errors.New("initial public project forecast omitted stable id")
	}
	if err := assertForecastProjection("initial project forecast write", first, firstExpected); err != nil {
		return "", err
	}

	secondInput := map[string]any{
		"p50_at": base.Add(36 * time.Hour).Format(time.RFC3339), "p90_at": base.Add(72 * time.Hour).Format(time.RFC3339),
		"review_after": base.Add(-time.Hour).Format(time.RFC3339), "basis": "Superseding public project forecast",
		"assumptions": []string{"The new evidence remains bounded"}, "reason_codes": []string{"estimate-correction"},
		"impact": "Make stale attention explicit",
	}
	secondResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+historyProject.ID+"/forecasts", secondInput,
		run.versionedHuman("m2c-public-project-history-second-01", etagVersion(firstResponse)))
	if err := expect(secondResponse, http.StatusCreated, ""); err != nil {
		return "", fmt.Errorf("create superseding public project forecast: %w", err)
	}
	var second forecast
	if err := json.Unmarshal(secondResponse.Body, &second); err != nil {
		return "", err
	}
	secondExpected := forecast{
		ID: second.ID, OrganizationID: organizationID, ProjectID: historyProject.ID, Scope: "project",
		P50At: projectionTime(base.Add(36 * time.Hour)), P90At: projectionTime(base.Add(72 * time.Hour)), ReviewAfter: projectionTime(base.Add(-time.Hour)),
		Basis: secondInput["basis"].(string), Assumptions: secondInput["assumptions"].([]string),
		ReasonCodes: secondInput["reason_codes"].([]string), Impact: secondInput["impact"].(string), SupersedesID: &first.ID,
		CreatedBy: humanID, Current: true, Stale: true, AttentionOnly: true,
	}
	if second.ID == "" || second.ID == first.ID {
		return "", fmt.Errorf("superseding project forecast identity is not stable/distinct: first=%s second=%s", first.ID, second.ID)
	}
	if err := assertForecastProjection("superseding project forecast write", second, secondExpected); err != nil {
		return "", err
	}

	beforeReads := run.counts(ctx)
	historyResponse := run.call(run.human, http.MethodGet, "/api/v1/projects/"+historyProject.ID+"/forecasts", nil, nil)
	if err := expect(historyResponse, http.StatusOK, ""); err != nil {
		return "", fmt.Errorf("public project forecast history: %w", err)
	}
	var history []forecast
	if err := json.Unmarshal(historyResponse.Body, &history); err != nil {
		return "", err
	}
	if len(history) != 2 {
		return "", fmt.Errorf("public project forecast history length=%d want=2: %+v", len(history), history)
	}
	byID := make(map[string]forecast, len(history))
	current := 0
	for _, item := range history {
		byID[item.ID] = item
		if item.Current {
			current++
		}
	}
	firstExpected.Current = false
	firstExpected.Stale = false
	if err := assertForecastProjection("initial immutable project forecast history", byID[first.ID], firstExpected); err != nil {
		return "", err
	}
	if err := assertForecastProjection("superseding current project forecast history", byID[second.ID], secondExpected); err != nil {
		return "", err
	}
	if current != 1 {
		return "", fmt.Errorf("public project forecast history current rows=%d want=1", current)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE forecasts SET impact='tampered project history' WHERE id=$1`, first.ID); err == nil || !strings.Contains(err.Error(), "append-only") {
		return "", fmt.Errorf("public project forecast history was not immutable: %v", err)
	}

	if err := run.privateProjectReadExpectedDenies(ctx, historyProject.ID, []readDenyCase{
		{Label: "public project forecast history private nonmember", Path: "/api/v1/projects/" + historyProject.ID + "/forecasts"},
	}); err != nil {
		return "", err
	}
	if afterReads := run.counts(ctx); afterReads != beforeReads {
		return "", fmt.Errorf("public project forecast reads/denies left residue: before=%+v after=%+v", beforeReads, afterReads)
	}
	run.checks = append(run.checks, "public-project-forecast-history-exact-projection-and-authority")
	return historyProject.ID, nil
}

func (run *runner) privateProjectReadExpectedDenies(ctx context.Context, projectID string, cases []readDenyCase) (err error) {
	if len(cases) == 0 {
		return errors.New("private project read expected-deny fixture has no routes")
	}
	var originalCreator, originalVisibility, originalOrganizationRole string
	var originalProjectRole sql.NullString
	if err := run.db.QueryRowContext(ctx, `SELECT project.created_by::text,project.visibility::text,
		(SELECT role::text FROM organization_memberships WHERE organization_id=project.organization_id AND principal_id=$2),
		(SELECT role FROM project_memberships WHERE project_id=project.id AND principal_id=$2)
		FROM projects project WHERE project.id=$1`, projectID, humanID).
		Scan(&originalCreator, &originalVisibility, &originalOrganizationRole, &originalProjectRole); err != nil {
		return err
	}
	if originalCreator != humanID || originalOrganizationRole != "owner" || !originalProjectRole.Valid || originalProjectRole.String != "owner" {
		return fmt.Errorf("private read deny fixture has unexpected authority: project=%s creator=%s visibility=%s organization_role=%s project_role=%+v",
			projectID, originalCreator, originalVisibility, originalOrganizationRole, originalProjectRole)
	}

	restore := func() error {
		tx, err := run.db.BeginTx(ctx, nil)
		if err != nil {
			return err
		}
		defer tx.Rollback()
		if _, err := tx.ExecContext(ctx, `UPDATE projects SET created_by=$2,visibility=$3::project_visibility WHERE id=$1`,
			projectID, originalCreator, originalVisibility); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO project_memberships (project_id,principal_id,role,created_at)
			VALUES ($1,$2,$3,CURRENT_TIMESTAMP)
			ON CONFLICT (project_id,principal_id) DO UPDATE SET role=EXCLUDED.role`,
			projectID, humanID, originalProjectRole.String); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `UPDATE organization_memberships SET role=$3::organization_role
			WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID, originalOrganizationRole); err != nil {
			return err
		}
		return tx.Commit()
	}
	restorePending := true
	defer func() {
		if restorePending {
			if restoreErr := restore(); err == nil && restoreErr != nil {
				err = restoreErr
			}
		}
	}()

	tx, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `UPDATE organization_memberships SET role='member'
		WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx, `DELETE FROM project_memberships WHERE project_id=$1 AND principal_id=$2`, projectID, humanID); err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx, `UPDATE projects SET created_by=$2,visibility='private' WHERE id=$1`, projectID, agentID); err != nil {
		return err
	}
	if err := tx.Commit(); err != nil {
		return err
	}

	before := run.counts(ctx)
	for _, test := range cases {
		response := run.call(run.human, http.MethodGet, test.Path, nil, nil)
		if err := expect(response, http.StatusNotFound, "not_found"); err != nil {
			return fmt.Errorf("%s expected project authorization deny after successful session authentication: %w", test.Label, err)
		}
	}
	if after := run.counts(ctx); after != before {
		return fmt.Errorf("private project read expected denies left residue: before=%+v after=%+v", before, after)
	}
	if err := restore(); err != nil {
		return err
	}
	restorePending = false

	var restoredCreator, restoredVisibility, restoredOrganizationRole, restoredProjectRole string
	if err := run.db.QueryRowContext(ctx, `SELECT project.created_by::text,project.visibility::text,
		(SELECT role::text FROM organization_memberships WHERE organization_id=project.organization_id AND principal_id=$2),
		(SELECT role FROM project_memberships WHERE project_id=project.id AND principal_id=$2)
		FROM projects project WHERE project.id=$1`, projectID, humanID).
		Scan(&restoredCreator, &restoredVisibility, &restoredOrganizationRole, &restoredProjectRole); err != nil {
		return err
	}
	if restoredCreator != originalCreator || restoredVisibility != originalVisibility ||
		restoredOrganizationRole != originalOrganizationRole || restoredProjectRole != originalProjectRole.String {
		return fmt.Errorf("private project read fixture did not restore authority: project=%s creator=%s visibility=%s organization_role=%s project_role=%s",
			projectID, restoredCreator, restoredVisibility, restoredOrganizationRole, restoredProjectRole)
	}
	return nil
}

func assertForecastProjection(label string, actual, expected forecast) error {
	if actual.ID != expected.ID || actual.OrganizationID != expected.OrganizationID || actual.ProjectID != expected.ProjectID ||
		!equalOptionalString(actual.DeliverableID, expected.DeliverableID) || actual.Scope != expected.Scope ||
		actual.P50At != expected.P50At || actual.P90At != expected.P90At || actual.ReviewAfter != expected.ReviewAfter ||
		actual.Basis != expected.Basis || !reflect.DeepEqual(actual.Assumptions, expected.Assumptions) ||
		!reflect.DeepEqual(actual.ReasonCodes, expected.ReasonCodes) || actual.Impact != expected.Impact ||
		!equalOptionalString(actual.SupersedesID, expected.SupersedesID) || actual.CreatedBy != expected.CreatedBy ||
		actual.Current != expected.Current || actual.Stale != expected.Stale || actual.AttentionOnly != expected.AttentionOnly {
		return fmt.Errorf("%s mismatch: got=%+v want=%+v", label, actual, expected)
	}
	return nil
}

func equalOptionalString(left, right *string) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return *left == *right
}

func (run *runner) assertTargetProjection(ctx context.Context, projectID string, targetAt time.Time, reason string, actual target) error {
	var storedID string
	var storedAt time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT head.target_id::text,item.target_at
		FROM project_target_heads head JOIN project_targets item ON item.id=head.target_id WHERE head.project_id=$1`, projectID).
		Scan(&storedID, &storedAt); err != nil {
		return err
	}
	if actual.ID == "" || actual.ID != storedID || actual.ProjectID != projectID || actual.TargetAt != projectionTime(targetAt) ||
		!storedAt.UTC().Equal(targetAt) || actual.Reason != reason || actual.CreatedBy != humanID ||
		!actual.Current || !actual.Missed || !actual.AttentionOnly {
		return fmt.Errorf("target-deadline-distinction-and-attention-only target mismatch: got=%+v stored_id=%s stored_at=%s want_at=%s",
			actual, storedID, storedAt.UTC().Format(time.RFC3339Nano), targetAt.Format(time.RFC3339Nano))
	}
	return nil
}

func (run *runner) assertDeadlineProjection(ctx context.Context, projectID string, deadlineAt time.Time, description string, actual deadline) error {
	var storedID string
	var storedAt time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT head.deadline_id::text,item.deadline_at
		FROM project_deadline_heads head JOIN project_deadlines item ON item.id=head.deadline_id WHERE head.project_id=$1`, projectID).
		Scan(&storedID, &storedAt); err != nil {
		return err
	}
	if actual.ID == "" || actual.ID != storedID || actual.ProjectID != projectID || actual.DeadlineAt != projectionTime(deadlineAt) ||
		!storedAt.UTC().Equal(deadlineAt) || actual.Source != "contract" || actual.Description != description || actual.CreatedBy != humanID ||
		!actual.Current || !actual.Passed || !actual.AttentionOnly {
		return fmt.Errorf("target-deadline-distinction-and-attention-only deadline mismatch: got=%+v stored_id=%s stored_at=%s want_at=%s",
			actual, storedID, storedAt.UTC().Format(time.RFC3339Nano), deadlineAt.Format(time.RFC3339Nano))
	}
	return nil
}

func (run *runner) authorityDenies(ctx context.Context, deniedProjectID, allowedProjectID string, version int64) error {
	before := run.counts(ctx)
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[$1::uuid] WHERE token_prefix=$2`, allowedProjectID, agentToken[:16]); err != nil {
		return err
	}
	denied := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+deniedProjectID+"/forecasts", forecastInput(100, 120, 1), map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "m2c-restricted-deny-01", "If-Match": fmt.Sprintf(`"%d"`, version)})
	if err := expect(denied, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("restricted token deny wrote rows: %+v/%+v", before, got)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[]::uuid[],revoked_at=CURRENT_TIMESTAMP WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := expect(run.call(run.agent, http.MethodGet, "/api/v1/projects/"+allowedProjectID, nil, run.agentHeaders()), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET revoked_at=NULL WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='disabled' WHERE id=$1`, agentID); err != nil {
		return err
	}
	if err := expect(run.call(run.agent, http.MethodGet, "/api/v1/projects/"+allowedProjectID, nil, run.agentHeaders()), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='active' WHERE id=$1`, agentID); err != nil {
		return err
	}
	allowedRead := run.call(run.agent, http.MethodGet, "/api/v1/projects/"+allowedProjectID, nil, run.agentHeaders())
	if err := expect(allowedRead, http.StatusOK, ""); err != nil {
		return err
	}
	var allowed project
	if err := json.Unmarshal(allowedRead.Body, &allowed); err != nil {
		return err
	}
	selfWidening := forecastInput(100, 120, 1)
	selfWidening["organization_id"] = organizationID
	selfWidening["project_ids"] = []string{allowedProjectID}
	if err := expect(run.call(run.agent, http.MethodPost, "/api/v1/projects/"+allowedProjectID+"/forecasts", selfWidening,
		map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "m2c-self-widen-deny-01", "If-Match": fmt.Sprintf(`"%d"`, allowed.Version)}),
		http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}

	const foreignOrganizationID = "00000000-0000-4000-8000-000000000090"
	const foreignProjectID = "00000000-0000-4000-8000-000000000091"
	if _, err := run.db.ExecContext(ctx, `INSERT INTO organizations (id,slug,name,created_at) VALUES ($1,'m2c-foreign','M2C Foreign Organization',CURRENT_TIMESTAMP)`, foreignOrganizationID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `INSERT INTO projects
		(id,organization_id,title,outcome,mode,state,version,hypothesis,falsifier,decision_criteria,experiment_bound,created_by,created_at,updated_at)
		VALUES ($1,$2,'Foreign project','Tenant boundary fixture','exploration','proposed',1,'Cross-organization access is hidden',
		'Any visible response','["not found"]','One read',$3,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)`, foreignProjectID, foreignOrganizationID, humanID); err != nil {
		return err
	}
	if err := expect(run.call(run.agent, http.MethodGet, "/api/v1/projects/"+foreignProjectID, nil, run.agentHeaders()), http.StatusNotFound, "not_found"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `DELETE FROM projects WHERE id=$1`, foreignProjectID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `DELETE FROM organizations WHERE id=$1`, foreignOrganizationID); err != nil {
		return err
	}

	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='member'
		WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	visibleRead := run.call(run.human, http.MethodGet, "/api/v1/projects/"+allowedProjectID, nil, nil)
	if err := expect(visibleRead, http.StatusOK, ""); err != nil {
		return fmt.Errorf("organization-visible project must remain readable: %w", err)
	}
	noProjectRoleBefore := run.counts(ctx)
	noProjectRoleWrite := run.call(run.human, http.MethodPost, "/api/v1/projects/"+allowedProjectID+"/target", map[string]any{
		"target_at": time.Now().UTC().Add(72 * time.Hour).Format(time.RFC3339), "reason": "Visibility is not write authority",
	}, run.versionedHuman("m2c-visible-no-role-deny", allowed.Version))
	if err := expect(noProjectRoleWrite, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if got := run.counts(ctx); got != noProjectRoleBefore {
		return fmt.Errorf("organization-visible no-project-role deny wrote rows: before=%+v after=%+v", noProjectRoleBefore, got)
	}
	if err := run.observerPlanningWriteExpectedDeny(ctx, allowedProjectID, allowed.Version); err != nil {
		return err
	}
	if err := run.creatorObserverPlanningWriteExpectedDeny(ctx, deniedProjectID, version); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='owner'
		WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}

	if _, err := run.db.ExecContext(ctx, `UPDATE projects SET visibility='private' WHERE id=$1`, allowedProjectID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='member' WHERE organization_id=$1 AND principal_id IN ($2,$3)`, organizationID, humanID, agentID); err != nil {
		return err
	}
	if err := expect(run.call(run.human, http.MethodGet, "/api/v1/projects/"+allowedProjectID, nil, nil), http.StatusNotFound, "not_found"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE projects SET visibility='organization' WHERE id=$1`, allowedProjectID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role=CASE WHEN principal_id=$1 THEN 'owner'::organization_role ELSE 'member'::organization_role END WHERE organization_id=$2 AND principal_id IN ($1,$3)`, humanID, organizationID, agentID); err != nil {
		return err
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("authority denies left domain residue: before=%+v after=%+v", before, got)
	}
	run.write("m2c-authority-denies.json", map[string]any{
		"restricted_token": "deny-no-write", "revoked_token": "deny", "disabled_principal": "deny",
		"organization_visible_no_project_role_write": "forbidden-no-write", "private_project": "not-found",
		"observer_project_role_write":     "expected-deny-no-state-event-outbox-idempotency-residue",
		"creator_explicit_observer_write": "expected-deny-no-state-event-outbox-idempotency-residue",
		"cross_organization":              "not-found", "self_widening_fields": "invalid-request-no-write",
	})
	run.checks = append(run.checks, "restricted-revoked-visible-no-role-observer-creator-demotion-private-denies-no-write")
	return nil
}

func (run *runner) observerPlanningWriteExpectedDeny(ctx context.Context, projectID string, version int64) error {
	if _, err := run.db.ExecContext(ctx, `INSERT INTO project_memberships (project_id,principal_id,role,created_at)
		VALUES ($1,$2,'observer',CURRENT_TIMESTAMP)
		ON CONFLICT (project_id,principal_id) DO UPDATE SET role='observer'`, projectID, humanID); err != nil {
		return err
	}
	if err := expect(run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectID, nil, nil), http.StatusOK, ""); err != nil {
		return fmt.Errorf("observer project read: %w", err)
	}
	before := run.counts(ctx)
	denied := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
		"target_at": time.Now().UTC().Add(72 * time.Hour).Format(time.RFC3339), "reason": "Observer visibility is read-only",
	}, run.versionedHuman("m2c-observer-target-expected-deny", version))
	if err := expect(denied, http.StatusForbidden, "forbidden"); err != nil {
		return fmt.Errorf("observer target expected deny: %w", err)
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("observer target expected deny left residue: before=%+v after=%+v", before, got)
	}
	if _, err := run.db.ExecContext(ctx, `DELETE FROM project_memberships WHERE project_id=$1 AND principal_id=$2`, projectID, humanID); err != nil {
		return err
	}
	return nil
}

func (run *runner) creatorObserverPlanningWriteExpectedDeny(ctx context.Context, projectID string, version int64) error {
	var createdBy string
	if err := run.db.QueryRowContext(ctx, `SELECT created_by FROM projects WHERE id=$1`, projectID).Scan(&createdBy); err != nil {
		return err
	}
	if createdBy != humanID {
		return fmt.Errorf("creator demotion fixture created_by=%s want=%s", createdBy, humanID)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE project_memberships SET role='observer'
		WHERE project_id=$1 AND principal_id=$2`, projectID, humanID); err != nil {
		return err
	}
	if err := expect(run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectID, nil, nil), http.StatusOK, ""); err != nil {
		return fmt.Errorf("demoted creator project read: %w", err)
	}
	before := run.counts(ctx)
	denied := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
		"target_at": time.Now().UTC().Add(72 * time.Hour).Format(time.RFC3339), "reason": "Explicit creator membership is authoritative",
	}, run.versionedHuman("m2c-creator-observer-target-deny", version))
	if err := expect(denied, http.StatusForbidden, "forbidden"); err != nil {
		return fmt.Errorf("creator observer target expected deny: %w", err)
	}
	if got := run.counts(ctx); got != before {
		return fmt.Errorf("creator observer target expected deny left residue: before=%+v after=%+v", before, got)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE project_memberships SET role='owner'
		WHERE project_id=$1 AND principal_id=$2`, projectID, humanID); err != nil {
		return err
	}
	run.checks = append(run.checks, "explicit-creator-membership-overrides-creator-owner-fallback")
	return nil
}

func (run *runner) realtimePlanningResume(ctx context.Context, projectID string, version int64) (int64, error) {
	unknown := run.call(run.agent, http.MethodGet, "/api/v1/events?cursor=not-a-cursor", nil,
		map[string]string{"Authorization": "Bearer " + agentToken, "Accept": "text/event-stream"})
	if err := expect(unknown, http.StatusConflict, "snapshot_required"); err != nil {
		return 0, err
	}
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return 0, err
	}
	initial, err := run.openSSE("")
	if err != nil {
		return 0, err
	}
	ready, err := initial.next(3 * time.Second)
	initial.close()
	if err != nil || ready.Kind != "ready" || ready.ID == "" {
		return 0, fmt.Errorf("M2C SSE ready=%+v err=%v", ready, err)
	}
	changed := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
		"target_at": time.Now().UTC().Add(48 * time.Hour).Format(time.RFC3339), "reason": "Realtime resume planning canary",
	}, run.versionedHuman("m2c-realtime-target-01", version))
	if err := expect(changed, http.StatusCreated, ""); err != nil {
		return 0, err
	}
	version = etagVersion(changed)
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return 0, err
	}
	resumed, err := run.openSSE(ready.ID)
	if err != nil {
		return 0, err
	}
	defer resumed.close()
	resumeReady, err := resumed.next(3 * time.Second)
	if err != nil || resumeReady.Kind != "ready" {
		return 0, fmt.Errorf("M2C resumed SSE ready=%+v err=%v", resumeReady, err)
	}
	event, err := resumed.next(3 * time.Second)
	if err != nil || event.Kind != "domain-event" {
		return 0, fmt.Errorf("M2C resumed SSE event=%+v err=%v", event, err)
	}
	var envelope struct {
		EventType   string `json:"event_type"`
		AggregateID string `json:"aggregate_id"`
	}
	if err := json.Unmarshal(event.Data, &envelope); err != nil {
		return 0, err
	}
	encoded := strings.ToLower(string(event.Data))
	if envelope.EventType != "target.changed" || envelope.AggregateID != projectID ||
		strings.Contains(encoded, "password") || strings.Contains(encoded, "csrf") || strings.Contains(encoded, "bearer") || strings.Contains(encoded, "token") {
		return 0, fmt.Errorf("invalid planning realtime envelope: %s", event.Data)
	}
	run.write("m2c-realtime-resume.json", map[string]any{
		"snapshot_required": true, "resumed_event_type": envelope.EventType, "aggregate_id": envelope.AggregateID,
		"signed_cursor": event.ID != "", "secret_fields": "absent", "realtime_checkpointed": true,
	})
	run.checks = append(run.checks, "planning-event-snapshot-required-and-sse-resume")
	return version, nil
}

func (run *runner) openSSE(lastEventID string) (*sseClient, error) {
	request, err := http.NewRequest(http.MethodGet, run.base+"/api/v1/events", nil)
	if err != nil {
		return nil, err
	}
	request.Header.Set("Authorization", "Bearer "+agentToken)
	request.Header.Set("Accept", "text/event-stream")
	if lastEventID != "" {
		request.Header.Set("Last-Event-ID", lastEventID)
	}
	response, err := run.agent.Do(request)
	if err != nil {
		return nil, err
	}
	if response.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(response.Body)
		response.Body.Close()
		return nil, fmt.Errorf("SSE status=%d body=%s", response.StatusCode, body)
	}
	return &sseClient{response: response, reader: bufio.NewReader(response.Body)}, nil
}

func (client *sseClient) next(timeout time.Duration) (sseEvent, error) {
	type result struct {
		event sseEvent
		err   error
	}
	results := make(chan result, 1)
	go func() {
		var event sseEvent
		data := make([]string, 0)
		for {
			line, err := client.reader.ReadString('\n')
			if err != nil {
				results <- result{err: err}
				return
			}
			line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")
			if line == "" {
				event.Data = json.RawMessage(strings.Join(data, "\n"))
				results <- result{event: event}
				return
			}
			switch {
			case strings.HasPrefix(line, "event: "):
				event.Kind = strings.TrimPrefix(line, "event: ")
			case strings.HasPrefix(line, "id: "):
				event.ID = strings.TrimPrefix(line, "id: ")
			case strings.HasPrefix(line, "data: "):
				data = append(data, strings.TrimPrefix(line, "data: "))
			}
		}
	}()
	select {
	case result := <-results:
		return result.event, result.err
	case <-time.After(timeout):
		client.close()
		return sseEvent{}, errors.New("SSE event timeout")
	}
}

func (client *sseClient) close() {
	if client != nil && client.response != nil {
		_ = client.response.Body.Close()
	}
}

func (run *runner) waitRealtime(ctx context.Context, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		var events, last, realtimeDelivered, projectionDelivered, realtimeCheckpoint, projectionCheckpoint int
		if err := run.db.QueryRowContext(ctx, `SELECT
			(SELECT count(*) FROM domain_events),COALESCE((SELECT max(sequence) FROM domain_events),0),
			(SELECT count(*) FROM consumer_deliveries WHERE consumer_name='realtime-v1'),
			(SELECT count(*) FROM consumer_deliveries WHERE consumer_name='projection-v1'),
			(SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name='realtime-v1'),
			(SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name='projection-v1')`).Scan(
			&events, &last, &realtimeDelivered, &projectionDelivered, &realtimeCheckpoint, &projectionCheckpoint); err != nil {
			return err
		}
		if events == realtimeDelivered && events == projectionDelivered && last == realtimeCheckpoint && last == projectionCheckpoint {
			return nil
		}
		time.Sleep(50 * time.Millisecond)
	}
	return errors.New("outbox consumers did not deliver and checkpoint every M2C event")
}

func (run *runner) counts(ctx context.Context) counts {
	var value counts
	check(run.db.QueryRowContext(ctx, `SELECT
		(SELECT count(*) FROM projects),(SELECT count(*) FROM deliverables),(SELECT count(*) FROM forecasts),
		(SELECT count(*) FROM project_targets),(SELECT count(*) FROM project_deadlines),(SELECT count(*) FROM evidence),
		(SELECT count(*) FROM review_gates),(SELECT count(*) FROM review_verdicts),(SELECT count(*) FROM review_findings),
		(SELECT count(*) FROM finding_actions),(SELECT count(*) FROM deliverable_submissions),(SELECT count(*) FROM decisions),
		(SELECT count(*) FROM domain_events),(SELECT count(*) FROM outbox_records),(SELECT count(*) FROM idempotency_results)`).Scan(
		&value.Projects, &value.Deliverables, &value.Forecasts, &value.Targets, &value.Deadlines, &value.Evidence,
		&value.Gates, &value.Verdicts, &value.Findings, &value.FindingActions, &value.Submissions, &value.Decisions,
		&value.Events, &value.Outbox, &value.Idempotency))
	return value
}
func (run *runner) humanHeaders() map[string]string {
	return map[string]string{"Origin": publicOrigin, "X-CSRF-Token": run.csrf}
}
func (run *runner) agentHeaders() map[string]string {
	return map[string]string{"Authorization": "Bearer " + agentToken}
}
func (run *runner) versionedHuman(key string, version int64) map[string]string {
	return merge(run.humanHeaders(), map[string]string{"Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, version)})
}
func (run *runner) call(client *http.Client, method, path string, input any, headers map[string]string) snapshot {
	var body io.Reader
	if input != nil {
		encoded, err := json.Marshal(input)
		check(err)
		body = bytes.NewReader(encoded)
	}
	request, err := http.NewRequest(method, run.base+path, body)
	check(err)
	if input != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	for key, value := range headers {
		request.Header.Set(key, value)
	}
	response, err := client.Do(request)
	check(err)
	defer response.Body.Close()
	value, err := io.ReadAll(response.Body)
	check(err)
	return snapshot{Status: response.StatusCode, Body: value, Headers: map[string]string{"ETag": response.Header.Get("ETag"), "X-Request-ID": response.Header.Get("X-Request-ID")}}
}
func expect(response snapshot, status int, code string) error {
	if response.Status != status {
		return fmt.Errorf("status=%d want=%d body=%s", response.Status, status, response.Body)
	}
	if code != "" {
		var problem struct {
			Code string `json:"code"`
		}
		if err := json.Unmarshal(response.Body, &problem); err != nil {
			return err
		}
		if problem.Code != code {
			return fmt.Errorf("problem code=%s want=%s", problem.Code, code)
		}
	}
	return nil
}
func etagVersion(response snapshot) int64 {
	var value int64
	if _, err := fmt.Sscanf(response.Headers["ETag"], `"%d"`, &value); err != nil {
		panic(err)
	}
	return value
}
func projectInput(title string) map[string]any {
	return map[string]any{"title": title, "outcome": "Prove the M2C planning contract", "hypothesis": "One public planning plane preserves parity", "falsifier": "Either actor requires private state", "decision_criteria": []string{"Exact replay and parity"}, "experiment_bound": "M2C Track B only"}
}
func decisionInput() map[string]any {
	return map[string]any{"kind": "continue", "question": "Continue the bounded exploration?", "choice": "Continue", "alternatives": []string{"Stop"}, "rationale": "The bounded evidence remains informative", "evidence": []string{"Current planning evidence"}, "consequences": []string{"Preserve the non-terminal project"}}
}
func deliverableInput(title string) map[string]any {
	return map[string]any{"title": title, "description": "Observable planning-contract outcome", "required": true, "weight": 1000, "state": "ready", "acceptance_criteria": []string{"Exact-head smoke evidence passes"}}
}
func forecastInput(p50Hours, p90Hours, reviewHours int) map[string]any {
	now := time.Now().UTC().Truncate(time.Second)
	return map[string]any{"p50_at": now.Add(time.Duration(p50Hours) * time.Hour).Format(time.RFC3339), "p90_at": now.Add(time.Duration(p90Hours) * time.Hour).Format(time.RFC3339), "review_after": now.Add(time.Duration(reviewHours) * time.Hour).Format(time.RFC3339), "basis": "Measured M2B throughput", "assumptions": []string{"No scope expansion"}, "reason_codes": []string{"new-evidence"}, "impact": "Quality and security gates remain unchanged"}
}
func projectionTime(value time.Time) string {
	return value.UTC().Format("2006-01-02T15:04:05.000000Z")
}
func promotionInput() map[string]any {
	return map[string]any{"decision": map[string]any{"question": "Promote to exploitation?", "choice": "Promote", "alternatives": []string{"Continue exploration"}, "rationale": "Decision criteria are met", "evidence": []string{"M2C contract"}, "consequences": []string{"Deliver the admitted outcome"}}, "residual_uncertainty": "Integration uncertainty remains visible", "priority_rationale": "This is the sole admitted product unit", "deliverables": []any{deliverableInput("M2C planning contract")}}
}
func clone(source map[string]string) map[string]string {
	target := map[string]string{}
	for key, value := range source {
		target[key] = value
	}
	return target
}
func merge(left, right map[string]string) map[string]string {
	result := clone(left)
	for key, value := range right {
		result[key] = value
	}
	return result
}
func (run *runner) write(name string, value any) {
	encoded, err := json.MarshalIndent(value, "", "  ")
	check(err)
	check(os.WriteFile(filepath.Join(run.artifacts, name), append(encoded, '\n'), 0o644))
}
func envOr(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}
func check(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func digest(value []byte) string { sum := sha256.Sum256(value); return hex.EncodeToString(sum[:]) }
