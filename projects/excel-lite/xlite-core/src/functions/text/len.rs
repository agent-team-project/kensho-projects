use crate::functions::prelude::*;

pub struct Len;

impl Function for Len {
    fn name(&self) -> &'static str {
        "LEN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match ctx.to_text(&args[0].as_value()) {
            Ok(text) => Value::Number(text.chars().count() as f64),
            Err(error) => Value::Error(error),
        }
    }
}

static LEN: Len = Len;
inventory::submit! { FunctionEntry(&LEN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_argument() {
        assert_eq!(LEN.name(), "LEN");
        assert_eq!(LEN.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_len(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_len(vec![
                Value::Text("abc".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn counts_text_characters() {
        assert_eq!(
            call_len(vec![Value::Text("Excel".to_string())]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn counts_spaces_as_characters() {
        assert_eq!(
            call_len(vec![Value::Text(" a b ".to_string())]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn empty_text_and_blank_are_zero() {
        assert_eq!(
            call_len(vec![Value::Text(String::new())]),
            Value::Number(0.0)
        );
        assert_eq!(call_len(vec![Value::Blank]), Value::Number(0.0));
    }

    #[test]
    fn coerces_numbers_and_booleans_to_text() {
        assert_eq!(call_len(vec![Value::Number(123.0)]), Value::Number(3.0));
        assert_eq!(call_len(vec![Value::Boolean(true)]), Value::Number(4.0));
        assert_eq!(call_len(vec![Value::Boolean(false)]), Value::Number(5.0));
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_len(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_len(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn counts_unicode_scalar_values() {
        assert_eq!(
            call_len(vec![Value::Text("a\u{00e9}\u{65e5}\u{1f600}".to_string())]),
            Value::Number(4.0)
        );
    }

    fn call_len(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        LEN.call(&args, &fn_ctx)
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
