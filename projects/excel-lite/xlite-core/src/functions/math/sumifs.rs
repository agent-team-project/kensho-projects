use crate::functions::prelude::*;

pub struct Sumifs;

impl Function for Sumifs {
    fn name(&self) -> &'static str {
        "SUMIFS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 3 || args.len() > 255 || args.len() % 2 == 0 {
            return Value::Error(ErrorValue::Value);
        }

        let sum_range = match &args[0] {
            Arg::Range(range) => range,
            Arg::Value(_) => return Value::Error(ErrorValue::Value),
        };

        let mut criteria = Vec::new();
        for pair in args[1..].chunks_exact(2) {
            let criteria_range = match &pair[0] {
                Arg::Range(range) => range,
                Arg::Value(_) => return Value::Error(ErrorValue::Value),
            };
            if criteria_range.rows() != sum_range.rows() || criteria_range.cols() != sum_range.cols()
            {
                return Value::Error(ErrorValue::Value);
            }

            let criteria_value = pair[1].as_value();
            if let Some(error) = criteria_value.as_error() {
                return Value::Error(error);
            }

            criteria.push((criteria_range, criteria_value));
        }

        let mut total = 0.0;
        for row in 0..sum_range.rows() {
            for col in 0..sum_range.cols() {
                if criteria.iter().all(|(range, criteria_value)| {
                    ctx.matches_criteria(&range.get(row, col), criteria_value)
                }) {
                    match sum_value(sum_range.get(row, col)) {
                        Ok(number) => total += number,
                        Err(error) => return Value::Error(error),
                    }
                }
            }
        }

        Value::Number(total)
    }
}

fn sum_value(value: Value) -> Result<f64, ErrorValue> {
    match value {
        Value::Number(number) => Ok(number),
        Value::Boolean(true) => Ok(1.0),
        Value::Boolean(false) | Value::Blank | Value::Text(_) => Ok(0.0),
        Value::Error(error) => Err(error),
    }
}

static SUMIFS: Sumifs = Sumifs;
inventory::submit! { FunctionEntry(&SUMIFS) }

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
    fn reports_sumifs_arity() {
        assert_eq!(SUMIFS.arity(), (3, Some(255)));
    }

    #[test]
    fn sums_matching_cells_for_single_criterion() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), Value::Number(20.0)),
            ((2, 0), Value::Number(30.0)),
            ((0, 1), text("fruit")),
            ((1, 1), text("veg")),
            ((2, 1), text("fruit")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (2, 0)),
                    range_arg(&ctx, (0, 1), (2, 1)),
                    Arg::Value(text("fruit")),
                ],
            ),
            Value::Number(40.0)
        );
    }

    #[test]
    fn requires_all_criteria_at_the_same_position() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), Value::Number(20.0)),
            ((2, 0), Value::Number(30.0)),
            ((0, 1), text("fruit")),
            ((1, 1), text("fruit")),
            ((2, 1), text("veg")),
            ((0, 2), text("west")),
            ((1, 2), text("east")),
            ((2, 2), text("east")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (2, 0)),
                    range_arg(&ctx, (0, 1), (2, 1)),
                    Arg::Value(text("fruit")),
                    range_arg(&ctx, (0, 2), (2, 2)),
                    Arg::Value(text("east")),
                ],
            ),
            Value::Number(20.0)
        );
    }

    #[test]
    fn supports_comparison_and_wildcard_criteria() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), Value::Number(20.0)),
            ((2, 0), Value::Number(30.0)),
            ((0, 1), Value::Number(1.0)),
            ((1, 1), Value::Number(2.0)),
            ((2, 1), Value::Number(3.0)),
            ((0, 2), text("alpha")),
            ((1, 2), text("alpine")),
            ((2, 2), text("beta")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (2, 0)),
                    range_arg(&ctx, (0, 1), (2, 1)),
                    Arg::Value(text(">=2")),
                    range_arg(&ctx, (0, 2), (2, 2)),
                    Arg::Value(text("al*")),
                ],
            ),
            Value::Number(20.0)
        );
    }

    #[test]
    fn returns_zero_when_no_cells_match() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), Value::Number(20.0)),
            ((0, 1), text("fruit")),
            ((1, 1), text("veg")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    Arg::Value(text("meat")),
                ],
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn applies_sum_cell_value_discipline() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), text("ignored")),
            ((3, 0), Value::Boolean(true)),
            ((4, 0), Value::Boolean(false)),
            ((0, 1), text("x")),
            ((1, 1), text("x")),
            ((2, 1), text("x")),
            ((3, 1), text("x")),
            ((4, 1), text("x")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (4, 0)),
                    range_arg(&ctx, (0, 1), (4, 1)),
                    Arg::Value(text("x")),
                ],
            ),
            Value::Number(11.0)
        );
    }

    #[test]
    fn propagates_sum_cell_and_criteria_argument_errors() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), Value::Error(ErrorValue::Ref)),
            ((0, 1), text("x")),
            ((1, 1), text("x")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    Arg::Value(text("x")),
                ],
            ),
            Value::Error(ErrorValue::Ref)
        );

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    Arg::Value(Value::Error(ErrorValue::Div0)),
                ],
            ),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn rejects_scalar_ranges_shape_mismatches_and_incomplete_pairs() {
        let ctx = TestContext::default();

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    Arg::Value(Value::Number(1.0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    Arg::Value(text("x")),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(text("x")),
                    Arg::Value(text("x")),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (2, 1)),
                    Arg::Value(text("x")),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    Arg::Value(text("x")),
                    range_arg(&ctx, (0, 2), (1, 2)),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_args(ctx: &TestContext, args: Vec<Arg<'_>>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        SUMIFS.call(&args, &fn_ctx)
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
