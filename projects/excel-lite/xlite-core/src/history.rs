use crate::recalc::{RecalcDelta, RecalcEngine};

pub mod format;
pub mod layout;
pub mod refshift;
pub mod structural;

pub trait Command: Send {
    /// Apply the mutation and return the recalc delta for the UI.
    fn apply(&mut self, wb: &mut RecalcEngine) -> RecalcDelta;

    /// Produce the inverse command. Implementations capture prior state at apply time.
    fn invert(&self) -> Box<dyn Command>;

    /// Short UI-facing label for the mutation.
    fn label(&self) -> String;
}

pub struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

struct HistoryEntry {
    command: Box<dyn Command>,
    label: String,
}

impl HistoryEntry {
    fn new(command: Box<dyn Command>, label: String) -> Self {
        Self { command, label }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn exec(&mut self, mut cmd: Box<dyn Command>, wb: &mut RecalcEngine) -> RecalcDelta {
        let delta = cmd.apply(wb);
        let inverse = cmd.invert();
        let label = cmd.label();

        self.undo.push(HistoryEntry::new(inverse, label));
        self.redo.clear();

        delta
    }

    pub fn undo(&mut self, wb: &mut RecalcEngine) -> Option<RecalcDelta> {
        let mut entry = self.undo.pop()?;
        let delta = entry.command.apply(wb);
        let redo = entry.command.invert();

        self.redo.push(HistoryEntry::new(redo, entry.label));

        Some(delta)
    }

    pub fn redo(&mut self, wb: &mut RecalcEngine) -> Option<RecalcDelta> {
        let mut entry = self.redo.pop()?;
        let delta = entry.command.apply(wb);
        let undo = entry.command.invert();

        self.undo.push(HistoryEntry::new(undo, entry.label));

        Some(delta)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|entry| entry.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|entry| entry.label.as_str())
    }
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CellId, Coord, Value, Workbook};

    #[derive(Debug)]
    struct ToySetCell {
        id: CellId,
        raw: String,
        prior_raw: Option<String>,
        label: String,
    }

    impl ToySetCell {
        fn boxed(addr: &str, raw: &str, label: &str) -> Box<dyn Command> {
            Box::new(Self {
                id: id(addr),
                raw: raw.to_string(),
                prior_raw: None,
                label: label.to_string(),
            })
        }
    }

    impl Command for ToySetCell {
        fn apply(&mut self, wb: &mut RecalcEngine) -> RecalcDelta {
            self.prior_raw = raw_at(wb, self.id);
            wb.set_cell(self.id, &self.raw)
        }

        fn invert(&self) -> Box<dyn Command> {
            Box::new(Self {
                id: self.id,
                raw: self.prior_raw.clone().unwrap_or_default(),
                prior_raw: None,
                label: self.label.clone(),
            })
        }

        fn label(&self) -> String {
            self.label.clone()
        }
    }

    fn engine() -> RecalcEngine {
        RecalcEngine::new(Workbook::new())
    }

    fn id(addr: &str) -> CellId {
        CellId::new(0, Coord::from_a1(addr).expect("valid test cell address"))
    }

    fn raw_at(wb: &RecalcEngine, id: CellId) -> Option<String> {
        wb.workbook()
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
            .map(|cell| cell.raw.clone())
    }

    fn expected_delta(addr: &str, value: Value) -> RecalcDelta {
        RecalcDelta {
            changed: vec![(id(addr), value)],
            circular: Vec::new(),
        }
    }

    #[test]
    fn exec_applies_command_and_records_undo_entry() {
        let mut history = History::new();
        let mut wb = engine();

        let delta = history.exec(ToySetCell::boxed("A1", "42", "Edit A1"), &mut wb);

        assert_eq!(wb.cell_value(id("A1")), Value::Number(42.0));
        assert_eq!(delta, expected_delta("A1", Value::Number(42.0)));
        assert_eq!(history.undo.len(), 1);
        assert_eq!(history.redo.len(), 0);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_label(), Some("Edit A1"));
        assert_eq!(history.redo_label(), None);
    }

    #[test]
    fn new_exec_clears_redo_after_undo() {
        let mut history = History::new();
        let mut wb = engine();

        history.exec(ToySetCell::boxed("A1", "1", "First edit"), &mut wb);
        assert!(history.undo(&mut wb).is_some());
        assert_eq!(history.redo.len(), 1);
        assert_eq!(history.redo_label(), Some("First edit"));

        history.exec(ToySetCell::boxed("A1", "2", "Second edit"), &mut wb);

        assert_eq!(history.redo.len(), 0);
        assert_eq!(history.redo_label(), None);
        assert_eq!(wb.cell_value(id("A1")), Value::Number(2.0));
    }

    #[test]
    fn undo_and_redo_apply_commands_in_lifo_order() {
        let mut history = History::new();
        let mut wb = engine();

        history.exec(ToySetCell::boxed("A1", "1", "First edit"), &mut wb);
        history.exec(ToySetCell::boxed("A1", "2", "Second edit"), &mut wb);

        assert_eq!(history.undo_label(), Some("Second edit"));

        history.undo(&mut wb).expect("second edit undo");
        assert_eq!(wb.cell_value(id("A1")), Value::Number(1.0));
        assert_eq!(history.undo_label(), Some("First edit"));
        assert_eq!(history.redo_label(), Some("Second edit"));

        history.undo(&mut wb).expect("first edit undo");
        assert_eq!(wb.cell_value(id("A1")), Value::Blank);
        assert_eq!(history.undo_label(), None);
        assert_eq!(history.redo_label(), Some("First edit"));

        history.redo(&mut wb).expect("first edit redo");
        assert_eq!(wb.cell_value(id("A1")), Value::Number(1.0));
        assert_eq!(history.undo_label(), Some("First edit"));
        assert_eq!(history.redo_label(), Some("Second edit"));

        history.redo(&mut wb).expect("second edit redo");
        assert_eq!(wb.cell_value(id("A1")), Value::Number(2.0));
        assert_eq!(history.undo_label(), Some("Second edit"));
        assert_eq!(history.redo_label(), None);
    }

    #[test]
    fn empty_undo_and_redo_return_none() {
        let mut history = History::new();
        let mut wb = engine();

        assert_eq!(history.undo(&mut wb), None);
        assert_eq!(history.redo(&mut wb), None);
    }

    #[test]
    fn exec_undo_and_redo_return_the_applied_command_delta() {
        let mut history = History::new();
        let mut wb = engine();

        let exec_delta = history.exec(ToySetCell::boxed("A1", "7", "Edit A1"), &mut wb);
        assert_eq!(exec_delta, expected_delta("A1", Value::Number(7.0)));

        let undo_delta = history.undo(&mut wb).expect("undo delta");
        assert_eq!(undo_delta, expected_delta("A1", Value::Blank));

        let redo_delta = history.redo(&mut wb).expect("redo delta");
        assert_eq!(redo_delta, expected_delta("A1", Value::Number(7.0)));
    }
}
