use crate::functions::prelude::*;

pub struct Fv;

impl Function for Fv {
    fn name(&self) -> &'static str {
        "FV"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(5))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(3..=5).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let rate = match number_arg(&args[0], ctx) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let nper = match number_arg(&args[1], ctx) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let pmt = match number_arg(&args[2], ctx) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let pv = match args.get(3) {
            Some(arg) => match number_arg(arg, ctx) {
                Ok(number) => number,
                Err(error) => return Value::Error(error),
            },
            None => 0.0,
        };
        let payment_type = match args.get(4) {
            Some(arg) => match number_arg(arg, ctx).and_then(payment_type) {
                Ok(payment_type) => payment_type,
                Err(error) => return Value::Error(error),
            },
            None => 0.0,
        };

        match future_value(rate, nper, pmt, pv, payment_type) {
            Ok(fv) => Value::Number(fv),
            Err(error) => Value::Error(error),
        }
    }
}

fn number_arg(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
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

fn future_value(
    rate: f64,
    nper: f64,
    pmt: f64,
    pv: f64,
    payment_type: f64,
) -> Result<f64, ErrorValue> {
    let result = if rate == 0.0 {
        -(pv + pmt * nper)
    } else {
        let factor = (1.0 + rate).powf(nper);
        if !factor.is_finite() {
            return Err(ErrorValue::Num);
        }

        let timing = 1.0 + rate * payment_type;
        let payment_term = pmt * timing * (factor - 1.0) / rate;
        if !payment_term.is_finite() {
            return Err(ErrorValue::Num);
        }

        -(pv * factor + payment_term)
    };

    if result.is_finite() {
        Ok(result)
    } else {
        Err(ErrorValue::Num)
    }
}

static FV: Fv = Fv;
inventory::submit! { FunctionEntry(&FV) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(FV.name(), "FV");
        assert_eq!(FV.arity(), (3, Some(5)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(10.0),
                Value::Number(-100.0),
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
                Value::Number(0.06 / 12.0),
                Value::Number(10.0),
                Value::Number(-200.0),
                Value::Number(-500.0),
                Value::Number(1.0),
            ]),
            2581.4033740601362,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.12 / 12.0),
                Value::Number(12.0),
                Value::Number(-1000.0),
            ]),
            12682.503013196976,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.11 / 12.0),
                Value::Number(35.0),
                Value::Number(-2000.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            82846.24637190053,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.06 / 12.0),
                Value::Number(12.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
                Value::Number(1.0),
            ]),
            2301.401830340914,
        );
    }

    #[test]
    fn handles_zero_rate_and_optional_defaults() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(10.0),
                Value::Number(-100.0),
                Value::Number(-1000.0),
            ]),
            Value::Number(2000.0)
        );
        assert_close(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(2.0),
                Value::Number(-100.0),
            ]),
            201.0,
        );
    }

    #[test]
    fn truncates_type_toward_zero_and_rejects_invalid_type() {
        assert_close(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(2.0),
                Value::Number(-100.0),
                Value::Number(0.0),
                Value::Number(1.9),
            ]),
            203.01,
        );
        assert_close(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(2.0),
                Value::Number(-100.0),
                Value::Number(0.0),
                Value::Number(-0.9),
            ]),
            201.0,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(0.01),
                Value::Number(2.0),
                Value::Number(-100.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_arguments_and_results() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(2.0),
                Value::Number(-100.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Number(2048.0),
                Value::Number(-100.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(f64::MAX),
                Value::Number(0.0),
                Value::Number(f64::MAX),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_close(
            call_values(vec![
                Value::Text("0.01".to_string()),
                Value::Boolean(true),
                Value::Text("-100".to_string()),
                Value::Blank,
                Value::Text("1.9".to_string()),
            ]),
            101.0,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(-100.0),
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

        assert_close(
            FV.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Number(-100.0)),
                ],
                &fn_ctx,
            ),
            201.0,
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        FV.call(&args, &fn_ctx)
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
                (0, 0) => Value::Number(0.01),
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
