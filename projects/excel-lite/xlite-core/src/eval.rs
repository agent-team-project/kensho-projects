use crate::functions::{Function, FunctionArgMode};
use crate::model::{CellId, DateSystem, ErrorValue, Value};
use crate::syntax::{BinaryOp, CellRef, Expr, RangeRef, UnaryOp};

/// Evaluation services supplied by the workbook/recalc layer.
///
/// The evaluator is pure: references, ranges, function lookup, current-cell
/// metadata, and deterministic time/random sources all flow through this trait.
pub trait EvalContext {
    /// Value of a single cell. Blank cells return `Value::Blank`.
    fn cell_value(&self, r: CellRef) -> Value;
    /// Lazy rectangular range view. Implementations must not materialize ranges.
    fn range_view(&self, r: RangeRef) -> RangeView<'_>;
    /// Resolve an uppercase canonical function name. Missing functions are `#NAME?`.
    fn function(&self, name: &str) -> Option<&dyn Function>;
    /// Workbook date system used by date/time functions.
    fn date_system(&self) -> DateSystem;
    /// Cell currently being evaluated.
    fn current_cell(&self) -> CellId;
    /// Deterministic serial date for TODAY-style functions.
    fn today_serial(&self) -> f64 {
        1.0
    }
    /// Deterministic serial timestamp for NOW-style functions.
    fn now_serial(&self) -> f64 {
        self.today_serial()
    }
    /// Deterministic random value in `[0, 1)` for RAND-style functions.
    fn rand(&self) -> f64 {
        0.5
    }
    /// Record a dynamically resolved reference result for the current formula.
    fn record_dynamic_range(&self, _r: RangeRef) {}
}

/// Internal evaluator result.
///
/// The public evaluator still returns a scalar `Value`; this type is used
/// inside expression/function evaluation so reference-returning functions can
/// pass a resolved range to another function before scalarization.
#[derive(Clone, Debug, PartialEq)]
pub enum EvalResult {
    Value(Value),
    Range(RangeRef),
}

impl EvalResult {
    pub fn into_value(self, ctx: &dyn EvalContext) -> Value {
        match self {
            Self::Value(value) => value,
            Self::Range(range) => ctx.range_view(range).get(0, 0),
        }
    }
}

pub fn eval(expr: &Expr, ctx: &dyn EvalContext) -> Value {
    eval_result(expr, ctx).into_value(ctx)
}

