use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
};

use crate::eval::{eval, EvalContext, RangeView};
use crate::functions::{build_registry, Function};
use crate::model::{
    Cell, CellFormat, CellId, Coord, DateSystem, ErrorValue, Sheet, Value, Workbook,
};
use crate::syntax::{parse, CellRef, Expr, RangeRef};

const MAX_RANGE_PRECEDENTS_TO_EXPAND: u64 = 10_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecalcDelta {
    pub changed: Vec<(CellId, Value)>,
    pub circular: Vec<CellId>,
}

pub struct RecalcEngine {
    workbook: Workbook,
    precedents: HashMap<CellId, HashSet<CellId>>,
    dependents: HashMap<CellId, HashSet<CellId>>,
    range_precedents: HashMap<CellId, Vec<CellRange>>,
    dynamic_range_precedents: HashMap<CellId, Vec<CellRange>>,
    registry: HashMap<&'static str, &'static dyn Function>,
    evaluation_count: usize,
}

impl RecalcEngine {
    pub fn new(workbook: Workbook) -> Self {
        let mut engine = Self {
            workbook,
            precedents: HashMap::new(),
            dependents: HashMap::new(),
            range_precedents: HashMap::new(),
            dynamic_range_precedents: HashMap::new(),
            registry: build_registry(),
            evaluation_count: 0,
        };
        engine.rebuild_graph();
        engine
    }

    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    pub fn workbook_mut(&mut self) -> &mut Workbook {
        &mut self.workbook
    }

    pub fn set_cell(&mut self, id: CellId, raw: &str) -> RecalcDelta {
        let old_value = self.cached_value(id);
        self.commit_cell(id, raw);
        self.replace_edges(id);
        let dirty = self.dirty_set(id);
        let mut delta = self.recalc_dirty(dirty);
        let new_value = self.cached_value(id);
        if old_value != new_value
            && !delta
                .changed
                .iter()
                .any(|(changed_id, _)| *changed_id == id)
        {
            delta.changed.insert(0, (id, new_value));
        }
        delta
    }

    pub fn recalc_all(&mut self) -> RecalcDelta {
        self.rebuild_graph();
        let dirty = self
            .workbook
            .sheets
            .iter()
            .enumerate()
            .flat_map(|(sheet_idx, sheet)| {
                sheet.iter_cells().map(move |(coord, _)| CellId {
                    sheet: sheet_idx as u16,
                    coord,
                })
            })
            .collect();
        self.recalc_dirty(dirty)
    }

    pub(crate) fn replace_sheet_snapshot(&mut self, sheet: u16, snapshot: Sheet) -> RecalcDelta {
        let Some(before) = self.workbook.sheet(sheet).cloned() else {
            return RecalcDelta::default();
        };

        let sheet_index = sheet as usize;
        if sheet_index >= self.workbook.sheets.len() {
            return RecalcDelta::default();
        }

        self.workbook.sheets[sheet_index] = snapshot;
        let recalc_delta = self.recalc_all();
        let after = self
            .workbook
            .sheet(sheet)
            .expect("sheet exists after snapshot replacement");

        sheet_delta(sheet, &before, after, recalc_delta.circular)
    }

    pub fn precedents(&self, id: CellId) -> Vec<CellId> {
        let mut precedents: HashSet<CellId> = self
            .precedents
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for range in self.range_precedents.get(&id).into_iter().flatten() {
            if range.cell_count() <= MAX_RANGE_PRECEDENTS_TO_EXPAND {
                precedents.extend(range.cells());
            }
        }
        for range in self.dynamic_range_precedents.get(&id).into_iter().flatten() {
            if range.cell_count() <= MAX_RANGE_PRECEDENTS_TO_EXPAND {
                precedents.extend(range.cells());
            }
        }
        sorted_cells(precedents.into_iter())
    }

    pub fn dependents(&self, id: CellId) -> Vec<CellId> {
        let mut dependents: HashSet<CellId> = self
            .dependents
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        dependents.extend(self.range_dependents(id));
        sorted_cells(dependents.into_iter())
    }

    pub fn cell_value(&self, id: CellId) -> Value {
        self.cached_value(id)
    }

