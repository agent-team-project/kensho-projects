use crate::functions::prelude::*;
use crate::model::date::{serial_to_ymd, ymd_to_serial};

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;

pub struct Eomonth;

impl Function for Eomonth {
    fn name(&self) -> &'static str {
        "EOMONTH"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        match eomonth_serial(args, ctx) {
            Ok(serial) => Value::Number(serial),
            Err(error) => Value::Error(error),
        }
    }
}

fn eomonth_serial(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let start_serial = ctx.to_serial_date(&args[0].as_value())?;
    let months = ctx.to_number(&args[1].as_value())?;

    let date_system = ctx.date_system();
    let start_day = supported_serial_day(start_serial, date_system)?;
    let month_offset = month_offset(months)?;
    let (year, month, _) = serial_to_ymd(start_day, date_system);
    let target_month = month_index(year, month)?
        .checked_add(month_offset)
        .ok_or(ErrorValue::Num)?;
    let serial = end_of_month_serial(target_month, date_system)?;

    validate_supported_serial(serial, date_system)
}

fn supported_serial_day(serial: f64, date_system: DateSystem) -> Result<f64, ErrorValue> {
    if !serial.is_finite() {
        return Err(ErrorValue::Num);
    }

    validate_supported_serial(serial.floor(), date_system)
}

fn month_offset(months: f64) -> Result<i64, ErrorValue> {
    if !months.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = months.trunc();
    if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
        return Err(ErrorValue::Num);
    }

    Ok(truncated as i64)
}

fn month_index(year: i32, month: u32) -> Result<i64, ErrorValue> {
    i64::from(year)
        .checked_mul(12)
        .and_then(|base| base.checked_add(i64::from(month) - 1))
        .ok_or(ErrorValue::Num)
}

fn end_of_month_serial(month_index: i64, date_system: DateSystem) -> Result<f64, ErrorValue> {
    let next_month = month_index.checked_add(1).ok_or(ErrorValue::Num)?;
    let (year, month) = year_month(next_month)?;
    Ok(ymd_to_serial(year, month, 1, date_system) - 1.0)
}

fn year_month(month_index: i64) -> Result<(i32, u32), ErrorValue> {
    let year = month_index.div_euclid(12);
    if year < i64::from(i32::MIN) || year > i64::from(i32::MAX) {
        return Err(ErrorValue::Num);
    }

    Ok((year as i32, (month_index.rem_euclid(12) + 1) as u32))
}

fn validate_supported_serial(serial: f64, date_system: DateSystem) -> Result<f64, ErrorValue> {
    let min_serial = ymd_to_serial(MIN_SUPPORTED_YEAR, 1, 1, date_system);
    let max_serial = ymd_to_serial(MAX_SUPPORTED_YEAR, 12, 31, date_system);
    if !serial.is_finite() || serial < min_serial || serial > max_serial {
        return Err(ErrorValue::Num);
    }

    let (year, _, _) = serial_to_ymd(serial, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(ErrorValue::Num);
    }

    Ok(serial)
}

static EOMONTH: Eomonth = Eomonth;
inventory::submit! { FunctionEntry(&EOMONTH) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_exact_two_argument_arity() {
        assert_eq!(EOMONTH.name(), "EOMONTH");
        assert_eq!(EOMONTH.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_eomonth(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_last_day_for_same_next_and_previous_months() {
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15)), Value::Number(0.0)]),
            Value::Number(serial(2020, 1, 31))
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15)), Value::Number(1.0)]),
            Value::Number(serial(2020, 2, 29))
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15)), Value::Number(-1.0)]),
            Value::Number(serial(2019, 12, 31))
        );
    }

    #[test]
    fn truncates_fractional_months_toward_zero() {
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15)), Value::Number(1.9)]),
            Value::Number(serial(2020, 2, 29))
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 3, 15)), Value::Number(-1.9)]),
            Value::Number(serial(2020, 2, 29))
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15)), Value::Number(-0.9)]),
            Value::Number(serial(2020, 1, 31))
        );
    }

    #[test]
    fn uses_date_portion_and_preserves_excel_1900_phantom_day() {
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15) + 0.75),
                Value::Number(0.0),
            ]),
            Value::Number(serial(2020, 1, 31))
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(60.0), Value::Number(0.0)]),
            Value::Number(60.0)
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(59.0), Value::Number(0.0)]),
            Value::Number(60.0)
        );
    }

    #[test]
    fn uses_scalar_top_left_values_and_number_coercion() {
        assert_eq!(
            call_eomonth(vec![
                Value::Text(" 43845 ".to_string()),
                Value::Text("1.9".to_string()),
            ]),
            Value::Number(serial(2020, 2, 29))
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(2020, 1, 15)), Value::Boolean(true)]),
            Value::Number(serial(2020, 2, 29))
        );

        let ctx = RangeContext;
        let fn_ctx = FnContext::new(&ctx);
        let start_range = RangeRef {
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
                row: 0,
                col_abs: false,
                row_abs: false,
            },
        };
        let months_range = RangeRef {
            start: CellRef {
                sheet: None,
                col: 2,
                row: 0,
                col_abs: false,
                row_abs: false,
            },
            end: CellRef {
                sheet: None,
                col: 3,
                row: 0,
                col_abs: false,
                row_abs: false,
            },
        };

        assert_eq!(
            EOMONTH.call(
                &[
                    Arg::Range(ctx.range_view(start_range)),
                    Arg::Range(ctx.range_view(months_range)),
                ],
                &fn_ctx,
            ),
            Value::Number(serial(2020, 2, 29))
        );
    }

    #[test]
    fn rejects_invalid_start_dates() {
        for value in [
            Value::Number(0.0),
            Value::Number(0.9),
            Value::Number(-1.0),
            Value::Number(max_supported_serial() + 1.0),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NAN),
        ] {
            assert_eq!(
                call_eomonth(vec![value, Value::Number(0.0)]),
                Value::Error(ErrorValue::Num)
            );
        }
    }

    #[test]
    fn rejects_non_finite_months_and_out_of_range_results() {
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Number(1.0e20),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(9999, 12, 31)), Value::Number(1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_eomonth(vec![Value::Number(serial(1900, 1, 1)), Value::Number(-1.0)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_eomonth(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Error(ErrorValue::Ref),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_eomonth(vec![
                Value::Text("2020-01-15".to_string()),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_eomonth(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        EOMONTH.call(&args, &fn_ctx)
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        ymd_to_serial(year, month, day, SYS)
    }

    fn max_supported_serial() -> f64 {
        serial(MAX_SUPPORTED_YEAR, 12, 31)
    }

    struct DummyContext;

    impl EvalContext for DummyContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, _name: &str) -> Option<&dyn Function> {
            None
        }

        fn date_system(&self) -> DateSystem {
            SYS
        }

        fn current_cell(&self) -> CellId {
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }

    struct RangeContext;

    impl EvalContext for RangeContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Number(serial(2020, 1, 15) + 0.75),
                (0, 2) => Value::Text("1.9".to_string()),
                _ => Value::Number(1.0),
            }
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, _name: &str) -> Option<&dyn Function> {
            None
        }

        fn date_system(&self) -> DateSystem {
            SYS
        }

        fn current_cell(&self) -> CellId {
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
