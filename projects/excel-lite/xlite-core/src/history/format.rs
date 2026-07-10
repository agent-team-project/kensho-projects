use std::{error::Error, fmt};

use crate::{
    model::{display, Cell, CellFormat, CellId, Coord, Sheet, Value, MAX_COLS, MAX_ROWS},
    recalc::{RecalcDelta, RecalcEngine},
};

use super::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatCommandError {
    SheetNotFound(u16),
    RangeOutOfBounds { start: Coord, end: Coord },
}

impl fmt::Display for FormatCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SheetNotFound(sheet) => write!(f, "unknown sheet: {sheet}"),
            Self::RangeOutOfBounds { start, end } => write!(
                f,
                "format range {}:{} exceeds grid bounds",
                start.to_a1(),
                end.to_a1()
            ),
        }
    }
}

impl Error for FormatCommandError {}

#[derive(Clone, Debug)]
pub struct SetFormatCommand {
    sheet: u16,
    range: CellRange,
    format: CellFormat,
    before: Option<Vec<CellSnapshot>>,
    after: Option<Vec<CellSnapshot>>,
}

impl SetFormatCommand {
    pub fn new(
        sheet: u16,
        start: Coord,
        end: Coord,
        format: CellFormat,
    ) -> Result<Self, FormatCommandError> {
        Ok(Self {
            sheet,
            range: CellRange::new(start, end)?,
            format,
            before: None,
            after: None,
        })
    }

    pub fn for_engine(
        engine: &RecalcEngine,
        sheet: u16,
        start: Coord,
        end: Coord,
        format: CellFormat,
    ) -> Result<Self, FormatCommandError> {
        let command = Self::new(sheet, start, end, format)?;
        command.validate(engine)?;
        Ok(command)
    }

    pub fn validate(&self, engine: &RecalcEngine) -> Result<(), FormatCommandError> {
        engine
            .workbook()
            .sheet(self.sheet)
            .ok_or(FormatCommandError::SheetNotFound(self.sheet))?;
        Ok(())
    }

    pub fn sheet(&self) -> u16 {
        self.sheet
    }

    pub fn start(&self) -> Coord {
        self.range.start
    }

    pub fn end(&self) -> Coord {
        self.range.end
    }

    pub fn format(&self) -> &CellFormat {
        &self.format
    }
}

impl Command for SetFormatCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        if self.validate(engine).is_err() {
            self.before = None;
            self.after = None;
            return RecalcDelta::default();
        }

        let before = snapshot_range(
            engine
                .workbook()
                .sheet(self.sheet)
                .expect("validated sheet"),
            self.range,
        );

        {
            let sheet = engine
                .workbook_mut()
                .sheet_mut(self.sheet)
                .expect("validated sheet");
            apply_format(sheet, self.range, &self.format);
        }

        let after = snapshot_range(
            engine
                .workbook()
                .sheet(self.sheet)
                .expect("validated sheet"),
            self.range,
        );
        let delta = cell_snapshot_delta(self.sheet, &before, &after);

        self.before = Some(before);
        self.after = Some(after);
        delta
    }

    fn invert(&self) -> Box<dyn Command> {
        Box::new(RestoreCellsCommand {
            sheet: self.sheet,
            snapshot: self.before.clone(),
            inverse_snapshot: self.after.clone(),
            label: self.label(),
        })
    }

    fn label(&self) -> String {
        "Set format".to_string()
    }
}

#[derive(Clone, Debug)]
struct RestoreCellsCommand {
    sheet: u16,
    snapshot: Option<Vec<CellSnapshot>>,
    inverse_snapshot: Option<Vec<CellSnapshot>>,
    label: String,
}

