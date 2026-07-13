// Command smoke executes the frozen M1 public transaction and admitted M2A
// durable spine inside the outbound-isolated Compose network.
package main

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
	"net/http"
	"net/http/cookiejar"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"time"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/app"
	"github.com/lib/pq"
)

const (
	organizationID = "00000000-0000-4000-8000-000000000010"
	humanID        = "00000000-0000-4000-8000-000000000001"
	agentID        = "00000000-0000-4000-8000-000000000002"
	agentToken     = "wpa_local_walking_slice_agent_token_00000000000000000001"
	publicOrigin   = "http://localhost:8080"
)

type snapshot struct {
	Status    int               `json:"status"`
	Headers   map[string]string `json:"headers"`
	Body      json.RawMessage   `json:"body"`
	BodyBytes []byte            `json:"-"`
}

type project struct {
	ID               string   `json:"id"`
	OrganizationID   string   `json:"organization_id"`
	Title            string   `json:"title"`
	Outcome          string   `json:"outcome"`
	Mode             string   `json:"mode"`
	State            string   `json:"state"`
	Version          int64    `json:"version"`
	Hypothesis       string   `json:"hypothesis"`
	Falsifier        string   `json:"falsifier"`
	DecisionCriteria []string `json:"decision_criteria"`
	ExperimentBound  string   `json:"experiment_bound"`
}

type decisionRecord struct {
	ID           string   `json:"id"`
	ProjectID    string   `json:"project_id"`
	ActorID      string   `json:"actor_id"`
	ActorKind    string   `json:"actor_kind"`
	PrincipalID  *string  `json:"principal_id"`
	RecordedAt   string   `json:"recorded_at"`
	Kind         string   `json:"kind"`
	Question     string   `json:"question"`
	Choice       string   `json:"choice"`
	Alternatives []string `json:"alternatives"`
	Rationale    string   `json:"rationale"`
	Evidence     []string `json:"evidence"`
	Consequences []string `json:"consequences"`
}

