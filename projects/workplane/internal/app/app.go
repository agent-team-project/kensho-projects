// Package app implements the serialized Workplane application boundary.
package app

// BuildStage is exposed by the health surface and evidence harness.
const BuildStage = "m2-work-dependencies"

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
	"immutable-attributable-evidence",
	"evidence-supersession-chains",
	"hard-and-soft-review-gates",
	"immutable-verdict-history",
	"actionable-finding-resolution",
	"deliverable-submit-bounce-resubmit-approve",
	"effective-principal-separation-of-duty",
	"human-policy-waivers",
	"review-replay-integrity",
	"fixed-work-item-lifecycle",
	"derived-work-blocking",
	"typed-cross-project-dependencies",
	"blocking-dependency-dag",
	"atomic-work-batch-transitions",
	"work-replay-integrity",
}