pub fn eval_result(expr: &Expr, ctx: &dyn EvalContext) -> EvalResult {
    match expr {
        Expr::Literal(value) => EvalResult::Value(value.clone()),
        Expr::Ref(cell_ref) => EvalResult::Value(ctx.cell_value(*cell_ref)),
        Expr::Range(_) => EvalResult::Value(Value::Error(ErrorValue::Value)),
        Expr::Unary { op, rhs } => {
            let value = eval(rhs, ctx);
            match value.as_error() {
                Some(error) => EvalResult::Value(Value::Error(error)),
                None => match to_number(&value) {
                    Ok(number) => match op {
                        UnaryOp::Neg => EvalResult::Value(Value::Number(-number)),
                        UnaryOp::Pos => EvalResult::Value(Value::Number(number)),
                        UnaryOp::Percent => EvalResult::Value(Value::Number(number / 100.0)),
                    },
                    Err(error) => EvalResult::Value(Value::Error(error)),
                },
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs = eval(lhs, ctx);
            if let Some(error) = lhs.as_error() {
                return EvalResult::Value(Value::Error(error));
            }
            let rhs = eval(rhs, ctx);
            if let Some(error) = rhs.as_error() {
                return EvalResult::Value(Value::Error(error));
            }
            EvalResult::Value(eval_binary(*op, &lhs, &rhs))
        }
        Expr::Call { name, args } => {
            let Some(function) = ctx.function(name) else {
                return EvalResult::Value(Value::Error(ErrorValue::Name));
            };

            let argc = args.len();
            let (min, max) = function.arity();
            if argc < min || max.is_some_and(|max| argc > max) {
                return EvalResult::Value(Value::Error(ErrorValue::Value));
            }

            let mut evaluated = Vec::with_capacity(args.len());
            for (index, arg) in args.iter().enumerate() {
                match arg {
                    Expr::Range(range) => evaluated.push(Arg::Range(ctx.range_view(*range))),
                    Expr::Ref(cell_ref)
                        if function.argument_mode(index) == FunctionArgMode::Reference =>
                    {
                        evaluated.push(Arg::Range(ctx.range_view(RangeRef {
                            start: *cell_ref,
                            end: *cell_ref,
                        })));
                    }
                    _ => match eval_result(arg, ctx) {
                        EvalResult::Value(value) => evaluated.push(Arg::Value(value)),
                        EvalResult::Range(range) => {
                            evaluated.push(Arg::Range(ctx.range_view(range)));
                        }
                    },
                }
            }

            if !function_receives_error_values(function, name) {
                if let Some(error) = evaluated.iter().find_map(Arg::first_error) {
                    return EvalResult::Value(Value::Error(error));
                }
            }

            let fn_ctx = FnContext::new(ctx);
            match function.call_result(&evaluated, &fn_ctx) {
                EvalResult::Range(range) => {
                    ctx.record_dynamic_range(range);
                    EvalResult::Range(range)
                }
                result => result,
            }
        }
    }
}

/// A worksheet function argument.
///
/// Function authors receive either an already evaluated scalar value or a lazy
/// range view. Ranges are intentionally not materialized at the call boundary.
#[derive(Clone)]
pub enum Arg<'a> {
    Value(Value),
    Range(RangeView<'a>),
}

impl<'a> Arg<'a> {
    /// Return the scalar value for this argument.
    ///
    /// For ranges this uses the top-left cell, matching the scalar helper path
    /// used by functions that accept either a value or a range.
    pub fn as_value(&self) -> Value {
        match self {
            Self::Value(value) => value.clone(),
            Self::Range(range) => range.get(0, 0),
        }
    }

    /// First error contained by this argument in left-to-right evaluation order.
    pub fn first_error(&self) -> Option<ErrorValue> {
        match self {
            Self::Value(value) => value.as_error(),
            Self::Range(range) => range.iter().find_map(|value| value.as_error()),
        }
    }
}

/// Lazy, read-only view over a rectangular cell range.
///
/// Iteration is row-major and values are fetched from the underlying
/// `EvalContext` on demand.
#[derive(Clone)]
pub struct RangeView<'a> {
    ctx: &'a dyn EvalContext,
    range: RangeRef,
}

impl<'a> RangeView<'a> {
    pub fn new(ctx: &'a dyn EvalContext, range: RangeRef) -> Self {
        Self { ctx, range }
    }

    pub fn rows(&self) -> u32 {
        let (start, end) = self.normalized_bounds();
        end.row - start.row + 1
    }

    pub fn cols(&self) -> u32 {
        let (start, end) = self.normalized_bounds();
        end.col - start.col + 1
    }

    pub fn top_left(&self) -> crate::model::Coord {
        self.normalized_bounds().0
    }

    pub fn range_ref(&self) -> RangeRef {
        self.range
    }

    pub fn original_bounds(&self) -> (CellRef, CellRef) {
        (self.range.start, self.range.end)
    }

    pub fn normalized_bounds(&self) -> (crate::model::Coord, crate::model::Coord) {
        let start = self.range.start.coord();
        let end = self.range.end.coord();
        (
            crate::model::Coord {
                row: start.row.min(end.row),
                col: start.col.min(end.col),
            },
            crate::model::Coord {
                row: start.row.max(end.row),
                col: start.col.max(end.col),
            },
        )
    }

    pub fn get(&self, r: u32, c: u32) -> Value {
        if r >= self.rows() || c >= self.cols() {
            return Value::Error(ErrorValue::Ref);
        }
        let (start, _) = self.normalized_bounds();
        self.ctx.cell_value(CellRef {
            sheet: self.range.start.sheet,
            col: start.col + c,
            row: start.row + r,
            col_abs: false,
            row_abs: false,
        })
    }

    pub fn iter(&self) -> RangeIter<'_, 'a> {
        RangeIter {
            view: self,
            next: 0,
        }
    }
}

pub struct RangeIter<'view, 'ctx> {
    view: &'view RangeView<'ctx>,
    next: u32,
}

impl<'view, 'ctx> Iterator for RangeIter<'view, 'ctx> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        let rows = self.view.rows();
        let cols = self.view.cols();
        if self.next >= rows * cols {
            return None;
        }
        let row = self.next / cols;
        let col = self.next % cols;
        self.next += 1;
        Some(self.view.get(row, col))
    }
}

pub struct FnContext<'a> {
    ctx: &'a dyn EvalContext,
}

impl<'a> FnContext<'a> {
    pub fn new(ctx: &'a dyn EvalContext) -> Self {
        Self { ctx }
    }