type replayActivityProjection struct {
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

type session struct {
	ActorID   string `json:"actor_id"`
	CSRF      string `json:"csrf_token"`
	Expires   string `json:"expires_at"`
	ActorKind string `json:"actor_kind"`
}

type activity struct {
	EventID          string  `json:"event_id"`
	EventType        string  `json:"event_type"`
	ActorID          string  `json:"actor_id"`
	ActorKind        string  `json:"actor_kind"`
	PrincipalID      *string `json:"principal_id"`
	AggregateVersion int64   `json:"aggregate_version"`
}

type eventGroupEntry struct {
	OrganizationID   string          `json:"organization_id"`
	AggregateType    string          `json:"aggregate_type"`
	EventType        string          `json:"event_type"`
	SchemaVersion    int             `json:"schema_version"`
	AggregateVersion int64           `json:"aggregate_version"`
	Payload          json.RawMessage `json:"payload"`
}

type normalizedEvent struct {
	OrganizationID   string `json:"organization_id"`
	AggregateType    string `json:"aggregate_type"`
	EventType        string `json:"event_type"`
	SchemaVersion    int    `json:"schema_version"`
	AggregateVersion int64  `json:"aggregate_version"`
	Payload          any    `json:"payload"`
}

type normalizedActivityEntry struct {
	EventType        string `json:"event_type"`
	AggregateVersion int64  `json:"aggregate_version"`
}

type normalizedResult struct {
	Project  project           `json:"project"`
	Decision decisionRecord    `json:"decision"`
	Activity []normalizedEvent `json:"event_group"`
}

type domainCounts struct {
	Projects    int `json:"projects"`
	Events      int `json:"events"`
	Decisions   int `json:"decisions"`
	Idempotency int `json:"idempotency"`
}

type runner struct {
	base      string
	artifacts string
	human     *http.Client
	agent     *http.Client
	db        *sql.DB
	database  string
	appDB     string
	checks    []string
}

func main() {
	base := envOr("WORKPLANE_API_BASE", "http://api:8080")
	artifacts := envOr("WORKPLANE_EVIDENCE_DIR", "/evidence")
	if err := os.MkdirAll(artifacts, 0o755); err != nil {
		fatal(err)
	}
	jar, err := cookiejar.New(nil)
	if err != nil {
		fatal(err)
	}
	databaseURL := envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable")
	db, err := sql.Open("postgres", databaseURL)
	if err != nil {
		fatal(err)
	}
	defer db.Close()
	run := &runner{
		base: base, artifacts: artifacts,
		human: &http.Client{Jar: jar, Timeout: 10 * time.Second}, agent: &http.Client{Timeout: 10 * time.Second},
		db: db, database: databaseURL,
		appDB: envOr("WORKPLANE_APP_DATABASE_URL", "postgres://workplane_app:workplane-app-local-only@postgres:5432/workplane?sslmode=disable"),
	}
	if err := run.execute(context.Background()); err != nil {
		fatal(err)
	}
	fmt.Printf("M1 walking slice and M2A durable spine passed: %d load-bearing checks\n", len(run.checks))
}

func (run *runner) execute(ctx context.Context) error {
	login := run.call(run.human, "human-login", http.MethodPost, "/api/v1/session/login",
		map[string]any{"email": "human@workplane.local", "password": "walking-slice-password"}, nil)
	if err := run.expect(login, http.StatusOK, "human login"); err != nil {
		return err
	}
	var authenticated session
	if err := json.Unmarshal(login.Body, &authenticated); err != nil {
		return err
	}
	if authenticated.ActorID != humanID || authenticated.ActorKind != "human" || len(authenticated.CSRF) < 32 {
		return fmt.Errorf("invalid session response")
	}
	run.checks = append(run.checks, "human-session-cookie-and-csrf")
	run.saveRedactedLogin("human-login", login, authenticated)

	humanInput := projectInput("Human/agent M1 parity transaction")
	humanHeaders := map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-create-000000000001"}
	projectFaultHeaders := clone(humanHeaders)
	projectFaultHeaders["X-Workplane-Fault"] = "after-project"
	projectFault := run.call(run.human, "deny-project-transaction-fault", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, projectFaultHeaders)
	if err := expectProblem(projectFault, http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return err
	}
	if err := run.assertCreateRowsAbsent(ctx, humanInput["title"].(string), humanHeaders["Idempotency-Key"]); err != nil {
		return fmt.Errorf("after-project rollback: %w", err)
	}
	run.checks = append(run.checks, "project-create-atomic-rollback-before-retry")
	eventFaultHeaders := clone(humanHeaders)
	eventFaultHeaders["X-Workplane-Fault"] = "after-event"
	eventFault := run.call(run.human, "deny-project-event-outbox-fault", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, eventFaultHeaders)
	if err := expectProblem(eventFault, http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return err
	}
	if err := run.assertCreateRowsAbsent(ctx, humanInput["title"].(string), humanHeaders["Idempotency-Key"]); err != nil {
		return fmt.Errorf("after-event rollback: %w", err)
	}
	run.checks = append(run.checks, "event-outbox-coupling-rollback")

	created := run.call(run.human, "human-project-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, humanHeaders)
	if err := run.expect(created, http.StatusCreated, "human project create"); err != nil {
		return err
	}
	var humanProject project
	if err := json.Unmarshal(created.Body, &humanProject); err != nil {
		return err
	}
	if humanProject.Version != 1 || humanProject.Mode != "exploration" || humanProject.State != "proposed" {
		return fmt.Errorf("invalid created project projection")
	}

	retry := run.call(run.human, "human-project-create-retry", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, humanHeaders)
	if err := exactReplay(created, retry, false); err != nil {
		return fmt.Errorf("create retry: %w", err)
	}
	run.checks = append(run.checks, "create-idempotent-exact-replay")

	changed := projectInput("Changed body under reused key")
	conflict := run.call(run.human, "deny-create-idempotency-conflict", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", changed, humanHeaders)
	if err := expectProblem(conflict, http.StatusConflict, "idempotency_conflict"); err != nil {
		return err
	}
	missingKeyHeaders := map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF}
	if err := expectProblem(run.call(run.human, "deny-create-missing-idempotency", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, missingKeyHeaders), http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.human, "deny-create-missing-csrf", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, map[string]string{"Origin": publicOrigin, "Idempotency-Key": "human-create-000000000002"}), http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.human, "deny-create-wrong-origin", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, map[string]string{"Origin": "https://attacker.invalid", "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-create-000000000003"}), http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.human, "deny-create-cross-organization", http.MethodPost, "/api/v1/orgs/00000000-0000-4000-8000-000000000099/projects", humanInput, map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-create-000000000004"}), http.StatusNotFound, "not_found"); err != nil {
		return err
	}
	run.checks = append(run.checks, "create-denies-before-mutation")

	read := run.call(run.human, "human-project-read", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, nil)
	if err := run.expect(read, http.StatusOK, "human project read"); err != nil {
		return err
	}
	decisionInput := decisionInput()
	decisionPath := "/api/v1/projects/" + humanProject.ID + "/decisions"
	baseDecisionHeaders := map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-decision-00000001"}
	if err := expectProblem(run.call(run.human, "deny-decision-missing-version", http.MethodPost, decisionPath, decisionInput, baseDecisionHeaders), http.StatusPreconditionRequired, "version_conflict"); err != nil {
		return err
	}
	staleHeaders := clone(baseDecisionHeaders)
	staleHeaders["If-Match"] = `"9"`
	staleHeaders["Idempotency-Key"] = "human-decision-00000002"
	if err := expectProblem(run.call(run.human, "deny-decision-stale-version", http.MethodPost, decisionPath, decisionInput, staleHeaders), http.StatusConflict, "version_conflict"); err != nil {
		return err
	}
	decisionFaultHeaders := clone(baseDecisionHeaders)
	decisionFaultHeaders["If-Match"] = `"1"`
	decisionFaultHeaders["Idempotency-Key"] = "human-decision-00000003"
	decisionFaultHeaders["X-Workplane-Fault"] = "after-decision"
	if err := expectProblem(run.call(run.human, "deny-decision-transaction-fault", http.MethodPost, decisionPath, decisionInput, decisionFaultHeaders), http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return err
	}
	afterFault := run.call(run.human, "human-project-after-fault", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, nil)
	var faultProjection project
	_ = json.Unmarshal(afterFault.Body, &faultProjection)
	if faultProjection.Version != 1 {
		return fmt.Errorf("fault injection partially committed version %d", faultProjection.Version)
	}
	decisionEventFaultHeaders := clone(baseDecisionHeaders)
	decisionEventFaultHeaders["If-Match"] = `"1"`
	decisionEventFaultHeaders["Idempotency-Key"] = "human-decision-00000005"
	decisionEventFaultHeaders["X-Workplane-Fault"] = "after-event"
	if err := expectProblem(run.call(run.human, "deny-decision-event-outbox-fault", http.MethodPost, decisionPath, decisionInput, decisionEventFaultHeaders), http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return err
	}
	afterEventFault := run.call(run.human, "human-project-after-event-fault", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, nil)
	_ = json.Unmarshal(afterEventFault.Body, &faultProjection)
	if faultProjection.Version != 1 {
		return fmt.Errorf("event/outbox fault partially committed version %d", faultProjection.Version)
	}
	run.checks = append(run.checks, "decision-event-outbox-coupling-rollback")

	decisionHeaders := clone(baseDecisionHeaders)
	decisionHeaders["If-Match"] = `"1"`
	decisionHeaders["Idempotency-Key"] = "human-decision-00000004"
	decision := run.call(run.human, "human-decision-create", http.MethodPost, decisionPath, decisionInput, decisionHeaders)
	if err := run.expect(decision, http.StatusCreated, "human decision"); err != nil {
		return err
	}
	var humanDecision decisionRecord
	if err := json.Unmarshal(decision.Body, &humanDecision); err != nil {
		return err
	}
	if decision.Headers["ETag"] != `"2"` {
		return fmt.Errorf("decision ETag = %q", decision.Headers["ETag"])
	}
	decisionRetry := run.call(run.human, "human-decision-retry", http.MethodPost, decisionPath, decisionInput, decisionHeaders)
	if err := exactReplay(decision, decisionRetry, true); err != nil {
		return fmt.Errorf("decision retry: %w", err)
	}
	humanActivity := run.activity(run.human, "human-activity", humanProject.ID, nil)
	if err := validateActivity(humanActivity, "human", humanID, nil); err != nil {
		return err
	}
	humanFinalResponse := run.call(run.human, "human-project-final", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, nil)
	if err := run.expect(humanFinalResponse, http.StatusOK, "human final project read"); err != nil {
		return err
	}
	var finalHuman project
	if err := json.Unmarshal(humanFinalResponse.Body, &finalHuman); err != nil {
		return err
	}
	run.checks = append(run.checks, "decision-version-idempotency-atomicity", "human-immutable-attribution")

	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[]::uuid[] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := run.assertAgentCreateAuthority(ctx, 0); err != nil {
		return fmt.Errorf("unrestricted agent create precondition: %w", err)
	}
	agentHeaders := map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-create-000000000001"}
	agentCreated := run.call(run.agent, "agent-project-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", humanInput, agentHeaders)
	if err := run.expect(agentCreated, http.StatusCreated, "agent project create"); err != nil {
		return err
	}
	var agentProject project
	if err := json.Unmarshal(agentCreated.Body, &agentProject); err != nil {
		return err
	}
	agentDecisionHeaders := map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-decision-00000001", "If-Match": `"1"`}
	agentDecision := run.call(run.agent, "agent-decision-create", http.MethodPost, "/api/v1/projects/"+agentProject.ID+"/decisions", decisionInput, agentDecisionHeaders)
	if err := run.expect(agentDecision, http.StatusCreated, "agent decision"); err != nil {
		return err
	}
	var recordedAgentDecision decisionRecord
	if err := json.Unmarshal(agentDecision.Body, &recordedAgentDecision); err != nil {
		return err
	}
	agentRead := run.call(run.agent, "agent-project-read", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken})
	if err := run.expect(agentRead, http.StatusOK, "agent project read"); err != nil {
		return err
	}
	var finalAgent project
	if err := json.Unmarshal(agentRead.Body, &finalAgent); err != nil {
		return err
	}
	agentActivity := run.activity(run.agent, "agent-activity", agentProject.ID, map[string]string{"Authorization": "Bearer " + agentToken})
	principal := humanID
	if err := validateActivity(agentActivity, "agent", agentID, &principal); err != nil {
		return err
	}
	if finalAgent.Version != 2 || finalHuman.Version != 2 {
		return fmt.Errorf("human/agent final versions differ: human=%d agent=%d", finalHuman.Version, finalAgent.Version)
	}
	humanEvents, err := run.loadEventGroup(ctx, humanProject.ID)
	if err != nil {
		return err
	}
	agentEvents, err := run.loadEventGroup(ctx, agentProject.ID)
	if err != nil {
		return err
	}
	normalizedHuman, err := normalizeResult(finalHuman, humanDecision, humanEvents)
	if err != nil {
		return err
	}
	normalizedAgent, err := normalizeResult(finalAgent, recordedAgentDecision, agentEvents)
	if err != nil {
		return err
	}
	if !reflect.DeepEqual(normalizedHuman, normalizedAgent) || !reflect.DeepEqual(normalizeActivity(humanActivity), normalizeActivity(agentActivity)) {
		return fmt.Errorf("human/agent field-level domain or event parity mismatch: human=%+v agent=%+v", normalizedHuman, normalizedAgent)
	}
	run.writeJSON("human-agent-parity.json", map[string]any{"human": normalizedHuman, "agent": normalizedAgent, "excluded": []string{"generated identifiers", "timestamps", "attributable actor identity"}})
	run.checks = append(run.checks, "agent-public-operation-parity", "agent-delegated-attribution")

	mixed := run.call(run.human, "deny-mixed-authentication", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken})
	if err := expectProblem(mixed, http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	malformedMixed := run.call(run.human, "deny-malformed-mixed-authentication", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, map[string]string{"Authorization": "Basic unexpected"})
	if err := expectProblem(malformedMixed, http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	getWithBody := run.call(run.human, "deny-get-with-body", http.MethodGet, "/api/v1/projects/"+humanProject.ID, map[string]string{"mutation": "forbidden"}, nil)
	if err := expectProblem(getWithBody, http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}
	agentWithCSRF := run.call(run.agent, "deny-agent-csrf-transport", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput("Mixed agent transport"), map[string]string{"Authorization": "Bearer " + agentToken, "X-CSRF-Token": "browser-token-is-not-agent-auth", "Idempotency-Key": "agent-create-000000000002"})
	if err := expectProblem(agentWithCSRF, http.StatusBadRequest, "invalid_request"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='observer' WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	beforeDelegatedDenies, err := run.loadDomainCounts(ctx)
	if err != nil {
		return err
	}
	observerCreate := run.call(run.human, "deny-observer-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput("Observer cannot create"), map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-create-000000000005"})
	if err := expectProblem(observerCreate, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	agentObserverTitle := "Delegated observer cannot create"
	agentObserverKey := "agent-create-000000000003"
	agentAuthorization := map[string]string{"Authorization": "Bearer " + agentToken}
	agentObserverCreate := run.call(run.agent, "deny-agent-delegated-observer-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput(agentObserverTitle), map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": agentObserverKey})
	if err := expectProblem(agentObserverCreate, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	agentObserverDecision := run.call(run.agent, "deny-agent-delegated-observer-decision", http.MethodPost, "/api/v1/projects/"+agentProject.ID+"/decisions", decisionInput, map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-decision-00000002", "If-Match": `"2"`})
	if err := expectProblem(agentObserverDecision, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if err := run.expect(run.call(run.agent, "agent-delegated-observer-read", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, agentAuthorization), http.StatusOK, "delegated observer read"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `DELETE FROM organization_memberships WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.agent, "deny-agent-missing-delegated-membership", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, agentAuthorization), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `INSERT INTO organization_memberships (organization_id,principal_id,role,created_at) VALUES ($1,$2,'observer',CURRENT_TIMESTAMP)`, organizationID, humanID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='disabled' WHERE id=$1`, humanID); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.agent, "deny-agent-disabled-delegated-principal", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, agentAuthorization), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='active' WHERE id=$1`, humanID); err != nil {
		return err
	}
	afterDelegatedDenies, err := run.loadDomainCounts(ctx)
	if err != nil {
		return err
	}
	if !reflect.DeepEqual(beforeDelegatedDenies, afterDelegatedDenies) {
		return fmt.Errorf("delegated-authority deny changed domain rows: before=%+v after=%+v", beforeDelegatedDenies, afterDelegatedDenies)
	}
	if err := run.assertCreateRowsAbsent(ctx, agentObserverTitle, agentObserverKey); err != nil {
		return fmt.Errorf("delegated observer create: %w", err)
	}
	run.checks = append(run.checks, "delegated-human-agent-authority-intersection-no-write")
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='owner' WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	restrictedCreateTitle := "Project-restricted token cannot create"
	restrictedCreateKey := "agent-create-000000000004"
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[$1::uuid] WHERE token_prefix=$2`, agentProject.ID, agentToken[:16]); err != nil {
		return err
	}
	if err := run.assertAgentCreateAuthority(ctx, 1); err != nil {
		return fmt.Errorf("project-restricted agent create precondition: %w", err)
	}
	if err := run.expect(run.call(run.agent, "agent-project-read-restricted-allowed", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, agentAuthorization), http.StatusOK, "project-restricted allowed project read"); err != nil {
		return err
	}
	beforeRestrictedCreate, err := run.loadDomainCounts(ctx)
	if err != nil {
		return err
	}
	restrictedCreate := run.call(run.agent, "deny-agent-project-restricted-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput(restrictedCreateTitle), map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": restrictedCreateKey})
	if err := expectProblem(restrictedCreate, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	afterRestrictedCreate, err := run.loadDomainCounts(ctx)
	if err != nil {
		return err
	}
	if !reflect.DeepEqual(beforeRestrictedCreate, afterRestrictedCreate) {
		return fmt.Errorf("project-restricted create changed domain rows: before=%+v after=%+v", beforeRestrictedCreate, afterRestrictedCreate)
	}
	if err := run.assertCreateRowsAbsent(ctx, restrictedCreateTitle, restrictedCreateKey); err != nil {
		return fmt.Errorf("project-restricted create: %w", err)
	}
	run.checks = append(run.checks, "agent-project-restriction-create-deny-no-write")
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET project_ids=ARRAY[]::uuid[] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := run.assertAgentCreateAuthority(ctx, 0); err != nil {
		return fmt.Errorf("restored unrestricted agent create precondition: %w", err)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET scopes=ARRAY['project.read'] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	outOfScope := run.call(run.agent, "deny-agent-out-of-scope", http.MethodPost, "/api/v1/projects/"+agentProject.ID+"/decisions", decisionInput, map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-decision-00000002", "If-Match": `"2"`})
	if err := expectProblem(outOfScope, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET scopes=ARRAY['project.create','project.read','decision.record'] WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET revoked_at=CURRENT_TIMESTAMP WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.agent, "deny-agent-revoked-token", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken}), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE agent_tokens SET revoked_at=NULL WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='disabled' WHERE id=$1`, agentID); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.agent, "deny-agent-disabled", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken}), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='active' WHERE id=$1`, agentID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `DELETE FROM organization_memberships WHERE organization_id=$1 AND principal_id=$2`, organizationID, agentID); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.agent, "deny-agent-missing-membership", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken}), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `INSERT INTO organization_memberships (organization_id,principal_id,role,created_at) VALUES ($1,$2,'member',CURRENT_TIMESTAMP)`, organizationID, agentID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE projects SET state='stopped' WHERE id=$1`, agentProject.ID); err != nil {
		return err
	}
	stateDeny := run.call(run.agent, "deny-decision-terminal-state", http.MethodPost, "/api/v1/projects/"+agentProject.ID+"/decisions", decisionInput, map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-decision-00000003", "If-Match": `"2"`})
	if err := expectProblem(stateDeny, http.StatusConflict, "invariant_violation"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE projects SET state='proposed' WHERE id=$1`, agentProject.ID); err != nil {
		return err
	}
	run.checks = append(run.checks, "auth-scope-revocation-status-state-separation-denies")

	var eventRows, decisions, idempotency, outboxRows, missingOutbox int
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM domain_events`).Scan(&eventRows); err != nil {
		return err
	}
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM decisions`).Scan(&decisions); err != nil {
		return err
	}
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM idempotency_results`).Scan(&idempotency); err != nil {
		return err
	}
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM outbox_records`).Scan(&outboxRows); err != nil {
		return err
	}
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM domain_events event LEFT JOIN outbox_records record ON record.event_id=event.event_id WHERE record.event_id IS NULL`).Scan(&missingOutbox); err != nil {
		return err
	}
	if eventRows != 4 || decisions != 2 || idempotency != 4 || outboxRows != 4 || missingOutbox != 0 {
		return fmt.Errorf("unexpected durable counts events=%d decisions=%d idempotency=%d outbox=%d missing_outbox=%d", eventRows, decisions, idempotency, outboxRows, missingOutbox)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE domain_events SET event_type='tampered' WHERE event_id=(SELECT event_id FROM domain_events LIMIT 1)`); err == nil || !strings.Contains(err.Error(), "append-only") {
		return fmt.Errorf("event ledger update did not fail closed: %v", err)
	}
	run.writeJSON("postgres-ledger.json", map[string]any{"domain_events": eventRows, "decisions": decisions, "idempotency_results": idempotency, "outbox_records": outboxRows, "missing_outbox": missingOutbox, "versions": []int{1, 2}, "append_only_update": "denied"})
	run.checks = append(run.checks, "postgres-contiguous-append-only-ledger")
	if err := run.executeDurable(ctx, []project{finalHuman, finalAgent}, []decisionRecord{humanDecision, recordedAgentDecision}); err != nil {
		return err
	}
	run.writeJSON("smoke-summary.json", map[string]any{"result": "pass", "checks": run.checks, "human_project_id": humanProject.ID, "agent_project_id": agentProject.ID})
	return nil
}

