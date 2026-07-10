use crate::functions::prelude::*;

pub struct Ddb;

impl Function for Ddb {
    fn name(&self) -> &'static str {
        "DDB"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (4, Some(5))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(4..=5).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match ddb(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn ddb(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let cost = finite_number(&args[0], ctx)?;
    let salvage = finite_number(&args[1], ctx)?;
    let life = finite_number(&args[2], ctx)?;
    let period = finite_number(&args[3], ctx)?.trunc();
    let factor = match args.get(4) {
        Some(arg) => finite_number(arg, ctx)?,
        None => 2.0,
    };

    if cost <= 0.0
        || salvage < 0.0
        || salvage > cost
        || life <= 0.0
        || period < 1.0
        || period > life
        || factor <= 0.0
    {
        return Err(ErrorValue::Num);
    }

    let rate = finite_result(factor / life)?;
    let depreciable_total = finite_result(cost - salvage)?;

    if rate == 0.0 {
        return Ok(0.0);
    }

    let depreciation = if rate >= 1.0 {
        if period == 1.0 {
            depreciable_total
        } else {
            0.0
        }
    } else {
        let decay = finite_result((1.0 - rate).powf(period - 1.0))?;
        let book_before = finite_result(cost * decay)?;
        let total_before = finite_result(cost - book_before)?;
        let declining = finite_result(book_before * rate)?;
        let remaining = finite_result(book_before - salvage)?;

        if total_before >= depreciable_total {
            0.0
        } else {
            finite_result(declining.min(remaining).max(0.0))?
        }
    };

    finite_result(depreciation)
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

static DDB: Ddb = Ddb;
inventory::submit! { FunctionEntry(&DDB) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(DDB.name(), "DDB");
        assert_eq!(DDB.arity(), (4, Some(5)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_examples() {
        assert_number_close(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0 * 365.0),
                Value::Number(1.0),
            ]),
            1.3150684931506849,
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0 * 12.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ]),
            Value::Number(40.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(1.0),
                Value::Number(2.0),
            ]),
            Value::Number(480.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(2.0),
                Value::Number(1.5),
            ]),
            Value::Number(306.0)
        );
        assert_number_close(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(10.0),
            ]),
            22.1225472000001,
        );
    }

    #[test]
    fn defaults_factor_to_two() {
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(2.0),
            ]),
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(2.0),
                Value::Number(2.0),
            ])
        );
    }

    #[test]
    fn truncates_period_toward_zero_before_domain_checks() {
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(2.9),
            ]),
            Value::Number(384.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(0.9),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_numeric_domains() {
        for values in [
            vec![
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(10.0),
                Value::Number(1.0),
            ],
            vec![
                Value::Number(2400.0),
                Value::Number(-1.0),
                Value::Number(10.0),
                Value::Number(1.0),
            ],
            vec![
                Value::Number(2400.0),
                Value::Number(3000.0),
                Value::Number(10.0),
                Value::Number(1.0),
            ],
            vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ],
            vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(11.0),
            ],
            vec![
                Value::Number(2400.0),
                Value::Number(300.0),
                Value::Number(10.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ],
        ] {
            assert_eq!(call_values(values), Value::Error(ErrorValue::Num));
        }
    }

    #[test]
    fn clamps_depreciation_after_salvage_limit_is_reached() {
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(4.0),
                Value::Number(2.0),
                Value::Number(10.0),
            ]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn rejects_non_finite_arguments() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn handles_large_finite_period_without_iteration() {
        assert_number_close(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(1_000_000_000_000.0),
                Value::Number(1_000_000_000_000.0),
            ]),
            2.70682542135179e-10,
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_eq!(
            call_values(vec![
                Value::Text("2400".to_string()),
                Value::Boolean(false),
                Value::Text("10".to_string()),
                Value::Text("1".to_string()),
            ]),
            Value::Number(480.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(10.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(2400.0),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
                Value::Number(1.0),
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
            DDB.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(300.0)),
                    Arg::Value(Value::Number(10.0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(480.0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DDB.call(&args, &fn_ctx)
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
                (0, 0) => Value::Number(2400.0),
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