    /// Coerce a scalar using the arithmetic operator discipline.
    pub fn to_number(&self, v: &Value) -> Result<f64, ErrorValue> {
        to_number(v)
    }

    /// Coerce a scalar using the concatenation/text discipline.
    pub fn to_text(&self, v: &Value) -> Result<String, ErrorValue> {
        to_text(v)
    }

    /// Coerce a scalar to a boolean for logical functions.
    pub fn to_bool(&self, v: &Value) -> Result<bool, ErrorValue> {
        to_bool(v)
    }

    /// Coerce a scalar to an Excel serial date number.
    pub fn to_serial_date(&self, v: &Value) -> Result<f64, ErrorValue> {
        self.to_number(v)
    }

    /// Iterate numeric values for aggregation functions.
    ///
    /// Direct scalar arguments use arithmetic coercion, so numeric text and
    /// booleans count. Range arguments ignore text, booleans, and blanks but
    /// the production evaluator has already propagated range errors before
    /// ordinary functions are called. Direct function tests that bypass `eval`
    /// can use `try_iter_numbers` to assert error behavior.
    pub fn iter_numbers(&self, arg: &Arg<'_>) -> impl Iterator<Item = f64> {
        self.number_values(arg).unwrap_or_default().into_iter()
    }

    /// Fallible form of `iter_numbers` for functions that are called directly.
    pub fn try_iter_numbers(&self, arg: &Arg<'_>) -> Result<impl Iterator<Item = f64>, ErrorValue> {
        Ok(self.number_values(arg)?.into_iter())
    }

    /// Collect numeric aggregation values. Prefer `iter_numbers` in new code.
    pub fn number_values(&self, arg: &Arg<'_>) -> Result<Vec<f64>, ErrorValue> {
        match arg {
            Arg::Value(value) => self.to_number(value).map(|number| vec![number]),
            Arg::Range(range) => {
                let mut numbers = Vec::new();
                for value in range.iter() {
                    match value {
                        Value::Error(error) => return Err(error),
                        Value::Number(number) => numbers.push(number),
                        Value::Text(_) | Value::Boolean(_) | Value::Blank => {}
                    }
                }
                Ok(numbers)
            }
        }
    }

    /// Count numeric aggregation values.
    pub fn count_numbers(&self, arg: &Arg<'_>) -> usize {
        self.number_values(arg)
            .map(|values| values.len())
            .unwrap_or(0)
    }

    /// Fallible form of `count_numbers` for direct function tests.
    pub fn try_count_numbers(&self, arg: &Arg<'_>) -> Result<usize, ErrorValue> {
        self.number_values(arg).map(|values| values.len())
    }

    /// Count non-blank cells/values for COUNTA-style functions.
    pub fn count_nonblank(&self, arg: &Arg<'_>) -> usize {
        match arg {
            Arg::Value(Value::Blank) => 0,
            Arg::Value(_) => 1,
            Arg::Range(range) => range
                .iter()
                .filter(|value| !matches!(value, Value::Blank))
                .count(),
        }
    }

    /// Match a value against a spreadsheet criteria expression.
    ///
    /// Supports comparison prefixes (`>`, `>=`, `<`, `<=`, `=`, `<>`) and
    /// case-insensitive `*`/`?` wildcards for text criteria.
    pub fn matches_criteria(&self, cell: &Value, criteria: &Value) -> bool {
        matches_criteria_value(cell, criteria)
    }

    /// Compare two scalar values using the same equality semantics as `=`.
    pub fn values_equal(&self, lhs: &Value, rhs: &Value) -> Result<bool, ErrorValue> {
        compare_values(BinaryOp::Eq, lhs, rhs)
    }

    pub fn today_serial(&self) -> f64 {
        self.ctx.today_serial()
    }

    pub fn now_serial(&self) -> f64 {
        self.ctx.now_serial()
    }

    pub fn rand(&self) -> f64 {
        self.ctx.rand()
    }

    pub fn date_system(&self) -> DateSystem {
        self.ctx.date_system()
    }

    pub fn current_cell(&self) -> CellId {
        self.ctx.current_cell()
    }
}

