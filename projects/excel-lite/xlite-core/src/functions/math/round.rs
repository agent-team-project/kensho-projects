use crate::functions::prelude::*;

pub struct Round;

impl Function for Round {
    fn name(&self) -> &'static str {
        "ROUND"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let num_digits = match ctx.to_number(&args[1].as_value()) {
            Ok(num_digits) => num_digits,
            Err(error) => return Value::Error(error),
        };

        Value::Number(round(number, num_digits))
    }
}

fn round(number: f64, num_digits: f64) -> f64 {
    let digits = num_digits.trunc() as i32;
    if digits >= 0 {
        let factor = 10_f64.powi(digits);
        round_half_away_from_zero(number * factor) / factor
    } else {
        let factor = 10_f64.powi(-digits);
        round_half_away_from_zero(number / factor) * factor
    }
}

fn round_half_away_from_zero(number: f64) -> f64 {
    if number.is_sign_negative() {
        (number - 0.5).ceil()
    } else {
        (number + 0.5).floor()
    }
}

static ROUND: Round = Round;
inventory::submit! { FunctionEntry(&ROUND) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(ROUND.arity(), (2, Some(2)));
    }

    #[test]
    fn rounds_half_away_from_zero_at_integer_precision() {
        assert_eq!(
            call_round(Value::Number(2.5), Value::Number(0.0)),
            Value::Number(3.0)
        );
        assert_eq!(
            call_round(Value::Number(-2.5), Value::Number(0.0)),
            Value::Number(-3.0)
        );
    }

    #[test]
    fn rounds_to_positive_decimal_places() {
        assert_eq!(
            call_round(Value::Number(1.24), Value::Number(1.0)),
            Value::Number(1.2)
        );
        assert_eq!(
            call_round(Value::Number(1.25), Value::Number(1.0)),
            Value::Number(1.3)
        );
        assert_eq!(
            call_round(Value::Number(-1.25), Value::Number(1.0)),
            Value::Number(-1.3)
        );
    }

    #[test]
    fn rounds_zero_digits_to_integers() {
        assert_eq!(
            call_round(Value::Number(12.4), Value::Number(0.0)),
            Value::Number(12.0)
        );
        assert_eq!(
            call_round(Value::Number(-12.4), Value::Number(0.0)),
            Value::Number(-12.0)
        );
    }

    #[test]
    fn rounds_negative_digits_to_powers_of_ten() {
        assert_eq!(
            call_round(Value::Number(25.0), Value::Number(-1.0)),
            Value::Number(30.0)
        );
        assert_eq!(
            call_round(Value::Number(-25.0), Value::Number(-1.0)),
            Value::Number(-30.0)
        );
        assert_eq!(
            call_round(Value::Number(149.0), Value::Number(-2.0)),
            Value::Number(100.0)
        );
    }

    #[test]
    fn truncates_fractional_digit_counts_toward_zero() {
        assert_eq!(
            call_round(Value::Number(1.25), Value::Number(1.9)),
            Value::Number(1.3)
        );
        assert_eq!(
            call_round(Value::Number(25.0), Value::Number(-1.9)),
            Value::Number(30.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call_round(
                Value::Text(" 8.764 ".to_string()),
                Value::Text("2".to_string())
            ),
            Value::Number(8.76)
        );
        assert_eq!(
            call_round(Value::Boolean(true), Value::Number(0.0)),
            Value::Number(1.0)
        );
        assert_eq!(
            call_round(Value::Number(9.9), Value::Blank),
            Value::Number(10.0)
        );
        assert_eq!(
            call_round(Value::Blank, Value::Number(0.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_round(
                Value::Text("not numeric".to_string()),
                Value::Number(0.0)
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_round(
                Value::Number(12.34),
                Value::Text("not numeric".to_string())
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_round(Value::Error(ErrorValue::Ref), Value::Number(1.0)),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_round(Value::Number(12.34), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_round(number: Value, num_digits: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        ROUND.call(&[Arg::Value(number), Arg::Value(num_digits)], &fn_ctx)
    }

    struct DummyContext;

    impl EvalContext for DummyContext {
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
