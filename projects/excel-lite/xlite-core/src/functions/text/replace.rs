use crate::functions::prelude::*;

pub struct Replace;

impl Function for Replace {
    fn name(&self) -> &'static str {
        "REPLACE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (4, Some(4))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 4 {
            return Value::Error(ErrorValue::Value);
        }

        let old_text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let start_num = match ctx.to_number(&args[1].as_value()) {
            Ok(start_num) => start_num,
            Err(error) => return Value::Error(error),
        };
        let num_chars = match ctx.to_number(&args[2].as_value()) {
            Ok(num_chars) => num_chars,
            Err(error) => return Value::Error(error),
        };
        let new_text = match ctx.to_text(&args[3].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };

        if !start_num.is_finite() || !num_chars.is_finite() || start_num < 1.0 || num_chars < 0.0 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Text(replace_chars(
            &old_text,
            start_num.trunc() as usize,
            num_chars.trunc() as usize,
            &new_text,
        ))
    }
}

static REPLACE: Replace = Replace;
inventory::submit! { FunctionEntry(&REPLACE) }

fn replace_chars(old_text: &str, start_num: usize, num_chars: usize, new_text: &str) -> String {
    let start_index = start_num - 1;
    let suffix_start = start_index.saturating_add(num_chars);

    let mut result = String::new();
    result.extend(old_text.chars().take(start_index));
    result.push_str(new_text);
    result.extend(old_text.chars().skip(suffix_start));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_four_argument_arity() {
        assert_eq!(REPLACE.name(), "REPLACE");
        assert_eq!(REPLACE.arity(), (4, Some(4)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_replace(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Text("x".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn replaces_requested_middle_span() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdefgh".to_string()),
                Value::Number(3.0),
                Value::Number(4.0),
                Value::Text("X".to_string()),
            ]),
            Value::Text("abXgh".to_string())
        );
    }

    #[test]
    fn replaces_leading_span() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdefgh".to_string()),
                Value::Number(1.0),
                Value::Number(3.0),
                Value::Text("X".to_string()),
            ]),
            Value::Text("Xdefgh".to_string())
        );
    }

    #[test]
    fn overlong_span_replaces_through_end() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(4.0),
                Value::Number(99.0),
                Value::Text("X".to_string()),
            ]),
            Value::Text("abcX".to_string())
        );
    }

    #[test]
    fn zero_count_inserts_before_start_character() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(3.0),
                Value::Number(0.0),
                Value::Text("X".to_string()),
            ]),
            Value::Text("abXcdef".to_string())
        );
    }

    #[test]
    fn start_beyond_text_length_appends_new_text() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(5.0),
                Value::Number(2.0),
                Value::Text("X".to_string()),
            ]),
            Value::Text("abcX".to_string())
        );
    }

    #[test]
    fn truncates_fractional_start_and_count_toward_zero() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.9),
                Value::Number(3.9),
                Value::Text("X".to_string()),
            ]),
            Value::Text("aXef".to_string())
        );
    }

    #[test]
    fn counts_positions_by_rust_char_values() {
        assert_eq!(
            call_replace(vec![
                Value::Text("a\u{00e9}\u{65e5}\u{1f600}z".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Text("X".to_string()),
            ]),
            Value::Text("aXz".to_string())
        );
    }

    #[test]
    fn uses_scalar_text_start_count_and_replacement_coercion() {
        assert_eq!(
            call_replace(vec![
                Value::Number(12345.0),
                Value::Text("2.9".to_string()),
                Value::Text("2.9".to_string()),
                Value::Boolean(true),
            ]),
            Value::Text("1TRUE45".to_string())
        );
        assert_eq!(
            call_replace(vec![
                Value::Blank,
                Value::Number(1.0),
                Value::Number(0.0),
                Value::Number(7.0),
            ]),
            Value::Text("7".to_string())
        );
    }

    #[test]
    fn rejects_invalid_and_nonfinite_start_or_count() {
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(0.9),
                Value::Number(1.0),
                Value::Text("X".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Number(-0.1),
                Value::Text("X".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(f64::INFINITY),
                Value::Number(1.0),
                Value::Text("X".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Number(f64::NAN),
                Value::Text("X".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_replace(vec![
                Value::Error(ErrorValue::Ref),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
                Value::Error(ErrorValue::Name),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
                Value::Error(ErrorValue::Name),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Error(ErrorValue::Na),
                Value::Error(ErrorValue::Name),
            ]),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Error(ErrorValue::Name),
            ]),
            Value::Error(ErrorValue::Name)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Text("not numeric".to_string()),
                Value::Number(1.0),
                Value::Text("X".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_replace(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Text("not numeric".to_string()),
                Value::Text("X".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_replace(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        REPLACE.call(&args, &fn_ctx)
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
