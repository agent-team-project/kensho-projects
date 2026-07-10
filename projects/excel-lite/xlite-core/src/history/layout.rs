use std::{error::Error, fmt};

use crate::{
    model::{Sheet, DEFAULT_COL_WIDTH, DEFAULT_ROW_HEIGHT, MAX_COLS, MAX_ROWS},
    recalc::{RecalcDelta, RecalcEngine},
};

use super::Command;

#[derive(Clone, Debug, PartialEq)]
pub enum ResizeCommandError {
    SheetNotFound(u16),
    ColumnOutOfBounds(u32),
    RowOutOfBounds(u32),
    InvalidSize(f32),
}

impl fmt::Display for ResizeCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SheetNotFound(sheet) => write!(f, "unknown sheet: {sheet}"),
            Self::ColumnOutOfBounds(col) => write!(f, "column index {col} exceeds grid bounds"),
            Self::RowOutOfBounds(row) => write!(f, "row index {row} exceeds grid bounds"),
            Self::InvalidSize(size) => write!(f, "resize size must be finite and positive: {size}"),
        }
    }
}

impl Error for ResizeCommandError {}

#[derive(Clone, Debug)]
pub struct ResizeColumnCommand {
    core: ResizeCommandCore,
}

impl ResizeColumnCommand {
    pub fn new(sheet: u16, col: u32, width: f32) -> Result<Self, ResizeCommandError> {
        Ok(Self {
            core: ResizeCommandCore::new(sheet, ResizeAxis::Column, col, width)?,
        })
    }

    pub fn for_engine(
        engine: &RecalcEngine,
        sheet: u16,
        col: u32,
        width: f32,
    ) -> Result<Self, ResizeCommandError> {
        let command = Self::new(sheet, col, width)?;
        command.validate(engine)?;
        Ok(command)
    }

    pub fn validate(&self, engine: &RecalcEngine) -> Result<(), ResizeCommandError> {
        self.core.validate(engine)
    }

    pub fn sheet(&self) -> u16 {
        self.core.edit.sheet
    }

    pub fn col(&self) -> u32 {
        self.core.edit.index
    }

    pub fn width(&self) -> f32 {
        self.core.edit.size
    }
}

impl Command for ResizeColumnCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        self.core.apply(engine)
    }

    fn invert(&self) -> Box<dyn Command> {
        self.core.invert_command()
    }

    fn label(&self) -> String {
        self.core.label()
    }
}

#[derive(Clone, Debug)]
pub struct ResizeRowCommand {
    core: ResizeCommandCore,
}

impl ResizeRowCommand {
    pub fn new(sheet: u16, row: u32, height: f32) -> Result<Self, ResizeCommandError> {
        Ok(Self {
            core: ResizeCommandCore::new(sheet, ResizeAxis::Row, row, height)?,
        })
    }

    pub fn for_engine(
        engine: &RecalcEngine,
        sheet: u16,
        row: u32,
        height: f32,
    ) -> Result<Self, ResizeCommandError> {
        let command = Self::new(sheet, row, height)?;
        command.validate(engine)?;
        Ok(command)
    }

    pub fn validate(&self, engine: &RecalcEngine) -> Result<(), ResizeCommandError> {
        self.core.validate(engine)
    }

    pub fn sheet(&self) -> u16 {
        self.core.edit.sheet
    }

    pub fn row(&self) -> u32 {
        self.core.edit.index
    }

    pub fn height(&self) -> f32 {
        self.core.edit.size
    }
}

impl Command for ResizeRowCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        self.core.apply(engine)
    }

    fn invert(&self) -> Box<dyn Command> {
        self.core.invert_command()
    }

    fn label(&self) -> String {
        self.core.label()
    }
}

#[derive(Clone, Debug)]
struct ResizeCommandCore {
    edit: ResizeEdit,
    before: Option<Option<f32>>,
    after: Option<Option<f32>>,
}

impl ResizeCommandCore {
    fn new(
        sheet: u16,
        axis: ResizeAxis,
        index: u32,
        size: f32,
    ) -> Result<Self, ResizeCommandError> {
        validate_static(axis, index, size)?;
        Ok(Self {
            edit: ResizeEdit {
                sheet,
                axis,
                index,
                size,
            },
            before: None,
            after: None,
        })
    }