    pub fn evaluation_count(&self) -> usize {
        self.evaluation_count
    }

    pub fn reset_evaluation_count(&mut self) {
        self.evaluation_count = 0;
    }

    fn commit_cell(&mut self, id: CellId, raw: &str) {
        if id.sheet as usize >= self.workbook.sheets.len() {
            return;
        }

        if raw.is_empty() {
            if let Some(sheet) = self.workbook.sheet_mut(id.sheet) {
                sheet.remove_cell(id.coord);
            }
            return;
        }

        let existing_format = self
            .workbook
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
            .map(|cell| cell.format.clone())
            .unwrap_or_default();

        if let Some(sheet) = self.workbook.sheet_mut(id.sheet) {
            sheet.set_cell(
                id.coord,
                parse_cell_source(raw.to_string(), existing_format),
            );
        }
    }

    fn rebuild_graph(&mut self) {
        self.precedents.clear();
        self.dependents.clear();
        self.range_precedents.clear();
        let ids: Vec<CellId> = self
            .workbook
            .sheets
            .iter()
            .enumerate()
            .flat_map(|(sheet_idx, sheet)| {
                sheet.iter_cells().map(move |(coord, _)| CellId {
                    sheet: sheet_idx as u16,
                    coord,
                })
            })
            .collect();
        let formula_ids: HashSet<_> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.workbook
                    .sheet(id.sheet)
                    .and_then(|sheet| sheet.get_cell(id.coord))
                    .is_some_and(|cell| cell.ast.is_some())
            })
            .collect();
        self.dynamic_range_precedents
            .retain(|id, _| formula_ids.contains(id));

        for id in ids {
            self.replace_static_edges(id);
        }
    }

    fn replace_edges(&mut self, id: CellId) {
        self.dynamic_range_precedents.remove(&id);
        self.replace_static_edges(id);
    }

    fn replace_static_edges(&mut self, id: CellId) {
        if let Some(old_precedents) = self.precedents.remove(&id) {
            for precedent in old_precedents {
                if let Some(dependents) = self.dependents.get_mut(&precedent) {
                    dependents.remove(&id);
                    if dependents.is_empty() {
                        self.dependents.remove(&precedent);
                    }
                }
            }
        }

        self.range_precedents.remove(&id);
        let new_precedents = self.extract_precedents(id);

        if !new_precedents.cells.is_empty() {
            for precedent in &new_precedents.cells {
                self.dependents.entry(*precedent).or_default().insert(id);
            }
            self.precedents.insert(id, new_precedents.cells);
        }

        if !new_precedents.ranges.is_empty() {
            self.range_precedents.insert(id, new_precedents.ranges);
        }
    }

    fn replace_dynamic_edges(&mut self, id: CellId, ranges: Vec<RangeRef>) {
        self.dynamic_range_precedents.remove(&id);
        let ranges: Vec<_> = ranges
            .into_iter()
            .filter_map(|range| CellRange::from_ref(range, id.sheet))
            .collect();
        if !ranges.is_empty() {
            self.dynamic_range_precedents.insert(id, ranges);
        }
    }

    fn extract_precedents(&self, id: CellId) -> ExtractedPrecedents {
        let mut precedents = ExtractedPrecedents::default();
        let Some(cell) = self
            .workbook
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
        else {
            return precedents;
        };

        if let Some(ast) = &cell.ast {
            collect_precedents(ast, id.sheet, &mut precedents);
        }
        precedents
    }

    fn dirty_set(&self, changed: CellId) -> HashSet<CellId> {
        let mut dirty = HashSet::new();
        let mut queue = VecDeque::from([changed]);
        while let Some(id) = queue.pop_front() {
            if !dirty.insert(id) {
                continue;
            }
            if let Some(dependents) = self.dependents.get(&id) {
                for dependent in dependents {
                    queue.push_back(*dependent);
                }
            }
            for dependent in self.range_dependents(id) {
                queue.push_back(dependent);
            }
        }
        dirty
    }

    fn range_dependents(&self, precedent: CellId) -> Vec<CellId> {
        sorted_cells(
            self.range_precedents
                .iter()
                .chain(self.dynamic_range_precedents.iter())
                .filter_map(|(dependent, ranges)| {
                    ranges
                        .iter()
                        .any(|range| range.contains(precedent))
                        .then_some(*dependent)
                }),
        )
    }

    fn recalc_dirty(&mut self, dirty: HashSet<CellId>) -> RecalcDelta {
        let mut delta = RecalcDelta::default();
        let old_values: HashMap<CellId, Value> = dirty
            .iter()
            .copied()
            .map(|id| (id, self.cached_value(id)))
            .collect();

        let circular = self.cycle_nodes(&dirty);
        for id in sorted_cells(circular.iter().copied()) {
            self.set_cached(id, Value::Error(ErrorValue::Circular));
            delta.circular.push(id);
        }

        let order = self.topological_order_without_cycles(&dirty, &circular);
        for id in order {
            self.evaluate_cell(id);
        }

        let dynamic_circular = self.cycle_nodes(&dirty);
        for id in sorted_cells(dynamic_circular.difference(&circular).copied()) {
            self.set_cached(id, Value::Error(ErrorValue::Circular));
            delta.circular.push(id);
        }

        for id in sorted_cells(dirty.into_iter()) {
            let old = old_values.get(&id).cloned().unwrap_or(Value::Blank);
            let new = self.cached_value(id);
            if old != new {
                delta.changed.push((id, new));
            }
        }
        delta
    }

    fn evaluate_cell(&mut self, id: CellId) {
        let ast = self
            .workbook
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
            .and_then(|cell| cell.ast.clone());

        let Some(ast) = ast else {
            return;
        };

        let (value, dynamic_ranges) = {
            let ctx = WorkbookEvalContext {
                engine: self,
                current_cell: id,
                dynamic_ranges: RefCell::new(Vec::new()),
            };
            let value = eval(&ast, &ctx);
            (value, ctx.into_dynamic_ranges())
        };
        self.evaluation_count += 1;
        self.replace_dynamic_edges(id, dynamic_ranges);
        self.set_cached(id, value);
    }

    fn set_cached(&mut self, id: CellId, value: Value) {
        if let Some(cell) = self
            .workbook
            .sheet_mut(id.sheet)
            .and_then(|sheet| sheet.get_cell_mut(id.coord))
        {
            cell.cached = value;
        }
    }

    fn cached_value(&self, id: CellId) -> Value {
        self.workbook
            .sheet(id.sheet)
            .and_then(|sheet| sheet.get_cell(id.coord))
            .map(|cell| cell.cached.clone())
            .unwrap_or(Value::Blank)
    }

    fn topological_order_without_cycles(
        &self,
        dirty: &HashSet<CellId>,
        circular: &HashSet<CellId>,
    ) -> Vec<CellId> {
        let candidates: HashSet<CellId> = dirty.difference(circular).copied().collect();
        let mut indegree: HashMap<CellId, usize> =
            candidates.iter().copied().map(|id| (id, 0)).collect();
        let mut outgoing: HashMap<CellId, Vec<CellId>> = HashMap::new();

        for id in &candidates {
            let mut incoming = HashSet::new();
            for precedent in self.precedents.get(id).into_iter().flatten() {
                if candidates.contains(precedent) {
                    incoming.insert(*precedent);
                }
            }
            for range in self.range_precedents.get(id).into_iter().flatten() {
                for candidate in &candidates {
                    if range.contains(*candidate) {
                        incoming.insert(*candidate);
                    }
                }
            }
            for range in self.dynamic_range_precedents.get(id).into_iter().flatten() {
                for candidate in &candidates {
                    if range.contains(*candidate) {
                        incoming.insert(*candidate);
                    }
                }
            }

            *indegree.entry(*id).or_default() = incoming.len();
            for precedent in incoming {
                outgoing.entry(precedent).or_default().push(*id);
            }
        }

        let mut queue: VecDeque<CellId> = sorted_cells(
            indegree
                .iter()
                .filter_map(|(id, degree)| (*degree == 0).then_some(*id)),
        )
        .into();
        let mut order = Vec::new();

        while let Some(id) = queue.pop_front() {
            order.push(id);
            if let Some(next) = outgoing.get(&id) {
                for dependent in sorted_cells(next.iter().copied()) {
                    let degree = indegree.get_mut(&dependent).expect("candidate indegree");
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dependent);
                    }
                }
            }
        }

        order
    }

    fn cycle_nodes(&self, nodes: &HashSet<CellId>) -> HashSet<CellId> {
        let mut tarjan = Tarjan::new(self, nodes);
        tarjan.run()
    }
}