func (run *runner) executeDurable(ctx context.Context, expectedProjects []project, expectedDecisions []decisionRecord) error {
	store, err := app.NewDurableStore(run.database)
	if err != nil {
		return err
	}
	defer store.Close()
	if err := store.Ping(ctx); err != nil {
		return err
	}
	if err := run.waitForConsumer(ctx, "projection-v1", 5*time.Second); err != nil {
		return fmt.Errorf("production outbox consumer: %w", err)
	}
	outboxRows, err := run.loadOutboxRows(ctx)
	if err != nil {
		return err
	}
	if len(outboxRows) != 4 {
		return fmt.Errorf("outbox evidence rows=%d, want 4", len(outboxRows))
	}
	run.writeJSON("m2-outbox-rows.json", map[string]any{"records": outboxRows, "production_consumer": "projection-v1"})

	const crashConsumer = "m2-crash-smoke"
	base := time.Now().UTC().Add(time.Second)
	lease := 250 * time.Millisecond
	timeline := make([]map[string]any, 0)
	claimed, err := store.RunOutboxOnce(ctx, crashConsumer, "crash-worker-a", lease, base, app.OutboxFaultAfterClaim)
	if !errors.Is(err, app.ErrInjectedCrash) || claimed.Published {
		return fmt.Errorf("after-claim crash boundary result=%+v err=%v", claimed, err)
	}
	timeline = append(timeline, deliveryTimeline("crash-after-claim", claimed, err))
	checkpoint, effects, acknowledged, err := run.consumerState(ctx, crashConsumer)
	if err != nil || checkpoint != 0 || effects != 0 || acknowledged != 0 {
		return fmt.Errorf("after-claim state checkpoint=%d effects=%d acknowledged=%d err=%v", checkpoint, effects, acknowledged, err)
	}
	published, err := store.RunOutboxOnce(ctx, crashConsumer, "crash-worker-b", lease, base.Add(300*time.Millisecond), app.OutboxFaultAfterPublish)
	if !errors.Is(err, app.ErrInjectedCrash) || !published.Published || published.Acknowledged {
		return fmt.Errorf("after-publish crash boundary result=%+v err=%v", published, err)
	}
	timeline = append(timeline, deliveryTimeline("crash-after-publish-before-checkpoint", published, err))
	checkpoint, effects, acknowledged, err = run.consumerState(ctx, crashConsumer)
	if err != nil || checkpoint != 0 || effects != 1 || acknowledged != 0 {
		return fmt.Errorf("after-publish state checkpoint=%d effects=%d acknowledged=%d err=%v", checkpoint, effects, acknowledged, err)
	}
	staleFindings, err := store.Doctor(ctx)
	if err != nil || !hasFinding(staleFindings, "stale_checkpoint", crashConsumer) {
		return fmt.Errorf("stale checkpoint was not detected: findings=%+v err=%v", staleFindings, err)
	}
	retried, err := store.RunOutboxOnce(ctx, crashConsumer, "crash-worker-c", lease, base.Add(600*time.Millisecond), app.OutboxFaultNone)
	if err != nil || !retried.Duplicate || !retried.Acknowledged || retried.Checkpoint != retried.EventSequence {
		return fmt.Errorf("duplicate recovery result=%+v err=%v", retried, err)
	}
	timeline = append(timeline, deliveryTimeline("duplicate-retry-checkpoint-and-ack", retried, err))
	completed, err := run.drainConsumer(ctx, store, crashConsumer, "crash-worker-c", base.Add(time.Second))
	if err != nil {
		return err
	}
	for _, item := range completed {
		timeline = append(timeline, deliveryTimeline("ordered-drain", item, nil))
	}
	run.writeJSON("m2-crash-retry-timeline.json", map[string]any{"consumer": crashConsumer, "timeline": timeline})

	const reorderConsumer = "m2-reorder-smoke"
	reorderBase := base.Add(10 * time.Second)
	reordered := make([]map[string]any, 0)
	first, err := store.RunOutboxOnce(ctx, reorderConsumer, "reorder-worker-a", lease, reorderBase, app.OutboxFaultAfterClaim)
	if !errors.Is(err, app.ErrInjectedCrash) {
		return fmt.Errorf("reorder first lease: %w", err)
	}
	reordered = append(reordered, deliveryTimeline("lease-earliest-then-crash", first, err))
	second, err := store.RunOutboxOnce(ctx, reorderConsumer, "reorder-worker-b", lease, reorderBase.Add(10*time.Millisecond), app.OutboxFaultNone)
	if !errors.Is(err, app.ErrOutOfOrder) || !second.Published || second.Acknowledged || second.EventSequence <= first.EventSequence {
		return fmt.Errorf("reordered delivery was not held: first=%+v second=%+v err=%v", first, second, err)
	}
	reordered = append(reordered, deliveryTimeline("publish-higher-hold-checkpoint", second, err))
	recoveredFirst, err := store.RunOutboxOnce(ctx, reorderConsumer, "reorder-worker-c", lease, reorderBase.Add(300*time.Millisecond), app.OutboxFaultNone)
	if err != nil || recoveredFirst.EventID != first.EventID || !recoveredFirst.Acknowledged {
		return fmt.Errorf("reordered earliest recovery result=%+v err=%v", recoveredFirst, err)
	}
	reordered = append(reordered, deliveryTimeline("recover-earliest", recoveredFirst, err))
	recoveredSecond, err := store.RunOutboxOnce(ctx, reorderConsumer, "reorder-worker-c", lease, reorderBase.Add(600*time.Millisecond), app.OutboxFaultNone)
	if err != nil || recoveredSecond.EventID != second.EventID || !recoveredSecond.Duplicate || !recoveredSecond.Acknowledged {
		return fmt.Errorf("reordered duplicate recovery result=%+v err=%v", recoveredSecond, err)
	}
	reordered = append(reordered, deliveryTimeline("recover-higher-as-identifiable-duplicate", recoveredSecond, err))
	remainder, err := run.drainConsumer(ctx, store, reorderConsumer, "reorder-worker-c", reorderBase.Add(time.Second))
	if err != nil {
		return err
	}
	for _, item := range remainder {
		reordered = append(reordered, deliveryTimeline("ordered-drain", item, nil))
	}
	run.writeJSON("m2-reordered-delivery.json", map[string]any{"consumer": reorderConsumer, "timeline": reordered})

	checkpointEvidence, err := run.loadCheckpointEvidence(ctx)
	if err != nil {
		return err
	}
	run.writeJSON("m2-checkpoint-transitions.json", map[string]any{"checkpoints": checkpointEvidence})
	if _, err := run.db.ExecContext(ctx, `UPDATE consumer_checkpoints SET last_sequence=0,last_event_id=NULL WHERE consumer_name=$1`, crashConsumer); err == nil || !strings.Contains(err.Error(), "cannot move backwards") {
		return fmt.Errorf("non-monotonic checkpoint update did not fail closed: %v", err)
	}

	firstReplay, err := store.Replay(ctx)
	if err != nil {
		return fmt.Errorf("first deterministic replay: %w", err)
	}
	secondReplay, err := store.Replay(ctx)
	if err != nil {
		return fmt.Errorf("second deterministic replay: %w", err)
	}
	if firstReplay.LiveChecksum != firstReplay.RebuiltChecksum || secondReplay.LiveChecksum != secondReplay.RebuiltChecksum ||
		firstReplay.RebuiltChecksum != secondReplay.RebuiltChecksum || firstReplay.Projects != 2 || firstReplay.Decisions != 2 || firstReplay.Activity != 4 {
		return fmt.Errorf("non-deterministic replay reports: first=%+v second=%+v", firstReplay, secondReplay)
	}
	installedReplay, err := run.assertReplayPersistence(ctx, secondReplay, expectedProjects, expectedDecisions)
	if err != nil {
		return err
	}
	run.writeJSON("m2-replay-checksums.json", map[string]any{
		"first": firstReplay, "second": secondReplay, "deterministic": true, "installed_generation": installedReplay,
	})
	baselineFindings, err := store.Doctor(ctx)
	if err != nil || len(baselineFindings) != 0 {
		return fmt.Errorf("healthy durable spine failed integrity doctor: findings=%+v err=%v", baselineFindings, err)
	}
	orphanEvidence, err := run.assertOrphanDecisionFailsClosed(ctx, store, secondReplay, expectedProjects[0].ID)
	if err != nil {
		return err
	}
	run.writeJSON("m2-orphan-decision-failure.json", orphanEvidence)
	baselineFindings, err = store.Doctor(ctx)
	if err != nil || len(baselineFindings) != 0 {
		return fmt.Errorf("durable spine did not recover after orphan corruption rollback: findings=%+v err=%v", baselineFindings, err)
	}

	negatives, err := run.integrityNegatives(ctx)
	if err != nil {
		return err
	}
	run.writeJSON("m2-integrity-doctor-negatives.json", negatives)
	privilegeEvidence, err := run.verifyApplicationImmutability(ctx)
	if err != nil {
		return err
	}
	run.writeJSON("m2-application-credentials.json", privilegeEvidence)

	headBefore, err := store.ActiveProjectionHead(ctx)
	if err != nil {
		return err
	}
	unknownSequence, unknownEventID, err := run.seedUnknownEvent(ctx)
	if err != nil {
		return err
	}
	_, replayErr := store.Replay(ctx)
	var replayFailure *app.ReplayFailure
	if !errors.As(replayErr, &replayFailure) || replayFailure.Code != "unknown_event_schema" ||
		replayFailure.Sequence != unknownSequence || replayFailure.EventID != unknownEventID || replayFailure.SchemaVersion != 2 {
		return fmt.Errorf("unknown schema did not stop exactly: failure=%+v err=%v", replayFailure, replayErr)
	}
	failurePersistence, err := run.assertReplayFailurePersistence(ctx, replayFailure)
	if err != nil {
		return err
	}
	headAfter, err := store.ActiveProjectionHead(ctx)
	if err != nil {
		return err
	}
	if headAfter.RunID != headBefore.RunID || headAfter.RebuiltChecksum != headBefore.RebuiltChecksum || headAfter.LastSequence != headBefore.LastSequence {
		return fmt.Errorf("unknown schema replaced last known-good head: before=%+v after=%+v", headBefore, headAfter)
	}
	unknownFindings, err := store.Doctor(ctx)
	if err != nil || !hasFinding(unknownFindings, "unknown_event_schema", "") {
		return fmt.Errorf("integrity doctor missed unknown schema: findings=%+v err=%v", unknownFindings, err)
	}
	run.writeJSON("m2-unknown-schema-failure.json", map[string]any{
		"failure": replayFailure, "persisted_failure": failurePersistence,
		"head_before": headBefore, "head_after": headAfter, "last_known_good_preserved": true,
	})
	run.checks = append(run.checks,
		"transactional-outbox-one-per-event", "bounded-lease-crash-retry", "duplicate-identifiable-idempotent-delivery",
		"monotonic-checkpoint-and-reorder-recovery", "deterministic-shadow-replay-checksum",
		"installed-replay-shadow-persistence", "orphan-decision-replay-and-integrity-fail-closed",
		"unknown-schema-exact-stop-preserves-head", "failed-replay-row-persistence",
		"integrity-doctor-seeded-negatives", "complete-outbox-envelope-tamper-detection", "application-ledger-outbox-immutability")
	return nil
}

