use crate::functions::prelude::*;

pub struct Today;

impl Function for Today {
    fn name(&self) -> &'static str {
        "TODAY"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !args.is_empty() {
            return Value::Error(ErrorValue::Value);
        }

        Value::Number(ctx.today_serial())
    }
}

static TODAY: Today = Today;
inventory::submit! { FunctionEntry(&TODAY) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(TODAY.name(), "TODAY");
        assert_eq!(TODAY.arity(), (0, Some(0)));
    }

    #[test]
    fn returns_default_context_today_serial() {
        assert_eq!(call_with(&DefaultTodayContext), Value::Number(1.0));
    }

    #[test]
    fn delegates_to_eval_context_today_hook() {
        assert_eq!(call_with(&FixedTodayContext(45_000.0)), Value::Number(45_000.0));
        assert_eq!(call_with(&FixedTodayContext(46_123.0)), Value::Number(46_123.0));
    }

    #[test]
    fn direct_call_rejects_arguments() {
        let ctx = FixedTodayContext(45_000.0);
        let fn_ctx = FnContext::new(&ctx);
        let one_arg = [Arg::Value(Value::Number(1.0))];
        let two_args = [
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Number(2.0)),
        ];

        assert_eq!(TODAY.call(&one_arg, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(TODAY.call(&two_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    fn call_with(ctx: &dyn EvalContext) -> Value {
        let fn_ctx = FnContext::new(ctx);
        TODAY.call(&[], &fn_ctx)
    }

    struct DefaultTodayContext;

    impl EvalContext for DefaultTodayContext {
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

    struct FixedTodayContext(f64);

    impl EvalContext for FixedTodayContext {
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

        fn today_serial(&self) -> f64 {
            self.0
        }
    }
}
