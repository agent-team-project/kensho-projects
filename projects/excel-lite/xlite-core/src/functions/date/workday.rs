use std::collections::HashSet;

use crate::functions::prelude::*;
use crate::model::date::serial_to_ymd;

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const MAX_EXCEL_1900_SERIAL: i64 = 2_958_465;

pub struct Workday;

impl Function for Workday {
    fn name(&self) -> &'static str {
        "WORKDAY"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match workday_serial(args, ctx) {
            Ok(serial) => Value::Number(serial),
            Err(error) => Value::Error(error),
        }
    }
}

fn workday_serial(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let start_day = serial_date_arg(&args[0], ctx)?;
    let offset = day_offset(ctx.to_number(&args[1].as_value())?)?;
    let holidays = match args.get(2) {
        Some(arg) => holiday_days(arg, ctx)?,
        None => HashSet::new(),
    };

    if offset == 0 {
        return Ok(start_day as f64);
    }

    let direction = if offset > 0 { 1 } else { -1 };
    let mut current_day = start_day;
    let mut remaining = offset.unsigned_abs();

    while remaining > 0 {
        current_day = current_day.checked_add(direction).ok_or(ErrorValue::Num)?;
        validate_result_day(current_day, ctx.date_system())?;

        if is_workday(current_day) && !holidays.contains(&current_day) {
            remaining -= 1;
        }
    }

    Ok(current_day as f64)
}

fn serial_date_arg(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let serial = ctx.to_serial_date(&arg.as_value())?;
    supported_serial_day(serial, ctx.date_system(), ErrorValue::Value)
}

fn day_offset(days: f64) -> Result<i64, ErrorValue> {
    if !days.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = days.trunc();
    if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
        return Err(ErrorValue::Num);
    }

    Ok(truncated as i64)
}

