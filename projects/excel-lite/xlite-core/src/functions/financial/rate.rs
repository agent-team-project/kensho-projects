use crate::functions::prelude::*;

const MAX_ITERATIONS: usize = 20;
const CONVERGENCE_TOLERANCE: f64 = 1e-7;

pub struct Rate;

impl Function for Rate {
    fn name(&self) -> &'static str {
        "RATE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(6))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(3..=6).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match rate(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn rate(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let nper = number(&args[0], ctx)?;
    let pmt = number(&args[1], ctx)?;
    let pv = number(&args[2], ctx)?;
    let fv = match args.get(3) {
        Some(arg) => number(arg, ctx)?,
        None => 0.0,
    };
    let payment_type_number = match args.get(4) {
        Some(arg) => number(arg, ctx)?,
        None => 0.0,
    };
    let guess = match args.get(5) {
        Some(arg) => number(arg, ctx)?,
        None => 0.1,
    };

    if ![nper, pmt, pv, fv, payment_type_number, guess]
        .into_iter()
        .all(f64::is_finite)
    {
        return Err(ErrorValue::Num);
    }

    if nper == 0.0 {
        return Err(ErrorValue::Num);
    }

    let payment_type = payment_type(payment_type_number)?;

    let mut estimate = guess;
    for _ in 0..MAX_ITERATIONS {
        let value = rate_value(estimate, nper, pmt, pv, fv, payment_type)?;
        let derivative = rate_derivative(estimate, nper, pmt, pv, payment_type)?;
        if derivative == 0.0 || !derivative.is_finite() {
            return Err(ErrorValue::Num);
        }

        let next = estimate - value / derivative;
        if !next.is_finite() {
            return Err(ErrorValue::Num);
        }

        if (next - estimate).abs() <= CONVERGENCE_TOLERANCE {
            return Ok(next);
        }

        estimate = next;
    }

    Err(ErrorValue::Num)
}

fn rate_value(
    rate: f64,
    nper: f64,
    pmt: f64,
    pv: f64,
    fv: f64,
    payment_type: f64,
) -> Result<f64, ErrorValue> {
    if rate == 0.0 {
        return finite_result(pv + pmt * nper + fv);
    }

    let factor = compound_factor(rate, nper)?;
    let annuity_factor = (factor - 1.0) / rate;
    if !annuity_factor.is_finite() {
        return Err(ErrorValue::Num);
    }

    let timing = 1.0 + rate * payment_type;
    if !timing.is_finite() {
        return Err(ErrorValue::Num);
    }

    finite_result(pv * factor + pmt * timing * annuity_factor + fv)
}

fn rate_derivative(
    rate: f64,
    nper: f64,
    pmt: f64,
    pv: f64,
    payment_type: f64,
) -> Result<f64, ErrorValue> {
    if rate == 0.0 {
        return finite_result(pv * nper + pmt * (nper * payment_type + nper * (nper - 1.0) / 2.0));
    }

    let factor = compound_factor(rate, nper)?;
    let factor_derivative = compound_factor_derivative(rate, nper)?;
    let annuity_factor = (factor - 1.0) / rate;
    let annuity_derivative = (factor_derivative * rate - (factor - 1.0)) / (rate * rate);

    if !annuity_factor.is_finite() || !annuity_derivative.is_finite() {
        return Err(ErrorValue::Num);
    }

    let timing = 1.0 + rate * payment_type;
    if !timing.is_finite() {
        return Err(ErrorValue::Num);
    }

    finite_result(
        pv * factor_derivative
            + pmt * (payment_type * annuity_factor + timing * annuity_derivative),
    )
}

fn compound_factor(rate: f64, nper: f64) -> Result<f64, ErrorValue> {
    let base = 1.0 + rate;
    if base <= 0.0 && nper.fract() != 0.0 {
        return Err(ErrorValue::Num);
    }

    finite_result(base.powf(nper))
}

fn compound_factor_derivative(rate: f64, nper: f64) -> Result<f64, ErrorValue> {
    let base = 1.0 + rate;
    if base <= 0.0 && (nper - 1.0).fract() != 0.0 {
        return Err(ErrorValue::Num);
    }

    finite_result(nper * base.powf(nper - 1.0))
}

