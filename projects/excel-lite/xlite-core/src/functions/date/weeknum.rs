use crate::functions::prelude::*;
use crate::model::date::serial_to_ymd;

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const MAX_EXCEL_1900_SERIAL: f64 = 2_958_465.0;

pub struct Weeknum;

impl Function for Weeknum {
    fn name(&self) -> &'static str {
        "WEEKNUM"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(1..=2).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match weeknum(args, ctx) {
            Ok(week) => Value::Number(week as f64),
            Err(error) => Value::Error(error),
        }
    }
}

fn weeknum(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let serial = ctx.to_serial_date(&args[0].as_value())?;
    let return_type = match args.get(1) {
        Some(arg) => ctx.to_number(&arg.as_value())?,
        None => 1.0,
    };

    let date = supported_serial_date(serial, ctx.date_system())?;
    match weeknum_system(return_type)? {
        WeeknumSystem::System1 { first_day } => Ok(system1_weeknum(&date, first_day)),
        WeeknumSystem::Iso => Ok(iso_weeknum(&date)),
    }
}

struct SerialDate {
    serial_day: i64,
    year: i32,
    month: u32,
    day: u32,
    day_of_year: u32,
}

fn supported_serial_date(serial: f64, date_system: DateSystem) -> Result<SerialDate, ErrorValue> {
    if !serial.is_finite() {
        return Err(ErrorValue::Num);
    }

    let serial_day = serial.trunc();
    if !(1.0..=MAX_EXCEL_1900_SERIAL).contains(&serial_day) {
        return Err(ErrorValue::Num);
    }

    let (year, month, day) = serial_to_ymd(serial_day, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(ErrorValue::Num);
    }

    Ok(SerialDate {
        serial_day: serial_day as i64,
        year,
        month,
        day,
        day_of_year: day_of_year(year, month, day, date_system),
    })
}

enum WeeknumSystem {
    System1 { first_day: i64 },
    Iso,
}

fn weeknum_system(return_type: f64) -> Result<WeeknumSystem, ErrorValue> {
    if !return_type.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = return_type.trunc();
    if truncated < i32::MIN as f64 || truncated > i32::MAX as f64 {
        return Err(ErrorValue::Num);
    }

    match truncated as i32 {
        1 | 17 => Ok(WeeknumSystem::System1 { first_day: 1 }),
        2 | 11 => Ok(WeeknumSystem::System1 { first_day: 2 }),
        12..=16 => Ok(WeeknumSystem::System1 {
            first_day: i64::from(truncated as i32 - 9),
        }),
        21 => Ok(WeeknumSystem::Iso),
        _ => Err(ErrorValue::Num),
    }
}

fn system1_weeknum(date: &SerialDate, first_day: i64) -> i64 {
    let jan1_serial = date.serial_day - i64::from(date.day_of_year) + 1;
    let jan1_weekday = sunday_based_weekday(jan1_serial);
    let offset = (jan1_weekday - first_day).rem_euclid(7);

    (i64::from(date.day_of_year) + offset - 1) / 7 + 1
}

fn sunday_based_weekday(serial_day: i64) -> i64 {
    (serial_day - 1).rem_euclid(7) + 1
}

fn iso_weeknum(date: &SerialDate) -> i64 {
    let weekday = gregorian_iso_weekday(date.year, date.month, date.day);
    let week = (i64::from(date.day_of_year) - weekday + 10).div_euclid(7);

    if week < 1 {
        iso_weeks_in_year(date.year - 1)
    } else if week > iso_weeks_in_year(date.year) {
        1
    } else {
        week
    }
}

fn iso_weeks_in_year(year: i32) -> i64 {
    let jan1_weekday = gregorian_iso_weekday(year, 1, 1);
    if jan1_weekday == 4 || (jan1_weekday == 3 && is_leap_year(year)) {
        53
    } else {
        52
    }
}

fn gregorian_iso_weekday(year: i32, month: u32, day: u32) -> i64 {
    const MONTH_OFFSETS: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];

    let mut y = i64::from(year);
    if month < 3 {
        y -= 1;
    }

    let weekday = (y + y / 4 - y / 100
        + y / 400
        + MONTH_OFFSETS[(month - 1) as usize]
        + i64::from(day))
    .rem_euclid(7);
    if weekday == 0 {
        7
    } else {
        weekday
    }
}

fn day_of_year(year: i32, month: u32, day: u32, date_system: DateSystem) -> u32 {
    (1..month)
        .map(|m| days_in_month(year, m, date_system))
        .sum::<u32>()
        + day
}

