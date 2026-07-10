use xlite_core::model::{
    display, CellFormat, Coord, DateFormat, NumberFormat, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT,
    MAX_COLS, MAX_ROWS,
};
use xlite_core::{
    Cell, CellId, Command, FormatCommandError, History, RecalcDelta, RecalcEngine,
    ResizeColumnCommand, ResizeCommandError, ResizeRowCommand, SetFormatCommand, Value, Workbook,
};

fn engine() -> RecalcEngine {
    RecalcEngine::new(Workbook::new())
}

fn id(addr: &str) -> CellId {
    CellId::new(0, Coord::from_a1(addr).expect("valid test address"))
}

fn coord(addr: &str) -> Coord {
    id(addr).coord
}

fn number_format(number: NumberFormat) -> CellFormat {
    CellFormat {
        number,
        ..CellFormat::default()
    }
}

fn cell_at(engine: &RecalcEngine, addr: &str) -> Option<Cell> {
    engine
        .workbook()
        .sheet(0)
        .and_then(|sheet| sheet.get_cell(coord(addr)))
        .cloned()
}

fn display_at(engine: &RecalcEngine, addr: &str) -> Option<String> {
    cell_at(engine, addr).map(|cell| display(&cell.cached, &cell.format))
}

fn changed_value(delta: &RecalcDelta, addr: &str) -> Option<Value> {
    delta
        .changed
        .iter()
        .find_map(|(cell_id, value)| (*cell_id == id(addr)).then(|| value.clone()))
}

fn apply_format(
    history: &mut History,
    engine: &mut RecalcEngine,
    addr: &str,
    format: CellFormat,
) -> RecalcDelta {
    let command = SetFormatCommand::for_engine(engine, 0, coord(addr), coord(addr), format)
        .expect("valid format command");
    history.exec(Box::new(command), engine)
}

#[test]
fn formatting_numeric_cells_changes_display_and_preserves_raw_and_cached_values() {
    let cases = [
        (
            "A1",
            "12.345",
            number_format(NumberFormat::Fixed(2)),
            "12.35",
            Value::Number(12.345),
        ),
        (
            "B1",
            "0.125",
            number_format(NumberFormat::Percent(1)),
            "12.5%",
            Value::Number(0.125),
        ),
        (
            "C1",
            "7",
            number_format(NumberFormat::Currency(2)),
            "$7.00",
            Value::Number(7.0),
        ),
        (
            "D1",
            "43831",
            number_format(NumberFormat::Date(DateFormat::Iso)),
            "2020-01-01",
            Value::Number(43_831.0),
        ),
        (
            "E1",
            "42",
            number_format(NumberFormat::Text),
            "42.0",
            Value::Number(42.0),
        ),
    ];

    let mut history = History::new();
    let mut engine = engine();
    for (addr, raw, _, _, _) in &cases {
        engine.set_cell(id(addr), raw);
    }

    for (addr, raw, format, expected_display, expected_value) in cases {
        let before_display = display_at(&engine, addr).expect("cell display before format");

        let delta = apply_format(&mut history, &mut engine, addr, format.clone());
        let cell = cell_at(&engine, addr).expect("formatted cell exists");

        assert_ne!(before_display, expected_display);
        assert_eq!(display(&cell.cached, &cell.format), expected_display);
        assert_eq!(cell.raw, raw);
        assert_eq!(cell.cached, expected_value);
        assert_eq!(cell.format, format);
        assert_eq!(changed_value(&delta, addr), Some(expected_value));
    }
}

#[test]
fn formatting_formula_cell_preserves_formula_and_cached_result_while_display_changes() {
    let mut history = History::new();
    let mut engine = engine();
    engine.set_cell(id("A1"), "0.125");
    engine.set_cell(id("B1"), "=A1");

    let delta = apply_format(
        &mut history,
        &mut engine,
        "B1",
        number_format(NumberFormat::Percent(1)),
    );
    let cell = cell_at(&engine, "B1").expect("formula cell exists");

    assert_eq!(cell.raw, "=A1");
    assert!(cell.ast.is_some());
    assert_eq!(cell.cached, Value::Number(0.125));
    assert_eq!(display(&cell.cached, &cell.format), "12.5%");
    assert_eq!(changed_value(&delta, "B1"), Some(Value::Number(0.125)));
}

