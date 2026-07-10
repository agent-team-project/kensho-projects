use crate::functions::prelude::*;
use crate::model::date::serial_to_ymd;

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const MIN_SUPPORTED_SERIAL: f64 = 1.0;
const MAX_SUPPORTED_SERIAL: f64 = 2_958_465.0;

pub struct Yearfrac;

impl Function for Yearfrac {
    fn name(&self) -> &'static str {
        "YEARFRAC"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match yearfrac(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn yearfrac(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let start_serial = ctx.to_serial_date(&args[0].as_value())?;
    let end_serial = ctx.to_serial_date(&args[1].as_value())?;
    let basis_number = match args.get(2) {
        Some(arg) => ctx.to_number(&arg.as_value())?,
        None => 0.0,
    };

    let start = supported_date(start_serial.trunc(), ctx.date_system())?;
    let end = supported_date(end_serial.trunc(), ctx.date_system())?;
    let basis = supported_basis(basis_number)?;

    let (from, to, sign) = if start.serial_day <= end.serial_day {
        (start, end, 1.0)
    } else {
        (end, start, -1.0)
    };

    let fraction = match basis {
        0 => us_30_360(&from, &to),
        1 => actual_actual(&from, &to, ctx.date_system()),
        2 => (to.serial_day - from.serial_day) / 360.0,
        3 => (to.serial_day - from.serial_day) / 365.0,
        4 => european_30_360(&from, &to),
        _ => unreachable!("basis already validated"),
    };

    Ok(sign * fraction)
}

#[derive(Clone, Copy)]
struct DateParts {
    serial_day: f64,
    year: i32,
    month: u32,
    day: u32,
}

fn supported_date(serial_day: f64, date_system: DateSystem) -> Result<DateParts, ErrorValue> {
    if !serial_day.is_finite() || serial_day <= 0.0 {
        return Err(ErrorValue::Value);
    }
    if !(MIN_SUPPORTED_SERIAL..=MAX_SUPPORTED_SERIAL).contains(&serial_day) {
        return Err(ErrorValue::Value);
    }

    let (year, month, day) = serial_to_ymd(serial_day, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(ErrorValue::Value);
    }

    Ok(DateParts {
        serial_day,
        year,
        month,
        day,
    })
}

fn supported_basis(number: f64) -> Result<i32, ErrorValue> {
    if !number.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = number.trunc();
    if !(0.0..=4.0).contains(&truncated) {
        return Err(ErrorValue::Num);
    }

    Ok(truncated as i32)
}

fn us_30_360(start: &DateParts, end: &DateParts) -> f64 {
    let start_day = if start.day == 31 { 30 } else { start.day };
    let mut end_year = end.year;
    let mut end_month = end.month;
    let mut end_day = end.day;

    if end_day == 31 {
        if start_day == 30 {
            end_day = 30;
        } else {
            end_day = 1;
            end_month += 1;
            if end_month == 13 {
                end_month = 1;
                end_year += 1;
            }
        }
    }

    days_30_360(
        start.year,
        start.month,
        start_day,
        end_year,
        end_month,
        end_day,
    ) / 360.0
}

fn european_30_360(start: &DateParts, end: &DateParts) -> f64 {
    let start_day = if start.day == 31 { 30 } else { start.day };
    let end_day = if end.day == 31 { 30 } else { end.day };

    days_30_360(
        start.year,
        start.month,
        start_day,
        end.year,
        end.month,
        end_day,
    ) / 360.0
}

fn days_30_360(
    start_year: i32,
    start_month: u32,
    start_day: u32,
    end_year: i32,
    end_month: u32,
    end_day: u32,
) -> f64 {
    f64::from(
        360 * (end_year - start_year) + 30 * (end_month as i32 - start_month as i32)
            + end_day as i32
            - start_day as i32,
    )
}

fn actual_actual(start: &DateParts, end: &DateParts, date_system: DateSystem) -> f64 {
    if start.year == end.year {
        return (end.serial_day - start.serial_day) / days_in_calendar_year(start.year, date_system);
    }

    let first_year_days =
        days_in_calendar_year(start.year, date_system) - day_of_year(start, date_system) + 1.0;
    let mut fraction = first_year_days / days_in_calendar_year(start.year, date_system);

    for _ in (start.year + 1)..end.year {
        fraction += 1.0;
    }

    let final_year_days = day_of_year(end, date_system) - 1.0;
    fraction + final_year_days / days_in_calendar_year(end.year, date_system)
}

fn days_in_calendar_year(year: i32, date_system: DateSystem) -> f64 {
    match date_system {
        DateSystem::Excel1900 if year == 1900 => 366.0,
        DateSystem::Excel1900 if is_leap_year(year) => 366.0,
        DateSystem::Excel1900 => 365.0,
    }
}

fn day_of_year(date: &DateParts, date_system: DateSystem) -> f64 {
    const DAYS_BEFORE_MONTH_COMMON: [u32; 12] =
        [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

    let mut day = DAYS_BEFORE_MONTH_COMMON[(date.month - 1) as usize] + date.day;
    if is_leap_year(date.year) && date.month > 2 {
        day += 1;
    }
    if matches!(date_system, DateSystem::Excel1900) && date.year == 1900 && date.month > 2 {
        day += 1;
    }

    f64::from(day)
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

static YEARFRAC: Yearfrac = Yearfrac;
inventory::submit! { FunctionEntry(&YEARFRAC) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::model::date::ymd_to_serial;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;
    const TOLERANCE: f64 = 1e-12;

    #[test]
    fn reports_two_or_three_argument_arity() {
        assert_eq!(YEARFRAC.name(), "YEARFRAC");
        assert_eq!(YEARFRAC.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_yearfrac(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_yearfrac(vec![Value::Number(serial(2012, 1, 1))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_documented_examples() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
            ]),
            0.580_555_555_555_555_6,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(1.0),
            ]),
            0.576_502_732_240_437_1,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(3.0),
            ]),
            0.578_082_191_780_821_9,
        );
    }

    #[test]
    fn supports_actual_360_actual_365_and_european_30_360() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(2.0),
            ]),
            211.0 / 360.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(3.0),
            ]),
            211.0 / 365.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(4.0),
            ]),
            209.0 / 360.0,
        );
    }

    #[test]
    fn returns_zero_for_same_date_and_negates_reversed_intervals() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(serial(2020, 1, 31)),
                Value::Number(1.0),
            ]),
            0.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 7, 30)),
                Value::Number(serial(2012, 1, 1)),
                Value::Number(2.0),
            ]),
            -211.0 / 360.0,
        );
    }

    #[test]
    fn truncates_date_and_basis_arguments_toward_zero() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1) + 0.9),
                Value::Number(serial(2012, 7, 30) + 0.9),
                Value::Number(3.9),
            ]),
            211.0 / 365.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(-0.9),
            ]),
            209.0 / 360.0,
        );
    }

    #[test]
    fn returns_num_for_invalid_basis() {
        assert_eq!(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(5.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn returns_value_for_invalid_dates() {
        for invalid in [
            0.0,
            -1.0,
            f64::INFINITY,
            f64::NAN,
            MAX_SUPPORTED_SERIAL + 1.0,
        ] {
            assert_eq!(
                call_yearfrac(vec![
                    Value::Number(invalid),
                    Value::Number(serial(2012, 7, 30)),
                ]),
                Value::Error(ErrorValue::Value)
            );
        }
    }

    #[test]
    fn propagates_first_coercion_error_left_to_right() {
        assert_eq!(
            call_yearfrac(vec![
                Value::Text("2012-01-01".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_yearfrac(vec![
                Value::Error(ErrorValue::Div0),
                Value::Text("2012-07-30".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_yearfrac(vec![
                Value::Number(serial(2012, 1, 1)),
                Value::Number(serial(2012, 7, 30)),
                Value::Text("basis".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn splits_actual_actual_across_calendar_years() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2019, 7, 1)),
                Value::Number(serial(2020, 7, 1)),
                Value::Number(1.0),
            ]),
            184.0 / 365.0 + 182.0 / 366.0,
        );
    }

    #[test]
    fn applies_us_30_360_31st_day_rules_without_february_special_cases() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2021, 1, 30)),
                Value::Number(serial(2021, 1, 31)),
                Value::Number(0.0),
            ]),
            0.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2021, 1, 29)),
                Value::Number(serial(2021, 1, 31)),
                Value::Number(0.0),
            ]),
            2.0 / 360.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2021, 2, 28)),
                Value::Number(serial(2021, 3, 31)),
                Value::Number(0.0),
            ]),
            33.0 / 360.0,
        );
    }

    #[test]
    fn applies_european_30_360_31st_day_rules() {
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2021, 1, 31)),
                Value::Number(serial(2021, 2, 28)),
                Value::Number(4.0),
            ]),
            28.0 / 360.0,
        );
        assert_number_close(
            call_yearfrac(vec![
                Value::Number(serial(2021, 1, 31)),
                Value::Number(serial(2021, 3, 31)),
                Value::Number(4.0),
            ]),
            60.0 / 360.0,
        );
    }

    #[test]
    fn uses_top_left_range_values_for_scalar_arguments() {
        let ctx = RangeContext;
        let fn_ctx = FnContext::new(&ctx);
        let start_range = range(0, 0, 0, 1);
        let end_range = range(1, 0, 1, 1);
        let basis_range = range(2, 0, 2, 1);

        assert_number_close(
            YEARFRAC.call(
                &[
                    Arg::Range(ctx.range_view(start_range)),
                    Arg::Range(ctx.range_view(end_range)),
                    Arg::Range(ctx.range_view(basis_range)),
                ],
                &fn_ctx,
            ),
            211.0 / 365.0,
        );
    }

    fn call_yearfrac(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        YEARFRAC.call(&args, &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= TOLERANCE,
                "expected {expected}, got {actual}"
            ),
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        ymd_to_serial(year, month, day, SYS)
    }

    fn range(start_row: u32, start_col: u32, end_row: u32, end_col: u32) -> RangeRef {
        RangeRef {
            start: CellRef {
                sheet: None,
                col: start_col,
                row: start_row,
                col_abs: false,
                row_abs: false,
            },
            end: CellRef {
                sheet: None,
                col: end_col,
                row: end_row,
                col_abs: false,
                row_abs: false,
            },
        }
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
                (0, 0) => Value::Number(serial(2012, 1, 1)),
                (1, 0) => Value::Number(serial(2012, 7, 30)),
                (2, 0) => Value::Number(3.9),
                _ => Value::Number(0.0),
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
