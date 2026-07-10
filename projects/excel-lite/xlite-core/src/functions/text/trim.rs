use crate::functions::prelude::*;

pub struct Trim;

impl Function for Trim {
    fn name(&self) -> &'static str {
        "TRIM"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match ctx.to_text(&args[0].as_value()) {
            Ok(text) => Value::Text(trim_ascii_spaces(&text)),
            Err(error) => Value::Error(error),
        }
    }
}

fn trim_ascii_spaces(text: &str) -> String {
    let mut result = String::new();
    let mut saw_non_space = false;
    let mut pending_space = false;

    for ch in text.chars() {
        if ch == ' ' {
            if saw_non_space {
                pending_space = true;
            }
        } else {
            if pending_space {
                result.push(' ');
                pending_space = false;
            }
            result.push(ch);
            saw_non_space = true;
        }
    }

    result
}

static TRIM: Trim = Trim;
inventory::submit! { FunctionEntry(&TRIM) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_argument() {
        assert_eq!(TRIM.name(), "TRIM");
        assert_eq!(TRIM.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_trim(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_trim(vec![
                Value::Text("abc".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn removes_leading_and_trailing_ascii_spaces() {
        assert_eq!(
            call_trim(vec![Value::Text("   Excel Lite   ".to_string())]),
            Value::Text("Excel Lite".to_string())
        );
    }

    #[test]
    fn collapses_interior_ascii_space_runs() {
        assert_eq!(
            call_trim(vec![Value::Text("a  b    c".to_string())]),
            Value::Text("a b c".to_string())
        );
    }

    #[test]
    fn leaves_text_without_extra_ascii_spaces_unchanged() {
        assert_eq!(
            call_trim(vec![Value::Text("a b,c".to_string())]),
            Value::Text("a b,c".to_string())
        );
    }

    #[test]
    fn all_ascii_spaces_return_empty_text() {
        assert_eq!(
            call_trim(vec![Value::Text("     ".to_string())]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn coerces_scalars_to_text_before_trimming() {
        assert_eq!(
            call_trim(vec![Value::Number(123.0)]),
            Value::Text("123".to_string())
        );
        assert_eq!(
            call_trim(vec![Value::Boolean(true)]),
            Value::Text("TRUE".to_string())
        );
        assert_eq!(
            call_trim(vec![Value::Boolean(false)]),
            Value::Text("FALSE".to_string())
        );
        assert_eq!(call_trim(vec![Value::Blank]), Value::Text(String::new()));
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_trim(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_trim(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn preserves_nonbreaking_spaces() {
        assert_eq!(
            call_trim(vec![Value::Text(" \u{00a0}  A  \u{00a0} ".to_string())]),
            Value::Text("\u{00a0} A \u{00a0}".to_string())
        );
    }

    #[test]
    fn preserves_tabs() {
        assert_eq!(
            call_trim(vec![Value::Text(" \t  A  \t ".to_string())]),
            Value::Text("\t A \t".to_string())
        );
    }

    fn call_trim(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        TRIM.call(&args, &fn_ctx)
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
