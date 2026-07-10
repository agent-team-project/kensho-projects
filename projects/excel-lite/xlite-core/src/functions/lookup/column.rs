use crate::functions::prelude::*;

pub struct Column;

impl Function for Column {
    fn name(&self) -> &'static str {
        "COLUMN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(1))
    }

    fn argument_mode(&self, index: usize) -> FunctionArgMode {
        if index == 0 {
            FunctionArgMode::Reference
        } else {
            FunctionArgMode::Value
        }
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match args {
            [] => Value::Number(f64::from(ctx.current_cell().coord.col + 1)),
            [Arg::Range(range)] => Value::Number(f64::from(range.top_left().col + 1)),
            [Arg::Value(Value::Error(error))] => Value::Error(*error),
            [Arg::Value(_)] => Value::Error(ErrorValue::Value),
            _ => Value::Error(ErrorValue::Value),
        }
    }
}

static COLUMN: Column = Column;
inventory::submit! { FunctionEntry(&COLUMN) }

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
        assert_eq!(COLUMN.name(), "COLUMN");
        assert_eq!(COLUMN.arity(), (0, Some(1)));
        assert_eq!(COLUMN.argument_mode(0), FunctionArgMode::Reference);
        assert_eq!(COLUMN.argument_mode(1), FunctionArgMode::Value);
    }

    #[test]
    fn rejects_too_many_direct_arguments() {
        let ctx = ShapeOnlyContext::at(5, 3);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COLUMN.call(
                &[
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(2.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn omitted_reference_returns_current_cell_column() {
        let ctx = ShapeOnlyContext::at(5, 3);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(COLUMN.call(&[], &fn_ctx), Value::Number(4.0));
    }

    #[test]
    fn range_reference_returns_normalized_left_column_without_materializing_values() {
        let ctx = ShapeOnlyContext::at(0, 0);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COLUMN.call(&[range_arg(&ctx, (9, 3), (9, 3))], &fn_ctx),
            Value::Number(4.0)
        );
        assert_eq!(
            COLUMN.call(&[range_arg(&ctx, (3, 3), (0, 1))], &fn_ctx),
            Value::Number(2.0)
        );
    }

    #[test]
    fn scalar_non_reference_arguments_return_value_error() {
        let ctx = ShapeOnlyContext::at(0, 0);
        let fn_ctx = FnContext::new(&ctx);

        for value in [
            Value::Number(1.0),
            Value::Text("A1".to_string()),
            Value::Boolean(false),
            Value::Blank,
        ] {
            assert_eq!(
                COLUMN.call(&[Arg::Value(value)], &fn_ctx),
                Value::Error(ErrorValue::Value)
            );
        }
    }

    #[test]
    fn direct_scalar_errors_are_propagated() {
        let ctx = ShapeOnlyContext::at(0, 0);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            COLUMN.call(&[Arg::Value(Value::Error(ErrorValue::Div0))], &fn_ctx),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            COLUMN.call(&[Arg::Value(Value::Error(ErrorValue::Name))], &fn_ctx),
            Value::Error(ErrorValue::Name)
        );
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

    struct ShapeOnlyContext {
        current: Coord,
    }

    impl ShapeOnlyContext {
        fn at(row: u32, col: u32) -> Self {
            Self {
                current: Coord { row, col },
            }
        }
    }

    impl EvalContext for ShapeOnlyContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            panic!("COLUMN should not materialize reference values")
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
            CellId::new(0, self.current)
        }
    }
}
