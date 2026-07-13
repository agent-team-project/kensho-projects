package app

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"errors"
	"io"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestM2CLivePlanningRejectsEveryPostRowIterationError(t *testing.T) {
	tests := []struct {
		name, failAt string
	}{
		{name: "deliverables", failAt: "deliverables"},
		{name: "forecasts", failAt: "forecasts"},
		{name: "forecast heads", failAt: "forecast_heads"},
		{name: "targets", failAt: "targets"},
		{name: "target heads", failAt: "target_heads"},
		{name: "deadlines", failAt: "deadlines"},
		{name: "deadline heads", failAt: "deadline_heads"},
	}
	for _, test := range tests {
		test := test
		t.Run(test.name, func(t *testing.T) {
			database, connector := durableIterationDatabase(test.failAt)
			t.Cleanup(func() { _ = database.Close() })
			snapshot := projectionSnapshot{}
			err := loadLivePlanning(context.Background(), database, &snapshot)
			if !errors.Is(err, errDurableIterationInterrupted) {
				t.Fatalf("loadLivePlanning error=%v, want injected iteration error", err)
			}
			if connector.rowsServed(test.failAt) != 1 {
				t.Fatalf("%s served %d rows, want one before the error", test.failAt, connector.rowsServed(test.failAt))
			}
		})
	}
}

func TestM2CReplayRejectsPlanningIterationErrorWithoutAdvancingHead(t *testing.T) {
	database, connector := durableIterationDatabase("deliverables")
	t.Cleanup(func() { _ = database.Close() })
	_, err := (&DurableStore{db: database}).Replay(context.Background())
	if !errors.Is(err, errDurableIterationInterrupted) {
		t.Fatalf("Replay error=%v, want injected planning iteration error", err)
	}
	if connector.projectionHeadWrites() != 0 {
		t.Fatalf("Replay wrote projection head %d times after partial planning load", connector.projectionHeadWrites())
	}
	if !connector.transactionRolledBack() {
		t.Fatal("Replay did not roll back after partial planning load")
	}
}

func TestM2CDoctorRejectsPlanningIterationErrors(t *testing.T) {
	for _, failAt := range []string{"planning_orphans", "head_mismatches"} {
		failAt := failAt
		t.Run(failAt, func(t *testing.T) {
			database, connector := durableIterationDatabase(failAt)
			t.Cleanup(func() { _ = database.Close() })
			findings, err := doctor(context.Background(), database)
			if !errors.Is(err, errDurableIterationInterrupted) {
				t.Fatalf("doctor error=%v findings=%+v, want injected iteration error", err, findings)
			}
			if findings != nil {
				t.Fatalf("doctor returned partial findings after iteration error: %+v", findings)
			}
			if connector.rowsServed(failAt) != 1 {
				t.Fatalf("%s served %d rows, want one before the error", failAt, connector.rowsServed(failAt))
			}
		})
	}
}

var errDurableIterationInterrupted = errors.New("durable planning rows interrupted after a row")

type durableIterationConnector struct {
	failAt string
	mu     sync.Mutex
	served map[string]int
	heads  int
	rolled bool
}

func durableIterationDatabase(failAt string) (*sql.DB, *durableIterationConnector) {
	connector := &durableIterationConnector{failAt: failAt, served: make(map[string]int)}
	database := sql.OpenDB(connector)
	database.SetMaxOpenConns(1)
	return database, connector
}

func (connector *durableIterationConnector) Connect(context.Context) (driver.Conn, error) {
	return &durableIterationConn{connector: connector}, nil
}

func (*durableIterationConnector) Driver() driver.Driver { return durableIterationDriver{} }

func (connector *durableIterationConnector) rowsServed(kind string) int {
	connector.mu.Lock()
	defer connector.mu.Unlock()
	return connector.served[kind]
}

func (connector *durableIterationConnector) projectionHeadWrites() int {
	connector.mu.Lock()
	defer connector.mu.Unlock()
	return connector.heads
}

