package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/cookiejar"
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
	organizationID = "00000000-0000-4000-8000-000000000010"
	humanID        = "00000000-0000-4000-8000-000000000001"
	agentID        = "00000000-0000-4000-8000-000000000002"
	agentToken     = "wpa_local_walking_slice_agent_token_00000000000000000001"
	publicOrigin   = "http://localhost:8080"
	viewerID       = "00000000-0000-4000-8000-000000000081"
	viewerSession  = "m2e-viewer-session-token-00000000000000000000000001"
	viewerCSRF     = "m2e-viewer-csrf-token-000000000000000000000000001"
	foreignOrgID   = "00000000-0000-4000-8000-000000000090"
	foreignActorID = "00000000-0000-4000-8000-000000000091"
	foreignProject = "00000000-0000-4000-8000-000000000092"
	foreignWork    = "00000000-0000-4000-8000-000000000093"
)

type snapshot struct {
	Status  int
	Body    json.RawMessage
	Headers map[string]string
}

type session struct {
	CSRF string `json:"csrf_token"`
}
type project struct {
	ID      string `json:"id"`
	Version int64  `json:"version"`
}
type deliverable struct {
	ID      string `json:"id"`
	State   string `json:"state"`
	Version int64  `json:"version"`
}
type evidence struct {
	ID string `json:"id"`
}
type gate struct {
	ID      string `json:"id"`
	State   string `json:"state"`
	Version int64  `json:"version"`
}
type verdictResult struct {
	Gate     gate `json:"gate"`
	Findings []struct {
		ID string `json:"id"`
	} `json:"findings"`
}
type workItem struct {
	ID              string   `json:"id"`
	OrganizationID  string   `json:"organization_id"`
	ProjectID       string   `json:"project_id"`
	DeliverableID   *string  `json:"deliverable_id"`
	Title           string   `json:"title"`
	Description     string   `json:"description"`
	State           string   `json:"state"`
	Priority        string   `json:"priority"`
	AssigneeID      *string  `json:"assignee_id"`
	Version         int64    `json:"version"`
	Blocked         bool     `json:"blocked"`
	BlockingReasons []string `json:"blocking_reasons"`
	CreatedBy       string   `json:"created_by"`
	CreatedAt       string   `json:"created_at"`
	UpdatedAt       string   `json:"updated_at"`
}
type dependency struct {
	ID               string `json:"id"`
	SourceWorkItemID string `json:"source_work_item_id"`
	TargetWorkItemID string `json:"target_work_item_id"`
	Kind             string `json:"kind"`
	Version          int64  `json:"version"`
}
type dependencyResult struct {
	Dependency dependency `json:"dependency"`
	Source     workItem   `json:"source"`
	Removed    bool       `json:"removed"`
}
type batchResult struct {
	Items []workItem `json:"items"`
}
type graph struct {
	ProjectID    string       `json:"project_id"`
	WorkItems    []workItem   `json:"work_items"`
	Dependencies []dependency `json:"dependencies"`
}
type counts struct{ Work, Dependencies, Events, Outbox, Idempotency int }
type persistedState struct {
	ProjectID, WorkItemID, DependencyProjectID, DependencySourceID string
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
	base, artifacts, csrf string
	human, agent, viewer  *http.Client
	db                    *sql.DB
	checks                []string
}

func main() {
	mode := "exercise"
	if len(os.Args) > 1 {
		mode = os.Args[1]
	}
	jar, err := cookiejar.New(nil)
	check(err)
	db, err := sql.Open("postgres", envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	check(err)
	defer db.Close()
	run := &runner{base: envOr("WORKPLANE_API_BASE", "http://api:8080"), artifacts: envOr("WORKPLANE_EVIDENCE_DIR", "/evidence"),
		human: &http.Client{Jar: jar, Timeout: 10 * time.Second}, agent: &http.Client{Timeout: 10 * time.Second},
		viewer: &http.Client{Timeout: 10 * time.Second}, db: db}
	check(os.MkdirAll(run.artifacts, 0o755))
	ctx := context.Background()
	if mode == "verify-restart" {
		check(run.verifyRestart(ctx))
		fmt.Printf("M2E restart, replay, integrity, and corruption checks passed: %d checks\n", len(run.checks))
		return
	}
	check(run.exercise(ctx))
	fmt.Printf("M2E work lifecycle and dependency spine passed: %d load-bearing checks\n", len(run.checks))
}

func (run *runner) login() error {
	response := run.call(run.human, http.MethodPost, "/api/v1/session/login", map[string]any{
		"email": "human@workplane.local", "password": "walking-slice-password",
	}, nil)
	if err := expect(response, http.StatusOK, ""); err != nil {
		return err
	}
	var value session
	if err := json.Unmarshal(response.Body, &value); err != nil {
		return err
	}
	if len(value.CSRF) < 32 {
		return errors.New("login omitted CSRF token")
	}
	run.csrf = value.CSRF
	return nil
}

func (run *runner) exercise(ctx context.Context) error {
	if err := run.login(); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET scopes=ARRAY[
		'project.create','project.read','deliverable.read','deliverable.edit','deliverable.submit',
		'evidence.read','evidence.create','review.request','review.verdict','finding.resolve',
		'work.read','work.edit','work.transition','work.assign','dependency.read','dependency.edit',
		'realtime.subscribe','event.subscribe'
	],project_ids=ARRAY[]::uuid[],revoked_at=NULL WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	lifecycleProject, version, deliverableItem, evidenceItem, findingID, err := run.reviewFixture(ctx)
	if err != nil {
		return err
	}
	work, err := run.lifecycleFlow(ctx, lifecycleProject, deliverableItem, evidenceItem, findingID)
	if err != nil {
		return err
	}
	if err := run.hardGateBlocking(ctx, lifecycleProject, version); err != nil {
		return err
	}
	humanParity, humanProject, err := run.parityFlow(run.human, "human", run.humanHeaders())
	if err != nil {
		return err
	}
	agentParity, _, err := run.parityFlow(run.agent, "agent", run.agentHeaders())
	if err != nil {
		return err
	}
	if err := assertParity(humanParity, agentParity); err != nil {
		return err
	}
	parityEvidence, err := run.assertParityEvidence(ctx, humanParity, agentParity)
	if err != nil {
		return err
	}
	run.write("m2e-parity-authority.json", map[string]any{"human": parityShape(humanParity), "agent": parityShape(agentParity),
		"responses": "field-for-field-except-generated-identity-time-and-attribution", "error_and_side_effect_parity": parityEvidence})
	run.checks = append(run.checks, "human-agent-response-error-event-parity")
	dependencyProject, dependencySource, err := run.dependencyFlow(ctx)
	if err != nil {
		return err
	}
	if err := run.batchAtomicity(ctx, humanProject); err != nil {
		return err
	}
	if err := run.currentAuthorityRetry(ctx, humanProject); err != nil {
		return err
	}
	if err := run.realtimeEvidence(ctx, humanProject); err != nil {
		return err
	}
	if err := run.foreignBatchDeny(ctx, humanProject); err != nil {
		return err
	}
	if err := run.waitRealtime(ctx, 15*time.Second); err != nil {
		return err
	}
	store, err := app.NewDurableStore(envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	if err != nil {
		return err
	}
	defer store.Close()
	replay, err := store.Replay(ctx)
	if err != nil || replay.LiveChecksum != replay.RebuiltChecksum || replay.Work < 1 {
		return fmt.Errorf("M2E replay mismatch: report=%+v err=%v", replay, err)
	}
	findings, err := store.Doctor(ctx)
	if err != nil || len(findings) != 0 {
		return fmt.Errorf("healthy M2E doctor findings=%+v err=%v", findings, err)
	}
	run.write("m2e-durability-realtime.json", map[string]any{"replay": replay, "doctor_findings": findings,
		"outbox_checkpointed": true, "websocket_sse_envelope_source": "canonical-outbox"})
	run.checks = append(run.checks, "ledger-outbox-replay-checksum-doctor")
	state := persistedState{ProjectID: lifecycleProject.ID, WorkItemID: work.ID, DependencyProjectID: dependencyProject.ID, DependencySourceID: dependencySource.ID}
	run.write("m2e-state.json", state)
	run.write("m2e-summary.json", map[string]any{"checks": run.checks, "count": len(run.checks), "schema_version": 8})
	return nil
}

func projectInput(title string) map[string]any {
	return map[string]any{"title": title, "outcome": "Prove the fixed M2E work contract", "hypothesis": "One public work plane preserves graph safety",
		"falsifier": "Any actor requires private state", "decision_criteria": []string{"Exact replay and parity"}, "experiment_bound": "M2E Track B only"}
}

func (run *runner) createProject(client *http.Client, label string, headers map[string]string) (project, error) {
	response := run.call(client, http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput("M2E "+label),
		merge(headers, map[string]string{"Idempotency-Key": "m2e-project-" + label + "-0001"}))
	if err := expect(response, http.StatusCreated, ""); err != nil {
		return project{}, err
	}
	var item project
	err := json.Unmarshal(response.Body, &item)
	return item, err
}

func (run *runner) reviewFixture(ctx context.Context) (project, int64, deliverable, evidence, string, error) {
	item, err := run.createProject(run.human, "lifecycle", run.humanHeaders())
	if err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	version, deliverableItem, err := run.promoteReviewProject(item)
	if err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	gateResponse := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/gates", map[string]any{
		"name": "Lifecycle review", "kind": "review", "hard": false, "independence_required": false,
		"required_evidence": []any{map[string]any{"kind": "test-run", "claim": "M2E lifecycle passes"}},
	}, run.versionedHuman("m2e-lifecycle-gate-0001", version))
	if err := expect(gateResponse, http.StatusCreated, ""); err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	version = etagVersion(gateResponse)
	var gateItem gate
	if err := json.Unmarshal(gateResponse.Body, &gateItem); err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	evidenceResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+item.ID+"/evidence", map[string]any{
		"kind": "test-run", "title": "M2E lifecycle evidence", "claim": "M2E lifecycle passes", "source": "work-smoke",
		"content": "fixed lifecycle observed", "metadata": map[string]string{"gate": "m2e"},
		"supports": []any{map[string]any{"target_type": "deliverable", "target_id": deliverableItem.ID}, map[string]any{"target_type": "gate", "target_id": gateItem.ID}},
	}, run.versionedHuman("m2e-lifecycle-evidence-01", version))
	if err := expect(evidenceResponse, http.StatusCreated, ""); err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	version = etagVersion(evidenceResponse)
	var evidenceItem evidence
	if err := json.Unmarshal(evidenceResponse.Body, &evidenceItem); err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	submitted := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/submit", map[string]any{
		"evidence_ids": []string{evidenceItem.ID}, "note": "Ready for M2E review",
	}, run.versionedHuman("m2e-lifecycle-submit-001", version))
	if err := expect(submitted, http.StatusOK, ""); err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	version = etagVersion(submitted)
	failed := run.call(run.human, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", map[string]any{
		"result": "fail", "evidence_ids": []string{evidenceItem.ID},
		"findings": []any{map[string]any{"title": "Actionable work finding", "detail": "Revise the work item", "blocking": true}},
	}, run.versionedHuman("m2e-lifecycle-verdict-01", version))
	if err := expect(failed, http.StatusCreated, ""); err != nil {
		return project{}, 0, deliverable{}, evidence{}, "", err
	}
	version = etagVersion(failed)
	var verdict verdictResult
	if err := json.Unmarshal(failed.Body, &verdict); err != nil || len(verdict.Findings) != 1 {
		return project{}, 0, deliverable{}, evidence{}, "", fmt.Errorf("invalid review fixture: %+v err=%v", verdict, err)
	}
	return item, version, deliverableItem, evidenceItem, verdict.Findings[0].ID, nil
}

