package app

import "testing"

func TestM2ClaimsOnlyAcceptedWalkingSliceBehavior(t *testing.T) {
	t.Parallel()
	if BuildStage != "m2-planning-contracts" {
		t.Fatalf("unexpected build stage %q", BuildStage)
	}
	if len(Capabilities) != 18 {
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
