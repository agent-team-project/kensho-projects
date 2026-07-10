use crate::functions::prelude::*;

pub struct Char;

impl Function for Char {
    fn name(&self) -> &'static str {
        "CHAR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let code = number.trunc();

        if !code.is_finite() || !(1.0..=255.0).contains(&code) {
            return Value::Error(ErrorValue::Value);
        }

        match char::from_u32(code as u32) {
            Some(ch) => Value::Text(ch.to_string()),
            None => Value::Error(ErrorValue::Value),
        }
    }
}

static CHAR: Char = Char;
inventory::submit! { FunctionEntry(&CHAR) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(CHAR.name(), "CHAR");
        assert_eq!(CHAR.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_char(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_char(vec![Value::Number(65.0), Value::Number(66.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_unicode_scalar_for_valid_code() {
        assert_eq!(call_char(vec![Value::Number(65.0)]), text("A"));
        assert_eq!(call_char(vec![Value::Number(33.0)]), text("!"));
    }

    #[test]
    fn supports_control_and_extended_scalar_codes() {
        assert_eq!(call_char(vec![Value::Number(10.0)]), text("\n"));
        assert_eq!(call_char(vec![Value::Number(255.0)]), text("\u{00ff}"));
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(call_char(vec![Value::Text("65".to_string())]), text("A"));
        assert_eq!(call_char(vec![Value::Boolean(true)]), text("\u{0001}"));
    }

    #[test]
    fn truncates_fractional_codes_toward_zero() {
        assert_eq!(call_char(vec![Value::Number(65.9)]), text("A"));
        assert_eq!(call_char(vec![Value::Number(-1.9)]), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn rejects_out_of_range_and_nonfinite_codes() {
        assert_eq!(call_char(vec![Value::Number(0.0)]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_char(vec![Value::Number(256.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_char(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_char(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_char(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_char(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_char(vec![Value::Text("not numeric".to_string())]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_char(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        CHAR.call(&args, &fn_ctx)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
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
