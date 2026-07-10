use crate::functions::prelude::*;

const MAX_RESULT_CHARS: usize = 32_767;
const MIN_ARGS: usize = 3;
const MAX_ARGS: usize = 254;

pub struct TextJoin;

impl Function for TextJoin {
    fn name(&self) -> &'static str {
        "TEXTJOIN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let delimiter = match ctx.to_text(&args[0].as_value()) {
            Ok(delimiter) => delimiter,
            Err(error) => return Value::Error(error),
        };
        let ignore_empty = match ctx.to_bool(&args[1].as_value()) {
            Ok(ignore_empty) => ignore_empty,
            Err(error) => return Value::Error(error),
        };

        let mut state = JoinState::new(delimiter, ignore_empty);
        for arg in &args[2..] {
            let append_result = match arg {
                Arg::Value(value) => state.append_value(value, ctx),
                Arg::Range(range) => state.append_range(range, ctx),
            };
            if let Err(error) = append_result {
                return Value::Error(error);
            }
        }

        Value::Text(state.result)
    }
}

static TEXTJOIN: TextJoin = TextJoin;
inventory::submit! { FunctionEntry(&TEXTJOIN) }

struct JoinState {
    delimiter: String,
    ignore_empty: bool,
    result: String,
    result_chars: usize,
    has_included_value: bool,
}

impl JoinState {
    fn new(delimiter: String, ignore_empty: bool) -> Self {
        Self {
            delimiter,
            ignore_empty,
            result: String::new(),
            result_chars: 0,
            has_included_value: false,
        }
    }

    fn append_range(
        &mut self,
        range: &RangeView<'_>,
        ctx: &FnContext<'_>,
    ) -> Result<(), ErrorValue> {
        for value in range.iter() {
            self.append_value(&value, ctx)?;
        }
        Ok(())
    }

    fn append_value(&mut self, value: &Value, ctx: &FnContext<'_>) -> Result<(), ErrorValue> {
        let text = ctx.to_text(value)?;
        if self.ignore_empty && text.is_empty() {
            return Ok(());
        }

        if self.has_included_value {
            self.append_text(self.delimiter.clone())?;
        }
        self.append_text(text)?;
        self.has_included_value = true;
        Ok(())
    }

    fn append_text(&mut self, text: String) -> Result<(), ErrorValue> {
        let text_chars = text.chars().count();
        let Some(next_chars) = self.result_chars.checked_add(text_chars) else {
            return Err(ErrorValue::Value);
        };
        if next_chars > MAX_RESULT_CHARS {
            return Err(ErrorValue::Value);
        }

        self.result.push_str(&text);
        self.result_chars = next_chars;
        Ok(())
    }
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
        assert_eq!(TEXTJOIN.name(), "TEXTJOIN");
        assert_eq!(TEXTJOIN.arity(), (3, Some(254)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let too_few_args = vec![
            Arg::Value(Value::Text(",".to_string())),
            Arg::Value(Value::Boolean(true)),
        ];
        let too_many_args = vec![Arg::Value(Value::Text("x".to_string())); MAX_ARGS + 1];

        assert_eq!(
            TEXTJOIN.call(&too_few_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            TEXTJOIN.call(&too_many_args, &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn joins_scalar_text_with_delimiter() {
        assert_eq!(
            call_values(
                Value::Text(", ".to_string()),
                Value::Boolean(true),
                vec![
                    Value::Text("red".to_string()),
                    Value::Text("blue".to_string()),
                    Value::Text("green".to_string()),
                ],
            ),
            Value::Text("red, blue, green".to_string())
        );
    }

    #[test]
    fn ignore_empty_true_skips_blank_and_empty_text() {
        assert_eq!(
            call_values(
                Value::Text("|".to_string()),
                Value::Boolean(true),
                vec![
                    Value::Text("A".to_string()),
                    Value::Blank,
                    Value::Text(String::new()),
                    Value::Text("B".to_string()),
                ],
            ),
            Value::Text("A|B".to_string())
        );
    }

    #[test]
    fn ignore_empty_false_includes_blank_and_empty_text() {
        assert_eq!(
            call_values(
                Value::Text("|".to_string()),
                Value::Boolean(false),
                vec![
                    Value::Text("A".to_string()),
                    Value::Blank,
                    Value::Text(String::new()),
                    Value::Text("B".to_string()),
                ],
            ),
            Value::Text("A|||B".to_string())
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
            TEXTJOIN.call(
                &[
                    Arg::Value(Value::Text("-".to_string())),
                    Arg::Value(Value::Boolean(true)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Text("A-B-C-D".to_string())
        );
    }

    #[test]
    fn coerces_mixed_scalar_text_values() {
        assert_eq!(
            call_values(
                Value::Text(",".to_string()),
                Value::Boolean(false),
                vec![
                    Value::Number(12.0),
                    Value::Boolean(false),
                    Value::Blank,
                    Value::Text("tail".to_string()),
                ],
            ),
            Value::Text("12,FALSE,,tail".to_string())
        );
    }

    #[test]
    fn coerces_delimiter_and_ignore_empty_from_scalars() {
        assert_eq!(
            call_values(
                Value::Number(0.0),
                Value::Text("TRUE".to_string()),
                vec![
                    Value::Text("A".to_string()),
                    Value::Blank,
                    Value::Text("B".to_string()),
                ],
            ),
            Value::Text("A0B".to_string())
        );
    }

    #[test]
    fn propagates_delimiter_and_ignore_empty_coercion_errors() {
        assert_eq!(
            call_values(
                Value::Error(ErrorValue::Ref),
                Value::Boolean(true),
                vec![Value::Text("A".to_string())],
            ),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_values(
                Value::Text(",".to_string()),
                Value::Text("maybe".to_string()),
                vec![Value::Text("A".to_string())],
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_direct_and_range_errors_even_when_ignoring_empty() {
        assert_eq!(
            call_values(
                Value::Text(",".to_string()),
                Value::Boolean(true),
                vec![
                    Value::Text("before".to_string()),
                    Value::Error(ErrorValue::Na),
                    Value::Text("after".to_string()),
                ],
            ),
            Value::Error(ErrorValue::Na)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Text("before".to_string())),
            ((0, 1), Value::Blank),
            ((1, 0), Value::Error(ErrorValue::Div0)),
            ((1, 1), Value::Text("after".to_string())),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            TEXTJOIN.call(
                &[
                    Arg::Value(Value::Text(",".to_string())),
                    Arg::Value(Value::Boolean(true)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn enforces_result_length_limit_after_delimiter_or_text_append() {
        assert_eq!(
            call_values(
                Value::Text(",".to_string()),
                Value::Boolean(true),
                vec![Value::Text("x".repeat(MAX_RESULT_CHARS))],
            ),
            Value::Text("x".repeat(MAX_RESULT_CHARS))
        );
        assert_eq!(
            call_values(
                Value::Text(",".to_string()),
                Value::Boolean(true),
                vec![
                    Value::Text("x".repeat(MAX_RESULT_CHARS)),
                    Value::Text("y".to_string()),
                ],
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(
                Value::Text("x".repeat(MAX_RESULT_CHARS + 1)),
                Value::Boolean(false),
                vec![Value::Blank, Value::Blank],
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(delimiter: Value, ignore_empty: Value, values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let mut args = vec![Arg::Value(delimiter), Arg::Value(ignore_empty)];
        args.extend(values.into_iter().map(Arg::Value));
        TEXTJOIN.call(&args, &fn_ctx)
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
