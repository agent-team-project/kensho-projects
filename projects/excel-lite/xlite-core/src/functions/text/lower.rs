use crate::functions::prelude::*;

pub struct Lower;

impl Function for Lower {
    fn name(&self) -> &'static str {
        "LOWER"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match ctx.to_text(&args[0].as_value()) {
            Ok(text) => Value::Text(text.to_lowercase()),
            Err(error) => Value::Error(error),
        }
    }
}

static LOWER: Lower = Lower;
inventory::submit! { FunctionEntry(&LOWER) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_argument() {
        assert_eq!(LOWER.name(), "LOWER");
        assert_eq!(LOWER.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_lower(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_lower(vec![
                Value::Text("ABC".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn lowercases_uppercase_text() {
        assert_eq!(
            call_lower(vec![Value::Text("EXCEL".to_string())]),
            Value::Text("excel".to_string())
        );
    }

    #[test]
    fn lowercases_mixed_case_text() {
        assert_eq!(
            call_lower(vec![Value::Text("MiXeD Case".to_string())]),
            Value::Text("mixed case".to_string())
        );
    }

    #[test]
    fn leaves_nonletters_unchanged() {
        assert_eq!(
            call_lower(vec![Value::Text("123 - ! ?".to_string())]),
            Value::Text("123 - ! ?".to_string())
        );
    }

    #[test]
    fn coerces_scalars_to_text_before_lowercasing() {
        assert_eq!(
            call_lower(vec![Value::Number(123.0)]),
            Value::Text("123".to_string())
        );
        assert_eq!(
            call_lower(vec![Value::Boolean(true)]),
            Value::Text("true".to_string())
        );
        assert_eq!(
            call_lower(vec![Value::Boolean(false)]),
            Value::Text("false".to_string())
        );
        assert_eq!(call_lower(vec![Value::Blank]), Value::Text(String::new()));
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_lower(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_lower(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn lowercases_unicode_text() {
        assert_eq!(
            call_lower(vec![Value::Text("ÄÖÜ İ ẞ Σ".to_string())]),
            Value::Text("äöü i\u{307} ß σ".to_string())
        );
    }

    fn call_lower(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        LOWER.call(&args, &fn_ctx)
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
