use crate::functions::prelude::*;

pub struct Exp;

impl Function for Exp {
    fn name(&self) -> &'static str {
        "EXP"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match ctx.to_number(&args[0].as_value()) {
            Ok(number) => Value::Number(number.exp()),
            Err(error) => Value::Error(error),
        }
    }
}

static EXP: Exp = Exp;
inventory::submit! { FunctionEntry(&EXP) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(EXP.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_e_to_the_input() {
        assert_number_close(
            EXP.call(&[Arg::Value(Value::Number(1.0))], &fn_context()),
            std::f64::consts::E,
        );
    }

    #[test]
    fn coerces_numeric_text_and_blank_scalars() {
        assert_number_close(
            EXP.call(
                &[Arg::Value(Value::Text("2".to_string()))],
                &fn_context(),
            ),
            2.0_f64.exp(),
        );
        assert_number_close(
            EXP.call(&[Arg::Value(Value::Blank)], &fn_context()),
            1.0,
        );
    }

    #[test]
    fn propagates_scalar_coercion_errors() {
        assert_eq!(
            EXP.call(
                &[Arg::Value(Value::Text("not numeric".to_string()))],
                &fn_context()
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            EXP.call(
                &[Arg::Value(Value::Error(ErrorValue::Ref))],
                &fn_context()
            ),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn fn_context() -> FnContext<'static> {
        static CTX: DummyContext = DummyContext;
        FnContext::new(&CTX)
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
