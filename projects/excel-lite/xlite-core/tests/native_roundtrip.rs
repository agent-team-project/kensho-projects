use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use proptest::{collection::btree_map, prelude::*, test_runner::TestCaseError};
use serde_json::{json, Value as JsonValue};
use xlite_core::{
    load_native,
    model::{Align, CellFormat, DateFormat, NumberFormat},
    save_native, Cell, CellId, Coord, DateSystem, ErrorValue, RecalcEngine, Sheet, Value, Workbook,
};

const CASES: u32 = 1_000;
const MAX_SHEETS: usize = 3;
const MAX_ROWS: u32 = 6;
const MAX_COLS: u32 = 5;

static NEXT_PATH_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct WorkbookCase {
    sheet_names: Vec<String>,
    anchors: AnchorSpec,
    cells: BTreeMap<CellKey, CellSpec>,
    col_widths: BTreeMap<DimensionKey, f32>,
    row_heights: BTreeMap<DimensionKey, f32>,
}

#[derive(Clone, Debug)]
struct AnchorSpec {
    a1: f64,
    b1: f64,
    a2: String,
    b2: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CellKey {
    sheet: u16,
    row: u32,
    col: u32,
}

impl CellKey {
    fn id(self) -> CellId {
        CellId::new(
            self.sheet,
            Coord {
                row: self.row,
                col: self.col,
            },
        )
    }

