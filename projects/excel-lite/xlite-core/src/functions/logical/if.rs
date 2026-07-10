use crate::functions::prelude::*;

pub struct If;

impl Function for If {
    fn name(&self) -> &'static str {
        "IF"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 2 || args.len() > 3 {
            return Value::Error(ErrorValue::Value);
        }

        let condition = args[0].as_value();
        let condition = match ctx.to_bool(&condition) {
            Ok(value) => value,
            Err(error) => return Value::Error(error),
        };

        if condition {
            args[1].as_value()
        } else {
            args.get(2).map(Arg::as_value).unwrap_or(Value::Blank)
        }
    }
}

static IF: If = If;
inventory::submit! { FunctionEntry(&IF) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(IF.name(), "IF");
        assert_eq!(IF.arity(), (2, Some(3)));
    }

    #[test]
    fn returns_true_branch_when_condition_is_true() {
        assert_eq!(
            call_if(vec![
                Value::Boolean(true),
                Value::Text("yes".to_string()),
                Value::Text("no".to_string()),
            ]),
            Value::Text("yes".to_string())
        );
    }

    #[test]
    fn returns_false_branch_when_condition_is_false() {
        assert_eq!(
            call_if(vec![
                Value::Boolean(false),
                Value::Text("yes".to_string()),
                Value::Text("no".to_string()),
            ]),
            Value::Text("no".to_string())
        );
    }

    #[test]
    fn omitted_false_branch_returns_blank() {
        assert_eq!(
            call_if(vec![Value::Boolean(false), Value::Number(1.0)]),
            Value::Blank
        );
    }

    #[test]
    fn coerces_logical_test_with_fn_context() {
        assert_eq!(
            call_if(vec![
                Value::Number(2.0),
                Value::Text("nonzero".to_string()),
                Value::Text("zero".to_string()),
            ]),
            Value::Text("nonzero".to_string())
        );
        assert_eq!(
            call_if(vec![
                Value::Number(0.0),
                Value::Text("nonzero".to_string()),
                Value::Text("zero".to_string()),
            ]),
            Value::Text("zero".to_string())
        );
        assert_eq!(
            call_if(vec![
                Value::Text("TRUE".to_string()),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_if(vec![
                Value::Text("FALSE".to_string()),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn invalid_logical_text_returns_value_error() {
        assert_eq!(
            call_if(vec![
                Value::Text("maybe".to_string()),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn chosen_branch_errors_propagate() {
        assert_eq!(
            call_if(vec![
                Value::Boolean(true),
                Value::Error(ErrorValue::Div0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_if(vec![
                Value::Boolean(false),
                Value::Number(1.0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn unchosen_branch_errors_do_not_preempt_selected_result() {
        assert_eq!(
            call_if(vec![
                Value::Boolean(true),
                Value::Number(7.0),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Number(7.0)
        );
        assert_eq!(
            call_if(vec![
                Value::Boolean(false),
                Value::Error(ErrorValue::Div0),
                Value::Number(8.0),
            ]),
            Value::Number(8.0)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);
        let one_arg = [Arg::Value(Value::Boolean(true))];
        let four_args = [
            Arg::Value(Value::Boolean(true)),
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Number(0.0)),
            Arg::Value(Value::Number(9.0)),
        ];

        assert_eq!(IF.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(IF.call(&one_arg, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(IF.call(&four_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    fn call_if(values: Vec<Value>) -> Value {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        IF.call(&args, &fn_ctx)
    }

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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