impl Default for RecalcEngine {
    fn default() -> Self {
        Self::new(Workbook::new())
    }
}

struct WorkbookEvalContext<'a> {
    engine: &'a RecalcEngine,
    current_cell: CellId,
    dynamic_ranges: RefCell<Vec<RangeRef>>,
}

impl WorkbookEvalContext<'_> {
    fn into_dynamic_ranges(self) -> Vec<RangeRef> {
        self.dynamic_ranges.into_inner()
    }
}

impl EvalContext for WorkbookEvalContext<'_> {
    fn cell_value(&self, r: CellRef) -> Value {
        let sheet = r.sheet.unwrap_or(self.current_cell.sheet);
        if sheet != 0 || sheet as usize >= self.engine.workbook.sheets.len() {
            return Value::Error(ErrorValue::Ref);
        }
        self.engine.cached_value(CellId {
            sheet,
            coord: Coord {
                row: r.row,
                col: r.col,
            },
        })
    }

    fn range_view(&self, r: RangeRef) -> RangeView<'_> {
        RangeView::new(self, r)
    }

    fn function(&self, name: &str) -> Option<&dyn Function> {
        self.engine.registry.get(name).copied()
    }

    fn date_system(&self) -> DateSystem {
        self.engine.workbook.date_system
    }

    fn current_cell(&self) -> CellId {
        self.current_cell
    }

    fn record_dynamic_range(&self, r: RangeRef) {
        self.dynamic_ranges.borrow_mut().push(r);
    }
}

