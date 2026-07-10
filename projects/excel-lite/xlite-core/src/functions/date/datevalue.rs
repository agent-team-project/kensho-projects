use crate::functions::prelude::*;
use crate::model::date::{serial_to_ymd, ymd_to_serial};

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;

pub struct Datevalue;

impl Function for Datevalue {
    fn name(&self) -> &'static str {
        "DATEVALUE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match datevalue_serial(args, ctx) {
            Ok(serial) => Value::Number(serial),
            Err(error) => Value::Error(error),
        }
    }
}

fn datevalue_serial(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let text = ctx.to_text(&args[0].as_value())?;
    parse_date_text(&text, ctx)
}

fn parse_date_text(text: &str, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let date_part = split_date_part(text).ok_or(ErrorValue::Value)?;

    let date = parse_numeric_date(date_part)
        .or_else(|| parse_month_name_date(date_part, ctx))
        .ok_or(ErrorValue::Value)?;

    serial_for_date(date, ctx.date_system())
}

fn split_date_part(text: &str) -> Option<&str> {
    let mut parts = text.trim().split_whitespace();
    let date_part = parts.next()?;
    let tail: Vec<_> = parts.collect();
    if tail.is_empty() || is_time_tail(&tail) {
        Some(date_part)
    } else {
        None
    }
}

fn is_time_tail(parts: &[&str]) -> bool {
    match parts {
        [time] => parse_time_token(time, false),
        [time, meridiem] if is_meridiem(meridiem) => parse_time_token(time, true),
        _ => false,
    }
}

fn parse_time_token(token: &str, has_separate_meridiem: bool) -> bool {
    let (time, has_suffix_meridiem) = strip_meridiem_suffix(token);
    let has_meridiem = has_separate_meridiem || has_suffix_meridiem;
    let fields: Vec<_> = time.split(':').collect();
    if fields.is_empty() || fields.len() > 3 {
        return false;
    }
    if fields.len() == 1 && !has_meridiem {
        return false;
    }
    if fields
        .iter()
        .any(|field| field.is_empty() || field.len() > 2 || !field.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }

    let hour = fields[0].parse::<u32>().ok();
    let minute = fields.get(1).and_then(|field| field.parse::<u32>().ok());
    let second = fields.get(2).and_then(|field| field.parse::<u32>().ok());

    match hour {
        Some(1..=12) if has_meridiem => {}
        Some(0..=23) if !has_meridiem => {}
        _ => return false,
    }

    minute.is_none_or(|value| value < 60) && second.is_none_or(|value| value < 60)
}

fn strip_meridiem_suffix(token: &str) -> (&str, bool) {
    if token.len() > 2 {
        let (prefix, suffix) = token.split_at(token.len() - 2);
        if is_meridiem(suffix) {
            return (prefix, true);
        }
    }

    (token, false)
}

fn is_meridiem(text: &str) -> bool {
    text.eq_ignore_ascii_case("AM") || text.eq_ignore_ascii_case("PM")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DateParts {
    year: i32,
    month: u32,
    day: u32,
}

fn parse_numeric_date(date_part: &str) -> Option<DateParts> {
    let separator = if date_part.contains('/') && !date_part.contains('-') {
        '/'
    } else if date_part.contains('-') && !date_part.contains('/') {
        '-'
    } else {
        return None;
    };

    let parts: Vec<_> = date_part.split(separator).collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }

    if parts[0].len() == 4 {
        let year = parse_year(parts[0])?;
        let month = parse_month_or_day(parts[1])?;
        let day = parse_month_or_day(parts[2])?;
        Some(DateParts { year, month, day })
    } else {
        let month = parse_month_or_day(parts[0])?;
        let day = parse_month_or_day(parts[1])?;
        let year = parse_year(parts[2])?;
        Some(DateParts { year, month, day })
    }
}

fn parse_month_name_date(date_part: &str, ctx: &FnContext<'_>) -> Option<DateParts> {
    let parts: Vec<_> = date_part.split('-').collect();
    if parts.len() == 3 {
        if let Some(month) = month_number(parts[1]) {
            let day = parse_month_or_day(parts[0])?;
            let year = parse_year(parts[2])?;
            return Some(DateParts { year, month, day });
        }

        let month = month_number(parts[0])?;
        let day = parse_month_or_day(parts[1])?;
        let year = parse_year(parts[2])?;
        return Some(DateParts { year, month, day });
    }

    if parts.len() == 2 {
        let year = current_year(ctx)?;
        if let Some(month) = month_number(parts[1]) {
            let day = parse_month_or_day(parts[0])?;
            return Some(DateParts { year, month, day });
        }

        let month = month_number(parts[0])?;
        let day = parse_month_or_day(parts[1])?;
        return Some(DateParts { year, month, day });
    }

    None
}

fn parse_year(text: &str) -> Option<i32> {
    if text.len() != 4 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    text.parse().ok()
}

fn parse_month_or_day(text: &str) -> Option<u32> {
    if !(1..=2).contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    text.parse().ok()
}

fn month_number(text: &str) -> Option<u32> {
    const MONTHS: [(&str, &str); 12] = [
        ("jan", "january"),
        ("feb", "february"),
        ("mar", "march"),
        ("apr", "april"),
        ("may", "may"),
        ("jun", "june"),
        ("jul", "july"),
        ("aug", "august"),
        ("sep", "september"),
        ("oct", "october"),
        ("nov", "november"),
        ("dec", "december"),
    ];

    MONTHS
        .iter()
        .position(|(short, full)| text.eq_ignore_ascii_case(short) || text.eq_ignore_ascii_case(full))
        .map(|index| index as u32 + 1)
}