impl Command for RestoreCellsCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        let Some(snapshot) = self.snapshot.clone() else {
            return RecalcDelta::default();
        };
        let Some(sheet) = engine.workbook().sheet(self.sheet) else {
            return RecalcDelta::default();
        };

        let before = snapshot_current_cells(sheet, &snapshot);
        {
            let Some(sheet) = engine.workbook_mut().sheet_mut(self.sheet) else {
                return RecalcDelta::default();
            };
            restore_cells(sheet, &snapshot);
        }

        cell_snapshot_delta(self.sheet, &before, &snapshot)
    }

    fn invert(&self) -> Box<dyn Command> {
        Box::new(Self {
            sheet: self.sheet,
            snapshot: self.inverse_snapshot.clone(),
            inverse_snapshot: self.snapshot.clone(),
            label: self.label.clone(),
        })
    }

    fn label(&self) -> String {
        self.label.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CellRange {
    start: Coord,
    end: Coord,
}

impl CellRange {
    fn new(start: Coord, end: Coord) -> Result<Self, FormatCommandError> {
        if !coord_in_bounds(start) || !coord_in_bounds(end) {
            return Err(FormatCommandError::RangeOutOfBounds { start, end });
        }

        Ok(Self {
            start: Coord {
                row: start.row.min(end.row),
                col: start.col.min(end.col),
            },
            end: Coord {
                row: start.row.max(end.row),
                col: start.col.max(end.col),
            },
        })
    }

    fn coords(self) -> impl Iterator<Item = Coord> {
        (self.start.row..=self.end.row)
            .flat_map(move |row| (self.start.col..=self.end.col).map(move |col| Coord { row, col }))
    }
}

#[derive(Clone, Debug)]
struct CellSnapshot {
    coord: Coord,
    cell: Option<Cell>,
}

fn coord_in_bounds(coord: Coord) -> bool {
    coord.row < MAX_ROWS && coord.col < MAX_COLS
}

fn snapshot_range(sheet: &Sheet, range: CellRange) -> Vec<CellSnapshot> {
    range
        .coords()
        .map(|coord| CellSnapshot {
            coord,
            cell: sheet.get_cell(coord).cloned(),
        })
        .collect()
}

fn snapshot_current_cells(sheet: &Sheet, target: &[CellSnapshot]) -> Vec<CellSnapshot> {
    target
        .iter()
        .map(|snapshot| CellSnapshot {
            coord: snapshot.coord,
            cell: sheet.get_cell(snapshot.coord).cloned(),
        })
        .collect()
}

fn apply_format(sheet: &mut Sheet, range: CellRange, format: &CellFormat) {
    for coord in range.coords() {
        if let Some(cell) = sheet.get_cell_mut(coord) {
            cell.format = format.clone();
        } else if format != &CellFormat::default() {
            sheet.set_cell(coord, blank_cell(format.clone()));
        }
    }
}

fn restore_cells(sheet: &mut Sheet, snapshots: &[CellSnapshot]) {
    for snapshot in snapshots {
        if let Some(cell) = &snapshot.cell {
            sheet.set_cell(snapshot.coord, cell.clone());
        } else {
            sheet.remove_cell(snapshot.coord);
        }
    }
}

fn blank_cell(format: CellFormat) -> Cell {
    let mut cell = Cell::new("", None, Value::Blank);
    cell.format = format;
    cell
}

fn cell_snapshot_delta(sheet: u16, before: &[CellSnapshot], after: &[CellSnapshot]) -> RecalcDelta {
    let changed = before.iter().zip(after).filter_map(|(before, after)| {
        debug_assert_eq!(before.coord, after.coord);
        (before.cell != after.cell || snapshot_display(before) != snapshot_display(after)).then(
            || {
                (
                    CellId {
                        sheet,
                        coord: after.coord,
                    },
                    snapshot_value(after),
                )
            },
        )
    });

    RecalcDelta {
        changed: changed.collect(),
        circular: Vec::new(),
    }
}

fn snapshot_display(snapshot: &CellSnapshot) -> String {
    snapshot
        .cell
        .as_ref()
        .map(|cell| display(&cell.cached, &cell.format))
        .unwrap_or_else(|| display(&Value::Blank, &CellFormat::default()))
}

fn snapshot_value(snapshot: &CellSnapshot) -> Value {
    snapshot
        .cell
        .as_ref()
        .map(|cell| cell.cached.clone())
        .unwrap_or(Value::Blank)
}
