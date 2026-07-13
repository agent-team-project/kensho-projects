// Command smoke executes the frozen M1 public transaction from inside the
// outbound-isolated Compose network and writes verifier-readable artifacts.
package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/cookiejar"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"time"

	_ "github.com/lib/pq"
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
	ID             string `json:"id"`
	OrganizationID string `json:"organization_id"`
	Mode           string `json:"mode"`
	State          string `json:"state"`
	Version        int64  `json:"version"`
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

type runner struct {
	base      string
	artifacts string
	human     *http.Client
	agent     *http.Client
	db        *sql.DB
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
	db, err := sql.Open("postgres", envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	if err != nil {
		fatal(err)
	}
	defer db.Close()
	run := &runner{
		base: base, artifacts: artifacts,
		human: &http.Client{Jar: jar, Timeout: 10 * time.Second},
		agent: &http.Client{Timeout: 10 * time.Second}, db: db,
	}
	if err := run.execute(context.Background()); err != nil {
		fatal(err)
	}
	fmt.Printf("M1 walking slice passed: %d load-bearing checks\n", len(run.checks))
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

	humanInput := projectInput("Human M1 transaction")
	humanHeaders := map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-create-000000000001"}
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
	faultHeaders := clone(baseDecisionHeaders)
	faultHeaders["If-Match"] = `"1"`
	faultHeaders["Idempotency-Key"] = "human-decision-00000003"
	faultHeaders["X-Workplane-Fault"] = "after-decision"
	if err := expectProblem(run.call(run.human, "deny-decision-transaction-fault", http.MethodPost, decisionPath, decisionInput, faultHeaders), http.StatusServiceUnavailable, "service_unavailable"); err != nil {
		return err
	}
	afterFault := run.call(run.human, "human-project-after-fault", http.MethodGet, "/api/v1/projects/"+humanProject.ID, nil, nil)
	var faultProjection project
	_ = json.Unmarshal(afterFault.Body, &faultProjection)
	if faultProjection.Version != 1 {
		return fmt.Errorf("fault injection partially committed version %d", faultProjection.Version)
	}

	decisionHeaders := clone(baseDecisionHeaders)
	decisionHeaders["If-Match"] = `"1"`
	decisionHeaders["Idempotency-Key"] = "human-decision-00000004"
	decision := run.call(run.human, "human-decision-create", http.MethodPost, decisionPath, decisionInput, decisionHeaders)
	if err := run.expect(decision, http.StatusCreated, "human decision"); err != nil {
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
	run.checks = append(run.checks, "decision-version-idempotency-atomicity", "human-immutable-attribution")

	agentHeaders := map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-create-000000000001"}
	agentCreated := run.call(run.agent, "agent-project-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput("Agent M1 transaction"), agentHeaders)
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
	agentRead := run.call(run.agent, "agent-project-read", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken})
	if err := run.expect(agentRead, http.StatusOK, "agent project read"); err != nil {
		return err
	}
	var finalAgent project
	_ = json.Unmarshal(agentRead.Body, &finalAgent)
	agentActivity := run.activity(run.agent, "agent-activity", agentProject.ID, map[string]string{"Authorization": "Bearer " + agentToken})
	principal := humanID
	if err := validateActivity(agentActivity, "agent", agentID, &principal); err != nil {
		return err
	}
	if finalAgent.Version != 2 || len(agentActivity) != len(humanActivity) {
		return fmt.Errorf("human/agent domain parity mismatch")
	}
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
	observerCreate := run.call(run.human, "deny-observer-create", http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", projectInput("Observer cannot create"), map[string]string{"Origin": publicOrigin, "X-CSRF-Token": authenticated.CSRF, "Idempotency-Key": "human-create-000000000005"})
	if err := expectProblem(observerCreate, http.StatusForbidden, "forbidden"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE organization_memberships SET role='owner' WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
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
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='disabled' WHERE id=$1`, humanID); err != nil {
		return err
	}
	if err := expectProblem(run.call(run.agent, "deny-agent-disabled-principal", http.MethodGet, "/api/v1/projects/"+agentProject.ID, nil, map[string]string{"Authorization": "Bearer " + agentToken}), http.StatusUnauthorized, "unauthenticated"); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE principals SET status='active' WHERE id=$1`, humanID); err != nil {
		return err
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE projects SET state='stopped' WHERE id=$1`, agentProject.ID); err != nil {
		return err
	}
	stateDeny := run.call(run.agent, "deny-decision-terminal-state", http.MethodPost, "/api/v1/projects/"+agentProject.ID+"/decisions", decisionInput, map[string]string{"Authorization": "Bearer " + agentToken, "Idempotency-Key": "agent-decision-00000003", "If-Match": `"2"`})
	if err := expectProblem(stateDeny, http.StatusConflict, "invariant_violation"); err != nil {
		return err
	}
	run.checks = append(run.checks, "auth-scope-revocation-status-state-separation-denies")

	var eventRows, decisions, idempotency int
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM domain_events`).Scan(&eventRows); err != nil {
		return err
	}
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM decisions`).Scan(&decisions); err != nil {
		return err
	}
	if err := run.db.QueryRowContext(ctx, `SELECT count(*) FROM idempotency_results`).Scan(&idempotency); err != nil {
		return err
	}
	if eventRows != 4 || decisions != 2 || idempotency != 4 {
		return fmt.Errorf("unexpected ledger counts events=%d decisions=%d idempotency=%d", eventRows, decisions, idempotency)
	}
	if _, err := run.db.ExecContext(ctx, `UPDATE domain_events SET event_type='tampered' WHERE event_id=(SELECT event_id FROM domain_events LIMIT 1)`); err == nil || !strings.Contains(err.Error(), "append-only") {
		return fmt.Errorf("event ledger update did not fail closed: %v", err)
	}
	run.writeJSON("postgres-ledger.json", map[string]any{"domain_events": eventRows, "decisions": decisions, "idempotency_results": idempotency, "versions": []int{1, 2}, "append_only_update": "denied"})
	run.checks = append(run.checks, "postgres-contiguous-append-only-ledger")
	run.writeJSON("smoke-summary.json", map[string]any{"result": "pass", "checks": run.checks, "human_project_id": humanProject.ID, "agent_project_id": agentProject.ID})
	return nil
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
