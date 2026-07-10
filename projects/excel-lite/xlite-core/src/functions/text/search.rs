use crate::functions::prelude::*;

pub struct Search;

impl Function for Search {
    fn name(&self) -> &'static str {
        "SEARCH"
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

        let char_count = within_text.chars().count();
        let start_num = match args.get(2) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(number) => {
                    let truncated = number.trunc();
                    if !number.is_finite() || truncated < 1.0 || truncated > char_count as f64 {
                        return Value::Error(ErrorValue::Value);
                    }
                    truncated as usize
                }
                Err(error) => return Value::Error(error),
            },
            None if char_count == 0 => return Value::Error(ErrorValue::Value),
            None => 1,
        };

        match search_position(&find_text, &within_text, start_num) {
            Some(position) => Value::Number(position as f64),
            None => Value::Error(ErrorValue::Value),
        }
    }
}

static SEARCH: Search = Search;
inventory::submit! { FunctionEntry(&SEARCH) }

#[derive(Debug, PartialEq, Eq)]
enum SearchToken {
    Literal(String),
    AnyChar,
    AnyChars,
}

fn search_position(find_text: &str, within_text: &str, start_num: usize) -> Option<usize> {
    let tokens = parse_search_pattern(find_text);
    let text: Vec<_> = within_text.chars().map(lower_char).collect();
    let matches = pattern_matches_from(&tokens, &text);
    let start_index = start_num - 1;

    (start_index..text.len())
        .find(|&index| matches[index])
        .map(|index| index + 1)
}

fn parse_search_pattern(pattern: &str) -> Vec<SearchToken> {
    let mut tokens = Vec::new();
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '?' => tokens.push(SearchToken::AnyChar),
            '*' => tokens.push(SearchToken::AnyChars),
            '~' => match chars.peek().copied() {
                Some('?') | Some('*') | Some('~') => {
                    let literal = chars.next().expect("peeked character exists");
                    tokens.push(SearchToken::Literal(lower_char(literal)));
                }
                _ => tokens.push(SearchToken::Literal(lower_char('~'))),
            },
            _ => tokens.push(SearchToken::Literal(lower_char(ch))),
        }
    }

    tokens
}

fn lower_char(ch: char) -> String {
    ch.to_lowercase().collect()
}

fn pattern_matches_from(tokens: &[SearchToken], text: &[String]) -> Vec<bool> {
    let len = text.len();
    let mut next = vec![true; len + 1];

    for token in tokens.iter().rev() {
        let mut current = vec![false; len + 1];

        match token {
            SearchToken::Literal(literal) => {
                for index in (0..len).rev() {
                    current[index] = text[index].as_str() == literal && next[index + 1];
                }
            }
            SearchToken::AnyChar => {
                for index in (0..len).rev() {
                    current[index] = next[index + 1];
                }
            }
            SearchToken::AnyChars => {
                current[len] = next[len];
                for index in (0..len).rev() {
                    current[index] = next[index] || current[index + 1];
                }
            }
        }

        next = current;
    }

    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_two_or_three_arguments() {
        assert_eq!(SEARCH.name(), "SEARCH");
        assert_eq!(SEARCH.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_search(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_search(vec![Value::Text("needle".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("needle".to_string()),
                Value::Text("haystack".to_string()),
                Value::Number(1.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn finds_case_insensitive_substrings() {
        assert_eq!(
            call_search(vec![
                Value::Text("margin".to_string()),
                Value::Text("Gross MARGIN".to_string()),
            ]),
            Value::Number(7.0)
        );
    }

    #[test]
    fn start_num_skips_earlier_matches_but_returns_full_position() {
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Text("banana".to_string()),
                Value::Number(3.0),
            ]),
            Value::Number(4.0)
        );
    }

    #[test]
    fn returns_value_when_find_text_is_missing() {
        assert_eq!(
            call_search(vec![
                Value::Text("z".to_string()),
                Value::Text("abc".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn question_mark_matches_one_character() {
        assert_eq!(
            call_search(vec![
                Value::Text("c?t".to_string()),
                Value::Text("concatenate".to_string()),
            ]),
            Value::Number(4.0)
        );
    }

    #[test]
    fn asterisk_matches_zero_or_more_characters() {
        assert_eq!(
            call_search(vec![
                Value::Text("a*d".to_string()),
                Value::Text("xxabbbd".to_string()),
            ]),
            Value::Number(3.0)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("a*d".to_string()),
                Value::Text("xxad".to_string()),
            ]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn tilde_escapes_literal_wildcards_and_tilde() {
        assert_eq!(
            call_search(vec![
                Value::Text("~?".to_string()),
                Value::Text("a?b".to_string()),
            ]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("~*".to_string()),
                Value::Text("a*b".to_string()),
            ]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("~~".to_string()),
                Value::Text("a~b".to_string()),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn counts_unicode_text_by_rust_chars_not_bytes() {
        assert_eq!(
            call_search(vec![
                Value::Text("\u{65e5}\u{1f600}".to_string()),
                Value::Text("a\u{00e9}\u{65e5}\u{1f600}z".to_string()),
            ]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn uses_scalar_text_and_numeric_start_coercion() {
        assert_eq!(
            call_search(vec![
                Value::Number(23.0),
                Value::Number(12345.0),
                Value::Text("2.9".to_string()),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn rejects_invalid_and_nonfinite_start_num() {
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(0.9),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(4.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_search(vec![
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
            call_search(vec![
                Value::Error(ErrorValue::Ref),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Error(ErrorValue::Name),
            ]),
            Value::Error(ErrorValue::Name)
        );
        assert_eq!(
            call_search(vec![
                Value::Text("a".to_string()),
                Value::Text("abc".to_string()),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_search(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        SEARCH.call(&args, &fn_ctx)
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