fn eval_binary(op: BinaryOp, lhs: &Value, rhs: &Value) -> Value {
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
            let lhs = match to_number(lhs) {
                Ok(number) => number,
                Err(error) => return Value::Error(error),
            };
            let rhs = match to_number(rhs) {
                Ok(number) => number,
                Err(error) => return Value::Error(error),
            };
            match op {
                BinaryOp::Add => Value::Number(lhs + rhs),
                BinaryOp::Sub => Value::Number(lhs - rhs),
                BinaryOp::Mul => Value::Number(lhs * rhs),
                BinaryOp::Div if rhs == 0.0 => Value::Error(ErrorValue::Div0),
                BinaryOp::Div => Value::Number(lhs / rhs),
                BinaryOp::Pow => Value::Number(lhs.powf(rhs)),
                _ => unreachable!(),
            }
        }
        BinaryOp::Concat => {
            let lhs = match to_text(lhs) {
                Ok(text) => text,
                Err(error) => return Value::Error(error),
            };
            let rhs = match to_text(rhs) {
                Ok(text) => text,
                Err(error) => return Value::Error(error),
            };
            Value::Text(format!("{lhs}{rhs}"))
        }
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
            match compare_values(op, lhs, rhs) {
                Ok(value) => Value::Boolean(value),
                Err(error) => Value::Error(error),
            }
        }
    }
}

fn to_number(value: &Value) -> Result<f64, ErrorValue> {
    match value {
        Value::Number(number) => Ok(*number),
        Value::Boolean(true) => Ok(1.0),
        Value::Boolean(false) => Ok(0.0),
        Value::Blank => Ok(0.0),
        Value::Text(text) => text.trim().parse::<f64>().map_err(|_| ErrorValue::Value),
        Value::Error(error) => Err(*error),
    }
}

fn to_text(value: &Value) -> Result<String, ErrorValue> {
    match value {
        Value::Number(number) => {
            if number.fract() == 0.0 && number.is_finite() {
                Ok(format!("{number:.0}"))
            } else {
                Ok(format!("{number}"))
            }
        }
        Value::Text(text) => Ok(text.clone()),
        Value::Boolean(true) => Ok("TRUE".to_string()),
        Value::Boolean(false) => Ok("FALSE".to_string()),
        Value::Blank => Ok(String::new()),
        Value::Error(error) => Err(*error),
    }
}

fn to_bool(value: &Value) -> Result<bool, ErrorValue> {
    match value {
        Value::Boolean(value) => Ok(*value),
        Value::Number(number) => Ok(*number != 0.0),
        Value::Text(text) if text.eq_ignore_ascii_case("TRUE") => Ok(true),
        Value::Text(text) if text.eq_ignore_ascii_case("FALSE") => Ok(false),
        Value::Blank => Ok(false),
        Value::Text(_) => Err(ErrorValue::Value),
        Value::Error(error) => Err(*error),
    }
}

fn compare_values(op: BinaryOp, lhs: &Value, rhs: &Value) -> Result<bool, ErrorValue> {
    let ordering = compare_order(lhs, rhs)?;
    Ok(match op {
        BinaryOp::Eq => ordering == std::cmp::Ordering::Equal,
        BinaryOp::Ne => ordering != std::cmp::Ordering::Equal,
        BinaryOp::Lt => ordering == std::cmp::Ordering::Less,
        BinaryOp::Gt => ordering == std::cmp::Ordering::Greater,
        BinaryOp::Le => ordering != std::cmp::Ordering::Greater,
        BinaryOp::Ge => ordering != std::cmp::Ordering::Less,
        _ => unreachable!(),
    })
}

fn compare_order(lhs: &Value, rhs: &Value) -> Result<std::cmp::Ordering, ErrorValue> {
    use std::cmp::Ordering;

    let (lhs, rhs) = ComparisonValue::coerce_pair(lhs, rhs)?;
    let lhs_rank = lhs.rank();
    let rhs_rank = rhs.rank();
    if lhs_rank != rhs_rank {
        return Ok(lhs_rank.cmp(&rhs_rank));
    }

    Ok(match (lhs, rhs) {
        (ComparisonValue::Number(lhs), ComparisonValue::Number(rhs)) => {
            lhs.partial_cmp(&rhs).unwrap_or(Ordering::Equal)
        }
        (ComparisonValue::Text(lhs), ComparisonValue::Text(rhs)) => lhs.cmp(&rhs),
        (ComparisonValue::Boolean(lhs), ComparisonValue::Boolean(rhs)) => lhs.cmp(&rhs),
        _ => Ordering::Equal,
    })
}

enum ComparisonValue {
    Number(f64),
    Text(String),
    Boolean(bool),
}

