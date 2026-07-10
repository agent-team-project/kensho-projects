use crate::functions::prelude::*;

pub struct NFn;

impl Function for NFn {
    fn name(&self) -> &'static str {
        "N"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match args[0].as_value() {
            Value::Number(number) => Value::Number(number),
            Value::Boolean(true) => Value::Number(1.0),
            Value::Boolean(false) => Value::Number(0.0),
            Value::Error(error) => Value::Error(error),
            Value::Text(_) | Value::Blank => Value::Number(0.0),
        }
    }
}

static N: NFn = NFn;
inventory::submit! { FunctionEntry(&N) }

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(N.name(), "N");
        assert_eq!(N.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_numbers_unchanged() {
        assert_eq!(call_n(Value::Number(7.0)), Value::Number(7.0));
        assert_eq!(call_n(Value::Number(-2.5)), Value::Number(-2.5));
    }

    #[test]
    fn converts_booleans_to_numbers() {
        assert_eq!(call_n(Value::Boolean(true)), Value::Number(1.0));
        assert_eq!(call_n(Value::Boolean(false)), Value::Number(0.0));
    }

    #[test]
    fn returns_zero_for_text_and_blank_without_numeric_text_coercion() {
        assert_eq!(call_n(Value::Text("7".to_string())), Value::Number(0.0));
        assert_eq!(call_n(Value::Text("text".to_string())), Value::Number(0.0));
        assert_eq!(call_n(Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn returns_error_values_unchanged() {
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
            assert_eq!(call_n(Value::Error(error)), Value::Error(error), "{error:?}");
        }
    }

    #[test]
    fn range_argument_uses_top_left_scalar_value() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("7".to_string())),
            ((0, 1), Value::Number(99.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            N.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Number(0.0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(7.0)),
            ((0, 1), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            N.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Number(7.0)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [Arg::Value(Value::Number(1.0)), Arg::Value(Value::Number(2.0))];

        assert_eq!(N.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(N.call(&two_args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    fn call_n(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        N.call(&[Arg::Value(value)], &fn_ctx)
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
        cells: HashMap<(u32, u32), Value>,
    }

    impl TestContext {
        fn with_cells(cells: Vec<((u32, u32), Value)>) -> Self {
            Self {
                cells: cells.into_iter().collect(),
            }
        }
    }

    impl EvalContext for TestContext {
        fn cell_value(&self, r: CellRef) -> Value {
            self.cells
                .get(&(r.row, r.col))
                .cloned()
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
