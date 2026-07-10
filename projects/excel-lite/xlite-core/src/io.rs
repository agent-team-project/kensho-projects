use std::{collections::BTreeMap, error::Error, fmt, fs, io as std_io, path::Path};

use calamine::{open_workbook, CellErrorType, DataRef, Reader, Xlsx, XlsxError};
use serde::{Deserialize, Serialize};

use crate::{
    model::{
        col_to_letters, display, Align, Cell, CellFormat, Coord, DateFormat, DateSystem,
        ErrorValue, NumberFormat, Sheet, Value, Workbook, MAX_COLS, MAX_ROWS,
    },
    recalc::RecalcEngine,
    CellId,
};

const FORMAT_VERSION: u32 = 1;
const APP_NAME: &str = "excel-lite";
const DATE_SYSTEM_1900: &str = "1900";

pub fn save_native(wb: &Workbook, path: &Path) -> std_io::Result<()> {
    let native = NativeWorkbook::from_model(wb)?;
    let mut bytes = serde_json::to_vec_pretty(&native).map_err(json_to_io_error)?;
    bytes.push(b'\n');
    fs::write(path, bytes)
}

pub fn load_native(path: &Path) -> Result<Workbook, LoadError> {
    let bytes = fs::read(path).map_err(LoadError::Io)?;
    let native: NativeWorkbook = serde_json::from_slice(&bytes).map_err(LoadError::Json)?;
    native.into_model()
}

pub fn import_csv(path: &Path) -> Result<Workbook, CsvError> {
    let file = fs::File::open(path).map_err(CsvError::Io)?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(file);
    let mut engine = RecalcEngine::new(Workbook::new());

    for (row_index, record) in reader.records().enumerate() {
        let record = record.map_err(CsvError::Csv)?;
        if row_index >= MAX_ROWS as usize {
            return Err(CsvError::CellOutOfBounds {
                row: row_index + 1,
                column: 1,
            });
        }

        for (column_index, field) in record.iter().enumerate() {
            if column_index >= MAX_COLS as usize {
                return Err(CsvError::CellOutOfBounds {
                    row: row_index + 1,
                    column: column_index + 1,
                });
            }
            if field.is_empty() {
                continue;
            }

            let id = CellId {
                sheet: 0,
                coord: Coord {
                    row: row_index as u32,
                    col: column_index as u32,
                },
            };
            set_imported_csv_cell(&mut engine, id, field);
        }
    }

    engine.recalc_all();
    Ok(engine.workbook().clone())
}

pub fn import_xlsx(path: &Path) -> Result<Workbook, LoadError> {
    let mut source: Xlsx<_> = open_workbook(path).map_err(LoadError::Xlsx)?;
    let sheet_names = source.sheet_names();
    if sheet_names.is_empty() {
        return Err(LoadError::MissingSheets);
    }
    if sheet_names.len() > usize::from(u16::MAX) + 1 {
        return Err(LoadError::TooManySheets(sheet_names.len()));
    }

    let workbook = Workbook {
        sheets: sheet_names.iter().cloned().map(Sheet::new).collect(),
        date_system: DateSystem::Excel1900,
    };
    let mut engine = RecalcEngine::new(workbook);

    for (sheet_index, sheet_name) in sheet_names.iter().enumerate() {
        let mut reader = source
            .worksheet_cells_reader(sheet_name)
            .map_err(LoadError::Xlsx)?;
        while let Some(cell) = reader.next_cell_with_formula().map_err(LoadError::Xlsx)? {
            let coord = xlsx_coord(sheet_name, cell.pos)?;
            let id = CellId {
                sheet: sheet_index as u16,
                coord,
            };

            match cell.formula.filter(|formula| !formula.trim().is_empty()) {
                Some(formula) => {
                    let raw = xlsx_formula_raw(&formula);
                    engine.set_cell(id, &raw);
                }
                None => {
                    if let Some((raw, value)) =
                        xlsx_literal_from_data(sheet_name, coord, cell.value)?
                    {
                        set_imported_literal_cell(&mut engine, id, raw, value);
                    }
                }
            }
        }
    }

    engine.recalc_all();
    Ok(engine.workbook().clone())
}

pub fn export_csv(wb: &Workbook, path: &Path) -> Result<(), CsvError> {
    let file = fs::File::create(path).map_err(CsvError::Io)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    let Some(sheet) = wb.sheets.first() else {
        writer.flush().map_err(CsvError::Io)?;
        return Ok(());
    };
    let Some(max_coord) = csv_used_rectangle(sheet) else {
        writer.flush().map_err(CsvError::Io)?;
        return Ok(());
    };

    for row in 0..=max_coord.row {
        let mut record = Vec::with_capacity(max_coord.col as usize + 1);
        for col in 0..=max_coord.col {
            let value = sheet
                .get_cell(Coord { row, col })
                .map(|cell| display(&cell.cached, &cell.format))
                .unwrap_or_default();
            record.push(value);
        }
        writer.write_record(record).map_err(CsvError::Csv)?;
    }

    writer.flush().map_err(CsvError::Io)
}

#[derive(Debug)]
pub enum CsvError {
    Io(std_io::Error),
    Csv(csv::Error),
    CellOutOfBounds { row: usize, column: usize },
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "failed to access CSV file: {error}"),
            Self::Csv(error) => write!(f, "failed to process CSV data: {error}"),
            Self::CellOutOfBounds { row, column } => {
                write!(
                    f,
                    "CSV cell at row {row}, column {column} exceeds workbook grid limits"
                )
            }
        }
    }
}

