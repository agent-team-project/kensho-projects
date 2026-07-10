use crate::functions::prelude::*;
use crate::model::date::{serial_to_ymd, ymd_to_serial};

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const SECONDS_PER_DAY: u32 = 86_400;

pub struct ValueFn;

impl Function for ValueFn {
    fn name(&self) -> &'static str {
        "VALUE"
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

        match parse_value_text(&text, ctx) {
            Ok(value) if value.is_finite() => Value::Number(value),
            _ => Value::Error(ErrorValue::Value),
        }
    }
}

fn parse_value_text(text: &str, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let trimmed = trim_ascii_whitespace(text);
    if trimmed.is_empty() {
        return Err(ErrorValue::Value);
    }

    if let Some(number) = parse_number_text(trimmed) {
        return Ok(number);
    }

    if let Some(time) = parse_timevalue_text(trimmed) {
        return Ok(time);
    }

    parse_datevalue_text(trimmed, ctx)
}

fn trim_ascii_whitespace(text: &str) -> &str {
    text.trim_matches(|ch: char| ch.is_ascii_whitespace())
}

fn parse_number_text(text: &str) -> Option<f64> {
    let (number_text, percent_count) = strip_percent_suffixes(text);
    let normalized = normalize_decimal_number(number_text)?;
    let mut value = normalized.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }

    for _ in 0..percent_count {
        value /= 100.0;
        if !value.is_finite() {
            return None;
        }
    }

    Some(value)
}

fn strip_percent_suffixes(text: &str) -> (&str, usize) {
    let mut end = text.len();
    let mut count = 0;
    while end > 0 && text.as_bytes()[end - 1] == b'%' {
        end -= 1;
        count += 1;
    }

    (&text[..end], count)
}

fn normalize_decimal_number(text: &str) -> Option<String> {
    if text.is_empty() || text.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }

    let mut rest = text;
    let mut normalized = String::new();
    if let Some(sign @ (b'+' | b'-')) = rest.as_bytes().first().copied() {
        normalized.push(sign as char);
        rest = &rest[1..];
    }

    if let Some(after_currency) = rest.strip_prefix('$') {
        rest = after_currency;
    }

    if rest.is_empty() || rest.contains('$') {
        return None;
    }

    let (mantissa, exponent) = split_exponent(rest)?;
    let (integer, fraction) = split_mantissa(mantissa)?;
    let integer = normalize_integer_grouping(integer)?;

    if integer.is_empty() && fraction.is_none_or(str::is_empty) {
        return None;
    }

    normalized.push_str(&integer);
    if let Some(fraction) = fraction {
        normalized.push('.');
        normalized.push_str(fraction);
    }
    if let Some(exponent) = exponent {
        normalized.push('e');
        normalized.push_str(exponent);
    }

    Some(normalized)
}

fn split_exponent(text: &str) -> Option<(&str, Option<&str>)> {
    let mut indices = text
        .bytes()
        .enumerate()
        .filter_map(|(index, byte)| (byte == b'e' || byte == b'E').then_some(index));
    let first = indices.next();
    if indices.next().is_some() {
        return None;
    }

    match first {
        Some(index) => {
            let mantissa = &text[..index];
            let exponent = &text[index + 1..];
            if mantissa.is_empty() || !valid_exponent(exponent) {
                return None;
            }
            Some((mantissa, Some(exponent)))
        }
        None => Some((text, None)),
    }
}

fn valid_exponent(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn split_mantissa(text: &str) -> Option<(&str, Option<&str>)> {
    let mut parts = text.split('.');
    let integer = parts.next()?;
    let fraction = parts.next();
    if parts.next().is_some() {
        return None;
    }
    if let Some(fraction) = fraction {
        if fraction.contains(',') || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }

    Some((integer, fraction))
}

fn normalize_integer_grouping(text: &str) -> Option<String> {
    if !text.contains(',') {
        return text
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| text.to_string());
    }

    let groups: Vec<_> = text.split(',').collect();
    if groups.len() < 2
        || groups[0].is_empty()
        || groups[0].len() > 3
        || !groups[0].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    for group in &groups[1..] {
        if group.len() != 3 || !group.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }

    Some(groups.concat())
}

