use xlite_core::model::{Align, NumberFormat};
use xlite_core::{Cell, CellId, Coord};
use xlite_core::{
    ErrorValue, History, RecalcDelta, RecalcEngine, StructuralAxis, StructuralEditCommand,
    StructuralEditError, StructuralEditKind, Value, Workbook,
};

fn engine() -> RecalcEngine {
    RecalcEngine::new(Workbook::new())
}

fn id(addr: &str) -> CellId {
    CellId::new(0, Coord::from_a1(addr).expect("valid test address"))
}

fn value_at(engine: &RecalcEngine, addr: &str) -> Value {
    engine.cell_value(id(addr))
}

fn raw_at(engine: &RecalcEngine, addr: &str) -> Option<String> {
    engine
        .workbook()
        .sheet(0)
        .and_then(|sheet| sheet.get_cell(id(addr).coord))
        .map(|cell| cell.raw.clone())
}

fn cell_at(engine: &RecalcEngine, addr: &str) -> Option<Cell> {
    engine
        .workbook()
        .sheet(0)
        .and_then(|sheet| sheet.get_cell(id(addr).coord))
        .cloned()
}

fn apply(engine: &mut RecalcEngine, command: StructuralEditCommand) -> RecalcDelta {
    command.validate(engine).expect("valid structural edit");
    let mut history = History::new();
    history.exec(Box::new(command), engine)
}

fn changed_value(delta: &RecalcDelta, addr: &str) -> Option<Value> {
    delta
        .changed
        .iter()
        .find_map(|(cell_id, value)| (*cell_id == id(addr)).then(|| value.clone()))
}

#[test]
fn insert_column_before_a_moves_formula_rewrites_reference_and_preserves_value() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "7");
    engine.set_cell(id("B1"), "=A1");

    let delta = apply(
        &mut engine,
        StructuralEditCommand::insert_cols(0, 0, 1).unwrap(),
    );

    assert_eq!(raw_at(&engine, "A1"), None);
    assert_eq!(raw_at(&engine, "B1").as_deref(), Some("7"));
    assert_eq!(raw_at(&engine, "C1").as_deref(), Some("=B1"));
    assert_eq!(value_at(&engine, "C1"), Value::Number(7.0));
    assert_eq!(changed_value(&delta, "A1"), Some(Value::Blank));
    assert_eq!(changed_value(&delta, "B1"), Some(Value::Number(7.0)));
    assert_eq!(changed_value(&delta, "C1"), Some(Value::Number(7.0)));
}

#[test]
fn insert_row_preserves_formula_looking_literal_text_cell() {
    let mut engine = engine();
    let mut literal = Cell::new("=SUM(A1:A1)", None, Value::Text("=SUM(A1:A1)".to_string()));
    literal.format.number = NumberFormat::Text;
    literal.format.align = Align::Right;
    literal.format.bold = true;
    let expected = literal.clone();
    engine
        .workbook_mut()
        .sheet_mut(0)
        .unwrap()
        .set_cell(id("A1").coord, literal);

    apply(
        &mut engine,
        StructuralEditCommand::insert_rows(0, 0, 1).unwrap(),
    );

    assert_eq!(cell_at(&engine, "A1"), None);
    assert_eq!(cell_at(&engine, "A2"), Some(expected));
}

#[test]
fn delete_column_preserves_boolean_looking_literal_text_cell() {
    let mut engine = engine();
    let literal = Cell::new("TRUE", None, Value::Text("TRUE".to_string()));
    let expected = literal.clone();
    engine.set_cell(id("A1"), "discarded");
    engine
        .workbook_mut()
        .sheet_mut(0)
        .unwrap()
        .set_cell(id("B1").coord, literal);

    apply(
        &mut engine,
        StructuralEditCommand::delete_cols(0, 0, 1).unwrap(),
    );

    assert_eq!(cell_at(&engine, "A1"), Some(expected));
    assert_eq!(raw_at(&engine, "B1"), None);
}

