use crate::functions::prelude::*;

pub struct Sln;

impl Function for Sln {
    fn name(&self) -> &'static str {
        "SLN"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 3 {
            return Value::Error(ErrorValue::Value);
        }

        match sln(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn sln(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let cost = finite_number(&args[0], ctx)?;
    let salvage = finite_number(&args[1], ctx)?;
    let life = finite_number(&args[2], ctx)?;

    if life == 0.0 {
        return Err(ErrorValue::Div0);
    }

    finite_result((cost - salvage) / life)
}

fn finite_number(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

fn finite_result(number: f64) -> Result<f64, ErrorValue> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

static SLN: Sln = Sln;
inventory::submit! { FunctionEntry(&SLN) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(SLN.name(), "SLN");
        assert_eq!(SLN.arity(), (3, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_values(vec![Value::Number(30000.0), Value::Number(7500.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(30000.0),
                Value::Number(7500.0),
                Value::Number(10.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_example() {
        assert_eq!(
            call_values(vec![
                Value::Number(30000.0),
                Value::Number(7500.0),
                Value::Number(10.0),
            ]),
            Value::Number(2250.0)
        );
    }

    #[test]
    fn computes_fractional_and_zero_salvage_cases() {
        assert_number_close(
            call_values(vec![
                Value::Number(1000.5),
                Value::Number(250.0),
                Value::Number(7.0),
            ]),
            107.21428571428571,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(600.0),
                Value::Number(0.0),
                Value::Number(5.0),
            ]),
            Value::Number(120.0)
        );
    }

    #[test]
    fn allows_negative_depreciation_when_salvage_exceeds_cost() {
        assert_eq!(
            call_values(vec![
                Value::Number(500.0),
                Value::Number(800.0),
                Value::Number(3.0),
            ]),
            Value::Number(-100.0)
        );
    }

    #[test]
    fn returns_div0_for_zero_life() {
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn rejects_non_finite_arguments_and_results() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(f64::MAX),
                Value::Number(-f64::MAX),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_eq!(
            call_values(vec![
                Value::Text("30000".to_string()),
                Value::Boolean(true),
                Value::Text("10".to_string()),
            ]),
            Value::Number(2999.9)
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(10.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(30000.0),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );

        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let range = RangeRef {
            start: CellRef {
                sheet: None,
                col: 0,
                row: 0,
                col_abs: false,
                row_abs: false,
            },
            end: CellRef {
                sheet: None,
                col: 1,
                row: 1,
                col_abs: false,
                row_abs: false,
            },
        };

        assert_eq!(
            SLN.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(7500.0)),
                    Arg::Value(Value::Number(10.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(2250.0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        SLN.call(&args, &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
                "expected {expected}, got {actual}"
            ),
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    struct GridContext;

    impl EvalContext for GridContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Number(30000.0),
                (0, 1) => Value::Error(ErrorValue::Div0),
                (1, 0) => Value::Text("not numeric".to_string()),
                _ => Value::Blank,
            }
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
