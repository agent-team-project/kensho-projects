use crate::functions::prelude::*;

pub struct Row;

impl Function for Row {
    fn name(&self) -> &'static str {
        "ROW"
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
            [] => Value::Number(f64::from(ctx.current_cell().coord.row + 1)),
            [Arg::Range(range)] => Value::Number(f64::from(range.top_left().row + 1)),
            [Arg::Value(Value::Error(error))] => Value::Error(*error),
            [Arg::Value(_)] => Value::Error(ErrorValue::Value),
            _ => Value::Error(ErrorValue::Value),
        }
    }
}

static ROW: Row = Row;
inventory::submit! { FunctionEntry(&ROW) }

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
        assert_eq!(ROW.name(), "ROW");
        assert_eq!(ROW.arity(), (0, Some(1)));
        assert_eq!(ROW.argument_mode(0), FunctionArgMode::Reference);
        assert_eq!(ROW.argument_mode(1), FunctionArgMode::Value);
    }

    #[test]
    fn rejects_too_many_direct_arguments() {
        let ctx = ShapeOnlyContext::at(6, 2);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            ROW.call(
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
    fn omitted_reference_returns_current_cell_row() {
        let ctx = ShapeOnlyContext::at(6, 2);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(ROW.call(&[], &fn_ctx), Value::Number(7.0));
    }

    #[test]
    fn range_reference_returns_normalized_top_row_without_materializing_values() {
        let ctx = ShapeOnlyContext::at(0, 0);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            ROW.call(&[range_arg(&ctx, (4, 0), (4, 0))], &fn_ctx),
            Value::Number(5.0)
        );
        assert_eq!(
            ROW.call(&[range_arg(&ctx, (3, 1), (0, 0))], &fn_ctx),
            Value::Number(1.0)
        );
    }

    #[test]
    fn scalar_non_reference_arguments_return_value_error() {
        let ctx = ShapeOnlyContext::at(0, 0);
        let fn_ctx = FnContext::new(&ctx);

        for value in [
            Value::Number(1.0),
            Value::Text("A1".to_string()),
            Value::Boolean(true),
            Value::Blank,
        ] {
            assert_eq!(
                ROW.call(&[Arg::Value(value)], &fn_ctx),
                Value::Error(ErrorValue::Value)
            );
        }
    }

    #[test]
    fn direct_scalar_errors_are_propagated() {
        let ctx = ShapeOnlyContext::at(0, 0);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            ROW.call(&[Arg::Value(Value::Error(ErrorValue::Div0))], &fn_ctx),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            ROW.call(&[Arg::Value(Value::Error(ErrorValue::Ref))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
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
            panic!("ROW should not materialize reference values")
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
