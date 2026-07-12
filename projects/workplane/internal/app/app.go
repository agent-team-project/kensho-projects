// Package app defines the Workplane application seam without implementing M1 behavior.
package app

// BuildStage is returned by the M0 health surface and prevents the substrate
// from being mistaken for an implemented walking slice.
const BuildStage = "m0-contract-substrate"

// Capabilities identifies the only behavior available at M0.
var Capabilities = []string{"health", "contract-generation", "contract-validation"}
