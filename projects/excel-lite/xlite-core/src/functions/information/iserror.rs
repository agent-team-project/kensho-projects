use crate::functions::prelude::*;

pub struct IsError;

impl Function for IsError {
    fn name(&self) -> &'static str {
        "ISERROR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Boolean(matches!(args[0].as_value(), Value::Error(_)))
    }
}

static ISERROR: IsError = IsError;
inventory::submit! { FunctionEntry(&ISERROR) }

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
        assert_eq!(ISERROR.name(), "ISERROR");
        assert_eq!(ISERROR.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_true_for_any_error_value() {
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
                call_iserror(Value::Error(error)),
                Value::Boolean(true),
                "{error:?}"
            );
        }
    }

    #[test]
    fn returns_false_for_non_error_values_without_coercion() {
        assert_eq!(call_iserror(Value::Number(42.0)), Value::Boolean(false));
        assert_eq!(
            call_iserror(Value::Text("#DIV/0!".to_string())),
            Value::Boolean(false)
        );
        assert_eq!(call_iserror(Value::Boolean(true)), Value::Boolean(false));
        assert_eq!(call_iserror(Value::Blank), Value::Boolean(false));
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [
            Arg::Value(Value::Error(ErrorValue::Div0)),
            Arg::Value(Value::Number(1.0)),
        ];

        assert_eq!(ISERROR.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            ISERROR.call(&two_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn range_argument_uses_top_left_scalar_value() {
        let top_left_error = TestContext::with_cells(vec![
            ((0, 0), Value::Error(ErrorValue::Div0)),
            ((1, 0), Value::Number(42.0)),
        ]);
        let fn_ctx = FnContext::new(&top_left_error);
        assert_eq!(
            ISERROR.call(&[range_arg(&top_left_error, (0, 0), (1, 0))], &fn_ctx),
            Value::Boolean(true)
        );

        let later_error = TestContext::with_cells(vec![
            ((0, 0), Value::Number(42.0)),
            ((1, 0), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&later_error);
        assert_eq!(
            ISERROR.call(&[range_arg(&later_error, (0, 0), (1, 0))], &fn_ctx),
            Value::Boolean(false)
        );
    }

    fn call_iserror(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        ISERROR.call(&[Arg::Value(value)], &fn_ctx)
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