func (run *runner) promoteReviewProject(item project) (int64, deliverable, error) {
	now := time.Now().UTC().Truncate(time.Second)
	forecast := run.call(run.human, http.MethodPost, "/api/v1/projects/"+item.ID+"/forecasts", map[string]any{
		"p50_at": now.Add(24 * time.Hour).Format(time.RFC3339), "p90_at": now.Add(48 * time.Hour).Format(time.RFC3339),
		"review_after": now.Add(time.Hour).Format(time.RFC3339), "basis": "M2E fixed-scope evidence",
		"assumptions": []string{"No scope expansion"}, "reason_codes": []string{"new-evidence"}, "impact": "Work contract only",
	}, run.versionedHuman("m2e-review-forecast-001", item.Version))
	if err := expect(forecast, http.StatusCreated, ""); err != nil {
		return 0, deliverable{}, err
	}
	version := etagVersion(forecast)
	activated := run.call(run.human, http.MethodPost, "/api/v1/projects/"+item.ID+"/activate", map[string]any{
		"reason": "Activate bounded M2E review fixture",
	}, run.versionedHuman("m2e-review-activate-001", version))
	if err := expect(activated, http.StatusOK, ""); err != nil {
		return 0, deliverable{}, err
	}
	version = etagVersion(activated)
	promoted := run.call(run.human, http.MethodPost, "/api/v1/projects/"+item.ID+"/promote", map[string]any{
		"decision": map[string]any{"question": "Promote the bounded review fixture?", "choice": "Promote",
			"alternatives": []string{"Continue exploration"}, "rationale": "The contract is fixed",
			"evidence": []string{"M2E contract"}, "consequences": []string{"Exercise one work lifecycle"}},
		"residual_uncertainty": "Concurrency remains under test", "priority_rationale": "This is the sole active unit",
		"deliverables": []any{map[string]any{"title": "M2E lifecycle", "description": "Observable fixed lifecycle",
			"required": true, "weight": 1000, "state": "ready", "acceptance_criteria": []string{"Exact work evidence passes"}}},
	}, run.versionedHuman("m2e-review-promote-0001", version))
	if err := expect(promoted, http.StatusOK, ""); err != nil {
		return 0, deliverable{}, err
	}
	var result struct {
		Deliverables []deliverable `json:"deliverables"`
	}
	if err := json.Unmarshal(promoted.Body, &result); err != nil || len(result.Deliverables) != 1 {
		return 0, deliverable{}, fmt.Errorf("invalid review promotion: %+v err=%v", result, err)
	}
	return etagVersion(promoted), result.Deliverables[0], nil
}

func workInput(title string, deliverableID *string) map[string]any {
	value := map[string]any{"title": title, "description": "Observable M2E work", "priority": "high"}
	if deliverableID != nil {
		value["deliverable_id"] = *deliverableID
	}
	return value
}

func (run *runner) createWork(client *http.Client, projectID, key, title string, deliverableID *string, headers map[string]string) (workItem, snapshot, error) {
	response := run.call(client, http.MethodPost, "/api/v1/projects/"+projectID+"/work-items", workInput(title, deliverableID),
		merge(headers, map[string]string{"Idempotency-Key": key}))
	if err := expect(response, http.StatusCreated, ""); err != nil {
		return workItem{}, response, err
	}
	var item workItem
	err := json.Unmarshal(response.Body, &item)
	return item, response, err
}

