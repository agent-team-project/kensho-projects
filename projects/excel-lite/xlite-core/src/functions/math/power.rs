use crate::functions::prelude::*;

pub struct Power;

impl Function for Power {
    fn name(&self) -> &'static str {
        "POWER"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let base = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let exponent = match ctx.to_number(&args[1].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };

        let result = base.powf(exponent);
        if result.is_finite() {
            Value::Number(result)
        } else {
            Value::Error(ErrorValue::Num)
        }
    }
}

static POWER: Power = Power;
inventory::submit! { FunctionEntry(&POWER) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(POWER.name(), "POWER");
        assert_eq!(POWER.arity(), (2, Some(2)));
    }

    #[test]
    fn raises_base_to_exponent() {
        assert_eq!(
            call(Value::Number(2.0), Value::Number(3.0)),
            Value::Number(8.0)
        );
        assert_eq!(
            call(Value::Number(-2.0), Value::Number(3.0)),
            Value::Number(-8.0)
        );
    }

    #[test]
    fn supports_identity_and_zero_boundaries() {
        assert_eq!(
            call(Value::Number(5.0), Value::Number(0.0)),
            Value::Number(1.0)
        );
        assert_eq!(
            call(Value::Number(0.0), Value::Number(3.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion_for_both_args() {
        assert_eq!(
            call(
                Value::Text(" 4 ".to_string()),
                Value::Text("0.5".to_string())
            ),
            Value::Number(2.0)
        );
        assert_eq!(
            call(Value::Boolean(true), Value::Number(4.0)),
            Value::Number(1.0)
        );
    }

    #[test]
    fn coerces_blank_arguments_to_zero() {
        assert_eq!(
            call(Value::Blank, Value::Number(2.0)),
            Value::Number(0.0)
        );
        assert_eq!(
            call(Value::Number(2.0), Value::Blank),
            Value::Number(1.0)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Value::Number(2.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Number(2.0), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn invalid_numeric_domains_return_num_error() {
        assert_eq!(
            call(Value::Number(-4.0), Value::Number(0.5)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(Value::Number(0.0), Value::Number(-1.0)),
            Value::Error(ErrorValue::Num)
        );
    }

    fn call(base: Value, exponent: Value) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        POWER.call(&[Arg::Value(base), Arg::Value(exponent)], &fn_ctx)
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