func (run *runner) assertReplayPersistence(ctx context.Context, report app.ReplayReport, expectedProjects []project, expectedDecisions []decisionRecord) (map[string]any, error) {
	var status, liveChecksum, rebuiltChecksum string
	var finished bool
	var lastSequence int64
	var projectsCount, decisionsCount, activityCount int
	if err := run.db.QueryRowContext(ctx, `SELECT status,finished_at IS NOT NULL,last_sequence,projects_count,
		decisions_count,activity_count,live_checksum,rebuilt_checksum FROM projection_replay_runs WHERE id=$1`, report.RunID).Scan(
		&status, &finished, &lastSequence, &projectsCount, &decisionsCount, &activityCount, &liveChecksum, &rebuiltChecksum); err != nil {
		return nil, fmt.Errorf("read installed replay run: %w", err)
	}
	if status != "succeeded" || !finished || lastSequence != report.LastSequence || projectsCount != report.Projects ||
		decisionsCount != report.Decisions || activityCount != report.Activity || liveChecksum != report.LiveChecksum ||
		rebuiltChecksum != report.RebuiltChecksum {
		return nil, fmt.Errorf("installed replay run differs from report: status=%s finished=%t sequence=%d counts=%d/%d/%d checksums=%s/%s report=%+v",
			status, finished, lastSequence, projectsCount, decisionsCount, activityCount, liveChecksum, rebuiltChecksum, report)
	}

	projectRows, err := run.db.QueryContext(ctx, `SELECT organization_id,project_id,projection
		FROM replay_project_projections WHERE run_id=$1 ORDER BY organization_id,project_id`, report.RunID)
	if err != nil {
		return nil, err
	}
	installedProjects := make([]project, 0)
	for projectRows.Next() {
		var organizationID, projectID string
		var encoded []byte
		if err := projectRows.Scan(&organizationID, &projectID, &encoded); err != nil {
			_ = projectRows.Close()
			return nil, err
		}
		var item project
		if err := json.Unmarshal(encoded, &item); err != nil {
			_ = projectRows.Close()
			return nil, err
		}
		if item.ID != projectID || item.OrganizationID != organizationID {
			_ = projectRows.Close()
			return nil, fmt.Errorf("replay project identity columns differ from projection: organization=%s project=%s projection=%+v", organizationID, projectID, item)
		}
		installedProjects = append(installedProjects, item)
	}
	if err := projectRows.Close(); err != nil {
		return nil, err
	}

	decisionRows, err := run.db.QueryContext(ctx, `SELECT organization_id,decision_id,project_id,projection
		FROM replay_decision_projections WHERE run_id=$1 ORDER BY project_id,decision_id`, report.RunID)
	if err != nil {
		return nil, err
	}
	installedDecisions := make([]decisionRecord, 0)
	for decisionRows.Next() {
		var rowOrganizationID, decisionID, projectID string
		var encoded []byte
		if err := decisionRows.Scan(&rowOrganizationID, &decisionID, &projectID, &encoded); err != nil {
			_ = decisionRows.Close()
			return nil, err
		}
		var item decisionRecord
		if err := json.Unmarshal(encoded, &item); err != nil {
			_ = decisionRows.Close()
			return nil, err
		}
		if rowOrganizationID != organizationID || item.ID != decisionID || item.ProjectID != projectID {
			_ = decisionRows.Close()
			return nil, fmt.Errorf("replay decision identity columns differ from projection: organization=%s decision=%s project=%s projection=%+v", rowOrganizationID, decisionID, projectID, item)
		}
		installedDecisions = append(installedDecisions, item)
	}
	if err := decisionRows.Close(); err != nil {
		return nil, err
	}

	activityRows, err := run.db.QueryContext(ctx, `SELECT sequence,event_id,projection
		FROM replay_activity_projections WHERE run_id=$1 ORDER BY sequence`, report.RunID)
	if err != nil {
		return nil, err
	}
	installedActivity := make([]replayActivityProjection, 0)
	for activityRows.Next() {
		var sequence int64
		var eventID string
		var encoded []byte
		if err := activityRows.Scan(&sequence, &eventID, &encoded); err != nil {
			_ = activityRows.Close()
			return nil, err
		}
		var item replayActivityProjection
		if err := json.Unmarshal(encoded, &item); err != nil {
			_ = activityRows.Close()
			return nil, err
		}
		if item.Sequence != sequence || item.EventID != eventID {
			_ = activityRows.Close()
			return nil, fmt.Errorf("replay activity identity columns differ from projection: sequence=%d event=%s projection=%+v", sequence, eventID, item)
		}
		installedActivity = append(installedActivity, item)
	}
	if err := activityRows.Close(); err != nil {
		return nil, err
	}

	ledgerRows, err := run.db.QueryContext(ctx, `SELECT sequence,event_id,event_type,schema_version,organization_id,
		aggregate_type,aggregate_id,aggregate_version,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at
		FROM domain_events WHERE sequence<=$1 ORDER BY sequence`, report.LastSequence)
	if err != nil {
		return nil, err
	}
	expectedActivity := make([]replayActivityProjection, 0)
	for ledgerRows.Next() {
		var item replayActivityProjection
		var principalID sql.NullString
		var occurredAt time.Time
		if err := ledgerRows.Scan(&item.Sequence, &item.EventID, &item.EventType, &item.SchemaVersion, &item.OrganizationID,
			&item.AggregateType, &item.AggregateID, &item.AggregateVersion, &item.ActorKind, &item.ActorID,
			&principalID, &item.CommandID, &item.RequestID, &occurredAt); err != nil {
			_ = ledgerRows.Close()
			return nil, err
		}
		if principalID.Valid {
			item.PrincipalID = &principalID.String
		}
		item.OccurredAt = occurredAt.UTC().Format("2006-01-02T15:04:05.000000Z")
		expectedActivity = append(expectedActivity, item)
	}
	if err := ledgerRows.Close(); err != nil {
		return nil, err
	}

	wantedProjects := append([]project(nil), expectedProjects...)
	wantedDecisions := append([]decisionRecord(nil), expectedDecisions...)
	sort.Slice(wantedProjects, func(i, j int) bool {
		if wantedProjects[i].OrganizationID != wantedProjects[j].OrganizationID {
			return wantedProjects[i].OrganizationID < wantedProjects[j].OrganizationID
		}
		return wantedProjects[i].ID < wantedProjects[j].ID
	})
	sort.Slice(wantedDecisions, func(i, j int) bool {
		if wantedDecisions[i].ProjectID != wantedDecisions[j].ProjectID {
			return wantedDecisions[i].ProjectID < wantedDecisions[j].ProjectID
		}
		return wantedDecisions[i].ID < wantedDecisions[j].ID
	})
	if len(installedProjects) != report.Projects || len(installedDecisions) != report.Decisions || len(installedActivity) != report.Activity ||
		!reflect.DeepEqual(installedProjects, wantedProjects) || !reflect.DeepEqual(installedDecisions, wantedDecisions) ||
		!reflect.DeepEqual(installedActivity, expectedActivity) {
		return nil, fmt.Errorf("installed replay shadows differ: projects=%+v want=%+v decisions=%+v want=%+v activity=%+v want=%+v",
			installedProjects, wantedProjects, installedDecisions, wantedDecisions, installedActivity, expectedActivity)
	}
	return map[string]any{
		"run_id": report.RunID, "status": status, "last_sequence": lastSequence,
		"projects": installedProjects, "decisions": installedDecisions, "activity": installedActivity,
	}, nil
}