fn number(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    ctx.to_number(&arg.as_value())
}

fn payment_type(number: f64) -> Result<f64, ErrorValue> {
    let truncated = number.trunc();
    if truncated == 0.0 || truncated == 1.0 {
        Ok(truncated)
    } else {
        Err(ErrorValue::Num)
    }
}

fn finite_result(number: f64) -> Result<f64, ErrorValue> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

static RATE: Rate = Rate;
inventory::submit! { FunctionEntry(&RATE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(RATE.name(), "RATE");
        assert_eq!(RATE.arity(), (3, Some(6)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![Value::Number(12.0), Value::Number(-100.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(12.0),
                Value::Number(-100.0),
                Value::Number(1000.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(0.1),
                Value::Number(0.1),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_examples() {
        let monthly = call_values(vec![
            Value::Number(4.0 * 12.0),
            Value::Number(-200.0),
            Value::Number(8000.0),
        ]);
        assert_number_close(monthly.clone(), 0.00770147248820239);

        match monthly {
            Value::Number(rate) => {
                assert!((rate * 12.0 - 0.09241766985842868).abs() <= 1e-12);
            }
            other => panic!("expected number, got {other:?}"),
        }
    }

    #[test]
    fn handles_defaults_and_zero_rate_solution() {
        let defaulted = call_values(vec![
            Value::Number(4.0 * 12.0),
            Value::Number(-200.0),
            Value::Number(8000.0),
        ]);
        let explicit = call_values(vec![
            Value::Number(4.0 * 12.0),
            Value::Number(-200.0),
            Value::Number(8000.0),
            Value::Number(0.0),
            Value::Number(0.0),
            Value::Number(0.1),
        ]);
        assert_number_close(defaulted, 0.00770147248820239);
        assert_number_close(explicit, 0.00770147248820239);

        assert_number_within(
            call_values(vec![
                Value::Number(10.0),
                Value::Number(-100.0),
                Value::Number(1000.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(0.01),
            ]),
            0.0,
            CONVERGENCE_TOLERANCE,
        );
    }

    #[test]
    fn handles_beginning_of_period_type() {
        assert_number_close(
            call_values(vec![
                Value::Number(24.0),
                Value::Number(-100.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            0.01655011906668426,
        );
    }

    #[test]
    fn truncates_type_toward_zero_and_rejects_invalid_type() {
        assert_number_close(
            call_values(vec![
                Value::Number(24.0),
                Value::Number(-100.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(1.9),
            ]),
            0.01655011906668426,
        );
        assert_number_close(
            call_values(vec![
                Value::Number(24.0),
                Value::Number(-100.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(-0.9),
            ]),
            0.015130843902310353,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(24.0),
                Value::Number(-100.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_numeric_domains_and_non_convergence() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(-100.0),
                Value::Number(1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2.5),
                Value::Number(-100.0),
                Value::Number(1000.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(-1.5),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn requires_finite_arguments_and_results() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(-100.0),
                Value::Number(1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(12.0),
                Value::Number(-100.0),
                Value::Number(1000.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_number_close(
            call_values(vec![
                Value::Text("48".to_string()),
                Value::Text("-200".to_string()),
                Value::Text("8000".to_string()),
                Value::Blank,
                Value::Boolean(false),
                Value::Text("0.01".to_string()),
            ]),
            0.00770147248820239,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(8000.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(48.0),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(24.0),
                Value::Number(-100.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(2.0),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Div0)
        );

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

        assert_number_close(
            RATE.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(-200.0)),
                    Arg::Value(Value::Number(8000.0)),
                ],
                &fn_ctx,
            ),
            0.00770147248820239,
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        RATE.call(&args, &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        assert_number_within(actual, expected, 1e-12 * expected.abs().max(1.0));
    }

    fn assert_number_within(actual: Value, expected: f64, tolerance: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= tolerance,
                "expected {expected}, got {actual}"
            ),
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    struct GridContext;

    impl EvalContext for GridContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Number(48.0),
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
