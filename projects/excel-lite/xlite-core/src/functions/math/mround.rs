use crate::functions::prelude::*;

pub struct Mround;

impl Function for Mround {
    fn name(&self) -> &'static str {
        "MROUND"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let multiple = match ctx.to_number(&args[1].as_value()) {
            Ok(multiple) => multiple,
            Err(error) => return Value::Error(error),
        };

        if multiple == 0.0 {
            return Value::Number(0.0);
        }

        if (number > 0.0 && multiple < 0.0) || (number < 0.0 && multiple > 0.0) {
            return Value::Error(ErrorValue::Num);
        }

        Value::Number((number / multiple).round() * multiple)
    }
}

static MROUND: Mround = Mround;
inventory::submit! { FunctionEntry(&MROUND) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(MROUND.arity(), (2, Some(2)));
    }

    #[test]
    fn rounds_positive_numbers_to_nearest_multiple() {
        assert_eq!(
            call(Value::Number(10.0), Value::Number(3.0)),
            Value::Number(9.0)
        );
        assert_eq!(
            call(Value::Number(11.0), Value::Number(3.0)),
            Value::Number(12.0)
        );
    }

    #[test]
    fn rounds_negative_numbers_to_nearest_negative_multiple() {
        assert_eq!(
            call(Value::Number(-10.0), Value::Number(-3.0)),
            Value::Number(-9.0)
        );
        assert_eq!(
            call(Value::Number(-11.0), Value::Number(-3.0)),
            Value::Number(-12.0)
        );
    }

    #[test]
    fn rounds_ties_away_from_zero() {
        assert_eq!(
            call(Value::Number(3.0), Value::Number(2.0)),
            Value::Number(4.0)
        );
        assert_eq!(
            call(Value::Number(-3.0), Value::Number(-2.0)),
            Value::Number(-4.0)
        );
    }

    #[test]
    fn returns_zero_for_zero_multiple_or_zero_number() {
        assert_eq!(
            call(Value::Number(10.0), Value::Number(0.0)),
            Value::Number(0.0)
        );
        assert_eq!(
            call(Value::Number(0.0), Value::Number(-5.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn rejects_opposite_nonzero_signs() {
        assert_eq!(
            call(Value::Number(5.0), Value::Number(-2.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(Value::Number(-5.0), Value::Number(2.0)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call(Value::Text("10".to_string()), Value::Text("4".to_string())),
            Value::Number(12.0)
        );
        assert_eq!(
            call(Value::Boolean(true), Value::Boolean(true)),
            Value::Number(1.0)
        );
        assert_eq!(
            call(Value::Blank, Value::Number(5.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_scalar_coercion_errors() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Value::Number(3.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Number(10.0), Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Ref), Value::Number(3.0)),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call(Value::Number(10.0), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call(number: Value, multiple: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        MROUND.call(&[Arg::Value(number), Arg::Value(multiple)], &fn_ctx)
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
