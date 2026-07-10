use crate::functions::prelude::*;

pub struct Var;

impl Function for Var {
    fn name(&self) -> &'static str {
        "VAR"
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

        if numbers.len() < 2 {
            return Value::Error(ErrorValue::Div0);
        }

        let count = numbers.len() as f64;
        let mean = numbers.iter().sum::<f64>() / count;
        let sum_squared_deviations = numbers
            .iter()
            .map(|number| {
                let deviation = number - mean;
                deviation * deviation
            })
            .sum::<f64>();

        Value::Number(sum_squared_deviations / (count - 1.0))
    }
}

static VAR: Var = Var;
inventory::submit! { FunctionEntry(&VAR) }

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
        assert_eq!(VAR.arity(), (1, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(VAR.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn calculates_sample_variance_for_known_data() {
        assert_number_close(
            call_values(vec![
                Value::Number(1345.0),
                Value::Number(1301.0),
                Value::Number(1368.0),
                Value::Number(1322.0),
                Value::Number(1310.0),
                Value::Number(1370.0),
                Value::Number(1318.0),
                Value::Number(1350.0),
                Value::Number(1303.0),
                Value::Number(1299.0),
            ]),
            754.2666666666667,
        );
    }

    #[test]
    fn combines_ranges_and_direct_scalars() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_number_close(
            VAR.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Number(3.0)),
                ],
                &fn_ctx,
            ),
            1.0,
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_number_close(
            call_values(vec![
                Value::Text("3".to_string()),
                Value::Boolean(true),
                Value::Boolean(false),
                Value::Blank,
            ]),
            2.0,
        );
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Text("99".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(0.0)),
            ((2, 1), Value::Number(4.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_number_close(VAR.call(&[range_arg(&ctx, (0, 0), (2, 1))], &fn_ctx), 4.0);
    }

    #[test]
    fn returns_div0_when_fewer_than_two_numbers_remain() {
        assert_eq!(
            call_values(vec![Value::Number(5.0)]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("3".to_string())),
            ((0, 1), Value::Boolean(false)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            VAR.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Error(ErrorValue::Div0)
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
            VAR.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        VAR.call(&args, &fn_ctx)
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
