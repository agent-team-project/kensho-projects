use crate::functions::prelude::*;

pub struct Abs;

impl Function for Abs {
    fn name(&self) -> &'static str {
        "ABS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match ctx.to_number(&args[0].as_value()) {
            Ok(number) => Value::Number(number.abs()),
            Err(error) => Value::Error(error),
        }
    }
}

static ABS: Abs = Abs;
inventory::submit! { FunctionEntry(&ABS) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn returns_absolute_value_for_negative_number() {
        assert_eq!(call_abs(Value::Number(-42.5)), Value::Number(42.5));
    }

    #[test]
    fn leaves_positive_number_unchanged() {
        assert_eq!(call_abs(Value::Number(7.0)), Value::Number(7.0));
    }

    #[test]
    fn coerces_blank_to_zero() {
        assert_eq!(call_abs(Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn coerces_numeric_text() {
        assert_eq!(
            call_abs(Value::Text(" -3.25 ".to_string())),
            Value::Number(3.25)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(call_abs(Value::Error(ErrorValue::Div0)), Value::Error(ErrorValue::Div0));
        assert_eq!(
            call_abs(Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_abs(value: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        ABS.call(&[Arg::Value(value)], &fn_ctx)
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
