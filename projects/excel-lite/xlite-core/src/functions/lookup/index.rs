use crate::functions::prelude::*;

const MIN_ARGS: usize = 2;
const MAX_ARGS: usize = 3;

pub struct Index;

impl Function for Index {
    fn name(&self) -> &'static str {
        "INDEX"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let row_num = match coerce_index(&args[1], ctx) {
            Ok(index) => index,
            Err(error) => return Value::Error(error),
        };
        let column_num = match args.get(2) {
            Some(arg) => match coerce_index(arg, ctx) {
                Ok(index) => Some(index),
                Err(error) => return Value::Error(error),
            },
            None => None,
        };

        match &args[0] {
            Arg::Range(range) => select_range(range, row_num, column_num),
            Arg::Value(value) => select_scalar(value, row_num, column_num),
        }
    }
}

fn coerce_index(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let index = ctx.to_number(&arg.as_value())?.trunc();
    if !index.is_finite() {
        return Err(ErrorValue::Value);
    }

    Ok(index)
}

fn select_range(range: &RangeView<'_>, row_num: f64, column_num: Option<f64>) -> Value {
    let rows = range.rows();
    let cols = range.cols();

    match column_num {
        Some(column_num) => {
            let row = match one_based_offset(row_num, rows) {
                Ok(row) => row,
                Err(error) => return Value::Error(error),
            };
            let col = match one_based_offset(column_num, cols) {
                Ok(col) => col,
                Err(error) => return Value::Error(error),
            };
            range.get(row, col)
        }
        None if cols == 1 => {
            let row = match one_based_offset(row_num, rows) {
                Ok(row) => row,
                Err(error) => return Value::Error(error),
            };
            range.get(row, 0)
        }
        None if rows == 1 => {
            let col = match one_based_offset(row_num, cols) {
                Ok(col) => col,
                Err(error) => return Value::Error(error),
            };
            range.get(0, col)
        }
        None => match one_based_offset(row_num, rows) {
            Ok(_) => Value::Error(ErrorValue::Value),
            Err(error) => Value::Error(error),
        },
    }
}

fn select_scalar(value: &Value, row_num: f64, column_num: Option<f64>) -> Value {
    let row = match one_based_offset(row_num, 1) {
        Ok(row) => row,
        Err(error) => return Value::Error(error),
    };
    let col = match column_num {
        Some(column_num) => match one_based_offset(column_num, 1) {
            Ok(col) => col,
            Err(error) => return Value::Error(error),
        },
        None => 0,
    };

    if row == 0 && col == 0 {
        value.clone()
    } else {
        Value::Error(ErrorValue::Ref)
    }
}

fn one_based_offset(index: f64, len: u32) -> Result<u32, ErrorValue> {
    // Excel can return row or column arrays for a zero index. EXCEL-LITE v1 only
    // returns scalars, so those array-producing requests are rejected locally.
    if index == 0.0 {
        return Err(ErrorValue::Value);
    }
    if index < 1.0 || index > f64::from(len) {
        return Err(ErrorValue::Ref);
    }

    Ok(index as u32 - 1)
}

static INDEX: Index = Index;
inventory::submit! { FunctionEntry(&INDEX) }

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
        assert_eq!(INDEX.name(), "INDEX");
        assert_eq!(INDEX.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![num(1.0)]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_values(vec![num(1.0), num(1.0), num(1.0), num(1.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn selects_range_intersection_with_one_based_indexes() {
        let ctx = TestContext::with_cells(vec![((1, 2), text("C2"))]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(2.0)),
                    Arg::Value(num(3.0)),
                ],
                &fn_ctx,
            ),
            text("C2")
        );
    }

    #[test]
    fn supports_single_column_and_single_row_shorthand() {
        let ctx = TestContext::with_cells(vec![
            ((1, 0), text("A2")),
            ((0, 1), text("B1")),
            ((0, 0), text("A1")),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDEX.call(
                &[range_arg(&ctx, (0, 0), (2, 0)), Arg::Value(num(2.0))],
                &fn_ctx,
            ),
            text("A2")
        );
        assert_eq!(
            INDEX.call(
                &[range_arg(&ctx, (0, 0), (0, 2)), Arg::Value(num(2.0))],
                &fn_ctx,
            ),
            text("B1")
        );
    }

    #[test]
    fn treats_direct_scalars_as_one_by_one_arrays() {
        assert_eq!(call_values(vec![num(99.0), num(1.0)]), num(99.0));
        assert_eq!(
            call_values(vec![text("ok"), num(1.0), num(1.0)]),
            text("ok")
        );
        assert_eq!(
            call_values(vec![Value::Error(ErrorValue::Div0), num(1.0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(vec![num(99.0), num(2.0)]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_values(vec![num(99.0), num(1.0), num(2.0)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn returns_selected_blank_cell() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(3.0)),
                    Arg::Value(num(2.0)),
                ],
                &fn_ctx,
            ),
            Value::Blank
        );
    }

    #[test]
    fn rejects_zero_indexes_and_unsupported_array_results() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDEX.call(
                &[range_arg(&ctx, (0, 0), (2, 2)), Arg::Value(num(0.0))],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(2.0)),
                    Arg::Value(num(0.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            INDEX.call(
                &[range_arg(&ctx, (0, 0), (2, 2)), Arg::Value(num(2.0))],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn out_of_bounds_indexes_return_ref() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(4.0)),
                    Arg::Value(num(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(1.0)),
                    Arg::Value(num(4.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            INDEX.call(
                &[range_arg(&ctx, (0, 0), (0, 2)), Arg::Value(num(4.0))],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn truncates_numeric_indexes_toward_zero() {
        let ctx = TestContext::with_cells(vec![((1, 1), num(22.0))]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(2.9)),
                    Arg::Value(num(2.1)),
                ],
                &fn_ctx,
            ),
            num(22.0)
        );
        assert_eq!(
            INDEX.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 2)),
                    Arg::Value(num(-1.2)),
                    Arg::Value(num(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn rejects_non_finite_indexes() {
        assert_eq!(
            call_values(vec![num(99.0), Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![num(99.0), Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_index_coercion_errors_left_to_right() {
        assert_eq!(
            call_values(vec![
                num(99.0),
                text("not numeric"),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![num(99.0), num(1.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        INDEX.call(&args, &fn_ctx)
    }

    fn num(value: f64) -> Value {
        Value::Number(value)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
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
