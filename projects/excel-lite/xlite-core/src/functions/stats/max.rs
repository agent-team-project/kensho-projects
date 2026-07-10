use crate::functions::prelude::*;

pub struct Max;

impl Function for Max {
    fn name(&self) -> &'static str {
        "MAX"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        let mut maximum = None;

        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => {
                    for number in numbers {
                        maximum = Some(match maximum {
                            Some(current) if current >= number => current,
                            _ => number,
                        });
                    }
                }
                Err(error) => return Value::Error(error),
            }
        }

        Value::Number(maximum.unwrap_or(0.0))
    }
}

static MAX: Max = Max;
inventory::submit! { FunctionEntry(&MAX) }

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
    fn reports_excel_arity_limit() {
        assert_eq!(MAX.arity(), (1, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(MAX.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn returns_largest_scalar_number() {
        assert_eq!(
            call_values(vec![
                Value::Number(-3.0),
                Value::Number(7.5),
                Value::Number(2.0),
            ]),
            Value::Number(7.5)
        );
    }

    #[test]
    fn combines_ranges_and_direct_scalars() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Number(12.0)),
            ((1, 0), Value::Number(-4.0)),
            ((1, 1), Value::Text("ignored".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MAX.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(10.0)),
                    Arg::Value(Value::Text("11".to_string())),
                ],
                &fn_ctx,
            ),
            Value::Number(12.0)
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text(" 6.5 ".to_string()),
                Value::Boolean(true),
                Value::Boolean(false),
                Value::Blank,
            ]),
            Value::Number(6.5)
        );
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Text("99".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(7.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MAX.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(7.0)
        );
    }

    #[test]
    fn returns_zero_for_blank_or_text_only_ranges() {
        let blank_ctx = TestContext::default();
        let blank_fn_ctx = FnContext::new(&blank_ctx);

        assert_eq!(
            MAX.call(&[range_arg(&blank_ctx, (0, 0), (1, 1))], &blank_fn_ctx),
            Value::Number(0.0)
        );

        let text_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("3".to_string())),
            ((0, 1), Value::Text("apple".to_string())),
        ]);
        let text_fn_ctx = FnContext::new(&text_ctx);

        assert_eq!(
            MAX.call(&[range_arg(&text_ctx, (0, 0), (0, 1))], &text_fn_ctx),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_aggregation_errors() {
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Text("apple".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        MAX.call(&args, &fn_ctx)
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
