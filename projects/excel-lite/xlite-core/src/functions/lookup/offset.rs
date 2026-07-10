use crate::functions::prelude::*;
use crate::model::{MAX_COLS, MAX_ROWS};

const MIN_ARGS: usize = 3;
const MAX_ARGS: usize = 5;

pub struct Offset;

impl Function for Offset {
    fn name(&self) -> &'static str {
        "OFFSET"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn argument_mode(&self, index: usize) -> FunctionArgMode {
        if index == 0 {
            FunctionArgMode::Reference
        } else {
            FunctionArgMode::Value
        }
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        match self.call_result(args, ctx) {
            EvalResult::Value(value) => value,
            EvalResult::Range(_) => Value::Error(ErrorValue::Value),
        }
    }

    fn call_result(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> EvalResult {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return value_result(ErrorValue::Value);
        }

        let reference = match &args[0] {
            Arg::Range(range) => range,
            Arg::Value(Value::Error(error)) => {
                return EvalResult::Value(Value::Error(*error));
            }
            Arg::Value(_) => return value_result(ErrorValue::Value),
        };

        let rows = match coerce_offset(&args[1], ctx) {
            Ok(rows) => rows,
            Err(error) => return value_result(error),
        };
        let cols = match coerce_offset(&args[2], ctx) {
            Ok(cols) => cols,
            Err(error) => return value_result(error),
        };
        let height = match args.get(3) {
            Some(arg) => match coerce_dimension(arg, ctx) {
                Ok(height) => height,
                Err(error) => return value_result(error),
            },
            None => f64::from(reference.rows()),
        };
        let width = match args.get(4) {
            Some(arg) => match coerce_dimension(arg, ctx) {
                Ok(width) => width,
                Err(error) => return value_result(error),
            },
            None => f64::from(reference.cols()),
        };

        match offset_range(reference, rows, cols, height, width) {
            Ok(range) => EvalResult::Range(range),
            Err(error) => value_result(error),
        }
    }
}

fn value_result(error: ErrorValue) -> EvalResult {
    EvalResult::Value(Value::Error(error))
}

fn coerce_offset(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let offset = ctx.to_number(&arg.as_value())?.trunc();
    if !offset.is_finite() {
        return Err(ErrorValue::Value);
    }

    Ok(offset)
}

fn coerce_dimension(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let dimension = coerce_offset(arg, ctx)?;
    if dimension <= 0.0 {
        return Err(ErrorValue::Ref);
    }

    Ok(dimension)
}

fn offset_range(
    reference: &RangeView<'_>,
    rows: f64,
    cols: f64,
    height: f64,
    width: f64,
) -> Result<RangeRef, ErrorValue> {
    let (top_left, _) = reference.normalized_bounds();
    let top = f64::from(top_left.row) + rows;
    let left = f64::from(top_left.col) + cols;
    let bottom = top + height - 1.0;
    let right = left + width - 1.0;

    if !target_edge_is_in_bounds(top, MAX_ROWS)
        || !target_edge_is_in_bounds(bottom, MAX_ROWS)
        || !target_edge_is_in_bounds(left, MAX_COLS)
        || !target_edge_is_in_bounds(right, MAX_COLS)
    {
        return Err(ErrorValue::Ref);
    }

    let sheet = reference_sheet(reference.range_ref());
    Ok(RangeRef {
        start: cell_ref(sheet, top as u32, left as u32),
        end: cell_ref(sheet, bottom as u32, right as u32),
    })
}

fn target_edge_is_in_bounds(edge: f64, max: u32) -> bool {
    edge.is_finite() && edge >= 0.0 && edge < f64::from(max)
}

fn reference_sheet(reference: RangeRef) -> Option<u16> {
    reference.start.sheet.or(reference.end.sheet)
}

fn cell_ref(sheet: Option<u16>, row: u32, col: u32) -> CellRef {
    CellRef {
        sheet,
        col,
        row,
        col_abs: false,
        row_abs: false,
    }
}