    fn validate(&self, engine: &RecalcEngine) -> Result<(), ResizeCommandError> {
        validate_static(self.edit.axis, self.edit.index, self.edit.size)?;
        engine
            .workbook()
            .sheet(self.edit.sheet)
            .ok_or(ResizeCommandError::SheetNotFound(self.edit.sheet))?;
        Ok(())
    }

    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        if self.validate(engine).is_err() {
            self.before = None;
            self.after = None;
            return RecalcDelta::default();
        }

        let before = dimension_value(
            engine
                .workbook()
                .sheet(self.edit.sheet)
                .expect("validated sheet"),
            self.edit,
        );

        {
            let sheet = engine
                .workbook_mut()
                .sheet_mut(self.edit.sheet)
                .expect("validated sheet");
            set_dimension_value(sheet, self.edit, target_dimension_value(self.edit));
        }

        let after = dimension_value(
            engine
                .workbook()
                .sheet(self.edit.sheet)
                .expect("validated sheet"),
            self.edit,
        );
        self.before = Some(before);
        self.after = Some(after);

        RecalcDelta::default()
    }

    fn invert_command(&self) -> Box<dyn Command> {
        Box::new(RestoreDimensionCommand {
            edit: self.edit,
            target: self.before,
            inverse_target: self.after,
            label: self.label(),
        })
    }

    fn label(&self) -> String {
        match self.edit.axis {
            ResizeAxis::Column => "Resize column",
            ResizeAxis::Row => "Resize row",
        }
        .to_string()
    }
}

#[derive(Clone, Debug)]
struct RestoreDimensionCommand {
    edit: ResizeEdit,
    target: Option<Option<f32>>,
    inverse_target: Option<Option<f32>>,
    label: String,
}

impl Command for RestoreDimensionCommand {
    fn apply(&mut self, engine: &mut RecalcEngine) -> RecalcDelta {
        let Some(target) = self.target else {
            return RecalcDelta::default();
        };
        let Some(sheet) = engine.workbook_mut().sheet_mut(self.edit.sheet) else {
            return RecalcDelta::default();
        };

        set_dimension_value(sheet, self.edit, target);
        RecalcDelta::default()
    }

    fn invert(&self) -> Box<dyn Command> {
        Box::new(Self {
            edit: self.edit,
            target: self.inverse_target,
            inverse_target: self.target,
            label: self.label.clone(),
        })
    }

    fn label(&self) -> String {
        self.label.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeAxis {
    Column,
    Row,
}

#[derive(Clone, Copy, Debug)]
struct ResizeEdit {
    sheet: u16,
    axis: ResizeAxis,
    index: u32,
    size: f32,
}

fn validate_static(axis: ResizeAxis, index: u32, size: f32) -> Result<(), ResizeCommandError> {
    match axis {
        ResizeAxis::Column if index >= MAX_COLS => {
            return Err(ResizeCommandError::ColumnOutOfBounds(index));
        }
        ResizeAxis::Row if index >= MAX_ROWS => {
            return Err(ResizeCommandError::RowOutOfBounds(index));
        }
        _ => {}
    }

    if !size.is_finite() || size <= 0.0 {
        return Err(ResizeCommandError::InvalidSize(size));
    }

    Ok(())
}

fn dimension_value(sheet: &Sheet, edit: ResizeEdit) -> Option<f32> {
    match edit.axis {
        ResizeAxis::Column => sheet.col_widths.get(&edit.index),
        ResizeAxis::Row => sheet.row_heights.get(&edit.index),
    }
    .copied()
}

fn set_dimension_value(sheet: &mut Sheet, edit: ResizeEdit, value: Option<f32>) {
    let dimensions = match edit.axis {
        ResizeAxis::Column => &mut sheet.col_widths,
        ResizeAxis::Row => &mut sheet.row_heights,
    };

    if let Some(value) = value {
        dimensions.insert(edit.index, value);
    } else {
        dimensions.remove(&edit.index);
    }
}

fn target_dimension_value(edit: ResizeEdit) -> Option<f32> {
    (edit.size != default_dimension(edit.axis)).then_some(edit.size)
}

fn default_dimension(axis: ResizeAxis) -> f32 {
    match axis {
        ResizeAxis::Column => DEFAULT_COL_WIDTH,
        ResizeAxis::Row => DEFAULT_ROW_HEIGHT,
    }
}