impl ComparisonValue {
    fn coerce_pair(lhs: &Value, rhs: &Value) -> Result<(Self, Self), ErrorValue> {
        if let Value::Error(error) = lhs {
            return Err(*error);
        }
        if let Value::Error(error) = rhs {
            return Err(*error);
        }

        Ok(match (lhs, rhs) {
            (Value::Blank, Value::Text(rhs)) => (
                Self::Text(String::new()),
                Self::Text(rhs.to_ascii_lowercase()),
            ),
            (Value::Text(lhs), Value::Blank) => (
                Self::Text(lhs.to_ascii_lowercase()),
                Self::Text(String::new()),
            ),
            (Value::Blank, Value::Blank) => (Self::Number(0.0), Self::Number(0.0)),
            (Value::Blank, rhs) => (Self::Number(0.0), Self::from_nonblank(rhs)?),
            (lhs, Value::Blank) => (Self::from_nonblank(lhs)?, Self::Number(0.0)),
            (lhs, rhs) => (Self::from_nonblank(lhs)?, Self::from_nonblank(rhs)?),
        })
    }

    fn from_nonblank(value: &Value) -> Result<Self, ErrorValue> {
        match value {
            Value::Number(number) => Ok(Self::Number(*number)),
            Value::Text(text) => Ok(Self::Text(text.to_ascii_lowercase())),
            Value::Boolean(value) => Ok(Self::Boolean(*value)),
            Value::Error(error) => Err(*error),
            Value::Blank => Ok(Self::Number(0.0)),
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Self::Number(_) => 0,
            Self::Text(_) => 1,
            Self::Boolean(_) => 2,
        }
    }
}

fn function_receives_error_values(function: &dyn Function, name: &str) -> bool {
    if matches!(
        name,
        // Error-trapping/introspection functions from SPEC.md §3.4.2.
        "IFERROR" | "IFNA" | "ISERROR" | "ISERR" | "ISNA" | "ISBLANK" | "ISNUMBER" | "ISTEXT"
            | "ISNONTEXT" | "ISLOGICAL" | "ERROR.TYPE" | "N" | "TYPE"
            // Branch-selection functions need to inspect candidate values so
            // an error in an unchosen branch does not preempt the selected result.
            | "IF" | "IFS" | "SWITCH" | "CHOOSE"
            // NPV ignores error values in cash-flow arguments and ranges while
            // still validating the rate argument in its implementation.
            | "NPV"
    ) {
        return true;
    }

    // Reference-introspection functions opt in through metadata, so the
    // evaluator does not materialize referenced cells during pre-scan.
    let (_, max) = function.arity();
    let limit = max.unwrap_or(16);
    (0..limit).any(|index| function.argument_mode(index) == FunctionArgMode::Reference)
}

fn matches_criteria_value(cell: &Value, criteria: &Value) -> bool {
    if matches!(cell, Value::Error(_)) || matches!(criteria, Value::Error(_)) {
        return false;
    }

    let Value::Text(criteria_text) = criteria else {
        return compare_values(BinaryOp::Eq, cell, criteria).unwrap_or(false);
    };

    let (op, operand) = parse_criteria_operator(criteria_text);
    if uses_wildcards(operand) && matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
        let Ok(text) = to_text(cell) else {
            return false;
        };
        let matched = wildcard_match(operand, &text);
        return if op == BinaryOp::Ne {
            !matched
        } else {
            matched
        };
    }

    let rhs = criteria_operand_value(cell, operand);
    compare_values(op, cell, &rhs).unwrap_or(false)
}

fn parse_criteria_operator(criteria: &str) -> (BinaryOp, &str) {
    for (prefix, op) in [
        (">=", BinaryOp::Ge),
        ("<=", BinaryOp::Le),
        ("<>", BinaryOp::Ne),
        (">", BinaryOp::Gt),
        ("<", BinaryOp::Lt),
        ("=", BinaryOp::Eq),
    ] {
        if let Some(rest) = criteria.strip_prefix(prefix) {
            return (op, rest);
        }
    }
    (BinaryOp::Eq, criteria)
}

fn criteria_operand_value(cell: &Value, operand: &str) -> Value {
    let trimmed = operand.trim();
    if trimmed.eq_ignore_ascii_case("TRUE") {
        return Value::Boolean(true);
    }
    if trimmed.eq_ignore_ascii_case("FALSE") {
        return Value::Boolean(false);
    }
    if !matches!(cell, Value::Text(_)) {
        if let Ok(number) = trimmed.parse::<f64>() {
            return Value::Number(number);
        }
    }
    Value::Text(operand.to_string())
}

