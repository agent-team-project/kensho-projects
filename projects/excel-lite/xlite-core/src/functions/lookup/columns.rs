use crate::functions::prelude::*;

pub struct Columns;

impl Function for Columns {
    fn name(&self) -> &'static str {
        "COLUMNS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match &args[0] {
            Arg::Range(range) => Value::Number(range.cols() as f64),
            Arg::Value(Value::Error(error)) => Value::Error(*error),
            Arg::Value(_) => Value::Number(1.0),
        }
    }
}

static COLUMNS: Columns = Columns;
inventory::submit! { FunctionEntry(&COLUMNS) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(COLUMNS.name(), "COLUMNS");
        assert_eq!(COLUMNS.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = ShapeOnlyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(COLUMNS.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            COLUMNS.call(
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
    fn returns_range_column_count_without_reading_cells() {
        let ctx = ShapeOnlyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COLUMNS.call(&[range_arg(&ctx, (0, 2), (3, 4))], &fn_ctx),
            Value::Number(3.0)
        );
        assert_eq!(
            COLUMNS.call(&[range_arg(&ctx, (0, 0), (9, 0))], &fn_ctx),
            Value::Number(1.0)
        );
        assert_eq!(
            COLUMNS.call(&[range_arg(&ctx, (0, 0), (0, 2))], &fn_ctx),
            Value::Number(3.0)
        );
    }

    #[test]
    fn treats_direct_non_error_scalars_as_one_cell_arrays() {
        for value in [
            Value::Number(42.0),
            Value::Text("text".to_string()),
            Value::Boolean(true),
            Value::Blank,
        ] {
            assert_eq!(call_value(value), Value::Number(1.0));
        }
    }

    #[test]
    fn direct_scalar_error_is_returned() {
        assert_eq!(
            call_value(Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_value(value: Value) -> Value {
        let ctx = ShapeOnlyContext;
        let fn_ctx = FnContext::new(&ctx);
        COLUMNS.call(&[Arg::Value(value)], &fn_ctx)
    }

    fn range_arg(ctx: &ShapeOnlyContext, start: (u32, u32), end: (u32, u32)) -> Arg<'_> {
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

    struct ShapeOnlyContext;

    impl EvalContext for ShapeOnlyContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            panic!("COLUMNS should not materialize range values")
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