#[test]
fn delete_column_a_moves_formula_to_a_and_rewrites_deleted_reference_to_ref() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "7");
    engine.set_cell(id("B1"), "=A1");

    apply(
        &mut engine,
        StructuralEditCommand::delete_cols(0, 0, 1).unwrap(),
    );

    assert_eq!(raw_at(&engine, "A1").as_deref(), Some("=#REF!"));
    assert_eq!(value_at(&engine, "A1"), Value::Error(ErrorValue::Ref));
    assert_eq!(raw_at(&engine, "B1"), None);
}

#[test]
fn insert_row_inside_range_grows_range_and_moves_formula_cell() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("A2"), "2");
    engine.set_cell(id("A3"), "=SUM(A1:A2)");

    apply(
        &mut engine,
        StructuralEditCommand::insert_rows(0, 1, 1).unwrap(),
    );

    assert_eq!(raw_at(&engine, "A3").as_deref(), Some("2"));
    assert_eq!(raw_at(&engine, "A4").as_deref(), Some("=SUM(A1:A3)"));
    assert_eq!(value_at(&engine, "A4"), Value::Number(3.0));
}

#[test]
fn insert_row_exactly_after_range_does_not_grow_range() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("A2"), "2");
    engine.set_cell(id("A3"), "=SUM(A1:A2)");

    apply(
        &mut engine,
        StructuralEditCommand::insert_rows(0, 2, 1).unwrap(),
    );

    assert_eq!(raw_at(&engine, "A3"), None);
    assert_eq!(raw_at(&engine, "A4").as_deref(), Some("=SUM(A1:A2)"));
    assert_eq!(value_at(&engine, "A4"), Value::Number(3.0));
}

#[test]
fn delete_row_inside_range_shrinks_range_to_surviving_cells() {
    let mut engine = engine();
    for row in 1..=5 {
        engine.set_cell(id(&format!("A{row}")), &row.to_string());
    }
    engine.set_cell(id("B1"), "=SUM(A1:A5)");

    apply(
        &mut engine,
        StructuralEditCommand::delete_rows(0, 2, 1).unwrap(),
    );

    assert_eq!(raw_at(&engine, "B1").as_deref(), Some("=SUM(A1:A4)"));
    assert_eq!(value_at(&engine, "B1"), Value::Number(12.0));
}

#[test]
fn delete_rows_covering_entire_range_rewrites_range_to_ref_error() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("A2"), "2");
    engine.set_cell(id("A3"), "=SUM(A1:A2)");

    apply(
        &mut engine,
        StructuralEditCommand::delete_rows(0, 0, 2).unwrap(),
    );

    assert_eq!(raw_at(&engine, "A1").as_deref(), Some("=SUM(#REF!)"));
    assert_eq!(value_at(&engine, "A1"), Value::Error(ErrorValue::Ref));
    assert_eq!(raw_at(&engine, "A2"), None);
}

