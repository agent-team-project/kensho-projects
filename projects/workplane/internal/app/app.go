// Package app implements the serialized Workplane M1 application boundary.
package app

// BuildStage is exposed by the health surface and evidence harness.
const BuildStage = "m1-walking-slice"

// Capabilities is deliberately limited to the frozen Track A transaction.
var Capabilities = []string{
	"human-session",
	"agent-bearer",
	"exploration-project",
	"continue-decision",
	"immutable-activity",
}
