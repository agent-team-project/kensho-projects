use crate::functions::prelude::*;
use crate::model::date::{serial_to_ymd, ymd_to_serial};

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;

pub struct Edate;

impl Function for Edate {
    fn name(&self) -> &'static str {
        "EDATE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        match edate_serial(args, ctx) {
            Ok(serial) => Value::Number(serial),
            Err(error) => Value::Error(error),
        }
    }
}

fn edate_serial(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let start_serial = ctx.to_serial_date(&args[0].as_value())?;
    let months = ctx.to_number(&args[1].as_value())?;

    let start_day = supported_serial_day(start_serial, ctx.date_system())?;
    let month_offset = month_offset(months)?;
    let (year, month, day) = serial_to_ymd(start_day, ctx.date_system());
    let (target_year, target_month) = normalize_year_month(year, month, month_offset)?;
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&target_year) {
        return Err(ErrorValue::Num);
    }

    let target_day = day.min(days_in_month(target_year, target_month, ctx.date_system()));
    let serial = ymd_to_serial(target_year, target_month, target_day, ctx.date_system());
    validate_supported_serial(serial, ctx.date_system())
}

fn supported_serial_day(serial: f64, date_system: DateSystem) -> Result<f64, ErrorValue> {
    if !serial.is_finite() || serial <= 0.0 {
        return Err(ErrorValue::Num);
    }

    validate_supported_serial(serial.floor(), date_system)
}

fn month_offset(months: f64) -> Result<i32, ErrorValue> {
    if !months.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = months.trunc();
    if truncated < i32::MIN as f64 || truncated > i32::MAX as f64 {
        return Err(ErrorValue::Num);
    }

    Ok(truncated as i32)
}

fn normalize_year_month(year: i32, month: u32, offset: i32) -> Result<(i32, u32), ErrorValue> {
    let months = i64::from(year)
        .checked_mul(12)
        .and_then(|base| base.checked_add(i64::from(month) - 1))
        .and_then(|base| base.checked_add(i64::from(offset)))
        .ok_or(ErrorValue::Num)?;
    let normalized_year = months.div_euclid(12);
    let normalized_month = months.rem_euclid(12) + 1;

    if normalized_year < i64::from(i32::MIN) || normalized_year > i64::from(i32::MAX) {
        return Err(ErrorValue::Num);
    }

    Ok((normalized_year as i32, normalized_month as u32))
}

fn days_in_month(year: i32, month: u32, date_system: DateSystem) -> u32 {
    match (date_system, year, month) {
        (DateSystem::Excel1900, 1900, 2) => 29,
        (_, _, 1 | 3 | 5 | 7 | 8 | 10 | 12) => 31,
        (_, _, 4 | 6 | 9 | 11) => 30,
        (_, _, 2) if is_leap_year(year) => 29,
        (_, _, 2) => 28,
        _ => unreachable!("normalized month is always 1 through 12"),
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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

static EDATE: Edate = Edate;
inventory::submit! { FunctionEntry(&EDATE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_exact_two_argument_arity() {
        assert_eq!(EDATE.name(), "EDATE");
        assert_eq!(EDATE.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_edate(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_edate(vec![Value::Number(serial(2020, 1, 1))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn adds_months_while_preserving_the_day_when_possible() {
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Number(1.0),
            ]),
            Value::Number(serial(2020, 2, 15))
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 12, 15)),
                Value::Number(2.0),
            ]),
            Value::Number(serial(2021, 2, 15))
        );
    }

    #[test]
    fn clamps_to_the_target_month_end() {
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(1.0),
            ]),
            Value::Number(serial(2020, 2, 29))
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2021, 1, 31)),
                Value::Number(1.0),
            ]),
            Value::Number(serial(2021, 2, 28))
        );
        assert_eq!(
            call_edate(vec![Value::Number(serial(1900, 1, 31)), Value::Number(1.0)]),
            Value::Number(serial(1900, 2, 29))
        );
    }

    #[test]
    fn supports_negative_month_offsets() {
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(-1.0),
            ]),
            Value::Number(serial(2019, 12, 31))
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 3, 31)),
                Value::Number(-1.0),
            ]),
            Value::Number(serial(2020, 2, 29))
        );
    }

    #[test]
    fn truncates_fractional_months_toward_zero() {
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(1.9),
            ]),
            Value::Number(serial(2020, 2, 29))
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(-1.9),
            ]),
            Value::Number(serial(2019, 12, 31))
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(-0.9),
            ]),
            Value::Number(serial(2020, 1, 31))
        );
    }

    #[test]
    fn uses_the_date_portion_of_positive_fractional_start_dates() {
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 15) + 0.75),
                Value::Number(1.0),
            ]),
            Value::Number(serial(2020, 2, 15))
        );
    }

    #[test]
    fn uses_scalar_top_left_values_and_numeric_coercion() {
        assert_eq!(
            call_edate(vec![
                Value::Text(" 43831 ".to_string()),
                Value::Text("1".to_string()),
            ]),
            Value::Number(serial(2020, 2, 1))
        );
        assert_eq!(
            call_edate(vec![Value::Number(serial(2020, 1, 1)), Value::Boolean(true)]),
            Value::Number(serial(2020, 2, 1))
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
            EDATE.call(
                &[
                    Arg::Range(ctx.range_view(start_range)),
                    Arg::Range(ctx.range_view(months_range)),
                ],
                &fn_ctx,
            ),
            Value::Number(serial(2020, 2, 1))
        );
    }

    #[test]
    fn rejects_invalid_start_dates_and_out_of_range_results() {
        assert_eq!(
            call_edate(vec![Value::Number(0.0), Value::Number(1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![Value::Number(0.9), Value::Number(1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![Value::Number(-1.0), Value::Number(1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![Value::Number(f64::INFINITY), Value::Number(1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(9999, 12, 31)),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![Value::Number(serial(1900, 1, 1)), Value::Number(-1.0)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_or_unrepresentable_months() {
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(f64::NAN),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_edate(vec![Value::Number(serial(2020, 1, 1)), Value::Number(1E20)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_edate(vec![
                Value::Text("2020-01-01".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Error(ErrorValue::Ref),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_edate(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_edate(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        EDATE.call(&args, &fn_ctx)
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        ymd_to_serial(year, month, day, SYS)
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
                (0, 0) => Value::Number(serial(2020, 1, 1)),
                (0, 2) => Value::Number(1.0),
                _ => Value::Number(serial(1999, 12, 31)),
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
