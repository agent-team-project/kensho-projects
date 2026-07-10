use crate::functions::prelude::*;

pub struct Na;

impl Function for Na {
    fn name(&self) -> &'static str {
        "NA"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if !args.is_empty() {
            return Value::Error(ErrorValue::Value);
        }

        Value::Error(ErrorValue::Na)
    }
}

static NA: Na = Na;
inventory::submit! { FunctionEntry(&NA) }

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
        assert_eq!(NA.name(), "NA");
        assert_eq!(NA.arity(), (0, Some(0)));
    }

    #[test]
    fn returns_na_error() {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(NA.call(&[], &fn_ctx), Value::Error(ErrorValue::Na));
    }

    #[test]
    fn direct_call_rejects_arguments() {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);
        let one_arg = [Arg::Value(Value::Number(1.0))];
        let two_args = [
            Arg::Value(Value::Error(ErrorValue::Na)),
            Arg::Value(Value::Number(1.0)),
        ];

        assert_eq!(NA.call(&one_arg, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(NA.call(&two_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }
}
