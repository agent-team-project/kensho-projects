package app

import (
	"bytes"
	"encoding/json"
	"fmt"
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

func TestCanonicalWorkEventPayloadRejectsEveryTamperClass(t *testing.T) {
	t.Parallel()
	const (
		organizationID = "00000000-0000-4000-8000-000000000010"
		projectID      = "00000000-0000-4000-8000-000000000020"
		workID         = "00000000-0000-4000-8000-000000000030"
		actorID        = "00000000-0000-4000-8000-000000000040"
		occurred       = "2026-07-13T10:00:00.123456Z"
	)
	item := WorkItemRecord{ID: workID, OrganizationID: organizationID, ProjectID: projectID, Title: "Canonical work",
		Description: "Every payload member is replay-significant", State: "open", Priority: "normal", Version: 1,
		CreatedBy: actorID, CreatedAt: occurred, UpdatedAt: occurred}
	created := canonicalJSON(WorkItemEvent{WorkItem: item, Command: "create", EvidenceIDs: []string{}})
	if _, err := decodeCanonicalWorkEvent(created, "work_item.created"); err != nil {
		t.Fatalf("canonical create rejected: %v", err)
	}
	started := item
	started.State, started.Version, started.AssigneeID, started.UpdatedAt = "in_progress", 2, &item.CreatedBy, "2026-07-13T10:01:00.123456Z"
	transition := canonicalJSON(WorkItemEvent{WorkItem: started, Command: "start", Reason: "Begin fixed work", EvidenceIDs: []string{}})
	tests := []struct {
		name, eventType string
		payload         []byte
		mutate          func(map[string]any)
	}{
		{"nonprojected-reason", "work_item.created", created, func(value map[string]any) { value["reason"] = "tampered" }},
		{"omitted-member", "work_item.created", created, func(value map[string]any) { delete(value, "reason") }},
		{"extra-member", "work_item.created", created, func(value map[string]any) { value["unexpected"] = true }},
		{"impossible-transition-metadata", "work_item.started", transition, func(value map[string]any) { value["finding_id"] = actorID }},
		{"omitted-batch-marker", "work_item.started", transition, func(value map[string]any) { delete(value, "batch") }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var value map[string]any
			if err := json.Unmarshal(test.payload, &value); err != nil {
				t.Fatal(err)
			}
			test.mutate(value)
			encoded, err := json.Marshal(value)
			if err != nil {
				t.Fatal(err)
			}
			if _, err := decodeCanonicalWorkEvent(encoded, test.eventType); err == nil {
				t.Fatalf("%s payload tamper was accepted: %s", test.name, encoded)
			}
		})
	}
}

func TestCanonicalDependencyEventPayloadRejectsInconsistentRemoval(t *testing.T) {
	t.Parallel()
	const (
		organizationID = "00000000-0000-4000-8000-000000000010"
		projectID      = "00000000-0000-4000-8000-000000000020"
		sourceID       = "00000000-0000-4000-8000-000000000030"
		targetID       = "00000000-0000-4000-8000-000000000031"
		dependencyID   = "00000000-0000-4000-8000-000000000032"
		actorID        = "00000000-0000-4000-8000-000000000040"
		occurred       = "2026-07-13T10:00:00.123456Z"
	)
	source := WorkItemRecord{ID: sourceID, OrganizationID: organizationID, ProjectID: projectID, Title: "Source",
		Description: "Canonical dependency source", State: "open", Priority: "normal", Version: 2,
		CreatedBy: actorID, CreatedAt: "2026-07-13T09:00:00.123456Z", UpdatedAt: occurred}
	dependency := WorkItemDependency{ID: dependencyID, OrganizationID: organizationID, SourceWorkItemID: sourceID,
		TargetWorkItemID: targetID, Kind: "blocks", Version: 1, CreatedBy: actorID, CreatedAt: occurred}
	for _, eventType := range []string{"dependency.added", "dependency.removed"} {
		removed := eventType == "dependency.removed"
		encoded := canonicalJSON(WorkDependencyEvent{Dependency: dependency, Source: source, Removed: removed})
		if _, err := decodeCanonicalDependencyEvent(encoded, eventType); err != nil {
			t.Fatalf("canonical %s rejected: %v", eventType, err)
		}
		var value map[string]any
		if err := json.Unmarshal(encoded, &value); err != nil {
			t.Fatal(err)
		}
		value["removed"] = !removed
		tampered, err := json.Marshal(value)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := decodeCanonicalDependencyEvent(tampered, eventType); err == nil {
			t.Fatalf("inconsistent %s removal marker was accepted", eventType)
		}
	}
}

