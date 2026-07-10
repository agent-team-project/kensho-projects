use crate::functions::prelude::*;

pub struct Pv;

impl Function for Pv {
    fn name(&self) -> &'static str {
        "PV"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(5))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(3..=5).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match pv(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn pv(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let rate = finite_number(&args[0], ctx)?;
    let nper = finite_number(&args[1], ctx)?;
    let pmt = finite_number(&args[2], ctx)?;
    let fv = match args.get(3) {
        Some(arg) => finite_number(arg, ctx)?,
        None => 0.0,
    };
    let payment_type = match args.get(4) {
        Some(arg) => payment_type(finite_number(arg, ctx)?)?,
        None => 0.0,
    };

    if rate == 0.0 {
        return finite_result(-(fv + pmt * nper));
    }

    let factor = (1.0 + rate).powf(nper);
    if !factor.is_finite() || factor == 0.0 {
        return Err(ErrorValue::Num);
    }

    let timing = 1.0 + rate * payment_type;
    if !timing.is_finite() {
        return Err(ErrorValue::Num);
    }

    let annuity = pmt * timing * (factor - 1.0) / rate;
    if !annuity.is_finite() {
        return Err(ErrorValue::Num);
    }

    finite_result(-(fv + annuity) / factor)
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

static PV: Pv = Pv;
inventory::submit! { FunctionEntry(&PV) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(PV.name(), "PV");
        assert_eq!(PV.arity(), (3, Some(5)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![Value::Number(0.01), Value::Number(12.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(-100.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn computes_microsoft_style_car_loan_example() {
        assert_number_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(4.0 * 12.0),
                Value::Number(-263.33),
            ]),
            9999.68275341816,
        );
    }

    #[test]
    fn computes_present_value_with_payments_and_future_value() {
        assert_number_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(20.0 * 12.0),
                Value::Number(500.0),
            ]),
            -59777.14585118777,
        );
        assert_number_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(20.0 * 12.0),
                Value::Number(500.0),
                Value::Number(10000.0),
            ]),
            -61806.85973769607,
        );
    }

    #[test]
    fn handles_beginning_of_period_type() {
        assert_number_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(20.0 * 12.0),
                Value::Number(500.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            -60175.660156862345,
        );
    }

    #[test]
    fn handles_zero_rate_case() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(10.0),
                Value::Number(-100.0),
                Value::Number(0.0),
            ]),
            Value::Number(1000.0)
        );
    }

    #[test]
    fn defaults_optional_future_value_and_type() {
        let defaulted = call_values(vec![
            Value::Number(0.12 / 12.0),
            Value::Number(4.0 * 12.0),
            Value::Number(-263.33),
        ]);
        let explicit = call_values(vec![
            Value::Number(0.12 / 12.0),
            Value::Number(4.0 * 12.0),
            Value::Number(-263.33),
            Value::Number(0.0),
            Value::Number(0.0),
        ]);
        assert_eq!(defaulted, explicit);
    }

    #[test]
    fn truncates_type_toward_zero_and_rejects_invalid_types() {
        assert_number_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(20.0 * 12.0),
                Value::Number(500.0),
                Value::Number(0.0),
                Value::Number(1.9),
            ]),
            -60175.660156862345,
        );
        assert_number_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(20.0 * 12.0),
                Value::Number(500.0),
                Value::Number(0.0),
                Value::Number(-0.9),
            ]),
            -59777.14585118777,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(12.0),
                Value::Number(-100.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn requires_finite_arguments_and_results() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(12.0),
                Value::Number(-100.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(1000.0),
                Value::Number(-100.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(-1.0),
                Value::Number(10.0),
                Value::Number(-100.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_number_coercion_left_to_right() {
        assert_number_close(
            call_values(vec![
                Value::Text("0.01".to_string()),
                Value::Text("12".to_string()),
                Value::Text("-100".to_string()),
                Value::Blank,
                Value::Boolean(true),
            ]),
            1136.762824821948,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("bad rate".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(-100.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Error(ErrorValue::Div0),
                Value::Text("bad pmt".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        PV.call(&args, &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => {
                assert!(
                    (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
                    "expected {expected}, got {actual}"
                );
            }
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    struct TestContext;

    impl EvalContext for TestContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
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
