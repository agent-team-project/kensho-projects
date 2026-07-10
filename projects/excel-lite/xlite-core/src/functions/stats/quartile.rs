use crate::functions::prelude::*;

pub struct Quartile;

impl Function for Quartile {
    fn name(&self) -> &'static str {
        "QUARTILE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let mut numbers: Vec<_> = match ctx.try_iter_numbers(&args[0]) {
            Ok(values) => values.collect(),
            Err(error) => return Value::Error(error),
        };

        let quart = match ctx.to_number(&args[1].as_value()) {
            Ok(quart) => quart,
            Err(_) => return Value::Error(ErrorValue::Value),
        };

        if numbers.is_empty() || !quart.is_finite() || !(0.0..=4.0).contains(&quart) {
            return Value::Error(ErrorValue::Num);
        }

        numbers.sort_by(f64::total_cmp);
        let percentile = quart.trunc() / 4.0;
        Value::Number(percentile_inc(&numbers, percentile))
    }
}

fn percentile_inc(sorted_numbers: &[f64], percentile: f64) -> f64 {
    if sorted_numbers.len() == 1 {
        return sorted_numbers[0];
    }

    let rank = percentile * (sorted_numbers.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        return sorted_numbers[lower];
    }

    let fraction = rank - lower as f64;
    sorted_numbers[lower] + (sorted_numbers[upper] - sorted_numbers[lower]) * fraction
}

static QUARTILE: Quartile = Quartile;
inventory::submit! { FunctionEntry(&QUARTILE) }

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
        assert_eq!(QUARTILE.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_args(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_args(vec![Arg::Value(Value::Number(1.0))]),
            Value::Error(ErrorValue::Value)
        );
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
    fn quart_zero_two_and_four_return_minimum_median_and_maximum() {
        let values = vec![9.0, 1.0, 5.0];

        assert_eq!(call_range(values.clone(), 0.0), Value::Number(1.0));
        assert_eq!(call_range(values.clone(), 2.0), Value::Number(5.0));
        assert_eq!(call_range(values, 4.0), Value::Number(9.0));
    }

    #[test]
    fn quart_one_and_three_interpolate_on_even_sized_data_set() {
        let values = vec![1.0, 2.0, 3.0, 4.0];

        assert_number_close(call_range(values.clone(), 1.0), 1.75);
        assert_number_close(call_range(values, 3.0), 3.25);
    }

    #[test]
    fn truncates_non_integer_quart_before_selecting_quartile() {
        let values = vec![1.0, 2.0, 3.0, 4.0];

        assert_number_close(call_range(values.clone(), 1.9), 1.75);
        assert_number_close(call_range(values, 3.7), 3.25);
    }

    #[test]
    fn single_numeric_value_returns_itself_for_any_valid_quart() {
        for quart in 0..=4 {
            assert_eq!(
                call_value(Value::Number(42.0), Value::Number(quart as f64)),
                Value::Number(42.0)
            );
        }
    }

    #[test]
    fn direct_scalar_array_is_one_item_data_set() {
        assert_eq!(
            call_value(Value::Text(" 7.5 ".to_string()), Value::Number(3.0)),
            Value::Number(7.5)
        );
    }

    #[test]
    fn range_ignores_text_booleans_and_blanks_while_including_zero() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("100".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((2, 0), Value::Number(0.0)),
            ((4, 0), Value::Number(10.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            QUARTILE.call(
                &[
                    range_arg(&ctx, (0, 0), (4, 0)),
                    Arg::Value(Value::Number(2.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(5.0)
        );
    }

    #[test]
    fn returns_num_for_empty_or_no_numeric_array() {
        let blank_ctx = TestContext::default();
        let blank_fn_ctx = FnContext::new(&blank_ctx);

        assert_eq!(
            QUARTILE.call(
                &[
                    range_arg(&blank_ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &blank_fn_ctx,
            ),
            Value::Error(ErrorValue::Num)
        );

        let text_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("3".to_string())),
            ((0, 1), Value::Boolean(false)),
        ]);
        let text_fn_ctx = FnContext::new(&text_ctx);

        assert_eq!(
            QUARTILE.call(
                &[
                    range_arg(&text_ctx, (0, 0), (0, 1)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &text_fn_ctx,
            ),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn returns_num_for_invalid_quart_values() {
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(-0.1)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(4.1)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(f64::INFINITY)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Number(f64::NAN)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_array_coercion_and_range_errors() {
        assert_eq!(
            call_value(Value::Text("apple".to_string()), Value::Number(1.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_value(Value::Error(ErrorValue::Div0), Value::Number(1.0)),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            QUARTILE.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn quart_coercion_errors_return_value() {
        assert_eq!(
            call_value(Value::Number(1.0), Value::Text("apple".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_value(Value::Number(1.0), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_value(array: Value, quart: Value) -> Value {
        call_args(vec![Arg::Value(array), Arg::Value(quart)])
    }

    fn call_range(values: Vec<f64>, quart: f64) -> Value {
        let row_count = values.len() as u32;
        let ctx = TestContext::with_cells(
            values
                .into_iter()
                .enumerate()
                .map(|(row, number)| ((row as u32, 0), Value::Number(number)))
                .collect(),
        );
        let fn_ctx = FnContext::new(&ctx);
        QUARTILE.call(
            &[
                range_arg(&ctx, (0, 0), (row_count - 1, 0)),
                Arg::Value(Value::Number(quart)),
            ],
            &fn_ctx,
        )
    }

    fn call_args(args: Vec<Arg<'_>>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        QUARTILE.call(&args, &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => {
                let tolerance = expected.abs().max(1.0) * 1e-12;
                assert!(
                    (actual - expected).abs() <= tolerance,
                    "expected {expected}, got {actual}"
                );
            }
            other => panic!("expected number {expected}, got {other:?}"),
        }
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
