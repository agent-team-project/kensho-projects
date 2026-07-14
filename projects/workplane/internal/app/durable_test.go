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
		{"null-required-reason", "work_item.created", created, func(value map[string]any) { value["reason"] = nil }},
		{"wrong-type-reason", "work_item.created", created, func(value map[string]any) { value["reason"] = false }},
		{"null-required-batch", "work_item.created", created, func(value map[string]any) { value["batch"] = nil }},
		{"wrong-type-batch", "work_item.created", created, func(value map[string]any) { value["batch"] = "false" }},
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
	added := canonicalJSON(WorkDependencyEvent{Dependency: dependency, Source: source, Removed: false})
	for name, replacement := range map[string]any{"null": nil, "string": "false"} {
		var value map[string]any
		if err := json.Unmarshal(added, &value); err != nil {
			t.Fatal(err)
		}
		value["removed"] = replacement
		tampered, err := json.Marshal(value)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := decodeCanonicalDependencyEvent(tampered, "dependency.added"); err == nil {
			t.Fatalf("dependency add accepted %s removed marker", name)
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
	snapshot, failure := rebuildSnapshot("batch-marker-tamper", events)
	if failure == nil || failure.Code != "invalid_event_payload" ||
		failure.Detail != "work item batch command contains a non-batch transition" ||
		failure.Sequence != 4 || failure.EventID != "00000000-0000-4000-8000-000000000104" {
		t.Fatalf("batch marker value tamper did not fail closed: %+v", failure)
	}
	if len(snapshot.Projects) != 0 || len(snapshot.WorkItems) != 0 || len(snapshot.Activity) != 0 {
		t.Fatalf("batch marker tamper built a partial projection: %+v", snapshot)
	}
}

func TestReplayRejectsInconsistentBatchCommandIdentityBeforeProjection(t *testing.T) {
	t.Parallel()
	const (
		organizationID = "00000000-0000-4000-8000-000000000010"
		otherOrgID     = "00000000-0000-4000-8000-000000000011"
		projectID      = "00000000-0000-4000-8000-000000000020"
		otherProjectID = "00000000-0000-4000-8000-000000000021"
		actorID        = "00000000-0000-4000-8000-000000000040"
		otherActorID   = "00000000-0000-4000-8000-000000000041"
		commandID      = "00000000-0000-4000-8000-000000000050"
		requestID      = "batch-identity-proof"
		createdAt      = "2026-07-13T10:00:00.123456Z"
		cancelledAt    = "2026-07-13T10:01:00.123456Z"
		otherTime      = "2026-07-13T10:01:01.123456Z"
		firstEventID   = "00000000-0000-4000-8000-000000000104"
	)
	items := []WorkItemRecord{
		{ID: "00000000-0000-4000-8000-000000000030", OrganizationID: organizationID, ProjectID: projectID,
			Title: "First identity member", Description: "First atomic member", State: "open", Priority: "normal",
			Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt},
		{ID: "00000000-0000-4000-8000-000000000031", OrganizationID: organizationID, ProjectID: projectID,
			Title: "Second identity member", Description: "Second atomic member", State: "open", Priority: "normal",
			Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt},
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
			RequestID: fmt.Sprintf("create-identity-%d", index), OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "create", EvidenceIDs: []string{}})})
	}
	for index, item := range items {
		item.State, item.Version, item.UpdatedAt = "cancelled", 2, cancelledAt
		events = append(events, eventRow{Projection: eventProjection{Sequence: int64(index + 4),
			EventID: fmt.Sprintf("00000000-0000-4000-8000-%012d", index+104), EventType: "work_item.cancelled", SchemaVersion: 1,
			OrganizationID: organizationID, AggregateType: "work_item", AggregateID: item.ID, AggregateVersion: 2,
			ActorKind: "human", ActorID: actorID, CommandID: commandID, RequestID: requestID, OccurredAt: cancelledAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "cancel", Reason: "Atomic cancellation",
				EvidenceIDs: []string{}, Batch: true})})
	}
	if _, failure := rebuildSnapshot("healthy-batch-identity", events); failure != nil {
		t.Fatalf("canonical batch identity did not replay: %v", failure)
	}
	tests := []struct {
		name   string
		mutate func(*eventRow)
	}{
		{"organization", func(row *eventRow) {
			row.Projection.OrganizationID = otherOrgID
			var payload WorkItemEvent
			_ = json.Unmarshal(row.Payload, &payload)
			payload.WorkItem.OrganizationID = otherOrgID
			row.Payload = canonicalJSON(payload)
		}},
		{"project", func(row *eventRow) {
			var payload WorkItemEvent
			_ = json.Unmarshal(row.Payload, &payload)
			payload.WorkItem.ProjectID = otherProjectID
			row.Payload = canonicalJSON(payload)
		}},
		{"actor", func(row *eventRow) { row.Projection.ActorID = otherActorID }},
		{"principal", func(row *eventRow) {
			principal := otherActorID
			row.Projection.PrincipalID = &principal
		}},
		{"request", func(row *eventRow) { row.Projection.RequestID = "different-batch-request" }},
		{"timestamp", func(row *eventRow) {
			row.Projection.OccurredAt = otherTime
			var payload WorkItemEvent
			_ = json.Unmarshal(row.Payload, &payload)
			payload.WorkItem.UpdatedAt = otherTime
			row.Payload = canonicalJSON(payload)
		}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			tampered := append([]eventRow(nil), events...)
			test.mutate(&tampered[len(tampered)-1])
			snapshot, failure := rebuildSnapshot("inconsistent-batch-"+test.name, tampered)
			if failure == nil || failure.Code != "invalid_event_payload" ||
				failure.Detail != "work item batch command identity is inconsistent" ||
				failure.Sequence != 4 || failure.EventID != firstEventID {
				t.Fatalf("inconsistent %s identity did not fail at the command's first event: %+v", test.name, failure)
			}
			if len(snapshot.Projects) != 0 || len(snapshot.WorkItems) != 0 || len(snapshot.Activity) != 0 {
				t.Fatalf("inconsistent %s identity built a partial projection: %+v", test.name, snapshot)
			}
			if _, failure := rebuildSnapshot("recovered-batch-"+test.name, events); failure != nil {
				t.Fatalf("restored %s identity did not recover: %v", test.name, failure)
			}
		})
	}
}

