package app

import (
	"encoding/json"
	"testing"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
)

func TestWorkTransitionVocabularyIsClosed(t *testing.T) {
	allowed := map[string]string{
		"open/start":                 "in_progress",
		"open/cancel":                "cancelled",
		"in_progress/request_review": "in_review",
		"in_progress/cancel":         "cancelled",
		"in_review/bounce":           "in_progress",
		"in_review/accept":           "done",
		"in_review/cancel":           "cancelled",
	}
	states := []string{"open", "in_progress", "in_review", "done", "cancelled"}
	commands := []string{"start", "request_review", "bounce", "accept", "cancel"}
	for _, state := range states {
		for _, command := range commands {
			target, _, ok := workTransitionTarget(state, command)
			expected, admitted := allowed[state+"/"+command]
			if ok != admitted || target != expected {
				t.Fatalf("transition %s/%s = (%q,%v), want (%q,%v)", state, command, target, ok, expected, admitted)
			}
		}
	}
}

func TestWorkUpdatePresenceAndBatchDuplicatesFailClosed(t *testing.T) {
	for _, body := range []string{`{}`, `{"title":null}`, `{"priority":"unknown"}`, `{"unexpected":"value"}`} {
		if _, ok := normalizeWorkItemUpdate(json.RawMessage(body)); ok {
			t.Fatalf("invalid presence-aware update accepted: %s", body)
		}
	}
	if value, ok := normalizeWorkItemUpdate(json.RawMessage(`{"title":" current title "}`)); !ok || value.Title == nil || *value.Title != "current title" {
		t.Fatalf("valid update rejected or not normalized: %+v ok=%v", value, ok)
	}
	entry := generated.WorkItemBatchTransitionEntry{WorkItemID: "00000000-0000-4000-8000-000000000001",
		ExpectedVersion: 1, Command: "cancel", Reason: "No longer required", EvidenceIDs: []string{}}
	if _, ok := normalizeBatchTransitions(generated.WorkItemBatchTransitionInput{Items: []generated.WorkItemBatchTransitionEntry{entry, entry}}); ok {
		t.Fatal("duplicate batch target was accepted")
	}
}

func TestWorkTransitionMetadataIsCommandSpecific(t *testing.T) {
	evidenceID := "00000000-0000-4000-8000-000000000031"
	findingID := "00000000-0000-4000-8000-000000000032"
	valid := []struct {
		command  string
		evidence []string
		finding  *string
	}{
		{"start", []string{}, nil},
		{"request_review", []string{evidenceID}, nil},
		{"bounce", []string{}, &findingID},
		{"accept", []string{evidenceID}, nil},
		{"cancel", []string{}, nil},
	}
	for _, test := range valid {
		if _, _, _, _, ok := normalizeWorkTransition(test.command, "Current reason", test.evidence, test.finding); !ok {
			t.Fatalf("canonical %s metadata was rejected", test.command)
		}
	}
	invalid := []struct {
		command  string
		evidence []string
		finding  *string
	}{
		{"start", []string{evidenceID}, nil},
		{"cancel", []string{}, &findingID},
		{"request_review", []string{}, nil},
		{"accept", []string{evidenceID}, &findingID},
		{"bounce", []string{evidenceID}, &findingID},
		{"bounce", []string{}, nil},
	}
	for _, test := range invalid {
		if _, _, _, _, ok := normalizeWorkTransition(test.command, "Current reason", test.evidence, test.finding); ok {
			t.Fatalf("impossible %s metadata was accepted: evidence=%v finding=%v", test.command, test.evidence, test.finding)
		}
	}
}

func TestDelegatedWorkPolicyBuildsIndependentPrincipalActors(t *testing.T) {
	principalID := "00000000-0000-4000-8000-000000000001"
	actor := Actor{ID: "00000000-0000-4000-8000-000000000002", Kind: "agent", PrincipalID: &principalID,
		OrganizationID: "00000000-0000-4000-8000-000000000010", Role: "member", DelegatedRole: "owner"}
	direct, delegated, ok := independentWorkProjectActors(actor)
	if !ok || direct.ID != actor.ID || direct.Kind != "human" || direct.Role != actor.Role || direct.PrincipalID != nil ||
		delegated.ID != principalID || delegated.Kind != "human" || delegated.Role != actor.DelegatedRole || delegated.PrincipalID != nil ||
		direct.OrganizationID != actor.OrganizationID || delegated.OrganizationID != actor.OrganizationID {
		t.Fatalf("delegated policy actors are not independent: direct=%+v delegated=%+v ok=%v", direct, delegated, ok)
	}
	if _, _, ok := independentWorkProjectActors(Actor{ID: actor.ID, Kind: "agent"}); ok {
		t.Fatal("agent without a distinct delegated principal was accepted")
	}
}

func TestReplayBlockingUsesAtomicBatchFinalState(t *testing.T) {
	source := WorkItemRecord{ID: "source", OrganizationID: "org", State: "open"}
	target := WorkItemRecord{ID: "target", OrganizationID: "org", State: "open"}
	records := map[string]WorkItemRecord{source.ID: source, target.ID: target}
	dependencies := map[string]WorkItemDependency{"edge": {
		ID: "edge", OrganizationID: "org", SourceWorkItemID: source.ID, TargetWorkItemID: target.ID, Kind: "blocks",
	}}
	if !replayWorkItemView(source, records, dependencies, nil).Blocked {
		t.Fatal("unfinished prerequisite did not block replay projection")
	}
	view := replayWorkItemViewWithFinalStates(source, records, dependencies, nil, map[string]string{target.ID: "cancelled"})
	if view.Blocked || len(view.BlockingReasons) != 0 {
		t.Fatalf("atomic final prerequisite state did not clear replay blocking: %+v", view)
	}
}
