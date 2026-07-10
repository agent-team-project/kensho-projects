use crate::functions::prelude::*;

pub struct Cumipmt;

impl Function for Cumipmt {
    fn name(&self) -> &'static str {
        "CUMIPMT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (6, Some(6))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 6 {
            return Value::Error(ErrorValue::Value);
        }

        match cumipmt(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn cumipmt(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let rate = finite_number(&args[0], ctx)?;
    let nper = finite_number(&args[1], ctx)?.trunc();
    let pv = finite_number(&args[2], ctx)?;
    let start_period = finite_number(&args[3], ctx)?.trunc();
    let end_period = finite_number(&args[4], ctx)?.trunc();
    let payment_type = payment_type(finite_number(&args[5], ctx)?)?;

    if rate <= 0.0 || nper <= 0.0 || pv <= 0.0 {
        return Err(ErrorValue::Num);
    }
    if start_period < 1.0 || start_period > end_period || end_period > nper {
        return Err(ErrorValue::Num);
    }

    let payment = regular_payment(rate, nper, pv, payment_type)?;

    if payment_type == 0.0 {
        let count = finite_result(end_period - start_period + 1.0)?;
        let sum = sum_q(rate, start_period - 1.0, end_period - 1.0)?;
        let interest = finite_result(-pv * rate * sum)?;
        let payment_interest = finite_result(payment * finite_result(sum - count)?)?;
        return finite_result(interest - payment_interest);
    }

    if end_period == 1.0 {
        return Ok(0.0);
    }

    let j_start = start_period.max(2.0) - 1.0;
    let j_end = end_period - 1.0;
    let count = finite_result(j_end - j_start + 1.0)?;
    let principal_sum = sum_q(rate, j_start - 1.0, j_end - 1.0)?;
    let payment_sum = sum_q(rate, j_start, j_end)?;
    let interest = finite_result(-pv * rate * principal_sum)?;
    let payment_interest = finite_result(payment * finite_result(payment_sum - count)?)?;
    finite_result(interest - payment_interest)
}

fn finite_number(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

fn payment_type(number: f64) -> Result<f64, ErrorValue> {
    let payment_type = number.trunc();
    if payment_type == 0.0 || payment_type == 1.0 {
        Ok(payment_type)
    } else {
        Err(ErrorValue::Num)
    }
}

fn regular_payment(
    rate: f64,
    nper: f64,
    pv: f64,
    payment_type: f64,
) -> Result<f64, ErrorValue> {
    let q = finite_result(1.0 + rate)?;
    let factor = finite_power(q, nper)?;
    let timing = finite_result(1.0 + rate * payment_type)?;
    let denominator = finite_result(timing * finite_result(factor - 1.0)?)?;
    if denominator == 0.0 {
        return Err(ErrorValue::Num);
    }

    let present_value_factor = finite_result(pv * factor)?;
    finite_result(-(present_value_factor * rate) / denominator)
}

fn sum_q(rate: f64, start: f64, end: f64) -> Result<f64, ErrorValue> {
    let q = finite_result(1.0 + rate)?;
    let upper = finite_power(q, end + 1.0)?;
    let lower = finite_power(q, start)?;
    let difference = finite_result(upper - lower)?;
    finite_result(difference / rate)
}

fn finite_power(base: f64, exponent: f64) -> Result<f64, ErrorValue> {
    finite_result(base.powf(exponent))
}

fn finite_result(number: f64) -> Result<f64, ErrorValue> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

static CUMIPMT: Cumipmt = Cumipmt;
inventory::submit! { FunctionEntry(&CUMIPMT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(CUMIPMT.name(), "CUMIPMT");
        assert_eq!(CUMIPMT.arity(), (6, Some(6)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_examples() {
        assert_close(
            call_values(vec![
                Value::Number(0.09 / 12.0),
                Value::Number(30.0 * 12.0),
                Value::Number(125000.0),
                Value::Number(13.0),
                Value::Number(24.0),
                Value::Number(0.0),
            ]),
            -11135.232130750845,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.09 / 12.0),
                Value::Number(30.0 * 12.0),
                Value::Number(125000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            -937.5,
        );
    }

    #[test]
    fn handles_beginning_of_period_interest() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.09 / 12.0),
                Value::Number(30.0 * 12.0),
                Value::Number(125000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Number(0.0)
        );
        assert_close(
            call_values(vec![
                Value::Number(0.09 / 12.0),
                Value::Number(30.0 * 12.0),
                Value::Number(125000.0),
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ]),
            -930.0128168398437,
        );
    }

    #[test]
    fn truncates_integer_oriented_arguments_toward_zero() {
        let truncated = call_values(vec![
            Value::Number(0.09 / 12.0),
            Value::Number(360.9),
            Value::Number(125000.0),
            Value::Number(1.9),
            Value::Number(2.9),
            Value::Number(1.9),
        ]);
        let explicit = call_values(vec![
            Value::Number(0.09 / 12.0),
            Value::Number(360.0),
            Value::Number(125000.0),
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(1.0),
        ]);

        assert_eq!(truncated, explicit);
        assert_close(truncated, -930.0128168398437);
    }

    #[test]
    fn rejects_invalid_domains() {
        for values in [
            vec![
                Value::Number(0.0),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ],
            vec![
                Value::Number(0.01),
                Value::Number(0.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ],
            vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ],
            vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ],
            vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ],
            vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(13.0),
                Value::Number(0.0),
            ],
            vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ],
        ] {
            assert_eq!(call_values(values), Value::Error(ErrorValue::Num));
        }
    }

    #[test]
    fn rejects_non_finite_arguments_and_results() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(1_000_000_000_000.0),
                Value::Number(1000.0),
                Value::Number(999_999_999_999.0),
                Value::Number(1_000_000_000_000.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let range = RangeRef {
            start: CellRef {
                sheet: None,
                col: 0,
                row: 0,
                col_abs: false,
                row_abs: false,
            },
            end: CellRef {
                sheet: None,
                col: 1,
                row: 1,
                col_abs: false,
                row_abs: false,
            },
        };

        assert_close(
            CUMIPMT.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(30.0 * 12.0)),
                    Arg::Value(Value::Number(125000.0)),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(0.0)),
                ],
                &fn_ctx,
            ),
            -937.5,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(125000.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        CUMIPMT.call(&args, &fn_ctx)
    }

    fn assert_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(number) => assert!(
                (number - expected).abs() <= 1e-12 * expected.abs().max(1.0),
                "expected {expected}, got {number}"
            ),
            other => panic!("expected numeric result, got {other:?}"),
        }
    }

    struct GridContext;

    impl EvalContext for GridContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Number(0.09 / 12.0),
                (0, 1) => Value::Error(ErrorValue::Div0),
                (1, 0) => Value::Text("not numeric".to_string()),
                _ => Value::Blank,
            }
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