func TestReplayRejectsBatchMarkerValueTamper(t *testing.T) {
	t.Parallel()
	const (
		organizationID = "00000000-0000-4000-8000-000000000010"
		projectID      = "00000000-0000-4000-8000-000000000020"
		actorID        = "00000000-0000-4000-8000-000000000040"
		commandID      = "00000000-0000-4000-8000-000000000050"
		requestID      = "batch-marker-proof"
		createdAt      = "2026-07-13T10:00:00.123456Z"
		cancelledAt    = "2026-07-13T10:01:00.123456Z"
	)
	items := []WorkItemRecord{
		{ID: "00000000-0000-4000-8000-000000000030", OrganizationID: organizationID, ProjectID: projectID,
			Title: "First", Description: "First atomic member", State: "open", Priority: "normal", Version: 1,
			CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt},
		{ID: "00000000-0000-4000-8000-000000000031", OrganizationID: organizationID, ProjectID: projectID,
			Title: "Second", Description: "Second atomic member", State: "open", Priority: "normal", Version: 1,
			CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt},
	}
	events := []eventRow{{Projection: eventProjection{Sequence: 1, EventID: "00000000-0000-4000-8000-000000000101",
		EventType: "project.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project",
		AggregateID: projectID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID},
		Payload: canonicalJSON(Project{ID: projectID, OrganizationID: organizationID, Mode: "exploration", State: "proposed", Version: 1})}}
	for index, item := range items {
		events = append(events, eventRow{Projection: eventProjection{Sequence: int64(index + 2),
			EventID: fmt.Sprintf("00000000-0000-4000-8000-%012d", index+102), EventType: "work_item.created", SchemaVersion: 1,
			OrganizationID: organizationID, AggregateType: "work_item", AggregateID: item.ID, AggregateVersion: 1,
			ActorKind: "human", ActorID: actorID, CommandID: fmt.Sprintf("00000000-0000-4000-8000-%012d", index+202),
			RequestID: fmt.Sprintf("create-%d", index), OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "create", EvidenceIDs: []string{}})})
	}
	for index, item := range items {
		item.State, item.Version, item.UpdatedAt = "cancelled", 2, cancelledAt
		payload := WorkItemEvent{WorkItem: item, Command: "cancel", Reason: "Atomic cancellation", EvidenceIDs: []string{}, Batch: true}
		if index == 0 {
			payload.Batch = false
		}
		events = append(events, eventRow{Projection: eventProjection{Sequence: int64(index + 4),
			EventID: fmt.Sprintf("00000000-0000-4000-8000-%012d", index+104), EventType: "work_item.cancelled", SchemaVersion: 1,
			OrganizationID: organizationID, AggregateType: "work_item", AggregateID: item.ID, AggregateVersion: 2,
			ActorKind: "human", ActorID: actorID, CommandID: commandID, RequestID: requestID, OccurredAt: cancelledAt},
			Payload: canonicalJSON(payload)})
	}
	_, failure := rebuildSnapshot("batch-marker-tamper", events)
	if failure == nil || failure.Code != "invalid_event_payload" {
		t.Fatalf("batch marker value tamper did not fail closed: %+v", failure)
	}
}
