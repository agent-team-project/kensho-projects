use crate::functions::prelude::*;

pub struct Rank;

impl Function for Rank {
    fn name(&self) -> &'static str {
        "RANK"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 2 || args.len() > 3 {
            return Value::Error(ErrorValue::Value);
        }

        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };

        let values = match numeric_ref_values(&args[1]) {
            Ok(values) => values,
            Err(error) => return Value::Error(error),
        };

        if values.is_empty() || !values.iter().any(|value| *value == number) {
            return Value::Error(ErrorValue::Na);
        }

        let descending = match args.get(2) {
            Some(order) => match ctx.to_number(&order.as_value()) {
                Ok(order) => order == 0.0,
                Err(error) => return Value::Error(error),
            },
            None => true,
        };

        let rank = if descending {
            values.iter().filter(|value| **value > number).count() + 1
        } else {
            values.iter().filter(|value| **value < number).count() + 1
        };

        Value::Number(rank as f64)
    }
}

fn numeric_ref_values(arg: &Arg<'_>) -> Result<Vec<f64>, ErrorValue> {
    let Arg::Range(range) = arg else {
        return Err(ErrorValue::Value);
    };

    let mut values = Vec::new();
    for value in range.iter() {
        match value {
            Value::Number(number) => values.push(number),
            Value::Error(error) => return Err(error),
            Value::Text(_) | Value::Boolean(_) | Value::Blank => {}
        }
    }
    Ok(values)
}

static RANK: Rank = Rank;
inventory::submit! { FunctionEntry(&RANK) }

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
        assert_eq!(RANK.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_args(vec![Arg::Value(Value::Number(1.0))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(0.0)),
                Arg::Value(Value::Number(0.0)),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn ranks_descending_by_default_and_with_zero_order() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(7.0)),
            ((1, 0), Value::Number(3.0)),
            ((2, 0), Value::Number(5.0)),
            ((3, 0), Value::Number(10.0)),
        ]);

        assert_eq!(rank_range(&ctx, 5.0, None), Value::Number(3.0));
        assert_eq!(
            rank_range(&ctx, 5.0, Some(Value::Number(0.0))),
            Value::Number(3.0)
        );
    }

    #[test]
    fn ranks_ascending_with_nonzero_order() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(7.0)),
            ((1, 0), Value::Number(3.0)),
            ((2, 0), Value::Number(5.0)),
            ((3, 0), Value::Number(10.0)),
        ]);

        assert_eq!(
            rank_range(&ctx, 5.0, Some(Value::Number(1.0))),
            Value::Number(2.0)
        );
    }

    #[test]
    fn duplicate_numbers_receive_same_rank_and_skip_subsequent_rank() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), Value::Number(8.0)),
            ((2, 0), Value::Number(8.0)),
            ((3, 0), Value::Number(7.0)),
            ((4, 0), Value::Number(5.0)),
        ]);

        assert_eq!(rank_range(&ctx, 8.0, None), Value::Number(2.0));
        assert_eq!(rank_range(&ctx, 7.0, None), Value::Number(4.0));
    }

    #[test]
    fn ignores_nonnumeric_values_in_ref() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(4.0)),
            ((1, 0), Value::Text("9".to_string())),
            ((2, 0), Value::Boolean(true)),
            ((4, 0), Value::Number(2.0)),
        ]);

        assert_eq!(rank_range(&ctx, 2.0, None), Value::Number(2.0));
    }

    #[test]
    fn returns_na_when_number_not_found_or_ref_has_no_numbers() {
        let mixed_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(4.0)),
            ((1, 0), Value::Number(2.0)),
        ]);
        assert_eq!(
            rank_range(&mixed_ctx, 3.0, None),
            Value::Error(ErrorValue::Na)
        );

        let empty_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("3".to_string())),
            ((1, 0), Value::Boolean(false)),
        ]);
        assert_eq!(
            rank_range(&empty_ctx, 3.0, None),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn coerces_direct_number_and_order() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(3.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            RANK.call(
                &[
                    Arg::Value(Value::Text("2".to_string())),
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Boolean(true)),
                ],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
        assert_eq!(
            RANK.call(
                &[
                    Arg::Value(Value::Number(2.0)),
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Blank),
                ],
                &fn_ctx,
            ),
            Value::Number(2.0)
        );
    }

    #[test]
    fn propagates_direct_number_and_order_coercion_errors() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            RANK.call(
                &[
                    Arg::Value(Value::Text("apple".to_string())),
                    range_arg(&ctx, (0, 0), (1, 0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            RANK.call(
                &[
                    Arg::Value(Value::Error(ErrorValue::Div0)),
                    range_arg(&ctx, (0, 0), (1, 0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            RANK.call(
                &[
                    Arg::Value(Value::Number(2.0)),
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Text("x".to_string())),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            RANK.call(
                &[
                    Arg::Value(Value::Number(2.0)),
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Error(ErrorValue::Div0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn requires_ref_to_be_a_range() {
        assert_eq!(
            call_args(vec![
                Arg::Value(Value::Number(1.0)),
                Arg::Value(Value::Number(1.0)),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_errors_from_ref() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((1, 0), Value::Error(ErrorValue::Div0)),
        ]);

        assert_eq!(rank_range(&ctx, 2.0, None), Value::Error(ErrorValue::Div0));
    }

    fn call_args(args: Vec<Arg<'_>>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        RANK.call(&args, &fn_ctx)
    }

    fn rank_range(ctx: &TestContext, number: f64, order: Option<Value>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        let mut args = vec![
            Arg::Value(Value::Number(number)),
            range_arg(ctx, (0, 0), (4, 0)),
        ];
        if let Some(order) = order {
            args.push(Arg::Value(order));
        }
        RANK.call(&args, &fn_ctx)
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
