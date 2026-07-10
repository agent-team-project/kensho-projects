use crate::functions::prelude::*;

pub struct Large;

impl Function for Large {
    fn name(&self) -> &'static str {
        "LARGE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let mut numbers: Vec<_> = match ctx.try_iter_numbers(&args[0]) {
            Ok(values) => values.collect(),
            Err(error) => return Value::Error(error),
        };

        let k = match ctx.to_number(&args[1].as_value()) {
            Ok(k) => k,
            Err(_) => return Value::Error(ErrorValue::Value),
        };

        if numbers.is_empty() || !k.is_finite() || k.fract() != 0.0 || k <= 0.0 {
            return Value::Error(ErrorValue::Num);
        }

        let index = k as usize;
        if index > numbers.len() {
            return Value::Error(ErrorValue::Num);
        }

        numbers.sort_by(|left, right| right.total_cmp(left));

        Value::Number(numbers[index - 1])
    }
}

static LARGE: Large = Large;
inventory::submit! { FunctionEntry(&LARGE) }

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
    fn reports_exact_arity() {
        assert_eq!(LARGE.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_args(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_args(vec![Arg::Value(Value::Number(1.0))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_kth_largest_value_with_duplicates_preserved() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(4.0)),
            ((1, 0), Value::Number(9.0)),
            ((2, 0), Value::Number(9.0)),
            ((3, 0), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (3, 0)),
                    Arg::Value(Value::Number(2.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(9.0)
        );
        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (3, 0)),
                    Arg::Value(Value::Number(3.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(4.0)
        );
    }

    #[test]
    fn k_one_returns_maximum_and_k_count_returns_minimum() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(5.0)),
            ((1, 0), Value::Number(-3.0)),
            ((2, 0), Value::Number(12.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(12.0)
        );
        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(3.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(-3.0)
        );
    }

    #[test]
    fn direct_scalar_array_is_one_item_data_set() {
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Text(" 7.5 ".to_string())),
                Arg::Value(Value::Number(1.0)),
            ]),
            Value::Number(7.5)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(7.5)),
                Arg::Value(Value::Number(2.0)),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn range_ignores_non_numbers_and_includes_zero() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("100".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((2, 0), Value::Number(0.0)),
            ((4, 0), Value::Number(-2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (4, 0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn empty_numeric_data_returns_num() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("x".to_string())),
            ((1, 0), Value::Boolean(true)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn invalid_k_returns_num() {
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(0.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(-1.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(1.5)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(f64::INFINITY)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(2.0)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_array_coercion_and_range_errors() {
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Text("apple".to_string())),
                Arg::Value(Value::Number(1.0)),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Error(ErrorValue::Div0)),
                Arg::Value(Value::Number(1.0)),
            ]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LARGE.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn k_coercion_errors_return_value() {
        assert_eq!(
            call_value(Value::Number(1.0), Value::Text("apple".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_value(array: Value, k: Value) -> Value {
        call_args(vec![Arg::Value(array), Arg::Value(k)])
    }

    fn call_args(args: Vec<Arg<'_>>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        LARGE.call(&args, &fn_ctx)
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
            CellId {
                sheet: 0,
                coord: Coord { row: 0, col: 0 },
            }
        }
    }
}