func (connector *durableIterationConnector) transactionRolledBack() bool {
	connector.mu.Lock()
	defer connector.mu.Unlock()
	return connector.rolled
}

type durableIterationDriver struct{}

func (durableIterationDriver) Open(string) (driver.Conn, error) {
	return nil, errors.New("durable iteration test driver requires its connector")
}

type durableIterationConn struct {
	connector *durableIterationConnector
}

func (*durableIterationConn) Prepare(string) (driver.Stmt, error) {
	return nil, errors.New("durable iteration test driver does not prepare statements")
}

func (*durableIterationConn) Close() error              { return nil }
func (*durableIterationConn) Begin() (driver.Tx, error) { return nil, errors.New("use BeginTx") }

func (connection *durableIterationConn) BeginTx(context.Context, driver.TxOptions) (driver.Tx, error) {
	return &durableIterationTx{connector: connection.connector}, nil
}

func (connection *durableIterationConn) ExecContext(_ context.Context, query string, _ []driver.NamedValue) (driver.Result, error) {
	if strings.Contains(query, "INSERT INTO projection_heads") {
		connection.connector.mu.Lock()
		connection.connector.heads++
		connection.connector.mu.Unlock()
	}
	return driver.RowsAffected(1), nil
}

func (connection *durableIterationConn) QueryContext(_ context.Context, query string, _ []driver.NamedValue) (driver.Rows, error) {
	switch {
	case strings.Contains(query, "FROM domain_events ORDER BY sequence"):
		return connection.rows("events", []string{"sequence"}, nil), nil
	case strings.Contains(query, "FROM projects") && strings.Contains(query, "ORDER BY organization_id,id"):
		return connection.rows("projects", []string{"id"}, nil), nil
	case strings.Contains(query, "FROM decisions d"):
		return connection.rows("decisions", []string{"id"}, nil), nil
	case strings.Contains(query, "FROM deliverables") && strings.Contains(query, "ORDER BY project_id,id"):
		created := time.Date(2026, 7, 13, 10, 0, 0, 0, time.UTC)
		return connection.rows("deliverables",
			[]string{"id", "organization_id", "project_id", "title", "description", "required", "weight", "state", "acceptance_criteria", "version", "created_by", "created_at", "updated_at"},
			[]driver.Value{"00000000-0000-4000-8000-000000000030", "00000000-0000-4000-8000-000000000010", "00000000-0000-4000-8000-000000000020", "Release", "Observable output", true, int64(1000), "ready", []byte(`["Exact evidence passes"]`), int64(1), "00000000-0000-4000-8000-000000000040", created, created}), nil
	case strings.Contains(query, "FROM forecasts") && strings.Contains(query, "ORDER BY project_id"):
		created := time.Date(2026, 7, 13, 10, 0, 0, 0, time.UTC)
		return connection.rows("forecasts",
			[]string{"id", "organization_id", "project_id", "deliverable_id", "p50_at", "p90_at", "review_after", "basis", "assumptions", "reason_codes", "impact", "supersedes_id", "created_by", "created_at"},
			[]driver.Value{"00000000-0000-4000-8000-000000000050", "00000000-0000-4000-8000-000000000010", "00000000-0000-4000-8000-000000000020", nil, created.Add(time.Hour), created.Add(2 * time.Hour), created.Add(30 * time.Minute), "Measured throughput", []byte(`["No scope expansion"]`), `{"new-evidence"}`, "No gate changes", nil, "00000000-0000-4000-8000-000000000040", created}), nil
	case strings.Contains(query, "SELECT project_id,deliverable_id,forecast_id FROM forecast_heads"):
		return connection.rows("forecast_heads", []string{"project_id", "deliverable_id", "forecast_id"},
			[]driver.Value{"00000000-0000-4000-8000-000000000020", nil, "00000000-0000-4000-8000-000000000050"}), nil
	case strings.Contains(query, "FROM project_targets") && strings.Contains(query, "ORDER BY project_id,id"):
		created := time.Date(2026, 7, 13, 10, 0, 0, 0, time.UTC)
		return connection.rows("targets", []string{"id", "organization_id", "project_id", "target_at", "reason", "supersedes_id", "created_by", "created_at"},
			[]driver.Value{"00000000-0000-4000-8000-000000000060", "00000000-0000-4000-8000-000000000010", "00000000-0000-4000-8000-000000000020", created.Add(time.Hour), "Contract", nil, "00000000-0000-4000-8000-000000000040", created}), nil
	case strings.Contains(query, "SELECT project_id,target_id FROM project_target_heads"):
		return connection.rows("target_heads", []string{"project_id", "target_id"},
			[]driver.Value{"00000000-0000-4000-8000-000000000020", "00000000-0000-4000-8000-000000000060"}), nil
	case strings.Contains(query, "FROM project_deadlines") && strings.Contains(query, "ORDER BY project_id,id"):
		created := time.Date(2026, 7, 13, 10, 0, 0, 0, time.UTC)
		return connection.rows("deadlines", []string{"id", "organization_id", "project_id", "deadline_at", "source", "description", "supersedes_id", "created_by", "created_at"},
			[]driver.Value{"00000000-0000-4000-8000-000000000070", "00000000-0000-4000-8000-000000000010", "00000000-0000-4000-8000-000000000020", created.Add(time.Hour), "contract", "Review", nil, "00000000-0000-4000-8000-000000000040", created}), nil
	case strings.Contains(query, "SELECT project_id,deadline_id FROM project_deadline_heads"):
		return connection.rows("deadline_heads", []string{"project_id", "deadline_id"},
			[]driver.Value{"00000000-0000-4000-8000-000000000020", "00000000-0000-4000-8000-000000000070"}), nil
	case strings.Contains(query, "SELECT kind,id,project_id FROM ("):
		return connection.rows("planning_orphans", []string{"kind", "id", "project_id"},
			[]driver.Value{"deliverable", "00000000-0000-4000-8000-000000000030", "00000000-0000-4000-8000-000000000020"}), nil
	case strings.Contains(query, "SELECT kind,project_id,id FROM ("):
		return connection.rows("head_mismatches", []string{"kind", "project_id", "id"},
			[]driver.Value{"forecast", "00000000-0000-4000-8000-000000000020", "00000000-0000-4000-8000-000000000050"}), nil
	default:
		return connection.rows("other", []string{"value"}, nil), nil
	}
}

