use crate::functions::prelude::*;

pub struct Exact;

impl Function for Exact {
    fn name(&self) -> &'static str {
        "EXACT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let text1 = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let text2 = match ctx.to_text(&args[1].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };

        Value::Boolean(text1 == text2)
    }
}

static EXACT: Exact = Exact;
inventory::submit! { FunctionEntry(&EXACT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_two_arguments() {
        assert_eq!(EXACT.name(), "EXACT");
        assert_eq!(EXACT.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_exact(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_exact(vec![Value::Text("only".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_exact(vec![
                Value::Text("same".to_string()),
                Value::Text("same".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_true_for_identical_text() {
        assert_eq!(
            call_exact(vec![
                Value::Text("Excel".to_string()),
                Value::Text("Excel".to_string()),
            ]),
            Value::Boolean(true)
        );
    }

    #[test]
    fn is_case_sensitive() {
        assert_eq!(
            call_exact(vec![
                Value::Text("Excel".to_string()),
                Value::Text("excel".to_string()),
            ]),
            Value::Boolean(false)
        );
    }

    #[test]
    fn treats_whitespace_differences_as_not_equal() {
        assert_eq!(
            call_exact(vec![
                Value::Text("Excel".to_string()),
                Value::Text("Excel ".to_string()),
            ]),
            Value::Boolean(false)
        );
    }

    #[test]
    fn coerces_scalars_to_text_before_comparing() {
        assert_eq!(
            call_exact(vec![Value::Number(123.0), Value::Text("123".to_string())]),
            Value::Boolean(true)
        );
        assert_eq!(
            call_exact(vec![Value::Boolean(true), Value::Text("TRUE".to_string())]),
            Value::Boolean(true)
        );
        assert_eq!(
            call_exact(vec![Value::Blank, Value::Text(String::new())]),
            Value::Boolean(true)
        );
    }

    #[test]
    fn propagates_text_coercion_errors_left_to_right() {
        assert_eq!(
            call_exact(vec![
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Ref),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_exact(vec![
                Value::Text("ok".to_string()),
                Value::Error(ErrorValue::Ref),
            ]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn compares_unicode_without_normalizing() {
        assert_eq!(
            call_exact(vec![
                Value::Text("Cafe\u{301}".to_string()),
                Value::Text("Cafe\u{301}".to_string()),
            ]),
            Value::Boolean(true)
        );
        assert_eq!(
            call_exact(vec![
                Value::Text("Caf\u{00e9}".to_string()),
                Value::Text("Cafe\u{301}".to_string()),
            ]),
            Value::Boolean(false)
        );
    }

    fn call_exact(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        EXACT.call(&args, &fn_ctx)
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
            CellId {
                sheet: 0,
                coord: Coord { row: 0, col: 0 },
            }
        }
    }
}
