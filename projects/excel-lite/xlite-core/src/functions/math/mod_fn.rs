use crate::functions::prelude::*;

pub struct Mod;

impl Function for Mod {
    fn name(&self) -> &'static str {
        "MOD"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let divisor = match ctx.to_number(&args[1].as_value()) {
            Ok(divisor) => divisor,
            Err(error) => return Value::Error(error),
        };

        if divisor == 0.0 {
            Value::Error(ErrorValue::Div0)
        } else {
            Value::Number(number - divisor * (number / divisor).floor())
        }
    }
}

static MOD: Mod = Mod;
inventory::submit! { FunctionEntry(&MOD) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(MOD.arity(), (2, Some(2)));
    }

    #[test]
    fn returns_positive_remainder_for_positive_inputs() {
        assert_eq!(
            call_mod(Value::Number(10.0), Value::Number(3.0)),
            Value::Number(1.0)
        );
    }

    #[test]
    fn remainder_uses_divisor_sign() {
        assert_eq!(
            call_mod(Value::Number(-3.0), Value::Number(2.0)),
            Value::Number(1.0)
        );
        assert_eq!(
            call_mod(Value::Number(3.0), Value::Number(-2.0)),
            Value::Number(-1.0)
        );
    }

    #[test]
    fn rejects_zero_divisor() {
        assert_eq!(
            call_mod(Value::Number(10.0), Value::Number(0.0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call_mod(Value::Text(" 10 ".to_string()), Value::Text("4".to_string())),
            Value::Number(2.0)
        );
        assert_eq!(
            call_mod(Value::Boolean(true), Value::Number(3.0)),
            Value::Number(1.0)
        );
        assert_eq!(
            call_mod(Value::Blank, Value::Number(3.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_scalar_coercion_errors() {
        assert_eq!(
            call_mod(Value::Text("not numeric".to_string()), Value::Number(3.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mod(Value::Number(10.0), Value::Error(ErrorValue::Ref)),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_mod(number: Value, divisor: Value) -> Value {
        MOD.call(&[Arg::Value(number), Arg::Value(divisor)], &fn_context())
    }

    fn fn_context() -> FnContext<'static> {
        static CTX: DummyContext = DummyContext;
        FnContext::new(&CTX)
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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
