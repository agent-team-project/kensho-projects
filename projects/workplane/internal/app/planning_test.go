package app

import (
	"fmt"
	"testing"
	"time"

	"github.com/agent-team-project/kensho-projects/projects/workplane/internal/api/generated"
)

func TestM2CLifecycleGuardsAreLoadBearing(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name, mode, state, command string
		forecast, required, want   bool
	}{
		{"activate exploration", "exploration", "proposed", "activate", true, false, true},
		{"activate requires forecast", "exploration", "proposed", "activate", false, false, false},
		{"activate exploitation requires deliverable", "exploitation", "proposed", "activate", true, false, false},
		{"activate exploitation", "exploitation", "proposed", "activate", true, true, true},
		{"hold active", "exploration", "active", "hold", false, false, true},
		{"hold proposed denied", "exploration", "proposed", "hold", true, false, false},
		{"resume held", "exploration", "held", "resume", false, false, true},
		{"resume active denied", "exploration", "active", "resume", false, false, false},
		{"promote complete contract", "exploration", "active", "promote", true, true, true},
		{"promote requires forecast", "exploration", "active", "promote", false, true, false},
		{"promote requires deliverable", "exploration", "active", "promote", true, false, false},
		{"promote never repeats", "exploitation", "active", "promote", true, true, false},
		{"terminal command absent", "exploitation", "active", "complete", true, true, false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if got := validLifecycleTransition(test.mode, test.state, test.command, test.forecast, test.required); got != test.want {
				t.Fatalf("transition=%t, want %t", got, test.want)
			}
		})
	}
}

func TestM2CPlanningInputInvariants(t *testing.T) {
	t.Parallel()
	deliverable := generated.DeliverableInput{Title: "Release", Description: "Observable output", Required: true,
		Weight: 1000, State: "ready", AcceptanceCriteria: []string{"Exact evidence passes"}}
	if _, ok := normalizeDeliverableInput(deliverable); !ok {
		t.Fatal("valid deliverable rejected")
	}
	for name, mutation := range map[string]func(*generated.DeliverableInput){
		"zero weight":      func(value *generated.DeliverableInput) { value.Weight = 0 },
		"unknown state":    func(value *generated.DeliverableInput) { value.State = "accepted" },
		"missing criteria": func(value *generated.DeliverableInput) { value.AcceptanceCriteria = nil },
		"blank criterion":  func(value *generated.DeliverableInput) { value.AcceptanceCriteria = []string{"  "} },
	} {
		value := deliverable
		mutation(&value)
		if _, ok := normalizeDeliverableInput(value); ok {
			t.Fatalf("production validator accepted %s", name)
		}
	}

	forecast := generated.ForecastInput{
		P50At: "2026-07-14T00:00:00Z", P90At: "2026-07-15T00:00:00Z", ReviewAfter: "2026-07-13T18:00:00Z",
		Basis: "Measured throughput", Assumptions: []string{"No scope expansion"}, ReasonCodes: []string{"new-evidence"}, Impact: "No gate changes",
	}
	if _, _, _, _, ok := normalizeForecastInput(forecast); !ok {
		t.Fatal("valid forecast rejected")
	}
	forecast.P50At, forecast.P90At = forecast.P90At, forecast.P50At
	if _, _, _, _, ok := normalizeForecastInput(forecast); ok {
		t.Fatal("P50>P90 accepted")
	}
	forecast.P50At, forecast.P90At = "2026-07-14T00:00:00Z", "2026-07-15T00:00:00Z"
	forecast.ReasonCodes = []string{"other", "other"}
	if _, _, _, _, ok := normalizeForecastInput(forecast); ok {
		t.Fatal("duplicate forecast reasons accepted")
	}
}

func TestM2COneHundredThousandCommandSequenceModel(t *testing.T) {
	t.Parallel()
	mode, state := "exploration", "proposed"
	hasForecast, hasRequired := false, false
	seed := uint64(0x6d32632d706c616e)
	for step := 0; step < 100_000; step++ {
		seed = seed*6364136223846793005 + 1442695040888963407
		switch seed % 7 {
		case 0:
			hasForecast = true
		case 1:
			hasRequired = true
		case 2:
			if validLifecycleTransition(mode, state, "activate", hasForecast, hasRequired) {
				state = "active"
			}
		case 3:
			if validLifecycleTransition(mode, state, "hold", hasForecast, hasRequired) {
				state = "held"
			}
		case 4:
			if validLifecycleTransition(mode, state, "resume", hasForecast, hasRequired) {
				state = "active"
			}
		case 5:
			if validLifecycleTransition(mode, state, "promote", hasForecast, hasRequired) {
				mode = "exploitation"
			}
		case 6:
			// Dates becoming stale is attention only: it cannot mutate mode/state.
			_ = time.Unix(int64(seed>>8), 0)
		}
		if state != "proposed" && state != "active" && state != "held" {
			t.Fatalf("step %d reached out-of-scope state %q", step, state)
		}
		if mode != "exploration" && mode != "exploitation" {
			t.Fatalf("step %d reached invalid mode %q", step, mode)
		}
		if mode == "exploitation" && state == "proposed" {
			t.Fatalf("step %d promoted without activation", step)
		}
	}
	if mode != "exploitation" || (state != "active" && state != "held") {
		t.Fatal(fmt.Sprintf("model did not exercise promotion: mode=%s state=%s", mode, state))
	}
}
