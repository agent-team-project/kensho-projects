use crate::functions::prelude::*;

pub struct RandBetween;

impl Function for RandBetween {
    fn name(&self) -> &'static str {
        "RANDBETWEEN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let bottom = match ctx.to_number(&args[0].as_value()) {
            Ok(bottom) => bottom.trunc(),
            Err(error) => return Value::Error(error),
        };
        let top = match ctx.to_number(&args[1].as_value()) {
            Ok(top) => top.trunc(),
            Err(error) => return Value::Error(error),
        };

        if bottom > top {
            return Value::Error(ErrorValue::Num);
        }

        Value::Number(bottom + (ctx.rand() * (top - bottom + 1.0)).floor())
    }
}

static RANDBETWEEN: RandBetween = RandBetween;
inventory::submit! { FunctionEntry(&RANDBETWEEN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(RANDBETWEEN.arity(), (2, Some(2)));
    }

    #[test]
    fn maps_default_rng_to_inclusive_integer_range() {
        assert_eq!(
            call_with_rand(Value::Number(1.0), Value::Number(10.0), 0.5),
            Value::Number(6.0)
        );
    }

    #[test]
    fn includes_negative_and_positive_bounds() {
        assert_eq!(
            call_with_rand(Value::Number(-1.0), Value::Number(1.0), 0.5),
            Value::Number(0.0)
        );
        assert_eq!(
            call_with_rand(Value::Number(-1.0), Value::Number(1.0), 0.0),
            Value::Number(-1.0)
        );
        assert_eq!(
            call_with_rand(Value::Number(-1.0), Value::Number(1.0), 0.999),
            Value::Number(1.0)
        );
    }

    #[test]
    fn returns_degenerate_single_value_range() {
        assert_eq!(
            call_with_rand(Value::Number(5.0), Value::Number(5.0), 0.5),
            Value::Number(5.0)
        );
    }

    #[test]
    fn truncates_bounds_toward_zero_before_generating() {
        assert_eq!(
            call_with_rand(Value::Number(1.9), Value::Number(3.9), 0.5),
            Value::Number(2.0)
        );
        assert_eq!(
            call_with_rand(Value::Number(-1.9), Value::Number(1.9), 0.5),
            Value::Number(0.0)
        );
    }

    #[test]
    fn rejects_bottom_greater_than_top_after_truncation() {
        assert_eq!(
            call_with_rand(Value::Number(2.9), Value::Number(1.9), 0.5),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_number_coercion_for_bounds() {
        assert_eq!(
            call_with_rand(
                Value::Text(" 1.9 ".to_string()),
                Value::Text("3.2".to_string()),
                0.5
            ),
            Value::Number(2.0)
        );
        assert_eq!(
            call_with_rand(Value::Boolean(false), Value::Boolean(true), 0.5),
            Value::Number(1.0)
        );
        assert_eq!(
            call_with_rand(Value::Blank, Value::Number(0.0), 0.5),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_with_rand(
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Ref),
                0.5
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_with_rand(
                Value::Number(1.0),
                Value::Text("not numeric".to_string()),
                0.5
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_with_rand(Value::Error(ErrorValue::Ref), Value::Number(1.0), 0.5),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_with_rand(bottom: Value, top: Value, rand: f64) -> Value {
        let ctx = DummyContext { rand };
        let fn_ctx = FnContext::new(&ctx);
        RANDBETWEEN.call(&[Arg::Value(bottom), Arg::Value(top)], &fn_ctx)
    }

    struct DummyContext {
        rand: f64,
    }

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

        fn rand(&self) -> f64 {
            self.rand
        }
    }
}