#[test]
fn formatting_blank_cell_persists_format_and_undo_redo_restores_absence() {
    let mut history = History::new();
    let mut engine = engine();
    let format = number_format(NumberFormat::Currency(2));

    let delta = apply_format(&mut history, &mut engine, "C3", format.clone());
    let cell = cell_at(&engine, "C3").expect("blank formatted cell exists");

    assert_eq!(cell.raw, "");
    assert_eq!(cell.cached, Value::Blank);
    assert_eq!(cell.format, format);
    assert_eq!(changed_value(&delta, "C3"), Some(Value::Blank));

    history.undo(&mut engine).expect("undo blank format");
    assert_eq!(cell_at(&engine, "C3"), None);

    history.redo(&mut engine).expect("redo blank format");
    assert_eq!(
        cell_at(&engine, "C3")
            .expect("blank formatted cell exists after redo")
            .format,
        format
    );
}

#[test]
fn undo_and_redo_restore_exact_formats_for_multi_cell_range() {
    let mut history = History::new();
    let mut engine = engine();
    engine.set_cell(id("A1"), "1");
    engine.set_cell(id("B1"), "2");
    engine.set_cell(id("B2"), "3");

    let original_b2_format = number_format(NumberFormat::Currency(0));
    engine
        .workbook_mut()
        .sheet_mut(0)
        .unwrap()
        .get_cell_mut(coord("B2"))
        .unwrap()
        .format = original_b2_format.clone();
    let before = engine.workbook().sheet(0).unwrap().clone();

    let range_format = number_format(NumberFormat::Fixed(1));
    let command =
        SetFormatCommand::for_engine(&engine, 0, coord("A1"), coord("B2"), range_format.clone())
            .expect("valid range format");
    history.exec(Box::new(command), &mut engine);
    let after = engine.workbook().sheet(0).unwrap().clone();

    assert_eq!(cell_at(&engine, "A1").unwrap().format, range_format);
    assert_eq!(cell_at(&engine, "B1").unwrap().format, range_format);
    assert_eq!(cell_at(&engine, "A2").unwrap().format, range_format);
    assert_eq!(cell_at(&engine, "B2").unwrap().format, range_format);

    history.undo(&mut engine).expect("undo range format");
    assert_eq!(engine.workbook().sheet(0).unwrap(), &before);
    assert_eq!(cell_at(&engine, "A2"), None);
    assert_eq!(cell_at(&engine, "B2").unwrap().format, original_b2_format);

    history.redo(&mut engine).expect("redo range format");
    assert_eq!(engine.workbook().sheet(0).unwrap(), &after);
}

#[test]
fn new_format_edit_after_undo_clears_redo() {
    let mut history = History::new();
    let mut engine = engine();
    engine.set_cell(id("A1"), "12");

    apply_format(
        &mut history,
        &mut engine,
        "A1",
        number_format(NumberFormat::Fixed(2)),
    );
    history.undo(&mut engine).expect("undo first format");
    assert!(history.can_redo());

    apply_format(
        &mut history,
        &mut engine,
        "A1",
        number_format(NumberFormat::Currency(0)),
    );

    assert!(!history.can_redo());
    assert_eq!(display_at(&engine, "A1").as_deref(), Some("$12"));
}

#[test]
fn resize_column_and_row_set_sparse_dimensions_and_undo_redo_exactly() {
    let mut history = History::new();
    let mut engine = engine();
    let before = engine.workbook().sheet(0).unwrap().clone();

    history.exec(
        Box::new(ResizeColumnCommand::for_engine(&engine, 0, 1, 144.0).unwrap()),
        &mut engine,
    );
    history.exec(
        Box::new(ResizeRowCommand::for_engine(&engine, 0, 2, 40.0).unwrap()),
        &mut engine,
    );
    let after = engine.workbook().sheet(0).unwrap().clone();

    assert_eq!(after.col_widths.get(&1), Some(&144.0));
    assert_eq!(after.row_heights.get(&2), Some(&40.0));

    history.undo(&mut engine).expect("undo row resize");
    assert_eq!(
        engine.workbook().sheet(0).unwrap().row_heights.get(&2),
        None
    );
    assert_eq!(
        engine.workbook().sheet(0).unwrap().col_widths.get(&1),
        Some(&144.0)
    );

    history.undo(&mut engine).expect("undo column resize");
    assert_eq!(engine.workbook().sheet(0).unwrap(), &before);

    history.redo(&mut engine).expect("redo column resize");
    history.redo(&mut engine).expect("redo row resize");
    assert_eq!(engine.workbook().sheet(0).unwrap(), &after);
}

