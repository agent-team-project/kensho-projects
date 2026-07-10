use crate::functions::prelude::*;

pub struct Right;

impl Function for Right {
    fn name(&self) -> &'static str {
        "RIGHT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 2 {
            return Value::Error(ErrorValue::Value);
        }

        let text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let num_chars = match args.get(1) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(num_chars) => num_chars,
                Err(error) => return Value::Error(error),
            },
            None => 1.0,
        };

        if !num_chars.is_finite() || num_chars < 0.0 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Text(right(&text, num_chars.trunc() as usize))
    }
}

static RIGHT: Right = Right;
inventory::submit! { FunctionEntry(&RIGHT) }

fn right(text: &str, count: usize) -> String {
    let text_chars = text.chars().count();
    if count >= text_chars {
        return text.to_string();
    }

    text.chars().skip(text_chars - count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_or_two_arguments() {
        assert_eq!(RIGHT.name(), "RIGHT");
        assert_eq!(RIGHT.arity(), (1, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(RIGHT.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            RIGHT.call(
                &[
                    Arg::Value(Value::Text("abc".to_string())),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn defaults_to_last_character() {
        assert_eq!(
            call_right(Value::Text("Excel".to_string()), None),
            Value::Text("l".to_string())
        );
    }

    #[test]
    fn returns_requested_rightmost_characters() {
        assert_eq!(
            call_right(
                Value::Text("spreadsheet".to_string()),
                Some(Value::Number(5.0))
            ),
            Value::Text("sheet".to_string())
        );
    }

    #[test]
    fn counts_unicode_scalar_values() {
        assert_eq!(
            call_right(Value::Text("aé日🙂".to_string()), Some(Value::Number(2.0))),
            Value::Text("日🙂".to_string())
        );
    }

    #[test]
    fn returns_all_text_when_count_exceeds_length() {
        assert_eq!(
            call_right(Value::Text("abc".to_string()), Some(Value::Number(10.0))),
            Value::Text("abc".to_string())
        );
    }

    #[test]
    fn zero_count_returns_empty_text() {
        assert_eq!(
            call_right(Value::Text("abc".to_string()), Some(Value::Number(0.0))),
            Value::Text(String::new())
        );
    }

    #[test]
    fn truncates_fractional_counts_toward_zero() {
        assert_eq!(
            call_right(Value::Text("abcdef".to_string()), Some(Value::Number(2.9))),
            Value::Text("ef".to_string())
        );
    }

    #[test]
    fn rejects_negative_and_non_finite_counts() {
        assert_eq!(
            call_right(Value::Text("abc".to_string()), Some(Value::Number(-1.0))),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_right(
                Value::Text("abc".to_string()),
                Some(Value::Number(f64::INFINITY))
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn uses_scalar_text_and_number_coercion() {
        assert_eq!(
            call_right(Value::Number(12345.0), Some(Value::Text("2".to_string()))),
            Value::Text("45".to_string())
        );
        assert_eq!(
            call_right(Value::Boolean(true), Some(Value::Number(3.0))),
            Value::Text("RUE".to_string())
        );
        assert_eq!(
            call_right(Value::Blank, Some(Value::Number(1.0))),
            Value::Text(String::new())
        );
    }

    #[test]
    fn propagates_text_and_count_coercion_errors() {
        assert_eq!(
            call_right(Value::Error(ErrorValue::Na), Some(Value::Number(1.0))),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(
            call_right(
                Value::Text("abc".to_string()),
                Some(Value::Text("not numeric".to_string()))
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_right(Value::Text("abc".to_string()), Some(Value::Error(ErrorValue::Ref))),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_right(text: Value, num_chars: Option<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let mut args = vec![Arg::Value(text)];
        if let Some(num_chars) = num_chars {
            args.push(Arg::Value(num_chars));
        }

        RIGHT.call(&args, &fn_ctx)
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
