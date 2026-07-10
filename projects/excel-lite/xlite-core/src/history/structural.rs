use std::{collections::HashMap, error::Error, fmt};

use crate::{
    model::{Cell, Coord, Sheet, MAX_COLS, MAX_ROWS},
    recalc::{parse_cell_source, RecalcDelta, RecalcEngine},
};

use super::{
    refshift::{rewrite_formula, Axis as RefShiftAxis, EditKind as RefShiftEditKind, RefShift},
    Command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralAxis {
    Row,
    Col,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralEditKind {
    Insert,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralEditError {
    ZeroCount,
    SheetNotFound(u16),
    OutOfBounds {
        axis: StructuralAxis,
        kind: StructuralEditKind,
        at: u32,
        count: u32,
    },
    OccupiedCellsWouldLeaveGrid {
        axis: StructuralAxis,
        first_index: u32,
    },
}

impl fmt::Display for StructuralEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCount => write!(f, "structural edit count must be greater than zero"),
            Self::SheetNotFound(sheet) => write!(f, "unknown sheet: {sheet}"),
            Self::OutOfBounds {
                axis,
                kind,
                at,
                count,
            } => write!(
                f,
                "{kind:?} {axis:?} edit at {at} for {count} exceeds grid bounds"
            ),
            Self::OccupiedCellsWouldLeaveGrid { axis, first_index } => write!(
                f,
                "insert would push occupied {axis:?} index {first_index} outside the grid"
            ),
        }
    }
}

impl Error for StructuralEditError {}

#[derive(Clone, Debug)]
pub struct StructuralEditCommand {
    edit: StructuralEdit,
    before: Option<Sheet>,
    after: Option<Sheet>,
}

impl StructuralEditCommand {
    pub fn new(
        sheet: u16,
        axis: StructuralAxis,
        kind: StructuralEditKind,
        at: u32,
        count: u32,
    ) -> Result<Self, StructuralEditError> {
        validate_static(axis, kind, at, count)?;
        Ok(Self {
            edit: StructuralEdit {
                sheet,
                axis,
                kind,
                at,
                count,
            },
            before: None,
            after: None,
        })
    }

    pub fn for_engine(
        engine: &RecalcEngine,
        sheet: u16,
        axis: StructuralAxis,
        kind: StructuralEditKind,
        at: u32,
        count: u32,
    ) -> Result<Self, StructuralEditError> {
        let command = Self::new(sheet, axis, kind, at, count)?;
        command.validate(engine)?;
        Ok(command)
    }

    pub fn insert_rows(sheet: u16, at: u32, count: u32) -> Result<Self, StructuralEditError> {
        Self::new(
            sheet,
            StructuralAxis::Row,
            StructuralEditKind::Insert,
            at,
            count,
        )
    }

    pub fn delete_rows(sheet: u16, at: u32, count: u32) -> Result<Self, StructuralEditError> {
        Self::new(
            sheet,
            StructuralAxis::Row,
            StructuralEditKind::Delete,
            at,
            count,
        )
    }

    pub fn insert_cols(sheet: u16, at: u32, count: u32) -> Result<Self, StructuralEditError> {
        Self::new(
            sheet,
            StructuralAxis::Col,
            StructuralEditKind::Insert,
            at,
            count,
        )
    }

    pub fn delete_cols(sheet: u16, at: u32, count: u32) -> Result<Self, StructuralEditError> {
        Self::new(
            sheet,
            StructuralAxis::Col,
            StructuralEditKind::Delete,
            at,
            count,
        )
    }

    pub fn validate(&self, engine: &RecalcEngine) -> Result<(), StructuralEditError> {
        validate_static(
            self.edit.axis,
            self.edit.kind,
            self.edit.at,
            self.edit.count,
        )?;

        let sheet = engine
            .workbook()
            .sheet(self.edit.sheet)
            .ok_or(StructuralEditError::SheetNotFound(self.edit.sheet))?;

        if self.edit.kind == StructuralEditKind::Insert {
            if let Some(first_index) = first_occupied_index_pushed_out(sheet, self.edit) {
                return Err(StructuralEditError::OccupiedCellsWouldLeaveGrid {
                    axis: self.edit.axis,
                    first_index,
                });
            }
        }

        Ok(())
    }

    pub fn sheet(&self) -> u16 {
        self.edit.sheet
    }

    pub fn axis(&self) -> StructuralAxis {
        self.edit.axis
    }

    pub fn kind(&self) -> StructuralEditKind {
        self.edit.kind
    }

    pub fn at(&self) -> u32 {
        self.edit.at
    }

    pub fn count(&self) -> u32 {
        self.edit.count
    }
}

impl Command for StructuralEditCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        if self.validate(engine).is_err() {
            self.before = None;
            self.after = None;
            return RecalcDelta::default();
        }

        let before = engine
            .workbook()
            .sheet(self.edit.sheet)
            .expect("validated sheet")
            .clone();
        let mut edited = before.clone();
        apply_structural_edit(&mut edited, self.edit);

        let delta = engine.replace_sheet_snapshot(self.edit.sheet, edited);
        self.before = Some(before);
        self.after = engine.workbook().sheet(self.edit.sheet).cloned();
        delta
    }

    fn invert(&self) -> Box<dyn Command> {
        Box::new(RestoreSheetCommand {
            sheet: self.edit.sheet,
            snapshot: self.before.clone(),
            inverse_snapshot: self.after.clone(),
            label: self.label(),
        })
    }

    fn label(&self) -> String {
        label_for_edit(self.edit)
    }
}

#[derive(Clone, Debug)]
struct RestoreSheetCommand {
    sheet: u16,
    snapshot: Option<Sheet>,
    inverse_snapshot: Option<Sheet>,
    label: String,
}

