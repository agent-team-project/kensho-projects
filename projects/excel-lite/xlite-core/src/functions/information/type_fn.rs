use crate::functions::prelude::*;

pub struct TypeFn;

impl Function for TypeFn {
    fn name(&self) -> &'static str {
        "TYPE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match args[0].as_value() {
            Value::Number(_) | Value::Blank => Value::Number(1.0),
            Value::Text(_) => Value::Number(2.0),
            Value::Boolean(_) => Value::Number(4.0),
            Value::Error(_) => Value::Number(16.0),
        }
    }
}

static TYPE: TypeFn = TypeFn;
inventory::submit! { FunctionEntry(&TYPE) }

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
        assert_eq!(TYPE.name(), "TYPE");
        assert_eq!(TYPE.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_documented_type_codes_without_coercion() {
        assert_eq!(call_type(Value::Number(7.0)), Value::Number(1.0));
        assert_eq!(call_type(Value::Blank), Value::Number(1.0));
        assert_eq!(
            call_type(Value::Text("text".to_string())),
            Value::Number(2.0)
        );
        assert_eq!(call_type(Value::Text("7".to_string())), Value::Number(2.0));
        assert_eq!(call_type(Value::Boolean(true)), Value::Number(4.0));
        assert_eq!(call_type(Value::Boolean(false)), Value::Number(4.0));
    }

    #[test]
    fn returns_error_code_for_all_error_values() {
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
                call_type(Value::Error(error)),
                Value::Number(16.0),
                "{error:?}"
            );
        }
    }

    #[test]
    fn range_argument_uses_top_left_scalar_value() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("top-left".to_string())),
            ((0, 1), Value::Number(7.0)),
            ((1, 0), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            TYPE.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(2.0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Blank),
            ((0, 1), Value::Boolean(true)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            TYPE.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Number(1.0)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [Arg::Value(Value::Number(1.0)), Arg::Value(Value::Number(2.0))];

        assert_eq!(TYPE.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            TYPE.call(&two_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_type(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        TYPE.call(&[Arg::Value(value)], &fn_ctx)
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
