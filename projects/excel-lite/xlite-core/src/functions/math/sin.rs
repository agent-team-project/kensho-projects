use crate::functions::prelude::*;

pub struct Sin;

impl Function for Sin {
    fn name(&self) -> &'static str {
        "SIN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match ctx.to_number(&args[0].as_value()) {
            Ok(number) => Value::Number(number.sin()),
            Err(error) => Value::Error(error),
        }
    }
}

static SIN: Sin = Sin;
inventory::submit! { FunctionEntry(&SIN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(SIN.name(), "SIN");
        assert_eq!(SIN.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_sine_for_radian_inputs() {
        assert_number_close(call(Value::Number(0.0)), 0.0);
        assert_number_close(call(Value::Number(std::f64::consts::FRAC_PI_2)), 1.0);
        assert_number_close(call(Value::Number(0.5)), 0.5_f64.sin());
    }

    #[test]
    fn preserves_odd_function_sign_behavior() {
        assert_number_close(call(Value::Number(-std::f64::consts::FRAC_PI_2)), -1.0);
        assert_number_close(call(Value::Number(-0.5)), -0.5_f64.sin());
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_number_close(call(Value::Text("0.5".to_string())), 0.5_f64.sin());
        assert_number_close(call(Value::Boolean(true)), 1.0_f64.sin());
        assert_number_close(call(Value::Blank), 0.0);
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
        SIN.call(&[Arg::Value(value)], &fn_ctx)
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
