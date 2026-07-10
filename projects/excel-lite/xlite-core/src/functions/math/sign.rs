use crate::functions::prelude::*;

pub struct Sign;

impl Function for Sign {
    fn name(&self) -> &'static str {
        "SIGN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let value = args[0].as_value();
        let number = match ctx.to_number(&value) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };

        Value::Number(match number {
            number if number > 0.0 => 1.0,
            number if number < 0.0 => -1.0,
            _ => 0.0,
        })
    }
}

static SIGN: Sign = Sign;
inventory::submit! { FunctionEntry(&SIGN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exactly_one_argument() {
        assert_eq!(SIGN.arity(), (1, Some(1)));
    }

    #[test]
    fn classifies_number_signs() {
        assert_eq!(call_with(Value::Number(3.5)), Value::Number(1.0));
        assert_eq!(call_with(Value::Number(-2.0)), Value::Number(-1.0));
        assert_eq!(call_with(Value::Number(0.0)), Value::Number(0.0));
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(call_with(Value::Text(" 5 ".to_string())), Value::Number(1.0));
        assert_eq!(call_with(Value::Boolean(false)), Value::Number(0.0));
        assert_eq!(call_with(Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_with(Value::Text("x".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_with(Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_with(value: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        SIGN.call(&[Arg::Value(value)], &fn_ctx)
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
