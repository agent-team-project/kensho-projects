use crate::functions::prelude::*;

pub struct CountA;

impl Function for CountA {
    fn name(&self) -> &'static str {
        "COUNTA"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Number(
            args.iter()
                .map(|arg| ctx.count_nonblank(arg))
                .sum::<usize>() as f64,
        )
    }
}

static COUNTA: CountA = CountA;
inventory::submit! { FunctionEntry(&COUNTA) }

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
        assert_eq!(COUNTA.arity(), (1, Some(255)));
    }

    #[test]
    fn counts_direct_nonblank_values() {
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Text(String::new()),
                Value::Boolean(false),
                Value::Error(ErrorValue::Div0),
                Value::Blank,
            ]),
            Value::Number(4.0)
        );
    }

    #[test]
    fn counts_range_values_except_blanks() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((0, 1), Value::Text("text".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTA.call(&[range_arg(&ctx, (0, 0), (2, 1))], &fn_ctx),
            Value::Number(4.0)
        );
    }

    #[test]
    fn returns_zero_for_blank_only_range() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTA.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(0.0)
        );
    }

    #[test]
    fn combines_ranges_and_direct_values() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((2, 0), Value::Text("text".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTA.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Text(String::new())),
                    Arg::Value(Value::Boolean(true)),
                    Arg::Value(Value::Blank),
                ],
                &fn_ctx
            ),
            Value::Number(4.0)
        );
    }

    #[test]
    fn returns_value_for_direct_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let too_many_args = vec![Arg::Value(Value::Number(1.0)); 256];

        assert_eq!(COUNTA.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            COUNTA.call(&too_many_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        COUNTA.call(&args, &fn_ctx)
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
