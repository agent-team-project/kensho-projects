use crate::functions::prelude::*;

pub struct Small;

impl Function for Small {
    fn name(&self) -> &'static str {
        "SMALL"
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
            Err(error) => return Value::Error(error),
        };
        if numbers.is_empty() {
            return Value::Error(ErrorValue::Num);
        }
        if !k.is_finite() || k.fract() != 0.0 || k <= 0.0 || k > numbers.len() as f64 {
            return Value::Error(ErrorValue::Num);
        }

        numbers.sort_by(f64::total_cmp);
        Value::Number(numbers[k as usize - 1])
    }
}

static SMALL: Small = Small;
inventory::submit! { FunctionEntry(&SMALL) }

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
        assert_eq!(SMALL.arity(), (2, Some(2)));
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
    fn returns_kth_smallest_and_preserves_duplicates() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(9.0)),
            ((0, 1), Value::Number(3.0)),
            ((1, 0), Value::Number(3.0)),
            ((1, 1), Value::Number(1.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(3.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(3.0)
        );
    }

    #[test]
    fn returns_minimum_for_first_position_and_maximum_for_last_position() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(7.0)),
            ((0, 1), Value::Number(-2.0)),
            ((1, 0), Value::Number(4.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(-2.0)
        );
        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(3.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(7.0)
        );
    }

    #[test]
    fn treats_direct_scalar_array_as_one_item_data_set() {
        assert_eq!(
            call_values(Value::Text(" 4.5 ".to_string()), Value::Number(1.0)),
            Value::Number(4.5)
        );
    }

    #[test]
    fn ignores_range_text_booleans_and_blanks_while_including_zero() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("-100".to_string())),
            ((1, 0), Value::Boolean(false)),
            ((2, 0), Value::Number(0.0)),
            ((4, 0), Value::Number(5.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SMALL.call(
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
    fn returns_num_for_empty_or_no_numeric_array() {
        let blank_ctx = TestContext::default();
        let blank_fn_ctx = FnContext::new(&blank_ctx);

        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&blank_ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &blank_fn_ctx,
            ),
            Value::Error(ErrorValue::Num)
        );

        let text_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("3".to_string())),
            ((0, 1), Value::Boolean(true)),
        ]);
        let text_fn_ctx = FnContext::new(&text_ctx);

        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&text_ctx, (0, 0), (0, 1)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &text_fn_ctx,
            ),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_k_positions() {
        assert_eq!(
            call_values(Value::Number(8.0), Value::Number(0.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(Value::Number(8.0), Value::Number(-1.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(Value::Number(8.0), Value::Number(1.5)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(Value::Number(8.0), Value::Number(2.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(Value::Number(8.0), Value::Number(f64::INFINITY)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_array_errors_and_k_coercion_errors() {
        assert_eq!(
            call_values(Value::Text("apple".to_string()), Value::Number(1.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(Value::Error(ErrorValue::Div0), Value::Number(1.0)),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(Value::Number(1.0), Value::Text("apple".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(Value::Number(1.0), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&ctx, (0, 0), (0, 1)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );

        let blank_ctx = TestContext::default();
        let blank_fn_ctx = FnContext::new(&blank_ctx);
        assert_eq!(
            SMALL.call(
                &[
                    range_arg(&blank_ctx, (0, 0), (0, 0)),
                    Arg::Value(Value::Text("apple".to_string())),
                ],
                &blank_fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(array: Value, k: Value) -> Value {
        call_args(vec![Arg::Value(array), Arg::Value(k)])
    }

    fn call_args(args: Vec<Arg<'_>>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        SMALL.call(&args, &fn_ctx)
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
