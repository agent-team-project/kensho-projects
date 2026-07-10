use crate::functions::prelude::*;

pub struct IsLogical;

impl Function for IsLogical {
    fn name(&self) -> &'static str {
        "ISLOGICAL"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        Value::Boolean(matches!(args[0].as_value(), Value::Boolean(_)))
    }
}

static ISLOGICAL: IsLogical = IsLogical;
inventory::submit! { FunctionEntry(&ISLOGICAL) }

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
        assert_eq!(ISLOGICAL.name(), "ISLOGICAL");
        assert_eq!(ISLOGICAL.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_true_for_logical_values() {
        assert_eq!(call_islogical(Value::Boolean(true)), Value::Boolean(true));
        assert_eq!(call_islogical(Value::Boolean(false)), Value::Boolean(true));
    }

    #[test]
    fn returns_false_for_nonlogical_values_without_coercion() {
        assert_eq!(call_islogical(Value::Number(1.0)), Value::Boolean(false));
        assert_eq!(
            call_islogical(Value::Text("TRUE".to_string())),
            Value::Boolean(false)
        );
        assert_eq!(
            call_islogical(Value::Text(String::new())),
            Value::Boolean(false)
        );
        assert_eq!(call_islogical(Value::Blank), Value::Boolean(false));

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
                call_islogical(Value::Error(error)),
                Value::Boolean(false),
                "{error:?}"
            );
        }
    }

    #[test]
    fn range_argument_uses_top_left_scalar_value() {
        let top_left_logical = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(false)),
            ((1, 0), Value::Number(1.0)),
        ]);
        let fn_ctx = FnContext::new(&top_left_logical);

        assert_eq!(
            ISLOGICAL.call(&[range_arg(&top_left_logical, (0, 0), (1, 0))], &fn_ctx),
            Value::Boolean(true)
        );

        let later_logical = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Boolean(true)),
        ]);
        let fn_ctx = FnContext::new(&later_logical);

        assert_eq!(
            ISLOGICAL.call(&[range_arg(&later_logical, (0, 0), (1, 0))], &fn_ctx),
            Value::Boolean(false)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [
            Arg::Value(Value::Boolean(true)),
            Arg::Value(Value::Boolean(false)),
        ];

        assert_eq!(
            ISLOGICAL.call(&[], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            ISLOGICAL.call(&two_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_islogical(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        ISLOGICAL.call(&[Arg::Value(value)], &fn_ctx)
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
