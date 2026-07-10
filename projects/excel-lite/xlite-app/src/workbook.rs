use serde::{Deserialize, Serialize};
use xlite_core::model::{display, Align, CellFormat, DateFormat, NumberFormat, MAX_COLS, MAX_ROWS};
use xlite_core::{
    export_csv as core_export_csv, import_csv as core_import_csv, import_xlsx as core_import_xlsx,
    load_native, save_native, CellId, Command, Coord, History, RecalcDelta as CoreRecalcDelta,
    RecalcEngine, ResizeColumnCommand, ResizeRowCommand, SetFormatCommand, StructuralAxis,
    StructuralEditCommand, StructuralEditKind, Value, Workbook,
};

const VIEWPORT_CELL_LIMIT: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CellValue {
    Blank,
    Number { value: f64 },
    Text { value: String },
    Boolean { value: bool },
    Error { code: String },
}

impl From<&Value> for CellValue {
    fn from(value: &Value) -> Self {
        match value {
            Value::Blank => Self::Blank,
            Value::Number(value) => Self::Number { value: *value },
            Value::Text(value) => Self::Text {
                value: value.clone(),
            },
            Value::Boolean(value) => Self::Boolean { value: *value },
            Value::Error(error) => Self::Error {
                code: error.code().to_string(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CellSnapshot {
    pub addr: String,
    pub raw: String,
    pub value: CellValue,
    pub display: String,
}

impl CellSnapshot {
    fn blank(addr: String) -> Self {
        Self {
            addr,
            raw: String::new(),
            value: CellValue::Blank,
            display: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookMeta {
    pub workbook_id: String,
    pub active_sheet: u16,
    pub rows: u32,
    pub cols: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecalcDelta {
    pub changed: Vec<CellSnapshot>,
    pub circular: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RectA1 {
    pub start: String,
    pub end: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppCellFormat {
    pub number: AppNumberFormat,
    pub align: AppAlign,
    pub bold: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppNumberFormat {
    General,
    Fixed { dp: u8 },
    Percent { dp: u8 },
    Currency { dp: u8 },
    DateIso,
    Text,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AppAlign {
    Default,
    Left,
    Center,
    Right,
}

impl From<AppCellFormat> for CellFormat {
    fn from(format: AppCellFormat) -> Self {
        Self {
            number: format.number.into(),
            align: format.align.into(),
            bold: format.bold,
        }
    }
}

impl From<AppNumberFormat> for NumberFormat {
    fn from(format: AppNumberFormat) -> Self {
        match format {
            AppNumberFormat::General => Self::General,
            AppNumberFormat::Fixed { dp } => Self::Fixed(dp),
            AppNumberFormat::Percent { dp } => Self::Percent(dp),
            AppNumberFormat::Currency { dp } => Self::Currency(dp),
            AppNumberFormat::DateIso => Self::Date(DateFormat::Iso),
            AppNumberFormat::Text => Self::Text,
        }
    }
}

impl From<AppAlign> for Align {
    fn from(align: AppAlign) -> Self {
        match align {
            AppAlign::Default => Self::Default,
            AppAlign::Left => Self::Left,
            AppAlign::Center => Self::Center,
            AppAlign::Right => Self::Right,
        }
    }
}

pub struct WorkbookAdapter {
    engine: RecalcEngine,
    history: History,
}

impl Default for WorkbookAdapter {
    fn default() -> Self {
        Self {
            engine: RecalcEngine::new(Workbook::new()),
            history: History::new(),
        }
    }
}

impl WorkbookAdapter {
    pub fn new_workbook(&mut self) -> WorkbookMeta {
        *self = Self::default();
        self.meta()
    }

    pub fn meta(&self) -> WorkbookMeta {
        WorkbookMeta {
            workbook_id: "local-scratch".to_string(),
            active_sheet: 0,
            rows: MAX_ROWS,
            cols: MAX_COLS,
        }
    }

    pub fn set_cell(&mut self, sheet: u16, addr: &str, raw: &str) -> Result<RecalcDelta, String> {
        let id = self.cell_id(sheet, addr)?;
        let delta = self
            .history
            .exec(Box::new(SetCellCommand::new(id, raw)), &mut self.engine);
        Ok(self.delta_from_core(delta))
    }

    pub fn insert_rows(&mut self, sheet: u16, at: u32, count: u32) -> Result<RecalcDelta, String> {
        self.structural_edit(
            sheet,
            StructuralAxis::Row,
            StructuralEditKind::Insert,
            at,
            count,
        )
    }

    pub fn delete_rows(&mut self, sheet: u16, at: u32, count: u32) -> Result<RecalcDelta, String> {
        self.structural_edit(
            sheet,
            StructuralAxis::Row,
            StructuralEditKind::Delete,
            at,
            count,
        )
    }

    pub fn insert_cols(&mut self, sheet: u16, at: u32, count: u32) -> Result<RecalcDelta, String> {
        self.structural_edit(
            sheet,
            StructuralAxis::Col,
            StructuralEditKind::Insert,
            at,
            count,
        )
    }

    pub fn delete_cols(&mut self, sheet: u16, at: u32, count: u32) -> Result<RecalcDelta, String> {
        self.structural_edit(
            sheet,
            StructuralAxis::Col,
            StructuralEditKind::Delete,
            at,
            count,
        )
    }

    pub fn set_format(
        &mut self,
        sheet: u16,
        rect: RectA1,
        format: AppCellFormat,
    ) -> Result<RecalcDelta, String> {
        let (start, end) = rect_bounds(&rect).map_err(|error| {
            format!(
                "failed to set format on sheet {sheet} for {}:{}: {error}",
                rect.start, rect.end
            )
        })?;
        let command = SetFormatCommand::for_engine(&self.engine, sheet, start, end, format.into())
            .map_err(|error| {
                format!(
                    "failed to set format on sheet {sheet} for {}:{}: {error}",
                    rect.start, rect.end
                )
            })?;
        let delta = self.history.exec(Box::new(command), &mut self.engine);
        Ok(self.delta_from_core(delta))
    }

    pub fn resize_column(
        &mut self,
        sheet: u16,
        col: u32,
        width: f32,
    ) -> Result<RecalcDelta, String> {
        let command =
            ResizeColumnCommand::for_engine(&self.engine, sheet, col, width).map_err(|error| {
                format!("failed to resize column on sheet {sheet} at {col} to {width}: {error}")
            })?;
        let delta = self.history.exec(Box::new(command), &mut self.engine);
        Ok(self.delta_from_core(delta))
    }

    pub fn resize_row(&mut self, sheet: u16, row: u32, height: f32) -> Result<RecalcDelta, String> {
        let command =
            ResizeRowCommand::for_engine(&self.engine, sheet, row, height).map_err(|error| {
                format!("failed to resize row on sheet {sheet} at {row} to {height}: {error}")
            })?;
        let delta = self.history.exec(Box::new(command), &mut self.engine);
        Ok(self.delta_from_core(delta))
    }

    pub fn undo(&mut self) -> Result<Option<RecalcDelta>, String> {
        Ok(self
            .history
            .undo(&mut self.engine)
            .map(|delta| self.delta_from_core(delta)))
    }

    pub fn redo(&mut self) -> Result<Option<RecalcDelta>, String> {
        Ok(self
            .history
            .redo(&mut self.engine)
            .map(|delta| self.delta_from_core(delta)))
    }

    pub fn save_workbook(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        save_native(self.engine.workbook(), path.as_ref())
            .map_err(|error| format!("failed to save native workbook: {error}"))
    }

    pub fn open_workbook(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<WorkbookMeta, String> {
        let workbook = load_native(path.as_ref())
            .map_err(|error| format!("failed to open native workbook: {error}"))?;
        let mut engine = RecalcEngine::new(workbook);
        engine.recalc_all();
        self.engine = engine;
        self.history = History::new();
        Ok(self.meta())
    }

    pub fn import_csv(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<WorkbookMeta, String> {
        let path = path.as_ref();
        let workbook = core_import_csv(path)
            .map_err(|error| format!("failed to import CSV from {}: {error}", path.display()))?;
        let mut engine = RecalcEngine::new(workbook);
        engine.recalc_all();
        self.engine = engine;
        self.history = History::new();
        Ok(self.meta())
    }

    pub fn import_xlsx(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<WorkbookMeta, String> {
        let path = path.as_ref();
        let workbook = core_import_xlsx(path)
            .map_err(|error| format!("failed to import XLSX from {}: {error}", path.display()))?;
        let mut engine = RecalcEngine::new(workbook);
        engine.recalc_all();
        self.engine = engine;
        self.history = History::new();
        Ok(self.meta())
    }

    pub fn export_csv(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref();
        core_export_csv(self.engine.workbook(), path)
            .map_err(|error| format!("failed to export CSV to {}: {error}", path.display()))
    }

    pub fn get_cell(&self, sheet: u16, addr: &str) -> Result<CellSnapshot, String> {
        let id = self.cell_id(sheet, addr)?;
        Ok(self.snapshot(id))
    }

    pub fn get_viewport(&self, sheet: u16, rect: RectA1) -> Result<Vec<CellSnapshot>, String> {
        self.ensure_sheet(sheet)?;
        let (start, end) = rect_bounds(&rect)?;
        let row_count = end.row - start.row + 1;
        let col_count = end.col - start.col + 1;
        let cell_count = row_count as usize * col_count as usize;

        if cell_count > VIEWPORT_CELL_LIMIT {
            return Err(format!("viewport too large: {cell_count} cells"));
        }

        let mut snapshots = Vec::with_capacity(cell_count);
        for row in start.row..=end.row {
            for col in start.col..=end.col {
                snapshots.push(self.snapshot(CellId::new(sheet, Coord { row, col })));
            }
        }
        Ok(snapshots)
    }

    fn delta_from_core(&self, delta: CoreRecalcDelta) -> RecalcDelta {
        RecalcDelta {
            changed: delta
                .changed
                .into_iter()
                .map(|(id, _)| self.snapshot(id))
                .collect(),
            circular: delta
                .circular
                .into_iter()
                .map(|id| id.coord.to_a1())
                .collect(),
        }
    }

    fn structural_edit(
        &mut self,
        sheet: u16,
        axis: StructuralAxis,
        kind: StructuralEditKind,
        at: u32,
        count: u32,
    ) -> Result<RecalcDelta, String> {
        let command = StructuralEditCommand::for_engine(&self.engine, sheet, axis, kind, at, count)
            .map_err(|error| {
                format!(
                    "failed to {} {} on sheet {sheet} at {at} for {count}: {error}",
                    edit_action(kind),
                    edit_unit(axis, count),
                )
            })?;
        let delta = self.history.exec(Box::new(command), &mut self.engine);
        Ok(self.delta_from_core(delta))
    }

    fn cell_id(&self, sheet: u16, addr: &str) -> Result<CellId, String> {
        self.ensure_sheet(sheet)?;
        Ok(CellId::new(sheet, coord_from_a1(addr)?))
    }

    fn ensure_sheet(&self, sheet: u16) -> Result<(), String> {
        if self.engine.workbook().sheet(sheet).is_some() {
            Ok(())
        } else {
            Err(format!("unknown sheet: {sheet}"))
        }
    }

    fn snapshot(&self, id: CellId) -> CellSnapshot {
        let addr = id.coord.to_a1();
        let Some(cell) = self
            .engine
            .workbook()
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
        else {
            return CellSnapshot::blank(addr);
        };

        CellSnapshot {
            addr,
            raw: cell.raw.clone(),
            value: CellValue::from(&cell.cached),
            display: display(&cell.cached, &cell.format),
        }
    }
}

fn edit_action(kind: StructuralEditKind) -> &'static str {
    match kind {
        StructuralEditKind::Insert => "insert",
        StructuralEditKind::Delete => "delete",
    }
}

fn edit_unit(axis: StructuralAxis, count: u32) -> &'static str {
    match (axis, count) {
        (StructuralAxis::Row, 1) => "row",
        (StructuralAxis::Row, _) => "rows",
        (StructuralAxis::Col, 1) => "column",
        (StructuralAxis::Col, _) => "columns",
    }
}

#[derive(Clone, Debug)]
struct SetCellCommand {
    id: CellId,
    raw: String,
    prior_raw: Option<String>,
}

impl SetCellCommand {
    fn new(id: CellId, raw: &str) -> Self {
        Self {
            id,
            raw: raw.to_string(),
            prior_raw: None,
        }
    }

    fn raw_at(engine: &RecalcEngine, id: CellId) -> Option<String> {
        engine
            .workbook()
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
            .map(|cell| cell.raw.clone())
    }
}

impl Command for SetCellCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> CoreRecalcDelta {
        self.prior_raw = Self::raw_at(engine, self.id);
        engine.set_cell(self.id, &self.raw)
    }

    fn invert(&self) -> Box<dyn Command> {
        Box::new(Self {
            id: self.id,
            raw: self.prior_raw.clone().unwrap_or_default(),
            prior_raw: None,
        })
    }

    fn label(&self) -> String {
        format!("Edit {}", self.id.coord.to_a1())
    }
}

fn rect_bounds(rect: &RectA1) -> Result<(Coord, Coord), String> {
    let start = coord_from_a1(&rect.start)?;
    let end = coord_from_a1(&rect.end)?;
    Ok((
        Coord {
            row: start.row.min(end.row),
            col: start.col.min(end.col),
        },
        Coord {
            row: start.row.max(end.row),
            col: start.col.max(end.col),
        },
    ))
}

fn coord_from_a1(addr: &str) -> Result<Coord, String> {
    Coord::from_a1(addr.trim()).map_err(|_| format!("invalid cell address: {addr}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    use xlite_core::model::{CellFormat, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT};
    use xlite_core::{load_native as core_load_native, Cell, Value};
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    static NEXT_PATH_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "xlite-app-workbook-{name}-{}-{unique}.xlite",
            std::process::id()
        ))
    }

    fn temp_csv_path(name: &str) -> PathBuf {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "xlite-app-workbook-{name}-{}-{unique}.csv",
            std::process::id()
        ))
    }

    fn temp_xlsx_path(name: &str) -> PathBuf {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "xlite-app-workbook-{name}-{}-{unique}.xlsx",
            std::process::id()
        ))
    }

    fn write_xlsx(path: &Path, sheet_xml: &str) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        write_xlsx_part(
            &mut zip,
            options,
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#,
        );
        write_xlsx_part(
            &mut zip,
            options,
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#,
        );
        write_xlsx_part(
            &mut zip,
            options,
            "xl/workbook.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<workbookPr date1904="false"/>
<sheets><sheet name="Import" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#,
        );
        write_xlsx_part(
            &mut zip,
            options,
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#,
        );
        write_xlsx_part(&mut zip, options, "xl/worksheets/sheet1.xml", sheet_xml);

        zip.finish().unwrap();
    }

    fn write_xlsx_part(
        zip: &mut ZipWriter<fs::File>,
        options: SimpleFileOptions,
        name: &str,
        body: &str,
    ) {
        zip.start_file(name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }

    const XLSX_IMPORT_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1:C2"/>
<sheetData>
<row r="1">
<c r="A1"><v>2</v></c>
<c r="B1"><f>SUM(A1:A2)</f><v>999</v></c>
<c r="C1" t="inlineStr"><is><t>fresh</t></is></c>
</row>
<row r="2">
<c r="A2"><v>3</v></c>
</row>
</sheetData>
</worksheet>"#;

    fn assert_delta_contains(delta: &RecalcDelta, addr: &str, display: &str) {
        assert!(
            delta
                .changed
                .iter()
                .any(|snapshot| snapshot.addr == addr && snapshot.display == display),
            "expected delta to contain {addr} with display {display:?}, got {:?}",
            delta.changed
        );
    }

    fn id(addr: &str) -> CellId {
        CellId::new(0, Coord::from_a1(addr).expect("valid test address"))
    }

    fn rect(start: &str, end: &str) -> RectA1 {
        RectA1 {
            start: start.to_string(),
            end: end.to_string(),
        }
    }

    fn app_format(number: AppNumberFormat, align: AppAlign, bold: bool) -> AppCellFormat {
        AppCellFormat {
            number,
            align,
            bold,
        }
    }

    fn fixed_format(dp: u8) -> AppCellFormat {
        app_format(AppNumberFormat::Fixed { dp }, AppAlign::Default, false)
    }

    fn percent_format(dp: u8) -> AppCellFormat {
        app_format(AppNumberFormat::Percent { dp }, AppAlign::Default, false)
    }

    fn currency_format(dp: u8) -> AppCellFormat {
        app_format(AppNumberFormat::Currency { dp }, AppAlign::Right, true)
    }

    fn stored_format(workbook: &WorkbookAdapter, addr: &str) -> Option<CellFormat> {
        workbook
            .engine
            .workbook()
            .sheet(0)
            .and_then(|sheet| sheet.get_cell(id(addr).coord))
            .map(|cell| cell.format.clone())
    }

    fn col_width(workbook: &WorkbookAdapter, col: u32) -> Option<f32> {
        workbook
            .engine
            .workbook()
            .sheet(0)
            .and_then(|sheet| sheet.col_widths.get(&col).copied())
    }

    fn row_height(workbook: &WorkbookAdapter, row: u32) -> Option<f32> {
        workbook
            .engine
            .workbook()
            .sheet(0)
            .and_then(|sheet| sheet.row_heights.get(&row).copied())
    }

    #[test]
    fn evaluates_slice_zero_demo_formulas_through_core() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "A2", "2").unwrap();
        workbook.set_cell(0, "A3", "3").unwrap();

        workbook.set_cell(0, "B1", "=SUM(A1:A3)").unwrap();
        workbook.set_cell(0, "B2", "=AVERAGE(A1:A3)").unwrap();
        workbook
            .set_cell(0, "B3", r#"=IF(A1>0,"pos","neg")"#)
            .unwrap();

        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "6");
        assert_eq!(workbook.get_cell(0, "B2").unwrap().display, "2");
        assert_eq!(workbook.get_cell(0, "B3").unwrap().display, "pos");

        let delta = workbook.set_cell(0, "A1", "5").unwrap();
        assert!(delta
            .changed
            .iter()
            .any(|snapshot| snapshot.addr == "B1" && snapshot.display == "10"));
    }

    #[test]
    fn undo_restores_precedent_and_recalculates_dependents() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "B1", "=A1*2").unwrap();
        workbook.set_cell(0, "A1", "5").unwrap();

        let undo = workbook.undo().unwrap().expect("undo cell edit");

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "1");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "2");
        assert_delta_contains(&undo, "A1", "1");
        assert_delta_contains(&undo, "B1", "2");
    }

    #[test]
    fn redo_reapplies_precedent_edit_and_recalculates_dependents() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "B1", "=A1*2").unwrap();
        workbook.set_cell(0, "A1", "5").unwrap();
        workbook.undo().unwrap().expect("undo cell edit");

        let redo = workbook.redo().unwrap().expect("redo cell edit");

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "5");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "10");
        assert_delta_contains(&redo, "A1", "5");
        assert_delta_contains(&redo, "B1", "10");
    }

    #[test]
    fn new_edit_after_undo_clears_redo() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "A1", "2").unwrap();
        workbook.undo().unwrap().expect("undo cell edit");

        workbook.set_cell(0, "B1", "9").unwrap();

        assert!(workbook.redo().unwrap().is_none());
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "1");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().raw, "9");
    }

    #[test]
    fn insert_column_before_a_shifts_values_formulas_and_returns_delta() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "7").unwrap();
        workbook.set_cell(0, "B1", "=A1+1").unwrap();

        let delta = workbook.insert_cols(0, 0, 1).unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().raw, "7");
        let formula = workbook.get_cell(0, "C1").unwrap();
        assert_eq!(formula.raw, "=B1+1");
        assert_eq!(formula.display, "8");
        assert_delta_contains(&delta, "A1", "");
        assert_delta_contains(&delta, "B1", "7");
        assert_delta_contains(&delta, "C1", "8");
    }

    #[test]
    fn delete_column_a_rewrites_deleted_reference_and_undo_restores_cells() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "7").unwrap();
        workbook.set_cell(0, "B1", "=A1").unwrap();

        workbook.delete_cols(0, 0, 1).unwrap();

        let shifted_formula = workbook.get_cell(0, "A1").unwrap();
        assert_eq!(shifted_formula.raw, "=#REF!");
        assert_eq!(shifted_formula.display, "#REF!");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().raw, "");

        let undo = workbook.undo().unwrap().expect("undo delete column");

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "7");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().raw, "=A1");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "7");
        assert_delta_contains(&undo, "A1", "7");
        assert_delta_contains(&undo, "B1", "7");
    }

    #[test]
    fn insert_row_preserves_formula_looking_literal_text_after_move() {
        let mut workbook = WorkbookAdapter::default();
        let literal = Cell::new("=SUM(A1:A1)", None, Value::Text("=SUM(A1:A1)".to_string()));
        workbook
            .engine
            .workbook_mut()
            .sheet_mut(0)
            .unwrap()
            .set_cell(id("A1").coord, literal);

        workbook.insert_rows(0, 0, 1).unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "");
        let moved = workbook.get_cell(0, "A2").unwrap();
        assert_eq!(moved.raw, "=SUM(A1:A1)");
        assert_eq!(moved.display, "=SUM(A1:A1)");
        assert_eq!(
            moved.value,
            CellValue::Text {
                value: "=SUM(A1:A1)".to_string()
            }
        );
    }

    #[test]
    fn structural_edit_after_undo_clears_redo() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.insert_cols(0, 0, 1).unwrap();
        workbook.undo().unwrap().expect("undo structural edit");

        workbook.insert_rows(0, 0, 1).unwrap();

        assert!(workbook.redo().unwrap().is_none());
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "");
        assert_eq!(workbook.get_cell(0, "A2").unwrap().raw, "1");
    }

    #[test]
    fn invalid_structural_edits_surface_errors_without_mutation_or_undo() {
        let mut invalid_sheet = WorkbookAdapter::default();
        invalid_sheet.engine.set_cell(id("A1"), "kept");
        let error = invalid_sheet.insert_rows(1, 0, 1).unwrap_err();
        assert!(error.contains("failed to insert row on sheet 1"));
        assert!(error.contains("unknown sheet: 1"));
        assert_eq!(invalid_sheet.get_cell(0, "A1").unwrap().raw, "kept");
        assert!(invalid_sheet.undo().unwrap().is_none());

        let mut out_of_bounds = WorkbookAdapter::default();
        out_of_bounds.engine.set_cell(id("A1"), "kept");
        let error = out_of_bounds.delete_cols(0, MAX_COLS - 1, 2).unwrap_err();
        assert!(error.contains("failed to delete columns on sheet 0"));
        assert!(error.contains("exceeds grid bounds"));
        assert_eq!(out_of_bounds.get_cell(0, "A1").unwrap().raw, "kept");
        assert!(out_of_bounds.undo().unwrap().is_none());

        let mut zero_count = WorkbookAdapter::default();
        zero_count.engine.set_cell(id("A1"), "kept");
        let error = zero_count.insert_rows(0, 0, 0).unwrap_err();
        assert!(error.contains("failed to insert rows on sheet 0"));
        assert!(error.contains("count must be greater than zero"));
        assert_eq!(zero_count.get_cell(0, "A1").unwrap().raw, "kept");
        assert!(zero_count.undo().unwrap().is_none());
    }

    #[test]
    fn formatting_numeric_cell_changes_display_without_changing_value() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "12.345").unwrap();

        let delta = workbook
            .set_format(0, rect("A1", "A1"), fixed_format(2))
            .unwrap();

        let cell = workbook.get_cell(0, "A1").unwrap();
        assert_eq!(cell.raw, "12.345");
        assert_eq!(cell.value, CellValue::Number { value: 12.345 });
        assert_eq!(cell.display, "12.35");
        assert_delta_contains(&delta, "A1", "12.35");
    }

    #[test]
    fn formatting_formula_cell_preserves_formula_and_cached_result() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "2").unwrap();
        workbook.set_cell(0, "A2", "3").unwrap();
        workbook.set_cell(0, "B1", "=SUM(A1:A2)").unwrap();

        let delta = workbook
            .set_format(0, rect("B1", "B1"), currency_format(2))
            .unwrap();

        let formula = workbook.get_cell(0, "B1").unwrap();
        assert_eq!(formula.raw, "=SUM(A1:A2)");
        assert_eq!(formula.value, CellValue::Number { value: 5.0 });
        assert_eq!(formula.display, "$5.00");
        assert_delta_contains(&delta, "B1", "$5.00");
    }

    #[test]
    fn formatting_blank_cell_persists_through_snapshots_and_history() {
        let mut workbook = WorkbookAdapter::default();
        let format = fixed_format(2);

        let delta = workbook
            .set_format(0, rect("C3", "C3"), format.clone())
            .unwrap();

        let cell = workbook.get_cell(0, "C3").unwrap();
        assert_eq!(cell.raw, "");
        assert_eq!(cell.value, CellValue::Blank);
        assert_eq!(cell.display, "");
        assert_delta_contains(&delta, "C3", "");
        let viewport = workbook.get_viewport(0, rect("C3", "C3")).unwrap();
        assert_eq!(viewport, vec![cell]);
        assert_eq!(stored_format(&workbook, "C3"), Some(format.clone().into()));

        workbook.undo().unwrap().expect("undo blank format");

        assert_eq!(workbook.get_cell(0, "C3").unwrap().display, "");
        assert_eq!(stored_format(&workbook, "C3"), None);

        workbook.redo().unwrap().expect("redo blank format");

        assert_eq!(stored_format(&workbook, "C3"), Some(format.into()));
        workbook.set_cell(0, "C3", "1").unwrap();
        assert_eq!(workbook.get_cell(0, "C3").unwrap().display, "1.00");
    }

    #[test]
    fn multi_cell_format_undo_redo_restores_displays_and_clears_redo() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1.234").unwrap();
        workbook.set_cell(0, "B1", "2.345").unwrap();

        workbook
            .set_format(0, rect("A1", "B1"), fixed_format(1))
            .unwrap();
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "1.2");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "2.3");

        let undo = workbook.undo().unwrap().expect("undo multi-cell format");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "1.234");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "2.345");
        assert_delta_contains(&undo, "A1", "1.234");
        assert_delta_contains(&undo, "B1", "2.345");

        let redo = workbook.redo().unwrap().expect("redo multi-cell format");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "1.2");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "2.3");
        assert_delta_contains(&redo, "A1", "1.2");
        assert_delta_contains(&redo, "B1", "2.3");

        workbook.undo().unwrap().expect("undo before new format");
        workbook
            .set_format(0, rect("A1", "A1"), percent_format(1))
            .unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "123.4%");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "2.345");
        assert!(workbook.redo().unwrap().is_none());
    }

    #[test]
    fn resize_column_and_row_undo_redo_restore_dimensions() {
        let mut workbook = WorkbookAdapter::default();

        let col_delta = workbook.resize_column(0, 1, 128.0).unwrap();
        let row_delta = workbook.resize_row(0, 2, 48.0).unwrap();

        assert!(col_delta.changed.is_empty());
        assert!(row_delta.changed.is_empty());
        assert_eq!(col_width(&workbook, 1), Some(128.0));
        assert_eq!(row_height(&workbook, 2), Some(48.0));

        workbook.undo().unwrap().expect("undo row resize");
        assert_eq!(col_width(&workbook, 1), Some(128.0));
        assert_eq!(row_height(&workbook, 2), None);

        workbook.undo().unwrap().expect("undo column resize");
        assert_eq!(col_width(&workbook, 1), None);
        assert_eq!(row_height(&workbook, 2), None);

        workbook.redo().unwrap().expect("redo column resize");
        assert_eq!(col_width(&workbook, 1), Some(128.0));
        assert_eq!(row_height(&workbook, 2), None);

        workbook.redo().unwrap().expect("redo row resize");
        assert_eq!(col_width(&workbook, 1), Some(128.0));
        assert_eq!(row_height(&workbook, 2), Some(48.0));
    }

    #[test]
    fn resizing_to_default_dimensions_removes_sparse_overrides() {
        let mut workbook = WorkbookAdapter::default();

        workbook.resize_column(0, 0, 111.0).unwrap();
        workbook.resize_column(0, 0, DEFAULT_COL_WIDTH).unwrap();
        workbook.resize_row(0, 0, 33.0).unwrap();
        workbook.resize_row(0, 0, DEFAULT_ROW_HEIGHT).unwrap();

        assert_eq!(col_width(&workbook, 0), None);
        assert_eq!(row_height(&workbook, 0), None);

        workbook.undo().unwrap().expect("undo row reset to default");
        assert_eq!(row_height(&workbook, 0), Some(33.0));
        workbook.redo().unwrap().expect("redo row reset to default");
        assert_eq!(row_height(&workbook, 0), None);

        workbook
            .undo()
            .unwrap()
            .expect("undo row reset before column undo");
        workbook.undo().unwrap().expect("undo row resize");
        workbook
            .undo()
            .unwrap()
            .expect("undo column reset to default");
        assert_eq!(col_width(&workbook, 0), Some(111.0));
        workbook
            .redo()
            .unwrap()
            .expect("redo column reset to default");
        assert_eq!(col_width(&workbook, 0), None);
    }

    #[test]
    fn invalid_format_and_resize_edits_surface_errors_without_mutation_or_undo() {
        let mut workbook = WorkbookAdapter::default();
        workbook.engine.set_cell(id("A1"), "kept");

        let error = workbook
            .set_format(1, rect("A1", "A1"), fixed_format(2))
            .unwrap_err();
        assert!(error.contains("failed to set format on sheet 1"));
        assert!(error.contains("unknown sheet: 1"));

        let error = workbook
            .set_format(0, rect("A0", "A1"), fixed_format(2))
            .unwrap_err();
        assert!(error.contains("failed to set format on sheet 0"));
        assert!(error.contains("invalid cell address: A0"));

        let error = workbook.resize_column(0, MAX_COLS, 120.0).unwrap_err();
        assert!(error.contains("failed to resize column on sheet 0"));
        assert!(error.contains("column index"));
        assert!(error.contains("exceeds grid bounds"));

        let error = workbook.resize_row(0, MAX_ROWS, 30.0).unwrap_err();
        assert!(error.contains("failed to resize row on sheet 0"));
        assert!(error.contains("row index"));
        assert!(error.contains("exceeds grid bounds"));

        let error = workbook.resize_column(0, 0, 0.0).unwrap_err();
        assert!(error.contains("failed to resize column on sheet 0"));
        assert!(error.contains("resize size must be finite and positive"));

        let error = workbook.resize_row(1, 0, 30.0).unwrap_err();
        assert!(error.contains("failed to resize row on sheet 1"));
        assert!(error.contains("unknown sheet: 1"));

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "kept");
        assert_eq!(col_width(&workbook, 0), None);
        assert_eq!(row_height(&workbook, 0), None);
        assert!(workbook.undo().unwrap().is_none());
    }

    #[test]
    fn undo_and_redo_on_empty_history_return_none() {
        let mut workbook = WorkbookAdapter::default();

        assert!(workbook.undo().unwrap().is_none());
        assert!(workbook.redo().unwrap().is_none());
    }

    #[test]
    fn undo_of_first_edit_restores_blank_cell() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "42").unwrap();

        let undo = workbook.undo().unwrap().expect("undo first edit");

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "");
        assert_delta_contains(&undo, "A1", "");
    }

    #[test]
    fn new_workbook_clears_existing_history() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "A1", "2").unwrap();

        let meta = workbook.new_workbook();

        assert_eq!(meta.workbook_id, "local-scratch");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "");
        assert!(workbook.undo().unwrap().is_none());
    }

    #[test]
    fn viewport_returns_cell_snapshots() {
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "42").unwrap();

        let viewport = workbook
            .get_viewport(
                0,
                RectA1 {
                    start: "A1".to_string(),
                    end: "B2".to_string(),
                },
            )
            .unwrap();

        assert_eq!(viewport.len(), 4);
        assert_eq!(viewport[0].addr, "A1");
        assert_eq!(viewport[0].display, "42");
        assert_eq!(viewport[3].addr, "B2");
    }

    #[test]
    fn reports_self_reference_as_circular() {
        let mut workbook = WorkbookAdapter::default();
        let delta = workbook.set_cell(0, "A1", "=SUM(A1:A1)").unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "#CIRC!");
        assert_eq!(delta.circular, vec!["A1".to_string()]);
    }

    #[test]
    fn save_workbook_writes_native_file_loadable_by_core() {
        let path = temp_path("adapter-save");
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "7").unwrap();
        workbook.set_cell(0, "B1", "=A1+5").unwrap();

        workbook.save_workbook(&path).unwrap();

        let loaded = core_load_native(&path).unwrap();
        let sheet = loaded.sheet(0).unwrap();
        assert_eq!(
            sheet.get_cell(Coord::from_a1("A1").unwrap()).unwrap().raw,
            "7"
        );
        assert_eq!(
            sheet.get_cell(Coord::from_a1("B1").unwrap()).unwrap().raw,
            "=A1+5"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn open_workbook_replaces_state_and_returns_metadata() {
        let path = temp_path("adapter-open");
        let mut saved = WorkbookAdapter::default();
        saved.set_cell(0, "A1", "42").unwrap();
        saved.set_cell(0, "B1", "=A1*2").unwrap();
        saved.save_workbook(&path).unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "stale").unwrap();

        let meta = workbook.open_workbook(&path).unwrap();

        assert_eq!(meta.workbook_id, "local-scratch");
        assert_eq!(meta.active_sheet, 0);
        assert_eq!(meta.rows, MAX_ROWS);
        assert_eq!(meta.cols, MAX_COLS);
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "42");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "42");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "84");
        assert_eq!(workbook.get_cell(0, "C1").unwrap().display, "");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn open_workbook_clears_existing_history() {
        let path = temp_path("adapter-open-clears-history");
        let mut saved = WorkbookAdapter::default();
        saved.set_cell(0, "A1", "saved").unwrap();
        saved.save_workbook(&path).unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "old").unwrap();
        workbook.set_cell(0, "A1", "new").unwrap();

        workbook.open_workbook(&path).unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "saved");
        assert!(workbook.undo().unwrap().is_none());
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "saved");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_workbook_does_not_clear_history() {
        let path = temp_path("adapter-save-keeps-history");
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "A1", "2").unwrap();

        workbook.save_workbook(&path).unwrap();
        let undo = workbook.undo().unwrap().expect("undo after save");

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "1");
        assert_delta_contains(&undo, "A1", "1");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn open_workbook_exposes_loaded_cells_through_viewport() {
        let path = temp_path("adapter-open-viewport");
        let mut saved = WorkbookAdapter::default();
        saved.set_cell(0, "A1", "3").unwrap();
        saved.set_cell(0, "A2", "4").unwrap();
        saved.set_cell(0, "B1", "=SUM(A1:A2)").unwrap();
        saved.save_workbook(&path).unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.open_workbook(&path).unwrap();

        assert_eq!(workbook.get_cell(0, "B1").unwrap().raw, "=SUM(A1:A2)");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "7");

        let viewport = workbook
            .get_viewport(
                0,
                RectA1 {
                    start: "A1".to_string(),
                    end: "B2".to_string(),
                },
            )
            .unwrap();

        assert_eq!(viewport.len(), 4);
        assert_eq!(viewport[0].addr, "A1");
        assert_eq!(viewport[0].raw, "3");
        assert_eq!(viewport[0].display, "3");
        assert_eq!(viewport[1].addr, "B1");
        assert_eq!(viewport[1].raw, "=SUM(A1:A2)");
        assert_eq!(viewport[1].display, "7");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn open_workbook_rebuilds_dynamic_dependencies_before_edits() {
        let path = temp_path("adapter-open-dynamic");
        let mut saved = WorkbookAdapter::default();
        saved.set_cell(0, "A1", "B1").unwrap();
        saved.set_cell(0, "B1", "10").unwrap();
        saved.set_cell(0, "C1", "=INDIRECT(A1)+1").unwrap();
        saved.save_workbook(&path).unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.open_workbook(&path).unwrap();

        assert_eq!(workbook.get_cell(0, "C1").unwrap().display, "11");

        let delta = workbook.set_cell(0, "B1", "20").unwrap();

        assert_eq!(workbook.get_cell(0, "C1").unwrap().display, "21");
        assert!(delta
            .changed
            .iter()
            .any(|snapshot| snapshot.addr == "C1" && snapshot.display == "21"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_xlsx_replaces_state_exposes_cells_and_recalculates_formulas() {
        let path = temp_xlsx_path("adapter-import-values");
        write_xlsx(&path, XLSX_IMPORT_SHEET);

        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "stale").unwrap();
        workbook.set_cell(0, "C2", "old").unwrap();

        let meta = workbook.import_xlsx(&path).unwrap();

        assert_eq!(meta.workbook_id, "local-scratch");
        assert_eq!(meta.active_sheet, 0);
        assert_eq!(meta.rows, MAX_ROWS);
        assert_eq!(meta.cols, MAX_COLS);
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "2");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "2");
        assert_eq!(workbook.get_cell(0, "A2").unwrap().display, "3");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().raw, "=SUM(A1:A2)");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "5");
        assert_eq!(workbook.get_cell(0, "C1").unwrap().display, "fresh");
        assert_eq!(workbook.get_cell(0, "C2").unwrap().display, "");

        let viewport = workbook
            .get_viewport(
                0,
                RectA1 {
                    start: "A1".to_string(),
                    end: "C2".to_string(),
                },
            )
            .unwrap();
        assert_eq!(viewport.len(), 6);
        assert_eq!(viewport[0].addr, "A1");
        assert_eq!(viewport[0].display, "2");
        assert_eq!(viewport[1].addr, "B1");
        assert_eq!(viewport[1].raw, "=SUM(A1:A2)");
        assert_eq!(viewport[1].display, "5");
        assert_eq!(viewport[2].addr, "C1");
        assert_eq!(viewport[2].display, "fresh");
        assert_eq!(viewport[5].addr, "C2");
        assert_eq!(viewport[5].display, "");

        let delta = workbook.set_cell(0, "A1", "10").unwrap();
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "13");
        assert!(delta
            .changed
            .iter()
            .any(|snapshot| snapshot.addr == "B1" && snapshot.display == "13"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_xlsx_clears_existing_history() {
        let path = temp_xlsx_path("adapter-import-clears-history");
        write_xlsx(&path, XLSX_IMPORT_SHEET);

        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "old").unwrap();
        workbook.set_cell(0, "A1", "new").unwrap();

        workbook.import_xlsx(&path).unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "2");
        assert!(workbook.undo().unwrap().is_none());
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "2");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_xlsx_reports_missing_and_malformed_path_errors() {
        let missing = temp_xlsx_path("adapter-missing-import");
        let mut workbook = WorkbookAdapter::default();

        let error = workbook.import_xlsx(&missing).unwrap_err();

        assert!(error.contains("failed to import XLSX from"));
        assert!(error.contains(&missing.display().to_string()));
        assert!(error.contains("failed to import XLSX workbook"));

        let malformed = temp_xlsx_path("adapter-malformed-import");
        fs::write(&malformed, b"not a zip file").unwrap();

        let error = workbook.import_xlsx(&malformed).unwrap_err();

        assert!(error.contains("failed to import XLSX from"));
        assert!(error.contains(&malformed.display().to_string()));
        assert!(error.contains("failed to import XLSX workbook"));
        let _ = fs::remove_file(malformed);
    }

    #[test]
    fn import_csv_replaces_state_and_exposes_imported_cells() {
        let path = temp_csv_path("adapter-import-values");
        fs::write(&path, "1,2\ntext,\n").unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "stale").unwrap();
        workbook.set_cell(0, "C1", "old").unwrap();

        let meta = workbook.import_csv(&path).unwrap();

        assert_eq!(meta.workbook_id, "local-scratch");
        assert_eq!(meta.active_sheet, 0);
        assert_eq!(meta.rows, MAX_ROWS);
        assert_eq!(meta.cols, MAX_COLS);
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "1");
        assert_eq!(workbook.get_cell(0, "A1").unwrap().display, "1");
        assert_eq!(workbook.get_cell(0, "B1").unwrap().display, "2");
        assert_eq!(workbook.get_cell(0, "A2").unwrap().display, "text");
        assert_eq!(workbook.get_cell(0, "C1").unwrap().display, "");

        let viewport = workbook
            .get_viewport(
                0,
                RectA1 {
                    start: "A1".to_string(),
                    end: "B2".to_string(),
                },
            )
            .unwrap();
        assert_eq!(viewport.len(), 4);
        assert_eq!(viewport[0].addr, "A1");
        assert_eq!(viewport[0].display, "1");
        assert_eq!(viewport[1].addr, "B1");
        assert_eq!(viewport[1].display, "2");
        assert_eq!(viewport[2].addr, "A2");
        assert_eq!(viewport[2].display, "text");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_csv_keeps_formula_like_fields_as_literal_text() {
        let path = temp_csv_path("adapter-import-formula-like");
        fs::write(&path, "=SUM(A1:A1)\n").unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.import_csv(&path).unwrap();

        let cell = workbook.get_cell(0, "A1").unwrap();
        assert_eq!(cell.raw, "=SUM(A1:A1)");
        assert_eq!(cell.display, "=SUM(A1:A1)");
        assert_eq!(
            cell.value,
            CellValue::Text {
                value: "=SUM(A1:A1)".to_string()
            }
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn export_csv_writes_displayed_formula_values() {
        let path = temp_csv_path("adapter-export-formula-display");
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "2").unwrap();
        workbook.set_cell(0, "A2", "3").unwrap();
        workbook.set_cell(0, "B1", "=SUM(A1:A2)").unwrap();

        workbook.export_csv(&path).unwrap();

        let exported = fs::read_to_string(&path).unwrap();
        assert!(exported.contains("2,5"), "exported CSV: {exported:?}");
        assert!(
            !exported.contains("=SUM(A1:A2)"),
            "exported CSV: {exported:?}"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_csv_clears_existing_history() {
        let path = temp_csv_path("adapter-import-clears-history");
        fs::write(&path, "imported\n").unwrap();

        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "old").unwrap();
        workbook.set_cell(0, "A1", "new").unwrap();

        workbook.import_csv(&path).unwrap();

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "imported");
        assert!(workbook.undo().unwrap().is_none());
        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "imported");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn export_csv_does_not_clear_history() {
        let path = temp_csv_path("adapter-export-keeps-history");
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();
        workbook.set_cell(0, "A1", "2").unwrap();

        workbook.export_csv(&path).unwrap();
        let undo = workbook.undo().unwrap().expect("undo after CSV export");

        assert_eq!(workbook.get_cell(0, "A1").unwrap().raw, "1");
        assert_delta_contains(&undo, "A1", "1");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn import_csv_reports_missing_path_errors() {
        let path = temp_csv_path("adapter-missing-import");
        let mut workbook = WorkbookAdapter::default();

        let error = workbook.import_csv(&path).unwrap_err();

        assert!(error.contains("failed to import CSV from"));
        assert!(error.contains("failed to access CSV file"));
    }

    #[test]
    fn export_csv_reports_unwritable_path_errors() {
        let path = temp_csv_path("adapter-unwritable-export");
        fs::create_dir(&path).unwrap();
        let mut workbook = WorkbookAdapter::default();
        workbook.set_cell(0, "A1", "1").unwrap();

        let error = workbook.export_csv(&path).unwrap_err();

        assert!(error.contains("failed to export CSV to"));
        assert!(error.contains("failed to access CSV file"));
        let _ = fs::remove_dir(path);
    }

    #[test]
    fn open_workbook_reports_missing_file_errors() {
        let path = temp_path("missing");
        let mut workbook = WorkbookAdapter::default();

        let error = workbook.open_workbook(&path).unwrap_err();

        assert!(error.contains("failed to open native workbook"));
        assert!(error.contains("failed to read native workbook"));
    }

    #[test]
    fn rejects_oversized_viewports() {
        let workbook = WorkbookAdapter::default();
        let error = workbook
            .get_viewport(
                0,
                RectA1 {
                    start: "A1".to_string(),
                    end: "Z1000".to_string(),
                },
            )
            .unwrap_err();

        assert!(error.contains("viewport too large"));
    }
}
