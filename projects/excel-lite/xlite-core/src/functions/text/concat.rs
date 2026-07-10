use crate::functions::prelude::*;

const MAX_RESULT_CHARS: usize = 32_767;
const MAX_ARGS: usize = 254;

pub struct Concat;

impl Function for Concat {
    fn name(&self) -> &'static str {
        "CONCAT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.is_empty() || args.len() > MAX_ARGS {
            return Value::Error(ErrorValue::Value);
        }

        let mut result = String::new();
        let mut result_chars = 0;

        for arg in args {
            let append_result = match arg {
                Arg::Value(value) => append_value(&mut result, &mut result_chars, value, ctx),
                Arg::Range(range) => append_range(&mut result, &mut result_chars, range, ctx),
            };

            if let Err(error) = append_result {
                return Value::Error(error);
            }
        }

        Value::Text(result)
    }
}

static CONCAT: Concat = Concat;
inventory::submit! { FunctionEntry(&CONCAT) }

fn append_range(
    result: &mut String,
    result_chars: &mut usize,
    range: &RangeView<'_>,
    ctx: &FnContext<'_>,
) -> Result<(), ErrorValue> {
    for value in range.iter() {
        append_value(result, result_chars, &value, ctx)?;
    }
    Ok(())
}

fn append_value(
    result: &mut String,
    result_chars: &mut usize,
    value: &Value,
    ctx: &FnContext<'_>,
) -> Result<(), ErrorValue> {
    let text = ctx.to_text(value)?;
    let text_chars = text.chars().count();
    if text_chars > MAX_RESULT_CHARS - *result_chars {
        return Err(ErrorValue::Value);
    }

    result.push_str(&text);
    *result_chars += text_chars;
    Ok(())
}

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
        assert_eq!(CONCAT.name(), "CONCAT");
        assert_eq!(CONCAT.arity(), (1, Some(254)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let too_many_args = vec![Arg::Value(Value::Text("x".to_string())); 255];

        assert_eq!(CONCAT.call(&[], &fn_ctx), Value::Error(ErrorValue::Value));
        assert_eq!(
            CONCAT.call(&too_many_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn concatenates_direct_scalars_with_text_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Text("A".to_string()),
                Value::Number(2.0),
                Value::Boolean(true),
                Value::Blank,
                Value::Text("z".to_string()),
            ]),
            Value::Text("A2TRUEz".to_string())
        );
    }

    #[test]
    fn scans_ranges_in_row_major_order() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("A".to_string())),
            ((0, 1), Value::Text("B".to_string())),
            ((1, 0), Value::Text("C".to_string())),
            ((1, 1), Value::Text("D".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            CONCAT.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Text("ABCD".to_string())
        );
    }

    #[test]
    fn combines_ranges_and_direct_values() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(1.0)),
            ((0, 1), Value::Boolean(false)),
            ((1, 0), Value::Blank),
            ((1, 1), Value::Text("tail".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            CONCAT.call(
                &[
                    Arg::Value(Value::Text("start:".to_string())),
                    range_arg(&ctx, (0, 0), (1, 1)),
                ],
                &fn_ctx
            ),
            Value::Text("start:1FALSEtail".to_string())
        );
    }

    #[test]
    fn propagates_direct_and_range_errors() {
        assert_eq!(
            call_values(vec![Value::Text("before".to_string()), Value::Error(ErrorValue::Na)]),
            Value::Error(ErrorValue::Na)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("before".to_string())),
            ((0, 1), Value::Error(ErrorValue::Div0)),
            ((1, 0), Value::Error(ErrorValue::Ref)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            CONCAT.call(&[range_arg(&ctx, (0, 0), (1, 1))], &fn_ctx),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn enforces_result_length_limit_after_each_append() {
        assert_eq!(
            call_values(vec![Value::Text("x".repeat(MAX_RESULT_CHARS))]),
            Value::Text("x".repeat(MAX_RESULT_CHARS))
        );
        assert_eq!(
            call_values(vec![
                Value::Text("x".repeat(MAX_RESULT_CHARS)),
                Value::Text("y".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Text("x".repeat(MAX_RESULT_CHARS + 1))]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        CONCAT.call(&args, &fn_ctx)
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
