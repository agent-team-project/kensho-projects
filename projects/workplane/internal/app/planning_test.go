package app

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
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
	required := true
	deliverable := generated.DeliverableInput{Title: "Release", Description: "Observable output", Required: &required,
		Weight: 1000, State: "ready", AcceptanceCriteria: []string{"Exact evidence passes"}}
	if _, ok := normalizeDeliverableInput(deliverable); !ok {
		t.Fatal("valid deliverable rejected")
	}
	for name, mutation := range map[string]func(*generated.DeliverableInput){
		"missing required": func(value *generated.DeliverableInput) { value.Required = nil },
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
	optional := false
	deliverable.Required = &optional
	if _, ok := normalizeDeliverableInput(deliverable); !ok {
		t.Fatal("explicit false requiredness was rejected")
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

func TestM2CForecastHistoryPublicReadsFailClosedAfterRowIterationError(t *testing.T) {
	t.Parallel()
	const (
		projectID     = "00000000-0000-4000-8000-000000000010"
		deliverableID = "00000000-0000-4000-8000-000000000020"
	)
	tests := []struct {
		name      string
		pathValue string
		call      func(*Service, context.Context, generated.Request) (generated.Response, error)
	}{
		{
			name:      "project forecast history",
			pathValue: projectID,
			call: func(service *Service, ctx context.Context, request generated.Request) (generated.Response, error) {
				return service.ListProjectForecasts(ctx, request)
			},
		},
		{
			name:      "deliverable forecast history",
			pathValue: deliverableID,
			call: func(service *Service, ctx context.Context, request generated.Request) (generated.Response, error) {
				return service.ListDeliverableForecasts(ctx, request)
			},
		},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			t.Parallel()
			for _, scenario := range []struct {
				name         string
				iterationErr error
				wantStatus   int
			}{
				{name: "valid row completes", wantStatus: http.StatusOK},
				{name: "error after valid row fails closed", iterationErr: errForecastHistoryInterrupted, wantStatus: http.StatusServiceUnavailable},
			} {
				scenario := scenario
				t.Run(scenario.name, func(t *testing.T) {
					t.Parallel()
					connector := &forecastHistoryTestConnector{iterationErr: scenario.iterationErr}
					database := sql.OpenDB(connector)
					t.Cleanup(func() { _ = database.Close() })
					service := &Service{
						db:     database,
						config: Config{TokenHashKey: []byte("forecast-history-test-token-key")},
						now:    func() time.Time { return time.Date(2026, 7, 13, 12, 0, 0, 0, time.UTC) },
					}
					httpRequest := httptest.NewRequest(http.MethodGet, "/forecast-history", nil)
					if test.name == "project forecast history" {
						httpRequest.SetPathValue("project_id", test.pathValue)
					} else {
						httpRequest.SetPathValue("deliverable_id", test.pathValue)
					}
					response, err := test.call(service, context.Background(), generated.Request{
						HTTPRequest: httpRequest,
						Security:    generated.RequestSecurity{SessionCookie: "session-token"},
					})
					if err != nil {
						t.Fatalf("public forecast history returned error: %v", err)
					}
					if response.Status != scenario.wantStatus {
						t.Fatalf("status=%d want=%d body=%#v", response.Status, scenario.wantStatus, response.Body)
					}
					if scenario.iterationErr == nil {
						items, ok := response.Body.([]ForecastView)
						if !ok || len(items) != 1 {
							t.Fatalf("valid history body=%#v, want one forecast", response.Body)
						}
						return
					}
					failure, ok := response.Body.(Problem)
					if !ok || failure.Code != "service_unavailable" {
						t.Fatalf("iteration failure body=%#v, want service_unavailable problem", response.Body)
					}
				})
			}
		})
	}
}

var errForecastHistoryInterrupted = errors.New("forecast history interrupted after a row")

type forecastHistoryTestConnector struct {
	iterationErr error
}

func (connector *forecastHistoryTestConnector) Connect(context.Context) (driver.Conn, error) {
	return &forecastHistoryTestConn{connector: connector}, nil
}

func (*forecastHistoryTestConnector) Driver() driver.Driver { return forecastHistoryTestDriver{} }

type forecastHistoryTestDriver struct{}

func (forecastHistoryTestDriver) Open(string) (driver.Conn, error) {
	return nil, errors.New("forecast history test driver requires its connector")
}

type forecastHistoryTestConn struct {
	connector *forecastHistoryTestConnector
}

func (*forecastHistoryTestConn) Prepare(string) (driver.Stmt, error) {
	return nil, errors.New("forecast history test driver does not prepare statements")
}

func (*forecastHistoryTestConn) Close() error { return nil }
func (*forecastHistoryTestConn) Begin() (driver.Tx, error) {
	return nil, errors.New("forecast history test driver does not begin transactions")
}

func (connection *forecastHistoryTestConn) QueryContext(_ context.Context, query string, arguments []driver.NamedValue) (driver.Rows, error) {
	const (
		actorID        = "00000000-0000-4000-8000-000000000001"
		organizationID = "00000000-0000-4000-8000-000000000002"
		projectID      = "00000000-0000-4000-8000-000000000010"
		deliverableID  = "00000000-0000-4000-8000-000000000020"
		forecastID     = "00000000-0000-4000-8000-000000000030"
	)
	rows := func(columns []string, values ...driver.Value) driver.Rows {
		return &forecastHistoryTestRows{columns: columns, values: [][]driver.Value{values}}
	}
	switch {
	case strings.Contains(query, "FROM human_sessions"):
		return rows([]string{"id", "kind", "status", "organization_id", "role"}, actorID, "human", "active", organizationID, "owner"), nil
	case strings.Contains(query, "SELECT project_id FROM deliverables"):
		return rows([]string{"project_id"}, projectID), nil
	case strings.Contains(query, "SELECT visibility::text,created_by"):
		return rows([]string{"visibility", "created_by", "membership_role"}, "private", actorID, "owner"), nil
	case strings.Contains(query, "FROM forecasts forecast"):
		var deliverable driver.Value
		if len(arguments) == 3 && arguments[2].Value != nil {
			deliverable = deliverableID
		}
		created := time.Date(2026, 7, 13, 10, 0, 0, 0, time.UTC)
		return &forecastHistoryTestRows{
			columns: []string{"id", "organization_id", "project_id", "deliverable_id", "p50_at", "p90_at", "review_after", "basis", "assumptions", "reason_codes", "impact", "supersedes_id", "created_by", "created_at", "current"},
			values: [][]driver.Value{{
				forecastID, organizationID, projectID, deliverable,
				created.Add(24 * time.Hour), created.Add(48 * time.Hour), created.Add(12 * time.Hour),
				"Measured throughput", []byte(`["No scope expansion"]`), `{"new-evidence"}`,
				"No gate changes", nil, actorID, created, true,
			}},
			iterationErr: connection.connector.iterationErr,
		}, nil
	default:
		return nil, fmt.Errorf("unexpected forecast history query: %s", query)
	}
}

type forecastHistoryTestRows struct {
	columns      []string
	values       [][]driver.Value
	index        int
	iterationErr error
}

func (rows *forecastHistoryTestRows) Columns() []string { return rows.columns }
func (*forecastHistoryTestRows) Close() error           { return nil }

func (rows *forecastHistoryTestRows) Next(destination []driver.Value) error {
	if rows.index < len(rows.values) {
		copy(destination, rows.values[rows.index])
		rows.index++
		return nil
	}
	if rows.iterationErr != nil {
		err := rows.iterationErr
		rows.iterationErr = nil
		return err
	}
	return io.EOF
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
