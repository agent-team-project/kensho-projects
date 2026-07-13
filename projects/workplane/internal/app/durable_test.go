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

func TestCrossActorDeliverableRevisionPreservesCreatorAndReplays(t *testing.T) {
	t.Parallel()
	const (
		organizationID = "00000000-0000-4000-8000-000000000010"
		projectID      = "00000000-0000-4000-8000-000000000020"
		deliverableID  = "00000000-0000-4000-8000-000000000030"
		creatorID      = "00000000-0000-4000-8000-000000000040"
		reviserID      = "00000000-0000-4000-8000-000000000050"
		createdAt      = "2026-07-13T10:00:00Z"
	)
	created := Deliverable{ID: deliverableID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Original", Description: "Observable output", Required: true, Weight: 1000, State: "ready",
		AcceptanceCriteria: []string{"Exact replay passes"}, Version: 1, CreatedBy: creatorID,
		CreatedAt: createdAt, UpdatedAt: createdAt}
	revised := created
	revised.Title, revised.Version, revised.UpdatedAt = "Revised", 2, "2026-07-13T11:00:00Z"
	events := []eventRow{
		{Projection: eventProjection{Sequence: 1, EventID: "00000000-0000-4000-8000-000000000101", EventType: "project.created",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project", AggregateID: projectID,
			AggregateVersion: 1, ActorKind: "human", ActorID: creatorID},
			Payload: canonicalJSON(Project{ID: projectID, OrganizationID: organizationID, Mode: "exploitation", State: "active", Version: 1})},
		{Projection: eventProjection{Sequence: 2, EventID: "00000000-0000-4000-8000-000000000102", EventType: "deliverable.created",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project", AggregateID: projectID,
			AggregateVersion: 2, ActorKind: "human", ActorID: creatorID}, Payload: canonicalJSON(created)},
		{Projection: eventProjection{Sequence: 3, EventID: "00000000-0000-4000-8000-000000000103", EventType: "deliverable.revised",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project", AggregateID: projectID,
			AggregateVersion: 3, ActorKind: "agent", ActorID: reviserID}, Payload: canonicalJSON(revised)},
	}

	snapshot, failure := rebuildSnapshot("cross-actor-revision", events)
	if failure != nil {
		t.Fatalf("cross-actor deliverable revision must replay: %v", failure)
	}
	if len(snapshot.Deliverables) != 1 || snapshot.Deliverables[0].CreatedBy != creatorID {
		t.Fatalf("creator attribution changed during replay: %+v", snapshot.Deliverables)
	}
	if len(snapshot.Activity) != 3 || snapshot.Activity[2].ActorID != reviserID {
		t.Fatalf("revision event attribution was not preserved: %+v", snapshot.Activity)
	}
}
