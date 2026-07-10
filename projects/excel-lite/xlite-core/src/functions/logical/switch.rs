use crate::functions::prelude::*;

pub struct Switch;

impl Function for Switch {
    fn name(&self) -> &'static str {
        "SWITCH"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(254))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 3 || args.len() > 254 {
            return Value::Error(ErrorValue::Value);
        }

        let expression = args[0].as_value();
        if let Some(error) = expression.as_error() {
            return Value::Error(error);
        }

        let remaining = &args[1..];
        let pair_count = remaining.len() / 2;
        let default = if remaining.len() % 2 == 1 {
            remaining.last()
        } else {
            None
        };

        for pair in remaining[..pair_count * 2].chunks_exact(2) {
            let value = pair[0].as_value();
            let matched = match ctx.values_equal(&expression, &value) {
                Ok(value) => value,
                Err(error) => return Value::Error(error),
            };

            if matched {
                return pair[1].as_value();
            }
        }

        default
            .map(Arg::as_value)
            .unwrap_or(Value::Error(ErrorValue::Na))
    }
}

static SWITCH: Switch = Switch;
inventory::submit! { FunctionEntry(&SWITCH) }

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
        assert_eq!(SWITCH.name(), "SWITCH");
        assert_eq!(SWITCH.arity(), (3, Some(254)));
    }

    #[test]
    fn returns_first_matching_result() {
        assert_eq!(
            call_switch(vec![
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Text("one".to_string()),
                Value::Number(2.0),
                Value::Text("two".to_string()),
                Value::Text("other".to_string()),
            ]),
            Value::Text("two".to_string())
        );
        assert_eq!(
            call_switch(vec![
                Value::Text("B".to_string()),
                Value::Text("a".to_string()),
                Value::Number(1.0),
                Value::Text("b".to_string()),
                Value::Number(2.0),
                Value::Number(0.0),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn returns_default_when_no_match_exists() {
        assert_eq!(
            call_switch(vec![
                Value::Text("z".to_string()),
                Value::Text("a".to_string()),
                Value::Number(1.0),
                Value::Text("b".to_string()),
                Value::Number(2.0),
                Value::Number(0.0),
            ]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_switch(vec![
                Value::Text("z".to_string()),
                Value::Text("a".to_string()),
                Value::Number(1.0),
                Value::Text("b".to_string()),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn expression_errors_propagate() {
        assert_eq!(
            call_switch(vec![
                Value::Error(ErrorValue::Div0),
                Value::Number(1.0),
                Value::Text("one".to_string()),
                Value::Text("other".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn inspected_match_value_errors_propagate() {
        assert_eq!(
            call_switch(vec![
                Value::Number(1.0),
                Value::Error(ErrorValue::Div0),
                Value::Text("bad".to_string()),
                Value::Number(1.0),
                Value::Text("one".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn selected_result_errors_propagate() {
        assert_eq!(
            call_switch(vec![
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Error(ErrorValue::Div0),
                Value::Number(2.0),
                Value::Text("two".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn non_matching_result_errors_are_ignored() {
        assert_eq!(
            call_switch(vec![
                Value::Number(2.0),
                Value::Number(1.0),
                Value::Error(ErrorValue::Div0),
                Value::Number(2.0),
                Value::Text("two".to_string()),
            ]),
            Value::Text("two".to_string())
        );
    }

    #[test]
    fn later_args_after_match_are_ignored() {
        assert_eq!(
            call_switch(vec![
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Text("one".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
                Value::Error(ErrorValue::Value),
            ]),
            Value::Text("one".to_string())
        );
    }

    #[test]
    fn default_error_is_returned_when_no_match_exists() {
        assert_eq!(
            call_switch(vec![
                Value::Text("z".to_string()),
                Value::Text("a".to_string()),
                Value::Number(1.0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let fn_ctx = fn_context();
        let too_many_args = vec![Arg::Value(Value::Number(1.0)); 255];

        assert_eq!(SWITCH.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            SWITCH.call(&[Arg::Value(Value::Number(1.0))], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            SWITCH.call(
                &[Arg::Value(Value::Number(1.0)), Arg::Value(Value::Number(1.0))],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            SWITCH.call(&too_many_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_switch(values: Vec<Value>) -> Value {
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        SWITCH.call(&args, &fn_context())
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