#[test]
fn absolute_markers_unsupported_sheets_and_string_references_keep_expected_text() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "3");
    engine.set_cell(id("C1"), r#"=$A$1+A$1+$A1+Sheet2!A1+INDIRECT("A1")+A1"#);

    apply(
        &mut engine,
        StructuralEditCommand::insert_cols(0, 0, 1).unwrap(),
    );

    assert_eq!(
        raw_at(&engine, "D1").as_deref(),
        Some(r#"=$B$1+B$1+$B1+Sheet2!A1+INDIRECT("A1")+B1"#)
    );
}

#[test]
fn row_heights_and_column_widths_shift_and_delete_with_structural_edits() {
    let mut engine = engine();
    {
        let sheet = engine.workbook_mut().sheet_mut(0).unwrap();
        sheet.row_heights.insert(0, 30.0);
        sheet.row_heights.insert(2, 44.0);
        sheet.col_widths.insert(0, 100.0);
        sheet.col_widths.insert(1, 120.0);
        sheet.col_widths.insert(3, 160.0);
    }

    apply(
        &mut engine,
        StructuralEditCommand::insert_rows(0, 1, 1).unwrap(),
    );
    let sheet = engine.workbook().sheet(0).unwrap();
    assert_eq!(sheet.row_heights.get(&0), Some(&30.0));
    assert_eq!(sheet.row_heights.get(&1), None);
    assert_eq!(sheet.row_heights.get(&2), None);
    assert_eq!(sheet.row_heights.get(&3), Some(&44.0));

    apply(
        &mut engine,
        StructuralEditCommand::delete_cols(0, 1, 2).unwrap(),
    );
    let sheet = engine.workbook().sheet(0).unwrap();
    assert_eq!(sheet.col_widths.get(&0), Some(&100.0));
    assert_eq!(sheet.col_widths.get(&1), Some(&160.0));
    assert_eq!(sheet.col_widths.get(&2), None);
    assert_eq!(sheet.col_widths.get(&3), None);
}

#[test]
fn dependency_graph_rebuilds_to_new_precedent_after_insert() {
    let mut engine = engine();
    engine.set_cell(id("A1"), "5");
    engine.set_cell(id("B1"), "=A1");

    apply(
        &mut engine,
        StructuralEditCommand::insert_cols(0, 0, 1).unwrap(),
    );

    assert_eq!(engine.dependents(id("A1")), Vec::<CellId>::new());
    assert_eq!(engine.dependents(id("B1")), vec![id("C1")]);

    let old_delta = engine.set_cell(id("A1"), "100");
    assert_eq!(value_at(&engine, "C1"), Value::Number(5.0));
    assert_eq!(changed_value(&old_delta, "C1"), None);

    let new_delta = engine.set_cell(id("B1"), "9");
    assert_eq!(value_at(&engine, "C1"), Value::Number(9.0));
    assert_eq!(changed_value(&new_delta, "C1"), Some(Value::Number(9.0)));
}

#[test]
fn undo_restores_exact_sheet_snapshot_and_redo_restores_structural_snapshot() {
    let mut history = History::new();
    let mut engine = engine();
    engine.set_cell(id("A1"), "4");
    engine.set_cell(id("B1"), "=A1");
    {
        let sheet = engine.workbook_mut().sheet_mut(0).unwrap();
        sheet.row_heights.insert(0, 31.0);
        sheet.col_widths.insert(0, 101.0);
        sheet.col_widths.insert(1, 121.0);
    }
    let before = engine.workbook().sheet(0).unwrap().clone();

    history.exec(
        Box::new(StructuralEditCommand::insert_cols(0, 0, 1).unwrap()),
        &mut engine,
    );
    let after = engine.workbook().sheet(0).unwrap().clone();
    assert_eq!(raw_at(&engine, "C1").as_deref(), Some("=B1"));
    assert_eq!(engine.dependents(id("B1")), vec![id("C1")]);

    history.undo(&mut engine).expect("undo structural edit");
    assert_eq!(engine.workbook().sheet(0).unwrap(), &before);
    assert_eq!(engine.dependents(id("A1")), vec![id("B1")]);
    assert_eq!(engine.dependents(id("B1")), Vec::<CellId>::new());

    history.redo(&mut engine).expect("redo structural edit");
    assert_eq!(engine.workbook().sheet(0).unwrap(), &after);
    assert_eq!(engine.dependents(id("A1")), Vec::<CellId>::new());
    assert_eq!(engine.dependents(id("B1")), vec![id("C1")]);
}

#[test]
fn checked_constructors_reject_invalid_input() {
    assert_eq!(
        StructuralEditCommand::insert_rows(0, 0, 0).unwrap_err(),
        StructuralEditError::ZeroCount
    );
    assert!(matches!(
        StructuralEditCommand::delete_cols(0, 16_383, 2).unwrap_err(),
        StructuralEditError::OutOfBounds {
            axis: StructuralAxis::Col,
            kind: StructuralEditKind::Delete,
            at: 16_383,
            count: 2
        }
    ));

    let engine = engine();
    assert_eq!(
        StructuralEditCommand::for_engine(
            &engine,
            1,
            StructuralAxis::Row,
            StructuralEditKind::Insert,
            0,
            1,
        )
        .unwrap_err(),
        StructuralEditError::SheetNotFound(1)
    );
}