fn current_year(ctx: &FnContext<'_>) -> Option<i32> {
    let today = ctx.today_serial();
    if !today.is_finite() {
        return None;
    }

    let (year, _, _) = serial_to_ymd(today, ctx.date_system());
    Some(year)
}

fn serial_for_date(date: DateParts, date_system: DateSystem) -> Result<f64, ErrorValue> {
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&date.year) {
        return Err(ErrorValue::Value);
    }

    if date.day == 0 || date.day > days_in_month(date.year, date.month, date_system)? {
        return Err(ErrorValue::Value);
    }

    Ok(ymd_to_serial(date.year, date.month, date.day, date_system))
}

fn days_in_month(year: i32, month: u32, date_system: DateSystem) -> Result<u32, ErrorValue> {
    match (date_system, year, month) {
        (DateSystem::Excel1900, 1900, 2) => Ok(29),
        (_, _, 1 | 3 | 5 | 7 | 8 | 10 | 12) => Ok(31),
        (_, _, 4 | 6 | 9 | 11) => Ok(30),
        (_, _, 2) if is_leap_year(year) => Ok(29),
        (_, _, 2) => Ok(28),
        _ => Err(ErrorValue::Value),
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

static DATEVALUE: Datevalue = Datevalue;
inventory::submit! { FunctionEntry(&DATEVALUE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(DATEVALUE.name(), "DATEVALUE");
        assert_eq!(DATEVALUE.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_datevalue(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_datevalue(vec![
                Value::Text("1/1/2008".to_string()),
                Value::Text("1/2/2008".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn parses_microsoft_documented_examples() {
        assert_eq!(
            call_datevalue(vec![Value::Text("1/1/2008".to_string())]),
            Value::Number(39_448.0)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("8/22/2011".to_string())]),
            Value::Number(40_777.0)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("22-MAY-2011".to_string())]),
            Value::Number(40_685.0)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("2011/02/23".to_string())]),
            Value::Number(40_597.0)
        );
    }

    #[test]
    fn parses_invariant_numeric_forms() {
        assert_eq!(
            call_datevalue(vec![Value::Text("02-03-2020".to_string())]),
            Value::Number(serial(2020, 2, 3))
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("2020-2-3".to_string())]),
            Value::Number(serial(2020, 2, 3))
        );
    }

    #[test]
    fn parses_month_names_case_insensitively() {
        assert_eq!(
            call_datevalue(vec![Value::Text("15-feBRuary-2020".to_string())]),
            Value::Number(serial(2020, 2, 15))
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("February-15-2020".to_string())]),
            Value::Number(serial(2020, 2, 15))
        );
    }

    #[test]
    fn uses_current_year_when_month_name_year_is_omitted() {
        let ctx = FixedTodayContext(serial(2024, 1, 1));
        assert_eq!(
            call_datevalue_with(&ctx, vec![Value::Text("5-JUL".to_string())]),
            Value::Number(serial(2024, 7, 5))
        );
        assert_eq!(
            call_datevalue_with(&ctx, vec![Value::Text("Jul-5".to_string())]),
            Value::Number(serial(2024, 7, 5))
        );
    }

    #[test]
    fn ignores_time_portion_after_recognized_date_text() {
        assert_eq!(
            call_datevalue(vec![Value::Text("1/1/2008 10:45 PM".to_string())]),
            Value::Number(39_448.0)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("1/1/2008 not-time".to_string())]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn preserves_excel_1900_phantom_day() {
        assert_eq!(
            call_datevalue(vec![Value::Text("2/29/1900".to_string())]),
            Value::Number(60.0)
        );
    }

    #[test]
    fn rejects_unrecognized_and_invalid_dates() {
        assert_eq!(
            call_datevalue(vec![Value::Text("not a date".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("2/30/2020".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("12/31/1899".to_string())]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_two_digit_years_and_ambiguous_day_month_numeric_forms() {
        assert_eq!(
            call_datevalue(vec![Value::Text("1/1/08".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_datevalue(vec![Value::Text("30/1/2008".to_string())]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn uses_top_left_scalar_and_propagates_text_coercion_errors() {
        let ctx = RangeContext;
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
                row: 0,
                col_abs: false,
                row_abs: false,
            },
        };

        assert_eq!(
            DATEVALUE.call(&[Arg::Range(ctx.range_view(range))], &fn_ctx),
            Value::Number(40_597.0)
        );
        assert_eq!(
            call_datevalue(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_datevalue(values: Vec<Value>) -> Value {
        call_datevalue_with(&DummyContext, values)
    }

    fn call_datevalue_with(ctx: &dyn EvalContext, values: Vec<Value>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DATEVALUE.call(&args, &fn_ctx)
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

    struct FixedTodayContext(f64);

    impl EvalContext for FixedTodayContext {
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

        fn today_serial(&self) -> f64 {
            self.0
        }
    }

    struct RangeContext;

    impl EvalContext for RangeContext {
        fn cell_value(&self, r: CellRef) -> Value {
            if r.row == 0 && r.col == 0 {
                Value::Text("2011/02/23".to_string())
            } else {
                Value::Text("not a date".to_string())
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
