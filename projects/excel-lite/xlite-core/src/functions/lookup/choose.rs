use crate::functions::prelude::*;

const MIN_ARGS: usize = 2;
const MAX_ARGS: usize = 255;

pub struct Choose;

impl Function for Choose {
    fn name(&self) -> &'static str {
        "CHOOSE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let index = match ctx.to_number(&args[0].as_value()) {
            Ok(index) => index,
            Err(error) => return Value::Error(error),
        };
        if !index.is_finite() {
            return Value::Error(ErrorValue::Value);
        }

        let index = index.floor();
        let value_count = (args.len() - 1) as f64;
        if index < 1.0 || index > value_count {
            return Value::Error(ErrorValue::Value);
        }

        args[index as usize].as_value()
    }
}

static CHOOSE: Choose = Choose;
inventory::submit! { FunctionEntry(&CHOOSE) }

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
        assert_eq!(CHOOSE.name(), "CHOOSE");
        assert_eq!(CHOOSE.arity(), (2, Some(255)));
    }

    #[test]
    fn selects_one_based_value_argument() {
        assert_eq!(
            call_values(vec![num(2.0), text("red"), text("blue"), text("green")]),
            text("blue")
        );
        assert_eq!(
            call_values(vec![num(3.0), num(10.0), num(20.0), num(30.0)]),
            num(30.0)
        );
    }

    #[test]
    fn floors_fractional_indexes_before_selection() {
        assert_eq!(
            call_values(vec![num(1.9), text("first"), text("second")]),
            text("first")
        );
        assert_eq!(
            call_values(vec![num(2.1), text("first"), text("second")]),
            text("second")
        );
    }

    #[test]
    fn rejects_out_of_range_and_non_finite_indexes() {
        assert_eq!(
            call_values(vec![num(0.0), text("bad")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![num(3.0), text("a"), text("b")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Number(f64::INFINITY), text("bad")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Number(f64::NAN), text("bad")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_index_coercion_errors() {
        assert_eq!(
            call_values(vec![Value::Error(ErrorValue::Div0), text("bad")]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(vec![text("not numeric"), text("bad")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_selected_error_and_ignores_unselected_errors() {
        assert_eq!(
            call_values(vec![num(2.0), Value::Error(ErrorValue::Div0), text("ok")]),
            text("ok")
        );
        assert_eq!(
            call_values(vec![num(1.0), Value::Error(ErrorValue::Div0), text("ok")]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn scalarizes_selected_range_to_top_left() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Error(ErrorValue::Div0)),
            ((1, 0), text("ignored")),
            ((0, 1), text("selected")),
            ((1, 1), text("not top-left")),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            CHOOSE.call(
                &[
                    Arg::Value(num(2.0)),
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                ],
                &fn_ctx,
            ),
            text("selected")
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let too_many_args = vec![Arg::Value(num(1.0)); 256];

        assert_eq!(CHOOSE.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            CHOOSE.call(&[Arg::Value(num(1.0))], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            CHOOSE.call(&too_many_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        CHOOSE.call(&args, &fn_ctx)
    }

    fn num(value: f64) -> Value {
        Value::Number(value)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
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