impl Error for CsvError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Csv(error) => Some(error),
            Self::CellOutOfBounds { .. } => None,
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    Io(std_io::Error),
    Json(serde_json::Error),
    Xlsx(XlsxError),
    UnsupportedFormatVersion(u32),
    UnsupportedApp(String),
    UnsupportedDateSystem(String),
    MissingSheets,
    InvalidCellAddress(String),
    InvalidColumnKey(String),
    InvalidRowKey(String),
    InvalidCellPayload {
        address: String,
        reason: String,
    },
    InvalidXlsxCell {
        sheet: String,
        address: String,
        reason: String,
    },
    XlsxCellOutOfBounds {
        sheet: String,
        row: u64,
        column: u64,
    },
    InvalidCachedValue(String),
    InvalidFormat(String),
    TooManySheets(usize),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "failed to read native workbook: {error}"),
            Self::Json(error) => write!(f, "failed to parse native workbook JSON: {error}"),
            Self::Xlsx(error) => write!(f, "failed to import XLSX workbook: {error}"),
            Self::UnsupportedFormatVersion(version) => {
                write!(f, "unsupported native workbook format_version {version}")
            }
            Self::UnsupportedApp(app) => write!(f, "unsupported native workbook app {app:?}"),
            Self::UnsupportedDateSystem(date_system) => {
                write!(f, "unsupported native workbook date_system {date_system:?}")
            }
            Self::MissingSheets => write!(f, "native workbook must contain at least one sheet"),
            Self::InvalidCellAddress(address) => {
                write!(f, "invalid native workbook cell address {address:?}")
            }
            Self::InvalidColumnKey(column) => {
                write!(f, "invalid native workbook column key {column:?}")
            }
            Self::InvalidRowKey(row) => write!(f, "invalid native workbook row key {row:?}"),
            Self::InvalidCellPayload { address, reason } => {
                write!(f, "invalid native workbook cell {address:?}: {reason}")
            }
            Self::InvalidXlsxCell {
                sheet,
                address,
                reason,
            } => {
                write!(f, "invalid XLSX cell {sheet}!{address}: {reason}")
            }
            Self::XlsxCellOutOfBounds { sheet, row, column } => {
                write!(
                    f,
                    "XLSX cell at sheet {sheet:?}, row {row}, column {column} exceeds workbook grid limits"
                )
            }
            Self::InvalidCachedValue(reason) => {
                write!(f, "invalid native workbook cached value: {reason}")
            }
            Self::InvalidFormat(reason) => write!(f, "invalid native workbook format: {reason}"),
            Self::TooManySheets(count) => {
                write!(
                    f,
                    "native workbook has {count} sheets, exceeding u16 sheet ids"
                )
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Xlsx(error) => Some(error),
            _ => None,
        }
    }
}

fn set_imported_csv_cell(engine: &mut RecalcEngine, id: CellId, field: &str) {
    if parse_csv_number(field).is_some() {
        engine.set_cell(id, field);
        return;
    }

    let cell = Cell::new(field.to_string(), None, Value::Text(field.to_string()));
    if let Some(sheet) = engine.workbook_mut().sheet_mut(id.sheet) {
        sheet.set_cell(id.coord, cell);
    }
}

fn parse_csv_number(field: &str) -> Option<f64> {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return None;
    }

    match trimmed.parse::<f64>() {
        Ok(number) if number.is_finite() => Some(number),
        _ => None,
    }
}

fn set_imported_literal_cell(engine: &mut RecalcEngine, id: CellId, raw: String, value: Value) {
    let cell = Cell::new(raw, None, value);
    if let Some(sheet) = engine.workbook_mut().sheet_mut(id.sheet) {
        sheet.set_cell(id.coord, cell);
    }
}

fn xlsx_coord(sheet_name: &str, pos: (u32, u32)) -> Result<Coord, LoadError> {
    let (row, col) = pos;
    if row >= MAX_ROWS || col >= MAX_COLS {
        return Err(LoadError::XlsxCellOutOfBounds {
            sheet: sheet_name.to_string(),
            row: u64::from(row) + 1,
            column: u64::from(col) + 1,
        });
    }
    Ok(Coord { row, col })
}

fn xlsx_formula_raw(formula: &str) -> String {
    if formula.starts_with('=') {
        formula.to_string()
    } else {
        format!("={formula}")
    }
}

fn xlsx_literal_from_data(
    sheet_name: &str,
    coord: Coord,
    data: DataRef<'_>,
) -> Result<Option<(String, Value)>, LoadError> {
    match data {
        DataRef::Empty => Ok(None),
        DataRef::Int(value) => Ok(Some((value.to_string(), Value::Number(value as f64)))),
        DataRef::Float(value) => finite_xlsx_number(sheet_name, coord, value)
            .map(|number| Some((number.to_string(), Value::Number(number)))),
        DataRef::DateTime(value) => finite_xlsx_number(sheet_name, coord, value.as_f64())
            .map(|number| Some((number.to_string(), Value::Number(number)))),
        DataRef::String(value) => Ok(text_xlsx_literal(value)),
        DataRef::SharedString(value) => Ok(text_xlsx_literal(value.to_string())),
        DataRef::Bool(value) => Ok(Some((
            if value { "TRUE" } else { "FALSE" }.to_string(),
            Value::Boolean(value),
        ))),
        DataRef::DateTimeIso(value) | DataRef::DurationIso(value) => Ok(text_xlsx_literal(value)),
        DataRef::Error(error) => xlsx_error_value(sheet_name, coord, error).map(|value| {
            Some((
                value
                    .as_error()
                    .expect("xlsx error value")
                    .code()
                    .to_string(),
                value,
            ))
        }),
    }
}

fn text_xlsx_literal(value: String) -> Option<(String, Value)> {
    if value.is_empty() {
        None
    } else {
        Some((value.clone(), Value::Text(value)))
    }
}

fn finite_xlsx_number(sheet_name: &str, coord: Coord, value: f64) -> Result<f64, LoadError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(LoadError::InvalidXlsxCell {
            sheet: sheet_name.to_string(),
            address: coord.to_a1(),
            reason: "number value must be finite".to_string(),
        })
    }
}

