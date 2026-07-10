use crate::functions::prelude::*;

const MAX_EXACT_INTEGER: u64 = 1_u64 << 53;
const MAX_EXACT_INTEGER_F64: f64 = MAX_EXACT_INTEGER as f64;

pub struct Lcm;

impl Function for Lcm {
    fn name(&self) -> &'static str {
        "LCM"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let mut result = 1_u64;
        let mut count = 0usize;

        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => {
                    for number in numbers {
                        count += 1;
                        let value = match lcm_input(number) {
                            Ok(value) => value,
                            Err(error) => return Value::Error(error),
                        };

                        if value == 0 {
                            return Value::Number(0.0);
                        }

                        result = match checked_lcm(result, value) {
                            Ok(value) => value,
                            Err(error) => return Value::Error(error),
                        };
                    }
                }
                Err(error) => return Value::Error(error),
            }
        }

        if count == 0 {
            Value::Number(0.0)
        } else {
            Value::Number(result as f64)
        }
    }
}

fn lcm_input(number: f64) -> Result<u64, ErrorValue> {
    if number < 0.0 {
        return Err(ErrorValue::Num);
    }

    let integer = number.trunc();
    if integer == 0.0 {
        return Ok(0);
    }
    if !integer.is_finite() || integer >= MAX_EXACT_INTEGER_F64 {
        return Err(ErrorValue::Num);
    }

    Ok(integer as u64)
}

fn checked_lcm(lhs: u64, rhs: u64) -> Result<u64, ErrorValue> {
    let divisor = gcd(lhs, rhs) as u128;
    let result = (lhs as u128 / divisor) * rhs as u128;
    if result >= MAX_EXACT_INTEGER as u128 {
        Err(ErrorValue::Num)
    } else {
        Ok(result as u64)
    }
}

fn gcd(mut lhs: u64, mut rhs: u64) -> u64 {
    while rhs != 0 {
        let remainder = lhs % rhs;
        lhs = rhs;
        rhs = remainder;
    }
    lhs
}

static LCM: Lcm = Lcm;
inventory::submit! { FunctionEntry(&LCM) }

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
    fn reports_variadic_bounded_arity() {
        assert_eq!(LCM.arity(), (1, Some(255)));
    }

    #[test]
    fn computes_lcm_for_scalar_numbers() {
        assert_eq!(
            call_values(vec![Value::Number(5.0), Value::Number(2.0)]),
            Value::Number(10.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(24.0),
                Value::Number(36.0),
                Value::Number(10.0),
            ]),
            Value::Number(360.0)
        );
    }

    #[test]
    fn truncates_decimals_toward_zero() {
        assert_eq!(
            call_values(vec![Value::Number(4.9), Value::Number(6.1)]),
            Value::Number(12.0)
        );
    }

    #[test]
    fn returns_zero_when_any_number_truncates_to_zero() {
        assert_eq!(
            call_values(vec![Value::Number(0.0), Value::Number(5.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(0.9), Value::Number(5.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn rejects_negative_numbers_before_truncation() {
        assert_eq!(
            call_values(vec![Value::Number(-0.2), Value::Number(5.0)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_values_at_or_above_exact_integer_limit() {
        assert_eq!(
            call_values(vec![Value::Number(MAX_EXACT_INTEGER_F64)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_intermediate_results_at_or_above_exact_integer_limit() {
        assert_eq!(
            call_values(vec![
                Value::Number((MAX_EXACT_INTEGER / 2) as f64),
                Value::Number(3.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text(" 6 ".to_string()),
                Value::Boolean(true),
                Value::Number(4.0),
            ]),
            Value::Number(12.0)
        );
        assert_eq!(
            call_values(vec![Value::Blank, Value::Number(5.0)]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(6.0)),
            ((0, 1), Value::Text("ignored".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(4.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LCM.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(10.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(60.0)
        );
    }

    #[test]
    fn returns_zero_for_blank_only_range() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            LCM.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(0.0)
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
            LCM.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        LCM.call(&args, &fn_ctx)
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
