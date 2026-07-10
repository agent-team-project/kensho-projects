use crate::functions::prelude::*;

pub struct Median;

impl Function for Median {
    fn name(&self) -> &'static str {
        "MEDIAN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        let mut numbers = Vec::new();
        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(values) => numbers.extend(values),
                Err(error) => return Value::Error(error),
            }
        }

        if numbers.is_empty() {
            return Value::Error(ErrorValue::Num);
        }

        numbers.sort_by(f64::total_cmp);

        let middle = numbers.len() / 2;
        if numbers.len() % 2 == 1 {
            Value::Number(numbers[middle])
        } else {
            Value::Number((numbers[middle - 1] + numbers[middle]) / 2.0)
        }
    }
}

static MEDIAN: Median = Median;
inventory::submit! { FunctionEntry(&MEDIAN) }

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
        assert_eq!(MEDIAN.arity(), (1, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(MEDIAN.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn returns_middle_sorted_number_for_odd_count() {
        assert_eq!(
            call_values(vec![
                Value::Number(9.0),
                Value::Number(1.0),
                Value::Number(5.0),
            ]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn averages_middle_sorted_numbers_for_even_count() {
        assert_eq!(
            call_values(vec![
                Value::Number(9.0),
                Value::Number(1.0),
                Value::Number(6.0),
                Value::Number(4.0),
            ]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn combines_ranges_and_direct_scalars_before_sorting() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(100.0)),
            ((0, 1), Value::Number(12.0)),
            ((1, 0), Value::Number(1.0)),
            ((1, 1), Value::Text("ignored".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MEDIAN.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(4.0)),
                    Arg::Value(Value::Text("8".to_string())),
                ],
                &fn_ctx,
            ),
            Value::Number(8.0)
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text(" 6 ".to_string()),
                Value::Boolean(true),
                Value::Boolean(false),
                Value::Blank,
            ]),
            Value::Number(0.5)
        );
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("99".to_string())),
            ((0, 1), Value::Boolean(true)),
            ((1, 0), Value::Number(0.0)),
            ((1, 1), Value::Number(10.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MEDIAN.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(5.0)
        );
    }

    #[test]
    fn returns_num_for_blank_or_text_only_ranges() {
        let blank_ctx = TestContext::default();
        let blank_fn_ctx = FnContext::new(&blank_ctx);

        assert_eq!(
            MEDIAN.call(&[range_arg(&blank_ctx, (0, 0), (1, 1))], &blank_fn_ctx),
            Value::Error(ErrorValue::Num)
        );

        let text_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("3".to_string())),
            ((0, 1), Value::Text("apple".to_string())),
        ]);
        let text_fn_ctx = FnContext::new(&text_ctx);

        assert_eq!(
            MEDIAN.call(&[range_arg(&text_ctx, (0, 0), (0, 1))], &text_fn_ctx),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_aggregation_errors() {
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Text("apple".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MEDIAN.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        MEDIAN.call(&args, &fn_ctx)
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
