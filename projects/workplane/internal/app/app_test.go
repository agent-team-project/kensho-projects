package app

import "testing"

func TestM2ClaimsOnlyAcceptedWalkingSliceBehavior(t *testing.T) {
	t.Parallel()
	if BuildStage != "m2-evidence-review" {
		t.Fatalf("unexpected build stage %q", BuildStage)
	}
	if len(Capabilities) != 27 {
		t.Fatalf("unexpected accepted capability count %d", len(Capabilities))
	}
}

func TestAgentProjectRestrictionAllows(t *testing.T) {
	t.Parallel()
	const allowedProject = "00000000-0000-4000-8000-000000000010"
	tests := []struct {
		name        string
		projectIDs  map[string]bool
		projectID   string
		wantAllowed bool
	}{
		{name: "unrestricted organization operation", wantAllowed: true},
		{name: "unrestricted project operation", projectID: allowedProject, wantAllowed: true},
		{name: "restricted organization operation", projectIDs: map[string]bool{allowedProject: true}, wantAllowed: false},
		{name: "restricted listed project", projectIDs: map[string]bool{allowedProject: true}, projectID: allowedProject, wantAllowed: true},
		{name: "restricted unlisted project", projectIDs: map[string]bool{allowedProject: true}, projectID: "00000000-0000-4000-8000-000000000099", wantAllowed: false},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := agentProjectRestrictionAllows(test.projectIDs, test.projectID); got != test.wantAllowed {
				t.Fatalf("agentProjectRestrictionAllows(%v, %q) = %t, want %t", test.projectIDs, test.projectID, got, test.wantAllowed)
			}
		})
	}
}

func TestProjectRoleAllowsRequestedAction(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name, role, action string
		want               bool
	}{
		{name: "owner can write target", role: "owner", action: "project.target.write", want: true},
		{name: "owner can satisfy compound decision", role: "owner", action: "decision.record", want: true},
		{name: "contributor can read deliverables", role: "contributor", action: "deliverable.read", want: true},
		{name: "reviewer can read projects", role: "reviewer", action: "project.read", want: true},
		{name: "canonical viewer can read projects", role: "viewer", action: "project.read", want: true},
		{name: "existing observer can read projects", role: "observer", action: "project.read", want: true},
		{name: "existing observer cannot write target", role: "observer", action: "project.target.write", want: false},
		{name: "canonical viewer cannot revise deliverable", role: "viewer", action: "deliverable.edit", want: false},
		{name: "contributor cannot promote project", role: "contributor", action: "project.promote", want: false},
		{name: "unknown role denies", role: "future-role", action: "project.read", want: false},
		{name: "unknown action denies", role: "owner", action: "project.future", want: false},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			if got := projectRoleAllows(test.role, test.action); got != test.want {
				t.Fatalf("projectRoleAllows(%q, %q) = %t, want %t", test.role, test.action, got, test.want)
			}
		})
	}
}
