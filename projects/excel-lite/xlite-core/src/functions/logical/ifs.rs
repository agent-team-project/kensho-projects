use crate::functions::prelude::*;

pub struct Ifs;

impl Function for Ifs {
    fn name(&self) -> &'static str {
        "IFS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(254))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 2 || args.len() > 254 || args.len() % 2 != 0 {
            return Value::Error(ErrorValue::Value);
        }

        for pair in args.chunks_exact(2) {
            let test = pair[0].as_value();
            let test = match ctx.to_bool(&test) {
                Ok(value) => value,
                Err(error) => return Value::Error(error),
            };

            if test {
                return pair[1].as_value();
            }
        }

        Value::Error(ErrorValue::Na)
    }
}

static IFS: Ifs = Ifs;
inventory::submit! { FunctionEntry(&IFS) }

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
        assert_eq!(IFS.name(), "IFS");
        assert_eq!(IFS.arity(), (2, Some(254)));
    }

    #[test]
    fn returns_first_true_branch() {
        assert_eq!(
            call_ifs(vec![
                Value::Boolean(false),
                Value::Text("no".to_string()),
                Value::Boolean(true),
                Value::Text("yes".to_string()),
            ]),
            Value::Text("yes".to_string())
        );
        assert_eq!(
            call_ifs(vec![
                Value::Number(1.0),
                Value::Text("first".to_string()),
                Value::Boolean(true),
                Value::Text("second".to_string()),
            ]),
            Value::Text("first".to_string())
        );
    }

    #[test]
    fn returns_na_when_no_test_is_true() {
        assert_eq!(
            call_ifs(vec![
                Value::Boolean(false),
                Value::Number(1.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn inspected_test_errors_propagate() {
        assert_eq!(
            call_ifs(vec![
                Value::Error(ErrorValue::Div0),
                Value::Text("bad".to_string()),
                Value::Boolean(true),
                Value::Text("ok".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn selected_value_errors_propagate() {
        assert_eq!(
            call_ifs(vec![Value::Boolean(true), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn false_branch_value_errors_are_ignored() {
        assert_eq!(
            call_ifs(vec![
                Value::Boolean(false),
                Value::Error(ErrorValue::Div0),
                Value::Boolean(true),
                Value::Text("ok".to_string()),
            ]),
            Value::Text("ok".to_string())
        );
    }

    #[test]
    fn later_tests_and_values_after_match_are_ignored() {
        assert_eq!(
            call_ifs(vec![
                Value::Boolean(true),
                Value::Text("ok".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Text("ok".to_string())
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let fn_ctx = fn_context();
        let odd_args = [
            Arg::Value(Value::Boolean(false)),
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Boolean(true)),
        ];
        let too_many_args = vec![Arg::Value(Value::Boolean(false)); 256];

        assert_eq!(IFS.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            IFS.call(&[Arg::Value(Value::Boolean(true))], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(IFS.call(&odd_args, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            IFS.call(&too_many_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_ifs(values: Vec<Value>) -> Value {
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        IFS.call(&args, &fn_context())
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