func (run *runner) assertReplayFailurePersistence(ctx context.Context, failure *app.ReplayFailure) (map[string]any, error) {
	var status, code, detail string
	var finished bool
	var lastSequence, failedSequence int64
	var failedEventID sql.NullString
	if err := run.db.QueryRowContext(ctx, `SELECT status,finished_at IS NOT NULL,last_sequence,failed_sequence,
		failed_event_id,failure_code,failure_detail FROM projection_replay_runs WHERE id=$1`, failure.RunID).Scan(
		&status, &finished, &lastSequence, &failedSequence, &failedEventID, &code, &detail); err != nil {
		return nil, fmt.Errorf("read failed replay run: %w", err)
	}
	if status != "failed" || !finished || lastSequence != failure.Sequence || failedSequence != failure.Sequence ||
		failedEventID.String != failure.EventID || code != failure.Code || detail != failure.Detail {
		return nil, fmt.Errorf("failed replay row differs from failure: status=%s finished=%t sequence=%d/%d event=%s code=%s detail=%q failure=%+v",
			status, finished, lastSequence, failedSequence, failedEventID.String, code, detail, failure)
	}
	return map[string]any{
		"run_id": failure.RunID, "status": status, "last_sequence": lastSequence, "failed_sequence": failedSequence,
		"failed_event_id": failedEventID.String, "failure_code": code, "failure_detail": detail,
	}, nil
}

