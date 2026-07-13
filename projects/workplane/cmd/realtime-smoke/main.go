// Command realtime-smoke exercises the M2B streaming surface through the
// production HTTP server and the real PostgreSQL realtime outbox consumer.
package main

import (
	"bufio"
	"bytes"
	"crypto/rand"
	"database/sql"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"os"
	"path/filepath"
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
	consumerName   = "realtime-v1"
	privateCanary  = "00000000-0000-4000-8000-000000000052"
	publicCanary   = "00000000-0000-4000-8000-000000000054"
)

type harness struct {
	base      string
	artifacts string
	db        *sql.DB
	human     *http.Client
	agent     *http.Client
	csrf      string
}

type project struct {
	ID      string `json:"id"`
	Version int64  `json:"version"`
}

type envelope struct {
	Sequence       int64           `json:"sequence"`
	EventID        string          `json:"event_id"`
	EventType      string          `json:"event_type"`
	OrganizationID string          `json:"organization_id"`
	AggregateID    string          `json:"aggregate_id"`
	Payload        json.RawMessage `json:"payload"`
}

type wireFrame struct {
	Type   string          `json:"type"`
	Cursor string          `json:"cursor"`
	Event  json.RawMessage `json:"event"`
	Code   string          `json:"code"`
}

type restartState struct {
	WebSocketCursor string            `json:"websocket_cursor"`
	SSECursor       string            `json:"sse_cursor"`
	Events          []json.RawMessage `json:"events"`
}

