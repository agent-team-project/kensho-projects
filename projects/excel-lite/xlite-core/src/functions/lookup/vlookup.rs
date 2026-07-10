use std::cmp::Ordering;

use crate::functions::prelude::*;

const MIN_ARGS: usize = 3;
const MAX_ARGS: usize = 4;

pub struct Vlookup;

impl Function for Vlookup {
    fn name(&self) -> &'static str {
        "VLOOKUP"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let lookup_value = args[0].as_value();
        let table = match &args[1] {
            Arg::Range(range) => range,
            Arg::Value(_) => return Value::Error(ErrorValue::Value),
        };

        let return_col = match return_column_index(&args[2], table.cols(), ctx) {
            Ok(index) => index,
            Err(error) => return Value::Error(error),
        };

        let range_lookup = match args.get(3) {
            Some(arg) => match ctx.to_bool(&arg.as_value()) {
                Ok(value) => value,
                Err(error) => return Value::Error(error),
            },
            None => true,
        };

        let matched_row = if range_lookup {
            match approximate_match_row(table, &lookup_value) {
                Ok(row) => row,
                Err(error) => return Value::Error(error),
            }
        } else {
            match exact_match_row(table, &lookup_value) {
                Ok(row) => row,
                Err(error) => return Value::Error(error),
            }
        };

        match matched_row {
            Some(row) => return_value(table, row, return_col),
            None => Value::Error(ErrorValue::Na),
        }
    }
}

fn return_column_index(
    arg: &Arg<'_>,
    table_cols: u32,
    ctx: &FnContext<'_>,
) -> Result<u32, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    let index = number.trunc();
    if !index.is_finite() || index < 1.0 {
        return Err(ErrorValue::Value);
    }
    if index > f64::from(table_cols) {
        return Err(ErrorValue::Ref);
    }

    Ok(index as u32 - 1)
}

fn exact_match_row(table: &RangeView<'_>, lookup_value: &Value) -> Result<Option<u32>, ErrorValue> {
    for row in 0..table.rows() {
        if exact_matches(&table.get(row, 0), lookup_value)? {
            return Ok(Some(row));
        }
    }

    Ok(None)
}

fn exact_matches(left: &Value, right: &Value) -> Result<bool, ErrorValue> {
    if let Some(error) = left.as_error().or_else(|| right.as_error()) {
        return Err(error);
    }

    Ok(match (left, right) {
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::Text(left), Value::Text(right)) => left.eq_ignore_ascii_case(right),
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Blank, Value::Blank) => true,
        _ => false,
    })
}

fn approximate_match_row(
    table: &RangeView<'_>,
    lookup_value: &Value,
) -> Result<Option<u32>, ErrorValue> {
    let mut candidate = None;

    for row in 0..table.rows() {
        match compare_lookup_key(&table.get(row, 0), lookup_value)? {
            Some(Ordering::Greater) => break,
            Some(Ordering::Less | Ordering::Equal) => candidate = Some(row),
            None => {}
        }
    }

    Ok(candidate)
}

fn compare_lookup_key(left: &Value, right: &Value) -> Result<Option<Ordering>, ErrorValue> {
    if let Some(error) = left.as_error().or_else(|| right.as_error()) {
        return Err(error);
    }

    Ok(match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.partial_cmp(right),
        (Value::Text(left), Value::Text(right)) => {
            Some(left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()))
        }
        (Value::Boolean(left), Value::Boolean(right)) => Some(left.cmp(right)),
        (Value::Blank, Value::Blank) => Some(Ordering::Equal),
        _ => None,
    })
}

fn return_value(table: &RangeView<'_>, row: u32, col: u32) -> Value {
    match table.get(row, col) {
        Value::Blank => Value::Number(0.0),
        value => value,
    }
}

