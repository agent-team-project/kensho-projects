use crate::functions::prelude::*;
use crate::model::date::{serial_to_ymd, ymd_to_serial};

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const SECONDS_PER_DAY: u32 = 86_400;
const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub struct TextFn;

impl Function for TextFn {
    fn name(&self) -> &'static str {
        "TEXT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        let value = match ctx.to_number(&args[0].as_value()) {
            Ok(value) => value,
            Err(error) => return Value::Error(error),
        };
        let format_text = match ctx.to_text(&args[1].as_value()) {
            Ok(format_text) => format_text,
            Err(error) => return Value::Error(error),
        };

        match format_text_value(value, &format_text, ctx.date_system()) {
            Ok(text) => Value::Text(text),
            Err(error) => Value::Error(error),
        }
    }
}

fn format_text_value(
    value: f64,
    format_text: &str,
    date_system: DateSystem,
) -> Result<String, ErrorValue> {
    if !value.is_finite() {
        return Err(ErrorValue::Value);
    }

    if let Some(format) = NumberFormat::parse(format_text) {
        return format.format(value);
    }
    if let Some(format) = TextDateFormat::parse(format_text) {
        return format.format(value, date_system);
    }
    if let Some(format) = TextTimeFormat::parse(format_text) {
        return format.format(value);
    }

    // V1 intentionally rejects the rest of Excel's custom format language.
    Err(ErrorValue::Value)
}

#[derive(Clone, Copy)]
enum NumberFormat {
    Fixed { decimals: usize, grouped: bool },
    Percent { decimals: usize },
    Currency { grouped: bool },
}

impl NumberFormat {
    fn parse(format_text: &str) -> Option<Self> {
        match format_text {
            "0" => Some(Self::Fixed {
                decimals: 0,
                grouped: false,
            }),
            "0.0" => Some(Self::Fixed {
                decimals: 1,
                grouped: false,
            }),
            "0.00" => Some(Self::Fixed {
                decimals: 2,
                grouped: false,
            }),
            "#,##0" => Some(Self::Fixed {
                decimals: 0,
                grouped: true,
            }),
            "#,##0.0" => Some(Self::Fixed {
                decimals: 1,
                grouped: true,
            }),
            "#,##0.00" => Some(Self::Fixed {
                decimals: 2,
                grouped: true,
            }),
            "0%" => Some(Self::Percent { decimals: 0 }),
            "0.0%" => Some(Self::Percent { decimals: 1 }),
            "0.00%" => Some(Self::Percent { decimals: 2 }),
            "$0.00" => Some(Self::Currency { grouped: false }),
            "$#,##0.00" => Some(Self::Currency { grouped: true }),
            _ => None,
        }
    }

    fn format(self, value: f64) -> Result<String, ErrorValue> {
        match self {
            Self::Fixed { decimals, grouped } => format_fixed_number(value, decimals, grouped, "", ""),
            Self::Percent { decimals } => {
                let percent = value * 100.0;
                if percent.is_finite() {
                    format_fixed_number(percent, decimals, false, "", "%")
                } else {
                    Err(ErrorValue::Value)
                }
            }
            Self::Currency { grouped } => format_fixed_number(value, 2, grouped, "$", ""),
        }
    }
}

fn format_fixed_number(
    value: f64,
    decimals: usize,
    grouped: bool,
    prefix: &str,
    suffix: &str,
) -> Result<String, ErrorValue> {
    let rounded = round_to_decimal_places(value, decimals)?;
    let negative = rounded < 0.0;
    let mut digits = format!("{:.*}", decimals, rounded.abs());
    if grouped {
        digits = add_group_separators(&digits);
    }

    let sign_len = if negative { 1 } else { 0 };
    let mut out = String::with_capacity(sign_len + prefix.len() + digits.len() + suffix.len());
    if negative {
        out.push('-');
    }
    out.push_str(prefix);
    out.push_str(&digits);
    out.push_str(suffix);
    Ok(out)
}

fn round_to_decimal_places(value: f64, decimals: usize) -> Result<f64, ErrorValue> {
    let factor = 10_f64.powi(decimals as i32);
    let scaled = value * factor;
    if !scaled.is_finite() {
        return Err(ErrorValue::Value);
    }

    Ok(round_half_away_from_zero(scaled) / factor)
}

fn round_half_away_from_zero(value: f64) -> f64 {
    if value.is_sign_negative() {
        (value - 0.5).ceil()
    } else {
        (value + 0.5).floor()
    }
}

fn add_group_separators(digits: &str) -> String {
    let (integer, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let mut reversed = String::with_capacity(integer.len() + integer.len() / 3);
    for (index, ch) in integer.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            reversed.push(',');
        }
        reversed.push(ch);
    }

    let mut grouped: String = reversed.chars().rev().collect();
    if !fraction.is_empty() {
        grouped.push('.');
        grouped.push_str(fraction);
    }
    grouped
}

