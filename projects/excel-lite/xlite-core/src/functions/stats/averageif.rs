use crate::functions::prelude::*;

pub struct AverageIf;

impl Function for AverageIf {
    fn name(&self) -> &'static str {
        "AVERAGEIF"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 2 || args.len() > 3 {
            return Value::Error(ErrorValue::Value);
        }

        let criteria = args[1].as_value();
        if let Some(error) = criteria.as_error() {
            return Value::Error(error);
        }

        let criteria_range = &args[0];
        let average_range = args.get(2).unwrap_or(criteria_range);
        let (rows, cols) = arg_dimensions(criteria_range);
        let mut total = 0.0;
        let mut count = 0usize;

        for row in 0..rows {
            for col in 0..cols {
                let cell = arg_get(criteria_range, row, col);
                if ctx.matches_criteria(&cell, &criteria) {
                    match average_number(average_range, row, col, ctx) {
                        Ok(Some(number)) => {
                            total += number;
                            count += 1;
                        }
                        Ok(None) => {}
                        Err(error) => return Value::Error(error),
                    }
                }
            }
        }

        if count == 0 {
            Value::Error(ErrorValue::Div0)
        } else {
            Value::Number(total / count as f64)
        }
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

fn average_number(
    arg: &Arg<'_>,
    row: u32,
    col: u32,
    ctx: &FnContext<'_>,
) -> Result<Option<f64>, ErrorValue> {
    match arg {
        Arg::Value(value) if row == 0 && col == 0 => ctx.to_number(value).map(Some),
        Arg::Value(_) => Err(ErrorValue::Ref),
        Arg::Range(range) => range_average_number(range.get(row, col)),
    }
}

fn range_average_number(value: Value) -> Result<Option<f64>, ErrorValue> {
    match value {
        Value::Number(number) => Ok(Some(number)),
        Value::Text(_) | Value::Boolean(_) | Value::Blank => Ok(None),
        Value::Error(error) => Err(error),
    }
}

static AVERAGEIF: AverageIf = AverageIf;
inventory::submit! { FunctionEntry(&AVERAGEIF) }

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
    fn reports_averageif_arity() {
        assert_eq!(AVERAGEIF.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_args(vec![Arg::Value(Value::Number(1.0))]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn averages_explicit_average_range_for_matching_cells() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(1.0)),
            ((0, 1), Value::Number(10.0)),
            ((1, 1), Value::Number(20.0)),
            ((2, 1), Value::Number(30.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(1.0)),
                    range_arg(&ctx, (0, 1), (2, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(20.0)
        );
    }

    #[test]
    fn omitted_average_range_averages_matching_range_cells() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(3.0)),
            ((3, 0), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (3, 0)),
                    Arg::Value(Value::Number(2.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn supports_comparison_and_wildcard_criteria() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(3.0)),
            ((3, 0), Value::Number(4.0)),
            ((0, 1), Value::Number(10.0)),
            ((1, 1), Value::Number(20.0)),
            ((2, 1), Value::Number(30.0)),
            ((3, 1), Value::Number(40.0)),
            ((0, 2), text("apple")),
            ((1, 2), text("apricot")),
            ((2, 2), text("banana")),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (3, 0)),
                    Arg::Value(text(">2")),
                    range_arg(&ctx, (0, 1), (3, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(35.0)
        );
        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 2), (2, 2)),
                    Arg::Value(text("ap*")),
                    range_arg(&ctx, (0, 1), (2, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(15.0)
        );
    }

    #[test]
    fn returns_div0_when_no_numeric_average_cells_match() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((0, 1), Value::Number(10.0)),
            ((1, 1), Value::Number(20.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(text(">5")),
                    range_arg(&ctx, (0, 1), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn ignores_text_blanks_and_booleans_in_range_average_cells() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("x")),
            ((1, 0), text("x")),
            ((2, 0), text("x")),
            ((3, 0), text("x")),
            ((0, 1), Value::Number(5.0)),
            ((1, 1), text("ignored")),
            ((2, 1), Value::Boolean(true)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (3, 0)),
                    Arg::Value(text("x")),
                    range_arg(&ctx, (0, 1), (3, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(5.0)
        );
    }

    #[test]
    fn scalar_direct_calls_are_one_by_one_ranges() {
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(5.0)),
                Arg::Value(text(">2")),
            ]),
            Value::Number(5.0)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(5.0)),
                Arg::Value(text(">2")),
                Arg::Value(text("7")),
            ]),
            Value::Number(7.0)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(5.0)),
                Arg::Value(text(">2")),
                Arg::Value(Value::Boolean(true)),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(1.0)),
                Arg::Value(text(">2")),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn larger_average_range_is_top_left_aligned_to_criteria_shape() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("x")),
            ((1, 0), text("x")),
            ((0, 1), Value::Number(10.0)),
            ((1, 1), Value::Number(20.0)),
            ((2, 1), Value::Number(30.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(text("x")),
                    range_arg(&ctx, (0, 1), (2, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(15.0)
        );
    }

    #[test]
    fn smaller_average_range_ref_is_propagated_for_matching_cells() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("x")),
            ((1, 0), text("x")),
            ((0, 1), Value::Number(10.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(text("x")),
                    range_arg(&ctx, (0, 1), (0, 1)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
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
    fn propagates_selected_average_cell_errors_only() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((0, 1), Value::Error(ErrorValue::Div0)),
            ((1, 1), Value::Number(20.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Number(2.0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(20.0)
        );
        assert_eq!(
            AVERAGEIF.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Number(1.0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_args(args: Vec<Arg<'_>>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        AVERAGEIF.call(&args, &fn_ctx)
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