type wsClient struct {
	conn   net.Conn
	reader *bufio.Reader
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

func main() {
	mode := "full"
	if len(os.Args) == 2 {
		mode = os.Args[1]
	}
	jar, err := cookiejar.New(nil)
	check(err)
	db, err := sql.Open("postgres", envOr("WORKPLANE_DATABASE_URL", "postgres://workplane:workplane-local-only@postgres:5432/workplane?sslmode=disable"))
	check(err)
	defer db.Close()
	run := &harness{
		base: envOr("WORKPLANE_API_BASE", "http://api:8080"), artifacts: envOr("WORKPLANE_EVIDENCE_DIR", "/evidence"), db: db,
		human: &http.Client{Jar: jar, Timeout: 10 * time.Second}, agent: &http.Client{Timeout: 10 * time.Second},
	}
	check(os.MkdirAll(run.artifacts, 0o755))
	check(run.login())
	check(run.restoreAgentAuthority())
	switch mode {
	case "prepare-restart":
		check(run.prepareRestart())
	case "verify-restart":
		check(run.verifyRestart())
	case "full":
		check(run.full())
	default:
		fatal(fmt.Errorf("unknown mode %q", mode))
	}
	fmt.Printf("M2B realtime smoke %s passed\n", mode)
}

func (run *harness) login() error {
	response, body, err := run.request(run.human, http.MethodPost, "/api/v1/session/login",
		map[string]any{"email": "human@workplane.local", "password": "walking-slice-password"}, nil)
	if err != nil {
		return err
	}
	if response.StatusCode != http.StatusOK {
		return fmt.Errorf("login status=%d body=%s", response.StatusCode, body)
	}
	var session struct {
		CSRF string `json:"csrf_token"`
	}
	if err := json.Unmarshal(body, &session); err != nil {
		return err
	}
	run.csrf = session.CSRF
	return nil
}

func (run *harness) restoreAgentAuthority() error {
	_, err := run.db.Exec(`UPDATE principals SET status='active' WHERE id IN ($1,$2)`, humanID, agentID)
	if err != nil {
		return err
	}
	_, err = run.db.Exec(`INSERT INTO organization_memberships (organization_id,principal_id,role,created_at)
		VALUES ($1,$2,'owner',CURRENT_TIMESTAMP),($1,$3,'member',CURRENT_TIMESTAMP)
		ON CONFLICT (organization_id,principal_id) DO UPDATE SET role=EXCLUDED.role`, organizationID, humanID, agentID)
	if err != nil {
		return err
	}
	_, err = run.db.Exec(`UPDATE agent_tokens SET revoked_at=NULL,expires_at=CURRENT_TIMESTAMP+INTERVAL '24 hours',
		scopes=ARRAY['project.create','project.read','decision.record','realtime.subscribe','event.subscribe'],project_ids=NULL
		WHERE token_prefix=$1`, agentToken[:16])
	return err
}

func (run *harness) prepareRestart() error {
	if err := run.waitRealtimeCaughtUp(10 * time.Second); err != nil {
		return err
	}
	ws, readyWS, err := run.openWebSocket("", "")
	if err != nil {
		return err
	}
	ws.close()
	sse, err := run.openSSE("", "", nil)
	if err != nil {
		return err
	}
	readySSE, err := sse.next(3 * time.Second)
	sse.close()
	if err != nil || readySSE.Kind != "ready" {
		return fmt.Errorf("SSE ready: event=%+v err=%v", readySSE, err)
	}
	created, err := run.createProject("M2B restart resume canary", "restart-create-000000001", nil)
	if err != nil {
		return err
	}
	if err := run.recordDecision(created.ID, created.Version, "restart-decision-00001"); err != nil {
		return err
	}
	events, err := run.projectEnvelopes(created.ID)
	if err != nil || len(events) != 2 {
		return fmt.Errorf("restart event load count=%d err=%v", len(events), err)
	}
	if err := run.waitCheckpoint(events[1], 10*time.Second); err != nil {
		return err
	}
	state := restartState{WebSocketCursor: readyWS.Cursor, SSECursor: readySSE.ID, Events: events}
	return run.writeJSON("restart-state.json", state)
}

func (run *harness) verifyRestart() error {
	var state restartState
	if err := run.readJSON("restart-state.json", &state); err != nil {
		return err
	}
	ws, ready, err := run.openWebSocket(state.WebSocketCursor, "")
	if err != nil || ready.Type != "ready" {
		return fmt.Errorf("restart websocket ready: %+v %v", ready, err)
	}
	wsEvents := make([]wireFrame, 0, 2)
	for len(wsEvents) < 2 {
		frame, err := ws.next(5 * time.Second)
		if err != nil {
			return err
		}
		if frame.Type == "event" {
			wsEvents = append(wsEvents, frame)
			if err := ws.ack(frame.Cursor); err != nil {
				return err
			}
		}
	}
	ws.close()
	sse, err := run.openSSE("", state.SSECursor, nil)
	if err != nil {
		return err
	}
	if event, err := sse.next(3 * time.Second); err != nil || event.Kind != "ready" {
		return fmt.Errorf("restart SSE ready: %+v %v", event, err)
	}
	sseEvents := make([]sseEvent, 0, 2)
	for len(sseEvents) < 2 {
		event, err := sse.next(5 * time.Second)
		if err != nil {
			return err
		}
		if event.Kind == "domain-event" {
			sseEvents = append(sseEvents, event)
		}
	}
	sse.close()
	for index := range state.Events {
		if !equalJSON(wsEvents[index].Event, state.Events[index]) || !equalJSON(sseEvents[index].Data, state.Events[index]) ||
			!equalJSON(wsEvents[index].Event, sseEvents[index].Data) {
			return fmt.Errorf("restart envelope %d differs across ledger/ws/sse", index)
		}
	}
	evidence := map[string]any{
		"api_and_outbox_restarted": true, "websocket_resume_count": len(wsEvents), "sse_resume_count": len(sseEvents),
		"canonical_envelope_equal": true, "sequences": []int64{sequenceOf(state.Events[0]), sequenceOf(state.Events[1])},
	}
	return run.writeJSON("m2b-restart-resume.json", evidence)
}

func (run *harness) full() error {
	results := make(map[string]any)
	if err := run.snapshotOutcomes(); err != nil {
		return err
	}
	results["snapshot_required"] = true
	if err := run.authorityCanaries(); err != nil {
		return err
	}
	results["authority_canaries"] = true
	if err := run.liveRevocation(); err != nil {
		return err
	}
	results["live_revocation"] = true
	if err := run.commitBeforePublish(); err != nil {
		return err
	}
	results["commit_before_publish"] = true
	if err := run.websocketBackpressure(); err != nil {
		return err
	}
	results["websocket_backpressure"] = true
	if err := run.sseBackpressure(); err != nil {
		return err
	}
	results["sse_backpressure"] = true
	return run.writeJSON("m2b-load-bearing-outcomes.json", results)
}

func (run *harness) snapshotOutcomes() error {
	response, body, err := run.request(run.agent, http.MethodGet, "/api/v1/events?cursor=not-a-cursor", nil,
		map[string]string{"Authorization": "Bearer " + agentToken, "Accept": "text/event-stream"})
	if err != nil {
		return err
	}
	if response.StatusCode != http.StatusConflict || !bytes.Contains(body, []byte(`"code":"snapshot_required"`)) {
		return fmt.Errorf("unknown cursor did not require snapshot: %d %s", response.StatusCode, body)
	}
	ws, ready, err := run.openWebSocket("", "")
	if err != nil {
		return err
	}
	ws.close()
	var checkpoint int64
	if err := run.db.QueryRow(`SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name=$1`, consumerName).Scan(&checkpoint); err != nil {
		return err
	}
	if _, err := run.db.Exec(`UPDATE realtime_retention SET minimum_cursor_sequence=$2 WHERE organization_id=$1`, organizationID, checkpoint+1); err != nil {
		return err
	}
	defer run.db.Exec(`UPDATE realtime_retention SET minimum_cursor_sequence=0 WHERE organization_id=$1`, organizationID) //nolint:errcheck
	request, _ := http.NewRequest(http.MethodGet, run.base+"/api/v1/realtime?cursor="+url.QueryEscape(ready.Cursor), nil)
	request.Header.Set("Origin", publicOrigin)
	request.Header.Set("Connection", "Upgrade")
	request.Header.Set("Upgrade", "websocket")
	request.Header.Set("Sec-WebSocket-Version", "13")
	request.Header.Set("Sec-WebSocket-Key", base64.StdEncoding.EncodeToString(make([]byte, 16)))
	request.Header.Set("Sec-WebSocket-Protocol", "workplane.v1")
	response, err = run.human.Do(request)
	if err != nil {
		return err
	}
	defer response.Body.Close()
	body, _ = io.ReadAll(response.Body)
	if response.StatusCode != http.StatusConflict || !bytes.Contains(body, []byte(`"code":"snapshot_required"`)) {
		return fmt.Errorf("stale cursor did not require snapshot: %d %s", response.StatusCode, body)
	}
	return run.writeJSON("m2b-snapshot-required.json", map[string]any{"unknown_cursor": "snapshot_required", "stale_cursor": "snapshot_required", "minimum_cursor_sequence": checkpoint + 1})
}

func (run *harness) authorityCanaries() error {
	if _, err := run.db.Exec(`UPDATE realtime_retention SET minimum_cursor_sequence=0 WHERE organization_id=$1`, organizationID); err != nil {
		return err
	}
	allowed, err := run.createProject("M2B allowed restricted project", "authority-allowed-00001", nil)
	if err != nil {
		return err
	}
	if err := run.waitRealtimeCaughtUp(10 * time.Second); err != nil {
		return err
	}
	if _, err := run.db.Exec(`UPDATE agent_tokens SET project_ids=ARRAY[$2::uuid] WHERE token_prefix=$1`, agentToken[:16], allowed.ID); err != nil {
		return err
	}
	if _, err := run.db.Exec(`UPDATE organization_memberships SET role='observer' WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	stream, err := run.openSSE("", "", nil)
	if err != nil {
		return err
	}
	defer stream.close()
	if event, err := stream.next(3 * time.Second); err != nil || event.Kind != "ready" {
		return fmt.Errorf("authority stream ready: %+v %v", event, err)
	}
	cross, err := run.seedCanary("00000000-0000-4000-8000-000000000090", "00000000-0000-4000-8000-000000000091", "00000000-0000-4000-8000-000000000092", "organization", "CROSS_ORG_CANARY")
	if err != nil {
		return err
	}
	private, err := run.seedCanary(organizationID, "00000000-0000-4000-8000-000000000051", privateCanary, "private", "PRIVATE_PROJECT_CANARY")
	if err != nil {
		return err
	}
	restricted, err := run.seedCanary(organizationID, "00000000-0000-4000-8000-000000000051", "00000000-0000-4000-8000-000000000053", "organization", "RESTRICTED_PROJECT_CANARY")
	if err != nil {
		return err
	}
	allowedEvent, err := run.seedDecision(allowed.ID, "ALLOWED_PROJECT_EVENT")
	if err != nil {
		return err
	}
	if err := run.waitCheckpoint(allowedEvent, 10*time.Second); err != nil {
		return err
	}
	var delivered sseEvent
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		event, err := stream.next(time.Until(deadline))
		if err != nil {
			return err
		}
		if event.Kind == "domain-event" {
			delivered = event
			break
		}
	}
	if !equalJSON(delivered.Data, allowedEvent) || bytes.Contains(delivered.Data, []byte("CANARY")) && !bytes.Contains(delivered.Data, []byte("ALLOWED_PROJECT_EVENT")) {
		return fmt.Errorf("authority stream disclosed a canary or omitted allowed event: %s", delivered.Data)
	}
	if err := run.restoreAgentAuthority(); err != nil {
		return err
	}
	return run.writeJSON("m2b-authority-canaries.json", map[string]any{
		"delivered_event_id": eventIDOf(delivered.Data), "cross_org_event_id": eventIDOf(cross),
		"private_event_id": eventIDOf(private), "restricted_event_id": eventIDOf(restricted),
		"cross_org_disclosed": false, "private_disclosed": false, "restricted_disclosed": false,
	})
}

func (run *harness) liveRevocation() error {
	stream, err := run.openSSE("", "", nil)
	if err != nil {
		return err
	}
	if event, err := stream.next(3 * time.Second); err != nil || event.Kind != "ready" {
		return fmt.Errorf("revocation stream ready: %+v %v", event, err)
	}
	started := time.Now()
	if _, err := run.db.Exec(`UPDATE agent_tokens SET revoked_at=CURRENT_TIMESTAMP WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	postRevokeCanary, err := run.seedDecision(privateCanary, "POST_TOKEN_REVOKE_CANARY")
	if err != nil {
		return err
	}
	if err := run.waitCheckpoint(postRevokeCanary, 10*time.Second); err != nil {
		return err
	}
	event, elapsed, err := awaitSSEPermissionChange(stream, started, "token revoke")
	stream.close()
	if err != nil {
		return err
	}
	if err := run.restoreAgentAuthority(); err != nil {
		return err
	}
	expiryStream, err := run.openSSE("", "", nil)
	if err != nil {
		return err
	}
	if ready, err := expiryStream.next(3 * time.Second); err != nil || ready.Kind != "ready" {
		return fmt.Errorf("expiry stream ready: %+v %v", ready, err)
	}
	expiryStarted := time.Now()
	if _, err := run.db.Exec(`UPDATE agent_tokens SET expires_at=CURRENT_TIMESTAMP-INTERVAL '1 second' WHERE token_prefix=$1`, agentToken[:16]); err != nil {
		return err
	}
	expiryEvent, expiryElapsed, err := awaitSSEPermissionChange(expiryStream, expiryStarted, "token expiry")
	expiryStream.close()
	if err != nil {
		return err
	}
	if err := run.restoreAgentAuthority(); err != nil {
		return err
	}
	identityStream, err := run.openSSE("", "", nil)
	if err != nil {
		return err
	}
	if ready, err := identityStream.next(3 * time.Second); err != nil || ready.Kind != "ready" {
		return fmt.Errorf("service identity stream ready: %+v %v", ready, err)
	}
	identityStarted := time.Now()
	if _, err := run.db.Exec(`UPDATE principals SET status='disabled' WHERE id=$1`, agentID); err != nil {
		return err
	}
	identityEvent, identityElapsed, err := awaitSSEPermissionChange(identityStream, identityStarted, "service identity disable")
	identityStream.close()
	if err != nil {
		return err
	}
	if err := run.restoreAgentAuthority(); err != nil {
		return err
	}
	ws, ready, err := run.openWebSocket("", "")
	if err != nil || ready.Type != "ready" {
		return fmt.Errorf("membership websocket ready: %+v %v", ready, err)
	}
	if _, err := run.db.Exec(`DELETE FROM organization_memberships WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	membershipCanary, err := run.seedDecision(privateCanary, "POST_MEMBERSHIP_REVOKE_CANARY")
	if err != nil {
		return err
	}
	if err := run.waitCheckpoint(membershipCanary, 10*time.Second); err != nil {
		return err
	}
	frame, err := awaitWebSocketPermissionChange(ws, "membership revoke")
	ws.close()
	if err != nil {
		return err
	}
	if err := run.restoreAgentAuthority(); err != nil {
		return err
	}
	if _, err := run.db.Exec(`UPDATE projects SET visibility='organization' WHERE id=$1`, privateCanary); err != nil {
		return err
	}
	visibilityStream, err := run.openSSE("", "", nil)
	if err != nil {
		return err
	}
	if ready, err := visibilityStream.next(3 * time.Second); err != nil || ready.Kind != "ready" {
		return fmt.Errorf("role/visibility stream ready: %+v %v", ready, err)
	}
	tx, err := run.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	if _, err := tx.Exec(`UPDATE organization_memberships SET role='observer' WHERE organization_id=$1 AND principal_id=$2`, organizationID, humanID); err != nil {
		return err
	}
	if _, err := tx.Exec(`UPDATE projects SET visibility='private' WHERE id=$1`, privateCanary); err != nil {
		return err
	}
	if err := tx.Commit(); err != nil {
		return err
	}
	postVisibilityCanary, err := run.seedDecision(privateCanary, "POST_VISIBILITY_REVOKE_CANARY")
	if err != nil {
		return err
	}
	publicEvent, err := run.seedCanary(organizationID, "00000000-0000-4000-8000-000000000051", publicCanary, "organization", "LIVE_PUBLIC_ALLOWED")
	if err != nil {
		return err
	}
	if err := run.waitCheckpoint(publicEvent, 10*time.Second); err != nil {
		return err
	}
	var delivered sseEvent
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		candidate, err := visibilityStream.next(time.Until(deadline))
		if err != nil {
			return err
		}
		if candidate.Kind == "domain-event" {
			delivered = candidate
			break
		}
	}
	visibilityStream.close()
	if !equalJSON(delivered.Data, publicEvent) || bytes.Contains(delivered.Data, []byte("POST_VISIBILITY_REVOKE_CANARY")) {
		return fmt.Errorf("role/visibility change disclosed private event or omitted public event: %s", delivered.Data)
	}
	if err := run.restoreAgentAuthority(); err != nil {
		return err
	}
	return run.writeJSON("m2b-live-revocation.json", map[string]any{
		"token_close_ms": elapsed.Milliseconds(), "token_outcome": event.Kind,
		"token_post_revoke_event_id": eventIDOf(postRevokeCanary), "token_post_revoke_disclosed": false,
		"expiry_close_ms": expiryElapsed.Milliseconds(), "expiry_outcome": expiryEvent.Kind,
		"identity_close_ms": identityElapsed.Milliseconds(), "identity_outcome": identityEvent.Kind,
		"membership_outcome": frame.Type, "membership_post_revoke_event_id": eventIDOf(membershipCanary), "membership_post_revoke_disclosed": false,
		"role_visibility_private_event_id": eventIDOf(postVisibilityCanary), "role_visibility_private_disclosed": false,
		"role_visibility_public_event_id": eventIDOf(publicEvent), "under_five_seconds": true,
	})
}

func awaitSSEPermissionChange(stream *sseClient, started time.Time, label string) (sseEvent, time.Duration, error) {
	deadline := started.Add(5 * time.Second)
	for time.Now().Before(deadline) {
		event, err := stream.next(time.Until(deadline))
		if err != nil {
			return event, time.Since(started), fmt.Errorf("%s: %w", label, err)
		}
		switch event.Kind {
		case "permission_changed":
			return event, time.Since(started), nil
		case "domain-event":
			return event, time.Since(started), fmt.Errorf("%s disclosed a domain event after authority loss: %s", label, event.Data)
		}
	}
	return sseEvent{}, time.Since(started), fmt.Errorf("%s did not close within five seconds", label)
}

func awaitWebSocketPermissionChange(ws *wsClient, label string) (wireFrame, error) {
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		frame, err := ws.next(time.Until(deadline))
		if err != nil {
			return frame, fmt.Errorf("%s: %w", label, err)
		}
		switch frame.Type {
		case "permission_changed":
			return frame, nil
		case "event":
			return frame, fmt.Errorf("%s disclosed a domain event after authority loss: %s", label, frame.Event)
		}
	}
	return wireFrame{}, fmt.Errorf("%s did not close within five seconds", label)
}

func (run *harness) commitBeforePublish() error {
	if err := run.waitRealtimeCaughtUp(10 * time.Second); err != nil {
		return err
	}
	ws, _, err := run.openWebSocket("", "")
	if err != nil {
		return err
	}
	defer ws.close()
	_, err = run.createProject("ROLLED_BACK_REALTIME_CANARY", "rollback-realtime-0001", map[string]string{"X-Workplane-Fault": "after-event"})
	if err == nil || !strings.Contains(err.Error(), "503") {
		return fmt.Errorf("faulted project did not roll back: %v", err)
	}
	created, err := run.createProject("COMMITTED_REALTIME_CANARY", "commit-realtime-00001", nil)
	if err != nil {
		return err
	}
	var delivered wireFrame
	for {
		frame, err := ws.next(5 * time.Second)
		if err != nil {
			return err
		}
		if frame.Type == "event" {
			delivered = frame
			break
		}
	}
	if bytes.Contains(delivered.Event, []byte("ROLLED_BACK")) || !bytes.Contains(delivered.Event, []byte("COMMITTED_REALTIME_CANARY")) {
		return fmt.Errorf("commit boundary delivered wrong event: %s", delivered.Event)
	}
	_ = ws.ack(delivered.Cursor)
	return run.writeJSON("m2b-commit-before-publish.json", map[string]any{"rolled_back_disclosed": false, "committed_project_id": created.ID, "committed_event_id": eventIDOf(delivered.Event)})
}

func (run *harness) websocketBackpressure() error {
	if err := run.waitRealtimeCaughtUp(10 * time.Second); err != nil {
		return err
	}
	ws, _, err := run.openWebSocket("", "")
	if err != nil {
		return err
	}
	defer ws.close()
	for index := 0; index < 10; index++ {
		if _, err := run.createProject(fmt.Sprintf("WS backpressure %02d", index), fmt.Sprintf("ws-backpressure-%08d", index), nil); err != nil {
			return err
		}
	}
	events := 0
	for {
		frame, err := ws.next(10 * time.Second)
		if err != nil {
			return err
		}
		if frame.Type == "event" {
			events++
		}
		if frame.Type == "rate_limited" {
			if frame.Code != "slow_consumer" || events != 8 {
				return fmt.Errorf("unexpected websocket backpressure frame=%+v events=%d", frame, events)
			}
			return run.writeJSON("m2b-websocket-backpressure.json", map[string]any{"unacked_events": events, "outcome": frame.Type, "code": frame.Code, "silent_drop": false})
		}
	}
}

func (run *harness) sseBackpressure() error {
	if err := run.waitRealtimeCaughtUp(10 * time.Second); err != nil {
		return err
	}
	stream, err := run.openSSE("", "", map[string]string{"X-Workplane-Fault": "slow-realtime-writer"})
	if err != nil {
		return err
	}
	defer stream.close()
	if event, err := stream.next(3 * time.Second); err != nil || event.Kind != "ready" {
		return fmt.Errorf("slow SSE ready: %+v %v", event, err)
	}
	for index := 0; index < 12; index++ {
		if _, err := run.createProject(fmt.Sprintf("SSE backpressure %02d", index), fmt.Sprintf("sse-backpressure-%07d", index), nil); err != nil {
			return err
		}
	}
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		event, err := stream.next(time.Until(deadline))
		if err != nil {
			return err
		}
		if event.Kind == "rate_limited" {
			return run.writeJSON("m2b-sse-backpressure.json", map[string]any{"outcome": event.Kind, "silent_drop": false, "bounded_buffer": 8})
		}
	}
	return errors.New("SSE slow consumer did not receive an explicit outcome")
}

func (run *harness) createProject(title, key string, extra map[string]string) (project, error) {
	headers := map[string]string{"Origin": publicOrigin, "X-CSRF-Token": run.csrf, "Idempotency-Key": key}
	for name, value := range extra {
		headers[name] = value
	}
	response, body, err := run.request(run.human, http.MethodPost, "/api/v1/orgs/"+organizationID+"/projects", map[string]any{
		"title": title, "outcome": "Exercise M2B realtime delivery", "hypothesis": "Committed events remain resumable",
		"falsifier": "A canary leaks or an event disappears", "decision_criteria": []string{"Exact canonical envelope"}, "experiment_bound": "Track B M2B only",
	}, headers)
	if err != nil {
		return project{}, err
	}
	if response.StatusCode != http.StatusCreated {
		return project{}, fmt.Errorf("create %q status=%d body=%s", title, response.StatusCode, body)
	}
	var value project
	if err := json.Unmarshal(body, &value); err != nil {
		return project{}, err
	}
	return value, nil
}

func (run *harness) recordDecision(projectID string, version int64, key string) error {
	response, body, err := run.request(run.human, http.MethodPost, "/api/v1/projects/"+projectID+"/decisions", map[string]any{
		"kind": "continue", "question": "Resume after restart?", "choice": "Continue", "alternatives": []string{"Stop"},
		"rationale": "The durable cursor must cross process lifetime", "evidence": []string{"realtime checkpoint"}, "consequences": []string{"M2B gate remains load-bearing"},
	}, map[string]string{"Origin": publicOrigin, "X-CSRF-Token": run.csrf, "Idempotency-Key": key, "If-Match": fmt.Sprintf(`"%d"`, version)})
	if err != nil {
		return err
	}
	if response.StatusCode != http.StatusCreated {
		return fmt.Errorf("decision status=%d body=%s", response.StatusCode, body)
	}
	return nil
}

func (run *harness) request(client *http.Client, method, path string, input any, headers map[string]string) (*http.Response, []byte, error) {
	var body io.Reader
	if input != nil {
		encoded, err := json.Marshal(input)
		if err != nil {
			return nil, nil, err
		}
		body = bytes.NewReader(encoded)
	}
	request, err := http.NewRequest(method, run.base+path, body)
	if err != nil {
		return nil, nil, err
	}
	if input != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	for name, value := range headers {
		request.Header.Set(name, value)
	}
	response, err := client.Do(request)
	if err != nil {
		return nil, nil, err
	}
	defer response.Body.Close()
	value, err := io.ReadAll(response.Body)
	return response, value, err
}

func (run *harness) openWebSocket(cursor, types string) (*wsClient, wireFrame, error) {
	target, err := url.Parse(run.base)
	if err != nil {
		return nil, wireFrame{}, err
	}
	connection, err := net.DialTimeout("tcp", target.Host, 3*time.Second)
	if err != nil {
		return nil, wireFrame{}, err
	}
	path := "/api/v1/realtime"
	query := url.Values{}
	if cursor != "" {
		query.Set("cursor", cursor)
	}
	if types != "" {
		query.Set("types", types)
	}
	if encoded := query.Encode(); encoded != "" {
		path += "?" + encoded
	}
	keyBytes := make([]byte, 16)
	_, _ = rand.Read(keyBytes)
	cookies := run.human.Jar.Cookies(target)
	cookieValues := make([]string, 0, len(cookies))
	for _, cookie := range cookies {
		cookieValues = append(cookieValues, cookie.Name+"="+cookie.Value)
	}
	request := fmt.Sprintf("GET %s HTTP/1.1\r\nHost: %s\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: %s\r\nSec-WebSocket-Protocol: workplane.v1\r\nOrigin: %s\r\nCookie: %s\r\n\r\n",
		path, target.Host, base64.StdEncoding.EncodeToString(keyBytes), publicOrigin, strings.Join(cookieValues, "; "))
	if _, err := io.WriteString(connection, request); err != nil {
		connection.Close()
		return nil, wireFrame{}, err
	}
	reader := bufio.NewReader(connection)
	response, err := http.ReadResponse(reader, &http.Request{Method: http.MethodGet})
	if err != nil {
		connection.Close()
		return nil, wireFrame{}, err
	}
	if response.StatusCode != http.StatusSwitchingProtocols {
		body, _ := io.ReadAll(response.Body)
		connection.Close()
		return nil, wireFrame{}, fmt.Errorf("websocket status=%d body=%s", response.StatusCode, body)
	}
	client := &wsClient{conn: connection, reader: reader}
	ready, err := client.next(3 * time.Second)
	if err != nil || ready.Type != "ready" {
		client.close()
		return nil, ready, fmt.Errorf("websocket ready: %+v %w", ready, err)
	}
	return client, ready, nil
}

func (client *wsClient) next(timeout time.Duration) (wireFrame, error) {
	_ = client.conn.SetReadDeadline(time.Now().Add(timeout))
	opcode, payload, err := readServerFrame(client.reader)
	if err != nil {
		return wireFrame{}, err
	}
	if opcode == 8 {
		return wireFrame{}, io.EOF
	}
	var frame wireFrame
	return frame, json.Unmarshal(payload, &frame)
}

func (client *wsClient) ack(cursor string) error {
	payload, _ := json.Marshal(map[string]string{"type": "ack", "cursor": cursor})
	return writeClientFrame(client.conn, payload)
}

func (client *wsClient) close() {
	if client != nil && client.conn != nil {
		_ = client.conn.Close()
	}
}

func readServerFrame(reader *bufio.Reader) (byte, []byte, error) {
	header := make([]byte, 2)
	if _, err := io.ReadFull(reader, header); err != nil {
		return 0, nil, err
	}
	length := uint64(header[1] & 0x7f)
	if length == 126 {
		value := make([]byte, 2)
		if _, err := io.ReadFull(reader, value); err != nil {
			return 0, nil, err
		}
		length = uint64(binary.BigEndian.Uint16(value))
	} else if length == 127 {
		value := make([]byte, 8)
		if _, err := io.ReadFull(reader, value); err != nil {
			return 0, nil, err
		}
		length = binary.BigEndian.Uint64(value)
	}
	if header[1]&0x80 != 0 || length > 1<<20 {
		return 0, nil, errors.New("invalid server websocket frame")
	}
	payload := make([]byte, length)
	_, err := io.ReadFull(reader, payload)
	return header[0] & 0x0f, payload, err
}

func writeClientFrame(connection net.Conn, payload []byte) error {
	mask := make([]byte, 4)
	_, _ = rand.Read(mask)
	header := []byte{0x81}
	if len(payload) < 126 {
		header = append(header, 0x80|byte(len(payload)))
	} else {
		header = append(header, 0x80|126, byte(len(payload)>>8), byte(len(payload)))
	}
	header = append(header, mask...)
	for index, value := range payload {
		header = append(header, value^mask[index%4])
	}
	_, err := connection.Write(header)
	return err
}

func (run *harness) openSSE(cursor, lastEventID string, extra map[string]string) (*sseClient, error) {
	path := "/api/v1/events"
	if cursor != "" {
		path += "?cursor=" + url.QueryEscape(cursor)
	}
	request, err := http.NewRequest(http.MethodGet, run.base+path, nil)
	if err != nil {
		return nil, err
	}
	request.Header.Set("Authorization", "Bearer "+agentToken)
	request.Header.Set("Accept", "text/event-stream")
	if lastEventID != "" {
		request.Header.Set("Last-Event-ID", lastEventID)
	}
	for name, value := range extra {
		request.Header.Set(name, value)
	}
	response, err := (&http.Client{}).Do(request)
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
	resultChannel := make(chan result, 1)
	go func() {
		var event sseEvent
		data := make([]string, 0)
		for {
			line, err := client.reader.ReadString('\n')
			if err != nil {
				resultChannel <- result{err: err}
				return
			}
			line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")
			if line == "" {
				event.Data = json.RawMessage(strings.Join(data, "\n"))
				resultChannel <- result{event: event}
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
	case value := <-resultChannel:
		return value.event, value.err
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

func (run *harness) projectEnvelopes(projectID string) ([]json.RawMessage, error) {
	rows, err := run.db.Query(`SELECT record.payload FROM outbox_records record
		WHERE record.payload->>'aggregate_id'=$1 ORDER BY record.event_sequence`, projectID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	result := make([]json.RawMessage, 0)
	for rows.Next() {
		var value []byte
		if err := rows.Scan(&value); err != nil {
			return nil, err
		}
		result = append(result, json.RawMessage(value))
	}
	return result, rows.Err()
}

func (run *harness) waitRealtimeCaughtUp(timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		var checkpoint, head int64
		err := run.db.QueryRow(`SELECT checkpoint.last_sequence,COALESCE((SELECT max(event_sequence) FROM outbox_records),0)
			FROM consumer_checkpoints checkpoint WHERE checkpoint.consumer_name=$1`, consumerName).Scan(&checkpoint, &head)
		if err == nil && checkpoint == head {
			return nil
		}
		time.Sleep(25 * time.Millisecond)
	}
	return errors.New("realtime consumer did not catch up")
}

func (run *harness) waitCheckpoint(raw json.RawMessage, timeout time.Duration) error {
	want := sequenceOf(raw)
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		var got int64
		if err := run.db.QueryRow(`SELECT last_sequence FROM consumer_checkpoints WHERE consumer_name=$1`, consumerName).Scan(&got); err == nil && got >= want {
			return nil
		}
		time.Sleep(25 * time.Millisecond)
	}
	return fmt.Errorf("realtime checkpoint did not reach %d", want)
}

func (run *harness) seedCanary(orgID, creatorID, projectID, visibility, title string) (json.RawMessage, error) {
	if orgID != organizationID {
		_, err := run.db.Exec(`INSERT INTO organizations (id,slug,name,created_at) VALUES ($1,'m2b-cross-org','M2B Cross Org',CURRENT_TIMESTAMP) ON CONFLICT DO NOTHING`, orgID)
		if err != nil {
			return nil, err
		}
	}
	_, err := run.db.Exec(`INSERT INTO principals (id,kind,display_name,status,human_principal_id,created_at)
		VALUES ($1,'human','M2B Canary Owner','active',NULL,CURRENT_TIMESTAMP) ON CONFLICT DO NOTHING`, creatorID)
	if err != nil {
		return nil, err
	}
	_, err = run.db.Exec(`INSERT INTO organization_memberships (organization_id,principal_id,role,created_at)
		VALUES ($1,$2,'member',CURRENT_TIMESTAMP) ON CONFLICT DO NOTHING`, orgID, creatorID)
	if err != nil {
		return nil, err
	}
	eventID, commandID := uuid(), uuid()
	payload := map[string]any{"id": projectID, "organization_id": orgID, "title": title, "outcome": "M2B canary", "mode": "exploration", "state": "proposed", "version": 1,
		"hypothesis": "Authority filters are fail closed", "falsifier": "Canary disclosure", "decision_criteria": []string{"No disclosure"}, "experiment_bound": "M2B"}
	tx, err := run.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()
	_, err = tx.Exec(`INSERT INTO projects (id,organization_id,title,outcome,mode,state,version,hypothesis,falsifier,decision_criteria,experiment_bound,created_by,created_at,updated_at,visibility)
		VALUES ($1,$2,$3,'M2B canary','exploration','proposed',1,'Authority filters are fail closed','Canary disclosure','["No disclosure"]','M2B',$4,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,$5)`,
		projectID, orgID, title, creatorID, visibility)
	if err != nil {
		return nil, err
	}
	encoded, _ := json.Marshal(payload)
	_, err = tx.Exec(`INSERT INTO domain_events (event_id,organization_id,aggregate_type,aggregate_id,aggregate_version,event_type,schema_version,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload)
		VALUES ($1,$2,'project',$3,1,'project.created',1,'human',$4,NULL,$5,$6,CURRENT_TIMESTAMP,$7)`, eventID, orgID, projectID, creatorID, commandID, "m2b-canary-"+title, encoded)
	if err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return run.outboxEnvelope(eventID)
}

func (run *harness) seedDecision(projectID, marker string) (json.RawMessage, error) {
	decisionID, eventID, commandID := uuid(), uuid(), uuid()
	payload := map[string]any{"id": decisionID, "project_id": projectID, "actor_id": humanID, "actor_kind": "human", "principal_id": nil,
		"recorded_at": time.Now().UTC().Format("2006-01-02T15:04:05.000000Z"), "kind": "continue", "question": marker, "choice": "Continue", "alternatives": []string{},
		"rationale": marker, "evidence": []string{}, "consequences": []string{}}
	encoded, _ := json.Marshal(payload)
	tx, err := run.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()
	var orgID string
	var version int64
	if err := tx.QueryRow(`UPDATE projects SET version=version+1,updated_at=CURRENT_TIMESTAMP WHERE id=$1 RETURNING organization_id,version`, projectID).Scan(&orgID, &version); err != nil {
		return nil, err
	}
	_, err = tx.Exec(`INSERT INTO decisions (id,organization_id,project_id,kind,question,choice,alternatives,rationale,evidence,consequences,actor_id,recorded_at)
		VALUES ($1,$2,$3,'continue',$4,'Continue','[]',$4,'[]','[]',$5,CURRENT_TIMESTAMP)`, decisionID, orgID, projectID, marker, humanID)
	if err != nil {
		return nil, err
	}
	_, err = tx.Exec(`INSERT INTO domain_events (event_id,organization_id,aggregate_type,aggregate_id,aggregate_version,event_type,schema_version,actor_kind,actor_id,principal_id,command_id,request_id,occurred_at,payload)
		VALUES ($1,$2,'project',$3,$4,'decision.recorded',1,'human',$5,NULL,$6,$7,CURRENT_TIMESTAMP,$8)`, eventID, orgID, projectID, version, humanID, commandID, "m2b-decision-"+marker, encoded)
	if err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return run.outboxEnvelope(eventID)
}

func (run *harness) outboxEnvelope(eventID string) (json.RawMessage, error) {
	var value []byte
	err := run.db.QueryRow(`SELECT payload FROM outbox_records WHERE event_id=$1`, eventID).Scan(&value)
	return json.RawMessage(value), err
}

func equalJSON(left, right []byte) bool {
	var a, b any
	return json.Unmarshal(left, &a) == nil && json.Unmarshal(right, &b) == nil && fmt.Sprintf("%#v", a) == fmt.Sprintf("%#v", b)
}

func sequenceOf(raw json.RawMessage) int64 {
	var value envelope
	_ = json.Unmarshal(raw, &value)
	return value.Sequence
}

func eventIDOf(raw json.RawMessage) string {
	var value envelope
	_ = json.Unmarshal(raw, &value)
	return value.EventID
}

func uuid() string {
	value := make([]byte, 16)
	_, _ = rand.Read(value)
	value[6] = value[6]&0x0f | 0x40
	value[8] = value[8]&0x3f | 0x80
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x", value[0:4], value[4:6], value[6:8], value[8:10], value[10:16])
}

func (run *harness) writeJSON(name string, value any) error {
	encoded, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(run.artifacts, name), append(encoded, '\n'), 0o644)
}

func (run *harness) readJSON(name string, target any) error {
	encoded, err := os.ReadFile(filepath.Join(run.artifacts, name))
	if err != nil {
		return err
	}
	return json.Unmarshal(encoded, target)
}

func envOr(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}

func check(err error) {
	if err != nil {
		fatal(err)
	}
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}
