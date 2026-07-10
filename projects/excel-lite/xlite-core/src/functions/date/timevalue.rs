use crate::functions::prelude::*;

const SECONDS_PER_DAY: u32 = 86_400;

pub struct Timevalue;

impl Function for Timevalue {
    fn name(&self) -> &'static str {
        "TIMEVALUE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        let text = match ctx.to_text(&args[0].as_value()) {
            Ok(text) => text,
            Err(error) => return Value::Error(error),
        };

        match parse_timevalue(&text) {
            Some(value) => Value::Number(value),
            None => Value::Error(ErrorValue::Value),
        }
    }
}

fn parse_timevalue(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    parse_time_candidate(trimmed)
        .or_else(|| parse_date_prefixed_time(trimmed))
        .map(|seconds| seconds as f64 / SECONDS_PER_DAY as f64)
}

fn parse_date_prefixed_time(text: &str) -> Option<u32> {
    let tokens: Vec<_> = text.split_whitespace().collect();
    match tokens.as_slice() {
        [date, time] if parse_date_prefix(date).is_some() => parse_time_candidate(time),
        [date, time, meridiem] if is_meridiem(meridiem) && parse_date_prefix(date).is_some() => {
            parse_time_candidate(&format!("{time} {meridiem}"))
        }
        _ => None,
    }
}

fn parse_time_candidate(text: &str) -> Option<u32> {
    let (time, meridiem) = split_meridiem(text.trim());
    let parts: Vec<_> = time.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }

    let mut hour = parse_hour(parts[0])?;
    let minute = parse_two_digit_part(parts[1])?;
    let second = if parts.len() == 3 {
        parse_two_digit_part(parts[2])?
    } else {
        0
    };

    if minute > 59 || second > 59 {
        return None;
    }

    match meridiem {
        Some(Meridiem::Am) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            if hour == 12 {
                hour = 0;
            }
        }
        Some(Meridiem::Pm) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            if hour != 12 {
                hour += 12;
            }
        }
        None => {
            if hour > 23 {
                return None;
            }
        }
    }

    Some(hour * 3_600 + minute * 60 + second)
}

fn split_meridiem(text: &str) -> (&str, Option<Meridiem>) {
    let bytes = text.as_bytes();
    if bytes.len() >= 2 {
        let suffix = &bytes[bytes.len() - 2..];
        if suffix.eq_ignore_ascii_case(b"AM") {
            return (text[..text.len() - 2].trim_end(), Some(Meridiem::Am));
        }
        if suffix.eq_ignore_ascii_case(b"PM") {
            return (text[..text.len() - 2].trim_end(), Some(Meridiem::Pm));
        }
    }

    (text, None)
}

fn is_meridiem(text: &str) -> bool {
    text.eq_ignore_ascii_case("AM") || text.eq_ignore_ascii_case("PM")
}

fn parse_hour(text: &str) -> Option<u32> {
    if !(1..=2).contains(&text.len()) || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    text.parse().ok()
}

fn parse_two_digit_part(text: &str) -> Option<u32> {
    if text.len() != 2 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    text.parse().ok()
}

fn parse_date_prefix(text: &str) -> Option<DateParts> {
    parse_numeric_date(text)
        .or_else(|| parse_month_name_date(text))
        .filter(DateParts::is_valid)
}

fn parse_numeric_date(text: &str) -> Option<DateParts> {
    let parts: Vec<_> = text.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 {
        return None;
    }

    Some(DateParts {
        year: parse_year(parts[0])?,
        month: parse_one_or_two_digit_number(parts[1])?,
        day: parse_one_or_two_digit_number(parts[2])?,
    })
}

fn parse_month_name_date(text: &str) -> Option<DateParts> {
    let parts: Vec<_> = text.split('-').collect();
    if parts.len() != 3 {
        return None;
    }

    Some(DateParts {
        day: parse_one_or_two_digit_number(parts[0])?,
        month: month_number(parts[1])?,
        year: parse_year(parts[2])?,
    })
}

fn parse_year(text: &str) -> Option<u32> {
    if text.len() != 4 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    let year = text.parse().ok()?;
    (1..=9999).contains(&year).then_some(year)
}

