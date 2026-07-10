use crate::functions::prelude::*;

pub struct Sqrt;

impl Function for Sqrt {
    fn name(&self) -> &'static str {
        "SQRT"
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

        if number < 0.0 {
            Value::Error(ErrorValue::Num)
        } else {
            Value::Number(number.sqrt())
        }
    }
}

static SQRT: Sqrt = Sqrt;
inventory::submit! { FunctionEntry(&SQRT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn returns_square_root_for_positive_number() {
        assert_eq!(call(Value::Number(144.0)), Value::Number(12.0));
    }

    #[test]
    fn rejects_negative_numbers() {
        assert_eq!(call(Value::Number(-1.0)), Value::Error(ErrorValue::Num));
    }

    #[test]
    fn coerces_numeric_text() {
        assert_eq!(call(Value::Text("2.25".to_string())), Value::Number(1.5));
    }

    #[test]
    fn coerces_blank_to_zero() {
        assert_eq!(call(Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call(Value::Text("not a number".to_string())),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call(value: Value) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        SQRT.call(&[Arg::Value(value)], &fn_ctx)
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