func TestReplayRejectsDuplicateTargetWithinBatch(t *testing.T) {
	t.Parallel()
	const (
		organizationID = "00000000-0000-4000-8000-000000000010"
		projectID      = "00000000-0000-4000-8000-000000000020"
		workID         = "00000000-0000-4000-8000-000000000030"
		actorID        = "00000000-0000-4000-8000-000000000040"
		batchCommandID = "00000000-0000-4000-8000-000000000050"
		createdAt      = "2026-07-13T10:00:00.123456Z"
		assignedAt     = "2026-07-13T10:01:00.123456Z"
		transitionedAt = "2026-07-13T10:02:00.123456Z"
	)
	item := WorkItemRecord{ID: workID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Repeated batch target", Description: "One aggregate may occur only once per batch", State: "open",
		Priority: "normal", Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt}
	events := []eventRow{
		{Projection: eventProjection{Sequence: 1, EventID: "00000000-0000-4000-8000-000000000101",
			EventType: "project.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project",
			AggregateID: projectID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID},
			Payload: canonicalJSON(Project{ID: projectID, OrganizationID: organizationID, Mode: "exploration", State: "proposed", Version: 1})},
		{Projection: eventProjection{Sequence: 2, EventID: "00000000-0000-4000-8000-000000000102",
			EventType: "work_item.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: workID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000201", RequestID: "create-repeated-target", OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "create", EvidenceIDs: []string{}})},
	}
	item.AssigneeID, item.Version, item.UpdatedAt = &item.CreatedBy, 2, assignedAt
	events = append(events, eventRow{Projection: eventProjection{Sequence: 3,
		EventID: "00000000-0000-4000-8000-000000000103", EventType: "work_item.assigned", SchemaVersion: 1,
		OrganizationID: organizationID, AggregateType: "work_item", AggregateID: workID, AggregateVersion: 2,
		ActorKind: "human", ActorID: actorID, CommandID: "00000000-0000-4000-8000-000000000202",
		RequestID: "assign-repeated-target", OccurredAt: assignedAt},
		Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "assign", EvidenceIDs: []string{}})})
	item.State, item.Version, item.UpdatedAt = "in_progress", 3, transitionedAt
	events = append(events, eventRow{Projection: eventProjection{Sequence: 4,
		EventID: "00000000-0000-4000-8000-000000000104", EventType: "work_item.started", SchemaVersion: 1,
		OrganizationID: organizationID, AggregateType: "work_item", AggregateID: workID, AggregateVersion: 3,
		ActorKind: "human", ActorID: actorID, CommandID: batchCommandID, RequestID: "duplicate-target-batch",
		OccurredAt: transitionedAt}, Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "start",
		Reason: "First valid transition", EvidenceIDs: []string{}, Batch: true})})
	item.State, item.Version = "cancelled", 4
	events = append(events, eventRow{Projection: eventProjection{Sequence: 5,
		EventID: "00000000-0000-4000-8000-000000000105", EventType: "work_item.cancelled", SchemaVersion: 1,
		OrganizationID: organizationID, AggregateType: "work_item", AggregateID: workID, AggregateVersion: 4,
		ActorKind: "human", ActorID: actorID, CommandID: batchCommandID, RequestID: "duplicate-target-batch",
		OccurredAt: transitionedAt}, Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "cancel",
		Reason: "Second individually valid transition", EvidenceIDs: []string{}, Batch: true})})

	_, failure := rebuildSnapshot("duplicate-target-batch", events)
	if failure == nil || failure.Code != "invalid_event_payload" || failure.Detail != "work item batch command repeats an aggregate" ||
		failure.Sequence != 4 || failure.EventID != "00000000-0000-4000-8000-000000000104" {
		t.Fatalf("duplicate target within a batch did not fail closed: %+v", failure)
	}
}

