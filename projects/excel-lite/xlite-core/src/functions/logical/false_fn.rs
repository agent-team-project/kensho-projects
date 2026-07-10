use crate::functions::prelude::*;

pub struct FalseFn;

impl Function for FalseFn {
    fn name(&self) -> &'static str {
        "FALSE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if !args.is_empty() {
            return Value::Error(ErrorValue::Value);
        }

        Value::Boolean(false)
    }
}

static FALSE: FalseFn = FalseFn;
inventory::submit! { FunctionEntry(&FALSE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    struct EmptyContext;

    impl EvalContext for EmptyContext {
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

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(FALSE.name(), "FALSE");
        assert_eq!(FALSE.arity(), (0, Some(0)));
    }

    #[test]
    fn returns_boolean_false() {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(FALSE.call(&[], &fn_ctx), Value::Boolean(false));
    }

    #[test]
    fn direct_call_rejects_arguments() {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);
        let one_arg = [Arg::Value(Value::Boolean(true))];
        let two_args = [
            Arg::Value(Value::Boolean(false)),
            Arg::Value(Value::Boolean(true)),
        ];

        assert_eq!(FALSE.call(&one_arg, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(FALSE.call(&two_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }
}
