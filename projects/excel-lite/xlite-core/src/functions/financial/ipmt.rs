use crate::functions::prelude::*;

pub struct Ipmt;

impl Function for Ipmt {
    fn name(&self) -> &'static str {
        "IPMT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (4, Some(6))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(4..=6).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match ipmt(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn ipmt(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let rate = finite_number(&args[0], ctx)?;
    let per = finite_number(&args[1], ctx)?;
    let nper = finite_number(&args[2], ctx)?;
    let pv = finite_number(&args[3], ctx)?;
    let fv = match args.get(4) {
        Some(arg) => finite_number(arg, ctx)?,
        None => 0.0,
    };
    let payment_type = match args.get(5) {
        Some(arg) => payment_type(finite_number(arg, ctx)?)?,
        None => 0.0,
    };

    if per < 1.0 || per > nper || nper == 0.0 {
        return Err(ErrorValue::Num);
    }

    let payment = regular_payment(rate, nper, pv, fv, payment_type)?;
    if rate == 0.0 || (payment_type == 1.0 && per == 1.0) {
        return Ok(0.0);
    }

    let balance = future_value(rate, per - 1.0, payment, pv, payment_type)?;
    let denominator = 1.0 + rate * payment_type;
    if !denominator.is_finite() || denominator == 0.0 {
        return Err(ErrorValue::Num);
    }

    finite_result(balance * rate / denominator)
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

fn regular_payment(
    rate: f64,
    nper: f64,
    pv: f64,
    fv: f64,
    payment_type: f64,
) -> Result<f64, ErrorValue> {
    if rate == 0.0 {
        if nper == 0.0 {
            return Err(ErrorValue::Num);
        }
        return finite_result(-(pv + fv) / nper);
    }

    let factor = (1.0 + rate).powf(nper);
    if !factor.is_finite() {
        return Err(ErrorValue::Num);
    }

    let timing = 1.0 + rate * payment_type;
    if !timing.is_finite() {
        return Err(ErrorValue::Num);
    }

    let denominator = timing * (factor - 1.0);
    if !denominator.is_finite() || denominator == 0.0 {
        return Err(ErrorValue::Num);
    }

    finite_result(-((fv + pv * factor) * rate) / denominator)
}

fn future_value(
    rate: f64,
    nper: f64,
    pmt: f64,
    pv: f64,
    payment_type: f64,
) -> Result<f64, ErrorValue> {
    if rate == 0.0 {
        return finite_result(-(pv + pmt * nper));
    }

    let factor = (1.0 + rate).powf(nper);
    if !factor.is_finite() {
        return Err(ErrorValue::Num);
    }

    let timing = 1.0 + rate * payment_type;
    if !timing.is_finite() {
        return Err(ErrorValue::Num);
    }

    let payment_term = pmt * timing * (factor - 1.0) / rate;
    if !payment_term.is_finite() {
        return Err(ErrorValue::Num);
    }

    finite_result(-(pv * factor + payment_term))
}

fn finite_result(number: f64) -> Result<f64, ErrorValue> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

static IPMT: Ipmt = Ipmt;
inventory::submit! { FunctionEntry(&IPMT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(IPMT.name(), "IPMT");
        assert_eq!(IPMT.arity(), (4, Some(6)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(1.0),
                Value::Number(12.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(1.0),
                Value::Number(12.0),
                Value::Number(1000.0),
                Value::Number(0.0),
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
                Value::Number(0.10 / 12.0),
                Value::Number(1.0),
                Value::Number(3.0 * 12.0),
                Value::Number(8000.0),
            ]),
            -66.66666666666667,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.10),
                Value::Number(3.0),
                Value::Number(3.0),
                Value::Number(8000.0),
            ]),
            -292.4471299093658,
        );
    }

    #[test]
    fn handles_beginning_period_first_payment_and_zero_rate() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(1.0),
                Value::Number(24.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(5.0),
                Value::Number(12.0),
                Value::Number(1200.0),
            ]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn defaults_optional_future_value_and_type() {
        let defaulted = call_values(vec![
            Value::Number(0.10 / 12.0),
            Value::Number(1.0),
            Value::Number(3.0 * 12.0),
            Value::Number(8000.0),
        ]);
        let explicit = call_values(vec![
            Value::Number(0.10 / 12.0),
            Value::Number(1.0),
            Value::Number(3.0 * 12.0),
            Value::Number(8000.0),
            Value::Number(0.0),
            Value::Number(0.0),
        ]);

        assert_eq!(defaulted, explicit);
    }

    #[test]
    fn truncates_type_toward_zero_and_rejects_invalid_type() {
        assert_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(2.0),
                Value::Number(24.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(1.9),
            ]),
            -12.734296360403743,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(2.0),
                Value::Number(24.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(-0.9),
            ]),
            -12.819191669473101,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(2.0),
                Value::Number(24.0),
                Value::Number(2000.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_period_domains() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(0.0),
                Value::Number(24.0),
                Value::Number(2000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(25.0),
                Value::Number(24.0),
                Value::Number(2000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.08 / 12.0),
                Value::Number(1.0),
                Value::Number(0.0),
                Value::Number(2000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_arguments_and_results() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(1.0),
                Value::Number(24.0),
                Value::Number(2000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(1.0),
                Value::Number(1000.0),
                Value::Number(2000.0),
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
            IPMT.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(3.0 * 12.0)),
                    Arg::Value(Value::Number(8000.0)),
                ],
                &fn_ctx,
            ),
            -66.66666666666667,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(12.0),
                Value::Number(1000.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
                Value::Number(1000.0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        IPMT.call(&args, &fn_ctx)
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
                (0, 0) => Value::Number(0.10 / 12.0),
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