func (connection *durableIterationConn) rows(kind string, columns []string, value []driver.Value) driver.Rows {
	values := make([][]driver.Value, 0, 1)
	var iterationErr error
	if connection.connector.failAt == kind {
		values = append(values, value)
		iterationErr = errDurableIterationInterrupted
	}
	return &durableIterationRows{connector: connection.connector, kind: kind, columns: columns, values: values, iterationErr: iterationErr}
}

type durableIterationRows struct {
	connector    *durableIterationConnector
	kind         string
	columns      []string
	values       [][]driver.Value
	index        int
	iterationErr error
}

func (rows *durableIterationRows) Columns() []string { return rows.columns }
func (*durableIterationRows) Close() error           { return nil }

func (rows *durableIterationRows) Next(destination []driver.Value) error {
	if rows.index < len(rows.values) {
		copy(destination, rows.values[rows.index])
		rows.index++
		rows.connector.mu.Lock()
		rows.connector.served[rows.kind]++
		rows.connector.mu.Unlock()
		return nil
	}
	if rows.iterationErr != nil {
		err := rows.iterationErr
		rows.iterationErr = nil
		return err
	}
	return io.EOF
}

type durableIterationTx struct {
	connector *durableIterationConnector
}

func (*durableIterationTx) Commit() error { return nil }

func (transaction *durableIterationTx) Rollback() error {
	transaction.connector.mu.Lock()
	transaction.connector.rolled = true
	transaction.connector.mu.Unlock()
	return nil
}
