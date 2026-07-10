use crate::functions::prelude::*;

pub struct Ifna;

impl Function for Ifna {
    fn name(&self) -> &'static str {
        "IFNA"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let value = args[0].as_value();
        if value == Value::Error(ErrorValue::Na) {
            args[1].as_value()
        } else {
            value
        }
    }
}

static IFNA: Ifna = Ifna;
inventory::submit! { FunctionEntry(&IFNA) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(IFNA.name(), "IFNA");
        assert_eq!(IFNA.arity(), (2, Some(2)));
    }

    #[test]
    fn returns_non_error_value_unchanged() {
        assert_eq!(
            call_ifna(vec![
                Value::Text("ok".to_string()),
                Value::Text("fallback".to_string()),
            ]),
            Value::Text("ok".to_string())
        );
    }

    #[test]
    fn returns_fallback_for_na_error() {
        assert_eq!(
            call_ifna(vec![Value::Error(ErrorValue::Na), Value::Number(42.0)]),
            Value::Number(42.0)
        );
    }

    #[test]
    fn returns_non_na_errors_unchanged() {
        assert_eq!(
            call_ifna(vec![Value::Error(ErrorValue::Div0), Value::Number(42.0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn fallback_errors_propagate_when_first_value_is_na() {
        assert_eq!(
            call_ifna(vec![
                Value::Error(ErrorValue::Na),
                Value::Error(ErrorValue::Value),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let one_arg = [Arg::Value(Value::Error(ErrorValue::Na))];
        let three_args = [
            Arg::Value(Value::Error(ErrorValue::Na)),
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Number(2.0)),
        ];

        assert_eq!(IFNA.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(IFNA.call(&one_arg, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(IFNA.call(&three_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    fn call_ifna(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        IFNA.call(&args, &fn_ctx)
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