fn parse_timevalue_text(text: &str) -> Option<f64> {
    parse_time_candidate(text)
        .or_else(|| parse_date_prefixed_time(text))
        .map(|seconds| seconds as f64 / SECONDS_PER_DAY as f64)
}

fn parse_date_prefixed_time(text: &str) -> Option<u32> {
    let tokens: Vec<_> = text.split_ascii_whitespace().collect();
    match tokens.as_slice() {
        [date, time] if parse_time_date_prefix(date).is_some() => parse_time_candidate(time),
        [date, time, meridiem] if is_meridiem(meridiem) && parse_time_date_prefix(date).is_some() => {
            parse_time_candidate(&format!("{time} {meridiem}"))
        }
        _ => None,
    }
}

fn parse_time_candidate(text: &str) -> Option<u32> {
    let (time, meridiem) = split_meridiem(trim_ascii_whitespace(text));
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
            return (trim_ascii_end(&text[..text.len() - 2]), Some(Meridiem::Am));
        }
        if suffix.eq_ignore_ascii_case(b"PM") {
            return (trim_ascii_end(&text[..text.len() - 2]), Some(Meridiem::Pm));
        }
    }

    (text, None)
}

fn trim_ascii_end(text: &str) -> &str {
    text.trim_end_matches(|ch: char| ch.is_ascii_whitespace())
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

fn parse_time_date_prefix(text: &str) -> Option<TimeDateParts> {
    parse_time_numeric_date(text)
        .or_else(|| parse_time_month_name_date(text))
        .filter(TimeDateParts::is_valid)
}

fn parse_time_numeric_date(text: &str) -> Option<TimeDateParts> {
    let parts: Vec<_> = text.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 {
        return None;
    }

    Some(TimeDateParts {
        year: parse_time_year(parts[0])?,
        month: parse_one_or_two_digit_number(parts[1])?,
        day: parse_one_or_two_digit_number(parts[2])?,
    })
}

fn parse_time_month_name_date(text: &str) -> Option<TimeDateParts> {
    let parts: Vec<_> = text.split('-').collect();
    if parts.len() != 3 {
        return None;
    }

    Some(TimeDateParts {
        day: parse_one_or_two_digit_number(parts[0])?,
        month: month_number(parts[1])?,
        year: parse_time_year(parts[2])?,
    })
}

fn parse_time_year(text: &str) -> Option<u32> {
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

#[derive(Clone, Copy)]
enum Meridiem {
    Am,
    Pm,
}

#[derive(Clone, Copy)]
struct TimeDateParts {
    year: u32,
    month: u32,
    day: u32,
}

impl TimeDateParts {
    fn is_valid(&self) -> bool {
        self.day > 0 && self.day <= time_days_in_month(self.year, self.month).unwrap_or(0)
    }
}

fn time_days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_time_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

fn is_time_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn parse_datevalue_text(text: &str, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let date_part = split_date_part(text).ok_or(ErrorValue::Value)?;

    let date = parse_numeric_date(date_part)
        .or_else(|| parse_month_name_date(date_part, ctx))
        .ok_or(ErrorValue::Value)?;

    serial_for_date(date, ctx.date_system())
}

fn split_date_part(text: &str) -> Option<&str> {
    let mut parts = text.split_ascii_whitespace();
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
        [time] => parse_datevalue_time_token(time, false),
        [time, meridiem] if is_meridiem(meridiem) => parse_datevalue_time_token(time, true),
        _ => false,
    }
}

fn parse_datevalue_time_token(token: &str, has_separate_meridiem: bool) -> bool {
    let (time, has_suffix_meridiem) = strip_meridiem_suffix(token);
    let has_meridiem = has_separate_meridiem || has_suffix_meridiem;
    let fields: Vec<_> = time.split(':').collect();
    if fields.is_empty() || fields.len() > 3 {
        return false;
    }
    if fields.len() == 1 && !has_meridiem {
        return false;
    }
    if fields.iter().any(|field| {
        field.is_empty() || field.len() > 2 || !field.bytes().all(|byte| byte.is_ascii_digit())
    }) {
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

static VALUE: ValueFn = ValueFn;
inventory::submit! { FunctionEntry(&VALUE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(VALUE.name(), "VALUE");
        assert_eq!(VALUE.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_value(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_value(vec![text("1"), text("2")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn parses_decimal_and_scientific_numbers() {
        assert_eq!(call_value(vec![text("123")]), Value::Number(123.0));
        assert_eq!(call_value(vec![text("-12.5")]), Value::Number(-12.5));
        assert_eq!(call_value(vec![text("1.2E3")]), Value::Number(1200.0));
        assert_eq!(call_value(vec![text("+.5")]), Value::Number(0.5));
        assert_eq!(call_value(vec![text("1.")]), Value::Number(1.0));
    }

    #[test]
    fn parses_currency_grouping_and_percent_suffixes() {
        assert_eq!(call_value(vec![text("$1,000")]), Value::Number(1000.0));
        assert_eq!(call_value(vec![text("-$1,000.50")]), Value::Number(-1000.5));
        assert_close(call_value(vec![text("3.5%")]), 0.035);
        assert_close(call_value(vec![text("9%%")]), 0.0009);
    }

    #[test]
    fn rejects_malformed_number_text() {
        assert_eq!(call_value(vec![text("")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("   ")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("not a number")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("12,34")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("1,0000")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("$")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("1e309")]), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn parses_timevalue_compatible_text() {
        assert_close(call_value(vec![text("16:48:00")]), 0.7);
        assert_eq!(call_value(vec![text("12:00:00")]), Value::Number(0.5));
        assert_eq!(call_value(vec![text("6:45 PM")]), Value::Number(0.78125));
        assert_eq!(
            call_value(vec![text("2024-01-31 18:45")]),
            Value::Number(0.78125)
        );
    }

    #[test]
    fn parses_datevalue_compatible_text() {
        assert_eq!(call_value(vec![text("2011/02/23")]), Value::Number(40_597.0));
        assert_eq!(
            call_value(vec![text("15-feBRuary-2020")]),
            Value::Number(serial(2020, 2, 15))
        );
        assert_eq!(
            call_value(vec![text("1/1/2008 10:45 PM")]),
            Value::Number(39_448.0)
        );
    }

    #[test]
    fn uses_current_year_for_month_day_text() {
        let ctx = FixedTodayContext(serial(2024, 1, 1));
        assert_eq!(
            call_value_with(&ctx, vec![text("5-JUL")]),
            Value::Number(serial(2024, 7, 5))
        );
    }

    #[test]
    fn rejects_invalid_dates_and_times() {
        assert_eq!(call_value(vec![text("24:00")]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![text("13:00 PM")]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_value(vec![text("2024-02-30 18:45")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_value(vec![text("2/30/2020")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn coerces_scalars_and_uses_range_top_left() {
        assert_eq!(call_value(vec![Value::Number(123.0)]), Value::Number(123.0));
        assert_eq!(call_value(vec![Value::Boolean(true)]), Value::Error(ErrorValue::Value));
        assert_eq!(call_value(vec![Value::Blank]), Value::Error(ErrorValue::Value));

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
            VALUE.call(&[Arg::Range(ctx.range_view(range))], &fn_ctx),
            Value::Number(1000.0)
        );
    }

    #[test]
    fn propagates_text_coercion_errors() {
        assert_eq!(
            call_value(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_value(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_value(values: Vec<Value>) -> Value {
        call_value_with(&DummyContext, values)
    }

    fn call_value_with(ctx: &dyn EvalContext, values: Vec<Value>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        VALUE.call(&args, &fn_ctx)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        ymd_to_serial(year, month, day, SYS)
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
                text("$1,000")
            } else {
                text("not a number")
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