static VLOOKUP: Vlookup = Vlookup;
inventory::submit! { FunctionEntry(&VLOOKUP) }

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(VLOOKUP.name(), "VLOOKUP");
        assert_eq!(VLOOKUP.arity(), (3, Some(4)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_args(TestContext::default(), vec![]), err(ErrorValue::Value));
        assert_eq!(
            call_args(
                TestContext::default(),
                vec![
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_args(
                TestContext::default(),
                vec![
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Number(3.0)),
                    Arg::Value(Value::Boolean(false)),
                    Arg::Value(Value::Number(5.0)),
                ],
            ),
            err(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_scalar_table_arguments() {
        assert_eq!(
            call_args(
                TestContext::default(),
                vec![
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                ],
            ),
            err(ErrorValue::Value)
        );
    }

    #[test]
    fn validates_and_truncates_column_index() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("sku".to_string())),
            ((0, 1), Value::Text("first".to_string())),
            ((0, 2), Value::Text("second".to_string())),
        ]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("sku".to_string())),
                    range_arg(&ctx, (0, 0), (0, 2)),
                    Arg::Value(Value::Number(2.9)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("first")
        );
        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("sku".to_string())),
                    range_arg(&ctx, (0, 0), (0, 2)),
                    Arg::Value(Value::Number(0.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("sku".to_string())),
                    range_arg(&ctx, (0, 0), (0, 2)),
                    Arg::Value(Value::Number(f64::INFINITY)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("sku".to_string())),
                    range_arg(&ctx, (0, 0), (0, 2)),
                    Arg::Value(Value::Number(4.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Ref)
        );
    }

    #[test]
    fn exact_mode_returns_first_case_insensitive_text_match() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("SKU-2".to_string())),
            ((0, 1), Value::Text("first".to_string())),
            ((1, 0), Value::Text("sku-2".to_string())),
            ((1, 1), Value::Text("second".to_string())),
        ]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("sku-2".to_string())),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("first")
        );
    }

    #[test]
    fn exact_mode_keeps_numbers_and_numeric_text_distinct() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(42.0)),
            ((0, 1), Value::Text("number".to_string())),
            ((1, 0), Value::Text("42".to_string())),
            ((1, 1), Value::Text("text".to_string())),
        ]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("42".to_string())),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("text")
        );
        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Number(42.0)),
                    range_arg(&ctx, (1, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Na)
        );
    }

    #[test]
    fn approximate_mode_returns_last_comparable_key_less_than_or_equal() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((0, 1), Value::Text("a".to_string())),
            ((1, 0), Value::Number(20.0)),
            ((1, 1), Value::Text("b".to_string())),
            ((2, 0), Value::Number(30.0)),
            ((2, 1), Value::Text("c".to_string())),
        ]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Number(25.0)),
                    range_arg(&ctx, (0, 0), (2, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("b")
        );
        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Number(5.0)),
                    range_arg(&ctx, (0, 0), (2, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            err(ErrorValue::Na)
        );
    }

    #[test]
    fn approximate_mode_compares_only_same_broad_type() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((0, 1), Value::Text("number".to_string())),
            ((1, 0), Value::Text("20".to_string())),
            ((1, 1), Value::Text("text".to_string())),
            ((2, 0), Value::Text("30".to_string())),
            ((2, 1), Value::Text("later text".to_string())),
        ]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Text("25".to_string())),
                    range_arg(&ctx, (0, 0), (2, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("text")
        );
    }

    #[test]
    fn approximate_mode_orders_booleans_false_before_true() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(false)),
            ((0, 1), Value::Text("false row".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Text("true row".to_string())),
        ]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Boolean(true)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("true row")
        );
        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Boolean(false)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("false row")
        );
    }

    #[test]
    fn exact_blank_match_and_blank_return_cell_returns_zero() {
        let ctx = TestContext::with_cells(vec![((0, 0), Value::Blank)]);

        assert_eq!(
            call_vlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Blank),
                    range_arg(&ctx, (0, 0), (0, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            Value::Number(0.0)
        );
    }

    fn call_args<'a>(ctx: TestContext, args: Vec<Arg<'a>>) -> Value {
        let fn_ctx = FnContext::new(&ctx);
        VLOOKUP.call(&args, &fn_ctx)
    }

    fn call_vlookup(ctx: &TestContext, args: Vec<Arg<'_>>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        VLOOKUP.call(&args, &fn_ctx)
    }

    fn range_arg(ctx: &TestContext, start: (u32, u32), end: (u32, u32)) -> Arg<'_> {
        Arg::Range(RangeView::new(
            ctx,
            RangeRef {
                start: cell_ref(start),
                end: cell_ref(end),
            },
        ))
    }

    fn cell_ref((row, col): (u32, u32)) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn err(error: ErrorValue) -> Value {
        Value::Error(error)
    }

    #[derive(Default)]
    struct TestContext {
        cells: HashMap<(u32, u32), Value>,
    }

    impl TestContext {
        fn with_cells(cells: Vec<((u32, u32), Value)>) -> Self {
            Self {
                cells: cells.into_iter().collect(),
            }
        }
    }

    impl EvalContext for TestContext {
        fn cell_value(&self, r: CellRef) -> Value {
            self.cells
                .get(&(r.row, r.col))
                .cloned()
                .unwrap_or(Value::Blank)
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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
