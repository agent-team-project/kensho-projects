use crate::functions::prelude::*;

pub struct Db;

impl Function for Db {
    fn name(&self) -> &'static str {
        "DB"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (4, Some(5))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(4..=5).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match db(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn db(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let cost = number_arg(&args[0], ctx)?;
    let salvage = number_arg(&args[1], ctx)?;
    let life = number_arg(&args[2], ctx)?;
    let period = number_arg(&args[3], ctx)?.trunc();
    let month = match args.get(4) {
        Some(arg) => number_arg(arg, ctx)?.trunc(),
        None => 12.0,
    };

    if cost <= 0.0
        || salvage < 0.0
        || salvage > cost
        || life <= 0.0
        || period < 1.0
        || month < 1.0
        || month > 12.0
    {
        return Err(ErrorValue::Num);
    }

    let life_periods = life.trunc();
    let max_period = if month == 12.0 {
        life_periods
    } else {
        life_periods + 1.0
    };
    if period > max_period {
        return Err(ErrorValue::Num);
    }

    let rate = fixed_declining_balance_rate(cost, salvage, life)?;
    let first_period_depreciation = finite_result(cost * rate * month / 12.0)?;
    let basis_after_first = finite_result(cost - first_period_depreciation)?;

    let depreciation = if period == 1.0 {
        first_period_depreciation
    } else if month < 12.0 && period == life_periods + 1.0 {
        let factor = finite_result((1.0 - rate).powf(life_periods - 1.0))?;
        finite_result(basis_after_first * factor * rate * (12.0 - month) / 12.0)?
    } else {
        let factor = finite_result((1.0 - rate).powf(period - 2.0))?;
        finite_result(basis_after_first * factor * rate)?
    };

    finite_result(depreciation)
}

fn number_arg(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

fn fixed_declining_balance_rate(cost: f64, salvage: f64, life: f64) -> Result<f64, ErrorValue> {
    let rate = finite_result(1.0 - (salvage / cost).powf(1.0 / life))?;
    finite_result((rate * 1000.0).round() / 1000.0)
}

fn finite_result(number: f64) -> Result<f64, ErrorValue> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

static DB: Db = Db;
inventory::submit! { FunctionEntry(&DB) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(DB.name(), "DB");
        assert_eq!(DB.arity(), (4, Some(5)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
                Value::Number(1.0),
                Value::Number(12.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_examples() {
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(1.0),
                Value::Number(7.0),
            ]),
            186083.33333333334,
        );
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(2.0),
                Value::Number(7.0),
            ]),
            259639.41666666666,
        );
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(3.0),
                Value::Number(7.0),
            ]),
            176814.44275,
        );
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(4.0),
                Value::Number(7.0),
            ]),
            120410.63551274998,
        );
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(5.0),
                Value::Number(7.0),
            ]),
            81999.64278418274,
        );
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(6.0),
                Value::Number(7.0),
            ]),
            55841.75673602846,
        );
        assert_close(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(7.0),
                Value::Number(7.0),
            ]),
            15845.098473848071,
        );
    }

    #[test]
    fn defaults_month_to_twelve() {
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
                Value::Number(1.0),
            ]),
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
                Value::Number(1.0),
                Value::Number(12.0),
            ])
        );
    }

    #[test]
    fn truncates_period_and_month_toward_zero() {
        assert_eq!(
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(2.9),
                Value::Number(7.9),
            ]),
            call_values(vec![
                Value::Number(1_000_000.0),
                Value::Number(100_000.0),
                Value::Number(6.0),
                Value::Number(2.0),
                Value::Number(7.0),
            ])
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
                Value::Number(-0.9),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_numeric_domains() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(5.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(-1.0),
                Value::Number(5.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(1001.0),
                Value::Number(5.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
                Value::Number(1.0),
                Value::Number(13.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(100.0),
                Value::Number(5.0),
                Value::Number(6.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_arguments_and_computed_values() {
        assert_eq!(
            call_values(vec![
                Value::Number(f64::INFINITY),
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(f64::MAX),
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(11.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn computes_large_period_without_looping() {
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
                Value::Number(0.0),
                Value::Number(1_000_000_000_000.0),
                Value::Number(1_000_000_000_000.0),
            ]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_close(
            call_values(vec![
                Value::Text("1000".to_string()),
                Value::Text("100".to_string()),
                Value::Text("5".to_string()),
                Value::Text("1".to_string()),
            ]),
            369.0,
        );
        assert_eq!(
            call_values(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(5.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1000.0),
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

        assert_close(
            DB.call(
                &[
                    Arg::Range(RangeView::new(&ctx, range)),
                    Arg::Value(Value::Number(100.0)),
                    Arg::Value(Value::Number(5.0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            369.0,
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DB.call(&args, &fn_ctx)
    }

    fn assert_close(actual: Value, expected: f64) {
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
                (0, 0) => Value::Number(1000.0),
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
