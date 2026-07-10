use crate::functions::prelude::*;

pub struct Int;

impl Function for Int {
    fn name(&self) -> &'static str {
        "INT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match ctx.to_number(&args[0].as_value()) {
            Ok(number) => Value::Number(number.floor()),
            Err(error) => Value::Error(error),
        }
    }
}

static INT: Int = Int;
inventory::submit! { FunctionEntry(&INT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(INT.arity(), (1, Some(1)));
    }

    #[test]
    fn floors_positive_decimal_down() {
        assert_eq!(call_int(Value::Number(12.9)), Value::Number(12.0));
        assert_eq!(call_int(Value::Number(12.0)), Value::Number(12.0));
    }

    #[test]
    fn floors_negative_decimal_toward_negative_infinity() {
        assert_eq!(call_int(Value::Number(-2.5)), Value::Number(-3.0));
        assert_eq!(call_int(Value::Number(-2.0)), Value::Number(-2.0));
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call_int(Value::Text(" 8.75 ".to_string())),
            Value::Number(8.0)
        );
        assert_eq!(call_int(Value::Boolean(true)), Value::Number(1.0));
        assert_eq!(call_int(Value::Boolean(false)), Value::Number(0.0));
        assert_eq!(call_int(Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_int(Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_int(Value::Error(ErrorValue::Ref)),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_int(value: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        INT.call(&[Arg::Value(value)], &fn_ctx)
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