fn parse_one_or_two_digit_number(text: &str) -> Option<u32> {
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

#[derive(Clone, Copy)]
enum Meridiem {
    Am,
    Pm,
}

#[derive(Clone, Copy)]
struct DateParts {
    year: u32,
    month: u32,
    day: u32,
}

impl DateParts {
    fn is_valid(&self) -> bool {
        self.day > 0 && self.day <= days_in_month(self.year, self.month).unwrap_or(0)
    }
}

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

static TIMEVALUE: Timevalue = Timevalue;
inventory::submit! { FunctionEntry(&TIMEVALUE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(TIMEVALUE.name(), "TIMEVALUE");
        assert_eq!(TIMEVALUE.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_timevalue(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_timevalue(vec![
                Value::Text("6:45 PM".to_string()),
                Value::Text("extra".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn parses_documented_examples() {
        assert_close(call_timevalue(vec![text("2:24 AM")]), 0.1);
        assert_close(
            call_timevalue(vec![text("22-Aug-2011 6:35 AM")]),
            0.2743055555555556,
        );
        assert_eq!(call_timevalue(vec![text("6:45 PM")]), Value::Number(0.78125));
        assert_eq!(call_timevalue(vec![text("18:45")]), Value::Number(0.78125));
    }

    #[test]
    fn parses_midnight_and_noon_edges() {
        assert_eq!(call_timevalue(vec![text("12:00 AM")]), Value::Number(0.0));
        assert_eq!(call_timevalue(vec![text("12:00 PM")]), Value::Number(0.5));
        assert_eq!(call_timevalue(vec![text("00:00")]), Value::Number(0.0));
        assert_eq!(call_timevalue(vec![text("0:00")]), Value::Number(0.0));
    }

    #[test]
    fn includes_seconds_in_fraction() {
        assert_close(
            call_timevalue(vec![text("23:59:59")]),
            86_399.0 / 86_400.0,
        );
        assert_close(
            call_timevalue(vec![text("1:02:03 PM")]),
            46_923.0 / 86_400.0,
        );
    }

    #[test]
    fn accepts_case_insensitive_suffix_with_optional_whitespace() {
        assert_eq!(call_timevalue(vec![text("6:45pm")]), Value::Number(0.78125));
        assert_eq!(call_timevalue(vec![text("6:45 pM")]), Value::Number(0.78125));
    }

    #[test]
    fn accepts_recognized_date_prefixes_and_ignores_date_value() {
        assert_eq!(
            call_timevalue(vec![text("2024-01-31 18:45")]),
            Value::Number(0.78125)
        );
        assert_eq!(
            call_timevalue(vec![text("2024-1-31 6:45 PM")]),
            Value::Number(0.78125)
        );
        assert_eq!(
            call_timevalue(vec![text("1-August-2024 6:45PM")]),
            Value::Number(0.78125)
        );
    }

    #[test]
    fn rejects_arbitrary_or_invalid_date_prefixes() {
        assert_eq!(
            call_timevalue(vec![text("not a date 6:45 PM")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_timevalue(vec![text("notadate 6:45PM")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_timevalue(vec![text("2024/01/31 18:45")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_timevalue(vec![text("2024-02-30 18:45")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_invalid_time_ranges() {
        assert_eq!(call_timevalue(vec![text("24:00")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![text("13:00 PM")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![text("0:30 AM")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![text("12:60")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![text("12:00:60")]), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn rejects_unrecognized_and_numeric_only_text() {
        assert_eq!(call_timevalue(vec![text("0.5")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![text("12")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![text("")]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_timevalue(vec![text("6:45 PM trailing")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn uses_text_coercion_on_scalar_and_top_left_range_value() {
        assert_eq!(call_timevalue(vec![Value::Number(0.5)]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![Value::Boolean(true)]), Value::Error(ErrorValue::Value));
        assert_eq!(call_timevalue(vec![Value::Blank]), Value::Error(ErrorValue::Value));

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
            TIMEVALUE.call(&[Arg::Range(ctx.range_view(range))], &fn_ctx),
            Value::Number(0.78125)
        );
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_timevalue(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_timevalue(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_timevalue(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        TIMEVALUE.call(&args, &fn_ctx)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn assert_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => {
                assert!(
                    (actual - expected).abs() <= 1e-12,
                    "expected {expected}, got {actual}"
                );
            }
            other => panic!("expected number {expected}, got {other:?}"),
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
            DateSystem::Excel1900
        }

        fn current_cell(&self) -> CellId {
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }

    struct RangeContext;

    impl EvalContext for RangeContext {
        fn cell_value(&self, r: CellRef) -> Value {
            if r.row == 0 && r.col == 0 {
                text("6:45 PM")
            } else {
                text("0.5")
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