impl Command for RestoreSheetCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        let Some(snapshot) = self.snapshot.clone() else {
            return RecalcDelta::default();
        };

        engine.replace_sheet_snapshot(self.sheet, snapshot)
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
struct StructuralEdit {
    sheet: u16,
    axis: StructuralAxis,
    kind: StructuralEditKind,
    at: u32,
    count: u32,
}

fn validate_static(
    axis: StructuralAxis,
    kind: StructuralEditKind,
    at: u32,
    count: u32,
) -> Result<(), StructuralEditError> {
    if count == 0 {
        return Err(StructuralEditError::ZeroCount);
    }

    let limit = axis_limit(axis);
    let in_bounds = at < limit && at.checked_add(count).is_some_and(|end| end <= limit);
    if in_bounds {
        Ok(())
    } else {
        Err(StructuralEditError::OutOfBounds {
            axis,
            kind,
            at,
            count,
        })
    }
}

fn apply_structural_edit(sheet: &mut Sheet, edit: StructuralEdit) {
    let refshift = RefShift {
        sheet: edit.sheet,
        axis: edit.axis.into(),
        at: edit.at,
        count: edit.count,
        kind: edit.kind.into(),
    };

    let mut shifted_cells = HashMap::new();
    for (coord, cell) in sheet
        .iter_cells()
        .map(|(coord, cell)| (coord, cell.clone()))
        .collect::<Vec<_>>()
    {
        let Some(shifted_coord) = shift_coord(coord, edit) else {
            continue;
        };
        let shifted_cell = shift_cell(cell, refshift);
        shifted_cells.insert(shifted_coord, shifted_cell);
    }
    sheet.replace_cells(shifted_cells);

    match edit.axis {
        StructuralAxis::Row => shift_dimensions(&mut sheet.row_heights, edit),
        StructuralAxis::Col => shift_dimensions(&mut sheet.col_widths, edit),
    }
}

fn shift_cell(cell: Cell, refshift: RefShift) -> Cell {
    if cell.ast.is_none() {
        return cell;
    }

    let raw = rewrite_formula(&cell.raw, refshift).raw().to_string();
    parse_cell_source(raw, cell.format)
}

fn shift_dimensions(dimensions: &mut HashMap<u32, f32>, edit: StructuralEdit) {
    let shifted = std::mem::take(dimensions)
        .into_iter()
        .filter_map(|(index, value)| shift_index(index, edit).map(|index| (index, value)))
        .collect();
    *dimensions = shifted;
}

fn shift_coord(mut coord: Coord, edit: StructuralEdit) -> Option<Coord> {
    let shifted = shift_index(coord_axis(coord, edit.axis), edit)?;
    set_coord_axis(&mut coord, edit.axis, shifted);
    Some(coord)
}

fn shift_index(index: u32, edit: StructuralEdit) -> Option<u32> {
    match edit.kind {
        StructuralEditKind::Insert => {
            if index < edit.at {
                Some(index)
            } else {
                index
                    .checked_add(edit.count)
                    .filter(|shifted| *shifted < axis_limit(edit.axis))
            }
        }
        StructuralEditKind::Delete => {
            let delete_end = edit.at + edit.count - 1;
            if index < edit.at {
                Some(index)
            } else if index <= delete_end {
                None
            } else {
                index.checked_sub(edit.count)
            }
        }
    }
}

fn first_occupied_index_pushed_out(sheet: &Sheet, edit: StructuralEdit) -> Option<u32> {
    let first_overflowing_source = axis_limit(edit.axis) - edit.count;
    let occupied_cells = sheet
        .iter_cells()
        .map(|(coord, _)| coord_axis(coord, edit.axis));
    let occupied_dimensions = match edit.axis {
        StructuralAxis::Row => sheet.row_heights.keys(),
        StructuralAxis::Col => sheet.col_widths.keys(),
    }
    .copied();

    occupied_cells
        .chain(occupied_dimensions)
        .filter(|index| *index >= edit.at && *index >= first_overflowing_source)
        .min()
}

fn label_for_edit(edit: StructuralEdit) -> String {
    let action = match edit.kind {
        StructuralEditKind::Insert => "Insert",
        StructuralEditKind::Delete => "Delete",
    };
    let unit = match (edit.axis, edit.count) {
        (StructuralAxis::Row, 1) => "row",
        (StructuralAxis::Row, _) => "rows",
        (StructuralAxis::Col, 1) => "column",
        (StructuralAxis::Col, _) => "columns",
    };
    format!("{action} {} {unit}", edit.count)
}

fn coord_axis(coord: Coord, axis: StructuralAxis) -> u32 {
    match axis {
        StructuralAxis::Row => coord.row,
        StructuralAxis::Col => coord.col,
    }
}

fn set_coord_axis(coord: &mut Coord, axis: StructuralAxis, value: u32) {
    match axis {
        StructuralAxis::Row => coord.row = value,
        StructuralAxis::Col => coord.col = value,
    }
}

fn axis_limit(axis: StructuralAxis) -> u32 {
    match axis {
        StructuralAxis::Row => MAX_ROWS,
        StructuralAxis::Col => MAX_COLS,
    }
}

impl From<StructuralAxis> for RefShiftAxis {
    fn from(axis: StructuralAxis) -> Self {
        match axis {
            StructuralAxis::Row => Self::Row,
            StructuralAxis::Col => Self::Col,
        }
    }
}

impl From<StructuralEditKind> for RefShiftEditKind {
    fn from(kind: StructuralEditKind) -> Self {
        match kind {
            StructuralEditKind::Insert => Self::Insert,
            StructuralEditKind::Delete => Self::Delete,
        }
    }
}
