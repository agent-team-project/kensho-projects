use std::collections::HashMap;

pub mod date;

pub const MAX_ROWS: u32 = 1_048_576;
pub const MAX_COLS: u32 = 16_384;
pub const DEFAULT_COL_WIDTH: f32 = 96.0;
pub const DEFAULT_ROW_HEIGHT: f32 = 24.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Boolean(bool),
    Error(ErrorValue),
    Blank,
}

impl Default for Value {
    fn default() -> Self {
        Self::Blank
    }
}

impl Value {
    pub fn as_error(&self) -> Option<ErrorValue> {
        match self {
            Self::Error(error) => Some(*error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorValue {
    Null,
    Div0,
    Value,
    Ref,
    Name,
    Num,
    Na,
    Circular,
}

impl ErrorValue {
    pub fn code(self) -> &'static str {
        match self {
            Self::Null => "#NULL!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::Na => "#N/A",
            Self::Circular => "#CIRC!",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "#NULL!" => Some(Self::Null),
            "#DIV/0!" => Some(Self::Div0),
            "#VALUE!" => Some(Self::Value),
            "#REF!" => Some(Self::Ref),
            "#NAME?" => Some(Self::Name),
            "#NUM!" => Some(Self::Num),
            "#N/A" => Some(Self::Na),
            "#CIRC!" => Some(Self::Circular),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coord {
    pub row: u32,
    pub col: u32,
}

impl Coord {
    pub fn to_a1(self) -> String {
        format!("{}{}", col_to_letters(self.col), self.row + 1)
    }

    pub fn from_a1(s: &str) -> Result<Coord, crate::syntax::ParseError> {
        parse_coord(s).ok_or_else(|| {
            crate::syntax::ParseError::new(
                crate::syntax::ParseErrorKind::InvalidReference,
                0,
                format!("invalid A1 coordinate: {s}"),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellId {
    pub sheet: u16,
    pub coord: Coord,
}

impl CellId {
    pub fn new(sheet: u16, coord: Coord) -> Self {
        Self { sheet, coord }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub raw: String,
    pub ast: Option<crate::syntax::Expr>,
    pub cached: Value,
    pub format: CellFormat,
}

impl Cell {
    pub fn new(raw: impl Into<String>, ast: Option<crate::syntax::Expr>, cached: Value) -> Self {
        Self {
            raw: raw.into(),
            ast,
            cached,
            format: CellFormat::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub name: String,
    cells: HashMap<Coord, Cell>,
    pub col_widths: HashMap<u32, f32>,
    pub row_heights: HashMap<u32, f32>,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cells: HashMap::new(),
            col_widths: HashMap::new(),
            row_heights: HashMap::new(),
        }
    }

    pub fn get_cell(&self, coord: Coord) -> Option<&Cell> {
        self.cells.get(&coord)
    }

    pub fn get_cell_mut(&mut self, coord: Coord) -> Option<&mut Cell> {
        self.cells.get_mut(&coord)
    }

    pub fn set_cell(&mut self, coord: Coord, cell: Cell) {
        self.cells.insert(coord, cell);
    }

    pub fn remove_cell(&mut self, coord: Coord) -> Option<Cell> {
        self.cells.remove(&coord)
    }

    pub fn iter_cells(&self) -> impl Iterator<Item = (Coord, &Cell)> {
        self.cells.iter().map(|(coord, cell)| (*coord, cell))
    }

    pub(crate) fn replace_cells(&mut self, cells: HashMap<Coord, Cell>) {
        self.cells = cells;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub date_system: DateSystem,
}

impl Workbook {
    pub fn new() -> Self {
        Self {
            sheets: vec![Sheet::new("Sheet1")],
            date_system: DateSystem::Excel1900,
        }
    }

    pub fn sheet(&self, sheet: u16) -> Option<&Sheet> {
        self.sheets.get(sheet as usize)
    }

    pub fn sheet_mut(&mut self, sheet: u16) -> Option<&mut Sheet> {
        self.sheets.get_mut(sheet as usize)
    }
}

impl Default for Workbook {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateSystem {
    Excel1900,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NumberFormat {
    General,
    Fixed(u8),
    Percent(u8),
    Currency(u8),
    Date(DateFormat),
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DateFormat {
    Iso,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Default,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellFormat {
    pub number: NumberFormat,
    pub align: Align,
    pub bold: bool,
}

impl Default for CellFormat {
    fn default() -> Self {
        Self {
            number: NumberFormat::General,
            align: Align::Default,
            bold: false,
        }
    }
}

pub fn display(value: &Value, format: &CellFormat) -> String {
    match value {
        Value::Number(number) => display_number(*number, &format.number),
        Value::Text(text) => text.clone(),
        Value::Boolean(true) => "TRUE".to_string(),
        Value::Boolean(false) => "FALSE".to_string(),
        Value::Error(error) => error.code().to_string(),
        Value::Blank => String::new(),
    }
}

pub(crate) fn parse_coord(s: &str) -> Option<Coord> {
    let mut chars = s.chars().peekable();
    if chars.peek() == Some(&'$') {
        chars.next();
    }

    let mut col: u32 = 0;
    let mut saw_col = false;
    while let Some(ch) = chars.peek().copied() {
        if !ch.is_ascii_alphabetic() {
            break;
        }
        saw_col = true;
        col = col
            .checked_mul(26)?
            .checked_add((ch.to_ascii_uppercase() as u8 - b'A' + 1) as u32)?;
        chars.next();
    }

    if chars.peek() == Some(&'$') {
        chars.next();
    }

    let mut row_text = String::new();
    while let Some(ch) = chars.peek().copied() {
        if !ch.is_ascii_digit() {
            break;
        }
        row_text.push(ch);
        chars.next();
    }

    if !saw_col || row_text.is_empty() || chars.next().is_some() {
        return None;
    }

    let row_one_based: u32 = row_text.parse().ok()?;
    if row_one_based == 0 || row_one_based > MAX_ROWS || col == 0 || col > MAX_COLS {
        return None;
    }

    Some(Coord {
        row: row_one_based - 1,
        col: col - 1,
    })
}

pub(crate) fn col_to_letters(mut col: u32) -> String {
    let mut letters = Vec::new();
    col += 1;
    while col > 0 {
        let rem = ((col - 1) % 26) as u8;
        letters.push((b'A' + rem) as char);
        col = (col - 1) / 26;
    }
    letters.iter().rev().collect()
}

fn display_number(number: f64, format: &NumberFormat) -> String {
    match format {
        NumberFormat::General => display_general_number(number),
        NumberFormat::Fixed(dp) => format!("{number:.prec$}", prec = *dp as usize),
        NumberFormat::Percent(dp) => format!("{:.prec$}%", number * 100.0, prec = *dp as usize),
        NumberFormat::Currency(dp) => format!("${number:.prec$}", prec = *dp as usize),
        NumberFormat::Date(DateFormat::Iso) => {
            display_iso_date(number).unwrap_or_else(|| display_general_number(number))
        }
        NumberFormat::Text => format!("{number:?}"),
    }
}

fn display_general_number(number: f64) -> String {
    if number.fract() == 0.0 && number.is_finite() {
        format!("{number:.0}")
    } else {
        format!("{number}")
    }
}

fn display_iso_date(number: f64) -> Option<String> {
    if !number.is_finite() || number < 1.0 {
        return None;
    }

    let (year, month, day) = date::serial_to_ymd(number.floor(), DateSystem::Excel1900);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a1_conversion_round_trips() {
        let coord = Coord::from_a1("AA10").unwrap();
        assert_eq!(coord, Coord { row: 9, col: 26 });
        assert_eq!(coord.to_a1(), "AA10");
    }

    #[test]
    fn error_codes_are_canonical() {
        assert_eq!(ErrorValue::Div0.code(), "#DIV/0!");
        assert_eq!(ErrorValue::from_code("#CIRC!"), Some(ErrorValue::Circular));
    }
}
