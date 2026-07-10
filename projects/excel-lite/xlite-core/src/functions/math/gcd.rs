use crate::functions::prelude::*;

const INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0;

pub struct Gcd;

impl Function for Gcd {
    fn name(&self) -> &'static str {
        "GCD"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let mut result = 0_u64;

        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => {
                    for number in numbers {
                        let integer = match integer_argument(number) {
                            Ok(integer) => integer,
                            Err(error) => return Value::Error(error),
                        };
                        result = gcd(result, integer);
                    }
                }
                Err(error) => return Value::Error(error),
            }
        }

        Value::Number(result as f64)
    }
}

fn integer_argument(number: f64) -> Result<u64, ErrorValue> {
    if number < 0.0 || !number.is_finite() {
        return Err(ErrorValue::Num);
    }

    let integer = number.trunc();
    if integer >= INTEGER_LIMIT {
        return Err(ErrorValue::Num);
    }

    Ok(integer as u64)
}

fn gcd(mut lhs: u64, mut rhs: u64) -> u64 {
    while rhs != 0 {
        let remainder = lhs % rhs;
        lhs = rhs;
        rhs = remainder;
    }
    lhs
}

static GCD: Gcd = Gcd;
inventory::submit! { FunctionEntry(&GCD) }

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
        assert_eq!(GCD.arity(), (1, Some(255)));
    }

    #[test]
    fn computes_two_and_many_argument_gcds() {
        assert_eq!(
            call_values(vec![Value::Number(24.0), Value::Number(36.0)]),
            Value::Number(12.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(48.0),
                Value::Number(180.0),
                Value::Number(30.0),
            ]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn handles_zero_arguments() {
        assert_eq!(
            call_values(vec![Value::Number(5.0), Value::Number(0.0)]),
            Value::Number(5.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(0.0), Value::Number(0.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn truncates_decimals_toward_zero() {
        assert_eq!(
            call_values(vec![Value::Number(8.9), Value::Number(12.1)]),
            Value::Number(4.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(0.9), Value::Number(5.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text(" 6 ".to_string()),
                Value::Boolean(true),
                Value::Blank,
            ]),
            Value::Number(1.0)
        );
        assert_eq!(call_values(vec![Value::Blank]), Value::Number(0.0));
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(12.0)),
            ((0, 1), Value::Text("ignored".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(18.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            GCD.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(6.0)
        );
    }

    #[test]
    fn rejects_negative_arguments() {
        assert_eq!(
            call_values(vec![Value::Number(-0.1), Value::Number(5.0)]),
            Value::Error(ErrorValue::Num)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(12.0)),
            ((0, 1), Value::Number(-3.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            GCD.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_truncated_arguments_at_integer_limit() {
        assert_eq!(
            call_values(vec![Value::Number(INTEGER_LIMIT)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_aggregation_errors() {
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
            GCD.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        GCD.call(&args, &fn_ctx)
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