static OFFSET: Offset = Offset;
inventory::submit! { FunctionEntry(&OFFSET) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::EvalContext,
        model::{CellId, Coord},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(OFFSET.name(), "OFFSET");
        assert_eq!(OFFSET.arity(), (3, Some(5)));
        assert_eq!(OFFSET.argument_mode(0), FunctionArgMode::Reference);
        assert_eq!(OFFSET.argument_mode(1), FunctionArgMode::Value);
    }

    #[test]
    fn call_result_returns_default_shape_from_normalized_reference() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (2, 2), (1, 1)),
                    Arg::Value(num(1.0)),
                    Arg::Value(num(2.0)),
                ],
                &fn_ctx,
            ),
            EvalResult::Range(range_ref((2, 3), (3, 4)))
        );
    }

    #[test]
    fn explicit_offsets_and_dimensions_truncate_toward_zero() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(num(1.9)),
                    Arg::Value(num(1.1)),
                    Arg::Value(num(2.9)),
                    Arg::Value(num(3.1)),
                ],
                &fn_ctx,
            ),
            EvalResult::Range(range_ref((1, 1), (2, 3)))
        );
        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (2, 2), (2, 2)),
                    Arg::Value(num(-1.9)),
                    Arg::Value(num(-1.1)),
                ],
                &fn_ctx,
            ),
            EvalResult::Range(range_ref((1, 1), (1, 1)))
        );
    }

    #[test]
    fn rejects_non_reference_first_argument_and_propagates_direct_error() {
        assert_eq!(
            call_result_values(vec![num(1.0), num(0.0), num(0.0)]),
            EvalResult::Value(Value::Error(ErrorValue::Value))
        );
        assert_eq!(
            call_result_values(vec![
                Value::Error(ErrorValue::Div0),
                num(0.0),
                num(0.0),
            ]),
            EvalResult::Value(Value::Error(ErrorValue::Div0))
        );
    }

    #[test]
    fn propagates_scalar_coercion_errors() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(text("row")),
                    Arg::Value(num(0.0)),
                ],
                &fn_ctx,
            ),
            EvalResult::Value(Value::Error(ErrorValue::Value))
        );
        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(Value::Error(ErrorValue::Ref)),
                    Arg::Value(num(0.0)),
                ],
                &fn_ctx,
            ),
            EvalResult::Value(Value::Error(ErrorValue::Ref))
        );
    }

    #[test]
    fn validates_dimensions_and_grid_edges() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        for height in [0.0, -1.0, -0.9] {
            assert_eq!(
                OFFSET.call_result(
                    &[
                        range_arg(&ctx, (0, 0), (0, 0)),
                        Arg::Value(num(0.0)),
                        Arg::Value(num(0.0)),
                        Arg::Value(num(height)),
                    ],
                    &fn_ctx,
                ),
                EvalResult::Value(Value::Error(ErrorValue::Ref)),
                "height {height}"
            );
        }
        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(num(-1.0)),
                    Arg::Value(num(0.0)),
                ],
                &fn_ctx,
            ),
            EvalResult::Value(Value::Error(ErrorValue::Ref))
        );
        assert_eq!(
            OFFSET.call_result(
                &[
                    range_arg(&ctx, (MAX_ROWS - 1, MAX_COLS - 1), (MAX_ROWS - 1, MAX_COLS - 1)),
                    Arg::Value(num(0.0)),
                    Arg::Value(num(0.0)),
                    Arg::Value(num(2.0)),
                    Arg::Value(num(1.0)),
                ],
                &fn_ctx,
            ),
            EvalResult::Value(Value::Error(ErrorValue::Ref))
        );
    }

    #[test]
    fn non_finite_offsets_and_dimensions_return_value_error() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        for args in [
            [
                Arg::Value(num(f64::INFINITY)),
                Arg::Value(num(0.0)),
                Arg::Value(num(1.0)),
            ],
            [
                Arg::Value(num(0.0)),
                Arg::Value(num(f64::NEG_INFINITY)),
                Arg::Value(num(1.0)),
            ],
            [
                Arg::Value(num(0.0)),
                Arg::Value(num(0.0)),
                Arg::Value(num(f64::NAN)),
            ],
        ] {
            assert_eq!(
                OFFSET.call_result(
                    &[
                        range_arg(&ctx, (0, 0), (0, 0)),
                        args[0].clone(),
                        args[1].clone(),
                        args[2].clone(),
                    ],
                    &fn_ctx,
                ),
                EvalResult::Value(Value::Error(ErrorValue::Value))
            );
        }
    }

    #[test]
    fn direct_call_returns_value_error_for_reference_result() {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            OFFSET.call(
                &[
                    range_arg(&ctx, (0, 0), (0, 0)),
                    Arg::Value(num(0.0)),
                    Arg::Value(num(0.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_result_values(values: Vec<Value>) -> EvalResult {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        OFFSET.call_result(&args, &fn_ctx)
    }

    fn range_arg(ctx: &TestContext, start: (u32, u32), end: (u32, u32)) -> Arg<'_> {
        Arg::Range(RangeView::new(ctx, range_ref(start, end)))
    }

    fn range_ref(start: (u32, u32), end: (u32, u32)) -> RangeRef {
        RangeRef {
            start: cell_ref(None, start.0, start.1),
            end: cell_ref(None, end.0, end.1),
        }
    }

    fn num(value: f64) -> Value {
        Value::Number(value)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    struct TestContext;

    impl EvalContext for TestContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            panic!("OFFSET should not materialize its reference argument")
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
