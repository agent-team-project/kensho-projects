use crate::functions::prelude::*;

pub struct Find;

impl Function for Find {
    fn name(&self) -> &'static str {
        "FIND"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let find_text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let within_text = match ctx.to_text(&args[1].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let start_num = match args.get(2) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(start_num) => start_num,
                Err(error) => return Value::Error(error),
            },
            None => 1.0,
        };

        if !start_num.is_finite() {
            return Value::Error(ErrorValue::Value);
        }

        let start_num = start_num.trunc();
        let text_len = within_text.chars().count();
        if start_num < 1.0 || start_num > text_len as f64 {
            return Value::Error(ErrorValue::Value);
        }

        match find_from_char_position(&find_text, &within_text, start_num as usize) {
            Some(position) => Value::Number(position as f64),
            None => Value::Error(ErrorValue::Value),
        }
    }
}

static FIND: Find = Find;
inventory::submit! { FunctionEntry(&FIND) }

fn find_from_char_position(find_text: &str, within_text: &str, start_num: usize) -> Option<usize> {
    let start_byte = byte_index_for_char_position(within_text, start_num);
    let match_byte = start_byte + within_text[start_byte..].find(find_text)?;
    Some(within_text[..match_byte].chars().count() + 1)
}

fn byte_index_for_char_position(text: &str, start_num: usize) -> usize {
    text.char_indices()
        .nth(start_num - 1)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_two_or_three_arguments() {
        assert_eq!(FIND.name(), "FIND");
        assert_eq!(FIND.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_find(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_find(vec![Value::Text("a".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn finds_case_sensitive_literal_substrings() {
        assert_eq!(
            call_find(vec![
                Value::Text("sheet".to_string()),
                Value::Text("spreadsheet".to_string()),
            ]),
            Value::Number(7.0)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("cat".to_string()),
                Value::Text("Cat cat CAT".to_string()),
            ]),
            Value::Number(5.0)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("Cat".to_string()),
                Value::Text("cat".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn start_num_skips_earlier_matches_but_returns_full_position() {
        assert_eq!(
            call_find(vec![
                Value::Text("is".to_string()),
                Value::Text("Mississippi".to_string()),
                Value::Number(4.0),
            ]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn reports_value_when_text_is_missing() {
        assert_eq!(
            call_find(vec![
                Value::Text("z".to_string()),
                Value::Text("abcdef".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn truncates_start_num_toward_zero() {
        assert_eq!(
            call_find(vec![
                Value::Text("b".to_string()),
                Value::Text("abcabc".to_string()),
                Value::Number(2.9),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn uses_scalar_text_and_start_num_coercion() {
        assert_eq!(
            call_find(vec![
                Value::Number(3.0),
                Value::Number(12345.0),
                Value::Text("2.9".to_string()),
            ]),
            Value::Number(3.0)
        );
        assert_eq!(
            call_find(vec![
                Value::Boolean(true),
                Value::Text("xxTRUE".to_string()),
                Value::Number(2.0),
            ]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn rejects_invalid_and_nonfinite_start_num() {
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(0.9),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(4.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_find(vec![
                Value::Error(ErrorValue::Ref),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Text("apple".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn counts_unicode_text_by_rust_char_values() {
        assert_eq!(
            call_find(vec![
                Value::Text("\u{65e5}\u{1f600}".to_string()),
                Value::Text("a\u{00e9}\u{65e5}\u{1f600}z".to_string()),
            ]),
            Value::Number(3.0)
        );
        assert_eq!(
            call_find(vec![
                Value::Text("\u{1f600}".to_string()),
                Value::Text("a\u{00e9}\u{65e5}\u{1f600}z".to_string()),
                Value::Number(4.0),
            ]),
            Value::Number(4.0)
        );
    }

    fn call_find(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        FIND.call(&args, &fn_ctx)
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
