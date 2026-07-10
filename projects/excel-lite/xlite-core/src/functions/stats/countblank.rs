use crate::functions::prelude::*;

pub struct CountBlank;

impl Function for CountBlank {
    fn name(&self) -> &'static str {
        "COUNTBLANK"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Number(count_arg_blanks(&args[0]) as f64)
    }
}

fn count_arg_blanks(arg: &Arg<'_>) -> usize {
    match arg {
        Arg::Value(value) => usize::from(is_blank(value)),
        Arg::Range(range) => range.iter().filter(is_blank).count(),
    }
}

fn is_blank(value: &Value) -> bool {
    match value {
        Value::Blank => true,
        Value::Text(text) => text.is_empty(),
        Value::Number(_) | Value::Boolean(_) | Value::Error(_) => false,
    }
}

static COUNTBLANK: CountBlank = CountBlank;
inventory::submit! { FunctionEntry(&COUNTBLANK) }

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
        assert_eq!(COUNTBLANK.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(COUNTBLANK.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            COUNTBLANK.call(
                &[
                    Arg::Value(Value::Blank),
                    Arg::Value(Value::Text(String::new())),
                ],
                &fn_ctx
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn counts_direct_blank_scalars_as_one_cell_ranges() {
        assert_eq!(call_value(Value::Blank), Value::Number(1.0));
        assert_eq!(
            call_value(Value::Text(String::new())),
            Value::Number(1.0)
        );
    }

    #[test]
    fn ignores_direct_nonblank_scalars() {
        for value in [
            Value::Number(0.0),
            Value::Number(1.0),
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Text(" ".to_string()),
            Value::Text("text".to_string()),
            Value::Error(ErrorValue::Div0),
        ] {
            assert_eq!(call_value(value), Value::Number(0.0));
        }
    }

    #[test]
    fn mixed_range_counts_only_blanks_and_empty_text() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Blank),
            ((0, 1), Value::Number(0.0)),
            ((1, 0), Value::Text(String::new())),
            ((1, 1), Value::Text(" ".to_string())),
            ((2, 0), Value::Boolean(false)),
            ((2, 1), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTBLANK.call(&[range_arg(&ctx, (0, 0), (2, 1))], &fn_ctx),
            Value::Number(2.0)
        );
    }

    #[test]
    fn range_with_no_blank_cells_returns_zero() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((0, 1), Value::Text("text".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Text(" ".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTBLANK.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(0.0)
        );
    }

    #[test]
    fn fully_blank_range_returns_cell_count() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTBLANK.call(&[range_arg(&ctx, (0, 0), (1, 2))], &fn_ctx),
            Value::Number(6.0)
        );
    }

    fn call_value(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        COUNTBLANK.call(&[Arg::Value(value)], &fn_ctx)
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