#[derive(Clone, Copy)]
enum TextDateFormat {
    MonthDayYear,
    PaddedMonthDayYear,
    Iso,
    ShortMonthName,
    LongMonthName,
}

impl TextDateFormat {
    fn parse(format_text: &str) -> Option<Self> {
        if format_text.eq_ignore_ascii_case("m/d/yyyy") {
            Some(Self::MonthDayYear)
        } else if format_text.eq_ignore_ascii_case("mm/dd/yyyy") {
            Some(Self::PaddedMonthDayYear)
        } else if format_text.eq_ignore_ascii_case("yyyy-mm-dd") {
            Some(Self::Iso)
        } else if format_text.eq_ignore_ascii_case("mmm d, yyyy") {
            Some(Self::ShortMonthName)
        } else if format_text.eq_ignore_ascii_case("mmmm d, yyyy") {
            Some(Self::LongMonthName)
        } else {
            None
        }
    }

    fn format(self, value: f64, date_system: DateSystem) -> Result<String, ErrorValue> {
        let DateParts { year, month, day } = date_parts(value, date_system)?;
        Ok(match self {
            Self::MonthDayYear => format!("{month}/{day}/{year:04}"),
            Self::PaddedMonthDayYear => format!("{month:02}/{day:02}/{year:04}"),
            Self::Iso => format!("{year:04}-{month:02}-{day:02}"),
            Self::ShortMonthName => format!("{} {day}, {year:04}", month_name(month, MONTH_SHORT)?),
            Self::LongMonthName => format!("{} {day}, {year:04}", month_name(month, MONTH_LONG)?),
        })
    }
}

#[derive(Clone, Copy)]
struct DateParts {
    year: i32,
    month: u32,
    day: u32,
}

fn date_parts(value: f64, date_system: DateSystem) -> Result<DateParts, ErrorValue> {
    let serial_day = value.floor();
    let max_serial = ymd_to_serial(MAX_SUPPORTED_YEAR, 12, 31, date_system);
    if serial_day < 1.0 || serial_day > max_serial {
        return Err(ErrorValue::Value);
    }

    let (year, month, day) = serial_to_ymd(serial_day, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(ErrorValue::Value);
    }

    Ok(DateParts { year, month, day })
}