fn uses_wildcards(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let text: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let mut matched = vec![vec![false; text.len() + 1]; pattern.len() + 1];
    matched[0][0] = true;

    for i in 1..=pattern.len() {
        if pattern[i - 1] == '*' {
            matched[i][0] = matched[i - 1][0];
        }
    }

    for i in 1..=pattern.len() {
        for j in 1..=text.len() {
            matched[i][j] = match pattern[i - 1] {
                '*' => matched[i - 1][j] || matched[i][j - 1],
                '?' => matched[i - 1][j - 1],
                ch => ch == text[j - 1] && matched[i - 1][j - 1],
            };
        }
    }

    matched[pattern.len()][text.len()]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn scalar_coercion_matches_slice_contract() {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        assert_eq!(fn_ctx.to_number(&Value::Text("5".to_string())), Ok(5.0));
        assert_eq!(fn_ctx.to_number(&Value::Boolean(true)), Ok(1.0));
        assert_eq!(fn_ctx.to_number(&Value::Blank), Ok(0.0));
        assert_eq!(
            fn_ctx.to_number(&Value::Text("x".to_string())),
            Err(ErrorValue::Value)
        );
    }

    #[test]
    fn criteria_matching_supports_comparisons_and_wildcards() {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert!(fn_ctx.matches_criteria(&Value::Number(6.0), &Value::Text(">5".to_string())));
        assert!(fn_ctx.matches_criteria(
            &Value::Text("Apple".to_string()),
            &Value::Text("a*".to_string())
        ));
        assert!(fn_ctx.matches_criteria(
            &Value::Text("beta".to_string()),
            &Value::Text("<>a*".to_string())
        ));
        assert!(!fn_ctx.matches_criteria(
            &Value::Text("alpha".to_string()),
            &Value::Text("<>a*".to_string())
        ));
    }

    #[test]
    fn value_equality_uses_binary_comparison_semantics() {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            fn_ctx.values_equal(&Value::Text("A".to_string()), &Value::Text("a".to_string())),
            Ok(true)
        );
        assert_eq!(
            fn_ctx.values_equal(&Value::Blank, &Value::Number(0.0)),
            Ok(true)
        );
        assert_eq!(
            fn_ctx.values_equal(&Value::Error(ErrorValue::Div0), &Value::Number(0.0)),
            Err(ErrorValue::Div0)
        );
    }

    #[test]
    fn environment_hooks_delegate_to_eval_context() {
        let ctx = EnvironmentContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(fn_ctx.today_serial(), 45_000.0);
        assert_eq!(fn_ctx.now_serial(), 45_000.25);
        assert_eq!(fn_ctx.rand(), 0.25);
        assert_eq!(fn_ctx.current_cell(), EnvironmentContext::current());
    }

    #[test]
    fn function_error_propagation_honors_trap_names() {
        let ctx = FunctionContext;

        let regular = crate::syntax::parse("REGULAR(1/0)").unwrap();
        assert_eq!(eval(&regular, &ctx), Value::Error(ErrorValue::Div0));

        let iferror = crate::syntax::parse("IFERROR(1/0,99)").unwrap();
        assert_eq!(eval(&iferror, &ctx), Value::Number(99.0));

        for name in ["ISBLANK", "ISNUMBER", "ISTEXT", "ISNONTEXT", "ISLOGICAL"] {
            let expr = crate::syntax::parse(&format!("{name}(1/0)")).unwrap();
            assert_eq!(eval(&expr, &ctx), Value::Boolean(false), "{name}");
        }

        for name in ["IFS", "SWITCH"] {
            let expr = crate::syntax::parse(&format!("{name}(1/0)")).unwrap();
            assert_eq!(eval(&expr, &ctx), Value::Number(77.0), "{name}");
        }
    }

    #[test]
    fn reference_coordinate_functions_receive_cell_refs_as_ranges() {
        let ctx = ReferenceCoordinateContext;

        let row = crate::syntax::parse("ROW(A5)").unwrap();
        assert_eq!(eval(&row, &ctx), Value::Number(5.0));

        let column = crate::syntax::parse("COLUMN(D10)").unwrap();
        assert_eq!(eval(&column, &ctx), Value::Number(4.0));
    }

    #[test]
    fn reference_coordinate_functions_receive_normalized_range_metadata() {
        let ctx = ReferenceCoordinateContext;

        let row = crate::syntax::parse("ROW(B4:A1)").unwrap();
        assert_eq!(eval(&row, &ctx), Value::Number(1.0));

        let column = crate::syntax::parse("COLUMN(D4:B1)").unwrap();
        assert_eq!(eval(&column, &ctx), Value::Number(2.0));
    }

    #[test]
    fn ordinary_functions_still_receive_cell_refs_as_values() {
        let ctx = ValueLoweringContext;

        let expr = crate::syntax::parse("ECHO(A1)").unwrap();
        assert_eq!(eval(&expr, &ctx), Value::Number(42.0));
    }

    #[test]
    fn reference_results_scalarize_to_top_left_value() {
        let ctx = ReferenceReturnContext::new()
            .with_cell("B2", Value::Number(42.0))
            .with_cell("C3", Value::Number(99.0));

        let expr = crate::syntax::parse("REF(B2:C3)").unwrap();
        assert_eq!(eval(&expr, &ctx), Value::Number(42.0));
    }

    #[test]
    fn reference_results_are_forwarded_to_range_functions() {
        let ctx = ReferenceReturnContext::new()
            .with_cell("A1", Value::Number(1.0))
            .with_cell("B1", Value::Number(2.0))
            .with_cell("A2", Value::Number(3.0))
            .with_cell("B2", Value::Number(4.0));

        for (formula, expected) in [
            ("SUM(REF(A1:B2))", Value::Number(10.0)),
            ("ROWS(REF(A1:B2))", Value::Number(2.0)),
            ("COLUMNS(REF(A1:B2))", Value::Number(2.0)),
            ("INDEX(REF(A1:B2),2,2)", Value::Number(4.0)),
        ] {
            let expr = crate::syntax::parse(formula).unwrap();
            assert_eq!(eval(&expr, &ctx), expected, "{formula}");
        }
    }

    #[test]
    fn range_view_exposes_original_and_normalized_bounds() {
        let ctx = DummyContext;
        let range_ref = RangeRef {
            start: cell_ref_at(3, 1),
            end: cell_ref_at(0, 0),
        };
        let view = RangeView::new(&ctx, range_ref);

        assert_eq!(view.range_ref(), range_ref);
        assert_eq!(view.original_bounds(), (range_ref.start, range_ref.end));
        assert_eq!(
            view.normalized_bounds(),
            (
                crate::model::Coord { row: 0, col: 0 },
                crate::model::Coord { row: 3, col: 1 },
            )
        );
    }

    fn cell_ref_at(row: u32, col: u32) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    struct DummyContext;

    impl EvalContext for DummyContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, _name: &str) -> Option<&dyn Function> {
            None
        }

        fn date_system(&self) -> DateSystem {
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId {
                sheet: 0,
                coord: crate::model::Coord { row: 0, col: 0 },
            }
        }
    }

    struct EnvironmentContext;

    impl EnvironmentContext {
        fn current() -> CellId {
            CellId {
                sheet: 0,
                coord: crate::model::Coord { row: 1, col: 2 },
            }
        }
    }

    impl EvalContext for EnvironmentContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, _name: &str) -> Option<&dyn Function> {
            None
        }

        fn date_system(&self) -> DateSystem {
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            Self::current()
        }

        fn today_serial(&self) -> f64 {
            45_000.0
        }

        fn now_serial(&self) -> f64 {
            45_000.25
        }

        fn rand(&self) -> f64 {
            0.25
        }
    }

    struct FunctionContext;

    impl EvalContext for FunctionContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            match name {
                "REGULAR" => Some(&REGULAR_FUNCTION),
                "IFERROR" => Some(&IFERROR_FUNCTION),
                "ISBLANK" | "ISNUMBER" | "ISTEXT" | "ISNONTEXT" | "ISLOGICAL" => {
                    Some(&IS_PREDICATE_STUB)
                }
                "IFS" | "SWITCH" => Some(&BRANCH_SELECTOR_STUB),
                _ => None,
            }
        }

        fn date_system(&self) -> DateSystem {
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId {
                sheet: 0,
                coord: crate::model::Coord { row: 0, col: 0 },
            }
        }
    }

    struct ReferenceCoordinateContext;

    impl EvalContext for ReferenceCoordinateContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            panic!("ROW and COLUMN should not materialize referenced cells")
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            match name {
                "ROW" => Some(&ROW_COORDINATE_FUNCTION),
                "COLUMN" => Some(&COLUMN_COORDINATE_FUNCTION),
                _ => None,
            }
        }

        fn date_system(&self) -> DateSystem {
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId {
                sheet: 0,
                coord: crate::model::Coord { row: 0, col: 0 },
            }
        }
    }

    struct ValueLoweringContext;

    impl EvalContext for ValueLoweringContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Number(42.0)
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            match name {
                "ECHO" => Some(&ECHO_FUNCTION),
                _ => None,
            }
        }

        fn date_system(&self) -> DateSystem {
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId {
                sheet: 0,
                coord: crate::model::Coord { row: 0, col: 0 },
            }
        }
    }

    struct ReferenceReturnContext {
        cells: HashMap<crate::model::Coord, Value>,
        registry: HashMap<&'static str, &'static dyn Function>,
    }

    impl ReferenceReturnContext {
        fn new() -> Self {
            let mut registry = crate::functions::build_registry();
            registry.insert("REF", &REFERENCE_RESULT_FUNCTION);
            Self {
                cells: HashMap::new(),
                registry,
            }
        }

        fn with_cell(mut self, addr: &str, value: Value) -> Self {
            self.cells
                .insert(crate::model::Coord::from_a1(addr).unwrap(), value);
            self
        }
    }

    impl EvalContext for ReferenceReturnContext {
        fn cell_value(&self, r: CellRef) -> Value {
            if r.sheet.unwrap_or(0) != 0 {
                return Value::Error(ErrorValue::Ref);
            }
            self.cells.get(&r.coord()).cloned().unwrap_or(Value::Blank)
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            self.registry.get(name).copied()
        }

        fn date_system(&self) -> DateSystem {
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId {
                sheet: 0,
                coord: crate::model::Coord { row: 0, col: 0 },
            }
        }
    }

    struct RegularFunction;

    impl Function for RegularFunction {
        fn name(&self) -> &'static str {
            "REGULAR"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }

        fn call(&self, _args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            Value::Number(123.0)
        }
    }

    struct IfErrorFunction;

    impl Function for IfErrorFunction {
        fn name(&self) -> &'static str {
            "IFERROR"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (2, Some(2))
        }

        fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            let value = args[0].as_value();
            if value.as_error().is_some() {
                args[1].as_value()
            } else {
                value
            }
        }
    }

    struct IsPredicateStub;

    impl Function for IsPredicateStub {
        fn name(&self) -> &'static str {
            "IS_PREDICATE_STUB"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }

        fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            Value::Boolean(args[0].as_value().as_error().is_none())
        }
    }

    struct BranchSelectorStub;

    impl Function for BranchSelectorStub {
        fn name(&self) -> &'static str {
            "BRANCH_SELECTOR_STUB"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }

        fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            match args {
                [Arg::Value(Value::Error(_))] => Value::Number(77.0),
                _ => Value::Error(ErrorValue::Value),
            }
        }
    }

    struct RowCoordinateFunction;

    impl Function for RowCoordinateFunction {
        fn name(&self) -> &'static str {
            "ROW"
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

        fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            match args {
                [Arg::Range(range)] => Value::Number(f64::from(range.top_left().row + 1)),
                _ => Value::Error(ErrorValue::Value),
            }
        }
    }

    struct ColumnCoordinateFunction;

    impl Function for ColumnCoordinateFunction {
        fn name(&self) -> &'static str {
            "COLUMN"
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

        fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            match args {
                [Arg::Range(range)] => Value::Number(f64::from(range.top_left().col + 1)),
                _ => Value::Error(ErrorValue::Value),
            }
        }
    }

    struct EchoFunction;

    impl Function for EchoFunction {
        fn name(&self) -> &'static str {
            "ECHO"
        }

        fn arity(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }

        fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
            match args {
                [Arg::Value(value)] => value.clone(),
                _ => Value::Error(ErrorValue::Ref),
            }
        }
    }

    struct ReferenceResultFunction;

    impl Function for ReferenceResultFunction {
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

    static REGULAR_FUNCTION: RegularFunction = RegularFunction;
    static IFERROR_FUNCTION: IfErrorFunction = IfErrorFunction;
    static IS_PREDICATE_STUB: IsPredicateStub = IsPredicateStub;
    static BRANCH_SELECTOR_STUB: BranchSelectorStub = BranchSelectorStub;
    static ROW_COORDINATE_FUNCTION: RowCoordinateFunction = RowCoordinateFunction;
    static COLUMN_COORDINATE_FUNCTION: ColumnCoordinateFunction = ColumnCoordinateFunction;
    static ECHO_FUNCTION: EchoFunction = EchoFunction;
    static REFERENCE_RESULT_FUNCTION: ReferenceResultFunction = ReferenceResultFunction;
}