    fn address(self) -> String {
        Coord {
            row: self.row,
            col: self.col,
        }
        .to_a1()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct DimensionKey {
    sheet: u16,
    index: u32,
}

#[derive(Clone, Debug)]
struct CellSpec {
    raw: CellRaw,
    format: FormatSpec,
}

#[derive(Clone, Debug)]
enum CellRaw {
    Number(f64),
    Text(String),
    Boolean(bool),
    Blank,
    Formula(FormulaKind),
}

impl CellRaw {
    fn raw_for(&self, key: CellKey) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::Text(value) => value.clone(),
            Self::Boolean(true) => "TRUE".to_string(),
            Self::Boolean(false) => "FALSE".to_string(),
            Self::Blank => String::new(),
            Self::Formula(kind) => kind.raw_for(key),
        }
    }
}

#[derive(Clone, Debug)]
enum FormulaKind {
    Arithmetic,
    DirectReference,
    SumRange,
    IfReference,
    DynamicReference,
    Div0Error,
    NameError,
    ValueError,
    NumError,
    NaError,
    SelfReference,
}

impl FormulaKind {
    fn raw_for(&self, key: CellKey) -> String {
        match self {
            Self::Arithmetic => "=(1+2)*3".to_string(),
            Self::DirectReference if key.sheet == 0 => "=A1+B1".to_string(),
            Self::DirectReference => "=1+2".to_string(),
            Self::SumRange if key.sheet == 0 => "=SUM(A1:B1)".to_string(),
            Self::SumRange => "=SUM(1,2,3)".to_string(),
            Self::IfReference if key.sheet == 0 => "=IF(A1>0,B1,A1)".to_string(),
            Self::IfReference => "=IF(TRUE,1,0)".to_string(),
            Self::DynamicReference if key.sheet == 0 => r#"=INDIRECT("A2")"#.to_string(),
            Self::DynamicReference => "=IF(TRUE,2,3)".to_string(),
            Self::Div0Error => "=1/0".to_string(),
            Self::NameError => "=NOT_A_REAL_FUNCTION()".to_string(),
            Self::ValueError => r#"=IF("maybe",1,0)"#.to_string(),
            Self::NumError => "=SQRT(-1)".to_string(),
            Self::NaError => "=NA()".to_string(),
            Self::SelfReference if key.sheet == 0 => format!("={}", key.address()),
            Self::SelfReference => "=1/0".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
struct FormatSpec {
    number: NumberFormatSpec,
    align: Align,
    bold: bool,
}

impl FormatSpec {
    fn to_model(&self) -> CellFormat {
        CellFormat {
            number: self.number.to_model(),
            align: self.align,
            bold: self.bold,
        }
    }
}

#[derive(Clone, Debug)]
enum NumberFormatSpec {
    General,
    Fixed(u8),
    Percent(u8),
    Currency(u8),
    DateIso,
    Text,
}

impl NumberFormatSpec {
    fn to_model(&self) -> NumberFormat {
        match self {
            Self::General => NumberFormat::General,
            Self::Fixed(dp) => NumberFormat::Fixed(*dp),
            Self::Percent(dp) => NumberFormat::Percent(*dp),
            Self::Currency(dp) => NumberFormat::Currency(*dp),
            Self::DateIso => NumberFormat::Date(DateFormat::Iso),
            Self::Text => NumberFormat::Text,
        }
    }
}

impl WorkbookCase {
    fn into_workbook(self) -> Workbook {
        let workbook = Workbook {
            sheets: self.sheet_names.into_iter().map(Sheet::new).collect(),
            date_system: DateSystem::Excel1900,
        };
        let mut engine = RecalcEngine::new(workbook);

        engine.set_cell(id("A1"), &self.anchors.a1.to_string());
        engine.set_cell(id("B1"), &self.anchors.b1.to_string());
        engine.set_cell(id("A2"), &self.anchors.a2);
        engine.set_cell(id("B2"), if self.anchors.b2 { "TRUE" } else { "FALSE" });

        for (key, spec) in self.cells {
            let raw = spec.raw.raw_for(key);
            engine.set_cell(key.id(), &raw);
            if !raw.is_empty() {
                if let Some(cell) = engine
                    .workbook_mut()
                    .sheet_mut(key.sheet)
                    .and_then(|sheet| {
                        sheet.get_cell_mut(Coord {
                            row: key.row,
                            col: key.col,
                        })
                    })
                {
                    cell.format = spec.format.to_model();
                }
            }
        }

        for (key, width) in self.col_widths {
            if let Some(sheet) = engine.workbook_mut().sheet_mut(key.sheet) {
                sheet.col_widths.insert(key.index, width);
            }
        }

        for (key, height) in self.row_heights {
            if let Some(sheet) = engine.workbook_mut().sheet_mut(key.sheet) {
                sheet.row_heights.insert(key.index, height);
            }
        }

        engine.recalc_all();
        engine.workbook().clone()
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: CASES,
        max_shrink_iters: 128,
        .. ProptestConfig::default()
    })]

    #[test]
    fn native_roundtrip_preserves_generated_workbooks(case in workbook_case_strategy()) {
        let expected = recalc_canonical(case.into_workbook());
        let temp_path = TempNativePath::new("property");

        save_native(&expected, temp_path.path())
            .map_err(|error| TestCaseError::fail(format!("save native: {error}")))?;
        assert_saved_cells_are_non_empty(temp_path.path())?;

        let loaded = load_native(temp_path.path())
            .map_err(|error| TestCaseError::fail(format!("load native: {error}")))?;
        let loaded = recalc_canonical(loaded);

        assert_workbooks_equal(&expected, &loaded)?;
    }
}

#[test]
fn load_native_recomputes_stale_formula_cache_through_public_api() {
    let mut engine = RecalcEngine::default();
    engine.set_cell(id("A1"), "2");
    engine.set_cell(id("B1"), "=A1+1");

    let temp_path = TempNativePath::new("stale-formula");
    save_native(engine.workbook(), temp_path.path()).unwrap();

    let text = fs::read_to_string(temp_path.path()).unwrap();
    let mut json: JsonValue = serde_json::from_str(&text).unwrap();
    json["sheets"][0]["cells"]["B1"]["cached"] = json!({
        "type": "number",
        "value": 999.0
    });
    fs::write(temp_path.path(), serde_json::to_vec_pretty(&json).unwrap()).unwrap();

    let loaded = load_native(temp_path.path()).unwrap();

    assert_eq!(
        loaded
            .sheet(0)
            .unwrap()
            .get_cell(Coord::from_a1("B1").unwrap())
            .unwrap()
            .cached,
        Value::Number(3.0)
    );
}

#[test]
fn native_roundtrip_preserves_reachable_canonical_error_codes() {
    let formulas = [
        ("A1", "=1/0", ErrorValue::Div0),
        ("A2", "=NOT_A_REAL_FUNCTION()", ErrorValue::Name),
        ("A3", r#"=IF("maybe",1,0)"#, ErrorValue::Value),
        ("A4", "=SQRT(-1)", ErrorValue::Num),
        ("A5", "=NA()", ErrorValue::Na),
        ("A6", "=A6", ErrorValue::Circular),
    ];
    let mut engine = RecalcEngine::default();
    for (addr, raw, _) in formulas {
        engine.set_cell(id(addr), raw);
    }

    let temp_path = TempNativePath::new("errors");
    save_native(engine.workbook(), temp_path.path()).unwrap();
    let loaded = load_native(temp_path.path()).unwrap();

    for (addr, _, expected) in formulas {
        assert_eq!(
            loaded
                .sheet(0)
                .unwrap()
                .get_cell(Coord::from_a1(addr).unwrap())
                .unwrap()
                .cached,
            Value::Error(expected),
            "{addr}"
        );
    }
}

fn workbook_case_strategy() -> impl Strategy<Value = WorkbookCase> {
    sheet_names_strategy().prop_flat_map(|sheet_names| {
        let sheet_count = sheet_names.len();
        (
            Just(sheet_names),
            anchor_strategy(),
            btree_map(cell_key_strategy(sheet_count), cell_spec_strategy(), 0..=18),
            btree_map(
                dimension_key_strategy(sheet_count, MAX_COLS),
                width_strategy(),
                0..=8,
            ),
            btree_map(
                dimension_key_strategy(sheet_count, MAX_ROWS),
                height_strategy(),
                0..=8,
            ),
        )
            .prop_map(|(sheet_names, anchors, cells, col_widths, row_heights)| {
                WorkbookCase {
                    sheet_names,
                    anchors,
                    cells,
                    col_widths,
                    row_heights,
                }
            })
    })
}

fn sheet_names_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::btree_set(1u16..=999, 1..=MAX_SHEETS)
        .prop_map(|names| names.into_iter().map(|id| format!("Sheet{id}")).collect())
}

fn anchor_strategy() -> impl Strategy<Value = AnchorSpec> {
    (
        small_number_strategy(),
        small_number_strategy(),
        text_strategy(),
        any::<bool>(),
    )
        .prop_map(|(a1, b1, a2, b2)| AnchorSpec { a1, b1, a2, b2 })
}

fn cell_key_strategy(sheet_count: usize) -> impl Strategy<Value = CellKey> {
    let mut keys = Vec::new();
    for sheet in 0..sheet_count as u16 {
        for row in 0..MAX_ROWS {
            for col in 0..MAX_COLS {
                if sheet == 0 && matches!((row, col), (0, 0) | (0, 1) | (1, 0) | (1, 1)) {
                    continue;
                }
                keys.push(CellKey { sheet, row, col });
            }
        }
    }
    prop::sample::select(keys)
}

fn dimension_key_strategy(sheet_count: usize, count: u32) -> impl Strategy<Value = DimensionKey> {
    let mut keys = Vec::new();
    for sheet in 0..sheet_count as u16 {
        for index in 0..count {
            keys.push(DimensionKey { sheet, index });
        }
    }
    prop::sample::select(keys)
}

fn cell_spec_strategy() -> impl Strategy<Value = CellSpec> {
    (cell_raw_strategy(), format_strategy()).prop_map(|(raw, format)| CellSpec { raw, format })
}

fn cell_raw_strategy() -> impl Strategy<Value = CellRaw> {
    prop_oneof![
        4 => small_number_strategy().prop_map(CellRaw::Number),
        3 => text_strategy().prop_map(CellRaw::Text),
        2 => any::<bool>().prop_map(CellRaw::Boolean),
        1 => Just(CellRaw::Blank),
        5 => formula_strategy().prop_map(CellRaw::Formula),
    ]
}

fn formula_strategy() -> impl Strategy<Value = FormulaKind> {
    prop_oneof![
        Just(FormulaKind::Arithmetic),
        Just(FormulaKind::DirectReference),
        Just(FormulaKind::SumRange),
        Just(FormulaKind::IfReference),
        Just(FormulaKind::DynamicReference),
        Just(FormulaKind::Div0Error),
        Just(FormulaKind::NameError),
        Just(FormulaKind::ValueError),
        Just(FormulaKind::NumError),
        Just(FormulaKind::NaError),
        Just(FormulaKind::SelfReference),
    ]
}

fn format_strategy() -> impl Strategy<Value = FormatSpec> {
    (number_format_strategy(), align_strategy(), any::<bool>()).prop_map(|(number, align, bold)| {
        FormatSpec {
            number,
            align,
            bold,
        }
    })
}

fn number_format_strategy() -> impl Strategy<Value = NumberFormatSpec> {
    prop_oneof![
        Just(NumberFormatSpec::General),
        (0u8..=4).prop_map(NumberFormatSpec::Fixed),
        (0u8..=4).prop_map(NumberFormatSpec::Percent),
        (0u8..=4).prop_map(NumberFormatSpec::Currency),
        Just(NumberFormatSpec::DateIso),
        Just(NumberFormatSpec::Text),
    ]
}

fn align_strategy() -> impl Strategy<Value = Align> {
    prop_oneof![
        Just(Align::Default),
        Just(Align::Left),
        Just(Align::Center),
        Just(Align::Right),
    ]
}

fn small_number_strategy() -> impl Strategy<Value = f64> {
    (-2000i16..=2000).prop_map(|value| f64::from(value) / 4.0)
}

fn text_strategy() -> impl Strategy<Value = String> {
    "[a-z]{1,8}".prop_map(|text| format!("text-{text}"))
}

fn width_strategy() -> impl Strategy<Value = f32> {
    (40u16..=240).prop_map(|width| f32::from(width) + 0.5)
}

fn height_strategy() -> impl Strategy<Value = f32> {
    (12u16..=80).prop_map(|height| f32::from(height) + 0.25)
}

fn recalc_canonical(workbook: Workbook) -> Workbook {
    let mut engine = RecalcEngine::new(workbook);
    engine.recalc_all();
    engine.workbook().clone()
}

fn assert_saved_cells_are_non_empty(path: &Path) -> Result<(), TestCaseError> {
    let text = fs::read_to_string(path)
        .map_err(|error| TestCaseError::fail(format!("read saved native file: {error}")))?;
    let json: JsonValue = serde_json::from_str(&text)
        .map_err(|error| TestCaseError::fail(format!("parse saved native JSON: {error}")))?;
    for (sheet_index, sheet) in json["sheets"].as_array().unwrap().iter().enumerate() {
        for (address, cell) in sheet["cells"].as_object().unwrap() {
            prop_assert_ne!(
                cell["raw"].as_str(),
                Some(""),
                "sheet {} cell {} serialized an empty raw string",
                sheet_index,
                address
            );
        }
    }
    Ok(())
}

fn assert_workbooks_equal(expected: &Workbook, actual: &Workbook) -> Result<(), TestCaseError> {
    prop_assert_eq!(actual.date_system, expected.date_system, "date system");
    prop_assert_eq!(actual.sheets.len(), expected.sheets.len(), "sheet count");

    for (sheet_index, (expected_sheet, actual_sheet)) in
        expected.sheets.iter().zip(&actual.sheets).enumerate()
    {
        prop_assert_eq!(
            &actual_sheet.name,
            &expected_sheet.name,
            "sheet {} name",
            sheet_index
        );
        prop_assert_eq!(
            sorted_f32_map(&actual_sheet.col_widths),
            sorted_f32_map(&expected_sheet.col_widths),
            "sheet {} column widths",
            sheet_index
        );
        prop_assert_eq!(
            sorted_f32_map(&actual_sheet.row_heights),
            sorted_f32_map(&expected_sheet.row_heights),
            "sheet {} row heights",
            sheet_index
        );

        let expected_cells = sorted_cells(expected_sheet);
        let actual_cells = sorted_cells(actual_sheet);
        prop_assert_eq!(
            actual_cells.len(),
            expected_cells.len(),
            "sheet {} cell count",
            sheet_index
        );

        for ((expected_coord, expected_cell), (actual_coord, actual_cell)) in
            expected_cells.into_iter().zip(actual_cells)
        {
            let address = expected_coord.to_a1();
            prop_assert_eq!(
                actual_coord,
                expected_coord,
                "sheet {} cell address",
                sheet_index
            );
            assert_cells_equal(sheet_index, &address, expected_cell, actual_cell)?;
        }
    }

    Ok(())
}

fn assert_cells_equal(
    sheet_index: usize,
    address: &str,
    expected: &Cell,
    actual: &Cell,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        &actual.raw,
        &expected.raw,
        "sheet {} cell {} raw",
        sheet_index,
        address
    );
    prop_assert_eq!(
        actual.ast.is_some(),
        expected.ast.is_some(),
        "sheet {} cell {} parsed formula presence",
        sheet_index,
        address
    );
    if expected.raw.starts_with('=') {
        prop_assert!(
            actual.ast.is_some(),
            "sheet {} cell {} formula did not parse after load",
            sheet_index,
            address
        );
    }
    prop_assert_eq!(
        &actual.cached,
        &expected.cached,
        "sheet {} cell {} cached value",
        sheet_index,
        address
    );
    prop_assert_eq!(
        &actual.format,
        &expected.format,
        "sheet {} cell {} format",
        sheet_index,
        address
    );
    Ok(())
}

fn sorted_cells(sheet: &Sheet) -> Vec<(Coord, &Cell)> {
    let mut cells: Vec<_> = sheet.iter_cells().collect();
    cells.sort_by_key(|(coord, _)| *coord);
    cells
}

fn sorted_f32_map(map: &std::collections::HashMap<u32, f32>) -> Vec<(u32, f32)> {
    let mut entries: Vec<_> = map.iter().map(|(key, value)| (*key, *value)).collect();
    entries.sort_by_key(|(key, _)| *key);
    entries
}

fn id(addr: &str) -> CellId {
    CellId::new(0, Coord::from_a1(addr).unwrap())
}

struct TempNativePath {
    path: PathBuf,
}

impl TempNativePath {
    fn new(name: &str) -> Self {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            path: std::env::temp_dir().join(format!(
                "xlite-core-native-roundtrip-{name}-{}-{unique}.xlite",
                std::process::id()
            )),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempNativePath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
