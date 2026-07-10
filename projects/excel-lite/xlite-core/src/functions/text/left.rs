use crate::functions::prelude::*;

pub struct Left;

impl Function for Left {
    fn name(&self) -> &'static str {
        "LEFT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(1..=2).contains(&args.len()) {
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

        Value::Text(left_chars(text, num_chars))
    }
}

fn left_chars(text: String, num_chars: f64) -> String {
    let count = num_chars.trunc() as usize;
    if count >= text.chars().count() {
        text
    } else {
        text.chars().take(count).collect()
    }
}

static LEFT: Left = Left;
inventory::submit! { FunctionEntry(&LEFT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_or_two_arguments() {
        assert_eq!(LEFT.name(), "LEFT");
        assert_eq!(LEFT.arity(), (1, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_left(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_left(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn defaults_num_chars_to_one() {
        assert_eq!(
            call_left(vec![Value::Text("Excel".to_string())]),
            Value::Text("E".to_string())
        );
    }

    #[test]
    fn returns_requested_leftmost_text() {
        assert_eq!(
            call_left(vec![Value::Text("Excel".to_string()), Value::Number(3.0)]),
            Value::Text("Exc".to_string())
        );
    }

    #[test]
    fn overlong_count_returns_all_text() {
        assert_eq!(
            call_left(vec![Value::Text("Excel".to_string()), Value::Number(99.0)]),
            Value::Text("Excel".to_string())
        );
    }

    #[test]
    fn zero_count_returns_empty_text() {
        assert_eq!(
            call_left(vec![Value::Text("Excel".to_string()), Value::Number(0.0)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn truncates_fractional_counts_toward_zero() {
        assert_eq!(
            call_left(vec![
                Value::Text("Excel".to_string()),
                Value::Number(2.9),
            ]),
            Value::Text("Ex".to_string())
        );
    }

    #[test]
    fn slices_by_rust_char_count() {
        assert_eq!(
            call_left(vec![
                Value::Text("a\u{00e9}\u{1f600}z".to_string()),
                Value::Number(3.0),
            ]),
            Value::Text("a\u{00e9}\u{1f600}".to_string())
        );
    }

    #[test]
    fn uses_scalar_text_and_count_coercion() {
        assert_eq!(
            call_left(vec![Value::Number(123.0), Value::Text("2.9".to_string())]),
            Value::Text("12".to_string())
        );
        assert_eq!(
            call_left(vec![Value::Boolean(true), Value::Number(4.0)]),
            Value::Text("TRUE".to_string())
        );
        assert_eq!(
            call_left(vec![Value::Blank, Value::Number(1.0)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn rejects_negative_and_nonfinite_counts() {
        assert_eq!(
            call_left(vec![Value::Text("Excel".to_string()), Value::Number(-1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_left(vec![
                Value::Text("Excel".to_string()),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_left(vec![
                Value::Text("Excel".to_string()),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_left(vec![Value::Error(ErrorValue::Ref), Value::Number(1.0)]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_left(vec![
                Value::Text("Excel".to_string()),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_left(vec![
                Value::Text("Excel".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_left(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        LEFT.call(&args, &fn_ctx)
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
