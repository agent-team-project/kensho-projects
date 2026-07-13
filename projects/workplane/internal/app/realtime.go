package app

import (
	"bufio"
	"context"
	"crypto/hmac"
	"crypto/sha1" // #nosec G505 -- RFC 6455 mandates SHA-1 for the handshake accept value.
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
)

const (
	realtimeConsumer  = "realtime-v1"
	webSocketProtocol = "workplane.v1"
)

var eventTypePattern = regexp.MustCompile(`^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$`)

type realtimeCursor struct {
	Version        int    `json:"v"`
	Sequence       int64  `json:"s"`
	EventID        string `json:"e,omitempty"`
	OrganizationID string `json:"o"`
	ActorID        string `json:"a"`
	Transport      string `json:"t"`
	FilterDigest   string `json:"f"`
	ExpiresAt      int64  `json:"x"`
}

type cursorBinding struct {
	OrganizationID string
	ActorID        string
	Transport      string
	FilterDigest   string
}

type realtimeFilter struct {
	Types  map[string]bool
	Digest string
}

type realtimeFrame struct {
	Type   string          `json:"type"`
	Cursor string          `json:"cursor,omitempty"`
	Event  json.RawMessage `json:"event,omitempty"`
	Code   string          `json:"code,omitempty"`
	Detail string          `json:"detail,omitempty"`
}

type streamRecord struct {
	Envelope OutboxEnvelope
	Encoded  json.RawMessage
}

type streamMessage struct {
	Kind    string
	Cursor  string
	Payload json.RawMessage
}

type snapshotProblem struct {
	Problem
	Links map[string]string `json:"links"`
}

// RegisterRealtime attaches the two manual streaming transports. They remain
// outside the generated request/response adapter because one upgrades the HTTP
// connection and the other intentionally keeps it open.
func (service *Service) RegisterRealtime(mux *http.ServeMux) {
	mux.HandleFunc("GET /api/v1/realtime", service.serveWebSocket)
	mux.HandleFunc("GET /api/v1/events", service.serveSSE)
}

func requestFromHTTP(request *http.Request) generated.Request {
	sessionCookie := ""
	if cookie, err := request.Cookie("workplane_session"); err == nil {
		sessionCookie = cookie.Value
	}
	bearerToken := ""
	if authorization := request.Header.Get("Authorization"); strings.HasPrefix(authorization, "Bearer ") {
		bearerToken = strings.TrimPrefix(authorization, "Bearer ")
	}
	return generated.Request{
		HTTPRequest: request,
		Security: generated.RequestSecurity{
			SessionCookie: sessionCookie,
			CSRFToken:     request.Header.Get("X-CSRF-Token"),
			BearerToken:   bearerToken,
		},
	}
}

func (service *Service) subscriptionActor(ctx context.Context, request *http.Request, action string) (Actor, generated.Response, bool) {
	return service.authenticateSubscription(ctx, requestFromHTTP(request), action)
}

func parseRealtimeFilter(request *http.Request) (realtimeFilter, error) {
	for key := range request.URL.Query() {
		if key != "cursor" && key != "types" {
			return realtimeFilter{}, fmt.Errorf("unsupported query parameter")
		}
	}
	raw := strings.TrimSpace(request.URL.Query().Get("types"))
	types := make(map[string]bool)
	if raw != "" {
		parts := strings.Split(raw, ",")
		if len(parts) > 32 {
			return realtimeFilter{}, fmt.Errorf("too many event filters")
		}
		for _, item := range parts {
			item = strings.TrimSpace(item)
			if !eventTypePattern.MatchString(item) {
				return realtimeFilter{}, fmt.Errorf("invalid event filter")
			}
			types[item] = true
		}
	}
	ordered := make([]string, 0, len(types))
	for item := range types {
		ordered = append(ordered, item)
	}
	sort.Strings(ordered)
	digest := sha256.Sum256([]byte(strings.Join(ordered, ",")))
	return realtimeFilter{Types: types, Digest: hex.EncodeToString(digest[:])}, nil
}

func (filter realtimeFilter) allows(eventType string) bool {
	return len(filter.Types) == 0 || filter.Types[eventType]
}