fn holiday_days(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<HashSet<i64>, ErrorValue> {
    let mut days = HashSet::new();
    match arg {
        Arg::Value(value) => add_holiday(value, ctx, &mut days)?,
        Arg::Range(range) => {
            for value in range.iter() {
                add_holiday(&value, ctx, &mut days)?;
            }
        }
    }
    Ok(days)
}

fn add_holiday(
    value: &Value,
    ctx: &FnContext<'_>,
    days: &mut HashSet<i64>,
) -> Result<(), ErrorValue> {
    let serial = ctx.to_serial_date(value)?;
    let day = supported_serial_day(serial, ctx.date_system(), ErrorValue::Value)?;
    if is_workday(day) {
        days.insert(day);
    }
    Ok(())
}

fn validate_result_day(day: i64, date_system: DateSystem) -> Result<(), ErrorValue> {
    supported_serial_day(day as f64, date_system, ErrorValue::Num).map(|_| ())
}

fn supported_serial_day(
    serial: f64,
    date_system: DateSystem,
    error: ErrorValue,
) -> Result<i64, ErrorValue> {
    if !serial.is_finite() {
        return Err(error);
    }

    let day = serial.floor();
    if day < 1.0 || day > MAX_EXCEL_1900_SERIAL as f64 {
        return Err(error);
    }

    let (year, _, _) = serial_to_ymd(day, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(error);
    }

    Ok(day as i64)
}

fn is_workday(serial_day: i64) -> bool {
    !matches!(sunday_based_weekday(serial_day), 1 | 7)
}

fn sunday_based_weekday(serial_day: i64) -> i64 {
    (serial_day - 1).rem_euclid(7) + 1
}

static WORKDAY: Workday = Workday;
inventory::submit! { FunctionEntry(&WORKDAY) }

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        eval::EvalContext,
        model::{date::ymd_to_serial, Coord},
        syntax::{CellRef, RangeRef},
    };

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_two_or_three_argument_arity() {
        assert_eq!(WORKDAY.name(), "WORKDAY");
        assert_eq!(WORKDAY.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_values(vec![Value::Number(serial(2020, 1, 1))]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(1.0),
                Value::Number(serial(2020, 1, 2)),
                Value::Number(serial(2020, 1, 3)),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_documented_microsoft_examples() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2008, 10, 1)),
                Value::Number(151.0),
            ]),
            Value::Number(serial(2009, 4, 30))
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2008, 11, 26))),
            ((1, 0), Value::Number(serial(2008, 12, 4))),
            ((2, 0), Value::Number(serial(2009, 1, 21))),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            WORKDAY.call(
                &[
                    Arg::Value(Value::Number(serial(2008, 10, 1))),
                    Arg::Value(Value::Number(151.0)),
                    range_arg(&ctx, (0, 0), (2, 0)),
                ],
                &fn_ctx,
            ),
            Value::Number(serial(2009, 5, 5))
        );
    }

    #[test]
    fn moves_forward_and_backward_skipping_weekends() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 10)),
                Value::Number(1.0),
            ]),
            Value::Number(serial(2020, 1, 13))
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(-1.0),
            ]),
            Value::Number(serial(2020, 1, 3))
        );
    }

    #[test]
    fn truncates_fractional_days_and_returns_start_for_zero_days() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(1.9),
            ]),
            Value::Number(serial(2020, 1, 7))
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(-1.9),
            ]),
            Value::Number(serial(2020, 1, 3))
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(0.0),
            ]),
            Value::Number(serial(2020, 1, 6))
        );
    }

    #[test]
    fn applies_holidays_once_only_when_they_are_workdays() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 10)),
                Value::Number(1.0),
                Value::Number(serial(2020, 1, 11)),
            ]),
            Value::Number(serial(2020, 1, 13))
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(3.0),
                Value::Number(serial(2020, 1, 8)),
            ]),
            Value::Number(serial(2020, 1, 10))
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2020, 1, 8))),
            ((0, 1), Value::Number(serial(2020, 1, 8))),
            ((1, 0), Value::Number(serial(2020, 1, 11))),
            ((1, 1), Value::Number(serial(2020, 2, 1))),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            WORKDAY.call(
                &[
                    Arg::Value(Value::Number(serial(2020, 1, 6))),
                    Arg::Value(Value::Number(3.0)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(serial(2020, 1, 10))
        );
    }

    #[test]
    fn scans_holiday_ranges_in_row_major_order() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2020, 1, 7))),
            ((0, 1), Value::Text("bad date".to_string())),
            ((1, 0), Value::Error(ErrorValue::Div0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            WORKDAY.call(
                &[
                    Arg::Value(Value::Number(serial(2020, 1, 6))),
                    Arg::Value(Value::Number(1.0)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn uses_date_portions_and_serial_date_coercion() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 10) + 0.75),
                Value::Text("1.9".to_string()),
            ]),
            Value::Number(serial(2020, 1, 13))
        );
        assert_eq!(
            call_values(vec![
                Value::Text(format!("{}", serial(2020, 1, 6))),
                Value::Text("3".to_string()),
                Value::Text(format!("{}", serial(2020, 1, 8) + 0.9)),
            ]),
            Value::Number(serial(2020, 1, 10))
        );
        assert_eq!(
            call_values(vec![Value::Boolean(true), Value::Number(1.0)]),
            Value::Number(2.0)
        );
    }

    #[test]
    fn uses_top_left_scalar_values_for_start_and_days_ranges() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2020, 1, 10))),
            ((0, 1), Value::Number(serial(2020, 1, 11))),
            ((0, 2), Value::Number(1.0)),
            ((0, 3), Value::Number(10.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            WORKDAY.call(
                &[
                    range_arg(&ctx, (0, 0), (0, 1)),
                    range_arg(&ctx, (0, 2), (0, 3)),
                ],
                &fn_ctx,
            ),
            Value::Number(serial(2020, 1, 13))
        );
    }

    #[test]
    fn rejects_invalid_start_and_holiday_dates_with_value() {
        for invalid in [
            Value::Number(0.0),
            Value::Number(0.9),
            Value::Number(-1.0),
            Value::Number(MAX_EXCEL_1900_SERIAL as f64 + 1.0),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NAN),
        ] {
            assert_eq!(
                call_values(vec![
                    invalid.clone(),
                    Value::Number(1.0),
                    Value::Number(serial(2020, 1, 8)),
                ]),
                Value::Error(ErrorValue::Value)
            );
            assert_eq!(
                call_values(vec![
                    Value::Number(serial(2020, 1, 6)),
                    Value::Number(1.0),
                    invalid,
                ]),
                Value::Error(ErrorValue::Value)
            );
        }
    }

    #[test]
    fn returns_num_for_out_of_range_results_and_nonfinite_days() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(9999, 12, 31)),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![Value::Number(1.0), Value::Number(-1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![Value::Number(1.0), Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn preserves_excel_1900_weekday_anchors() {
        assert_eq!(
            call_values(vec![Value::Number(1.0), Value::Number(1.0)]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(6.0), Value::Number(1.0)]),
            Value::Number(9.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(9.0), Value::Number(-1.0)]),
            Value::Number(6.0)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_values(vec![
                Value::Text("2020-01-01".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Error(ErrorValue::Ref),
                Value::Text("bad days".to_string()),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Error(ErrorValue::Div0),
                Value::Error(ErrorValue::Ref),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(1.0),
                Value::Error(ErrorValue::Ref),
            ]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        WORKDAY.call(&args, &fn_ctx)
    }

    fn range_arg(ctx: &TestContext, start: (u32, u32), end: (u32, u32)) -> Arg<'_> {
        Arg::Range(RangeView::new(
            ctx,
            RangeRef {
                start: cell_ref(start),
                end: cell_ref(end),
            },
        ))
    }

    fn cell_ref((row, col): (u32, u32)) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    fn serial(year: i32, month: u32, day: u32) -> f64 {
        ymd_to_serial(year, month, day, SYS)
    }

    #[derive(Default)]
    struct TestContext {
        cells: HashMap<(u32, u32), Value>,
    }

    impl TestContext {
        fn with_cells(cells: Vec<((u32, u32), Value)>) -> Self {
            Self {
                cells: cells.into_iter().collect(),
            }
        }
    }

    impl EvalContext for TestContext {
        fn cell_value(&self, r: CellRef) -> Value {
            self.cells
                .get(&(r.row, r.col))
                .cloned()
                .unwrap_or(Value::Blank)
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