struct Tarjan<'a> {
    engine: &'a RecalcEngine,
    nodes: &'a HashSet<CellId>,
    next_index: usize,
    indices: HashMap<CellId, usize>,
    lowlinks: HashMap<CellId, usize>,
    stack: Vec<CellId>,
    on_stack: HashSet<CellId>,
    cycles: HashSet<CellId>,
}

impl<'a> Tarjan<'a> {
    fn new(engine: &'a RecalcEngine, nodes: &'a HashSet<CellId>) -> Self {
        Self {
            engine,
            nodes,
            next_index: 0,
            indices: HashMap::new(),
            lowlinks: HashMap::new(),
            stack: Vec::new(),
            on_stack: HashSet::new(),
            cycles: HashSet::new(),
        }
    }

    fn run(&mut self) -> HashSet<CellId> {
        for id in sorted_cells(self.nodes.iter().copied()) {
            if !self.indices.contains_key(&id) {
                self.strong_connect(id);
            }
        }
        std::mem::take(&mut self.cycles)
    }

    fn strong_connect(&mut self, id: CellId) {
        let index = self.next_index;
        self.next_index += 1;
        self.indices.insert(id, index);
        self.lowlinks.insert(id, index);
        self.stack.push(id);
        self.on_stack.insert(id);

        for next in self.neighbors(id) {
            if !self.indices.contains_key(&next) {
                self.strong_connect(next);
                let low = self.lowlinks[&id].min(self.lowlinks[&next]);
                self.lowlinks.insert(id, low);
            } else if self.on_stack.contains(&next) {
                let low = self.lowlinks[&id].min(self.indices[&next]);
                self.lowlinks.insert(id, low);
            }
        }

        if self.lowlinks[&id] == self.indices[&id] {
            let mut component = Vec::new();
            loop {
                let node = self.stack.pop().expect("non-empty tarjan stack");
                self.on_stack.remove(&node);
                component.push(node);
                if node == id {
                    break;
                }
            }

            let self_loop = component.len() == 1
                && (self
                    .engine
                    .precedents
                    .get(&component[0])
                    .is_some_and(|deps| deps.contains(&component[0]))
                    || self
                        .engine
                        .range_precedents
                        .get(&component[0])
                        .is_some_and(|ranges| {
                            ranges.iter().any(|range| range.contains(component[0]))
                        })
                    || self
                        .engine
                        .dynamic_range_precedents
                        .get(&component[0])
                        .is_some_and(|ranges| {
                            ranges.iter().any(|range| range.contains(component[0]))
                        }));
            if component.len() > 1 || self_loop {
                self.cycles.extend(component);
            }
        }
    }

