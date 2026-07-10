use crate::functions::prelude::*;

pub struct Mid;

impl Function for Mid {
    fn name(&self) -> &'static str {
        "MID"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 3 {
            return Value::Error(ErrorValue::Value);
        }

        let text = match ctx.to_text(&args[0].as_value()) {
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

        if !start_num.is_finite()
            || !num_chars.is_finite()
            || start_num < 1.0
            || num_chars < 0.0
        {
            return Value::Error(ErrorValue::Value);
        }

        Value::Text(mid_chars(
            &text,
            start_num.trunc() as usize,
            num_chars.trunc() as usize,
        ))
    }
}

static MID: Mid = Mid;
inventory::submit! { FunctionEntry(&MID) }

fn mid_chars(text: &str, start_num: usize, num_chars: usize) -> String {
    text.chars().skip(start_num - 1).take(num_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_three_argument_arity() {
        assert_eq!(MID.name(), "MID");
        assert_eq!(MID.arity(), (3, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_mid(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_mid(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_requested_middle_text() {
        assert_eq!(
            call_mid(vec![
                Value::Text("spreadsheet".to_string()),
                Value::Number(4.0),
                Value::Number(5.0),
            ]),
            Value::Text("eadsh".to_string())
        );
    }

    #[test]
    fn start_one_extracts_from_first_character() {
        assert_eq!(
            call_mid(vec![
                Value::Text("Excel".to_string()),
                Value::Number(1.0),
                Value::Number(3.0),
            ]),
            Value::Text("Exc".to_string())
        );
    }

    #[test]
    fn start_beyond_text_length_returns_empty_text() {
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(5.0),
                Value::Number(2.0),
            ]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn zero_count_returns_empty_text() {
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(2.0),
                Value::Number(0.0),
            ]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn overlong_count_returns_through_end() {
        assert_eq!(
            call_mid(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(4.0),
                Value::Number(99.0),
            ]),
            Value::Text("def".to_string())
        );
    }

    #[test]
    fn truncates_fractional_start_and_count_toward_zero() {
        assert_eq!(
            call_mid(vec![
                Value::Text("abcdef".to_string()),
                Value::Number(2.9),
                Value::Number(3.9),
            ]),
            Value::Text("bcd".to_string())
        );
    }

    #[test]
    fn slices_by_rust_char_count() {
        assert_eq!(
            call_mid(vec![
                Value::Text("a\u{00e9}\u{65e5}\u{1f600}z".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
            Value::Text("\u{00e9}\u{65e5}\u{1f600}".to_string())
        );
    }

    #[test]
    fn uses_scalar_text_start_and_count_coercion() {
        assert_eq!(
            call_mid(vec![
                Value::Number(12345.0),
                Value::Text("2.9".to_string()),
                Value::Text("3.9".to_string()),
            ]),
            Value::Text("234".to_string())
        );
        assert_eq!(
            call_mid(vec![
                Value::Boolean(true),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
            Value::Text("RUE".to_string())
        );
        assert_eq!(
            call_mid(vec![Value::Blank, Value::Number(1.0), Value::Number(1.0)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn rejects_invalid_and_nonfinite_start_or_count() {
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(0.9),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Number(-0.1),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(f64::INFINITY),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_mid(vec![
                Value::Error(ErrorValue::Ref),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Text("not numeric".to_string()),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_mid(vec![
                Value::Text("abc".to_string()),
                Value::Number(1.0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    fn call_mid(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        MID.call(&args, &fn_ctx)
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
