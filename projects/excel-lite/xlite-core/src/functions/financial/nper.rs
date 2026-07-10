use crate::functions::prelude::*;

pub struct Nper;

impl Function for Nper {
    fn name(&self) -> &'static str {
        "NPER"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(5))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(3..=5).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match nper(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn nper(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let rate = finite_number(&args[0], ctx)?;
    let pmt = finite_number(&args[1], ctx)?;
    let pv = finite_number(&args[2], ctx)?;
    let fv = match args.get(3) {
        Some(arg) => finite_number(arg, ctx)?,
        None => 0.0,
    };
    let payment_type = match args.get(4) {
        Some(arg) => payment_type(finite_number(arg, ctx)?)?,
        None => 0.0,
    };

    if rate == 0.0 {
        if pmt == 0.0 {
            return Err(ErrorValue::Num);
        }
        return finite_result(-(pv + fv) / pmt);
    }

    let base = 1.0 + rate;
    if base <= 0.0 {
        return Err(ErrorValue::Num);
    }

    let log_base = base.ln();
    if !log_base.is_finite() || log_base == 0.0 {
        return Err(ErrorValue::Num);
    }

    let timing = 1.0 + rate * payment_type;
    if !timing.is_finite() {
        return Err(ErrorValue::Num);
    }

    let numerator = pmt * timing - fv * rate;
    if !numerator.is_finite() {
        return Err(ErrorValue::Num);
    }

    let denominator = pv * rate + pmt * timing;
    if !denominator.is_finite() || denominator == 0.0 {
        return Err(ErrorValue::Num);
    }

    let ratio = numerator / denominator;
    if !ratio.is_finite() || ratio <= 0.0 {
        return Err(ErrorValue::Num);
    }

    finite_result(ratio.ln() / log_base)
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

static NPER: Nper = Nper;
inventory::submit! { FunctionEntry(&NPER) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(NPER.name(), "NPER");
        assert_eq!(NPER.arity(), (3, Some(5)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![Value::Number(0.01), Value::Number(-100.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_examples() {
        assert_number_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(10000.0),
                Value::Number(1.0),
            ]),
            59.67386567429457,
        );
        assert_number_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(10000.0),
            ]),
            60.08212285376166,
        );
        assert_number_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
            ]),
            -9.578594039813161,
        );
    }

    #[test]
    fn handles_zero_rate_and_optional_defaults() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(10000.0),
            ]),
            Value::Number(90.0)
        );

        let defaulted = call_values(vec![
            Value::Number(0.12 / 12.0),
            Value::Number(-100.0),
            Value::Number(-1000.0),
        ]);
        let explicit = call_values(vec![
            Value::Number(0.12 / 12.0),
            Value::Number(-100.0),
            Value::Number(-1000.0),
            Value::Number(0.0),
            Value::Number(0.0),
        ]);
        assert_eq!(defaulted, explicit);
    }

    #[test]
    fn truncates_type_toward_zero_and_rejects_invalid_type() {
        assert_number_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(10000.0),
                Value::Number(1.9),
            ]),
            59.67386567429457,
        );
        assert_number_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(10000.0),
                Value::Number(-0.9),
            ]),
            60.08212285376166,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_numeric_domains() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(-1000.0),
                Value::Number(10000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(-1.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(f64::EPSILON / 4.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.1),
                Value::Number(-100.0),
                Value::Number(1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(-5.0),
                Value::Number(1000.0),
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
                Value::Number(-1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(f64::MAX),
                Value::Number(-100.0),
                Value::Number(-1000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_number_close(
            call_values(vec![
                Value::Text("0.01".to_string()),
                Value::Text("-100".to_string()),
                Value::Text("-1000".to_string()),
                Value::Blank,
                Value::Boolean(true),
            ]),
            -9.488095005505821,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(-1000.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
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
            NPER.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(-100.0)),
                    Arg::Value(Value::Number(-1000.0)),
                ],
                &fn_ctx,
            ),
            -9.578594039813161,
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        NPER.call(&args, &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
                "expected {expected}, got {actual}"
            ),
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    struct GridContext;

    impl EvalContext for GridContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Number(0.12 / 12.0),
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
