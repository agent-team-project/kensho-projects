use crate::functions::prelude::*;

pub struct Rows;

impl Function for Rows {
    fn name(&self) -> &'static str {
        "ROWS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match &args[0] {
            Arg::Range(range) => Value::Number(range.rows() as f64),
            Arg::Value(Value::Error(error)) => Value::Error(*error),
            Arg::Value(_) => Value::Number(1.0),
        }
    }
}

static ROWS: Rows = Rows;
inventory::submit! { FunctionEntry(&ROWS) }

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
        assert_eq!(ROWS.name(), "ROWS");
        assert_eq!(ROWS.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(ROWS.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            ROWS.call(
                &[
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(2.0)),
                ],
                &fn_ctx
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_range_row_count_without_materializing_values() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            ROWS.call(&[range_arg(&ctx, (0, 2), (3, 4))], &fn_ctx),
            Value::Number(4.0)
        );
        assert_eq!(
            ROWS.call(&[range_arg(&ctx, (0, 0), (0, 2))], &fn_ctx),
            Value::Number(1.0)
        );
        assert_eq!(
            ROWS.call(&[range_arg(&ctx, (9, 0), (0, 0))], &fn_ctx),
            Value::Number(10.0)
        );
    }

    #[test]
    fn returns_one_for_direct_non_error_scalars() {
        for value in [
            Value::Number(42.0),
            Value::Text("text".to_string()),
            Value::Boolean(false),
            Value::Blank,
        ] {
            assert_eq!(call_value(value), Value::Number(1.0));
        }
    }

    #[test]
    fn propagates_direct_scalar_errors() {
        assert_eq!(
            call_value(Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_value(Value::Error(ErrorValue::Ref)),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_value(value: Value) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        ROWS.call(&[Arg::Value(value)], &fn_ctx)
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

    struct TestContext;

    impl EvalContext for TestContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            panic!("ROWS should not materialize range values")
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
