use crate::functions::prelude::*;

pub struct Ln;

impl Function for Ln {
    fn name(&self) -> &'static str {
        "LN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let value = args[0].as_value();
        match ctx.to_number(&value) {
            Ok(number) if number <= 0.0 => Value::Error(ErrorValue::Num),
            Ok(number) => Value::Number(number.ln()),
            Err(error) => Value::Error(error),
        }
    }
}

static LN: Ln = Ln;
inventory::submit! { FunctionEntry(&LN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(LN.name(), "LN");
        assert_eq!(LN.arity(), (1, Some(1)));
    }

    #[test]
    fn positive_numbers_return_natural_logarithm() {
        assert_number_close(call(Value::Number(std::f64::consts::E)), 1.0);
    }

    #[test]
    fn non_positive_numbers_return_num_error() {
        assert_eq!(call(Value::Number(0.0)), Value::Error(ErrorValue::Num));
        assert_eq!(call(Value::Number(-1.0)), Value::Error(ErrorValue::Num));
    }

    #[test]
    fn numeric_text_uses_scalar_coercion() {
        assert_number_close(call(Value::Text("4".to_string())), 4.0_f64.ln());
    }

    #[test]
    fn blank_coerces_to_zero_and_returns_num_error() {
        assert_eq!(call(Value::Blank), Value::Error(ErrorValue::Num));
    }

    #[test]
    fn coercion_errors_propagate() {
        assert_eq!(
            call(Value::Text("not a number".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call(value: Value) -> Value {
        let eval_ctx = TestContext;
        let fn_ctx = FnContext::new(&eval_ctx);
        LN.call(&[Arg::Value(value)], &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= 1e-12,
                "expected {expected}, got {actual}"
            ),
            other => panic!("expected number {expected}, got {other:?}"),
        }
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
