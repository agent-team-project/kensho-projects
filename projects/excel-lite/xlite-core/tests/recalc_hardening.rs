use std::{
    panic::{self, AssertUnwindSafe},
    sync::mpsc,
    thread,
    time::Duration,
};

use xlite_core::{CellId, Coord, ErrorValue, RecalcEngine, Value};

fn id(addr: &str) -> CellId {
    CellId {
        sheet: 0,
        coord: Coord::from_a1(addr).unwrap(),
    }
}

fn id_at(col: u32, row: u32) -> CellId {
    CellId {
        sheet: 0,
        coord: Coord { row, col },
    }
}

fn assert_number(engine: &RecalcEngine, addr: &str, expected: f64) {
    assert_eq!(engine.cell_value(id(addr)), Value::Number(expected));
}

fn assert_circular(engine: &RecalcEngine, addr: &str) {
    assert_eq!(
        engine.cell_value(id(addr)),
        Value::Error(ErrorValue::Circular),
        "{addr}"
    );
}

fn run_cycle_case(name: &'static str, case: impl FnOnce() + Send + 'static) {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = panic::catch_unwind(AssertUnwindSafe(case));
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => panic::resume_unwind(payload),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{name} timed out after 1s")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{name} runner thread disconnected")
        }
    }
}

#[test]
fn recalc_001_propagates_direct_dependents() {
    let mut engine = RecalcEngine::default();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("B1"), "=A1+1");

    assert_number(&engine, "B1", 2.0);

    engine.reset_evaluation_count();
    engine.set_cell(id("A1"), "5");

    assert_number(&engine, "B1", 6.0);
    assert_eq!(engine.evaluation_count(), 1);
}

#[test]
fn recalc_002_recalculates_cascades_in_order() {
    let mut engine = RecalcEngine::default();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("B1"), "=A1+1");
    engine.set_cell(id("C1"), "=B1+1");
    engine.set_cell(id("D1"), "=C1+1");

    engine.reset_evaluation_count();
    engine.set_cell(id("A1"), "5");

    assert_number(&engine, "B1", 6.0);
    assert_number(&engine, "C1", 7.0);
    assert_number(&engine, "D1", 8.0);
    assert_eq!(engine.evaluation_count(), 3);
}

#[test]
fn recalc_003_recalculates_diamond_join_once() {
    let mut engine = RecalcEngine::default();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("B1"), "=A1+1");
    engine.set_cell(id("C1"), "=A1+2");
    engine.set_cell(id("D1"), "=B1+C1");

    engine.reset_evaluation_count();
    engine.set_cell(id("A1"), "5");

    assert_number(&engine, "B1", 6.0);
    assert_number(&engine, "C1", 7.0);
    assert_number(&engine, "D1", 13.0);
    assert_eq!(engine.evaluation_count(), 3);
}

#[test]
fn recalc_004_updates_large_fanout() {
    let mut engine = RecalcEngine::default();
    engine.set_cell(id("A1"), "2");
    for row in 0..1_000 {
        engine.set_cell(id_at(1, row), "=A1*2");
    }

    engine.reset_evaluation_count();
    engine.set_cell(id("A1"), "7");

    for row in 0..1_000 {
        assert_eq!(engine.cell_value(id_at(1, row)), Value::Number(14.0));
    }
    assert_eq!(engine.evaluation_count(), 1_000);
}

#[test]
fn recalc_005_edit_recomputes_only_transitive_dependents() {
    let mut engine = RecalcEngine::default();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("B1"), "=A1+1");
    for row in 0..10_000 {
        engine.set_cell(id_at(3, row), "=1+1");
    }

    engine.reset_evaluation_count();
    engine.set_cell(id("A1"), "5");

    assert_number(&engine, "B1", 6.0);
    assert_eq!(engine.cell_value(id_at(3, 9_999)), Value::Number(2.0));
    assert_eq!(engine.evaluation_count(), 1);
}

#[test]
fn recalc_006_range_dependency_tracks_inside_edits_only() {
    let mut engine = RecalcEngine::default();
    for row in 1..=100 {
        engine.set_cell(id_at(0, row - 1), &row.to_string());
    }
    engine.set_cell(id("S1"), "=SUM(A1:A100)");
    assert_number(&engine, "S1", 5_050.0);
    assert_eq!(engine.precedents(id("S1")).len(), 100);
    assert!(engine.precedents(id("S1")).contains(&id("A50")));
    assert!(engine.dependents(id("A50")).contains(&id("S1")));

    engine.reset_evaluation_count();
    engine.set_cell(id("A50"), "1000");

    assert_number(&engine, "S1", 6_000.0);
    assert_eq!(engine.evaluation_count(), 1);

    engine.reset_evaluation_count();
    engine.set_cell(id("A101"), "1000");

    assert_number(&engine, "S1", 6_000.0);
    assert_eq!(engine.evaluation_count(), 0);
}

#[test]
fn cycle_001_self_reference_is_circular() {
    run_cycle_case("CYCLE-001", || {
        let mut engine = RecalcEngine::default();
        let delta = engine.set_cell(id("A1"), "=A1");

        assert_circular(&engine, "A1");
        assert_eq!(delta.circular, vec![id("A1")]);
    });
}

#[test]
fn cycle_002_two_cell_cycle_marks_both_cells() {
    run_cycle_case("CYCLE-002", || {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "=B1");
        let delta = engine.set_cell(id("B1"), "=A1");

        assert_circular(&engine, "A1");
        assert_circular(&engine, "B1");
        assert_eq!(delta.circular, vec![id("A1"), id("B1")]);
    });
}

#[test]
fn cycle_003_three_cell_cycle_marks_all_cells() {
    run_cycle_case("CYCLE-003", || {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "=B1");
        engine.set_cell(id("B1"), "=C1");
        let delta = engine.set_cell(id("C1"), "=A1");

        assert_circular(&engine, "A1");
        assert_circular(&engine, "B1");
        assert_circular(&engine, "C1");
        assert_eq!(delta.circular, vec![id("A1"), id("B1"), id("C1")]);
    });
}

#[test]
fn cycle_004_breaking_cycle_recovers_values() {
    run_cycle_case("CYCLE-004", || {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "=B1");
        engine.set_cell(id("B1"), "=A1");

        let delta = engine.set_cell(id("B1"), "5");

        assert!(delta.circular.is_empty());
        assert_number(&engine, "A1", 5.0);
        assert_number(&engine, "B1", 5.0);
    });
}

#[test]
fn cycle_005_dependent_of_cycle_propagates_without_circular_membership() {
    run_cycle_case("CYCLE-005", || {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "=B1");
        engine.set_cell(id("B1"), "=A1");
        let delta = engine.set_cell(id("C1"), "=A1");

        assert_circular(&engine, "C1");
        assert!(delta.circular.is_empty());
    });
}

#[test]
fn cycle_range_self_reference_is_circular() {
    run_cycle_case("CYCLE-RANGE-SELF", || {
        let mut engine = RecalcEngine::default();
        let delta = engine.set_cell(id("A1"), "=SUM(A1:A2)");

        assert_circular(&engine, "A1");
        assert_eq!(delta.circular, vec![id("A1")]);
    });
}
