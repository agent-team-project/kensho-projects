package app

import "testing"

func TestM1ClaimsOnlyFrozenWalkingSliceBehavior(t *testing.T) {
	t.Parallel()
	if BuildStage != "m1-walking-slice" {
		t.Fatalf("unexpected build stage %q", BuildStage)
	}
	if len(Capabilities) != 5 {
		t.Fatalf("unexpected M1 capability count %d", len(Capabilities))
	}
}
