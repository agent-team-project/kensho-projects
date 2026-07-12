package app

import "testing"

func TestM0DoesNotClaimWalkingSliceBehavior(t *testing.T) {
	t.Parallel()
	if BuildStage != "m0-contract-substrate" {
		t.Fatalf("unexpected build stage %q", BuildStage)
	}
	if len(Capabilities) != 3 {
		t.Fatalf("unexpected M0 capability count %d", len(Capabilities))
	}
}
