use crate::functions::prelude::*;

const MIN_ARGS: usize = 1;
const MAX_ARGS: usize = 2;

pub struct Indirect;

impl Function for Indirect {
    fn name(&self) -> &'static str {
        "INDIRECT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match self.call_result(args, ctx) {
            EvalResult::Value(value) => value,
            EvalResult::Range(_) => Value::Error(ErrorValue::Value),
        }
    }

    fn call_result(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> EvalResult {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return EvalResult::Value(Value::Error(ErrorValue::Value));
        }

        let ref_text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return EvalResult::Value(Value::Error(error)),
        };

        let a1 = match args.get(1) {
            Some(arg) => match ctx.to_bool(&arg.as_value()) {
                Ok(a1) => a1,
                Err(error) => return EvalResult::Value(Value::Error(error)),
            },
            None => true,
        };
        if !a1 {
            return EvalResult::Value(Value::Error(ErrorValue::Ref));
        }

        match parse_local_reference_text(&ref_text) {
            Ok(LocalReference::Cell(cell_ref)) => EvalResult::Range(RangeRef {
                start: cell_ref,
                end: cell_ref,
            }),
            Ok(LocalReference::Range(range_ref)) => EvalResult::Range(range_ref),
            Err(_) => EvalResult::Value(Value::Error(ErrorValue::Ref)),
        }
    }
}

static INDIRECT: Indirect = Indirect;
inventory::submit! { FunctionEntry(&INDIRECT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval::EvalContext, model::Coord};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(INDIRECT.name(), "INDIRECT");
        assert_eq!(INDIRECT.arity(), (1, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDIRECT.call_result(&[], &fn_ctx),
            EvalResult::Value(Value::Error(ErrorValue::Value))
        );
        assert_eq!(
            INDIRECT.call_result(
                &[
                    Arg::Value(text("A1")),
                    Arg::Value(Value::Boolean(true)),
                    Arg::Value(Value::Boolean(true)),
                ],
                &fn_ctx,
            ),
            EvalResult::Value(Value::Error(ErrorValue::Value))
        );
    }

    #[test]
    fn returns_single_cell_reference_ranges() {
        assert_eq!(
            call_result(vec![text("$A$1")]),
            EvalResult::Range(RangeRef {
                start: CellRef {
                    sheet: None,
                    col: 0,
                    row: 0,
                    col_abs: true,
                    row_abs: true,
                },
                end: CellRef {
                    sheet: None,
                    col: 0,
                    row: 0,
                    col_abs: true,
                    row_abs: true,
                },
            })
        );
        assert_eq!(
            call_result(vec![text("Sheet1!B2")]),
            EvalResult::Range(RangeRef {
                start: CellRef {
                    sheet: Some(0),
                    col: 1,
                    row: 1,
                    col_abs: false,
                    row_abs: false,
                },
                end: CellRef {
                    sheet: Some(0),
                    col: 1,
                    row: 1,
                    col_abs: false,
                    row_abs: false,
                },
            })
        );
    }

    #[test]
    fn returns_range_references() {
        assert_eq!(
            call_result(vec![text("B2:D4")]),
            EvalResult::Range(RangeRef {
                start: rel_cell(1, 1),
                end: rel_cell(3, 3),
            })
        );
    }

    #[test]
    fn rejects_r1c1_mode_and_invalid_a1_text_as_ref() {
        for args in [
            vec![text("R1C1"), Value::Boolean(false)],
            vec![text("R1C1")],
            vec![text("Sheet2!A1")],
            vec![text("[Book1]Sheet1!A1")],
            vec![text("")],
            vec![Value::Number(123.0)],
        ] {
            assert_eq!(
                call_result(args),
                EvalResult::Value(Value::Error(ErrorValue::Ref))
            );
        }
    }

    #[test]
    fn propagates_ref_text_and_a1_coercion_errors() {
        assert_eq!(
            call_result(vec![Value::Error(ErrorValue::Div0)]),
            EvalResult::Value(Value::Error(ErrorValue::Div0))
        );
        assert_eq!(
            call_result(vec![text("A1"), Value::Error(ErrorValue::Na)]),
            EvalResult::Value(Value::Error(ErrorValue::Na))
        );
        assert_eq!(
            call_result(vec![text("A1"), text("not bool")]),
            EvalResult::Value(Value::Error(ErrorValue::Value))
        );
    }

    #[test]
    fn direct_call_returns_value_for_reference_results() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            INDIRECT.call(&[Arg::Value(text("A1"))], &fn_ctx),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            INDIRECT.call(&[Arg::Value(text("Sheet2!A1"))], &fn_ctx),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_result(values: Vec<Value>) -> EvalResult {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        INDIRECT.call_result(&args, &fn_ctx)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn rel_cell(row: u32, col: u32) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    struct TestContext;

    impl EvalContext for TestContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            panic!("INDIRECT unit tests should not materialize range values")
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