    fn neighbors(&self, id: CellId) -> Vec<CellId> {
        let mut neighbors: HashSet<CellId> = self
            .engine
            .precedents
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| self.nodes.contains(id))
            .collect();
        for range in self.engine.range_precedents.get(&id).into_iter().flatten() {
            for node in self.nodes {
                if range.contains(*node) {
                    neighbors.insert(*node);
                }
            }
        }
        for range in self
            .engine
            .dynamic_range_precedents
            .get(&id)
            .into_iter()
            .flatten()
        {
            for node in self.nodes {
                if range.contains(*node) {
                    neighbors.insert(*node);
                }
            }
        }
        sorted_cells(neighbors.into_iter())
    }
}

#[derive(Default)]
struct ExtractedPrecedents {
    cells: HashSet<CellId>,
    ranges: Vec<CellRange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CellRange {
    sheet: u16,
    row_start: u32,
    row_end: u32,
    col_start: u32,
    col_end: u32,
}

impl CellRange {
    fn from_ref(range: RangeRef, current_sheet: u16) -> Option<Self> {
        let sheet = match (range.start.sheet, range.end.sheet) {
            (Some(start), Some(end)) if start == end => start,
            (Some(_), Some(_)) => return None,
            (Some(start), None) => start,
            (None, Some(end)) => end,
            (None, None) => current_sheet,
        };
        if sheet != 0 {
            return None;
        }

        let start = range.start.coord();
        let end = range.end.coord();
        Some(Self {
            sheet,
            row_start: start.row.min(end.row),
            row_end: start.row.max(end.row),
            col_start: start.col.min(end.col),
            col_end: start.col.max(end.col),
        })
    }

    fn contains(self, id: CellId) -> bool {
        id.sheet == self.sheet
            && (self.row_start..=self.row_end).contains(&id.coord.row)
            && (self.col_start..=self.col_end).contains(&id.coord.col)
    }

    fn cell_count(self) -> u64 {
        let rows = u64::from(self.row_end - self.row_start + 1);
        let cols = u64::from(self.col_end - self.col_start + 1);
        rows * cols
    }

    fn cells(self) -> impl Iterator<Item = CellId> {
        (self.row_start..=self.row_end).flat_map(move |row| {
            (self.col_start..=self.col_end).map(move |col| CellId {
                sheet: self.sheet,
                coord: Coord { row, col },
            })
        })
    }
}

fn collect_precedents(expr: &Expr, current_sheet: u16, precedents: &mut ExtractedPrecedents) {
    match expr {
        Expr::Literal(_) => {}
        Expr::Ref(cell_ref) => {
            if let Some(id) = cell_id_from_ref(*cell_ref, current_sheet) {
                precedents.cells.insert(id);
            }
        }
        Expr::Range(range) => {
            if let Some(range) = CellRange::from_ref(*range, current_sheet) {
                precedents.ranges.push(range);
            }
        }
        Expr::Unary { rhs, .. } => collect_precedents(rhs, current_sheet, precedents),
        Expr::Binary { lhs, rhs, .. } => {
            collect_precedents(lhs, current_sheet, precedents);
            collect_precedents(rhs, current_sheet, precedents);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_precedents(arg, current_sheet, precedents);
            }
        }
    }
}

fn cell_id_from_ref(cell_ref: CellRef, current_sheet: u16) -> Option<CellId> {
    let sheet = cell_ref.sheet.unwrap_or(current_sheet);
    if sheet != 0 {
        return None;
    }
    Some(CellId {
        sheet,
        coord: cell_ref.coord(),
    })
}

fn parse_literal(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("TRUE") {
        Value::Boolean(true)
    } else if trimmed.eq_ignore_ascii_case("FALSE") {
        Value::Boolean(false)
    } else if let Ok(number) = trimmed.parse::<f64>() {
        Value::Number(number)
    } else {
        Value::Text(raw.to_string())
    }
}

