use crate::functions::prelude::*;
use crate::model::date::{serial_to_ymd, ymd_to_serial};

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;

pub struct Date;

impl Function for Date {
    fn name(&self) -> &'static str {
        "DATE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 3 {
            return Value::Error(ErrorValue::Value);
        }

        match date_serial(args, ctx) {
            Ok(serial) => Value::Number(serial),
            Err(error) => Value::Error(error),
        }
    }
}

fn date_serial(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let year = date_argument(&args[0], ctx)?;
    let month = date_argument(&args[1], ctx)?;
    let day = date_argument(&args[2], ctx)?;

    let year = normalize_year(year)?;
    let (year, month) = normalize_year_month(year, month)?;
    let first_of_month = ymd_to_serial(year, month, 1, ctx.date_system());
    let day_offset = day.checked_sub(1).ok_or(ErrorValue::Num)?;
    let serial = first_of_month + day_offset as f64;

    validate_supported_serial(serial, ctx.date_system())
}

fn date_argument(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    if !number.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = number.trunc();
    if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
        return Err(ErrorValue::Num);
    }

    Ok(truncated as i64)
}

fn normalize_year(year: i64) -> Result<i32, ErrorValue> {
    match year {
        0..=1899 => Ok((year + 1900) as i32),
        1900..=9999 => Ok(year as i32),
        _ => Err(ErrorValue::Num),
    }
}

fn normalize_year_month(year: i32, month: i64) -> Result<(i32, u32), ErrorValue> {
    let zero_based_month = month.checked_sub(1).ok_or(ErrorValue::Num)?;
    let months = i64::from(year)
        .checked_mul(12)
        .and_then(|base| base.checked_add(zero_based_month))
        .ok_or(ErrorValue::Num)?;
    let normalized_year = months.div_euclid(12);
    let normalized_month = months.rem_euclid(12) + 1;

    if normalized_year < i64::from(i32::MIN) || normalized_year > i64::from(i32::MAX) {
        return Err(ErrorValue::Num);
    }

    Ok((normalized_year as i32, normalized_month as u32))
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

static DATE: Date = Date;
inventory::submit! { FunctionEntry(&DATE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_three_argument_arity() {
        assert_eq!(DATE.name(), "DATE");
        assert_eq!(DATE.arity(), (3, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_date(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_date(vec![Value::Number(2020.0), Value::Number(1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_excel_1900_serials() {
        assert_eq!(
            call_date(vec![
                Value::Number(1900.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Number(43_831.0)
        );
    }

    #[test]
    fn preserves_excel_1900_phantom_day() {
        assert_eq!(
            call_date(vec![
                Value::Number(1900.0),
                Value::Number(2.0),
                Value::Number(29.0),
            ]),
            Value::Number(60.0)
        );
    }

    #[test]
    fn month_overflow_and_underflow_adjust_the_year() {
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Number(13.0),
                Value::Number(1.0),
            ]),
            Value::Number(44_197.0)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Number(43_800.0)
        );
    }

    #[test]
    fn day_overflow_and_underflow_use_serial_arithmetic() {
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Number(43_830.0)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(2024.0),
                Value::Number(2.0),
                Value::Number(30.0),
            ]),
            Value::Number(45_352.0)
        );

        let march = call_date(vec![
            Value::Number(2024.0),
            Value::Number(3.0),
            Value::Number(1.0),
        ]);
        let february = call_date(vec![
            Value::Number(2024.0),
            Value::Number(2.0),
            Value::Number(1.0),
        ]);
        assert_eq!(number(march) - number(february), 29.0);
    }

    #[test]
    fn applies_year_shorthand_and_truncates_fractional_arguments_toward_zero() {
        assert_eq!(
            call_date(vec![
                Value::Number(120.9),
                Value::Number(1.9),
                Value::Number(1.9),
            ]),
            Value::Number(43_831.0)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(-0.9),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Number(1.0)
        );
    }

    #[test]
    fn coerces_scalar_top_left_values_left_to_right() {
        assert_eq!(
            call_date(vec![
                Value::Text(" 2020 ".to_string()),
                Value::Boolean(true),
                Value::Text("1".to_string()),
            ]),
            Value::Number(43_831.0)
        );

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
            DATE.call(
                &[
                    Arg::Range(ctx.range_view(range)),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                ],
                &fn_ctx,
            ),
            Value::Number(43_831.0)
        );
    }

    #[test]
    fn rejects_non_finite_arguments_and_out_of_range_results() {
        assert_eq!(
            call_date(vec![
                Value::Number(f64::INFINITY),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Number(f64::NAN),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(10_000.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(1900.0),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(9999.0),
                Value::Number(12.0),
                Value::Number(32.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_date(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_date(vec![
                Value::Number(2020.0),
                Value::Error(ErrorValue::Ref),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_date(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DATE.call(&args, &fn_ctx)
    }

    fn number(value: Value) -> f64 {
        match value {
            Value::Number(number) => number,
            other => panic!("expected number, got {other:?}"),
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
                Value::Number(2020.0)
            } else {
                Value::Number(1999.0)
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
