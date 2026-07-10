use crate::functions::prelude::*;

pub struct IfError;

impl Function for IfError {
    fn name(&self) -> &'static str {
        "IFERROR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let value = args[0].as_value();
        match value {
            Value::Error(_) => args[1].as_value(),
            value => value,
        }
    }
}

static IFERROR: IfError = IfError;
inventory::submit! { FunctionEntry(&IFERROR) }

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
        assert_eq!(IFERROR.name(), "IFERROR");
        assert_eq!(IFERROR.arity(), (2, Some(2)));
    }

    #[test]
    fn returns_non_error_first_value_unchanged() {
        assert_eq!(
            call_iferror(Value::Number(7.0), Value::Number(99.0)),
            Value::Number(7.0)
        );
        assert_eq!(
            call_iferror(Value::Text("ok".to_string()), Value::Error(ErrorValue::Div0)),
            Value::Text("ok".to_string())
        );
        assert_eq!(
            call_iferror(Value::Blank, Value::Text("fallback".to_string())),
            Value::Blank
        );
    }

    #[test]
    fn returns_fallback_when_first_value_is_error() {
        assert_eq!(
            call_iferror(Value::Error(ErrorValue::Div0), Value::Number(99.0)),
            Value::Number(99.0)
        );
        assert_eq!(
            call_iferror(
                Value::Error(ErrorValue::Na),
                Value::Text("fallback".to_string())
            ),
            Value::Text("fallback".to_string())
        );
    }

    #[test]
    fn propagates_fallback_error_when_first_value_is_error() {
        assert_eq!(
            call_iferror(Value::Error(ErrorValue::Div0), Value::Error(ErrorValue::Na)),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let fn_ctx = fn_context();
        let one_arg = [Arg::Value(Value::Number(1.0))];
        let three_args = [
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Number(2.0)),
            Arg::Value(Value::Number(3.0)),
        ];

        assert_eq!(IFERROR.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            IFERROR.call(&one_arg, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            IFERROR.call(&three_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_iferror(value: Value, fallback: Value) -> Value {
        IFERROR.call(
            &[Arg::Value(value), Arg::Value(fallback)],
            &fn_context(),
        )
    }

    fn fn_context() -> FnContext<'static> {
        static CTX: DummyContext = DummyContext;
        FnContext::new(&CTX)
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