#[test]
fn resizing_to_default_dimensions_removes_sparse_overrides_and_undo_restores() {
    let mut history = History::new();
    let mut engine = engine();
    {
        let sheet = engine.workbook_mut().sheet_mut(0).unwrap();
        sheet.col_widths.insert(0, 150.0);
        sheet.row_heights.insert(0, 36.0);
    }

    history.exec(
        Box::new(ResizeColumnCommand::for_engine(&engine, 0, 0, DEFAULT_COL_WIDTH).unwrap()),
        &mut engine,
    );
    history.exec(
        Box::new(ResizeRowCommand::for_engine(&engine, 0, 0, DEFAULT_ROW_HEIGHT).unwrap()),
        &mut engine,
    );

    assert_eq!(engine.workbook().sheet(0).unwrap().col_widths.get(&0), None);
    assert_eq!(
        engine.workbook().sheet(0).unwrap().row_heights.get(&0),
        None
    );

    history.undo(&mut engine).expect("undo row default resize");
    history
        .undo(&mut engine)
        .expect("undo column default resize");
    assert_eq!(
        engine.workbook().sheet(0).unwrap().col_widths.get(&0),
        Some(&150.0)
    );
    assert_eq!(
        engine.workbook().sheet(0).unwrap().row_heights.get(&0),
        Some(&36.0)
    );

    history
        .redo(&mut engine)
        .expect("redo column default resize");
    history.redo(&mut engine).expect("redo row default resize");
    assert_eq!(engine.workbook().sheet(0).unwrap().col_widths.get(&0), None);
    assert_eq!(
        engine.workbook().sheet(0).unwrap().row_heights.get(&0),
        None
    );
}

#[test]
fn invalid_format_and_resize_inputs_are_rejected_before_mutation() {
    let mut engine = engine();
    let before = engine.workbook().clone();

    assert_eq!(
        SetFormatCommand::for_engine(
            &engine,
            1,
            coord("A1"),
            coord("A1"),
            number_format(NumberFormat::Fixed(1))
        )
        .unwrap_err(),
        FormatCommandError::SheetNotFound(1)
    );
    assert!(matches!(
        SetFormatCommand::new(
            0,
            Coord {
                row: MAX_ROWS,
                col: 0
            },
            coord("A1"),
            number_format(NumberFormat::Fixed(1))
        )
        .unwrap_err(),
        FormatCommandError::RangeOutOfBounds { .. }
    ));
    assert_eq!(
        ResizeColumnCommand::new(0, MAX_COLS, 100.0).unwrap_err(),
        ResizeCommandError::ColumnOutOfBounds(MAX_COLS)
    );
    assert_eq!(
        ResizeRowCommand::new(0, MAX_ROWS, 25.0).unwrap_err(),
        ResizeCommandError::RowOutOfBounds(MAX_ROWS)
    );
    assert!(matches!(
        ResizeColumnCommand::new(0, 0, 0.0).unwrap_err(),
        ResizeCommandError::InvalidSize(0.0)
    ));
    assert!(matches!(
        ResizeRowCommand::new(0, 0, f32::INFINITY).unwrap_err(),
        ResizeCommandError::InvalidSize(size) if size.is_infinite()
    ));
    assert!(matches!(
        ResizeRowCommand::new(0, 0, f32::NAN).unwrap_err(),
        ResizeCommandError::InvalidSize(size) if size.is_nan()
    ));

    let mut format_missing_sheet = SetFormatCommand::new(
        1,
        coord("A1"),
        coord("A1"),
        number_format(NumberFormat::Fixed(1)),
    )
    .unwrap();
    assert_eq!(
        format_missing_sheet.apply(&mut engine),
        RecalcDelta::default()
    );

    let mut resize_missing_sheet = ResizeColumnCommand::new(1, 0, 100.0).unwrap();
    assert_eq!(
        resize_missing_sheet.apply(&mut engine),
        RecalcDelta::default()
    );

    assert_eq!(engine.workbook(), &before);
}
