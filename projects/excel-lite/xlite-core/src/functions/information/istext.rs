use crate::functions::prelude::*;

pub struct IsText;

impl Function for IsText {
    fn name(&self) -> &'static str {
        "ISTEXT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Boolean(matches!(args[0].as_value(), Value::Text(_)))
    }
}

static ISTEXT: IsText = IsText;
inventory::submit! { FunctionEntry(&ISTEXT) }

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
        assert_eq!(ISTEXT.name(), "ISTEXT");
        assert_eq!(ISTEXT.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_true_for_text_values() {
        assert_eq!(
            call_istext(Value::Text("text".to_string())),
            Value::Boolean(true)
        );
        assert_eq!(
            call_istext(Value::Text(String::new())),
            Value::Boolean(true)
        );
    }

    #[test]
    fn returns_false_for_non_text_values_without_coercion() {
        assert_eq!(call_istext(Value::Number(42.0)), Value::Boolean(false));
        assert_eq!(call_istext(Value::Boolean(true)), Value::Boolean(false));
        assert_eq!(call_istext(Value::Blank), Value::Boolean(false));

        for error in [
            ErrorValue::Null,
            ErrorValue::Div0,
            ErrorValue::Value,
            ErrorValue::Ref,
            ErrorValue::Name,
            ErrorValue::Num,
            ErrorValue::Na,
            ErrorValue::Circular,
        ] {
            assert_eq!(
                call_istext(Value::Error(error)),
                Value::Boolean(false),
                "{error:?}"
            );
        }
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [
            Arg::Value(Value::Text("text".to_string())),
            Arg::Value(Value::Number(1.0)),
        ];

        assert_eq!(ISTEXT.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            ISTEXT.call(&two_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn range_argument_uses_top_left_scalar_value() {
        let top_left_text = TestContext::with_cells(vec![
            ((0, 0), Value::Text("text".to_string())),
            ((1, 0), Value::Number(42.0)),
        ]);
        let fn_ctx = FnContext::new(&top_left_text);
        assert_eq!(
            ISTEXT.call(&[range_arg(&top_left_text, (0, 0), (1, 0))], &fn_ctx),
            Value::Boolean(true)
        );

        let later_text = TestContext::with_cells(vec![
            ((0, 0), Value::Number(42.0)),
            ((1, 0), Value::Text("text".to_string())),
        ]);
        let fn_ctx = FnContext::new(&later_text);
        assert_eq!(
            ISTEXT.call(&[range_arg(&later_text, (0, 0), (1, 0))], &fn_ctx),
            Value::Boolean(false)
        );
    }

    fn call_istext(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        ISTEXT.call(&[Arg::Value(value)], &fn_ctx)
    }

    fn range_arg(ctx: &TestContext, start: (u32, u32), end: (u32, u32)) -> Arg<'_> {
        Arg::Range(RangeView::new(
            ctx,
            RangeRef {
                start: cell_ref(start),
                end: cell_ref(end),
            },
        ))
    }

    fn cell_ref((row, col): (u32, u32)) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    #[derive(Default)]
    struct TestContext {
        cells: Vec<((u32, u32), Value)>,
    }

    impl TestContext {
        fn with_cells(cells: Vec<((u32, u32), Value)>) -> Self {
            Self { cells }
        }
    }

    impl EvalContext for TestContext {
        fn cell_value(&self, r: CellRef) -> Value {
            self.cells
                .iter()
                .find_map(|((row, col), value)| {
                    (*row == r.row && *col == r.col).then(|| value.clone())
                })
                .unwrap_or(Value::Blank)
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
