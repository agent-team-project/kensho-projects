use crate::functions::prelude::*;

pub struct Upper;

impl Function for Upper {
    fn name(&self) -> &'static str {
        "UPPER"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match ctx.to_text(&args[0].as_value()) {
            Ok(text) => Value::Text(text.chars().flat_map(char::to_uppercase).collect()),
            Err(error) => Value::Error(error),
        }
    }
}

static UPPER: Upper = Upper;
inventory::submit! { FunctionEntry(&UPPER) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_argument() {
        assert_eq!(UPPER.name(), "UPPER");
        assert_eq!(UPPER.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_upper(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_upper(vec![
                Value::Text("abc".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn converts_lowercase_and_mixed_case_text() {
        assert_eq!(
            call_upper(vec![Value::Text("excel lite".to_string())]),
            Value::Text("EXCEL LITE".to_string())
        );
        assert_eq!(
            call_upper(vec![Value::Text("MiXeD Case".to_string())]),
            Value::Text("MIXED CASE".to_string())
        );
    }

    #[test]
    fn leaves_numbers_punctuation_and_spaces_unchanged() {
        assert_eq!(
            call_upper(vec![Value::Text("123 - += !?".to_string())]),
            Value::Text("123 - += !?".to_string())
        );
    }

    #[test]
    fn coerces_scalar_values_to_text_before_uppercasing() {
        assert_eq!(
            call_upper(vec![Value::Number(123.0)]),
            Value::Text("123".to_string())
        );
        assert_eq!(
            call_upper(vec![Value::Boolean(true)]),
            Value::Text("TRUE".to_string())
        );
        assert_eq!(
            call_upper(vec![Value::Boolean(false)]),
            Value::Text("FALSE".to_string())
        );
        assert_eq!(call_upper(vec![Value::Blank]), Value::Text(String::new()));
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_upper(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_upper(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn applies_rust_unicode_uppercase_mapping() {
        assert_eq!(
            call_upper(vec![Value::Text(
                "Stra\u{00df}e na\u{00ef}ve \u{03c3}\u{03c2}".to_string()
            )]),
            Value::Text("STRASSE NA\u{00cf}VE \u{03a3}\u{03a3}".to_string())
        );
    }

    fn call_upper(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        UPPER.call(&args, &fn_ctx)
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