fn days_in_month(year: i32, month: u32, date_system: DateSystem) -> u32 {
    match (date_system, year, month) {
        (DateSystem::Excel1900, 1900, 2) => 29,
        (_, _, 1 | 3 | 5 | 7 | 8 | 10 | 12) => 31,
        (_, _, 4 | 6 | 9 | 11) => 30,
        (_, _, 2) if is_leap_year(year) => 29,
        (_, _, 2) => 28,
        _ => unreachable!("serial_to_ymd returns a normalized month"),
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

static WEEKNUM: Weeknum = Weeknum;
inventory::submit! { FunctionEntry(&WEEKNUM) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::model::date::ymd_to_serial;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_one_or_two_argument_arity() {
        assert_eq!(WEEKNUM.name(), "WEEKNUM");
        assert_eq!(WEEKNUM.arity(), (1, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_weeknum(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_documented_microsoft_examples() {
        assert_eq!(
            call_weeknum(vec![Value::Number(serial(2012, 3, 9))]),
            Value::Number(10.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(2.0),
            ]),
            Value::Number(11.0)
        );
    }

    #[test]
    fn supports_system_one_return_types() {
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(11.0),
            ]),
            Value::Number(11.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(12.0),
            ]),
            Value::Number(11.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(16.0),
            ]),
            Value::Number(10.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(17.0),
            ]),
            Value::Number(10.0)
        );
    }

    #[test]
    fn supports_iso_system_two() {
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2021, 1, 1)),
                Value::Number(21.0),
            ]),
            Value::Number(53.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2021, 1, 4)),
                Value::Number(21.0),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2020, 12, 31)),
                Value::Number(21.0),
            ]),
            Value::Number(53.0)
        );
    }

    #[test]
    fn handles_year_end_system_one_boundaries() {
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2020, 12, 31)),
                Value::Number(1.0),
            ]),
            Value::Number(53.0)
        );
    }

    #[test]
    fn preserves_excel_1900_system_one_serial_anchors() {
        assert_eq!(call_weeknum(vec![Value::Number(1.0)]), Value::Number(1.0));
        assert_eq!(call_weeknum(vec![Value::Number(60.0)]), Value::Number(9.0));
        assert_eq!(call_weeknum(vec![Value::Number(61.0)]), Value::Number(9.0));
    }

    #[test]
    fn truncates_serial_and_return_type_toward_zero() {
        assert_eq!(
            call_weeknum(vec![Value::Number(serial(2012, 3, 9) + 0.75)]),
            Value::Number(10.0)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(2.9),
            ]),
            Value::Number(11.0)
        );
    }

    #[test]
    fn uses_scalar_top_left_values_and_coercion() {
        assert_eq!(
            call_weeknum(vec![Value::Text(" 40977 ".to_string())]),
            Value::Number(10.0)
        );
        assert_eq!(call_weeknum(vec![Value::Boolean(true)]), Value::Number(1.0));

        let ctx = RangeContext;
        let fn_ctx = FnContext::new(&ctx);
        let serial_range = RangeRef {
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
        let return_type_range = RangeRef {
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
            WEEKNUM.call(
                &[
                    Arg::Range(ctx.range_view(serial_range)),
                    Arg::Range(ctx.range_view(return_type_range)),
                ],
                &fn_ctx,
            ),
            Value::Number(11.0)
        );
    }

    #[test]
    fn rejects_invalid_serials() {
        assert_eq!(call_weeknum(vec![Value::Number(0.0)]), Value::Error(ErrorValue::Num));
        assert_eq!(
            call_weeknum(vec![Value::Number(0.9)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![Value::Number(-1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![Value::Number(2_958_466.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_return_types() {
        assert_eq!(
            call_weeknum(vec![Value::Number(serial(2012, 3, 9)), Value::Number(0.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![Value::Number(serial(2012, 3, 9)), Value::Number(10.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![Value::Number(serial(2012, 3, 9)), Value::Number(18.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_weeknum(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_weeknum(vec![
                Value::Number(serial(2012, 3, 9)),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_weeknum(vec![Value::Error(ErrorValue::Ref), Value::Text("bad".to_string())]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_weeknum(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        WEEKNUM.call(&args, &fn_ctx)
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
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }

    struct RangeContext;

    impl EvalContext for RangeContext {
        fn cell_value(&self, r: CellRef) -> Value {
            match (r.row, r.col) {
                (0, 0) => Value::Number(serial(2012, 3, 9)),
                (0, 2) => Value::Number(2.0),
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
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
