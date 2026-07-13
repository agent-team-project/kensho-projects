package main

import (
	"bufio"
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
	ID        string `json:"id"`
	ProjectID string `json:"project_id"`
	Required  bool   `json:"required"`
	State     string `json:"state"`
	Version   int64  `json:"version"`
}
type promotion struct {
	Project      project       `json:"project"`
	Deliverables []deliverable `json:"deliverables"`
}
type forecast struct {
	ID            string  `json:"id"`
	DeliverableID *string `json:"deliverable_id"`
	Scope         string  `json:"scope"`
	SupersedesID  *string `json:"supersedes_id"`
	Current       bool    `json:"current"`
	Stale         bool    `json:"stale"`
	AttentionOnly bool    `json:"attention_only"`
}
type counts struct{ Projects, Deliverables, Forecasts, Targets, Deadlines, Events, Outbox, Idempotency int }

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
	human, agent          *http.Client
	db                    *sql.DB
	checks                []string
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
	fmt.Printf("M2C planning-contract spine passed: %d load-bearing checks\n", len(run.checks))
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
		'deliverable.read','deliverable.edit','deliverable.reforecast','realtime.subscribe','event.subscribe'
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

	stateBeforeDates := "active"
	for index, boundary := range []string{"after-target", "after-target-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
			"target_at": time.Now().UTC().Add(-time.Hour).Format(time.RFC3339), "reason": "Fault rollback canary",
		}, run.versionedHuman(fmt.Sprintf("m2c-target-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	target := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/target", map[string]any{
		"target_at": time.Now().UTC().Add(-time.Hour).Format(time.RFC3339), "reason": "Intent remains visible after miss",
	}, run.versionedHuman("m2c-target-past-0001", version))
	if err := expect(target, http.StatusCreated, ""); err != nil {
		return err
	}
	version = etagVersion(target)
	for index, boundary := range []string{"after-deadline", "after-deadline-event"} {
		if err := run.faultNoResidue(ctx, run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deadline", map[string]any{
			"deadline_at": time.Now().UTC().Add(-30 * time.Minute).Format(time.RFC3339), "source": "contract", "description": "Fault rollback canary",
		}, run.versionedHuman(fmt.Sprintf("m2c-deadline-fault-%02d", index), version), boundary); err != nil {
			return err
		}
	}
	deadline := run.call(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/deadline", map[string]any{
		"deadline_at": time.Now().UTC().Add(-30 * time.Minute).Format(time.RFC3339), "source": "contract", "description": "External constraint only",
	}, run.versionedHuman("m2c-deadline-past-01", version))
	if err := expect(deadline, http.StatusCreated, ""); err != nil {
		return err
	}
	version = etagVersion(deadline)
	read := run.call(run.human, http.MethodGet, "/api/v1/projects/"+projectID, nil, nil)
	var current project
	_ = json.Unmarshal(read.Body, &current)
	if current.State != stateBeforeDates || version != current.Version {
		return fmt.Errorf("date command transitioned state or lost version: %+v version=%d", current, version)
	}
	run.write("m2c-forecasts.json", map[string]any{"history_count": len(forecasts), "current_per_scope": 1, "history_update": "denied", "target": "attention-only", "deadline_source": "contract", "state": current.State})
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

	store, err := app.NewDurableStore(envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	if err != nil {
		return err
	}
	defer store.Close()
	replay, err := store.Replay(ctx)
	if err != nil {
		return fmt.Errorf("planning replay: %w", err)
	}
	if replay.LiveChecksum != replay.RebuiltChecksum || replay.Planning == 0 {
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
	check(run.db.QueryRowContext(ctx, `SELECT (SELECT count(*) FROM projects),(SELECT count(*) FROM deliverables),(SELECT count(*) FROM forecasts),(SELECT count(*) FROM project_targets),(SELECT count(*) FROM project_deadlines),(SELECT count(*) FROM domain_events),(SELECT count(*) FROM outbox_records),(SELECT count(*) FROM idempotency_results)`).Scan(&value.Projects, &value.Deliverables, &value.Forecasts, &value.Targets, &value.Deadlines, &value.Events, &value.Outbox, &value.Idempotency))
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
