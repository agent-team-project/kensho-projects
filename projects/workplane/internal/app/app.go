// Package app implements the serialized Workplane application boundary.
package app

// BuildStage is exposed by the health surface and evidence harness.
const BuildStage = "m2-planning-contracts"

// Capabilities preserves the M1 surface and adds only the admitted Track B spine.
var Capabilities = []string{
	"human-session",
	"agent-bearer",
	"exploration-project",
	"continue-decision",
	"immutable-activity",
	"transactional-outbox",
	"deterministic-replay",
	"integrity-doctor",
	"organization-websocket",
	"agent-sse",
	"signed-resume-cursors",
	"live-authority-recheck",
	"bounded-backpressure",
	"nonterminal-project-lifecycle",
	"observable-deliverables",
	"p50-p90-forecast-history",
	"target-deadline-distinction",
	"planning-human-agent-parity",
}
