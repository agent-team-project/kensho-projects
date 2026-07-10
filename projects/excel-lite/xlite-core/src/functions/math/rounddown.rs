use crate::functions::prelude::*;

pub struct Rounddown;

impl Function for Rounddown {
    fn name(&self) -> &'static str {
        "ROUNDDOWN"
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

        Value::Number(round_down(number, num_digits))
    }
}

fn round_down(number: f64, num_digits: f64) -> f64 {
    let digits = num_digits.trunc() as i32;
    if digits >= 0 {
        let factor = 10_f64.powi(digits);
        (number * factor).trunc() / factor
    } else {
        let factor = 10_f64.powi(-digits);
        (number / factor).trunc() * factor
    }
}

static ROUNDDOWN: Rounddown = Rounddown;
inventory::submit! { FunctionEntry(&ROUNDDOWN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(ROUNDDOWN.arity(), (2, Some(2)));
    }

    #[test]
    fn rounds_positive_values_toward_zero() {
        assert_eq!(
            call_rounddown(Value::Number(12.987), Value::Number(2.0)),
            Value::Number(12.98)
        );
        assert_eq!(
            call_rounddown(Value::Number(12.9), Value::Number(0.0)),
            Value::Number(12.0)
        );
    }

    #[test]
    fn rounds_negative_values_toward_zero() {
        assert_eq!(
            call_rounddown(Value::Number(-12.987), Value::Number(2.0)),
            Value::Number(-12.98)
        );
        assert_eq!(
            call_rounddown(Value::Number(-12.9), Value::Number(0.0)),
            Value::Number(-12.0)
        );
    }

    #[test]
    fn preserves_values_exact_at_requested_precision() {
        assert_eq!(
            call_rounddown(Value::Number(12.34), Value::Number(2.0)),
            Value::Number(12.34)
        );
        assert_eq!(
            call_rounddown(Value::Number(-1200.0), Value::Number(-2.0)),
            Value::Number(-1200.0)
        );
    }

    #[test]
    fn supports_negative_digit_counts() {
        assert_eq!(
            call_rounddown(Value::Number(987.65), Value::Number(-1.0)),
            Value::Number(980.0)
        );
        assert_eq!(
            call_rounddown(Value::Number(-987.65), Value::Number(-2.0)),
            Value::Number(-900.0)
        );
    }

    #[test]
    fn truncates_fractional_digit_counts_toward_zero() {
        assert_eq!(
            call_rounddown(Value::Number(12.987), Value::Number(1.9)),
            Value::Number(12.9)
        );
        assert_eq!(
            call_rounddown(Value::Number(987.65), Value::Number(-1.9)),
            Value::Number(980.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call_rounddown(
                Value::Text(" 8.765 ".to_string()),
                Value::Text("2".to_string())
            ),
            Value::Number(8.76)
        );
        assert_eq!(
            call_rounddown(Value::Boolean(true), Value::Blank),
            Value::Number(1.0)
        );
        assert_eq!(
            call_rounddown(Value::Blank, Value::Boolean(true)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_rounddown(
                Value::Text("not numeric".to_string()),
                Value::Number(1.0)
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_rounddown(
                Value::Number(12.34),
                Value::Text("not numeric".to_string())
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_rounddown(Value::Error(ErrorValue::Ref), Value::Number(1.0)),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_rounddown(Value::Number(12.34), Value::Error(ErrorValue::Name)),
            Value::Error(ErrorValue::Name)
        );
    }

    fn call_rounddown(number: Value, num_digits: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        ROUNDDOWN.call(&[Arg::Value(number), Arg::Value(num_digits)], &fn_ctx)
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
