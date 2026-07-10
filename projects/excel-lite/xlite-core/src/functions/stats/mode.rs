use std::collections::HashMap;

use crate::functions::prelude::*;

pub struct Mode;

impl Function for Mode {
    fn name(&self) -> &'static str {
        "MODE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > 255 {
            return Value::Error(ErrorValue::Value);
        }

        let mut counts: HashMap<u64, (f64, usize)> = HashMap::new();
        let mut mode = None;
        let mut mode_count = 1usize;

        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => {
                    for number in numbers {
                        let number = canonical_number(number);
                        let entry = counts.entry(number_key(number)).or_insert((number, 0));
                        entry.1 += 1;

                        if entry.1 >= 2 && entry.1 > mode_count {
                            mode = Some(entry.0);
                            mode_count = entry.1;
                        }
                    }
                }
                Err(error) => return Value::Error(error),
            }
        }

        match mode {
            Some(number) => Value::Number(number),
            None => Value::Error(ErrorValue::Na),
        }
    }
}

fn canonical_number(number: f64) -> f64 {
    if number == 0.0 { 0.0 } else { number }
}

fn number_key(number: f64) -> u64 {
    canonical_number(number).to_bits()
}

static MODE: Mode = Mode;
inventory::submit! { FunctionEntry(&MODE) }

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
        assert_eq!(MODE.arity(), (1, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));

        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(MODE.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn returns_most_frequent_scalar_number() {
        assert_eq!(
            call_values(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(2.0),
                Value::Number(4.0),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn returns_na_when_no_value_repeats() {
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn tie_uses_first_value_that_reaches_highest_frequency() {
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn uses_scalar_arithmetic_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text("2".to_string()),
                Value::Boolean(true),
                Value::Blank,
                Value::Number(2.0),
                Value::Boolean(false),
            ]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn aggregates_range_numbers_and_ignores_non_numbers() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(0.0)),
            ((1, 0), Value::Text("0".to_string())),
            ((2, 0), Value::Boolean(true)),
            ((4, 0), Value::Number(0.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MODE.call(&[range_arg(&ctx, (0, 0), (4, 0))], &fn_ctx),
            Value::Number(0.0)
        );
    }

    #[test]
    fn combines_ranges_and_direct_scalars() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(8.0)),
            ((0, 1), Value::Number(3.0)),
            ((1, 0), Value::Text("ignored".to_string())),
            ((1, 1), Value::Number(8.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MODE.call(
                &[
                    Arg::Value(Value::Number(3.0)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(3.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(3.0)
        );
    }

    #[test]
    fn propagates_aggregation_errors() {
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Text("apple".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((1, 0), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MODE.call(&[range_arg(&ctx, (0, 0), (1, 0))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        MODE.call(&args, &fn_ctx)
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
