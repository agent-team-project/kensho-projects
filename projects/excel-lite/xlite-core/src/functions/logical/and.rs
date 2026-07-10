use crate::functions::prelude::*;

pub struct And;

impl Function for And {
    fn name(&self) -> &'static str {
        "AND"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        let mut contributed = false;
        let mut all_true = true;

        for arg in args {
            match arg {
                Arg::Value(value) => {
                    let value = match ctx.to_bool(value) {
                        Ok(value) => value,
                        Err(error) => return Value::Error(error),
                    };
                    contributed = true;
                    if !value {
                        all_true = false;
                    }
                }
                Arg::Range(range) => {
                    for value in range.iter() {
                        match value {
                            Value::Error(error) => return Value::Error(error),
                            Value::Boolean(value) => {
                                contributed = true;
                                if !value {
                                    all_true = false;
                                }
                            }
                            Value::Number(number) => {
                                contributed = true;
                                if number == 0.0 {
                                    all_true = false;
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
            Value::Boolean(all_true)
        }
    }
}

static AND: And = And;
inventory::submit! { FunctionEntry(&AND) }

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
        assert_eq!(AND.name(), "AND");
        assert_eq!(AND.arity(), (1, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args = vec![Arg::Value(Value::Boolean(true)); 256];

        assert_eq!(AND.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn returns_true_when_all_direct_values_are_true() {
        assert_eq!(
            call_values(vec![
                Value::Boolean(true),
                Value::Number(2.0),
                Value::Text("TRUE".to_string()),
            ]),
            Value::Boolean(true)
        );
    }

    #[test]
    fn returns_false_when_any_direct_value_is_false() {
        assert_eq!(
            call_values(vec![
                Value::Boolean(true),
                Value::Number(0.0),
                Value::Text("TRUE".to_string()),
            ]),
            Value::Boolean(false)
        );
        assert_eq!(
            call_values(vec![Value::Text("FALSE".to_string())]),
            Value::Boolean(false)
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
            ((1, 0), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AND.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Boolean(true)
        );
    }

    #[test]
    fn range_false_values_make_the_result_false() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(true)),
            ((0, 1), Value::Number(0.0)),
            ((1, 0), Value::Boolean(false)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AND.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
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
            AND.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn direct_scalars_count_as_contributions_even_with_empty_ranges() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AND.call(
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
            AND.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        AND.call(&args, &fn_ctx)
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
