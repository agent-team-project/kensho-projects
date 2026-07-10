use crate::functions::prelude::*;

pub struct CountIfs;

impl Function for CountIfs {
    fn name(&self) -> &'static str {
        "COUNTIFS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(254))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 2 || args.len() > 254 || args.len() % 2 != 0 {
            return Value::Error(ErrorValue::Value);
        }

        let first_range = match &args[0] {
            Arg::Range(range) => range,
            Arg::Value(_) => return Value::Error(ErrorValue::Value),
        };

        let mut criteria = Vec::new();
        for pair in args.chunks_exact(2) {
            let criteria_range = match &pair[0] {
                Arg::Range(range) => range,
                Arg::Value(_) => return Value::Error(ErrorValue::Value),
            };
            if criteria_range.rows() != first_range.rows()
                || criteria_range.cols() != first_range.cols()
            {
                return Value::Error(ErrorValue::Value);
            }

            let criteria_value = pair[1].as_value();
            if let Some(error) = criteria_value.as_error() {
                return Value::Error(error);
            }

            criteria.push((criteria_range, criteria_value));
        }

        let mut count = 0usize;
        for row in 0..first_range.rows() {
            for col in 0..first_range.cols() {
                if criteria.iter().all(|(range, criteria_value)| {
                    ctx.matches_criteria(&range.get(row, col), criteria_value)
                }) {
                    count += 1;
                }
            }
        }

        Value::Number(count as f64)
    }
}

static COUNTIFS: CountIfs = CountIfs;
inventory::submit! { FunctionEntry(&COUNTIFS) }

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
    fn reports_countifs_arity() {
        assert_eq!(COUNTIFS.arity(), (2, Some(254)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = TestContext::default();

        assert_eq!(
            call_args(&ctx, vec![range_arg(&ctx, (0, 0), (0, 0))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(Value::Number(1.0)),
                    range_arg(&ctx, (0, 1), (0, 1)),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );

        let too_many = (0..255)
            .map(|_| Arg::Value(Value::Number(1.0)))
            .collect::<Vec<_>>();
        assert_eq!(
            call_args(&ctx, too_many),
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

        assert_eq!(
            call_args(
                &ctx,
                vec![range_arg(&ctx, (0, 0), (2, 0)), Arg::Value(Value::Number(1.0))],
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn requires_all_criteria_at_the_same_position() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("fruit")),
            ((1, 0), text("fruit")),
            ((2, 0), text("veg")),
            ((3, 0), text("fruit")),
            ((0, 1), text("west")),
            ((1, 1), text("east")),
            ((2, 1), text("east")),
            ((3, 1), text("east")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (3, 0)),
                    Arg::Value(text("fruit")),
                    range_arg(&ctx, (0, 1), (3, 1)),
                    Arg::Value(text("east")),
                ],
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
            ((0, 1), text("alpha")),
            ((1, 1), text("alpine")),
            ((2, 1), text("beta")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(text(">=2")),
                    range_arg(&ctx, (0, 1), (2, 1)),
                    Arg::Value(text("al*")),
                ],
            ),
            Value::Number(1.0)
        );
    }

    #[test]
    fn returns_zero_when_no_cells_match() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("fruit")),
            ((1, 0), text("veg")),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![range_arg(&ctx, (0, 0), (1, 0)), Arg::Value(text("meat"))],
            ),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_criteria_argument_errors() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
        ]);

        assert_eq!(
            call_args(
                &ctx,
                vec![
                    range_arg(&ctx, (0, 0), (1, 0)),
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
                vec![Arg::Value(Value::Number(1.0)), Arg::Value(Value::Number(1.0))],
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
                    Arg::Value(text("x")),
                    range_arg(&ctx, (0, 1), (1, 1)),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_args(ctx: &TestContext, args: Vec<Arg<'_>>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        COUNTIFS.call(&args, &fn_ctx)
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
