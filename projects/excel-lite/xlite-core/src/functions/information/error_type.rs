use crate::functions::prelude::*;

pub struct ErrorType;

impl Function for ErrorType {
    fn name(&self) -> &'static str {
        "ERROR.TYPE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match args[0].as_value() {
            Value::Error(error) => error_type_code(error)
                .map(Value::Number)
                .unwrap_or(Value::Error(ErrorValue::Na)),
            _ => Value::Error(ErrorValue::Na),
        }
    }
}

fn error_type_code(error: ErrorValue) -> Option<f64> {
    match error {
        ErrorValue::Null => Some(1.0),
        ErrorValue::Div0 => Some(2.0),
        ErrorValue::Value => Some(3.0),
        ErrorValue::Ref => Some(4.0),
        ErrorValue::Name => Some(5.0),
        ErrorValue::Num => Some(6.0),
        ErrorValue::Na => Some(7.0),
        ErrorValue::Circular => None,
    }
}

static ERROR_TYPE: ErrorType = ErrorType;
inventory::submit! { FunctionEntry(&ERROR_TYPE) }

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
        assert_eq!(ERROR_TYPE.name(), "ERROR.TYPE");
        assert_eq!(ERROR_TYPE.arity(), (1, Some(1)));
    }

    #[test]
    fn returns_documented_error_codes() {
        for (error, code) in [
            (ErrorValue::Null, 1.0),
            (ErrorValue::Div0, 2.0),
            (ErrorValue::Value, 3.0),
            (ErrorValue::Ref, 4.0),
            (ErrorValue::Name, 5.0),
            (ErrorValue::Num, 6.0),
            (ErrorValue::Na, 7.0),
        ] {
            assert_eq!(call_error_type(Value::Error(error)), Value::Number(code));
        }
    }

    #[test]
    fn returns_na_for_circular_error() {
        assert_eq!(
            call_error_type(Value::Error(ErrorValue::Circular)),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn returns_na_for_non_errors_without_coercion() {
        assert_eq!(
            call_error_type(Value::Number(42.0)),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(
            call_error_type(Value::Text("#DIV/0!".to_string())),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(
            call_error_type(Value::Boolean(true)),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(call_error_type(Value::Blank), Value::Error(ErrorValue::Na));
    }

    #[test]
    fn range_argument_uses_top_left_value() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Error(ErrorValue::Ref)),
            ((0, 1), Value::Number(42.0)),
            ((1, 0), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            ERROR_TYPE.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(4.0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(42.0)),
            ((0, 1), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            ERROR_TYPE.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn direct_call_rejects_wrong_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let two_args = [
            Arg::Value(Value::Error(ErrorValue::Div0)),
            Arg::Value(Value::Number(1.0)),
        ];

        assert_eq!(
            ERROR_TYPE.call(&[], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            ERROR_TYPE.call(&two_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_error_type(value: Value) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        ERROR_TYPE.call(&[Arg::Value(value)], &fn_ctx)
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