fn month_name(month: u32, names: [&'static str; 12]) -> Result<&'static str, ErrorValue> {
    names
        .get(month.saturating_sub(1) as usize)
        .copied()
        .ok_or(ErrorValue::Value)
}

#[derive(Clone, Copy)]
enum TextTimeFormat {
    HourMinute { pad_hour: bool, meridiem: bool },
    HourMinuteSecond,
}

impl TextTimeFormat {
    fn parse(format_text: &str) -> Option<Self> {
        if format_text.eq_ignore_ascii_case("h:mm") {
            Some(Self::HourMinute {
                pad_hour: false,
                meridiem: false,
            })
        } else if format_text.eq_ignore_ascii_case("hh:mm") {
            Some(Self::HourMinute {
                pad_hour: true,
                meridiem: false,
            })
        } else if format_text.eq_ignore_ascii_case("h:mm AM/PM") {
            Some(Self::HourMinute {
                pad_hour: false,
                meridiem: true,
            })
        } else if format_text.eq_ignore_ascii_case("hh:mm AM/PM") {
            Some(Self::HourMinute {
                pad_hour: true,
                meridiem: true,
            })
        } else if format_text.eq_ignore_ascii_case("hh:mm:ss") {
            Some(Self::HourMinuteSecond)
        } else {
            None
        }
    }

    fn format(self, value: f64) -> Result<String, ErrorValue> {
        let TimeParts {
            hour,
            minute,
            second,
        } = time_parts(value)?;
        Ok(match self {
            Self::HourMinute {
                pad_hour,
                meridiem: false,
            } => {
                if pad_hour {
                    format!("{hour:02}:{minute:02}")
                } else {
                    format!("{hour}:{minute:02}")
                }
            }
            Self::HourMinute {
                pad_hour,
                meridiem: true,
            } => {
                let suffix = if hour < 12 { "AM" } else { "PM" };
                let hour12 = match hour % 12 {
                    0 => 12,
                    value => value,
                };
                if pad_hour {
                    format!("{hour12:02}:{minute:02} {suffix}")
                } else {
                    format!("{hour12}:{minute:02} {suffix}")
                }
            }
            Self::HourMinuteSecond => format!("{hour:02}:{minute:02}:{second:02}"),
        })
    }
}

#[derive(Clone, Copy)]
struct TimeParts {
    hour: u32,
    minute: u32,
    second: u32,
}

fn time_parts(value: f64) -> Result<TimeParts, ErrorValue> {
    if value < 0.0 {
        return Err(ErrorValue::Value);
    }

    let scaled = value.fract() * f64::from(SECONDS_PER_DAY);
    if !scaled.is_finite() {
        return Err(ErrorValue::Value);
    }

    let total_seconds = (round_half_away_from_zero(scaled) as u32) % SECONDS_PER_DAY;
    Ok(TimeParts {
        hour: total_seconds / 3_600,
        minute: (total_seconds % 3_600) / 60,
        second: total_seconds % 60,
    })
}

static TEXT: TextFn = TextFn;
inventory::submit! { FunctionEntry(&TEXT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::functions::build_registry;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_metadata_and_registers() {
        assert_eq!(TEXT.name(), "TEXT");
        assert_eq!(TEXT.arity(), (2, Some(2)));
        assert!(build_registry().contains_key("TEXT"));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_text(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(call_text(vec![number(1.0)]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_text(vec![number(1.0), text("0"), number(2.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn formats_fixed_decimals_with_half_away_rounding() {
        assert_text(call_text(vec![number(1234.567), text("0.00")]), "1234.57");
        assert_text(call_text(vec![number(2.5), text("0")]), "3");
        assert_text(call_text(vec![number(-2.5), text("0")]), "-3");
        assert_text(call_text(vec![number(1.25), text("0.0")]), "1.3");
        assert_text(call_text(vec![number(-1.25), text("0.0")]), "-1.3");
    }

    #[test]
    fn formats_grouped_percent_and_currency_patterns() {
        assert_text(call_text(vec![number(1234.5), text("#,##0.00")]), "1,234.50");
        assert_text(call_text(vec![number(0.285), text("0.0%")]), "28.5%");
        assert_text(
            call_text(vec![number(1234.5), text("$#,##0.00")]),
            "$1,234.50",
        );
        assert_text(call_text(vec![number(-12.3), text("$0.00")]), "-$12.30");
    }

    #[test]
    fn formats_dates_with_excel_1900_serials() {
        assert_text(
            call_text(vec![number(45_356.0), text("yyyy-mm-dd")]),
            "2024-03-05",
        );
        assert_text(
            call_text(vec![number(45_356.75), text("mmm d, yyyy")]),
            "Mar 5, 2024",
        );
        assert_text(call_text(vec![number(60.0), text("mmmm d, yyyy")]), "February 29, 1900");
    }

    #[test]
    fn date_and_time_format_codes_are_case_insensitive() {
        assert_text(
            call_text(vec![number(45_356.0), text("YYYY-MM-DD")]),
            "2024-03-05",
        );
        assert_text(call_text(vec![number(0.78125), text("h:mm am/pm")]), "6:45 PM");
    }

    #[test]
    fn rejects_dates_before_or_outside_supported_range() {
        assert_eq!(
            call_text(vec![number(0.999), text("m/d/yyyy")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_text(vec![number(3_000_000.0), text("m/d/yyyy")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn formats_times_from_fractional_days() {
        assert_text(call_text(vec![number(0.78125), text("h:mm AM/PM")]), "6:45 PM");
        assert_text(
            call_text(vec![number(21_907.0 / 86_400.0), text("hh:mm:ss")]),
            "06:05:07",
        );
        assert_text(
            call_text(vec![number(1.999_995), text("hh:mm:ss")]),
            "00:00:00",
        );
    }

    #[test]
    fn rejects_negative_time_serials() {
        assert_eq!(
            call_text(vec![number(-0.25), text("h:mm")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_non_finite_values_and_unsupported_formats() {
        assert_eq!(
            call_text(vec![number(f64::INFINITY), text("0.00")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_text(vec![number(f64::NAN), text("0.00")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_text(vec![number(1.0), text("General")]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_text(vec![number(1.0), text("0.00;[Red]-0.00")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn coerces_arguments_and_propagates_source_errors() {
        assert_text(call_text(vec![text("12.3"), text("$0.00")]), "$12.30");
        assert_text(call_text(vec![number(12.3), number(0.0)]), "12");
        assert_eq!(
            call_text(vec![Value::Error(ErrorValue::Ref), text("0")]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_text(vec![number(1.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_text(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        TEXT.call(&args, &fn_ctx)
    }

    fn number(value: f64) -> Value {
        Value::Number(value)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn assert_text(actual: Value, expected: &str) {
        assert_eq!(actual, Value::Text(expected.to_string()));
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
            CellId {
                sheet: 0,
                coord: Coord { row: 0, col: 0 },
            }
        }
    }
}
