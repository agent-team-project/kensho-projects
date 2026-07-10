use crate::functions::prelude::*;

pub struct Code;

impl Function for Code {
    fn name(&self) -> &'static str {
        "CODE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        let text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };

        match text.chars().next() {
            Some(ch) => Value::Number(ch as u32 as f64),
            None => Value::Error(ErrorValue::Value),
        }
    }
}

static CODE: Code = Code;
inventory::submit! { FunctionEntry(&CODE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_argument() {
        assert_eq!(CODE.name(), "CODE");
        assert_eq!(CODE.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_code(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_code(vec![
                Value::Text("A".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_ascii_code_for_first_character() {
        assert_eq!(call_code(vec![Value::Text("A".to_string())]), Value::Number(65.0));
        assert_eq!(call_code(vec![Value::Text("!".to_string())]), Value::Number(33.0));
    }

    #[test]
    fn uses_only_the_first_character() {
        assert_eq!(
            call_code(vec![Value::Text("ABC".to_string())]),
            Value::Number(65.0)
        );
    }

    #[test]
    fn coerces_scalars_to_text_before_reading_first_character() {
        assert_eq!(call_code(vec![Value::Number(123.0)]), Value::Number(49.0));
        assert_eq!(call_code(vec![Value::Boolean(true)]), Value::Number(84.0));
        assert_eq!(call_code(vec![Value::Boolean(false)]), Value::Number(70.0));
    }

    #[test]
    fn rejects_empty_text_and_blank() {
        assert_eq!(
            call_code(vec![Value::Text(String::new())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(call_code(vec![Value::Blank]), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_code(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_code(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn returns_unicode_scalar_value_for_non_ascii_first_character() {
        assert_eq!(
            call_code(vec![Value::Text("\u{00e9}clair".to_string())]),
            Value::Number(233.0)
        );
    }

    fn call_code(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        CODE.call(&args, &fn_ctx)
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