func (service *Service) encodeRealtimeCursor(binding cursorBinding, sequence int64, eventID string) (string, error) {
	ttl := service.config.RealtimeCursorTTL
	if ttl <= 0 {
		ttl = 24 * time.Hour
	}
	value := realtimeCursor{
		Version: 1, Sequence: sequence, EventID: eventID,
		OrganizationID: binding.OrganizationID, ActorID: binding.ActorID,
		Transport: binding.Transport, FilterDigest: binding.FilterDigest,
		ExpiresAt: service.now().Add(ttl).Unix(),
	}
	payload, err := json.Marshal(value)
	if err != nil {
		return "", err
	}
	mac := hmac.New(sha256.New, service.config.TokenHashKey)
	_, _ = mac.Write(payload)
	return base64.RawURLEncoding.EncodeToString(payload) + "." + base64.RawURLEncoding.EncodeToString(mac.Sum(nil)), nil
}

func (service *Service) decodeRealtimeCursor(token string, binding cursorBinding) (realtimeCursor, error) {
	if len(token) == 0 || len(token) > 2048 {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	parts := strings.Split(token, ".")
	if len(parts) != 2 {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	signature, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	mac := hmac.New(sha256.New, service.config.TokenHashKey)
	_, _ = mac.Write(payload)
	expected := mac.Sum(nil)
	if len(signature) != len(expected) || subtle.ConstantTimeCompare(signature, expected) != 1 {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	var value realtimeCursor
	decoder := json.NewDecoder(strings.NewReader(string(payload)))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&value); err != nil {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	if value.Version != 1 || value.Sequence < 0 || value.ExpiresAt <= service.now().Unix() ||
		value.OrganizationID != binding.OrganizationID || value.ActorID != binding.ActorID ||
		value.Transport != binding.Transport || value.FilterDigest != binding.FilterDigest ||
		(value.EventID != "" && !uuidPattern.MatchString(value.EventID)) {
		return realtimeCursor{}, errors.New("invalid cursor")
	}
	return value, nil
}

func (service *Service) resolveRealtimeStart(ctx context.Context, token string, binding cursorBinding) (int64, error) {
	var checkpoint, minimum int64
	err := service.db.QueryRowContext(ctx, `SELECT checkpoint.last_sequence,retention.minimum_cursor_sequence
		FROM consumer_checkpoints checkpoint CROSS JOIN realtime_retention retention
		WHERE checkpoint.consumer_name=$1 AND retention.organization_id=$2`, realtimeConsumer, binding.OrganizationID).
		Scan(&checkpoint, &minimum)
	if err != nil {
		return 0, fmt.Errorf("read realtime boundary: %w", err)
	}
	if token == "" {
		return checkpoint, nil
	}
	value, err := service.decodeRealtimeCursor(token, binding)
	if err != nil || value.Sequence < minimum || value.Sequence > checkpoint {
		return 0, errors.New("snapshot required")
	}
	if value.EventID != "" {
		var matches bool
		err = service.db.QueryRowContext(ctx, `SELECT EXISTS (
			SELECT 1 FROM outbox_records record
			WHERE record.event_sequence=$1 AND record.event_id=$2
			  AND record.payload->>'organization_id'=$3
		)`, value.Sequence, value.EventID, binding.OrganizationID).Scan(&matches)
		if err != nil || !matches {
			return 0, errors.New("snapshot required")
		}
	}
	return value.Sequence, nil
}

func (service *Service) realtimeBatch(ctx context.Context, after int64) ([]streamRecord, int64, error) {
	rows, err := service.db.QueryContext(ctx, `SELECT record.payload
		FROM consumer_deliveries delivery
		JOIN outbox_records record ON record.event_id=delivery.event_id
		JOIN consumer_checkpoints checkpoint ON checkpoint.consumer_name=delivery.consumer_name
		WHERE delivery.consumer_name=$1 AND delivery.event_sequence>$2
		  AND delivery.event_sequence<=checkpoint.last_sequence
		ORDER BY delivery.event_sequence LIMIT 64`, realtimeConsumer, after)
	if err != nil {
		return nil, after, err
	}
	defer rows.Close()
	records := make([]streamRecord, 0)
	last := after
	for rows.Next() {
		var encoded []byte
		if err := rows.Scan(&encoded); err != nil {
			return nil, last, err
		}
		var envelope OutboxEnvelope
		if err := decodeStrictJSON(encoded, &envelope); err != nil {
			return nil, last, fmt.Errorf("decode canonical outbox envelope: %w", err)
		}
		if envelope.Sequence <= last || envelope.EventID == "" || envelope.OrganizationID == "" {
			return nil, last, errors.New("non-monotonic realtime delivery")
		}
		last = envelope.Sequence
		records = append(records, streamRecord{Envelope: envelope, Encoded: json.RawMessage(encoded)})
	}
	return records, last, rows.Err()
}

func privilegedProjectRole(role string) bool { return role == "owner" || role == "admin" }

// Transport loops reject a mismatched organization before this project and
// principal check. Keeping those boundaries separate makes each guard explicit.
func (service *Service) actorCanReadEnvelope(ctx context.Context, actor Actor, envelope OutboxEnvelope) bool {
	if !organizationRoleAllows(actor.Role, "project.read") ||
		(actor.Kind == "agent" && !organizationRoleAllows(actor.DelegatedRole, "project.read")) {
		return false
	}
	if envelope.AggregateType != "project" {
		return actor.Kind != "agent" || len(actor.ProjectIDs) == 0
	}
	if actor.Kind == "agent" && !agentProjectRestrictionAllows(actor.ProjectIDs, envelope.AggregateID) {
		return false
	}
	var visibility, createdBy string
	var participant bool
	principalID := ""
	if actor.PrincipalID != nil {
		principalID = *actor.PrincipalID
	}
	err := service.db.QueryRowContext(ctx, `SELECT project.visibility::text,project.created_by,
		EXISTS (SELECT 1 FROM project_memberships membership
			WHERE membership.project_id=project.id
			  AND (membership.principal_id=$3 OR membership.principal_id=NULLIF($4,'')::uuid))
		FROM projects project WHERE project.id=$1 AND project.organization_id=$2`,
		envelope.AggregateID, envelope.OrganizationID, actor.ID, principalID).Scan(&visibility, &createdBy, &participant)
	if err != nil {
		return false
	}
	if visibility == "organization" {
		return true
	}
	if createdBy == actor.ID || (principalID != "" && createdBy == principalID) || participant {
		return true
	}
	return privilegedProjectRole(actor.Role) && (actor.Kind != "agent" || privilegedProjectRole(actor.DelegatedRole))
}

func writeRealtimeProblem(writer http.ResponseWriter, response generated.Response) {
	writer.Header().Set("Content-Type", response.ContentType)
	writer.Header().Set("Cache-Control", "no-store")
	if response.Headers.XRequestID != "" {
		writer.Header().Set("X-Request-ID", response.Headers.XRequestID)
	}
	writer.WriteHeader(response.Status)
	_ = json.NewEncoder(writer).Encode(response.Body)
}

func writeSnapshotRequired(writer http.ResponseWriter, rid string) {
	writer.Header().Set("Content-Type", "application/problem+json")
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("X-Request-ID", rid)
	writer.WriteHeader(http.StatusConflict)
	_ = json.NewEncoder(writer).Encode(snapshotProblem{
		Problem: Problem{Type: "https://workplane.local/problems/snapshot_required", Title: "Snapshot required",
			Status: http.StatusConflict, Code: "snapshot_required", RequestID: rid,
			Detail: "The requested realtime position is not resumable for this actor and filter."},
		Links: map[string]string{"projects": "/api/v1/orgs/{org_id}/projects", "activity": "/api/v1/projects/{project_id}/activity"},
	})
}

func (service *Service) serveWebSocket(writer http.ResponseWriter, request *http.Request) {
	filter, err := parseRealtimeFilter(request)
	if err != nil {
		writeRealtimeProblem(writer, problem(http.StatusBadRequest, "invalid_request", "Invalid subscription", err.Error(), requestID()))
		return
	}
	actor, authResponse, ok := service.subscriptionActor(request.Context(), request, "realtime.subscribe")
	if !ok {
		writeRealtimeProblem(writer, authResponse)
		return
	}
	rid := authResponse.Headers.XRequestID
	if actor.Kind == "human" && request.Header.Get("Origin") != service.config.PublicOrigin {
		writeRealtimeProblem(writer, problem(http.StatusForbidden, "forbidden", "WebSocket denied", "The browser origin is not allowed.", rid))
		return
	}
	if !headerContainsToken(request.Header.Get("Connection"), "upgrade") || !strings.EqualFold(request.Header.Get("Upgrade"), "websocket") ||
		request.Header.Get("Sec-WebSocket-Version") != "13" || !headerContainsToken(request.Header.Get("Sec-WebSocket-Protocol"), webSocketProtocol) {
		writeRealtimeProblem(writer, problem(http.StatusBadRequest, "invalid_request", "WebSocket upgrade required", "Use RFC 6455 with the workplane.v1 subprotocol.", rid))
		return
	}
	keyBytes, err := base64.StdEncoding.DecodeString(request.Header.Get("Sec-WebSocket-Key"))
	if err != nil || len(keyBytes) != 16 {
		writeRealtimeProblem(writer, problem(http.StatusBadRequest, "invalid_request", "Invalid WebSocket key", "The WebSocket key is malformed.", rid))
		return
	}
	binding := cursorBinding{OrganizationID: actor.OrganizationID, ActorID: actor.ID, Transport: "websocket", FilterDigest: filter.Digest}
	start, err := service.resolveRealtimeStart(request.Context(), request.URL.Query().Get("cursor"), binding)
	if err != nil {
		writeSnapshotRequired(writer, rid)
		return
	}
	hijacker, ok := writer.(http.Hijacker)
	if !ok {
		writeRealtimeProblem(writer, problem(http.StatusInternalServerError, "service_unavailable", "WebSocket unavailable", "The server cannot upgrade this connection.", rid))
		return
	}
	connection, buffered, err := hijacker.Hijack()
	if err != nil {
		return
	}
	defer connection.Close()
	acceptDigest := sha1.Sum([]byte(request.Header.Get("Sec-WebSocket-Key") + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")) // #nosec G401 -- mandated by RFC 6455.
	_, _ = fmt.Fprintf(buffered, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: %s\r\nSec-WebSocket-Protocol: %s\r\nX-Request-ID: %s\r\n\r\n",
		base64.StdEncoding.EncodeToString(acceptDigest[:]), webSocketProtocol, rid)
	if err := buffered.Flush(); err != nil {
		return
	}
	service.recordAcceptedAgentRequest(request.Context(), request)
	service.runWebSocket(request.Context(), connection, buffered.Reader, request, actor, binding, filter, start)
}

func headerContainsToken(value, wanted string) bool {
	for _, item := range strings.Split(value, ",") {
		if strings.EqualFold(strings.TrimSpace(item), wanted) {
			return true
		}
	}
	return false
}

func (service *Service) runWebSocket(ctx context.Context, connection net.Conn, reader *bufio.Reader, request *http.Request,
	actor Actor, binding cursorBinding, filter realtimeFilter, start int64) {
	poll := service.config.RealtimePollInterval
	if poll <= 0 {
		poll = 25 * time.Millisecond
	}
	heartbeat := service.config.RealtimeHeartbeat
	if heartbeat <= 0 {
		heartbeat = time.Second
	}
	maxUnacked := service.config.RealtimeMaxUnacked
	if maxUnacked <= 0 {
		maxUnacked = 8
	}
	writeTimeout := service.config.RealtimeWriteTimeout
	if writeTimeout <= 0 {
		writeTimeout = time.Second
	}
	current := start
	readyCursor, _ := service.encodeRealtimeCursor(binding, current, "")
	if err := writeWebSocketJSON(connection, writeTimeout, realtimeFrame{Type: "ready", Cursor: readyCursor}); err != nil {
		return
	}
	lastDeliveredCursor := readyCursor
	acknowledgements := make(chan realtimeCursor, maxUnacked*2)
	readErrors := make(chan error, 1)
	go service.readWebSocket(reader, binding, acknowledgements, readErrors)
	pending := make([]int64, 0, maxUnacked)
	lastHeartbeat := service.now()
	ticker := time.NewTicker(poll)
	defer ticker.Stop()
	for {
		for {
			select {
			case acknowledgement := <-acknowledgements:
				kept := pending[:0]
				for _, sequence := range pending {
					if sequence > acknowledgement.Sequence {
						kept = append(kept, sequence)
					}
				}
				pending = kept
			default:
				goto acknowledgementsDrained
			}
		}
	acknowledgementsDrained:
		select {
		case <-ctx.Done():
			return
		case <-readErrors:
			return
		case <-ticker.C:
		}
		fresh, _, valid := service.subscriptionActor(ctx, request, "realtime.subscribe")
		if !valid || fresh.ID != actor.ID || fresh.OrganizationID != actor.OrganizationID {
			_ = writeWebSocketJSON(connection, writeTimeout, realtimeFrame{Type: "permission_changed", Code: "permission_changed"})
			_ = writeWebSocketClose(connection, writeTimeout, 1008, "permission changed")
			return
		}
		actor = fresh
		records, examined, err := service.realtimeBatch(ctx, current)
		if err != nil {
			_ = writeWebSocketJSON(connection, writeTimeout, realtimeFrame{Type: "error", Code: "service_unavailable"})
			return
		}
		for _, record := range records {
			if record.Envelope.OrganizationID != actor.OrganizationID || !filter.allows(record.Envelope.EventType) ||
				!service.actorCanReadEnvelope(ctx, actor, record.Envelope) {
				current = record.Envelope.Sequence
				continue
			}
			if len(pending) >= maxUnacked {
				_ = writeWebSocketJSON(connection, writeTimeout, realtimeFrame{Type: "rate_limited", Cursor: lastDeliveredCursor, Code: "slow_consumer", Detail: "Too many events remain unacknowledged."})
				_ = writeWebSocketClose(connection, writeTimeout, 1013, "slow consumer")
				return
			}
			cursor, _ := service.encodeRealtimeCursor(binding, record.Envelope.Sequence, record.Envelope.EventID)
			if err := writeWebSocketJSON(connection, writeTimeout, realtimeFrame{Type: "event", Cursor: cursor, Event: record.Encoded}); err != nil {
				return
			}
			current = record.Envelope.Sequence
			lastDeliveredCursor = cursor
			pending = append(pending, current)
		}
		if examined > current {
			current = examined
		}
		if service.now().Sub(lastHeartbeat) >= heartbeat {
			cursor, _ := service.encodeRealtimeCursor(binding, current, "")
			if err := writeWebSocketJSON(connection, writeTimeout, realtimeFrame{Type: "heartbeat", Cursor: cursor}); err != nil {
				return
			}
			lastHeartbeat = service.now()
		}
	}
}

func (service *Service) readWebSocket(reader *bufio.Reader, binding cursorBinding, acknowledgements chan<- realtimeCursor, readErrors chan<- error) {
	for {
		opcode, payload, err := readWebSocketFrame(reader)
		if err != nil {
			readErrors <- err
			return
		}
		switch opcode {
		case 0x8:
			readErrors <- io.EOF
			return
		case 0x1:
			var value struct {
				Type   string `json:"type"`
				Cursor string `json:"cursor"`
			}
			if err := json.Unmarshal(payload, &value); err != nil || value.Type != "ack" {
				readErrors <- errors.New("invalid websocket message")
				return
			}
			cursor, err := service.decodeRealtimeCursor(value.Cursor, binding)
			if err != nil {
				readErrors <- err
				return
			}
			acknowledgements <- cursor
		default:
			readErrors <- errors.New("unsupported websocket frame")
			return
		}
	}
}

func writeWebSocketJSON(connection net.Conn, timeout time.Duration, value any) error {
	payload, err := json.Marshal(value)
	if err != nil {
		return err
	}
	return writeWebSocketFrame(connection, timeout, 0x1, payload)
}

func writeWebSocketClose(connection net.Conn, timeout time.Duration, code uint16, reason string) error {
	payload := make([]byte, 2+len(reason))
	binary.BigEndian.PutUint16(payload, code)
	copy(payload[2:], reason)
	return writeWebSocketFrame(connection, timeout, 0x8, payload)
}

func writeWebSocketFrame(connection net.Conn, timeout time.Duration, opcode byte, payload []byte) error {
	if len(payload) > 1<<20 {
		return errors.New("websocket frame exceeds limit")
	}
	_ = connection.SetWriteDeadline(time.Now().Add(timeout))
	header := []byte{0x80 | opcode}
	switch {
	case len(payload) < 126:
		header = append(header, byte(len(payload)))
	case len(payload) <= 65535:
		header = append(header, 126, byte(len(payload)>>8), byte(len(payload)))
	default:
		header = append(header, 127, 0, 0, 0, 0, byte(len(payload)>>24), byte(len(payload)>>16), byte(len(payload)>>8), byte(len(payload)))
	}
	if _, err := connection.Write(append(header, payload...)); err != nil {
		return err
	}
	return connection.SetWriteDeadline(time.Time{})
}

func readWebSocketFrame(reader *bufio.Reader) (byte, []byte, error) {
	header := make([]byte, 2)
	if _, err := io.ReadFull(reader, header); err != nil {
		return 0, nil, err
	}
	if header[0]&0x80 == 0 || header[1]&0x80 == 0 {
		return 0, nil, errors.New("fragmented or unmasked client frame")
	}
	opcode := header[0] & 0x0f
	length := int64(header[1] & 0x7f)
	if length == 126 {
		value := make([]byte, 2)
		if _, err := io.ReadFull(reader, value); err != nil {
			return 0, nil, err
		}
		length = int64(binary.BigEndian.Uint16(value))
	} else if length == 127 {
		value := make([]byte, 8)
		if _, err := io.ReadFull(reader, value); err != nil {
			return 0, nil, err
		}
		length = int64(binary.BigEndian.Uint64(value))
	}
	if length < 0 || length > 4096 {
		return 0, nil, errors.New("client frame exceeds limit")
	}
	mask := make([]byte, 4)
	if _, err := io.ReadFull(reader, mask); err != nil {
		return 0, nil, err
	}
	payload := make([]byte, length)
	if _, err := io.ReadFull(reader, payload); err != nil {
		return 0, nil, err
	}
	for index := range payload {
		payload[index] ^= mask[index%4]
	}
	return opcode, payload, nil
}

func (service *Service) serveSSE(writer http.ResponseWriter, request *http.Request) {
	if !headerContainsToken(request.Header.Get("Accept"), "text/event-stream") {
		writeRealtimeProblem(writer, problem(http.StatusNotAcceptable, "invalid_request", "Event stream required", "Accept must include text/event-stream.", requestID()))
		return
	}
	filter, err := parseRealtimeFilter(request)
	if err != nil {
		writeRealtimeProblem(writer, problem(http.StatusBadRequest, "invalid_request", "Invalid subscription", err.Error(), requestID()))
		return
	}
	actor, authResponse, ok := service.subscriptionActor(request.Context(), request, "event.subscribe")
	if !ok {
		writeRealtimeProblem(writer, authResponse)
		return
	}
	rid := authResponse.Headers.XRequestID
	if actor.Kind != "agent" {
		writeRealtimeProblem(writer, problem(http.StatusForbidden, "forbidden", "Agent stream required", "This stream requires a scoped agent bearer token.", rid))
		return
	}
	queryCursor := request.URL.Query().Get("cursor")
	headerCursor := request.Header.Get("Last-Event-ID")
	if queryCursor != "" && headerCursor != "" && queryCursor != headerCursor {
		writeRealtimeProblem(writer, problem(http.StatusBadRequest, "invalid_request", "Conflicting cursors", "cursor and Last-Event-ID must agree.", rid))
		return
	}
	token := queryCursor
	if token == "" {
		token = headerCursor
	}
	binding := cursorBinding{OrganizationID: actor.OrganizationID, ActorID: actor.ID, Transport: "sse", FilterDigest: filter.Digest}
	start, err := service.resolveRealtimeStart(request.Context(), token, binding)
	if err != nil {
		writeSnapshotRequired(writer, rid)
		return
	}
	flusher, ok := writer.(http.Flusher)
	if !ok {
		writeRealtimeProblem(writer, problem(http.StatusInternalServerError, "service_unavailable", "Event stream unavailable", "Streaming is unavailable.", rid))
		return
	}
	writer.Header().Set("Content-Type", "text/event-stream")
	writer.Header().Set("Cache-Control", "no-store")
	writer.Header().Set("Connection", "keep-alive")
	writer.Header().Set("X-Accel-Buffering", "no")
	writer.Header().Set("X-Request-ID", rid)
	service.recordAcceptedAgentRequest(request.Context(), request)
	writer.WriteHeader(http.StatusOK)
	readyCursor, _ := service.encodeRealtimeCursor(binding, start, "")
	ready, _ := json.Marshal(realtimeFrame{Type: "ready", Cursor: readyCursor})
	if err := writeSSE(writer, flusher, service.writeTimeout(), streamMessage{Kind: "ready", Cursor: readyCursor, Payload: ready}); err != nil {
		return
	}
	ctx, cancel := context.WithCancel(request.Context())
	defer cancel()
	bufferSize := service.config.RealtimeBuffer
	if bufferSize <= 0 {
		bufferSize = 8
	}
	messages := make(chan streamMessage, bufferSize)
	terminal := make(chan streamMessage, 1)
	go service.produceSSE(ctx, request, actor, binding, filter, start, messages, terminal)
	slowFault := service.config.FaultInjection && request.Header.Get("X-Workplane-Fault") == "slow-realtime-writer"
	for {
		select {
		case <-ctx.Done():
			return
		case message := <-terminal:
			_ = writeSSE(writer, flusher, service.writeTimeout(), message)
			return
		case message := <-messages:
			if slowFault {
				time.Sleep(300 * time.Millisecond)
			}
			if err := writeSSE(writer, flusher, service.writeTimeout(), message); err != nil {
				return
			}
		}
	}
}

func (service *Service) writeTimeout() time.Duration {
	if service.config.RealtimeWriteTimeout > 0 {
		return service.config.RealtimeWriteTimeout
	}
	return time.Second
}

func (service *Service) produceSSE(ctx context.Context, request *http.Request, actor Actor, binding cursorBinding,
	filter realtimeFilter, start int64, messages chan<- streamMessage, terminal chan<- streamMessage) {
	poll := service.config.RealtimePollInterval
	if poll <= 0 {
		poll = 25 * time.Millisecond
	}
	heartbeat := service.config.RealtimeHeartbeat
	if heartbeat <= 0 {
		heartbeat = time.Second
	}
	current := start
	lastHeartbeat := service.now()
	ticker := time.NewTicker(poll)
	defer ticker.Stop()
	offer := func(message streamMessage) bool {
		select {
		case messages <- message:
			return true
		default:
			select {
			case terminal <- streamMessage{Kind: "rate_limited", Payload: json.RawMessage(`{"type":"rate_limited","code":"slow_consumer"}`)}:
			default:
			}
			return false
		}
	}
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		fresh, _, valid := service.subscriptionActor(ctx, request, "event.subscribe")
		if !valid || fresh.ID != actor.ID || fresh.OrganizationID != actor.OrganizationID {
			terminal <- streamMessage{Kind: "permission_changed", Payload: json.RawMessage(`{"type":"permission_changed","code":"permission_changed"}`)}
			return
		}
		actor = fresh
		records, examined, err := service.realtimeBatch(ctx, current)
		if err != nil {
			terminal <- streamMessage{Kind: "error", Payload: json.RawMessage(`{"type":"error","code":"service_unavailable"}`)}
			return
		}
		for _, record := range records {
			current = record.Envelope.Sequence
			if record.Envelope.OrganizationID != actor.OrganizationID || !filter.allows(record.Envelope.EventType) ||
				!service.actorCanReadEnvelope(ctx, actor, record.Envelope) {
				continue
			}
			cursor, _ := service.encodeRealtimeCursor(binding, current, record.Envelope.EventID)
			if !offer(streamMessage{Kind: "domain-event", Cursor: cursor, Payload: record.Encoded}) {
				return
			}
		}
		if examined > current {
			current = examined
		}
		if service.now().Sub(lastHeartbeat) >= heartbeat {
			cursor, _ := service.encodeRealtimeCursor(binding, current, "")
			payload, _ := json.Marshal(realtimeFrame{Type: "heartbeat", Cursor: cursor})
			if !offer(streamMessage{Kind: "heartbeat", Cursor: cursor, Payload: payload}) {
				return
			}
			lastHeartbeat = service.now()
		}
	}
}

func writeSSE(writer http.ResponseWriter, flusher http.Flusher, timeout time.Duration, message streamMessage) error {
	controller := http.NewResponseController(writer)
	_ = controller.SetWriteDeadline(time.Now().Add(timeout))
	if message.Cursor != "" {
		if _, err := fmt.Fprintf(writer, "id: %s\n", message.Cursor); err != nil {
			return err
		}
	}
	if _, err := fmt.Fprintf(writer, "event: %s\n", message.Kind); err != nil {
		return err
	}
	for _, line := range strings.Split(string(message.Payload), "\n") {
		if _, err := fmt.Fprintf(writer, "data: %s\n", line); err != nil {
			return err
		}
	}
	if _, err := io.WriteString(writer, "\n"); err != nil {
		return err
	}
	flusher.Flush()
	_ = controller.SetWriteDeadline(time.Time{})
	return nil
}
