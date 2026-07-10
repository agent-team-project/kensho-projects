use crate::functions::prelude::*;

pub struct Proper;

impl Function for Proper {
    fn name(&self) -> &'static str {
        "PROPER"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match ctx.to_text(&args[0].as_value()) {
            Ok(text) => Value::Text(proper_case(&text)),
            Err(error) => Value::Error(error),
        }
    }
}

fn proper_case(text: &str) -> String {
    let mut result = String::new();
    let mut previous_was_alphabetic = false;

    for ch in text.chars() {
        if ch.is_alphabetic() {
            if previous_was_alphabetic {
                result.extend(ch.to_lowercase());
            } else {
                result.extend(ch.to_uppercase());
            }
            previous_was_alphabetic = true;
        } else {
            result.push(ch);
            previous_was_alphabetic = false;
        }
    }

    result
}

static PROPER: Proper = Proper;
inventory::submit! { FunctionEntry(&PROPER) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_argument() {
        assert_eq!(PROPER.name(), "PROPER");
        assert_eq!(PROPER.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_proper(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_proper(vec![
                Value::Text("abc".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn capitalizes_words_and_lowercases_remaining_letters() {
        assert_eq!(
            call_proper(vec![Value::Text("hello WORLD".to_string())]),
            Value::Text("Hello World".to_string())
        );
        assert_eq!(
            call_proper(vec![Value::Text("mIxEd CaSe".to_string())]),
            Value::Text("Mixed Case".to_string())
        );
    }

    #[test]
    fn capitalizes_after_spaces_punctuation_and_digits() {
        assert_eq!(
            call_proper(vec![Value::Text("rock'n'roll".to_string())]),
            Value::Text("Rock'N'Roll".to_string())
        );
        assert_eq!(
            call_proper(vec![Value::Text("2-way street".to_string())]),
            Value::Text("2-Way Street".to_string())
        );
        assert_eq!(
            call_proper(vec![Value::Text("76BudGet".to_string())]),
            Value::Text("76Budget".to_string())
        );
    }

    #[test]
    fn preserves_nonletters_while_casing_letters() {
        assert_eq!(
            call_proper(vec![Value::Text("  123!?  abc\tDEF".to_string())]),
            Value::Text("  123!?  Abc\tDef".to_string())
        );
    }

    #[test]
    fn coerces_scalar_values_to_text_before_casing() {
        assert_eq!(
            call_proper(vec![Value::Number(123.45)]),
            Value::Text("123.45".to_string())
        );
        assert_eq!(
            call_proper(vec![Value::Boolean(true)]),
            Value::Text("True".to_string())
        );
        assert_eq!(
            call_proper(vec![Value::Boolean(false)]),
            Value::Text("False".to_string())
        );
        assert_eq!(
            call_proper(vec![Value::Blank]),
            Value::Text(String::new())
        );
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_proper(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_proper(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn uses_unicode_alphabetic_boundaries_and_case_mapping() {
        assert_eq!(
            call_proper(vec![Value::Text("éLAN.über π-SCORE".to_string())]),
            Value::Text("Élan.Über Π-Score".to_string())
        );
    }

    fn call_proper(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        PROPER.call(&args, &fn_ctx)
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
