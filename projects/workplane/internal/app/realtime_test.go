package app

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"strings"
	"testing"
	"time"
)

func TestRealtimeCursorIsBoundAndExpires(t *testing.T) {
	t.Parallel()
	now := time.Date(2026, 7, 13, 6, 0, 0, 0, time.UTC)
	service := &Service{config: Config{TokenHashKey: []byte("unit-test-cursor-key-material-32-bytes"), RealtimeCursorTTL: time.Minute}, now: func() time.Time { return now }}
	binding := cursorBinding{OrganizationID: "00000000-0000-4000-8000-000000000001", ActorID: "00000000-0000-4000-8000-000000000002", Transport: "sse", FilterDigest: "filter"}
	token, err := service.encodeRealtimeCursor(binding, 42, "00000000-0000-4000-8000-000000000003")
	if err != nil {
		t.Fatal(err)
	}
	value, err := service.decodeRealtimeCursor(token, binding)
	if err != nil || value.Sequence != 42 {
		t.Fatalf("cursor round trip: value=%+v err=%v", value, err)
	}
	parts := strings.Split(token, ".")
	signature, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		t.Fatal(err)
	}
	signature[0] ^= 0x01
	tampered := parts[0] + "." + base64.RawURLEncoding.EncodeToString(signature)

	wrongOrganization := binding
	wrongOrganization.OrganizationID = "00000000-0000-4000-8000-000000000099"
	wrongActor := binding
	wrongActor.ActorID = "00000000-0000-4000-8000-000000000099"
	wrongTransport := binding
	wrongTransport.Transport = "websocket"
	wrongFilter := binding
	wrongFilter.FilterDigest = "different-filter"

	tests := []struct {
		name    string
		token   string
		binding cursorBinding
		expired bool
	}{
		{name: "rejects_signature_tampering", token: tampered, binding: binding},
		{name: "rejects_organization_binding_mismatch", token: token, binding: wrongOrganization},
		{name: "rejects_actor_binding_mismatch", token: token, binding: wrongActor},
		{name: "rejects_transport_binding_mismatch", token: token, binding: wrongTransport},
		{name: "rejects_filter_digest_binding_mismatch", token: token, binding: wrongFilter},
		{name: "rejects_expired_cursor", token: token, binding: binding, expired: true},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if test.expired {
				now = now.Add(time.Minute)
				defer func() { now = now.Add(-time.Minute) }()
			}
			if _, err := service.decodeRealtimeCursor(test.token, test.binding); err == nil {
				t.Fatalf("production cursor decoder accepted %s", test.name)
			}
		})
	}
}

func TestRealtimeFilterCanonicalizesAndOnlyReduces(t *testing.T) {
	t.Parallel()
	request := mustRequest(t, "/api/v1/events?types=decision.recorded,project.created,decision.recorded")
	filter, err := parseRealtimeFilter(request)
	if err != nil {
		t.Fatal(err)
	}
	if !filter.allows("project.created") || !filter.allows("decision.recorded") || filter.allows("project.future") {
		t.Fatalf("unexpected filter behavior: %+v", filter.Types)
	}
	reordered := mustRequest(t, "/api/v1/events?types=project.created,decision.recorded")
	reorderedFilter, err := parseRealtimeFilter(reordered)
	if err != nil || reorderedFilter.Digest != filter.Digest {
		t.Fatalf("filter digest is not canonical: %q/%q err=%v", filter.Digest, reorderedFilter.Digest, err)
	}
}

func TestWebSocketClientFrameRequiresMaskAndDecodes(t *testing.T) {
	t.Parallel()
	payload := []byte(`{"type":"ack","cursor":"cursor"}`)
	mask := [4]byte{1, 2, 3, 4}
	frame := []byte{0x81, 0x80 | byte(len(payload)), mask[0], mask[1], mask[2], mask[3]}
	for index, value := range payload {
		frame = append(frame, value^mask[index%4])
	}
	opcode, decoded, err := readWebSocketFrame(bufio.NewReader(bytes.NewReader(frame)))
	if err != nil || opcode != 1 || !bytes.Equal(decoded, payload) {
		t.Fatalf("decoded opcode=%d payload=%s err=%v", opcode, decoded, err)
	}
}

func TestWebSocketServerFramePreservesCanonicalEnvelope(t *testing.T) {
	t.Parallel()
	left, right := netPipe(t)
	defer left.Close()
	defer right.Close()
	envelope := json.RawMessage(`{"sequence":7,"event_id":"00000000-0000-4000-8000-000000000001"}`)
	done := make(chan error, 1)
	go func() {
		done <- writeWebSocketJSON(left, time.Second, realtimeFrame{Type: "event", Cursor: "opaque", Event: envelope})
	}()
	header := make([]byte, 2)
	if _, err := io.ReadFull(right, header); err != nil {
		t.Fatal(err)
	}
	length := int(header[1] & 0x7f)
	if length == 126 {
		extended := make([]byte, 2)
		if _, err := io.ReadFull(right, extended); err != nil {
			t.Fatal(err)
		}
		length = int(binary.BigEndian.Uint16(extended))
	}
	encoded := make([]byte, length)
	if _, err := io.ReadFull(right, encoded); err != nil {
		t.Fatal(err)
	}
	var frame realtimeFrame
	if err := json.Unmarshal(encoded, &frame); err != nil || !bytes.Equal(frame.Event, envelope) {
		t.Fatalf("canonical envelope changed: frame=%+v err=%v", frame, err)
	}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

func mustRequest(t *testing.T, target string) *http.Request {
	t.Helper()
	request, err := http.NewRequest(http.MethodGet, target, nil)
	if err != nil {
		t.Fatal(err)
	}
	return request
}

func netPipe(t *testing.T) (net.Conn, net.Conn) {
	t.Helper()
	return net.Pipe()
}
