use crate::functions::prelude::*;

pub struct Trunc;

impl Function for Trunc {
    fn name(&self) -> &'static str {
        "TRUNC"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let num_digits = match args.get(1) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(num_digits) => num_digits,
                Err(error) => return Value::Error(error),
            },
            None => 0.0,
        };

        Value::Number(truncate(number, num_digits))
    }
}

fn truncate(number: f64, num_digits: f64) -> f64 {
    let digits = num_digits.trunc() as i32;
    if digits >= 0 {
        let factor = 10_f64.powi(digits);
        (number * factor).trunc() / factor
    } else {
        let factor = 10_f64.powi(-digits);
        (number / factor).trunc() * factor
    }
}

static TRUNC: Trunc = Trunc;
inventory::submit! { FunctionEntry(&TRUNC) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_or_two_arguments() {
        assert_eq!(TRUNC.arity(), (1, Some(2)));
    }

    #[test]
    fn defaults_to_zero_digits_and_truncates_toward_zero() {
        assert_eq!(call_trunc(Value::Number(12.9), None), Value::Number(12.0));
        assert_eq!(call_trunc(Value::Number(-2.7), None), Value::Number(-2.0));
    }

    #[test]
    fn preserves_positive_decimal_places() {
        assert_eq!(
            call_trunc(Value::Number(12.987), Some(Value::Number(2.0))),
            Value::Number(12.98)
        );
        assert_eq!(
            call_trunc(Value::Number(-12.987), Some(Value::Number(2.0))),
            Value::Number(-12.98)
        );
    }

    #[test]
    fn truncates_negative_digits_to_powers_of_ten() {
        assert_eq!(
            call_trunc(Value::Number(987.65), Some(Value::Number(-1.0))),
            Value::Number(980.0)
        );
        assert_eq!(
            call_trunc(Value::Number(-987.65), Some(Value::Number(-2.0))),
            Value::Number(-900.0)
        );
    }

    #[test]
    fn truncates_fractional_digit_counts_toward_zero() {
        assert_eq!(
            call_trunc(Value::Number(12.987), Some(Value::Number(1.9))),
            Value::Number(12.9)
        );
        assert_eq!(
            call_trunc(Value::Number(987.65), Some(Value::Number(-1.9))),
            Value::Number(980.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call_trunc(
                Value::Text(" 8.765 ".to_string()),
                Some(Value::Text("2".to_string()))
            ),
            Value::Number(8.76)
        );
        assert_eq!(call_trunc(Value::Boolean(true), None), Value::Number(1.0));
        assert_eq!(
            call_trunc(Value::Number(9.9), Some(Value::Blank)),
            Value::Number(9.0)
        );
        assert_eq!(call_trunc(Value::Blank, None), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_trunc(Value::Text("not numeric".to_string()), None),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_trunc(
                Value::Number(12.34),
                Some(Value::Text("not numeric".to_string()))
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_trunc(Value::Error(ErrorValue::Ref), Some(Value::Number(1.0))),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_trunc(number: Value, num_digits: Option<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let mut args = vec![Arg::Value(number)];
        if let Some(num_digits) = num_digits {
            args.push(Arg::Value(num_digits));
        }
        TRUNC.call(&args, &fn_ctx)
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