func (run *runner) assertOrphanDecisionFailsClosed(ctx context.Context, store *app.DurableStore, expectedHead app.ReplayReport, projectID string) (evidence map[string]any, returnedErr error) {
	const orphanDecisionID = "00000000-0000-4000-8000-000000000880"
	if _, err := run.db.ExecContext(ctx, `ALTER TABLE decisions DISABLE TRIGGER decisions_append_only`); err != nil {
		return nil, fmt.Errorf("disable decision immutability for corruption fixture: %w", err)
	}
	if _, err := run.db.ExecContext(ctx, `INSERT INTO decisions
		(id,organization_id,project_id,kind,question,choice,alternatives,rationale,evidence,consequences,actor_id,recorded_at)
		VALUES ($1,$2,$3,'continue','Orphan decision corruption','continue','[]','corruption fixture','[]','[]',$4,$5)`,
		orphanDecisionID, organizationID, projectID, humanID, time.Now().UTC()); err != nil {
		_, _ = run.db.ExecContext(context.Background(), `ALTER TABLE decisions ENABLE TRIGGER decisions_append_only`)
		return nil, fmt.Errorf("seed orphan decision: %w", err)
	}
	defer func() {
		_, cleanupErr := run.db.ExecContext(context.Background(), `DELETE FROM decisions WHERE id=$1`, orphanDecisionID)
		_, enableErr := run.db.ExecContext(context.Background(), `ALTER TABLE decisions ENABLE TRIGGER decisions_append_only`)
		if cleanupErr != nil || enableErr != nil {
			cleanupFailure := errors.Join(cleanupErr, enableErr)
			if returnedErr == nil {
				returnedErr = fmt.Errorf("restore decisions after orphan fixture: %w", cleanupFailure)
			} else {
				returnedErr = errors.Join(returnedErr, fmt.Errorf("restore decisions after orphan fixture: %w", cleanupFailure))
			}
		}
	}()

	_, replayErr := store.Replay(ctx)
	var failure *app.ReplayFailure
	if !errors.As(replayErr, &failure) || failure.Code != "checksum_mismatch" || failure.Sequence != expectedHead.LastSequence {
		return nil, fmt.Errorf("orphan decision replay did not fail closed: failure=%+v err=%v", failure, replayErr)
	}
	persisted, err := run.assertReplayFailurePersistence(ctx, failure)
	if err != nil {
		return nil, err
	}
	currentHead, err := store.ActiveProjectionHead(ctx)
	if err != nil {
		return nil, err
	}
	if currentHead.RunID != expectedHead.RunID || currentHead.LastSequence != expectedHead.LastSequence ||
		currentHead.RebuiltChecksum != expectedHead.RebuiltChecksum || currentHead.Projects != expectedHead.Projects ||
		currentHead.Decisions != expectedHead.Decisions || currentHead.Activity != expectedHead.Activity {
		return nil, fmt.Errorf("orphan decision replaced last known-good head: before=%+v after=%+v", expectedHead, currentHead)
	}
	findings, err := store.Doctor(ctx)
	if err != nil || !hasFinding(findings, "decision_without_event", "") || !hasFinding(findings, "projection_checksum_drift", "") {
		return nil, fmt.Errorf("orphan decision corruption was not reported: findings=%+v err=%v", findings, err)
	}
	return map[string]any{
		"decision_id": orphanDecisionID, "failure": failure, "persisted_failure": persisted,
		"head_before": expectedHead, "head_after": currentHead, "findings": findings,
	}, nil
}

func (run *runner) waitForConsumer(ctx context.Context, consumer string, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		var remaining int
		if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM outbox_delivery_state WHERE consumer_name=$1 AND delivered_at IS NULL`, consumer).Scan(&remaining); err != nil {
			return err
		}
		if remaining == 0 {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("%d outbox records remain undelivered", remaining)
		}
		time.Sleep(25 * time.Millisecond)
	}
}

func (run *runner) drainConsumer(ctx context.Context, store *app.DurableStore, consumer, worker string, at time.Time) ([]app.DeliveryResult, error) {
	results := make([]app.DeliveryResult, 0)
	for attempt := 0; attempt < 32; attempt++ {
		result, err := store.RunOutboxOnce(ctx, consumer, worker, 250*time.Millisecond, at.Add(time.Duration(attempt)*time.Second), app.OutboxFaultNone)
		if errors.Is(err, app.ErrNoOutbox) {
			return results, nil
		}
		if err != nil {
			return nil, fmt.Errorf("drain consumer %s: result=%+v err=%w", consumer, result, err)
		}
		results = append(results, result)
	}
	return nil, fmt.Errorf("consumer %s did not drain within bound", consumer)
}

func (run *runner) consumerState(ctx context.Context, consumer string) (int64, int, int, error) {
	var checkpoint int64
	var effects, acknowledged int
	err := run.db.QueryRowContext(ctx, `SELECT
		(SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name=$1),
		(SELECT count(*) FROM consumer_deliveries WHERE consumer_name=$1),
		(SELECT count(*) FROM outbox_delivery_state WHERE consumer_name=$1 AND delivered_at IS NOT NULL)`, consumer).Scan(&checkpoint, &effects, &acknowledged)
	return checkpoint, effects, acknowledged, err
}

func deliveryTimeline(step string, result app.DeliveryResult, err error) map[string]any {
	errorName := ""
	if err != nil {
		errorName = err.Error()
	}
	return map[string]any{"step": step, "result": result, "error": errorName}
}

func hasFinding(findings []app.IntegrityFinding, code, consumer string) bool {
	for _, finding := range findings {
		if finding.Code == code && (consumer == "" || finding.Consumer == consumer) {
			return true
		}
	}
	return false
}

func (run *runner) loadOutboxRows(ctx context.Context) ([]map[string]any, error) {
	rows, err := run.db.QueryContext(ctx, `SELECT event_sequence,event_id,topic,encode(payload_sha256,'hex'),
		payload->>'event_type',payload->>'aggregate_id',payload->>'aggregate_version'
		FROM outbox_records ORDER BY event_sequence`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := make([]map[string]any, 0)
	for rows.Next() {
		var sequence int64
		var eventID, topic, digest, eventType, aggregateID, aggregateVersion string
		if err := rows.Scan(&sequence, &eventID, &topic, &digest, &eventType, &aggregateID, &aggregateVersion); err != nil {
			return nil, err
		}
		result = append(result, map[string]any{"event_sequence": sequence, "event_id": eventID, "topic": topic,
			"payload_sha256": digest, "event_type": eventType, "aggregate_id": aggregateID, "aggregate_version": aggregateVersion})
	}
	return result, rows.Err()
}

func (run *runner) loadCheckpointEvidence(ctx context.Context) ([]map[string]any, error) {
	rows, err := run.db.QueryContext(ctx, `SELECT checkpoint.consumer_name,checkpoint.last_sequence,checkpoint.last_event_id,
		(SELECT count(*) FROM consumer_deliveries delivery WHERE delivery.consumer_name=checkpoint.consumer_name),
		(SELECT count(*) FROM outbox_delivery_attempts attempt WHERE attempt.consumer_name=checkpoint.consumer_name),
		(SELECT count(*) FROM outbox_delivery_attempts attempt WHERE attempt.consumer_name=checkpoint.consumer_name AND attempt.duplicate)
		FROM consumer_checkpoints checkpoint ORDER BY checkpoint.consumer_name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := make([]map[string]any, 0)
	for rows.Next() {
		var consumer string
		var sequence int64
		var eventID sql.NullString
		var deliveries, attempts, duplicates int
		if err := rows.Scan(&consumer, &sequence, &eventID, &deliveries, &attempts, &duplicates); err != nil {
			return nil, err
		}
		result = append(result, map[string]any{"consumer": consumer, "last_sequence": sequence, "last_event_id": eventID.String,
			"logical_deliveries": deliveries, "publish_attempts": attempts, "duplicate_attempts": duplicates})
	}
	return result, rows.Err()
}

