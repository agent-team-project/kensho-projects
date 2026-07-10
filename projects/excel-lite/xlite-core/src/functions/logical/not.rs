use crate::functions::prelude::*;

pub struct Not;

impl Function for Not {
    fn name(&self) -> &'static str {
        "NOT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match ctx.to_bool(&args[0].as_value()) {
            Ok(value) => Value::Boolean(!value),
            Err(error) => Value::Error(error),
        }
    }
}

static NOT: Not = Not;
inventory::submit! { FunctionEntry(&NOT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(NOT.name(), "NOT");
        assert_eq!(NOT.arity(), (1, Some(1)));
    }

    #[test]
    fn reverses_boolean_values() {
        assert_eq!(call_not(Value::Boolean(true)), Value::Boolean(false));
        assert_eq!(call_not(Value::Boolean(false)), Value::Boolean(true));
    }

    #[test]
    fn coerces_numbers_to_logical_values() {
        assert_eq!(call_not(Value::Number(0.0)), Value::Boolean(true));
        assert_eq!(call_not(Value::Number(1.0)), Value::Boolean(false));
        assert_eq!(call_not(Value::Number(-2.0)), Value::Boolean(false));
    }

    #[test]
    fn coerces_logical_text_values() {
        assert_eq!(
            call_not(Value::Text("TRUE".to_string())),
            Value::Boolean(false)
        );
        assert_eq!(
            call_not(Value::Text("FALSE".to_string())),
            Value::Boolean(true)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_not(Value::Text("not logical".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_not(Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let fn_ctx = fn_context();
        let one_arg = Arg::Value(Value::Boolean(false));
        let two_args = [
            Arg::Value(Value::Boolean(true)),
            Arg::Value(Value::Boolean(false)),
        ];

        assert_eq!(NOT.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(NOT.call(&two_args, &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(NOT.call(&[one_arg], &fn_ctx), Value::Boolean(true));
    }

    #[test]
    fn direct_range_call_uses_top_left_scalar_value() {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let range = RangeView::new(
            &ctx,
            RangeRef {
                start: cell_ref(0, 0),
                end: cell_ref(0, 1),
            },
        );

        assert_eq!(NOT.call(&[Arg::Range(range)], &fn_ctx), Value::Boolean(true));
    }

    fn call_not(value: Value) -> Value {
        NOT.call(&[Arg::Value(value)], &fn_context())
    }

    fn fn_context() -> FnContext<'static> {
        static CTX: DummyContext = DummyContext;
        FnContext::new(&CTX)
    }

    fn cell_ref(row: u32, col: u32) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    struct DummyContext;

    impl EvalContext for DummyContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Boolean(false),
                (0, 1) => Value::Boolean(true),
                _ => Value::Blank,
            }
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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
