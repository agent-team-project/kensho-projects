use crate::functions::prelude::*;

pub struct SumProduct;

impl Function for SumProduct {
    fn name(&self) -> &'static str {
        "SUMPRODUCT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        let expected_shape = shape(&args[0]);
        let expected_is_range = is_range(&args[0]);

        for arg in &args[1..] {
            if is_range(arg) != expected_is_range || shape(arg) != expected_shape {
                return Value::Error(ErrorValue::Value);
            }
        }

        let mut total = 0.0;
        for row in 0..expected_shape.0 {
            for col in 0..expected_shape.1 {
                let mut product = 1.0;
                for arg in args {
                    let value = match value_at(arg, row, col, ctx) {
                        Ok(value) => value,
                        Err(error) => return Value::Error(error),
                    };
                    product *= value;
                }
                total += product;
            }
        }

        Value::Number(total)
    }
}

fn shape(arg: &Arg<'_>) -> (u32, u32) {
    match arg {
        Arg::Value(_) => (1, 1),
        Arg::Range(range) => (range.rows(), range.cols()),
    }
}

fn is_range(arg: &Arg<'_>) -> bool {
    matches!(arg, Arg::Range(_))
}

fn value_at(
    arg: &Arg<'_>,
    row: u32,
    col: u32,
    ctx: &FnContext<'_>,
) -> Result<f64, ErrorValue> {
    match arg {
        Arg::Value(value) => ctx.to_number(value),
        Arg::Range(range) => match range.get(row, col) {
            Value::Number(number) => Ok(number),
            Value::Text(_) | Value::Boolean(_) | Value::Blank => Ok(0.0),
            Value::Error(error) => Err(error),
        },
    }
}

static SUMPRODUCT: SumProduct = SumProduct;
inventory::submit! { FunctionEntry(&SUMPRODUCT) }

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
    fn reports_excel_arity_limit() {
        assert_eq!(SUMPRODUCT.arity(), (1, Some(255)));
    }

    #[test]
    fn multiplies_two_ranges_and_sums_products() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((2, 0), Value::Number(3.0)),
            ((0, 1), Value::Number(4.0)),
            ((1, 1), Value::Number(5.0)),
            ((2, 1), Value::Number(6.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SUMPRODUCT.call(
                &[
                    range_arg(&ctx, (0, 0), (2, 0)),
                    range_arg(&ctx, (0, 1), (2, 1)),
                ],
                &fn_ctx
            ),
            Value::Number(32.0)
        );
    }

    #[test]
    fn one_range_sums_numeric_entries_and_zeros_non_numeric_entries() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Text("ignored".to_string())),
            ((1, 0), Value::Boolean(true)),
            ((1, 1), Value::Number(3.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SUMPRODUCT.call(&[range_arg(&ctx, (0, 0), (2, 1))], &fn_ctx),
            Value::Number(5.0)
        );
    }

    #[test]
    fn multiplies_three_ranges_and_sums_products() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((0, 1), Value::Number(3.0)),
            ((1, 1), Value::Number(4.0)),
            ((0, 2), Value::Number(5.0)),
            ((1, 2), Value::Number(6.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SUMPRODUCT.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    range_arg(&ctx, (0, 2), (1, 2)),
                ],
                &fn_ctx
            ),
            Value::Number(63.0)
        );
    }

    #[test]
    fn rejects_mismatched_shapes_and_scalar_range_mixes() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Number(2.0)),
            ((0, 1), Value::Number(3.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SUMPRODUCT.call(
                &[
                    range_arg(&ctx, (0, 0), (1, 0)),
                    range_arg(&ctx, (0, 1), (0, 1)),
                ],
                &fn_ctx
            ),
            Value::Error(ErrorValue::Value)
        );

        assert_eq!(
            SUMPRODUCT.call(
                &[
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(Value::Number(2.0)),
                ],
                &fn_ctx
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn direct_scalars_use_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text(" 2 ".to_string()),
                Value::Boolean(true),
                Value::Number(3.0),
            ]),
            Value::Number(6.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Blank]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_direct_and_range_errors() {
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(2.0)),
            ((0, 1), Value::Error(ErrorValue::Ref)),
            ((1, 0), Value::Number(3.0)),
            ((1, 1), Value::Number(4.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            SUMPRODUCT.call(
                &[
                    range_arg(&ctx, (0, 0), (0, 1)),
                    range_arg(&ctx, (1, 0), (1, 1)),
                ],
                &fn_ctx
            ),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn direct_wrong_arity_returns_value_error() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        assert_eq!(
            SUMPRODUCT.call(&args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        SUMPRODUCT.call(&args, &fn_ctx)
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
