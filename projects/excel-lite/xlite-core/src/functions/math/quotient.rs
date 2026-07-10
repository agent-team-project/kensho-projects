use crate::functions::prelude::*;

pub struct Quotient;

impl Function for Quotient {
    fn name(&self) -> &'static str {
        "QUOTIENT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let numerator = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let denominator = match ctx.to_number(&args[1].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };

        if denominator == 0.0 {
            return Value::Error(ErrorValue::Div0);
        }

        Value::Number((numerator / denominator).trunc())
    }
}

static QUOTIENT: Quotient = Quotient;
inventory::submit! { FunctionEntry(&QUOTIENT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(QUOTIENT.arity(), (2, Some(2)));
    }

    #[test]
    fn returns_integer_portion_for_positive_division() {
        assert_eq!(
            call(Value::Number(9.0), Value::Number(2.0)),
            Value::Number(4.0)
        );
    }

    #[test]
    fn truncates_negative_results_toward_zero() {
        assert_eq!(
            call(Value::Number(-7.0), Value::Number(2.0)),
            Value::Number(-3.0)
        );
        assert_eq!(
            call(Value::Number(7.0), Value::Number(-2.0)),
            Value::Number(-3.0)
        );
    }

    #[test]
    fn returns_div0_for_zero_denominator() {
        assert_eq!(
            call(Value::Number(7.0), Value::Number(0.0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call(Value::Text(" 8.9 ".to_string()), Value::Boolean(true)),
            Value::Number(8.0)
        );
        assert_eq!(
            call(Value::Boolean(true), Value::Number(2.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn coerces_blanks_to_zero() {
        assert_eq!(call(Value::Blank, Value::Number(3.0)), Value::Number(0.0));
        assert_eq!(
            call(Value::Number(3.0), Value::Blank),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Value::Number(1.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Number(1.0), Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Ref), Value::Number(1.0)),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call(numerator: Value, denominator: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        QUOTIENT.call(&[Arg::Value(numerator), Arg::Value(denominator)], &fn_ctx)
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