func TestReplayRejectsMixedMemberWithinBatchCommandBeforeProjection(t *testing.T) {
	t.Parallel()
	const (
		organizationID  = "00000000-0000-4000-8000-000000000010"
		projectID       = "00000000-0000-4000-8000-000000000020"
		workID          = "00000000-0000-4000-8000-000000000030"
		targetID        = "00000000-0000-4000-8000-000000000031"
		createdWorkID   = "00000000-0000-4000-8000-000000000032"
		dependencyID    = "00000000-0000-4000-8000-000000000033"
		actorID         = "00000000-0000-4000-8000-000000000040"
		batchCommandID  = "00000000-0000-4000-8000-000000000050"
		createdAt       = "2026-07-13T10:00:00.123456Z"
		assignedAt      = "2026-07-13T10:01:00.123456Z"
		batchAt         = "2026-07-13T10:02:00.123456Z"
		ordinaryAt      = "2026-07-13T10:03:00.123456Z"
		batchEventID    = "00000000-0000-4000-8000-000000000105"
		ordinaryEventID = "00000000-0000-4000-8000-000000000106"
	)
	item := WorkItemRecord{ID: workID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Mixed batch target", Description: "Batch command membership is closed", State: "open",
		Priority: "normal", Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt}
	target := WorkItemRecord{ID: targetID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Dependency target", Description: "Canonical dependency endpoint", State: "open",
		Priority: "normal", Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt}
	events := []eventRow{
		{Projection: eventProjection{Sequence: 1, EventID: "00000000-0000-4000-8000-000000000101",
			EventType: "project.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project",
			AggregateID: projectID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID},
			Payload: canonicalJSON(Project{ID: projectID, OrganizationID: organizationID, Mode: "exploration", State: "proposed", Version: 1})},
		{Projection: eventProjection{Sequence: 2, EventID: "00000000-0000-4000-8000-000000000102",
			EventType: "work_item.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: workID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000201", RequestID: "create-mixed-target", OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "create", EvidenceIDs: []string{}})},
		{Projection: eventProjection{Sequence: 3, EventID: "00000000-0000-4000-8000-000000000103",
			EventType: "work_item.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: targetID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000202", RequestID: "create-dependency-target", OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: target, Command: "create", EvidenceIDs: []string{}})},
	}
	item.AssigneeID, item.Version, item.UpdatedAt = &item.CreatedBy, 2, assignedAt
	events = append(events, eventRow{Projection: eventProjection{Sequence: 4,
		EventID: "00000000-0000-4000-8000-000000000104", EventType: "work_item.assigned", SchemaVersion: 1,
		OrganizationID: organizationID, AggregateType: "work_item", AggregateID: workID, AggregateVersion: 2,
		ActorKind: "human", ActorID: actorID, CommandID: "00000000-0000-4000-8000-000000000203",
		RequestID: "assign-mixed-target", OccurredAt: assignedAt},
		Payload: canonicalJSON(WorkItemEvent{WorkItem: item, Command: "assign", EvidenceIDs: []string{}})})
	item.State, item.Version, item.UpdatedAt = "in_progress", 3, batchAt
	events = append(events, eventRow{Projection: eventProjection{Sequence: 5, EventID: batchEventID,
		EventType: "work_item.started", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
		AggregateID: workID, AggregateVersion: 3, ActorKind: "human", ActorID: actorID, CommandID: batchCommandID,
		RequestID: "mixed-command-batch", OccurredAt: batchAt}, Payload: canonicalJSON(WorkItemEvent{WorkItem: item,
		Command: "start", Reason: "Valid one-item batch", EvidenceIDs: []string{}, Batch: true})})

	updated := item
	updated.Description, updated.Version, updated.UpdatedAt = "Ordinary update after the batch", 4, ordinaryAt
	assigned := item
	assigned.Version, assigned.UpdatedAt = 4, ordinaryAt
	created := WorkItemRecord{ID: createdWorkID, OrganizationID: organizationID, ProjectID: projectID,
		Title: "Ordinary create", Description: "Separate production command", State: "open", Priority: "normal",
		Version: 1, CreatedBy: actorID, CreatedAt: ordinaryAt, UpdatedAt: ordinaryAt}
	dependencySource := item
	dependencySource.Version, dependencySource.UpdatedAt = 4, ordinaryAt
	dependency := WorkItemDependency{ID: dependencyID, OrganizationID: organizationID, SourceWorkItemID: workID,
		TargetWorkItemID: targetID, Kind: "relates", Version: 1, CreatedBy: actorID, CreatedAt: ordinaryAt}
	tests := []struct {
		name     string
		ordinary eventRow
		tamper   func(*eventRow)
	}{
		{"update", eventRow{Projection: eventProjection{Sequence: 6, EventID: ordinaryEventID,
			EventType: "work_item.updated", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: workID, AggregateVersion: 4, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000204", RequestID: "ordinary-update", OccurredAt: ordinaryAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: updated, Command: "update", EvidenceIDs: []string{}})},
			func(row *eventRow) {
				var payload WorkItemEvent
				_ = json.Unmarshal(row.Payload, &payload)
				payload.WorkItem.UpdatedAt = batchAt
				row.Payload = canonicalJSON(payload)
			}},
		{"assign", eventRow{Projection: eventProjection{Sequence: 6, EventID: ordinaryEventID,
			EventType: "work_item.assigned", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: workID, AggregateVersion: 4, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000205", RequestID: "ordinary-assign", OccurredAt: ordinaryAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: assigned, Command: "assign", EvidenceIDs: []string{}})},
			func(row *eventRow) {
				var payload WorkItemEvent
				_ = json.Unmarshal(row.Payload, &payload)
				payload.WorkItem.UpdatedAt = batchAt
				row.Payload = canonicalJSON(payload)
			}},
		{"create", eventRow{Projection: eventProjection{Sequence: 6, EventID: ordinaryEventID,
			EventType: "work_item.created", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: createdWorkID, AggregateVersion: 1, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000206", RequestID: "ordinary-create", OccurredAt: ordinaryAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: created, Command: "create", EvidenceIDs: []string{}})},
			func(row *eventRow) {
				var payload WorkItemEvent
				_ = json.Unmarshal(row.Payload, &payload)
				payload.WorkItem.CreatedAt, payload.WorkItem.UpdatedAt = batchAt, batchAt
				row.Payload = canonicalJSON(payload)
			}},
		{"dependency", eventRow{Projection: eventProjection{Sequence: 6, EventID: ordinaryEventID,
			EventType: "dependency.added", SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item",
			AggregateID: workID, AggregateVersion: 4, ActorKind: "human", ActorID: actorID,
			CommandID: "00000000-0000-4000-8000-000000000207", RequestID: "ordinary-dependency", OccurredAt: ordinaryAt},
			Payload: canonicalJSON(WorkDependencyEvent{Dependency: dependency, Source: dependencySource, Removed: false})},
			func(row *eventRow) {
				var payload WorkDependencyEvent
				_ = json.Unmarshal(row.Payload, &payload)
				payload.Dependency.CreatedAt, payload.Source.UpdatedAt = batchAt, batchAt
				row.Payload = canonicalJSON(payload)
			}},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			healthy := append(append([]eventRow(nil), events...), test.ordinary)
			if _, failure := rebuildSnapshot("healthy-"+test.name, healthy); failure != nil {
				t.Fatalf("ordinary %s history did not replay before tamper: %v", test.name, failure)
			}
			tampered := append([]eventRow(nil), healthy...)
			member := &tampered[len(tampered)-1]
			member.Projection.CommandID = batchCommandID
			member.Projection.RequestID = "mixed-command-batch"
			member.Projection.OccurredAt = batchAt
			test.tamper(member)
			snapshot, failure := rebuildSnapshot("mixed-"+test.name, tampered)
			if failure == nil || failure.Code != "invalid_event_payload" ||
				failure.Detail != "work item batch command contains a non-transition member" ||
				failure.Sequence != 5 || failure.EventID != batchEventID {
				t.Fatalf("mixed %s batch member did not fail at the command's first event: %+v", test.name, failure)
			}
			if len(snapshot.Projects) != 0 || len(snapshot.WorkItems) != 0 || len(snapshot.Activity) != 0 {
				t.Fatalf("mixed %s batch built a partial projection: %+v", test.name, snapshot)
			}
			if _, failure := rebuildSnapshot("recovered-"+test.name, healthy); failure != nil {
				t.Fatalf("restored %s history did not recover: %v", test.name, failure)
			}
		})
	}
}

func TestReplayAcceptsAuthorizedCrossProjectDependencyLifecycle(t *testing.T) {
	t.Parallel()
	const (
		organizationID  = "00000000-0000-4000-8000-000000000010"
		sourceProjectID = "00000000-0000-4000-8000-000000000020"
		targetProjectID = "00000000-0000-4000-8000-000000000021"
		sourceID        = "00000000-0000-4000-8000-000000000030"
		targetID        = "00000000-0000-4000-8000-000000000031"
		dependencyID    = "00000000-0000-4000-8000-000000000032"
		actorID         = "00000000-0000-4000-8000-000000000040"
		createdAt       = "2026-07-13T10:00:00.123456Z"
		addedAt         = "2026-07-13T10:01:00.123456Z"
		removedAt       = "2026-07-13T10:02:00.123456Z"
	)
	source := WorkItemRecord{ID: sourceID, OrganizationID: organizationID, ProjectID: sourceProjectID,
		Title: "Cross-project source", Description: "Authorized source endpoint", State: "open", Priority: "normal",
		Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt}
	target := WorkItemRecord{ID: targetID, OrganizationID: organizationID, ProjectID: targetProjectID,
		Title: "Cross-project target", Description: "Authorized target endpoint", State: "open", Priority: "normal",
		Version: 1, CreatedBy: actorID, CreatedAt: createdAt, UpdatedAt: createdAt}
	dependency := WorkItemDependency{ID: dependencyID, OrganizationID: organizationID, SourceWorkItemID: sourceID,
		TargetWorkItemID: targetID, Kind: "relates", Version: 1, CreatedBy: actorID, CreatedAt: addedAt}
	events := []eventRow{
		{Projection: eventProjection{Sequence: 1, EventID: "00000000-0000-4000-8000-000000000101", EventType: "project.created",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project", AggregateID: sourceProjectID,
			AggregateVersion: 1, ActorKind: "human", ActorID: actorID},
			Payload: canonicalJSON(Project{ID: sourceProjectID, OrganizationID: organizationID, Mode: "exploration", State: "proposed", Version: 1})},
		{Projection: eventProjection{Sequence: 2, EventID: "00000000-0000-4000-8000-000000000102", EventType: "project.created",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "project", AggregateID: targetProjectID,
			AggregateVersion: 1, ActorKind: "human", ActorID: actorID},
			Payload: canonicalJSON(Project{ID: targetProjectID, OrganizationID: organizationID, Mode: "exploration", State: "proposed", Version: 1})},
		{Projection: eventProjection{Sequence: 3, EventID: "00000000-0000-4000-8000-000000000103", EventType: "work_item.created",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item", AggregateID: sourceID,
			AggregateVersion: 1, ActorKind: "human", ActorID: actorID, CommandID: "00000000-0000-4000-8000-000000000203",
			RequestID: "create-cross-project-source", OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: source, Command: "create", EvidenceIDs: []string{}})},
		{Projection: eventProjection{Sequence: 4, EventID: "00000000-0000-4000-8000-000000000104", EventType: "work_item.created",
			SchemaVersion: 1, OrganizationID: organizationID, AggregateType: "work_item", AggregateID: targetID,
			AggregateVersion: 1, ActorKind: "human", ActorID: actorID, CommandID: "00000000-0000-4000-8000-000000000204",
			RequestID: "create-cross-project-target", OccurredAt: createdAt},
			Payload: canonicalJSON(WorkItemEvent{WorkItem: target, Command: "create", EvidenceIDs: []string{}})},
	}
	source.Version, source.UpdatedAt = 2, addedAt
	events = append(events, eventRow{Projection: eventProjection{Sequence: 5,
		EventID: "00000000-0000-4000-8000-000000000105", EventType: "dependency.added", SchemaVersion: 1,
		OrganizationID: organizationID, AggregateType: "work_item", AggregateID: sourceID, AggregateVersion: 2,
		ActorKind: "human", ActorID: actorID, CommandID: "00000000-0000-4000-8000-000000000205",
		RequestID: "add-cross-project-dependency", OccurredAt: addedAt},
		Payload: canonicalJSON(WorkDependencyEvent{Dependency: dependency, Source: source, Removed: false})})
	source.Version, source.UpdatedAt = 3, removedAt
	events = append(events, eventRow{Projection: eventProjection{Sequence: 6,
		EventID: "00000000-0000-4000-8000-000000000106", EventType: "dependency.removed", SchemaVersion: 1,
		OrganizationID: organizationID, AggregateType: "work_item", AggregateID: sourceID, AggregateVersion: 3,
		ActorKind: "human", ActorID: actorID, CommandID: "00000000-0000-4000-8000-000000000206",
		RequestID: "remove-cross-project-dependency", OccurredAt: removedAt},
		Payload: canonicalJSON(WorkDependencyEvent{Dependency: dependency, Source: source, Removed: true})})

	snapshot, failure := rebuildSnapshot("cross-project-dependency-lifecycle", events)
	if failure != nil {
		t.Fatalf("authorized cross-project dependency did not replay: %v", failure)
	}
	if len(snapshot.Dependencies) != 0 || len(snapshot.WorkItems) != 2 {
		t.Fatalf("cross-project dependency lifecycle projected incorrectly: %+v", snapshot)
	}
	for _, item := range snapshot.WorkItems {
		if item.ID == sourceID && item.Version != 3 {
			t.Fatalf("cross-project source version = %d, want 3", item.Version)
		}
	}
}