func (run *runner) integrityNegatives(ctx context.Context) (map[string]any, error) {
	result := make(map[string]any)
	gap, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}
	if _, err := gap.ExecContext(ctx, `ALTER TABLE domain_events DISABLE TRIGGER domain_events_append_only`); err != nil {
		_ = gap.Rollback()
		return nil, err
	}
	if _, err := gap.ExecContext(ctx, `UPDATE domain_events SET aggregate_version=3 WHERE event_id=(SELECT event_id FROM domain_events WHERE aggregate_version=2 ORDER BY sequence LIMIT 1)`); err != nil {
		_ = gap.Rollback()
		return nil, err
	}
	gapFindings, err := app.DoctorTx(ctx, gap)
	_ = gap.Rollback()
	if err != nil || !hasFinding(gapFindings, "event_version_gap", "") {
		return nil, fmt.Errorf("seeded event gap not detected: findings=%+v err=%v", gapFindings, err)
	}
	result["event_gap"] = gapFindings

	duplicate, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}
	var uniqueConstraint string
	if err := duplicate.QueryRowContext(ctx, `SELECT conname FROM pg_constraint
		WHERE conrelid='domain_events'::regclass AND contype='u' AND pg_get_constraintdef(oid) LIKE 'UNIQUE (aggregate_type, aggregate_id, aggregate_version)%'`).Scan(&uniqueConstraint); err != nil {
		_ = duplicate.Rollback()
		return nil, err
	}
	if _, err := duplicate.ExecContext(ctx, `ALTER TABLE domain_events DROP CONSTRAINT `+pq.QuoteIdentifier(uniqueConstraint)); err != nil {
		_ = duplicate.Rollback()
		return nil, err
	}
	if _, err := duplicate.ExecContext(ctx, `ALTER TABLE domain_events DISABLE TRIGGER domain_events_append_only`); err != nil {
		_ = duplicate.Rollback()
		return nil, err
	}
	if _, err := duplicate.ExecContext(ctx, `UPDATE domain_events SET aggregate_version=1 WHERE event_id=(SELECT event_id FROM domain_events WHERE aggregate_version=2 ORDER BY sequence LIMIT 1)`); err != nil {
		_ = duplicate.Rollback()
		return nil, err
	}
	duplicateFindings, err := app.DoctorTx(ctx, duplicate)
	_ = duplicate.Rollback()
	if err != nil || !hasFinding(duplicateFindings, "duplicate_aggregate_version", "") {
		return nil, fmt.Errorf("seeded duplicate version not detected: findings=%+v err=%v", duplicateFindings, err)
	}
	result["duplicate_aggregate_version"] = duplicateFindings

	envelopeTamper, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}
	if _, err := envelopeTamper.ExecContext(ctx, `ALTER TABLE outbox_records DISABLE TRIGGER outbox_records_immutable`); err != nil {
		_ = envelopeTamper.Rollback()
		return nil, err
	}
	if _, err := envelopeTamper.ExecContext(ctx, `WITH target AS (
		SELECT event_id,jsonb_set(payload,'{actor_id}',to_jsonb('00000000-0000-4000-8000-000000008888'::text),false) changed
		FROM outbox_records ORDER BY event_sequence LIMIT 1
	) UPDATE outbox_records record SET payload=target.changed,
		payload_sha256=digest(convert_to(target.changed::text,'UTF8'),'sha256')
		FROM target WHERE record.event_id=target.event_id`); err != nil {
		_ = envelopeTamper.Rollback()
		return nil, err
	}
	envelopeFindings, err := app.DoctorTx(ctx, envelopeTamper)
	_ = envelopeTamper.Rollback()
	if err != nil || !hasFinding(envelopeFindings, "outbox_payload_mismatch", "") {
		return nil, fmt.Errorf("recomputed-digest outbox envelope tamper not detected: findings=%+v err=%v", envelopeFindings, err)
	}
	result["recomputed_digest_outbox_envelope_tamper"] = envelopeFindings

	drift, err := run.db.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}
	if _, err := drift.ExecContext(ctx, `UPDATE projects SET title=title||' drift' WHERE id=(SELECT id FROM projects ORDER BY id LIMIT 1)`); err != nil {
		_ = drift.Rollback()
		return nil, err
	}
	driftFindings, err := app.DoctorTx(ctx, drift)
	_ = drift.Rollback()
	if err != nil || !hasFinding(driftFindings, "projection_checksum_drift", "") {
		return nil, fmt.Errorf("seeded checksum drift not detected: findings=%+v err=%v", driftFindings, err)
	}
	result["checksum_drift"] = driftFindings
	result["non_monotonic_checkpoint"] = "database trigger rejected backwards movement"
	return result, nil
}

func (run *runner) verifyApplicationImmutability(ctx context.Context) (map[string]any, error) {
	db, err := sql.Open("postgres", run.appDB)
	if err != nil {
		return nil, err
	}
	defer db.Close()
	if err := db.PingContext(ctx); err != nil {
		return nil, err
	}
	_, ledgerErr := db.ExecContext(ctx, `UPDATE domain_events SET event_type=event_type WHERE event_id=(SELECT event_id FROM domain_events LIMIT 1)`)
	_, outboxErr := db.ExecContext(ctx, `UPDATE outbox_records SET payload=payload WHERE event_id=(SELECT event_id FROM outbox_records LIMIT 1)`)
	if ledgerErr == nil || outboxErr == nil || !strings.Contains(ledgerErr.Error(), "permission denied") || !strings.Contains(outboxErr.Error(), "permission denied") {
		return nil, fmt.Errorf("application immutability grants failed: ledger=%v outbox=%v", ledgerErr, outboxErr)
	}
	return map[string]any{"role": "workplane_app", "event_ledger_update": "denied", "outbox_payload_update": "denied"}, nil
}

func (run *runner) seedUnknownEvent(ctx context.Context) (int64, string, error) {
	const (
		eventID   = "00000000-0000-4000-8000-000000000900"
		aggregate = "00000000-0000-4000-8000-000000000901"
		commandID = "00000000-0000-4000-8000-000000000902"
	)
	tx, err := run.db.BeginTx(ctx, &sql.TxOptions{Isolation: sql.LevelSerializable})
	if err != nil {
		return 0, "", err
	}
	defer tx.Rollback()
	occurredAt := time.Now().UTC()
	payload := json.RawMessage(`{"future":"unknown"}`)
	var sequence int64
	if err := tx.QueryRowContext(ctx, `INSERT INTO domain_events
		(event_id,organization_id,aggregate_type,aggregate_id,aggregate_version,event_type,schema_version,
		 actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload)
		VALUES ($1,$2,'project',$3,1,'project.future',2,'human',$4,NULL,$5,'m2-unknown-schema',$6,$7)
		RETURNING sequence`, eventID, organizationID, aggregate, humanID, commandID, occurredAt, payload).Scan(&sequence); err != nil {
		return 0, "", err
	}
	if err := tx.Commit(); err != nil {
		return 0, "", err
	}
	return sequence, eventID, nil
}

func (run *runner) activity(client *http.Client, name, projectID string, headers map[string]string) []activity {
	response := run.call(client, name, http.MethodGet, "/api/v1/projects/"+projectID+"/activity", nil, headers)
	if err := run.expect(response, http.StatusOK, name); err != nil {
		fatal(err)
	}
	var events []activity
	if err := json.Unmarshal(response.Body, &events); err != nil {
		fatal(err)
	}
	return events
}

func validateActivity(events []activity, kind, actorID string, principalID *string) error {
	if len(events) != 2 || events[0].EventType != "project.created" || events[1].EventType != "decision.recorded" || events[0].AggregateVersion != 1 || events[1].AggregateVersion != 2 {
		return fmt.Errorf("non-contiguous activity: %+v", events)
	}
	for _, event := range events {
		if event.ActorKind != kind || event.ActorID != actorID || !reflect.DeepEqual(event.PrincipalID, principalID) {
			return fmt.Errorf("incorrect activity attribution: %+v", event)
		}
	}
	return nil
}

func (run *runner) assertCreateRowsAbsent(ctx context.Context, title, idempotencyKey string) error {
	var projects, events, idempotency, outbox int
	err := run.db.QueryRowContext(ctx, `
		SELECT
			(SELECT count(*) FROM projects WHERE title=$1),
			(SELECT count(*) FROM domain_events WHERE event_type='project.created' AND payload->>'title'=$1),
				(SELECT count(*) FROM idempotency_results WHERE idempotency_key=$2),
				(SELECT count(*) FROM outbox_records WHERE payload->'payload'->>'title'=$1)`,
		title, idempotencyKey).Scan(&projects, &events, &idempotency, &outbox)
	if err != nil {
		return err
	}
	if projects != 0 || events != 0 || idempotency != 0 || outbox != 0 {
		return fmt.Errorf("partial create rows projects=%d events=%d idempotency=%d outbox=%d", projects, events, idempotency, outbox)
	}
	return nil
}