func (run *runner) assign(client *http.Client, item workItem, key string, headers map[string]string) (workItem, snapshot, error) {
	response := run.call(client, http.MethodPost, "/api/v1/work-items/"+item.ID+"/assign", map[string]any{"assignee_id": humanID},
		merge(headers, map[string]string{"Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, item.Version)}))
	if err := expect(response, http.StatusOK, ""); err != nil {
		return workItem{}, response, err
	}
	var assigned workItem
	err := json.Unmarshal(response.Body, &assigned)
	return assigned, response, err
}

func (run *runner) transition(client *http.Client, item workItem, key, command, reason string, evidenceIDs []string, findingID *string, headers map[string]string) (workItem, snapshot, error) {
	body := map[string]any{"command": command, "reason": reason, "evidence_ids": evidenceIDs}
	if findingID != nil {
		body["finding_id"] = *findingID
	}
	response := run.call(client, http.MethodPost, "/api/v1/work-items/"+item.ID+"/transition", body,
		merge(headers, map[string]string{"Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, item.Version)}))
	if err := expect(response, http.StatusOK, ""); err != nil {
		return workItem{}, response, err
	}
	var transitioned workItem
	err := json.Unmarshal(response.Body, &transitioned)
	return transitioned, response, err
}

func (run *runner) lifecycleFlow(ctx context.Context, projectItem project, deliverableItem deliverable, evidenceItem evidence, findingID string) (workItem, error) {
	before := run.counts(ctx)
	omitted := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items", map[string]any{
		"title": "Missing priority", "description": "Must fail at boundary",
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-work-omitted-priority"}))
	if err := expect(omitted, http.StatusBadRequest, "invalid_request"); err != nil || run.counts(ctx) != before {
		return workItem{}, fmt.Errorf("required create presence check left residue: err=%v", err)
	}
	item, _, err := run.createWork(run.human, projectItem.ID, "m2e-lifecycle-work-0001", "Lifecycle item", &deliverableItem.ID, run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	emptyUpdate := run.call(run.human, http.MethodPatch, "/api/v1/work-items/"+item.ID, map[string]any{}, run.versionedHuman("m2e-empty-update-000001", item.Version))
	if err := expect(emptyUpdate, http.StatusBadRequest, "invalid_request"); err != nil {
		return workItem{}, err
	}
	item, _, err = run.assign(run.human, item, "m2e-lifecycle-assign-01", run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	item, _, err = run.transition(run.human, item, "m2e-lifecycle-start-001", "start", "Assignee is active", []string{}, nil, run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	item, reviewResponse, err := run.transition(run.human, item, "m2e-lifecycle-review-01", "request_review", "Evidence is ready", []string{evidenceItem.ID}, nil, run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	retry := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+item.ID+"/transition", map[string]any{
		"command": "request_review", "reason": "Evidence is ready", "evidence_ids": []string{evidenceItem.ID},
	}, run.versionedHuman("m2e-lifecycle-review-01", item.Version-1))
	if err := expect(retry, http.StatusOK, ""); err != nil || !bytes.Equal(retry.Body, reviewResponse.Body) {
		return workItem{}, fmt.Errorf("transition idempotent replay diverged: err=%v", err)
	}
	item, _, err = run.transition(run.human, item, "m2e-lifecycle-bounce-01", "bounce", "Address actionable finding", []string{}, &findingID, run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	item, _, err = run.transition(run.human, item, "m2e-lifecycle-review-02", "request_review", "Finding addressed", []string{evidenceItem.ID}, nil, run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	item, _, err = run.transition(run.human, item, "m2e-lifecycle-accept-01", "accept", "Outcome evidence linked", []string{evidenceItem.ID}, nil, run.humanHeaders())
	if err != nil || item.State != "done" {
		return workItem{}, fmt.Errorf("accept transition=%+v err=%v", item, err)
	}
	getDeliverable := run.call(run.human, http.MethodGet, "/api/v1/deliverables/"+deliverableItem.ID, nil, nil)
	if err := expect(getDeliverable, http.StatusOK, ""); err != nil {
		return workItem{}, err
	}
	var stillSubmitted deliverable
	if err := json.Unmarshal(getDeliverable.Body, &stillSubmitted); err != nil || stillSubmitted.State != "submitted" {
		return workItem{}, fmt.Errorf("work done substituted deliverable acceptance: %+v err=%v", stillSubmitted, err)
	}
	cancelItem, _, err := run.createWork(run.human, projectItem.ID, "m2e-cancel-work-000001", "Cancellation item", nil, run.humanHeaders())
	if err != nil {
		return workItem{}, err
	}
	faultBefore := run.counts(ctx)
	fault := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+cancelItem.ID+"/transition", map[string]any{
		"command": "cancel", "reason": "Post-row fault", "evidence_ids": []string{},
	}, merge(run.versionedHuman("m2e-cancel-fault-00001", cancelItem.Version), map[string]string{"X-Workplane-Fault": "after-work-row"}))
	if err := expect(fault, http.StatusServiceUnavailable, "service_unavailable"); err != nil || run.counts(ctx) != faultBefore {
		return workItem{}, fmt.Errorf("post-row fault left residue: err=%v before=%+v after=%+v", err, faultBefore, run.counts(ctx))
	}
	cancelItem, _, err = run.transition(run.human, cancelItem, "m2e-cancel-success-0001", "cancel", "No longer required", []string{}, nil, run.humanHeaders())
	if err != nil || cancelItem.State != "cancelled" {
		return workItem{}, fmt.Errorf("cancellation=%+v err=%v", cancelItem, err)
	}
	run.write("m2e-lifecycle.json", map[string]any{"states": []string{"open", "in_progress", "in_review", "in_progress", "in_review", "done"},
		"cancelled": true, "blocked_persisted_state": false, "deliverable_state_after_work_done": stillSubmitted.State,
		"required_presence": "fail-no-write", "idempotent_retry": "exact-stored-response", "post_row_fault": "zero-residue"})
	run.checks = append(run.checks, "fixed-lifecycle-bounce-cancel-derived-state-and-separate-acceptance")
	return item, nil
}

func (run *runner) hardGateBlocking(ctx context.Context, projectItem project, version int64) error {
	created := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/deliverables", map[string]any{
		"title": "Hard-gated work", "description": "Hard gate drives derived blocking", "required": true, "weight": 1000,
		"state": "ready", "acceptance_criteria": []string{"Hard gate passes"},
	}, run.versionedHuman("m2e-hard-deliverable-001", version))
	if err := expect(created, http.StatusCreated, ""); err != nil {
		return err
	}
	version = etagVersion(created)
	var deliverableItem deliverable
	if err := json.Unmarshal(created.Body, &deliverableItem); err != nil {
		return err
	}
	gateResponse := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/gates", map[string]any{
		"name": "Hard work gate", "kind": "review", "hard": true, "independence_required": false,
		"required_evidence": []any{map[string]any{"kind": "test-run", "claim": "Hard work gate passes"}},
	}, run.versionedHuman("m2e-hard-gate-0000001", version))
	if err := expect(gateResponse, http.StatusCreated, ""); err != nil {
		return err
	}
	version = etagVersion(gateResponse)
	var gateItem gate
	if err := json.Unmarshal(gateResponse.Body, &gateItem); err != nil {
		return err
	}
	evidenceResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/evidence", map[string]any{
		"kind": "test-run", "title": "Hard gate evidence", "claim": "Hard work gate passes", "source": "work-smoke",
		"content": "hard gate observed", "metadata": map[string]string{"contract": "m2e"},
		"supports": []any{map[string]any{"target_type": "deliverable", "target_id": deliverableItem.ID}, map[string]any{"target_type": "gate", "target_id": gateItem.ID}},
	}, run.versionedHuman("m2e-hard-evidence-0001", version))
	if err := expect(evidenceResponse, http.StatusCreated, ""); err != nil {
		return err
	}
	version = etagVersion(evidenceResponse)
	var evidenceItem evidence
	if err := json.Unmarshal(evidenceResponse.Body, &evidenceItem); err != nil {
		return err
	}
	submitted := run.call(run.human, http.MethodPost, "/api/v1/deliverables/"+deliverableItem.ID+"/submit", map[string]any{
		"evidence_ids": []string{evidenceItem.ID}, "note": "Ready for hard-gate review",
	}, run.versionedHuman("m2e-hard-submit-000001", version))
	if err := expect(submitted, http.StatusOK, ""); err != nil {
		return err
	}
	version = etagVersion(submitted)
	item, _, err := run.createWork(run.human, projectItem.ID, "m2e-hard-work-0000001", "Hard-gated item", &deliverableItem.ID, run.humanHeaders())
	if err != nil {
		return err
	}
	if !item.Blocked || !reflect.DeepEqual(item.BlockingReasons, []string{"hard_gate"}) {
		return fmt.Errorf("hard gate was not a derived blocker: %+v", item)
	}
	passed := run.call(run.human, http.MethodPost, "/api/v1/gates/"+gateItem.ID+"/verdicts", map[string]any{
		"result": "pass", "evidence_ids": []string{evidenceItem.ID}, "findings": []any{},
	}, run.versionedHuman("m2e-hard-verdict-0001", version))
	if err := expect(passed, http.StatusCreated, ""); err != nil {
		return err
	}
	refreshed := run.call(run.human, http.MethodGet, "/api/v1/work-items/"+item.ID, nil, run.humanHeaders())
	if err := expect(refreshed, http.StatusOK, ""); err != nil {
		return err
	}
	if err := json.Unmarshal(refreshed.Body, &item); err != nil || item.Blocked || len(item.BlockingReasons) != 0 {
		return fmt.Errorf("passed hard gate did not clear derived blocking: %+v err=%v", item, err)
	}
	run.checks = append(run.checks, "hard-gate-derived-blocking")
	return nil
}

func (run *runner) parityFlow(client *http.Client, actor string, headers map[string]string) (workItem, project, error) {
	projectItem, err := run.createProject(client, "parity-"+actor, headers)
	if err != nil {
		return workItem{}, project{}, err
	}
	item, _, err := run.createWork(client, projectItem.ID, "m2e-parity-"+actor+"-work-01", "Parity item", nil, headers)
	if err != nil {
		return workItem{}, project{}, err
	}
	item, _, err = run.assign(client, item, "m2e-parity-"+actor+"-assign", headers)
	if err != nil {
		return workItem{}, project{}, err
	}
	item, _, err = run.transition(client, item, "m2e-parity-"+actor+"-start-1", "start", "Active assignee starts", []string{}, nil, headers)
	return item, projectItem, err
}

func parityShape(item workItem) map[string]any {
	return map[string]any{"title": item.Title, "description": item.Description, "state": item.State, "priority": item.Priority,
		"assignee_present": item.AssigneeID != nil, "version": item.Version, "blocked": item.Blocked, "blocking_reasons": item.BlockingReasons,
		"deliverable_present": item.DeliverableID != nil}
}

func assertParity(human, agent workItem) error {
	if !reflect.DeepEqual(parityShape(human), parityShape(agent)) {
		return fmt.Errorf("human/agent work response parity diverged: human=%+v agent=%+v", parityShape(human), parityShape(agent))
	}
	return nil
}

func (run *runner) assertParityEvidence(ctx context.Context, human, agent workItem) (map[string]any, error) {
	type problemShape struct {
		Type, Title, Code, Detail string
		Status                    int
	}
	invalid := func(client *http.Client, item workItem, key string, headers map[string]string) (problemShape, error) {
		response := run.call(client, http.MethodPost, "/api/v1/work-items/"+item.ID+"/transition", map[string]any{
			"command": "start", "reason": "Invalid repeated start", "evidence_ids": []string{},
		}, merge(headers, map[string]string{"Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, item.Version)}))
		if err := expect(response, http.StatusConflict, "invariant_violation"); err != nil {
			return problemShape{}, err
		}
		var value struct {
			Type   string `json:"type"`
			Title  string `json:"title"`
			Status int    `json:"status"`
			Code   string `json:"code"`
			Detail string `json:"detail"`
		}
		if err := json.Unmarshal(response.Body, &value); err != nil {
			return problemShape{}, err
		}
		return problemShape{Type: value.Type, Title: value.Title, Status: value.Status, Code: value.Code, Detail: value.Detail}, nil
	}
	before := run.counts(ctx)
	humanProblem, err := invalid(run.human, human, "m2e-parity-human-invalid", run.humanHeaders())
	if err != nil {
		return nil, err
	}
	agentProblem, err := invalid(run.agent, agent, "m2e-parity-agent-invalid", run.agentHeaders())
	if err != nil {
		return nil, err
	}
	if humanProblem != agentProblem {
		return nil, fmt.Errorf("human/agent error parity diverged: human=%+v agent=%+v", humanProblem, agentProblem)
	}
	if after := run.counts(ctx); after != before {
		return nil, fmt.Errorf("human/agent invalid parity attempts left residue: before=%+v after=%+v", before, after)
	}
	events := func(id string) ([]string, int, error) {
		rows, err := run.db.QueryContext(ctx, `SELECT event_type,aggregate_version FROM domain_events WHERE aggregate_type='work_item' AND aggregate_id=$1 ORDER BY aggregate_version`, id)
		if err != nil {
			return nil, 0, err
		}
		defer rows.Close()
		values := []string{}
		for rows.Next() {
			var eventType string
			var version int
			if err := rows.Scan(&eventType, &version); err != nil {
				return nil, 0, err
			}
			values = append(values, fmt.Sprintf("%d:%s", version, eventType))
		}
		var coupled int
		if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM domain_events event JOIN outbox_records record ON record.event_id=event.event_id
			WHERE event.aggregate_type='work_item' AND event.aggregate_id=$1`, id).Scan(&coupled); err != nil {
			return nil, 0, err
		}
		return values, coupled, rows.Err()
	}
	humanEvents, humanOutbox, err := events(human.ID)
	if err != nil {
		return nil, err
	}
	agentEvents, agentOutbox, err := events(agent.ID)
	if err != nil {
		return nil, err
	}
	if !reflect.DeepEqual(humanEvents, agentEvents) || humanOutbox != agentOutbox || humanOutbox != len(humanEvents) {
		return nil, fmt.Errorf("human/agent event/outbox parity diverged: human=%v/%d agent=%v/%d", humanEvents, humanOutbox, agentEvents, agentOutbox)
	}
	return map[string]any{"problem": humanProblem, "events": humanEvents, "outbox_rows_each": humanOutbox, "denied_side_effects": 0}, nil
}

func (run *runner) addDependency(client *http.Client, source, target workItem, kind, key string, headers map[string]string) (dependencyResult, snapshot, error) {
	response := run.call(client, http.MethodPost, "/api/v1/work-items/"+source.ID+"/dependencies", map[string]any{
		"target_work_item_id": target.ID, "kind": kind, "expected_target_version": target.Version,
	}, merge(headers, map[string]string{"Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, source.Version)}))
	if err := expect(response, http.StatusCreated, ""); err != nil {
		return dependencyResult{}, response, err
	}
	var result dependencyResult
	err := json.Unmarshal(response.Body, &result)
	return result, response, err
}

func (run *runner) dependencyFlow(ctx context.Context) (project, workItem, error) {
	projectItem, err := run.createProject(run.human, "dependencies", run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	create := func(label string) (workItem, error) {
		item, _, err := run.createWork(run.human, projectItem.ID, "m2e-dep-work-"+label+"-0001", "Dependency "+label, nil, run.humanHeaders())
		return item, err
	}
	a, err := create("a")
	if err != nil {
		return project{}, workItem{}, err
	}
	b, err := create("b")
	if err != nil {
		return project{}, workItem{}, err
	}
	c, err := create("c")
	if err != nil {
		return project{}, workItem{}, err
	}
	for index, boundary := range []string{"after-dependency-row", "after-dependency-event"} {
		before := run.counts(ctx)
		fault := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+a.ID+"/dependencies", map[string]any{
			"target_work_item_id": b.ID, "kind": "blocks", "expected_target_version": b.Version,
		}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": fmt.Sprintf("m2e-dep-fault-%02d-00000", index),
			"If-Match": fmt.Sprintf(`"%d"`, a.Version), "X-Workplane-Fault": boundary}))
		if err := expect(fault, http.StatusServiceUnavailable, "service_unavailable"); err != nil || run.counts(ctx) != before {
			return project{}, workItem{}, fmt.Errorf("dependency %s fault left residue: err=%v before=%+v after=%+v", boundary, err, before, run.counts(ctx))
		}
	}
	edgeAB, addAB, err := run.addDependency(run.human, a, b, "blocks", "m2e-dep-a-b-blocks-01", run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	a = edgeAB.Source
	if !a.Blocked || !reflect.DeepEqual(a.BlockingReasons, []string{"dependency"}) {
		return project{}, workItem{}, fmt.Errorf("blocks edge did not derive source blocking: %+v", a)
	}
	retry := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+a.ID+"/dependencies", map[string]any{
		"target_work_item_id": b.ID, "kind": "blocks", "expected_target_version": b.Version,
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-dep-a-b-blocks-01", "If-Match": `"1"`}))
	if err := expect(retry, http.StatusCreated, ""); err != nil || !bytes.Equal(retry.Body, addAB.Body) {
		return project{}, workItem{}, fmt.Errorf("dependency exact retry diverged: err=%v", err)
	}
	self := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+a.ID+"/dependencies", map[string]any{
		"target_work_item_id": a.ID, "kind": "blocks", "expected_target_version": a.Version,
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-dep-self-cycle-001", "If-Match": fmt.Sprintf(`"%d"`, a.Version)}))
	if err := expect(self, http.StatusConflict, "dependency_cycle"); err != nil {
		return project{}, workItem{}, err
	}
	direct := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+b.ID+"/dependencies", map[string]any{
		"target_work_item_id": a.ID, "kind": "blocks", "expected_target_version": a.Version,
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-dep-direct-cycle-1", "If-Match": fmt.Sprintf(`"%d"`, b.Version)}))
	if err := expect(direct, http.StatusConflict, "dependency_cycle"); err != nil {
		return project{}, workItem{}, err
	}
	edgeBC, _, err := run.addDependency(run.human, b, c, "blocks", "m2e-dep-b-c-blocks-01", run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	b = edgeBC.Source
	transitive := run.call(run.human, http.MethodPost, "/api/v1/work-items/"+c.ID+"/dependencies", map[string]any{
		"target_work_item_id": a.ID, "kind": "blocks", "expected_target_version": a.Version,
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-dep-transitive-cycle", "If-Match": fmt.Sprintf(`"%d"`, c.Version)}))
	if err := expect(transitive, http.StatusConflict, "dependency_cycle"); err != nil {
		return project{}, workItem{}, err
	}
	// Non-blocking edge kinds may point back through the blocking graph and must
	// not participate in DAG detection.
	caused, _, err := run.addDependency(run.human, c, a, "caused-by", "m2e-dep-caused-reverse", run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	c = caused.Source

	d, err := create("d")
	if err != nil {
		return project{}, workItem{}, err
	}
	e, err := create("e")
	if err != nil {
		return project{}, workItem{}, err
	}
	responses := make([]snapshot, 2)
	var wait sync.WaitGroup
	for index, pair := range [][2]workItem{{d, e}, {e, d}} {
		wait.Add(1)
		go func(index int, pair [2]workItem) {
			defer wait.Done()
			responses[index] = run.call(run.human, http.MethodPost, "/api/v1/work-items/"+pair[0].ID+"/dependencies", map[string]any{
				"target_work_item_id": pair[1].ID, "kind": "blocks", "expected_target_version": pair[1].Version,
			}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": fmt.Sprintf("m2e-concurrent-inverse-%d", index), "If-Match": fmt.Sprintf(`"%d"`, pair[0].Version)}))
		}(index, pair)
	}
	wait.Wait()
	sort.Slice(responses, func(i, j int) bool { return responses[i].Status < responses[j].Status })
	if responses[0].Status != http.StatusCreated || responses[1].Status != http.StatusConflict {
		return project{}, workItem{}, fmt.Errorf("concurrent inverse edge outcome was not one-create/one-conflict: %+v", responses)
	}
	var concurrentProblem struct {
		Code string `json:"code"`
	}
	if err := json.Unmarshal(responses[1].Body, &concurrentProblem); err != nil ||
		(concurrentProblem.Code != "dependency_cycle" && concurrentProblem.Code != "version_conflict") {
		return project{}, workItem{}, fmt.Errorf("concurrent inverse conflict was unstable: %s err=%v", responses[1].Body, err)
	}
	var cycle bool
	if err := run.db.QueryRowContext(ctx, `WITH RECURSIVE walk(root,id,path,cycle) AS (
		SELECT source_work_item_id,target_work_item_id,ARRAY[source_work_item_id,target_work_item_id],source_work_item_id=target_work_item_id
		FROM work_item_dependencies WHERE organization_id=$1 AND kind='blocks'
		UNION ALL SELECT walk.root,d.target_work_item_id,walk.path||d.target_work_item_id,d.target_work_item_id=ANY(walk.path)
		FROM walk JOIN work_item_dependencies d ON d.source_work_item_id=walk.id AND d.organization_id=$1 AND d.kind='blocks' WHERE NOT walk.cycle
	) SELECT EXISTS(SELECT 1 FROM walk WHERE cycle)`, organizationID).Scan(&cycle); err != nil || cycle {
		return project{}, workItem{}, fmt.Errorf("database graph contains cycle=%v err=%v", cycle, err)
	}

	removed := run.call(run.human, http.MethodDelete, "/api/v1/work-items/"+a.ID+"/dependencies/"+edgeAB.Dependency.ID, map[string]any{
		"expected_dependency_version": edgeAB.Dependency.Version, "expected_target_version": b.Version,
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-dep-remove-a-b-01", "If-Match": fmt.Sprintf(`"%d"`, a.Version)}))
	if err := expect(removed, http.StatusOK, ""); err != nil {
		return project{}, workItem{}, err
	}
	var removal dependencyResult
	if err := json.Unmarshal(removed.Body, &removal); err != nil || !removal.Removed || removal.Source.Blocked {
		return project{}, workItem{}, fmt.Errorf("dependency removal did not clear derived blocking: %+v err=%v", removal, err)
	}
	removeRetry := run.call(run.human, http.MethodDelete, "/api/v1/work-items/"+a.ID+"/dependencies/"+edgeAB.Dependency.ID, map[string]any{
		"expected_dependency_version": edgeAB.Dependency.Version, "expected_target_version": b.Version,
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-dep-remove-a-b-01", "If-Match": fmt.Sprintf(`"%d"`, a.Version)}))
	if err := expect(removeRetry, http.StatusOK, ""); err != nil || !bytes.Equal(removeRetry.Body, removed.Body) {
		return project{}, workItem{}, fmt.Errorf("dependency removal retry diverged: err=%v", err)
	}
	a = removal.Source
	privateTargetProject, err := run.createProject(run.human, "private-dependency-target", run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	privateTarget, _, err := run.createWork(run.human, privateTargetProject.ID, "m2e-private-target-work", "Private target", nil, run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	privateEdge, _, err := run.addDependency(run.human, a, privateTarget, "relates", "m2e-private-cross-edge", run.humanHeaders())
	if err != nil {
		return project{}, workItem{}, err
	}
	a = privateEdge.Source
	if err := run.seedViewer(ctx, projectItem.ID); err != nil {
		return project{}, workItem{}, err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE projects SET visibility='private' WHERE id=$1 OR id=$2`,
		projectItem.ID, privateTargetProject.ID); err != nil {
		return project{}, workItem{}, err
	}
	viewerGraph := run.call(run.viewer, http.MethodGet, "/api/v1/projects/"+projectItem.ID+"/dependency-graph", nil,
		map[string]string{"Cookie": "workplane_session=" + viewerSession})
	if err := expect(viewerGraph, http.StatusOK, ""); err != nil {
		return project{}, workItem{}, err
	}
	if bytes.Contains(viewerGraph.Body, []byte(privateTarget.ID)) || bytes.Contains(viewerGraph.Body, []byte(privateEdge.Dependency.ID)) {
		return project{}, workItem{}, fmt.Errorf("private cross-project graph disclosed target: %s", viewerGraph.Body)
	}

	graphResponse := run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectItem.ID+"/dependency-graph", nil, run.humanHeaders())
	if err := expect(graphResponse, http.StatusOK, ""); err != nil {
		return project{}, workItem{}, err
	}
	var complete graph
	if err := json.Unmarshal(graphResponse.Body, &complete); err != nil || len(complete.WorkItems) < 5 || len(complete.Dependencies) < 3 {
		return project{}, workItem{}, fmt.Errorf("incomplete dependency graph: %+v err=%v", complete, err)
	}
	run.write("m2e-dependencies.json", map[string]any{"direct_cycle": "dependency_cycle", "transitive_cycle": "dependency_cycle",
		"non_blocking_reverse": "allowed", "removal_exact_retry": true, "private_target_disclosed": false, "graph": complete})
	run.write("m2e-concurrency.json", map[string]any{"statuses": []int{responses[0].Status, responses[1].Status}, "conflict_code": concurrentProblem.Code, "database_cycle": cycle,
		"serialization": "organization-scoped-advisory-lock-plus-database-trigger"})
	run.checks = append(run.checks, "typed-dependency-dag-removal-and-concurrent-cycle-safety")
	return projectItem, removal.Source, nil
}

func keyedHash(value string) []byte {
	mac := hmac.New(sha256.New, []byte("local-only-key-material-32-bytes-minimum-change-me"))
	_, _ = mac.Write([]byte(value))
	return mac.Sum(nil)
}

func (run *runner) seedViewer(ctx context.Context, projectID string) error {
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
		{`INSERT INTO principals (id,kind,display_name,status,human_principal_id,created_at) VALUES ($1,'human','M2E private graph viewer','active',NULL,$2)`, []any{viewerID, now}},
		{`INSERT INTO organization_memberships (organization_id,principal_id,role,created_at) VALUES ($1,$2,'member',$3)`, []any{organizationID, viewerID, now}},
		{`INSERT INTO human_sessions (id,principal_id,token_hash,csrf_hash,expires_at,created_at) VALUES ('00000000-0000-4000-8000-000000000082',$1,$2,$3,$4,$5)`, []any{viewerID, keyedHash(viewerSession), keyedHash(viewerCSRF), now.Add(time.Hour), now}},
		{`INSERT INTO project_memberships (project_id,principal_id,role,created_at) VALUES ($1,$2,'observer',$3)`, []any{projectID, viewerID, now}},
	}
	for _, statement := range statements {
		if _, err := tx.ExecContext(ctx, statement.query, statement.args...); err != nil {
			return err
		}
	}
	return tx.Commit()
}

func batchEntry(item workItem, command string) map[string]any {
	return map[string]any{"work_item_id": item.ID, "expected_version": item.Version, "command": command,
		"reason": "Atomic M2E batch", "evidence_ids": []string{}}
}

func (run *runner) batchAtomicity(ctx context.Context, projectItem project) error {
	first, _, err := run.createWork(run.human, projectItem.ID, "m2e-batch-first-00001", "Batch first", nil, run.humanHeaders())
	if err != nil {
		return err
	}
	second, _, err := run.createWork(run.human, projectItem.ID, "m2e-batch-second-0001", "Batch second", nil, run.humanHeaders())
	if err != nil {
		return err
	}
	first, _, err = run.assign(run.human, first, "m2e-batch-first-assign", run.humanHeaders())
	if err != nil {
		return err
	}
	second, _, err = run.assign(run.human, second, "m2e-batch-second-assign", run.humanHeaders())
	if err != nil {
		return err
	}
	input := map[string]any{"items": []any{batchEntry(first, "start"), batchEntry(second, "start")}}
	before := run.counts(ctx)
	fault := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition", input,
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-fault-000001", "X-Workplane-Fault": "after-work-batch-row"}))
	if err := expect(fault, http.StatusServiceUnavailable, "service_unavailable"); err != nil || run.counts(ctx) != before {
		return fmt.Errorf("batch fault left residue: err=%v before=%+v after=%+v", err, before, run.counts(ctx))
	}
	for _, item := range []workItem{first, second} {
		response := run.call(run.human, http.MethodGet, "/api/v1/work-items/"+item.ID, nil, run.humanHeaders())
		var current workItem
		if err := expect(response, http.StatusOK, ""); err != nil {
			return err
		}
		if err := json.Unmarshal(response.Body, &current); err != nil || current.State != "open" || current.Version != item.Version {
			return fmt.Errorf("batch rollback changed item: %+v err=%v", current, err)
		}
	}
	success := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition", input,
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-success-0001"}))
	if err := expect(success, http.StatusOK, ""); err != nil {
		return err
	}
	var result batchResult
	if err := json.Unmarshal(success.Body, &result); err != nil || len(result.Items) != 2 || result.Items[0].State != "in_progress" || result.Items[1].State != "in_progress" {
		return fmt.Errorf("invalid successful batch: %+v err=%v", result, err)
	}
	retry := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition", input,
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-success-0001"}))
	if err := expect(retry, http.StatusOK, ""); err != nil || !bytes.Equal(retry.Body, success.Body) {
		return fmt.Errorf("batch exact retry diverged: err=%v", err)
	}
	staleBefore := run.counts(ctx)
	stale := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition", input,
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-stale-00001"}))
	if err := expect(stale, http.StatusConflict, "version_conflict"); err != nil || run.counts(ctx) != staleBefore {
		return fmt.Errorf("stale batch left residue: err=%v", err)
	}
	duplicate := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition",
		map[string]any{"items": []any{batchEntry(result.Items[0], "cancel"), batchEntry(result.Items[0], "cancel")}},
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-duplicate-001"}))
	if err := expect(duplicate, http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}
	missing := workItem{ID: "00000000-0000-4000-8000-000000000099", Version: 1}
	missingResponse := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition",
		map[string]any{"items": []any{batchEntry(result.Items[0], "cancel"), batchEntry(missing, "cancel")}},
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-missing-0001"}))
	if err := expect(missingResponse, http.StatusNotFound, "not_found"); err != nil {
		return err
	}
	otherProject, err := run.createProject(run.human, "batch-mixed", run.humanHeaders())
	if err != nil {
		return err
	}
	other, _, err := run.createWork(run.human, otherProject.ID, "m2e-batch-other-work-1", "Other project item", nil, run.humanHeaders())
	if err != nil {
		return err
	}
	mixed := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition",
		map[string]any{"items": []any{batchEntry(result.Items[0], "cancel"), batchEntry(other, "cancel")}},
		merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-mixed-project"}))
	if err := expect(mixed, http.StatusNotFound, "not_found"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[$1::uuid] WHERE token_prefix=$2`, otherProject.ID, agentToken[:16]); err != nil {
		return err
	}
	denied := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition",
		map[string]any{"items": []any{batchEntry(result.Items[0], "cancel")}},
		merge(run.agentHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-scope-deny-01"}))
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[]::uuid[] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := expect(denied, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	dependent, _, err := run.createWork(run.human, projectItem.ID, "m2e-batch-dependent-001", "Batch dependent", nil, run.humanHeaders())
	if err != nil {
		return err
	}
	prerequisite, _, err := run.createWork(run.human, projectItem.ID, "m2e-batch-prerequisite", "Batch prerequisite", nil, run.humanHeaders())
	if err != nil {
		return err
	}
	dependent, _, err = run.assign(run.human, dependent, "m2e-batch-dependent-assign", run.humanHeaders())
	if err != nil {
		return err
	}
	prerequisite, _, err = run.assign(run.human, prerequisite, "m2e-batch-prereq-assign", run.humanHeaders())
	if err != nil {
		return err
	}
	linked, _, err := run.addDependency(run.human, dependent, prerequisite, "blocks", "m2e-batch-final-state-edge", run.humanHeaders())
	if err != nil {
		return err
	}
	dependent = linked.Source
	if !dependent.Blocked {
		return errors.New("batch final-state fixture was not initially blocked")
	}
	finalStateBatch := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition", map[string]any{
		// Deliberately put the dependent event first. Replay must derive blocking
		// from this atomic command's final state, not incidental event order.
		"items": []any{batchEntry(dependent, "start"), batchEntry(prerequisite, "cancel")},
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-final-state-01"}))
	if err := expect(finalStateBatch, http.StatusOK, ""); err != nil {
		return err
	}
	var finalStateResult batchResult
	if err := json.Unmarshal(finalStateBatch.Body, &finalStateResult); err != nil || len(finalStateResult.Items) != 2 ||
		finalStateResult.Items[0].State != "in_progress" || finalStateResult.Items[0].Blocked || finalStateResult.Items[1].State != "cancelled" {
		return fmt.Errorf("batch final-state dependency resolution failed: %+v err=%v", finalStateResult, err)
	}
	run.write("m2e-batch-atomicity.json", map[string]any{"success_items": result.Items, "exact_retry": true,
		"stale": "version_conflict", "duplicate": "invalid_request", "missing": "not_found", "mixed_project": "not_found",
		"permission": "forbidden", "atomic_final_state_dependency_resolution": finalStateResult.Items})
	run.write("m2e-faults.json", map[string]any{"after_work_row": "zero-residue", "after_work_batch_row": "zero-residue",
		"after_dependency_row": "zero-residue", "after_dependency_event": "zero-residue"})
	run.checks = append(run.checks, "atomic-batch-success-stale-duplicate-missing-mixed-permission-and-fault")
	return nil
}

func (run *runner) currentAuthorityRetry(ctx context.Context, projectItem project) error {
	key := "m2e-current-authority-create"
	created, stored, err := run.createWork(run.agent, projectItem.ID, key, "Current authority retry", nil, run.agentHeaders())
	if err != nil {
		return err
	}
	var lastUsed time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT last_used_at FROM agent_tokens WHERE token_prefix=$1`, agentToken[:16]).Scan(&lastUsed); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET scopes=array_remove(scopes,'work.edit') WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	denied := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items", workInput("Current authority retry", nil),
		merge(run.agentHeaders(), map[string]string{"Idempotency-Key": key}))
	var afterDenied time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT last_used_at FROM agent_tokens WHERE token_prefix=$1`, agentToken[:16]).Scan(&afterDenied); err != nil {
		return err
	}
	if err := expect(denied, http.StatusForbidden, "forbidden"); err != nil || !afterDenied.Equal(lastUsed) {
		return fmt.Errorf("stored retry bypassed current scope or advanced usage: err=%v before=%s after=%s", err, lastUsed, afterDenied)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET scopes=array_append(scopes,'work.edit') WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	replayed := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items", workInput("Current authority retry", nil),
		merge(run.agentHeaders(), map[string]string{"Idempotency-Key": key}))
	if err := expect(replayed, http.StatusCreated, ""); err != nil || !bytes.Equal(replayed.Body, stored.Body) || created.ID == "" {
		return fmt.Errorf("restored authority did not return exact stored response: err=%v", err)
	}
	var beforeExpired time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT last_used_at FROM agent_tokens WHERE token_prefix=$1`, agentToken[:16]).Scan(&beforeExpired); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET expires_at=CURRENT_TIMESTAMP-interval '1 second' WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	expired := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items", workInput("Current authority retry", nil),
		merge(run.agentHeaders(), map[string]string{"Idempotency-Key": key}))
	var afterExpired time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT last_used_at FROM agent_tokens WHERE token_prefix=$1`, agentToken[:16]).Scan(&afterExpired); err != nil {
		return err
	}
	if err := expect(expired, http.StatusUnauthorized, "unauthenticated"); err != nil || !afterExpired.Equal(beforeExpired) {
		return fmt.Errorf("expired exact retry returned stored success or advanced usage: err=%v before=%s after=%s", err, beforeExpired, afterExpired)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET expires_at=CURRENT_TIMESTAMP+interval '1 hour' WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY['00000000-0000-4000-8000-000000000089'::uuid] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	restricted := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items", workInput("Current authority retry", nil),
		merge(run.agentHeaders(), map[string]string{"Idempotency-Key": key}))
	var afterRestricted time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT last_used_at FROM agent_tokens WHERE token_prefix=$1`, agentToken[:16]).Scan(&afterRestricted); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[]::uuid[] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := expect(restricted, http.StatusForbidden, "forbidden"); err != nil || !afterRestricted.Equal(afterExpired) {
		return fmt.Errorf("project-restricted exact retry returned stored success or advanced usage: err=%v before=%s after=%s", err, afterExpired, afterRestricted)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE project_memberships SET role='observer' WHERE project_id=$1 AND principal_id=$2`, projectItem.ID, humanID); err != nil {
		return err
	}
	roleDenied := run.call(run.agent, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items", workInput("Current authority retry", nil),
		merge(run.agentHeaders(), map[string]string{"Idempotency-Key": key}))
	var afterRoleDenied time.Time
	if err := run.db.QueryRowContext(ctx, `SELECT last_used_at FROM agent_tokens WHERE token_prefix=$1`, agentToken[:16]).Scan(&afterRoleDenied); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE project_memberships SET role='owner' WHERE project_id=$1 AND principal_id=$2`, projectItem.ID, humanID); err != nil {
		return err
	}
	if err := expect(roleDenied, http.StatusForbidden, "forbidden"); err != nil || !afterRoleDenied.Equal(afterRestricted) {
		return fmt.Errorf("current-role exact retry returned stored success or advanced usage: err=%v before=%s after=%s", err, afterRestricted, afterRoleDenied)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET revoked_at=CURRENT_TIMESTAMP WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	revoked := run.call(run.agent, http.MethodGet, "/api/v1/work-items/"+created.ID, nil, run.agentHeaders())
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET revoked_at=NULL,expires_at=CURRENT_TIMESTAMP+interval '1 hour' WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := expect(revoked, http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	run.write("m2e-current-authority.json", map[string]any{"scope_revoked_retry": "forbidden", "last_used_unchanged_on_deny": true,
		"authority_restored_exact_retry": true, "expired_retry": "unauthenticated", "project_restricted_retry": "forbidden",
		"current_role_retry": "forbidden", "revoked_token": "unauthenticated"})
	run.checks = append(run.checks, "current-scope-revocation-and-token-revocation-before-idempotency")
	return nil
}

func (run *runner) realtimeEvidence(ctx context.Context, projectItem project) error {
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return err
	}
	stream, err := run.openSSE("")
	if err != nil {
		return err
	}
	ready, err := stream.next(3 * time.Second)
	stream.close()
	if err != nil || ready.Kind != "ready" || ready.ID == "" {
		return fmt.Errorf("invalid initial SSE ready: %+v err=%v", ready, err)
	}
	created, _, err := run.createWork(run.agent, projectItem.ID, "m2e-realtime-work-0001", "Realtime work canary", nil, run.agentHeaders())
	if err != nil {
		return err
	}
	if err := run.waitRealtime(ctx, 10*time.Second); err != nil {
		return err
	}
	resumed, err := run.openSSE(ready.ID)
	if err != nil {
		return err
	}
	defer resumed.close()
	resumeReady, err := resumed.next(3 * time.Second)
	if err != nil || resumeReady.Kind != "ready" {
		return fmt.Errorf("invalid resumed SSE ready: %+v err=%v", resumeReady, err)
	}
	event, err := resumed.next(3 * time.Second)
	if err != nil || event.Kind != "domain-event" {
		return fmt.Errorf("invalid resumed domain event: %+v err=%v", event, err)
	}
	var envelope struct {
		EventType     string `json:"event_type"`
		AggregateType string `json:"aggregate_type"`
		AggregateID   string `json:"aggregate_id"`
	}
	if err := json.Unmarshal(event.Data, &envelope); err != nil {
		return err
	}
	encoded := strings.ToLower(string(event.Data))
	if envelope.EventType != "work_item.created" || envelope.AggregateType != "work_item" || envelope.AggregateID != created.ID ||
		strings.Contains(encoded, "password") || strings.Contains(encoded, "csrf") || strings.Contains(encoded, "bearer") || strings.Contains(encoded, "token") {
		return fmt.Errorf("invalid work realtime envelope: %s", event.Data)
	}
	run.write("m2e-realtime-resume.json", map[string]any{"event_type": envelope.EventType, "aggregate_type": envelope.AggregateType,
		"aggregate_id": envelope.AggregateID, "signed_cursor": event.ID != "", "secret_fields": "absent"})
	run.checks = append(run.checks, "work-ledger-outbox-sse-resume-envelope")
	return nil
}

func (run *runner) foreignBatchDeny(ctx context.Context, projectItem project) error {
	local, _, err := run.createWork(run.human, projectItem.ID, "m2e-foreign-local-work", "Local mixed-org sentinel", nil, run.humanHeaders())
	if err != nil {
		return err
	}
	now := time.Now().UTC().Truncate(time.Microsecond)
	tx, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	seed := []struct {
		query string
		args  []any
	}{
		{`INSERT INTO organizations (id,slug,name,created_at) VALUES ($1,'m2e-foreign','M2E foreign organization',$2)`, []any{foreignOrgID, now}},
		{`INSERT INTO principals (id,kind,display_name,status,human_principal_id,created_at) VALUES ($1,'human','M2E foreign actor','active',NULL,$2)`, []any{foreignActorID, now}},
		{`INSERT INTO organization_memberships (organization_id,principal_id,role,created_at) VALUES ($1,$2,'owner',$3)`, []any{foreignOrgID, foreignActorID, now}},
		{`INSERT INTO projects (id,organization_id,title,outcome,mode,state,version,hypothesis,falsifier,decision_criteria,experiment_bound,created_by,created_at,updated_at)
		 VALUES ($1,$2,'Foreign project','Foreign project remains undisclosed','exploration','proposed',1,'Foreign boundary holds','Any disclosure', '["No disclosure"]'::jsonb,'M2E only',$3,$4,$4)`, []any{foreignProject, foreignOrgID, foreignActorID, now}},
		{`INSERT INTO work_items (id,organization_id,project_id,title,description,state,priority,version,created_by,created_at,updated_at)
		 VALUES ($1,$2,$3,'Foreign work','Must not be discoverable','open','normal',1,$4,$5,$5)`, []any{foreignWork, foreignOrgID, foreignProject, foreignActorID, now}},
	}
	for _, statement := range seed {
		if _, err := tx.ExecContext(ctx, statement.query, statement.args...); err != nil {
			return err
		}
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	before := run.counts(ctx)
	response := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectItem.ID+"/work-items/batch-transition", map[string]any{
		"items": []any{batchEntry(local, "cancel"), batchEntry(workItem{ID: foreignWork, Version: 1}, "cancel")},
	}, merge(run.humanHeaders(), map[string]string{"Idempotency-Key": "m2e-batch-foreign-org"}))
	after := run.counts(ctx)
	if err := expect(response, http.StatusNotFound, "not_found"); err != nil || before != after {
		return fmt.Errorf("mixed-organization batch leaked or wrote residue: err=%v before=%+v after=%+v", err, before, after)
	}
	currentResponse := run.call(run.human, http.MethodGet, "/api/v1/work-items/"+local.ID, nil, run.humanHeaders())
	var current workItem
	if err := expect(currentResponse, http.StatusOK, ""); err != nil {
		return err
	}
	if err := json.Unmarshal(currentResponse.Body, &current); err != nil || current.Version != local.Version || current.State != local.State {
		return fmt.Errorf("mixed-organization batch partially changed local item: %+v err=%v", current, err)
	}
	cleanup, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer cleanup.Rollback()
	for _, query := range []string{
		`ALTER TABLE work_items DISABLE TRIGGER work_item_guard`,
		`DELETE FROM work_items WHERE id='` + foreignWork + `'`,
		`ALTER TABLE work_items ENABLE TRIGGER work_item_guard`,
		`DELETE FROM project_memberships WHERE project_id='` + foreignProject + `'`,
		`DELETE FROM projects WHERE id='` + foreignProject + `'`,
		`DELETE FROM organization_memberships WHERE organization_id='` + foreignOrgID + `'`,
		`DELETE FROM principals WHERE id='` + foreignActorID + `'`,
		`DELETE FROM organizations WHERE id='` + foreignOrgID + `'`,
	} {
		if _, err := cleanup.ExecContext(ctx, query); err != nil {
			return err
		}
	}
	if err := cleanup.Commit(); err != nil {
		return err
	}
	run.write("m2e-cross-organization.json", map[string]any{"response": "not_found", "local_version_unchanged": true,
		"local_state_unchanged": true, "event_outbox_idempotency_counts_unchanged": true})
	run.checks = append(run.checks, "mixed-organization-batch-nondisclosure-zero-residue")
	return nil
}

func (run *runner) verifyRestart(ctx context.Context) error {
	if err := run.login(); err != nil {
		return err
	}
	encoded, err := os.ReadFile(filepath.Join(run.artifacts, "m2e-state.json"))
	if err != nil {
		return err
	}
	var state persistedState
	if err := json.Unmarshal(encoded, &state); err != nil {
		return err
	}
	workResponse := run.call(run.human, http.MethodGet, "/api/v1/work-items/"+state.WorkItemID, nil, run.humanHeaders())
	if err := expect(workResponse, http.StatusOK, ""); err != nil {
		return fmt.Errorf("persisted work after restart: %w", err)
	}
	graphResponse := run.call(run.human, http.MethodGet, "/api/v1/projects/"+state.DependencyProjectID+"/dependency-graph", nil, run.humanHeaders())
	if err := expect(graphResponse, http.StatusOK, ""); err != nil {
		return fmt.Errorf("persisted graph after restart: %w", err)
	}
	if err := run.waitRealtime(ctx, 15*time.Second); err != nil {
		return err
	}
	store, err := app.NewDurableStore(envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	if err != nil {
		return err
	}
	defer store.Close()
	healthy, err := store.Replay(ctx)
	if err != nil || healthy.LiveChecksum != healthy.RebuiltChecksum || healthy.Work < 1 {
		return fmt.Errorf("restart replay mismatch: %+v err=%v", healthy, err)
	}
	findings, err := store.Doctor(ctx)
	if err != nil || len(findings) != 0 {
		return fmt.Errorf("restart doctor not clean: %+v err=%v", findings, err)
	}

	cycleTx, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer cycleTx.Rollback()
	if _, err := cycleTx.ExecContext(ctx, `ALTER TABLE work_item_dependencies DISABLE TRIGGER blocking_dependency_dag`); err != nil {
		return err
	}
	var source, target string
	if err := cycleTx.QueryRowContext(ctx, `SELECT source_work_item_id,target_work_item_id FROM work_item_dependencies WHERE kind='blocks' LIMIT 1`).Scan(&source, &target); err != nil {
		return err
	}
	if _, err := cycleTx.ExecContext(ctx, `INSERT INTO work_item_dependencies
		(id,organization_id,source_work_item_id,target_work_item_id,kind,version,created_by,created_at)
		VALUES ('00000000-0000-4000-8000-000000000088',$1,$2,$3,'blocks',1,$4,CURRENT_TIMESTAMP)`, organizationID, target, source, humanID); err != nil {
		return err
	}
	cycleFindings, err := app.DoctorTx(ctx, cycleTx)
	if err != nil || !hasFinding(cycleFindings, "dependency_cycle") {
		return fmt.Errorf("doctor missed persisted dependency cycle: %+v err=%v", cycleFindings, err)
	}
	if err := cycleTx.Rollback(); err != nil {
		return err
	}

	driftTx, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer driftTx.Rollback()
	if _, err := driftTx.ExecContext(ctx, `ALTER TABLE work_items DISABLE TRIGGER work_item_guard`); err != nil {
		return err
	}
	if _, err := driftTx.ExecContext(ctx, `UPDATE work_items SET title=title || ' drift' WHERE id=$1`, state.WorkItemID); err != nil {
		return err
	}
	driftFindings, err := app.DoctorTx(ctx, driftTx)
	if err != nil || !hasFinding(driftFindings, "projection_checksum_drift") {
		return fmt.Errorf("doctor missed projection drift: %+v err=%v", driftFindings, err)
	}
	if err := driftTx.Rollback(); err != nil {
		return err
	}

	headBefore, err := store.ActiveProjectionHead(ctx)
	if err != nil {
		return err
	}
	var eventID string
	var original []byte
	if err := run.db.QueryRowContext(ctx, `SELECT event_id,payload FROM domain_events WHERE event_type='work_item.created' ORDER BY sequence LIMIT 1`).Scan(&eventID, &original); err != nil {
		return err
	}
	var corrupt map[string]any
	if err := json.Unmarshal(original, &corrupt); err != nil {
		return err
	}
	workPayload, ok := corrupt["work_item"].(map[string]any)
	if !ok {
		return errors.New("work creation payload lacks work_item")
	}
	workPayload["state"] = "invalid"
	corruptJSON, err := json.Marshal(corrupt)
	if err != nil {
		return err
	}
	if err := run.replaceEventPayload(ctx, eventID, corruptJSON); err != nil {
		return err
	}
	_, replayErr := store.Replay(ctx)
	var replayFailure *app.ReplayFailure
	if !errors.As(replayErr, &replayFailure) || replayFailure.Code != "invalid_event_payload" {
		_ = run.replaceEventPayload(ctx, eventID, original)
		return fmt.Errorf("replay accepted corrupt work event: failure=%+v err=%v", replayFailure, replayErr)
	}
	headAfterFailure, err := store.ActiveProjectionHead(ctx)
	if err != nil {
		return err
	}
	if headAfterFailure.RunID != headBefore.RunID || headAfterFailure.LiveChecksum != headBefore.LiveChecksum {
		return fmt.Errorf("failed replay advanced active head: before=%+v after=%+v", headBefore, headAfterFailure)
	}
	if err := run.replaceEventPayload(ctx, eventID, original); err != nil {
		return err
	}
	recovered, err := store.Replay(ctx)
	if err != nil || recovered.LiveChecksum != recovered.RebuiltChecksum {
		return fmt.Errorf("replay did not recover after restoring event: %+v err=%v", recovered, err)
	}
	finalFindings, err := store.Doctor(ctx)
	if err != nil || len(finalFindings) != 0 {
		return fmt.Errorf("final doctor not clean: %+v err=%v", finalFindings, err)
	}
	run.write("m2e-upgrade-integrity.json", map[string]any{"restart_work_status": workResponse.Status, "restart_graph_status": graphResponse.Status,
		"healthy_replay": healthy, "dependency_cycle_detected": true, "projection_drift_detected": true,
		"corrupt_event_failure": replayFailure, "failed_replay_head_unchanged": true, "recovered_replay": recovered, "final_doctor": finalFindings})
	run.checks = append(run.checks, "restart-persistence-replay-failure-head-integrity-and-doctor-corruption")
	run.write("m2e-restart-summary.json", map[string]any{"checks": run.checks, "count": len(run.checks)})
	return nil
}

func (run *runner) replaceEventPayload(ctx context.Context, eventID string, payload []byte) error {
	tx, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.ExecContext(ctx, `ALTER TABLE domain_events DISABLE TRIGGER domain_events_append_only`); err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx, `UPDATE domain_events SET payload=$1::jsonb WHERE event_id=$2`, payload, eventID); err != nil {
		return err
	}
	if _, err := tx.ExecContext(ctx, `ALTER TABLE domain_events ENABLE TRIGGER domain_events_append_only`); err != nil {
		return err
	}
	return tx.Commit()
}

func hasFinding(findings []app.IntegrityFinding, code string) bool {
	for _, finding := range findings {
		if finding.Code == code {
			return true
		}
	}
	return false
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
		_ = response.Body.Close()
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
		err := run.db.QueryRowContext(ctx, `SELECT
			(SELECT count(*) FROM domain_events),COALESCE((SELECT max(sequence) FROM domain_events),0),
			(SELECT count(*) FROM consumer_deliveries WHERE consumer_name='realtime-v1'),
			(SELECT count(*) FROM consumer_deliveries WHERE consumer_name='projection-v1'),
			(SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name='realtime-v1'),
			(SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name='projection-v1')`).Scan(
			&events, &last, &realtimeDelivered, &projectionDelivered, &realtimeCheckpoint, &projectionCheckpoint)
		if err != nil {
			return err
		}
		if events == realtimeDelivered && events == projectionDelivered && last == realtimeCheckpoint && last == projectionCheckpoint {
			return nil
		}
		time.Sleep(50 * time.Millisecond)
	}
	return errors.New("outbox consumers did not deliver and checkpoint every M2E event")
}

func (run *runner) counts(ctx context.Context) counts {
	var value counts
	check(run.db.QueryRowContext(ctx, `SELECT
		(SELECT count(*) FROM work_items),(SELECT count(*) FROM work_item_dependencies),(SELECT count(*) FROM domain_events),
		(SELECT count(*) FROM outbox_records),(SELECT count(*) FROM idempotency_results)`).Scan(
		&value.Work, &value.Dependencies, &value.Events, &value.Outbox, &value.Idempotency))
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
	return snapshot{Status: response.StatusCode, Body: value,
		Headers: map[string]string{"ETag": response.Header.Get("ETag"), "X-Request-ID": response.Header.Get("X-Request-ID")}}
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
			return fmt.Errorf("problem code=%s want=%s body=%s", problem.Code, code, response.Body)
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