pub(crate) fn parse_cell_source(raw: String, format: CellFormat) -> Cell {
    let (ast, cached) = if let Some(body) = raw.strip_prefix('=') {
        match parse(body) {
            Ok(expr) => (Some(expr), Value::Blank),
            Err(_) => (None, Value::Error(ErrorValue::Name)),
        }
    } else {
        (None, parse_literal(&raw))
    };

    let mut cell = Cell::new(raw, ast, cached);
    cell.format = format;
    cell
}

fn sheet_delta(sheet: u16, before: &Sheet, after: &Sheet, circular: Vec<CellId>) -> RecalcDelta {
    let mut coords: HashSet<Coord> = before.iter_cells().map(|(coord, _)| coord).collect();
    coords.extend(after.iter_cells().map(|(coord, _)| coord));

    let changed = coords.into_iter().filter_map(|coord| {
        let before_cell = before.get_cell(coord);
        let after_cell = after.get_cell(coord);
        let before_value = before_cell
            .map(|cell| cell.cached.clone())
            .unwrap_or(Value::Blank);
        let after_value = after_cell
            .map(|cell| cell.cached.clone())
            .unwrap_or(Value::Blank);
        let raw_changed =
            before_cell.map(|cell| cell.raw.as_str()) != after_cell.map(|cell| cell.raw.as_str());

        (before_value != after_value || raw_changed)
            .then_some((CellId { sheet, coord }, after_value))
    });

    RecalcDelta {
        changed: sorted_cells_with_values(changed),
        circular,
    }
}

fn sorted_cells(iter: impl Iterator<Item = CellId>) -> Vec<CellId> {
    let mut cells: Vec<_> = iter.collect();
    cells.sort();
    cells
}

