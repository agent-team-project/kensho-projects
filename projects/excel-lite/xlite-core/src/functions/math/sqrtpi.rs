use crate::functions::prelude::*;

pub struct SqrtPi;

impl Function for SqrtPi {
    fn name(&self) -> &'static str {
        "SQRTPI"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match ctx.to_number(&args[0].as_value()) {
            Ok(number) if number < 0.0 => Value::Error(ErrorValue::Num),
            Ok(number) => Value::Number((number * std::f64::consts::PI).sqrt()),
            Err(error) => Value::Error(error),
        }
    }
}

static SQRTPI: SqrtPi = SqrtPi;
inventory::submit! { FunctionEntry(&SQRTPI) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(SQRTPI.name(), "SQRTPI");
        assert_eq!(SQRTPI.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_square_root_of_number_times_pi() {
        assert_number_close(call(Value::Number(1.0)), std::f64::consts::PI.sqrt());
        assert_number_close(
            call(Value::Number(2.0)),
            (2.0 * std::f64::consts::PI).sqrt(),
        );
    }

    #[test]
    fn returns_zero_for_zero() {
        assert_eq!(call(Value::Number(0.0)), Value::Number(0.0));
    }

    #[test]
    fn negative_numbers_return_num_error() {
        assert_eq!(call(Value::Number(-1.0)), Value::Error(ErrorValue::Num));
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_number_close(
            call(Value::Text("4".to_string())),
            (4.0 * std::f64::consts::PI).sqrt(),
        );
        assert_number_close(call(Value::Boolean(true)), std::f64::consts::PI.sqrt());
        assert_eq!(call(Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors() {
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
        SQRTPI.call(&[Arg::Value(value)], &fn_ctx)
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
