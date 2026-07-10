use crate::functions::prelude::*;

pub struct Iserr;

impl Function for Iserr {
    fn name(&self) -> &'static str {
        "ISERR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Boolean(matches!(
            args[0].as_value(),
            Value::Error(error) if error != ErrorValue::Na
        ))
    }
}

static ISERR: Iserr = Iserr;
inventory::submit! { FunctionEntry(&ISERR) }

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
        assert_eq!(ISERR.name(), "ISERR");
        assert_eq!(ISERR.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_true_for_non_na_errors() {
        for error in [
            ErrorValue::Null,
            ErrorValue::Div0,
            ErrorValue::Value,
            ErrorValue::Ref,
            ErrorValue::Name,
            ErrorValue::Num,
            ErrorValue::Circular,
        ] {
            assert_eq!(call_with(Value::Error(error)), Value::Boolean(true));
        }
    }

    #[test]
    fn returns_false_for_na_error() {
        assert_eq!(
            call_with(Value::Error(ErrorValue::Na)),
            Value::Boolean(false)
        );
    }

    #[test]
    fn returns_false_for_non_errors_without_coercion() {
        assert_eq!(call_with(Value::Number(42.0)), Value::Boolean(false));
        assert_eq!(
            call_with(Value::Text("#DIV/0!".to_string())),
            Value::Boolean(false)
        );
        assert_eq!(call_with(Value::Boolean(true)), Value::Boolean(false));
        assert_eq!(call_with(Value::Blank), Value::Boolean(false));
    }

    #[test]
    fn range_argument_uses_top_left_value() {
        let ctx = DummyContext {
            top_left: Value::Error(ErrorValue::Div0),
            other: Value::Error(ErrorValue::Na),
        };
        let fn_ctx = FnContext::new(&ctx);
        let args = [Arg::Range(RangeView::new(&ctx, range_ref()))];

        assert_eq!(ISERR.call(&args, &fn_ctx), Value::Boolean(true));

        let ctx = DummyContext {
            top_left: Value::Error(ErrorValue::Na),
            other: Value::Error(ErrorValue::Div0),
        };
        let fn_ctx = FnContext::new(&ctx);
        let args = [Arg::Range(RangeView::new(&ctx, range_ref()))];

        assert_eq!(ISERR.call(&args, &fn_ctx), Value::Boolean(false));
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = DummyContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [
            Arg::Value(Value::Number(1.0)),
            Arg::Value(Value::Error(ErrorValue::Div0)),
        ];

        assert_eq!(ISERR.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            ISERR.call(&two_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_with(value: Value) -> Value {
        let ctx = DummyContext::default();
        let fn_ctx = FnContext::new(&ctx);
        ISERR.call(&[Arg::Value(value)], &fn_ctx)
    }

    fn range_ref() -> RangeRef {
        RangeRef {
            start: cell_ref(0, 0),
            end: cell_ref(0, 1),
        }
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

    struct DummyContext {
        top_left: Value,
        other: Value,
    }

    impl Default for DummyContext {
        fn default() -> Self {
            Self {
                top_left: Value::Blank,
                other: Value::Blank,
            }
        }
    }

    impl EvalContext for DummyContext {
        fn cell_value(&self, r: CellRef) -> Value {
            if r.row == 0 && r.col == 0 {
                self.top_left.clone()
            } else {
                self.other.clone()
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
