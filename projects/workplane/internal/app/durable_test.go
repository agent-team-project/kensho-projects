package app

import (
	"bytes"
	"encoding/json"
	"testing"
)

func TestOutboxEnvelopeNormalizationIsDeterministic(t *testing.T) {
	t.Parallel()
	envelope := OutboxEnvelope{
		Sequence: 2, EventID: "00000000-0000-4000-8000-000000000001", EventType: "project.created",
		SchemaVersion: 1, OrganizationID: "00000000-0000-4000-8000-000000000002", AggregateType: "project",
		AggregateID: "00000000-0000-4000-8000-000000000003", AggregateVersion: 1,
		CommandID: "00000000-0000-4000-8000-000000000004", RequestID: "request",
		ActorKind: "human", ActorID: "00000000-0000-4000-8000-000000000005",
		OccurredAt: "2026-07-13T03:29:11.123456Z", Payload: json.RawMessage(`{"title":"A","decision_criteria":["one"]}`),
	}
	encoded := canonicalJSON(envelope)
	normalized, err := canonicalizeJSON(encoded)
	if err != nil {
		t.Fatal(err)
	}
	repeated, err := canonicalizeJSON(normalized)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(normalized, repeated) {
		t.Fatalf("normalization is not deterministic:\nfirst:  %s\nsecond: %s", normalized, repeated)
	}
}

func TestReplayStopsAtUnknownSchemaWithoutBuildingProjection(t *testing.T) {
	t.Parallel()
	events := []eventRow{{Projection: eventProjection{
		Sequence: 7, EventID: "00000000-0000-4000-8000-000000000001", EventType: "project.created",
		SchemaVersion: 2, AggregateType: "project", AggregateID: "00000000-0000-4000-8000-000000000002",
		AggregateVersion: 1,
	}}}
	_, failure := rebuildSnapshot("00000000-0000-4000-8000-000000000003", events)
	if failure == nil || failure.Code != "unknown_event_schema" || failure.Sequence != 7 || failure.SchemaVersion != 2 {
		t.Fatalf("unexpected replay failure: %+v", failure)
	}
}