fn xlsx_error_value(
    sheet_name: &str,
    coord: Coord,
    error: CellErrorType,
) -> Result<Value, LoadError> {
    let value = match error {
        CellErrorType::Div0 => ErrorValue::Div0,
        CellErrorType::NA => ErrorValue::Na,
        CellErrorType::Name => ErrorValue::Name,
        CellErrorType::Null => ErrorValue::Null,
        CellErrorType::Num => ErrorValue::Num,
        CellErrorType::Ref => ErrorValue::Ref,
        CellErrorType::Value => ErrorValue::Value,
        CellErrorType::GettingData => {
            return Err(LoadError::InvalidXlsxCell {
                sheet: sheet_name.to_string(),
                address: coord.to_a1(),
                reason: format!("unsupported error code {error}"),
            });
        }
    };
    Ok(Value::Error(value))
}

fn csv_used_rectangle(sheet: &Sheet) -> Option<Coord> {
    let mut max_coord: Option<Coord> = None;
    for (coord, cell) in sheet.iter_cells() {
        if display(&cell.cached, &cell.format).is_empty() {
            continue;
        }
        max_coord = Some(match max_coord {
            Some(max_coord) => Coord {
                row: max_coord.row.max(coord.row),
                col: max_coord.col.max(coord.col),
            },
            None => coord,
        });
    }
    max_coord
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeWorkbook {
    format_version: u32,
    app: String,
    date_system: String,
    #[serde(default)]
    sheets: Vec<NativeSheet>,
}

impl NativeWorkbook {
    fn from_model(workbook: &Workbook) -> std_io::Result<Self> {
        Ok(Self {
            format_version: FORMAT_VERSION,
            app: APP_NAME.to_string(),
            date_system: date_system_to_native(workbook.date_system).to_string(),
            sheets: workbook
                .sheets
                .iter()
                .map(NativeSheet::from_model)
                .collect::<std_io::Result<Vec<_>>>()?,
        })
    }

    fn into_model(self) -> Result<Workbook, LoadError> {
        if self.format_version != FORMAT_VERSION {
            return Err(LoadError::UnsupportedFormatVersion(self.format_version));
        }
        if self.app != APP_NAME {
            return Err(LoadError::UnsupportedApp(self.app));
        }
        let date_system = date_system_from_native(&self.date_system)?;
        if self.sheets.is_empty() {
            return Err(LoadError::MissingSheets);
        }
        if self.sheets.len() > usize::from(u16::MAX) + 1 {
            return Err(LoadError::TooManySheets(self.sheets.len()));
        }

        let mut workbook = Workbook {
            sheets: self
                .sheets
                .iter()
                .map(|sheet| Sheet::new(sheet.name.clone()))
                .collect(),
            date_system,
        };

        for (sheet_index, native_sheet) in self.sheets.iter().enumerate() {
            let sheet = workbook
                .sheets
                .get_mut(sheet_index)
                .expect("sheet skeleton created above");
            for (key, width) in &native_sheet.col_widths {
                let col = parse_column_key(key)?;
                sheet.col_widths.insert(col, *width);
            }
            for (key, height) in &native_sheet.row_heights {
                let row = parse_row_key(key)?;
                sheet.row_heights.insert(row, *height);
            }
        }

        let mut engine = RecalcEngine::new(workbook);
        for (sheet_index, native_sheet) in self.sheets.iter().enumerate() {
            for (address, native_cell) in &native_sheet.cells {
                let coord = parse_cell_key(address)?;
                if native_cell.raw.is_empty() {
                    return Err(LoadError::InvalidCellPayload {
                        address: address.clone(),
                        reason: "raw must not be empty".to_string(),
                    });
                }
                let _cached = native_cell.cached.to_model()?;
                let format = native_cell.format.to_model()?;
                let id = CellId {
                    sheet: sheet_index as u16,
                    coord,
                };
                engine.set_cell(id, &native_cell.raw);
                let loaded_cell = engine
                    .workbook_mut()
                    .sheet_mut(id.sheet)
                    .and_then(|sheet| sheet.get_cell_mut(coord))
                    .ok_or_else(|| LoadError::InvalidCellPayload {
                        address: address.clone(),
                        reason: "raw did not create a model cell".to_string(),
                    })?;
                loaded_cell.format = format;
            }
        }

        engine.recalc_all();
        Ok(engine.workbook().clone())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeSheet {
    name: String,
    cells: BTreeMap<String, NativeCell>,
    col_widths: BTreeMap<String, f32>,
    row_heights: BTreeMap<String, f32>,
}

impl NativeSheet {
    fn from_model(sheet: &Sheet) -> std_io::Result<Self> {
        let mut cells = BTreeMap::new();
        let mut model_cells: Vec<_> = sheet.iter_cells().collect();
        model_cells.sort_by_key(|(coord, _)| *coord);
        for (coord, cell) in model_cells {
            if cell.raw.is_empty() {
                continue;
            }
            cells.insert(coord.to_a1(), NativeCell::from_model(cell)?);
        }

        let mut col_widths = BTreeMap::new();
        let mut widths: Vec<_> = sheet.col_widths.iter().collect();
        widths.sort_by_key(|(col, _)| **col);
        for (col, width) in widths {
            validate_finite_f32(*width, "column width")?;
            col_widths.insert(col_to_letters(*col), *width);
        }

        let mut row_heights = BTreeMap::new();
        let mut heights: Vec<_> = sheet.row_heights.iter().collect();
        heights.sort_by_key(|(row, _)| **row);
        for (row, height) in heights {
            validate_finite_f32(*height, "row height")?;
            row_heights.insert((row + 1).to_string(), *height);
        }

        Ok(Self {
            name: sheet.name.clone(),
            cells,
            col_widths,
            row_heights,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeCell {
    raw: String,
    format: NativeCellFormat,
    cached: NativeValue,
}

impl NativeCell {
    fn from_model(cell: &crate::model::Cell) -> std_io::Result<Self> {
        Ok(Self {
            raw: cell.raw.clone(),
            format: NativeCellFormat::from_model(&cell.format),
            cached: NativeValue::from_model(&cell.cached)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeCellFormat {
    number: NativeNumberFormat,
    align: NativeAlign,
    bold: bool,
}

impl NativeCellFormat {
    fn from_model(format: &CellFormat) -> Self {
        Self {
            number: NativeNumberFormat::from_model(&format.number),
            align: NativeAlign::from_model(format.align),
            bold: format.bold,
        }
    }

    fn to_model(&self) -> Result<CellFormat, LoadError> {
        Ok(CellFormat {
            number: self.number.to_model()?,
            align: self.align.to_model(),
            bold: self.bold,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum NativeNumberFormat {
    General,
    Fixed { dp: u8 },
    Percent { dp: u8 },
    Currency { dp: u8 },
    Date { format: String },
    Text,
}

impl NativeNumberFormat {
    fn from_model(format: &NumberFormat) -> Self {
        match format {
            NumberFormat::General => Self::General,
            NumberFormat::Fixed(dp) => Self::Fixed { dp: *dp },
            NumberFormat::Percent(dp) => Self::Percent { dp: *dp },
            NumberFormat::Currency(dp) => Self::Currency { dp: *dp },
            NumberFormat::Date(DateFormat::Iso) => Self::Date {
                format: "iso".to_string(),
            },
            NumberFormat::Text => Self::Text,
        }
    }

    fn to_model(&self) -> Result<NumberFormat, LoadError> {
        match self {
            Self::General => Ok(NumberFormat::General),
            Self::Fixed { dp } => Ok(NumberFormat::Fixed(*dp)),
            Self::Percent { dp } => Ok(NumberFormat::Percent(*dp)),
            Self::Currency { dp } => Ok(NumberFormat::Currency(*dp)),
            Self::Date { format } if format == "iso" => Ok(NumberFormat::Date(DateFormat::Iso)),
            Self::Date { format } => Err(LoadError::InvalidFormat(format!(
                "unsupported date format {format:?}"
            ))),
            Self::Text => Ok(NumberFormat::Text),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum NativeAlign {
    Left,
    Center,
    Right,
    Default,
}

impl NativeAlign {
    fn from_model(align: Align) -> Self {
        match align {
            Align::Left => Self::Left,
            Align::Center => Self::Center,
            Align::Right => Self::Right,
            Align::Default => Self::Default,
        }
    }

    fn to_model(self) -> Align {
        match self {
            Self::Left => Align::Left,
            Self::Center => Align::Center,
            Self::Right => Align::Right,
            Self::Default => Align::Default,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum NativeValue {
    Blank,
    Number { value: f64 },
    Text { value: String },
    Boolean { value: bool },
    Error { code: String },
}

impl NativeValue {
    fn from_model(value: &Value) -> std_io::Result<Self> {
        match value {
            Value::Blank => Ok(Self::Blank),
            Value::Number(value) => {
                validate_finite_f64(*value, "number value")?;
                Ok(Self::Number { value: *value })
            }
            Value::Text(value) => Ok(Self::Text {
                value: value.clone(),
            }),
            Value::Boolean(value) => Ok(Self::Boolean { value: *value }),
            Value::Error(error) => Ok(Self::Error {
                code: error.code().to_string(),
            }),
        }
    }

    fn to_model(&self) -> Result<Value, LoadError> {
        match self {
            Self::Blank => Ok(Value::Blank),
            Self::Number { value } => {
                if value.is_finite() {
                    Ok(Value::Number(*value))
                } else {
                    Err(LoadError::InvalidCachedValue(
                        "number value must be finite".to_string(),
                    ))
                }
            }
            Self::Text { value } => Ok(Value::Text(value.clone())),
            Self::Boolean { value } => Ok(Value::Boolean(*value)),
            Self::Error { code } => {
                ErrorValue::from_code(code)
                    .map(Value::Error)
                    .ok_or_else(|| {
                        LoadError::InvalidCachedValue(format!("unsupported error code {code:?}"))
                    })
            }
        }
    }
}

fn date_system_to_native(date_system: DateSystem) -> &'static str {
    match date_system {
        DateSystem::Excel1900 => DATE_SYSTEM_1900,
    }
}

fn date_system_from_native(date_system: &str) -> Result<DateSystem, LoadError> {
    match date_system {
        DATE_SYSTEM_1900 => Ok(DateSystem::Excel1900),
        other => Err(LoadError::UnsupportedDateSystem(other.to_string())),
    }
}

fn parse_cell_key(key: &str) -> Result<Coord, LoadError> {
    if !is_canonical_a1_key(key) {
        return Err(LoadError::InvalidCellAddress(key.to_string()));
    }
    Coord::from_a1(key).map_err(|_| LoadError::InvalidCellAddress(key.to_string()))
}

fn parse_column_key(key: &str) -> Result<u32, LoadError> {
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(LoadError::InvalidColumnKey(key.to_string()));
    }
    let mut col: u32 = 0;
    for byte in key.bytes() {
        col = col
            .checked_mul(26)
            .and_then(|value| value.checked_add(u32::from(byte - b'A' + 1)))
            .ok_or_else(|| LoadError::InvalidColumnKey(key.to_string()))?;
    }
    if col == 0 || col > MAX_COLS {
        return Err(LoadError::InvalidColumnKey(key.to_string()));
    }
    Ok(col - 1)
}

fn parse_row_key(key: &str) -> Result<u32, LoadError> {
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_digit()) || key.starts_with('0') {
        return Err(LoadError::InvalidRowKey(key.to_string()));
    }
    let row: u32 = key
        .parse()
        .map_err(|_| LoadError::InvalidRowKey(key.to_string()))?;
    if row == 0 || row > MAX_ROWS {
        return Err(LoadError::InvalidRowKey(key.to_string()));
    }
    Ok(row - 1)
}

fn is_canonical_a1_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_uppercase() {
        index += 1;
    }
    if index == 0 || index == bytes.len() {
        return false;
    }
    let row = &key[index..];
    if row.starts_with('0') || !row.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    true
}

fn validate_finite_f32(value: f32, label: &str) -> std_io::Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(std_io::Error::new(
            std_io::ErrorKind::InvalidData,
            format!("{label} must be finite"),
        ))
    }
}

fn validate_finite_f64(value: f64, label: &str) -> std_io::Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(std_io::Error::new(
            std_io::ErrorKind::InvalidData,
            format!("{label} must be finite"),
        ))
    }
}

fn json_to_io_error(error: serde_json::Error) -> std_io::Error {
    std_io::Error::new(std_io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value as JsonValue};
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    use crate::model::{Cell, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT};

    static NEXT_PATH_ID: AtomicU64 = AtomicU64::new(0);

    fn id(addr: &str) -> CellId {
        CellId {
            sheet: 0,
            coord: Coord::from_a1(addr).unwrap(),
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "xlite-core-io-{name}-{}-{unique}.xlite",
            std::process::id()
        ))
    }

    fn temp_csv_path(name: &str) -> PathBuf {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "xlite-core-io-{name}-{}-{unique}.csv",
            std::process::id()
        ))
    }

    fn temp_xlsx_path(name: &str) -> PathBuf {
        let unique = NEXT_PATH_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "xlite-core-io-{name}-{}-{unique}.xlsx",
            std::process::id()
        ))
    }

    fn write_xlsx(path: &Path, sheets: &[(&str, &str)]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let sheet_overrides: String = (1..=sheets.len())
            .map(|index| {
                format!(
                    r#"<Override PartName="/xl/worksheets/sheet{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
                )
            })
            .collect();
        write_xlsx_part(
            &mut zip,
            options,
            "[Content_Types].xml",
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
{sheet_overrides}
</Types>"#
            ),
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

        let workbook_sheets: String = sheets
            .iter()
            .enumerate()
            .map(|(index, (name, _))| {
                let id = index + 1;
                format!(r#"<sheet name="{name}" sheetId="{id}" r:id="rId{id}"/>"#)
            })
            .collect();
        write_xlsx_part(
            &mut zip,
            options,
            "xl/workbook.xml",
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<workbookPr date1904="false"/>
<sheets>{workbook_sheets}</sheets>
</workbook>"#
            ),
        );

        let workbook_relationships: String = (1..=sheets.len())
            .map(|id| {
                format!(
                    r#"<Relationship Id="rId{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{id}.xml"/>"#
                )
            })
            .collect();
        write_xlsx_part(
            &mut zip,
            options,
            "xl/_rels/workbook.xml.rels",
            &format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
{workbook_relationships}
</Relationships>"#
            ),
        );

        for (index, (_, sheet_xml)) in sheets.iter().enumerate() {
            write_xlsx_part(
                &mut zip,
                options,
                &format!("xl/worksheets/sheet{}.xml", index + 1),
                sheet_xml,
            );
        }

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

    const XLSX_SMOKE_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1:I2"/>
<sheetData>
<row r="1">
<c r="A1"><v>2</v></c>
<c r="B1" t="inlineStr"><is><t>hello</t></is></c>
<c r="C1" t="b"><v>1</v></c>
<c r="D1" t="e"><v>#DIV/0!</v></c>
<c r="E1"><f>SUM(A1:A2)</f><v>999</v></c>
<c r="F1"><f>VLOOKUP("SKU-2",H1:I2,2,FALSE)</f><v>999</v></c>
<c r="H1" t="inlineStr"><is><t>SKU-1</t></is></c>
<c r="I1"><v>10</v></c>
</row>
<row r="2">
<c r="A2"><v>3</v></c>
<c r="H2" t="inlineStr"><is><t>SKU-2</t></is></c>
<c r="I2"><v>20</v></c>
</row>
</sheetData>
</worksheet>"#;

    const XLSX_SECOND_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1:A1"/>
<sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>second sheet</t></is></c></row>
</sheetData>
</worksheet>"#;

    const XLSX_OUT_OF_BOUNDS_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="XFE1:XFE1"/>
<sheetData>
<row r="1"><c r="XFE1"><v>1</v></c></row>
</sheetData>
</worksheet>"#;

    const XLSX_UNSUPPORTED_CELL_SHEET: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<dimension ref="A1:A1"/>
<sheetData>
<row r="1"><c r="A1" t="unsupported"><v>1</v></c></row>
</sheetData>
</worksheet>"#;

    fn read_csv_records(path: &Path) -> Vec<Vec<String>> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_path(path)
            .unwrap();
        reader
            .records()
            .map(|record| record.unwrap().iter().map(ToString::to_string).collect())
            .collect()
    }

    fn save_to_json(workbook: &Workbook, name: &str) -> JsonValue {
        let path = temp_path(name);
        save_native(workbook, &path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        serde_json::from_str(&text).unwrap()
    }

    fn write_json(name: &str, value: JsonValue) -> PathBuf {
        let path = temp_path(name);
        fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
        path
    }

    fn load_json(name: &str, value: JsonValue) -> Result<Workbook, LoadError> {
        let path = write_json(name, value);
        let result = load_native(&path);
        let _ = fs::remove_file(path);
        result
    }

    fn base_json() -> JsonValue {
        json!({
            "format_version": 1,
            "app": "excel-lite",
            "date_system": "1900",
            "sheets": [{
                "name": "Sheet1",
                "cells": {},
                "col_widths": {},
                "row_heights": {}
            }]
        })
    }

    #[test]
    fn import_csv_parses_numbers_and_preserves_literal_text_fields() {
        let path = temp_csv_path("import-values");
        fs::write(
            &path,
            "42,text,\"quoted \"\"value\"\"\",\"hello, csv\",\"line\nbreak\",,=SUM(A1:A1),TRUE\n",
        )
        .unwrap();

        let workbook = import_csv(&path).unwrap();
        let _ = fs::remove_file(path);
        let sheet = workbook.sheet(0).unwrap();

        assert_eq!(workbook.sheets.len(), 1);
        assert_eq!(sheet.name, "Sheet1");
        assert_eq!(
            sheet.get_cell(id("A1").coord).unwrap().cached,
            Value::Number(42.0)
        );
        assert_eq!(
            sheet.get_cell(id("B1").coord).unwrap().cached,
            Value::Text("text".to_string())
        );
        assert_eq!(
            sheet.get_cell(id("C1").coord).unwrap().cached,
            Value::Text("quoted \"value\"".to_string())
        );
        assert_eq!(
            sheet.get_cell(id("D1").coord).unwrap().cached,
            Value::Text("hello, csv".to_string())
        );
        assert_eq!(
            sheet.get_cell(id("E1").coord).unwrap().cached,
            Value::Text("line\nbreak".to_string())
        );
        assert!(sheet.get_cell(id("F1").coord).is_none());

        let formula_like = sheet.get_cell(id("G1").coord).unwrap();
        assert_eq!(formula_like.raw, "=SUM(A1:A1)");
        assert!(formula_like.ast.is_none());
        assert_eq!(formula_like.cached, Value::Text("=SUM(A1:A1)".to_string()));
        assert_eq!(
            sheet.get_cell(id("H1").coord).unwrap().cached,
            Value::Text("TRUE".to_string())
        );
    }

    #[test]
    fn import_xlsx_imports_values_formulas_and_recalculates() {
        let path = temp_xlsx_path("smoke");
        write_xlsx(
            &path,
            &[("Import", XLSX_SMOKE_SHEET), ("Second", XLSX_SECOND_SHEET)],
        );

        let workbook = import_xlsx(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(workbook.sheets.len(), 2);
        assert_eq!(workbook.sheets[0].name, "Import");
        assert_eq!(workbook.sheets[1].name, "Second");

        let sheet = workbook.sheet(0).unwrap();
        assert_eq!(
            sheet.get_cell(id("A1").coord).unwrap().cached,
            Value::Number(2.0)
        );
        assert_eq!(
            sheet.get_cell(id("A2").coord).unwrap().cached,
            Value::Number(3.0)
        );
        assert_eq!(
            sheet.get_cell(id("B1").coord).unwrap().cached,
            Value::Text("hello".to_string())
        );
        assert_eq!(
            sheet.get_cell(id("C1").coord).unwrap().cached,
            Value::Boolean(true)
        );
        assert_eq!(
            sheet.get_cell(id("D1").coord).unwrap().cached,
            Value::Error(ErrorValue::Div0)
        );
        assert!(sheet.get_cell(id("G1").coord).is_none());

        let sum = sheet.get_cell(id("E1").coord).unwrap();
        assert_eq!(sum.raw, "=SUM(A1:A2)");
        assert!(sum.ast.is_some());
        assert_eq!(sum.cached, Value::Number(5.0));

        let lookup = sheet.get_cell(id("F1").coord).unwrap();
        assert_eq!(lookup.raw, r#"=VLOOKUP("SKU-2",H1:I2,2,FALSE)"#);
        assert!(lookup.ast.is_some());
        assert_eq!(lookup.cached, Value::Number(20.0));

        assert_eq!(
            workbook
                .sheet(1)
                .unwrap()
                .get_cell(id("A1").coord)
                .unwrap()
                .cached,
            Value::Text("second sheet".to_string())
        );

        let mut engine = RecalcEngine::new(workbook);
        engine.recalc_all();
        assert_eq!(engine.cell_value(id("E1")), Value::Number(5.0));
        assert_eq!(engine.cell_value(id("F1")), Value::Number(20.0));

        engine.set_cell(id("A1"), "10");
        assert_eq!(engine.cell_value(id("E1")), Value::Number(13.0));
    }

    #[test]
    fn import_xlsx_reports_missing_and_malformed_files() {
        let missing = temp_xlsx_path("missing");
        let _ = fs::remove_file(&missing);
        assert!(matches!(import_xlsx(&missing), Err(LoadError::Xlsx(_))));

        let malformed = temp_xlsx_path("malformed");
        fs::write(&malformed, b"not a zip file").unwrap();
        let result = import_xlsx(&malformed);
        let _ = fs::remove_file(malformed);

        assert!(matches!(result, Err(LoadError::Xlsx(_))));
    }

    #[test]
    fn import_xlsx_reports_unsupported_or_out_of_bounds_cells() {
        let unsupported = temp_xlsx_path("unsupported-cell");
        write_xlsx(
            &unsupported,
            &[("Unsupported", XLSX_UNSUPPORTED_CELL_SHEET)],
        );
        let result = import_xlsx(&unsupported);
        let _ = fs::remove_file(unsupported);
        assert!(matches!(result, Err(LoadError::Xlsx(_))));

        let out_of_bounds = temp_xlsx_path("out-of-bounds");
        write_xlsx(&out_of_bounds, &[("OutOfBounds", XLSX_OUT_OF_BOUNDS_SHEET)]);
        let result = import_xlsx(&out_of_bounds);
        let _ = fs::remove_file(out_of_bounds);
        assert!(matches!(
            result,
            Err(LoadError::XlsxCellOutOfBounds { .. }) | Err(LoadError::Xlsx(_))
        ));
    }

    #[test]
    fn export_csv_writes_formula_display_values_not_formula_source() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "2");
        engine.set_cell(id("A2"), "3");
        engine.set_cell(id("B1"), "=SUM(A1:A2)");

        let path = temp_csv_path("export-formula-display");
        export_csv(engine.workbook(), &path).unwrap();
        let records = read_csv_records(&path);
        let _ = fs::remove_file(path);

        assert_eq!(
            records,
            vec![
                vec!["2".to_string(), "5".to_string()],
                vec!["3".to_string(), String::new()],
            ]
        );
    }

    #[test]
    fn export_csv_uses_cell_display_formatting() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "12.345");
        engine.set_cell(id("B1"), "0.125");
        engine.set_cell(id("C1"), "7");

        let sheet = engine.workbook_mut().sheet_mut(0).unwrap();
        sheet.get_cell_mut(id("A1").coord).unwrap().format = CellFormat {
            number: NumberFormat::Fixed(2),
            align: Align::Default,
            bold: false,
        };
        sheet.get_cell_mut(id("B1").coord).unwrap().format = CellFormat {
            number: NumberFormat::Percent(1),
            align: Align::Default,
            bold: false,
        };
        sheet.get_cell_mut(id("C1").coord).unwrap().format = CellFormat {
            number: NumberFormat::Currency(2),
            align: Align::Default,
            bold: false,
        };

        let path = temp_csv_path("export-formatting");
        export_csv(engine.workbook(), &path).unwrap();
        let records = read_csv_records(&path);
        let _ = fs::remove_file(path);

        assert_eq!(
            records,
            vec![vec![
                "12.35".to_string(),
                "12.5%".to_string(),
                "$7.00".to_string(),
            ]]
        );
    }

    #[test]
    fn csv_export_then_import_preserves_displayed_values_for_mixed_sheet() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "2");
        engine.set_cell(id("B1"), "hello, csv");
        engine.set_cell(id("C1"), "=A1+3");
        engine.set_cell(id("A2"), "0.125");
        engine.set_cell(id("B2"), "line\nbreak");
        engine.set_cell(id("C2"), "=A1*2");
        engine
            .workbook_mut()
            .sheet_mut(0)
            .unwrap()
            .get_cell_mut(id("A2").coord)
            .unwrap()
            .format = CellFormat {
            number: NumberFormat::Percent(1),
            align: Align::Default,
            bold: false,
        };

        let path = temp_csv_path("mixed-round-trip");
        export_csv(engine.workbook(), &path).unwrap();
        let imported = import_csv(&path).unwrap();
        let _ = fs::remove_file(path);
        let sheet = imported.sheet(0).unwrap();

        let displayed = |addr: &str| {
            let cell = sheet.get_cell(id(addr).coord).unwrap();
            display(&cell.cached, &cell.format)
        };
        assert_eq!(displayed("A1"), "2");
        assert_eq!(displayed("B1"), "hello, csv");
        assert_eq!(displayed("C1"), "5");
        assert_eq!(displayed("A2"), "12.5%");
        assert_eq!(displayed("B2"), "line\nbreak");
        assert_eq!(displayed("C2"), "4");
    }

    #[test]
    fn export_csv_empty_workbook_writes_empty_file() {
        let workbook = Workbook::new();
        let path = temp_csv_path("empty-export");

        export_csv(&workbook, &path).unwrap();
        let bytes = fs::read(&path).unwrap();
        let _ = fs::remove_file(path);

        assert!(bytes.is_empty());
    }

    #[test]
    fn import_csv_reports_missing_and_malformed_files() {
        let missing = temp_csv_path("missing");
        let _ = fs::remove_file(&missing);
        assert!(matches!(import_csv(&missing), Err(CsvError::Io(_))));

        let malformed = temp_csv_path("malformed");
        fs::write(&malformed, b"\xFF").unwrap();
        let result = import_csv(&malformed);
        let _ = fs::remove_file(malformed);

        assert!(matches!(result, Err(CsvError::Csv(_))));
    }

    #[test]
    fn save_native_writes_top_level_fields_and_a1_cells() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("B2"), "42");
        engine
            .workbook_mut()
            .sheet_mut(0)
            .unwrap()
            .col_widths
            .insert(Coord::from_a1("B1").unwrap().col, DEFAULT_COL_WIDTH + 12.0);
        engine
            .workbook_mut()
            .sheet_mut(0)
            .unwrap()
            .row_heights
            .insert(Coord::from_a1("A2").unwrap().row, DEFAULT_ROW_HEIGHT + 6.0);

        let json = save_to_json(engine.workbook(), "shape");

        assert_eq!(json["format_version"], 1);
        assert_eq!(json["app"], "excel-lite");
        assert_eq!(json["date_system"], "1900");
        assert_eq!(json["sheets"][0]["name"], "Sheet1");
        assert_eq!(json["sheets"][0]["cells"]["B2"]["raw"], "42");
        assert_eq!(
            json["sheets"][0]["cells"]["B2"]["cached"],
            json!({"type": "number", "value": 42.0})
        );
        assert_eq!(json["sheets"][0]["col_widths"]["B"], 108.0);
        assert_eq!(json["sheets"][0]["row_heights"]["2"], 30.0);
        assert!(json["sheets"][0]["cells"].get("A1").is_none());
    }

    #[test]
    fn round_trip_preserves_native_model_data() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "2");
        engine.set_cell(id("A2"), "3");
        engine.set_cell(id("A3"), "hello");
        engine.set_cell(id("A4"), "TRUE");
        engine.set_cell(id("A5"), "=1/0");
        engine.set_cell(id("B1"), "=SUM(A1:A2)");
        engine.set_cell(id("C1"), "left alone");

        let sheet = engine.workbook_mut().sheet_mut(0).unwrap();
        sheet.col_widths.insert(0, 120.0);
        sheet.col_widths.insert(26, 220.0);
        sheet.row_heights.insert(0, 33.0);
        sheet.row_heights.insert(9, 44.0);
        sheet.get_cell_mut(id("B1").coord).unwrap().format = CellFormat {
            number: NumberFormat::Currency(2),
            align: Align::Right,
            bold: true,
        };
        sheet.get_cell_mut(id("A3").coord).unwrap().format = CellFormat {
            number: NumberFormat::Text,
            align: Align::Left,
            bold: false,
        };
        sheet.set_cell(id("D1").coord, Cell::new("", None, Value::Blank));

        let path = temp_path("round-trip");
        save_native(engine.workbook(), &path).unwrap();
        let loaded = load_native(&path).unwrap();
        let _ = fs::remove_file(path);

        assert_eq!(loaded.date_system, DateSystem::Excel1900);
        assert_eq!(loaded.sheets.len(), 1);
        let loaded_sheet = loaded.sheet(0).unwrap();
        assert_eq!(loaded_sheet.name, "Sheet1");
        assert!(loaded_sheet.get_cell(id("D1").coord).is_none());
        assert_eq!(loaded_sheet.get_cell(id("A1").coord).unwrap().raw, "2");
        assert_eq!(
            loaded_sheet.get_cell(id("A1").coord).unwrap().cached,
            Value::Number(2.0)
        );
        assert_eq!(
            loaded_sheet.get_cell(id("A3").coord).unwrap().cached,
            Value::Text("hello".to_string())
        );
        assert_eq!(
            loaded_sheet.get_cell(id("A4").coord).unwrap().cached,
            Value::Boolean(true)
        );
        assert_eq!(
            loaded_sheet.get_cell(id("A5").coord).unwrap().cached,
            Value::Error(ErrorValue::Div0)
        );
        let formula = loaded_sheet.get_cell(id("B1").coord).unwrap();
        assert_eq!(formula.raw, "=SUM(A1:A2)");
        assert!(formula.ast.is_some());
        assert_eq!(formula.cached, Value::Number(5.0));
        assert_eq!(
            formula.format,
            CellFormat {
                number: NumberFormat::Currency(2),
                align: Align::Right,
                bold: true,
            }
        );
        assert_eq!(
            loaded_sheet.get_cell(id("A3").coord).unwrap().format,
            CellFormat {
                number: NumberFormat::Text,
                align: Align::Left,
                bold: false,
            }
        );
        assert_eq!(loaded_sheet.col_widths.get(&0), Some(&120.0));
        assert_eq!(loaded_sheet.col_widths.get(&26), Some(&220.0));
        assert_eq!(loaded_sheet.row_heights.get(&0), Some(&33.0));
        assert_eq!(loaded_sheet.row_heights.get(&9), Some(&44.0));
    }

    #[test]
    fn load_native_recomputes_stale_formula_cached_values() {
        let mut json = base_json();
        json["sheets"][0]["cells"] = json!({
            "A1": {
                "raw": "2",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "number", "value": 2.0}
            },
            "B1": {
                "raw": "=A1+1",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "number", "value": 999.0}
            }
        });

        let workbook = load_json("stale-formula", json).unwrap();

        assert_eq!(
            workbook
                .sheet(0)
                .unwrap()
                .get_cell(id("B1").coord)
                .unwrap()
                .cached,
            Value::Number(3.0)
        );
    }

    #[test]
    fn load_native_rebuilds_literal_cached_values_from_raw() {
        let mut json = base_json();
        json["sheets"][0]["cells"] = json!({
            "A1": {
                "raw": "42",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "text", "value": "stale"}
            },
            "A2": {
                "raw": "FALSE",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "number", "value": 99.0}
            },
            "A3": {
                "raw": " text ",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "blank"}
            }
        });

        let workbook = load_json("literal-rebuild", json).unwrap();
        let sheet = workbook.sheet(0).unwrap();

        assert_eq!(
            sheet.get_cell(id("A1").coord).unwrap().cached,
            Value::Number(42.0)
        );
        assert_eq!(
            sheet.get_cell(id("A2").coord).unwrap().cached,
            Value::Boolean(false)
        );
        assert_eq!(
            sheet.get_cell(id("A3").coord).unwrap().cached,
            Value::Text(" text ".to_string())
        );
    }

    #[test]
    fn load_native_rejects_unsupported_headers_and_invalid_keys() {
        let mut json = base_json();
        json["format_version"] = json!(2);
        assert!(matches!(
            load_json("bad-version", json),
            Err(LoadError::UnsupportedFormatVersion(2))
        ));

        let mut json = base_json();
        json["app"] = json!("other");
        assert!(matches!(
            load_json("bad-app", json),
            Err(LoadError::UnsupportedApp(app)) if app == "other"
        ));

        let mut json = base_json();
        json["date_system"] = json!("1904");
        assert!(matches!(
            load_json("bad-date-system", json),
            Err(LoadError::UnsupportedDateSystem(date_system)) if date_system == "1904"
        ));

        let mut json = base_json();
        json["sheets"] = json!([]);
        assert!(matches!(
            load_json("missing-sheets", json),
            Err(LoadError::MissingSheets)
        ));

        let mut json = base_json();
        json.as_object_mut().unwrap().remove("sheets");
        assert!(matches!(
            load_json("omitted-sheets", json),
            Err(LoadError::MissingSheets)
        ));

        let mut json = base_json();
        json["sheets"][0]["cells"] = json!({
            "a1": {
                "raw": "1",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "number", "value": 1.0}
            }
        });
        assert!(matches!(
            load_json("bad-cell-key", json),
            Err(LoadError::InvalidCellAddress(address)) if address == "a1"
        ));

        let mut json = base_json();
        json["sheets"][0]["col_widths"] = json!({"A1": 10.0});
        assert!(matches!(
            load_json("bad-column-key", json),
            Err(LoadError::InvalidColumnKey(column)) if column == "A1"
        ));

        let mut json = base_json();
        json["sheets"][0]["row_heights"] = json!({"01": 10.0});
        assert!(matches!(
            load_json("bad-row-key", json),
            Err(LoadError::InvalidRowKey(row)) if row == "01"
        ));
    }

    #[test]
    fn load_native_rejects_malformed_cached_error_and_format_payloads() {
        let mut json = base_json();
        json["sheets"][0]["cells"] = json!({
            "A1": {
                "raw": "=1/0",
                "format": {"number": {"type": "general"}, "align": "default", "bold": false},
                "cached": {"type": "error", "code": "#BOGUS!"}
            }
        });
        assert!(matches!(
            load_json("bad-cached-error", json),
            Err(LoadError::InvalidCachedValue(reason))
                if reason.contains("unsupported error code")
        ));

        let mut json = base_json();
        json["sheets"][0]["cells"] = json!({
            "A1": {
                "raw": "1",
                "format": {"number": {"type": "date", "format": "long"}, "align": "default", "bold": false},
                "cached": {"type": "number", "value": 1.0}
            }
        });
        assert!(matches!(
            load_json("bad-format", json),
            Err(LoadError::InvalidFormat(reason)) if reason.contains("long")
        ));
    }

    #[test]
    fn loaded_workbook_can_be_edited_by_recalc_engine() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "1");
        engine.set_cell(id("B1"), "=A1+1");

        let path = temp_path("post-load-edit");
        save_native(engine.workbook(), &path).unwrap();
        let loaded = load_native(&path).unwrap();
        let _ = fs::remove_file(path);

        let mut engine = RecalcEngine::new(loaded);
        engine.set_cell(id("A1"), "5");

        assert_eq!(engine.cell_value(id("B1")), Value::Number(6.0));
    }
}
