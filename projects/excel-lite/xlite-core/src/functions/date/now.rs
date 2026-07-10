use crate::functions::prelude::*;

pub struct Now;

impl Function for Now {
    fn name(&self) -> &'static str {
        "NOW"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !args.is_empty() {
            return Value::Error(ErrorValue::Value);
        }

        Value::Number(ctx.now_serial())
    }
}

static NOW: Now = Now;
inventory::submit! { FunctionEntry(&NOW) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(NOW.name(), "NOW");
        assert_eq!(NOW.arity(), (0, Some(0)));
    }

    #[test]
    fn returns_default_context_now_serial() {
        assert_eq!(call_with(&DefaultNowContext), Value::Number(1.0));
    }

    #[test]
    fn delegates_to_eval_context_now_hook() {
        assert_eq!(call_with(&FixedNowContext(45_000.25)), Value::Number(45_000.25));
        assert_eq!(call_with(&FixedNowContext(45_001.5)), Value::Number(45_001.5));
    }

    #[test]
    fn direct_call_rejects_arguments() {
        let ctx = DefaultNowContext;
        let fn_ctx = FnContext::new(&ctx);
        let one_arg = [Arg::Value(Value::Number(1.0))];
        let two_args = [
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Number(2.0)),
        ];

        assert_eq!(NOW.call(&one_arg, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(NOW.call(&two_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    fn call_with(ctx: &dyn EvalContext) -> Value {
        let fn_ctx = FnContext::new(ctx);
        NOW.call(&[], &fn_ctx)
    }

    struct DefaultNowContext;

    impl EvalContext for DefaultNowContext {
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

    struct FixedNowContext(f64);

    impl EvalContext for FixedNowContext {
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

        fn now_serial(&self) -> f64 {
            self.0
        }
    }
}
