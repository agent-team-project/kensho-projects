use crate::functions::prelude::*;

const MAX_RESULT_CHARS: usize = 32_767;

pub struct Rept;

impl Function for Rept {
    fn name(&self) -> &'static str {
        "REPT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let number_times = match ctx.to_number(&args[1].as_value()) {
            Ok(number_times) => number_times,
            Err(error) => return Value::Error(error),
        };

        match repeat_text(&text, number_times) {
            Ok(result) => Value::Text(result),
            Err(error) => Value::Error(error),
        }
    }
}

static REPT: Rept = Rept;
inventory::submit! { FunctionEntry(&REPT) }

fn repeat_text(text: &str, number_times: f64) -> Result<String, ErrorValue> {
    let count_value = number_times.trunc();
    if !count_value.is_finite() || count_value < 0.0 {
        return Err(ErrorValue::Value);
    }

    let text_chars = text.chars().count();
    if count_value == 0.0 || text_chars == 0 {
        return Ok(String::new());
    }
    if count_value > MAX_RESULT_CHARS as f64 {
        return Err(ErrorValue::Value);
    }

    let count = count_value as usize;
    if text_chars > MAX_RESULT_CHARS / count {
        return Err(ErrorValue::Value);
    }

    Ok(text.repeat(count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_two_argument_arity() {
        assert_eq!(REPT.name(), "REPT");
        assert_eq!(REPT.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_rept(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_rept(vec![Value::Text("x".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("x".to_string()),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn repeats_text_requested_times() {
        assert_eq!(
            call_rept(vec![Value::Text("ha".to_string()), Value::Number(3.0)]),
            Value::Text("hahaha".to_string())
        );
    }

    #[test]
    fn zero_count_returns_empty_text() {
        assert_eq!(
            call_rept(vec![Value::Text("x".to_string()), Value::Number(0.0)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn truncates_fractional_count_toward_zero() {
        assert_eq!(
            call_rept(vec![Value::Text("ab".to_string()), Value::Number(2.9)]),
            Value::Text("abab".to_string())
        );
        assert_eq!(
            call_rept(vec![Value::Text("ab".to_string()), Value::Number(-0.9)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn empty_text_repeated_remains_empty() {
        assert_eq!(
            call_rept(vec![Value::Text(String::new()), Value::Number(10.0)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn uses_scalar_text_and_count_coercion() {
        assert_eq!(
            call_rept(vec![Value::Number(12.0), Value::Text("2.9".to_string())]),
            Value::Text("1212".to_string())
        );
        assert_eq!(
            call_rept(vec![Value::Boolean(true), Value::Number(2.0)]),
            Value::Text("TRUETRUE".to_string())
        );
        assert_eq!(
            call_rept(vec![Value::Blank, Value::Number(3.0)]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn rejects_negative_and_nonfinite_counts() {
        assert_eq!(
            call_rept(vec![Value::Text("x".to_string()), Value::Number(-1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("x".to_string()),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_rept(vec![Value::Text("x".to_string()), Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn enforces_result_length_limit_by_rust_char_count() {
        assert_eq!(
            call_rept(vec![
                Value::Text("x".repeat(MAX_RESULT_CHARS)),
                Value::Number(1.0),
            ]),
            Value::Text("x".repeat(MAX_RESULT_CHARS))
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("\u{00e9}".to_string()),
                Value::Number(MAX_RESULT_CHARS as f64),
            ]),
            Value::Text("\u{00e9}".repeat(MAX_RESULT_CHARS))
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("xy".to_string()),
                Value::Number((MAX_RESULT_CHARS / 2 + 1) as f64),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("x".to_string()),
                Value::Number((MAX_RESULT_CHARS + 1) as f64),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_rept(vec![
                Value::Error(ErrorValue::Ref),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("x".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_rept(vec![
                Value::Text("x".to_string()),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_rept(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        REPT.call(&args, &fn_ctx)
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
