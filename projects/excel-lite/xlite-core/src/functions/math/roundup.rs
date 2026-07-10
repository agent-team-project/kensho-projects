use crate::functions::prelude::*;

pub struct Roundup;

impl Function for Roundup {
    fn name(&self) -> &'static str {
        "ROUNDUP"
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

        Value::Number(round_up(number, num_digits))
    }
}

fn round_up(number: f64, num_digits: f64) -> f64 {
    let digits = num_digits.trunc() as i32;
    if digits >= 0 {
        let factor = 10_f64.powi(digits);
        round_away_from_zero(number * factor) / factor
    } else {
        let factor = 10_f64.powi(-digits);
        round_away_from_zero(number / factor) * factor
    }
}

fn round_away_from_zero(number: f64) -> f64 {
    if number < 0.0 {
        number.floor()
    } else {
        number.ceil()
    }
}

static ROUNDUP: Roundup = Roundup;
inventory::submit! { FunctionEntry(&ROUNDUP) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(ROUNDUP.name(), "ROUNDUP");
        assert_eq!(ROUNDUP.arity(), (2, Some(2)));
    }

    #[test]
    fn rounds_positive_values_away_from_zero() {
        assert_number(call(Value::Number(12.341), Value::Number(2.0)), 12.35);
        assert_number(call(Value::Number(12.1), Value::Number(0.0)), 13.0);
    }

    #[test]
    fn rounds_negative_values_away_from_zero() {
        assert_number(call(Value::Number(-12.341), Value::Number(2.0)), -12.35);
        assert_number(call(Value::Number(-12.1), Value::Number(0.0)), -13.0);
    }

    #[test]
    fn leaves_values_exact_at_precision_unchanged() {
        assert_number(call(Value::Number(12.34), Value::Number(2.0)), 12.34);
        assert_number(call(Value::Number(-1200.0), Value::Number(-2.0)), -1200.0);
    }

    #[test]
    fn rounds_negative_digit_counts_left_of_decimal() {
        assert_number(call(Value::Number(987.65), Value::Number(-1.0)), 990.0);
        assert_number(call(Value::Number(-987.65), Value::Number(-2.0)), -1000.0);
    }

    #[test]
    fn truncates_fractional_digit_counts_toward_zero() {
        assert_number(call(Value::Number(12.341), Value::Number(1.9)), 12.4);
        assert_number(call(Value::Number(987.65), Value::Number(-1.9)), 990.0);
    }

    #[test]
    fn uses_scalar_number_coercion_for_both_args() {
        assert_number(
            call(
                Value::Text(" 8.121 ".to_string()),
                Value::Text("2".to_string()),
            ),
            8.13,
        );
        assert_number(call(Value::Boolean(true), Value::Number(0.0)), 1.0);
        assert_number(call(Value::Number(1.2), Value::Blank), 2.0);
        assert_number(call(Value::Blank, Value::Number(1.0)), 0.0);
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Value::Number(0.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Number(12.34), Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Ref), Value::Number(1.0)),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call(Value::Number(12.34), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call(number: Value, num_digits: Value) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        ROUNDUP.call(&[Arg::Value(number), Arg::Value(num_digits)], &fn_ctx)
    }

    fn assert_number(actual: Value, expected: f64) {
        match actual {
            Value::Number(number) => assert!(
                (number - expected).abs() < 1e-12,
                "expected {expected}, got {number}"
            ),
            other => panic!("expected numeric result, got {other:?}"),
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
