use crate::functions::prelude::*;

pub struct CountIf;

impl Function for CountIf {
    fn name(&self) -> &'static str {
        "COUNTIF"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let criteria = args[1].as_value();
        if let Some(error) = criteria.as_error() {
            return Value::Error(error);
        }

        let range = &args[0];
        let (rows, cols) = arg_dimensions(range);
        let mut count = 0usize;

        for row in 0..rows {
            for col in 0..cols {
                let cell = arg_get(range, row, col);
                if ctx.matches_criteria(&cell, &criteria) {
                    count += 1;
                }
            }
        }

        Value::Number(count as f64)
    }
}

fn arg_dimensions(arg: &Arg<'_>) -> (u32, u32) {
    match arg {
        Arg::Value(_) => (1, 1),
        Arg::Range(range) => (range.rows(), range.cols()),
    }
}

fn arg_get(arg: &Arg<'_>, row: u32, col: u32) -> Value {
    match arg {
        Arg::Value(value) if row == 0 && col == 0 => value.clone(),
        Arg::Value(_) => Value::Error(ErrorValue::Ref),
        Arg::Range(range) => range.get(row, col),
    }
}

static COUNTIF: CountIf = CountIf;
inventory::submit! { FunctionEntry(&COUNTIF) }

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
    fn reports_countif_arity() {
        assert_eq!(COUNTIF.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_args(vec![Arg::Value(Value::Number(1.0))]), Value::Error(ErrorValue::Value));
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
    fn counts_numeric_equality_matches() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(1.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTIF.call(
                &[range_arg(&ctx, (0, 0), (2, 0)), Arg::Value(Value::Number(1.0))],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn supports_comparison_and_text_criteria() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(3.0)),
            ((3, 0), Value::Number(4.0)),
            ((0, 1), text("apple")),
            ((1, 1), text("banana")),
            ((2, 1), text("apple")),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTIF.call(
                &[range_arg(&ctx, (0, 0), (3, 0)), Arg::Value(text(">2"))],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
        assert_eq!(
            COUNTIF.call(
                &[range_arg(&ctx, (0, 1), (2, 1)), Arg::Value(text("apple"))],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn supports_text_wildcard_criteria() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("apple")),
            ((1, 0), text("apricot")),
            ((2, 0), text("banana")),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTIF.call(
                &[range_arg(&ctx, (0, 0), (2, 0)), Arg::Value(text("ap*"))],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn returns_zero_when_no_cells_match() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("apple")),
            ((1, 0), text("apricot")),
            ((2, 0), text("banana")),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTIF.call(
                &[range_arg(&ctx, (0, 0), (2, 0)), Arg::Value(text("z*"))],
                &fn_ctx,
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_direct_criteria_error() {
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Error(ErrorValue::Div0)),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn scalar_direct_calls_are_one_by_one_ranges() {
        assert_eq!(
            call_args(vec![Arg::Value(Value::Number(5.0)), Arg::Value(text(">2"))]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_args(vec![Arg::Value(Value::Number(1.0)), Arg::Value(text(">2"))]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn range_errors_do_not_match_criteria() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Error(ErrorValue::Div0)),
            ((1, 0), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COUNTIF.call(
                &[range_arg(&ctx, (0, 0), (1, 0)), Arg::Value(text(">0"))],
                &fn_ctx,
            ),
            Value::Number(1.0)
        );
    }

    fn call_args(args: Vec<Arg<'_>>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        COUNTIF.call(&args, &fn_ctx)
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
