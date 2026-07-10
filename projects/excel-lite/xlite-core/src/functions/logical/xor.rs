use crate::functions::prelude::*;

pub struct Xor;

impl Function for Xor {
    fn name(&self) -> &'static str {
        "XOR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(254))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 254 {
            return Value::Error(ErrorValue::Value);
        }

        let mut contributed = false;
        let mut true_count = 0usize;

        for arg in args {
            match arg {
                Arg::Value(value) => {
                    let value = match ctx.to_bool(value) {
                        Ok(value) => value,
                        Err(error) => return Value::Error(error),
                    };
                    contributed = true;
                    if value {
                        true_count += 1;
                    }
                }
                Arg::Range(range) => {
                    for value in range.iter() {
                        match value {
                            Value::Error(error) => return Value::Error(error),
                            Value::Boolean(value) => {
                                contributed = true;
                                if value {
                                    true_count += 1;
                                }
                            }
                            Value::Number(number) => {
                                contributed = true;
                                if number != 0.0 {
                                    true_count += 1;
                                }
                            }
                            Value::Text(_) | Value::Blank => {}
                        }
                    }
                }
            }
        }

        if !contributed {
            Value::Error(ErrorValue::Value)
        } else {
            Value::Boolean(true_count % 2 == 1)
        }
    }
}

static XOR: Xor = Xor;
inventory::submit! { FunctionEntry(&XOR) }

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
        assert_eq!(XOR.name(), "XOR");
        assert_eq!(XOR.arity(), (1, Some(254)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args = vec![Arg::Value(Value::Boolean(true)); 255];

        assert_eq!(XOR.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn returns_true_when_true_count_is_odd() {
        assert_eq!(call_values(vec![Value::Boolean(true)]), Value::Boolean(true));
        assert_eq!(
            call_values(vec![
                Value::Boolean(true),
                Value::Boolean(true),
                Value::Boolean(true),
            ]),
            Value::Boolean(true)
        );
    }

    #[test]
    fn returns_false_when_true_count_is_even() {
        assert_eq!(
            call_values(vec![Value::Boolean(true), Value::Boolean(true)]),
            Value::Boolean(false)
        );
        assert_eq!(
            call_values(vec![Value::Boolean(false), Value::Number(0.0)]),
            Value::Boolean(false)
        );
    }

    #[test]
    fn coerces_direct_scalar_values() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(2.0),
                Value::Text("FALSE".to_string()),
            ]),
            Value::Boolean(true)
        );
        assert_eq!(
            call_values(vec![
                Value::Text("TRUE".to_string()),
                Value::Text("FALSE".to_string()),
            ]),
            Value::Boolean(true)
        );
        assert_eq!(call_values(vec![Value::Blank]), Value::Boolean(false));
    }

    #[test]
    fn propagates_direct_scalar_errors() {
        assert_eq!(
            call_values(vec![Value::Text("yes".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn scans_ranges_for_booleans_and_numbers_while_ignoring_text_and_blanks() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(true)),
            ((0, 1), Value::Text("ignored".to_string())),
            ((1, 0), Value::Number(0.0)),
            ((1, 1), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            XOR.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Boolean(false)
        );
    }

    #[test]
    fn returns_value_when_ranges_have_no_usable_values() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("ignored".to_string())),
            ((0, 1), Value::Text("also ignored".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            XOR.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn direct_scalars_count_as_contributions_even_with_empty_ranges() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            XOR.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Blank),
                ],
                &fn_ctx
            ),
            Value::Boolean(false)
        );
    }

    #[test]
    fn propagates_range_errors() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(false)),
            ((0, 1), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            XOR.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        XOR.call(&args, &fn_ctx)
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