fn sorted_cells_with_values(iter: impl Iterator<Item = (CellId, Value)>) -> Vec<(CellId, Value)> {
    let mut cells: Vec<_> = iter.collect();
    cells.sort_by_key(|(id, _)| *id);
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::{Arg, EvalResult, FnContext},
        functions::FunctionArgMode,
        syntax::{parse_local_reference_text, LocalReference},
    };

    fn id(addr: &str) -> CellId {
        CellId {
            sheet: 0,
            coord: Coord::from_a1(addr).unwrap(),
        }
    }

    #[test]
    fn exit_flow_recalculates_dependents() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "1");
        engine.set_cell(id("A2"), "2");
        engine.set_cell(id("A3"), "3");
        engine.set_cell(id("B1"), "=SUM(A1:A3)");
        engine.set_cell(id("B2"), "=AVERAGE(A1:A3)");
        engine.set_cell(id("B3"), r#"=IF(A1>0,"pos","neg")"#);

        assert_eq!(engine.cell_value(id("B1")), Value::Number(6.0));
        assert_eq!(engine.cell_value(id("B2")), Value::Number(2.0));
        assert_eq!(engine.cell_value(id("B3")), Value::Text("pos".to_string()));

        engine.set_cell(id("A1"), "5");
        assert_eq!(engine.cell_value(id("B1")), Value::Number(10.0));
    }

    #[test]
    fn cycle_001_self_ref_does_not_hang() {
        let mut engine = RecalcEngine::default();
        let delta = engine.set_cell(id("A1"), "=A1");
        assert_eq!(
            engine.cell_value(id("A1")),
            Value::Error(ErrorValue::Circular)
        );
        assert_eq!(delta.circular, vec![id("A1")]);
    }

    #[test]
    fn dependent_of_cycle_propagates_but_is_not_circular() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "=A1");
        let delta = engine.set_cell(id("C1"), "=A1");
        assert_eq!(
            engine.cell_value(id("C1")),
            Value::Error(ErrorValue::Circular)
        );
        assert!(delta.circular.is_empty());
    }

    #[test]
    fn tracks_precedents_and_dependents() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "1");
        engine.set_cell(id("B1"), "=A1+1");
        assert_eq!(engine.precedents(id("B1")), vec![id("A1")]);
        assert_eq!(engine.dependents(id("A1")), vec![id("B1")]);
    }

    #[test]
    fn row_and_column_reference_arguments_do_not_read_error_cells() {
        let mut engine = RecalcEngine::default();
        engine.set_cell(id("A1"), "=1/0");
        engine.set_cell(id("B1"), "=ROW(A1)");
        engine.set_cell(id("C1"), "=COLUMN(A1)");

        assert_eq!(engine.cell_value(id("A1")), Value::Error(ErrorValue::Div0));
        assert_eq!(engine.cell_value(id("B1")), Value::Number(1.0));
        assert_eq!(engine.cell_value(id("C1")), Value::Number(1.0));
    }

    #[test]
    fn dynamic_target_edit_dirties_reference_consumer() {
        let mut engine = contract_probe_engine();
        engine.set_cell(id("A1"), "B1");
        engine.set_cell(id("B1"), "10");
        engine.set_cell(id("C1"), "=REF_TEXT(A1)+1");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(11.0));

        engine.reset_evaluation_count();
        engine.set_cell(id("B1"), "20");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(21.0));
        assert_eq!(engine.evaluation_count(), 1);
    }

    #[test]
    fn dynamic_reference_orders_formula_target_before_consumer() {
        let mut engine = contract_probe_engine();
        engine.set_cell(id("D1"), "1");
        engine.set_cell(id("B1"), "=D1+1");
        engine.set_cell(id("A1"), "B1");
        engine.set_cell(id("C1"), "=REF_TEXT(A1)+1");

        assert_eq!(engine.cell_value(id("B1")), Value::Number(2.0));
        assert_eq!(engine.cell_value(id("C1")), Value::Number(3.0));

        engine.reset_evaluation_count();
        engine.set_cell(id("D1"), "5");

        assert_eq!(engine.cell_value(id("B1")), Value::Number(6.0));
        assert_eq!(engine.cell_value(id("C1")), Value::Number(7.0));
        assert_eq!(engine.evaluation_count(), 2);
    }

    #[test]
    fn dynamic_edges_are_replaced_after_reevaluation() {
        let mut engine = contract_probe_engine();
        engine.set_cell(id("A1"), "B1");
        engine.set_cell(id("B1"), "10");
        engine.set_cell(id("B2"), "20");
        engine.set_cell(id("C1"), "=REF_TEXT(A1)");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(10.0));
        assert!(engine.precedents(id("C1")).contains(&id("A1")));
        assert!(engine.precedents(id("C1")).contains(&id("B1")));
        assert!(!engine.precedents(id("C1")).contains(&id("B2")));
        assert!(engine.dependents(id("B1")).contains(&id("C1")));

        engine.set_cell(id("A1"), "B2");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(20.0));
        assert!(engine.precedents(id("C1")).contains(&id("A1")));
        assert!(!engine.precedents(id("C1")).contains(&id("B1")));
        assert!(engine.precedents(id("C1")).contains(&id("B2")));
        assert!(!engine.dependents(id("B1")).contains(&id("C1")));
        assert!(engine.dependents(id("B2")).contains(&id("C1")));
    }

    #[test]
    fn dynamic_range_dependencies_are_exact_for_dirtying() {
        let mut engine = contract_probe_engine();
        engine.set_cell(id("A1"), "B1:B2");
        engine.set_cell(id("B1"), "1");
        engine.set_cell(id("B2"), "2");
        engine.set_cell(id("B3"), "100");
        engine.set_cell(id("C1"), "=SUM(REF_TEXT(A1))");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(3.0));
        assert!(engine.precedents(id("C1")).contains(&id("B1")));
        assert!(engine.precedents(id("C1")).contains(&id("B2")));
        assert!(!engine.precedents(id("C1")).contains(&id("B3")));

        engine.reset_evaluation_count();
        engine.set_cell(id("B2"), "5");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(6.0));
        assert_eq!(engine.evaluation_count(), 1);

        engine.reset_evaluation_count();
        engine.set_cell(id("B3"), "200");

        assert_eq!(engine.cell_value(id("C1")), Value::Number(6.0));
        assert_eq!(engine.evaluation_count(), 0);
    }

    #[test]
    fn dynamic_self_cycle_is_detected() {
        let mut engine = contract_probe_engine();
        let delta = engine.set_cell(id("A1"), r#"=REF_TEXT("A1")"#);

        assert_eq!(
            engine.cell_value(id("A1")),
            Value::Error(ErrorValue::Circular)
        );
        assert_eq!(delta.circular, vec![id("A1")]);
    }

    #[test]
    fn dynamic_mutual_cycle_is_detected() {
        let mut engine = contract_probe_engine();
        engine.set_cell(id("A1"), r#"=REF_TEXT("B1")"#);
        let delta = engine.set_cell(id("B1"), r#"=REF_TEXT("A1")"#);

        assert_eq!(
            engine.cell_value(id("A1")),
            Value::Error(ErrorValue::Circular)
        );
        assert_eq!(
            engine.cell_value(id("B1")),
            Value::Error(ErrorValue::Circular)
        );
        assert_eq!(delta.circular, vec![id("A1"), id("B1")]);
    }

    #[test]
    fn invalid_dynamic_reference_text_returns_deterministic_error() {
        let mut engine = contract_probe_engine();

        for formula in [
            r#"=REF_TEXT("Sheet2!A1")"#,
            r#"=REF_TEXT("[Book1]Sheet1!A1")"#,
            r#"=REF_TEXT("R1C1")"#,
        ] {
            engine.set_cell(id("A1"), formula);
            assert_eq!(engine.cell_value(id("A1")), Value::Error(ErrorValue::Ref));
        }
    }

    #[test]
    fn formula_error_literals_evaluate_and_propagate() {
        let mut engine = RecalcEngine::default();

        for formula in ["=#REF!", "=#REF!+1", "=SUM(#REF!)"] {
            engine.set_cell(id("A1"), formula);
            assert_eq!(
                engine.cell_value(id("A1")),
                Value::Error(ErrorValue::Ref),
                "{formula}"
            );
        }
    }

    fn contract_probe_engine() -> RecalcEngine {
        let mut engine = RecalcEngine::default();
        engine.registry.insert("REF", &DYNAMIC_REF_FUNCTION);
        engine
            .registry
            .insert("REF_TEXT", &DYNAMIC_REF_TEXT_FUNCTION);
        engine
    }

    struct DynamicRefFunction;

    impl Function for DynamicRefFunction {
        fn name(&self) -> &'static str {
            "REF"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }

        fn argument_mode(&self, index: usize) -> FunctionArgMode {
            if index == 0 {
                FunctionArgMode::Reference
            } else {
                FunctionArgMode::Value
            }
        }

        fn call(&self, _args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            Value::Error(ErrorValue::Value)
        }

        fn call_result(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> EvalResult {
            match args {
                [Arg::Range(range)] => EvalResult::Range(range.range_ref()),
                [Arg::Value(Value::Error(error))] => EvalResult::Value(Value::Error(*error)),
                _ => EvalResult::Value(Value::Error(ErrorValue::Value)),
            }
        }
    }

    struct DynamicRefTextFunction;

    impl Function for DynamicRefTextFunction {
        fn name(&self) -> &'static str {
            "REF_TEXT"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }

        fn call(&self, _args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            Value::Error(ErrorValue::Value)
        }

        fn call_result(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> EvalResult {
            let [arg] = args else {
                return EvalResult::Value(Value::Error(ErrorValue::Value));
            };
            let text = match arg.as_value() {
                Value::Text(text) => text,
                Value::Error(error) => return EvalResult::Value(Value::Error(error)),
                _ => return EvalResult::Value(Value::Error(ErrorValue::Value)),
            };

            match parse_local_reference_text(&text) {
                Ok(LocalReference::Cell(cell_ref)) => EvalResult::Range(RangeRef {
                    start: cell_ref,
                    end: cell_ref,
                }),
                Ok(LocalReference::Range(range_ref)) => EvalResult::Range(range_ref),
                Err(_) => EvalResult::Value(Value::Error(ErrorValue::Ref)),
            }
        }
    }

    static DYNAMIC_REF_FUNCTION: DynamicRefFunction = DynamicRefFunction;
    static DYNAMIC_REF_TEXT_FUNCTION: DynamicRefTextFunction = DynamicRefTextFunction;
}
