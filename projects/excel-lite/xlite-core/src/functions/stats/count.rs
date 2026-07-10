use crate::functions::prelude::*;

pub struct Count;

impl Function for Count {
    fn name(&self) -> &'static str {
        "COUNT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        let count: usize = args.iter().map(|arg| count_arg(arg, ctx)).sum();
        Value::Number(count as f64)
    }
}

fn count_arg(arg: &Arg<'_>, ctx: &FnContext<'_>) -> usize {
    match arg {
        Arg::Value(value) => usize::from(counts_scalar(value, ctx)),
        Arg::Range(range) => range
            .iter()
            .filter(|value| matches!(value, Value::Number(_)))
            .count(),
    }
}

fn counts_scalar(value: &Value, ctx: &FnContext<'_>) -> bool {
    match value {
        Value::Number(_) | Value::Boolean(_) => true,
        Value::Text(_) => ctx.to_number(value).is_ok(),
        Value::Blank | Value::Error(_) => false,
    }
}

static COUNT: Count = Count;
inventory::submit! { FunctionEntry(&COUNT) }

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
        assert_eq!(COUNT.arity(), (1, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(COUNT.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn counts_direct_numbers_booleans_and_numeric_text() {
        assert_eq!(
            call_values(vec![
                Value::Number(2.0),
                Value::Boolean(true),
                Value::Boolean(false),
                Value::Text(" 3.5 ".to_string()),
            ]),
            Value::Number(4.0)
        );
    }

    #[test]
    fn ignores_direct_non_numeric_text_blanks_and_errors() {
        assert_eq!(
            call_values(vec![
                Value::Text("apple".to_string()),
                Value::Text(String::new()),
                Value::Blank,
                Value::Error(ErrorValue::Div0),
                Value::Number(1.0),
            ]),
            Value::Number(1.0)
        );
    }

    #[test]
    fn range_counts_only_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Text("3".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(4.0)),
            ((2, 0), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNT.call(&[range_arg(&ctx, (0, 0), (2, 1))], &fn_ctx),
            Value::Number(2.0)
        );
    }

    #[test]
    fn combines_ranges_and_direct_scalars() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Text("ignored".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(4.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNT.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(10.0)),
                    Arg::Value(Value::Text("11".to_string())),
                ],
                &fn_ctx
            ),
            Value::Number(4.0)
        );
    }

    #[test]
    fn blank_only_range_returns_zero() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNT.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(0.0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        COUNT.call(&args, &fn_ctx)
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
