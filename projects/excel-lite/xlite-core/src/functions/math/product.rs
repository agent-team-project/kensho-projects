use crate::functions::prelude::*;

pub struct Product;

impl Function for Product {
    fn name(&self) -> &'static str {
        "PRODUCT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, None)
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let mut product = 1.0;
        let mut count = 0usize;

        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => {
                    for number in numbers {
                        count += 1;
                        product *= number;
                    }
                }
                Err(error) => return Value::Error(error),
            }
        }

        if count == 0 {
            Value::Number(0.0)
        } else {
            Value::Number(product)
        }
    }
}

static PRODUCT: Product = Product;
inventory::submit! { FunctionEntry(&PRODUCT) }

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
    fn reports_variadic_arity() {
        assert_eq!(PRODUCT.arity(), (1, None));
    }

    #[test]
    fn multiplies_scalar_numbers() {
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Number(3.0), Value::Number(4.0)]),
            Value::Number(24.0)
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text(" 2 ".to_string()),
                Value::Boolean(true),
                Value::Number(3.0),
            ]),
            Value::Number(6.0)
        );
        assert_eq!(call_values(vec![Value::Blank]), Value::Number(0.0));
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Text("ignored".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(3.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            PRODUCT.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(6.0)
        );
    }

    #[test]
    fn returns_zero_for_blank_only_range() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            PRODUCT.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_aggregation_errors() {
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            PRODUCT.call(&[range_arg(&ctx, (0, 0), (0, 1))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        PRODUCT.call(&args, &fn_ctx)
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
            CellId {
                sheet: 0,
                coord: Coord { row: 0, col: 0 },
            }
        }
    }
}
