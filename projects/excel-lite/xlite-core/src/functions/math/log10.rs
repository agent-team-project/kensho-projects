use crate::functions::prelude::*;

pub struct Log10;

impl Function for Log10 {
    fn name(&self) -> &'static str {
        "LOG10"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let value = args[0].as_value();
        match ctx.to_number(&value) {
            Ok(number) if number <= 0.0 => Value::Error(ErrorValue::Num),
            Ok(number) => Value::Number(number.log10()),
            Err(error) => Value::Error(error),
        }
    }
}

static LOG10: Log10 = Log10;
inventory::submit! { FunctionEntry(&LOG10) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(LOG10.name(), "LOG10");
        assert_eq!(LOG10.arity(), (1, Some(1)));
    }

    #[test]
    fn powers_of_ten_return_exponents() {
        assert_number_close(call(Value::Number(10.0)), 1.0);
        assert_number_close(call(Value::Number(100_000.0)), 5.0);
    }

    #[test]
    fn non_power_positive_numbers_return_base_ten_logarithm() {
        assert_number_close(call(Value::Number(86.0)), 86.0_f64.log10());
    }

    #[test]
    fn non_positive_numbers_return_num_error() {
        assert_eq!(call(Value::Number(0.0)), Value::Error(ErrorValue::Num));
        assert_eq!(call(Value::Number(-1.0)), Value::Error(ErrorValue::Num));
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_number_close(call(Value::Text("100".to_string())), 2.0);
        assert_number_close(call(Value::Boolean(true)), 0.0);
        assert_eq!(call(Value::Blank), Value::Error(ErrorValue::Num));
    }

    #[test]
    fn coercion_errors_propagate() {
        assert_eq!(
            call(Value::Text("not numeric".to_string())),
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
        LOG10.call(&[Arg::Value(value)], &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => {
                let tolerance = 1e-12 * expected.abs().max(1.0);
                assert!(
                    (actual - expected).abs() <= tolerance,
                    "expected {expected}, got {actual}"
                );
            }
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
