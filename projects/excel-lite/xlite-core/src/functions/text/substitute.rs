use crate::functions::prelude::*;

pub struct Substitute;

impl Function for Substitute {
    fn name(&self) -> &'static str {
        "SUBSTITUTE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(4))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(3..=4).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };
        let old_text = match ctx.to_text(&args[1].as_value()) {
            Ok(old_text) => old_text,
            Err(error) => return Value::Error(error),
        };
        let new_text = match ctx.to_text(&args[2].as_value()) {
            Ok(new_text) => new_text,
            Err(error) => return Value::Error(error),
        };
        let instance_num = match args.get(3) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(instance_num) => {
                    let truncated = instance_num.trunc();
                    if !instance_num.is_finite() || truncated < 1.0 {
                        return Value::Error(ErrorValue::Value);
                    }
                    Some(truncated)
                }
                Err(error) => return Value::Error(error),
            },
            None => None,
        };

        if old_text.is_empty() {
            return Value::Text(text);
        }

        match instance_num {
            Some(instance_num) if instance_num > usize::MAX as f64 => Value::Text(text),
            Some(instance_num) => Value::Text(replace_instance(
                &text,
                &old_text,
                &new_text,
                instance_num as usize,
            )),
            None => Value::Text(text.replace(&old_text, &new_text)),
        }
    }
}

static SUBSTITUTE: Substitute = Substitute;
inventory::submit! { FunctionEntry(&SUBSTITUTE) }

fn replace_instance(text: &str, old_text: &str, new_text: &str, instance_num: usize) -> String {
    let Some((start, _)) = text.match_indices(old_text).nth(instance_num - 1) else {
        return text.to_string();
    };
    let end = start + old_text.len();

    let mut result = String::with_capacity(text.len() - old_text.len() + new_text.len());
    result.push_str(&text[..start]);
    result.push_str(new_text);
    result.push_str(&text[end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_three_or_four_arguments() {
        assert_eq!(SUBSTITUTE.name(), "SUBSTITUTE");
        assert_eq!(SUBSTITUTE.arity(), (3, Some(4)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_substitute(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Number(1.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn replaces_all_occurrences_when_instance_is_omitted() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("one fish, two fish".to_string()),
                Value::Text("fish".to_string()),
                Value::Text("cat".to_string()),
            ]),
            Value::Text("one cat, two cat".to_string())
        );
    }

    #[test]
    fn replaces_only_requested_occurrence() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("Quarter 1, 2011".to_string()),
                Value::Text("1".to_string()),
                Value::Text("2".to_string()),
                Value::Number(3.0),
            ]),
            Value::Text("Quarter 1, 2012".to_string())
        );
    }

    #[test]
    fn truncates_fractional_instance_toward_zero() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("ababab".to_string()),
                Value::Text("ab".to_string()),
                Value::Text("X".to_string()),
                Value::Number(2.9),
            ]),
            Value::Text("abXab".to_string())
        );
    }

    #[test]
    fn uses_case_sensitive_literal_matching() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("Cat cat CAT".to_string()),
                Value::Text("cat".to_string()),
                Value::Text("dog".to_string()),
            ]),
            Value::Text("Cat dog CAT".to_string())
        );
    }

    #[test]
    fn returns_original_text_when_old_text_is_missing() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("abcdef".to_string()),
                Value::Text("z".to_string()),
                Value::Text("x".to_string()),
            ]),
            Value::Text("abcdef".to_string())
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abcdef".to_string()),
                Value::Text("z".to_string()),
                Value::Text("x".to_string()),
                Value::Number(1.0),
            ]),
            Value::Text("abcdef".to_string())
        );
    }

    #[test]
    fn empty_new_text_removes_matches() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("1-2-3".to_string()),
                Value::Text("-".to_string()),
                Value::Text(String::new()),
            ]),
            Value::Text("123".to_string())
        );
    }

    #[test]
    fn empty_old_text_returns_original_text() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text(String::new()),
                Value::Text("x".to_string()),
            ]),
            Value::Text("abc".to_string())
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text(String::new()),
                Value::Text("x".to_string()),
                Value::Number(2.0),
            ]),
            Value::Text("abc".to_string())
        );
    }

    #[test]
    fn uses_scalar_text_and_instance_coercion() {
        assert_eq!(
            call_substitute(vec![
                Value::Number(12321.0),
                Value::Number(2.0),
                Value::Boolean(true),
                Value::Text("2.9".to_string()),
            ]),
            Value::Text("123TRUE1".to_string())
        );
        assert_eq!(
            call_substitute(vec![
                Value::Boolean(false),
                Value::Text("F".to_string()),
                Value::Text("T".to_string()),
            ]),
            Value::Text("TALSE".to_string())
        );
        assert_eq!(
            call_substitute(vec![
                Value::Blank,
                Value::Text("x".to_string()),
                Value::Text("y".to_string()),
            ]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn rejects_invalid_instance_numbers() {
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Number(0.9),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Number(-1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_substitute(vec![
                Value::Error(ErrorValue::Ref),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Text("apple".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_substitute(vec![
                Value::Text("abc".to_string()),
                Value::Text("a".to_string()),
                Value::Text("x".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_substitute(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        SUBSTITUTE.call(&args, &fn_ctx)
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