func (run *runner) assertAgentCreateAuthority(ctx context.Context, projectRestrictionCount int) error {
	var agentStatus, delegatedStatus, agentRole, delegatedRole string
	var createScope bool
	var actualRestrictionCount int
	err := run.db.QueryRowContext(ctx, `
		SELECT a.status,h.status,m.role,hm.role,'project.create'=ANY(t.scopes),
			cardinality(COALESCE(t.project_ids,ARRAY[]::uuid[]))
		FROM agent_tokens t
		JOIN principals a ON a.id=t.agent_id
		JOIN principals h ON h.id=a.human_principal_id
		JOIN organization_memberships m ON m.organization_id=t.organization_id AND m.principal_id=a.id
		JOIN organization_memberships hm ON hm.organization_id=t.organization_id AND hm.principal_id=h.id
		WHERE t.token_prefix=$1`, agentToken[:16]).Scan(
		&agentStatus, &delegatedStatus, &agentRole, &delegatedRole, &createScope, &actualRestrictionCount)
	if err != nil {
		return err
	}
	if agentStatus != "active" || delegatedStatus != "active" || agentRole != "member" || delegatedRole != "owner" || !createScope || actualRestrictionCount != projectRestrictionCount {
		return fmt.Errorf("authority status/roles/scope/restrictions = %s/%s/%s/%s/%t/%d, want active/active/member/owner/true/%d",
			agentStatus, delegatedStatus, agentRole, delegatedRole, createScope, actualRestrictionCount, projectRestrictionCount)
	}
	return nil
}

func (run *runner) loadDomainCounts(ctx context.Context) (domainCounts, error) {
	var result domainCounts
	err := run.db.QueryRowContext(ctx, `
		SELECT
			(SELECT count(*) FROM projects),
			(SELECT count(*) FROM domain_events),
			(SELECT count(*) FROM decisions),
			(SELECT count(*) FROM idempotency_results)`).Scan(
		&result.Projects, &result.Events, &result.Decisions, &result.Idempotency)
	return result, err
}

func (run *runner) loadEventGroup(ctx context.Context, projectID string) ([]eventGroupEntry, error) {
	rows, err := run.db.QueryContext(ctx, `
		SELECT organization_id,aggregate_type,event_type,schema_version,aggregate_version,payload
		FROM domain_events
		WHERE organization_id=$1 AND aggregate_type='project' AND aggregate_id=$2
		ORDER BY sequence`, organizationID, projectID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := make([]eventGroupEntry, 0)
	for rows.Next() {
		var entry eventGroupEntry
		if err := rows.Scan(&entry.OrganizationID, &entry.AggregateType, &entry.EventType, &entry.SchemaVersion, &entry.AggregateVersion, &entry.Payload); err != nil {
			return nil, err
		}
		result = append(result, entry)
	}
	return result, rows.Err()
}

func normalizeResult(projectValue project, decisionValue decisionRecord, events []eventGroupEntry) (normalizedResult, error) {
	projectValue.ID = ""
	decisionValue.ID = ""
	decisionValue.ProjectID = ""
	decisionValue.ActorID = ""
	decisionValue.ActorKind = ""
	decisionValue.PrincipalID = nil
	decisionValue.RecordedAt = ""
	result := normalizedResult{Project: projectValue, Decision: decisionValue, Activity: make([]normalizedEvent, 0, len(events))}
	for _, event := range events {
		normalized := normalizedEvent{
			OrganizationID: event.OrganizationID, AggregateType: event.AggregateType,
			EventType: event.EventType, SchemaVersion: event.SchemaVersion, AggregateVersion: event.AggregateVersion,
		}
		switch event.EventType {
		case "project.created":
			var payload project
			if err := json.Unmarshal(event.Payload, &payload); err != nil {
				return normalizedResult{}, err
			}
			payload.ID = ""
			normalized.Payload = payload
		case "decision.recorded":
			var payload decisionRecord
			if err := json.Unmarshal(event.Payload, &payload); err != nil {
				return normalizedResult{}, err
			}
			payload.ID = ""
			payload.ProjectID = ""
			payload.ActorID = ""
			payload.ActorKind = ""
			payload.PrincipalID = nil
			payload.RecordedAt = ""
			normalized.Payload = payload
		default:
			return normalizedResult{}, fmt.Errorf("unexpected event type in parity group: %s", event.EventType)
		}
		result.Activity = append(result.Activity, normalized)
	}
	return result, nil
}

func normalizeActivity(events []activity) []normalizedActivityEntry {
	result := make([]normalizedActivityEntry, len(events))
	for index, event := range events {
		result[index] = normalizedActivityEntry{EventType: event.EventType, AggregateVersion: event.AggregateVersion}
	}
	return result
}

func (run *runner) call(client *http.Client, name, method, path string, input any, headers map[string]string) snapshot {
	var body io.Reader
	if input != nil {
		encoded, err := json.Marshal(input)
		if err != nil {
			fatal(err)
		}
		body = bytes.NewReader(encoded)
	}
	request, err := http.NewRequest(method, run.base+path, body)
	if err != nil {
		fatal(err)
	}
	if input != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	for key, value := range headers {
		request.Header.Set(key, value)
	}
	response, err := client.Do(request)
	if err != nil {
		fatal(err)
	}
	defer response.Body.Close()
	value, err := io.ReadAll(response.Body)
	if err != nil {
		fatal(err)
	}
	result := snapshot{Status: response.StatusCode, Body: json.RawMessage(value), BodyBytes: value, Headers: map[string]string{
		"Content-Type": response.Header.Get("Content-Type"), "ETag": response.Header.Get("ETag"), "X-Request-ID": response.Header.Get("X-Request-ID"),
	}}
	if name != "human-login" {
		run.writeJSON(name+".json", result)
	}
	return result
}

func (run *runner) expect(response snapshot, status int, name string) error {
	if response.Status != status {
		return fmt.Errorf("%s status=%d body=%s", name, response.Status, response.Body)
	}
	return nil
}

func expectProblem(response snapshot, status int, code string) error {
	if response.Status != status {
		return fmt.Errorf("expected %d/%s, got %d: %s", status, code, response.Status, response.Body)
	}
	var value struct {
		Code string `json:"code"`
	}
	if err := json.Unmarshal(response.Body, &value); err != nil {
		return err
	}
	if value.Code != code {
		return fmt.Errorf("expected problem %s, got %s", code, value.Code)
	}
	return nil
}

func exactReplay(original, replay snapshot, etag bool) error {
	if original.Status != replay.Status || !bytes.Equal(original.BodyBytes, replay.BodyBytes) || original.Headers["X-Request-ID"] != replay.Headers["X-Request-ID"] {
		return fmt.Errorf("status/body/request id changed")
	}
	if etag && original.Headers["ETag"] != replay.Headers["ETag"] {
		return fmt.Errorf("ETag changed")
	}
	return nil
}

func projectInput(title string) map[string]any {
	return map[string]any{
		"title": title, "outcome": "Prove the M1 transaction", "hypothesis": "One public plane preserves parity",
		"falsifier": "Either actor requires a private mutation", "decision_criteria": []string{"Contiguous attributed events"}, "experiment_bound": "Track A M1 only",
	}
}
func decisionInput() map[string]any {
	return map[string]any{
		"kind": "continue", "question": "Continue?", "choice": "Continue", "alternatives": []string{"Stop"},
		"rationale": "Committed evidence supports the next verification step", "evidence": []string{"activity ledger"}, "consequences": []string{"contract repricing follows"},
	}
}
func clone(source map[string]string) map[string]string {
	target := make(map[string]string, len(source))
	for key, value := range source {
		target[key] = value
	}
	return target
}
func envOr(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}
func (run *runner) writeJSON(name string, value any) {
	encoded, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		fatal(err)
	}
	if err := os.WriteFile(filepath.Join(run.artifacts, name), append(encoded, '\n'), 0o644); err != nil {
		fatal(err)
	}
}
func (run *runner) saveRedactedLogin(name string, response snapshot, authenticated session) {
	digest := sha256.Sum256([]byte(authenticated.CSRF))
	run.writeJSON(name+".json", map[string]any{"status": response.Status, "headers": response.Headers, "body": map[string]any{"actor_id": authenticated.ActorID, "actor_kind": authenticated.ActorKind, "expires_at": authenticated.Expires, "csrf_token_sha256": hex.EncodeToString(digest[:]), "csrf_token_length": len(authenticated.CSRF)}})
}
func fatal(err error) { fmt.Fprintln(os.Stderr, err); os.Exit(1) }
