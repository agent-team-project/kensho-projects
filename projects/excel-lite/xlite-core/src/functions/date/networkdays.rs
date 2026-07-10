use std::collections::HashSet;

use crate::functions::prelude::*;
use crate::model::date::serial_to_ymd;

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const MAX_EXCEL_1900_SERIAL: f64 = 2_958_465.0;

pub struct Networkdays;

impl Function for Networkdays {
    fn name(&self) -> &'static str {
        "NETWORKDAYS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match networkdays(args, ctx) {
            Ok(days) => Value::Number(days as f64),
            Err(error) => Value::Error(error),
        }
    }
}

fn networkdays(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let start_day = serial_date_arg(&args[0], ctx)?;
    let end_day = serial_date_arg(&args[1], ctx)?;

    let (lower, upper, sign) = if start_day <= end_day {
        (start_day, end_day, 1)
    } else {
        (end_day, start_day, -1)
    };

    let mut count = workday_count_inclusive(lower, upper);
    if let Some(holiday_arg) = args.get(2) {
        count -= holiday_days(holiday_arg, ctx, lower, upper)?.len() as i64;
    }

    Ok(count * sign)
}

fn serial_date_arg(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let serial = ctx.to_serial_date(&arg.as_value())?;
    supported_serial_day(serial, ctx.date_system())
}

fn supported_serial_day(serial: f64, date_system: DateSystem) -> Result<i64, ErrorValue> {
    if !serial.is_finite() {
        return Err(ErrorValue::Value);
    }

    let day = serial.floor();
    if !(1.0..=MAX_EXCEL_1900_SERIAL).contains(&day) {
        return Err(ErrorValue::Value);
    }

    let (year, _, _) = serial_to_ymd(day, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(ErrorValue::Value);
    }

    Ok(day as i64)
}

fn workday_count_inclusive(start_day: i64, end_day: i64) -> i64 {
    let days = end_day - start_day + 1;
    let mut count = (days / 7) * 5;
    for offset in 0..(days % 7) {
        if is_workday(start_day + offset) {
            count += 1;
        }
    }
    count
}

fn holiday_days(
    arg: &Arg<'_>,
    ctx: &FnContext<'_>,
    lower: i64,
    upper: i64,
) -> Result<HashSet<i64>, ErrorValue> {
    let mut days = HashSet::new();
    match arg {
        Arg::Value(value) => add_holiday(value, ctx, lower, upper, &mut days)?,
        Arg::Range(range) => {
            for value in range.iter() {
                add_holiday(&value, ctx, lower, upper, &mut days)?;
            }
        }
    }
    Ok(days)
}

fn add_holiday(
    value: &Value,
    ctx: &FnContext<'_>,
    lower: i64,
    upper: i64,
    days: &mut HashSet<i64>,
) -> Result<(), ErrorValue> {
    let serial = ctx.to_serial_date(value)?;
    let day = supported_serial_day(serial, ctx.date_system())?;
    if (lower..=upper).contains(&day) && is_workday(day) {
        days.insert(day);
    }
    Ok(())
}

fn is_workday(serial_day: i64) -> bool {
    !matches!(sunday_based_weekday(serial_day), 1 | 7)
}

fn sunday_based_weekday(serial_day: i64) -> i64 {
    (serial_day - 1).rem_euclid(7) + 1
}

static NETWORKDAYS: Networkdays = Networkdays;
inventory::submit! { FunctionEntry(&NETWORKDAYS) }

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
        assert_eq!(NETWORKDAYS.name(), "NETWORKDAYS");
        assert_eq!(NETWORKDAYS.arity(), (2, Some(3)));
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
                Value::Number(serial(2020, 1, 2)),
                Value::Number(serial(2020, 1, 3)),
                Value::Number(serial(2020, 1, 4)),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_documented_microsoft_examples() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2012, 10, 1)),
                Value::Number(serial(2013, 3, 1)),
            ]),
            Value::Number(110.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2012, 10, 1)),
                Value::Number(serial(2013, 3, 1)),
                Value::Number(serial(2012, 11, 22)),
            ]),
            Value::Number(109.0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2012, 11, 22))),
            ((1, 0), Value::Number(serial(2012, 12, 4))),
            ((2, 0), Value::Number(serial(2013, 1, 21))),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            NETWORKDAYS.call(
                &[
                    Arg::Value(Value::Number(serial(2012, 10, 1))),
                    Arg::Value(Value::Number(serial(2013, 3, 1))),
                    range_arg(&ctx, (0, 0), (2, 0)),
                ],
                &fn_ctx,
            ),
            Value::Number(107.0)
        );
    }

    #[test]
    fn counts_same_day_and_reverse_intervals() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(serial(2020, 1, 6)),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 5)),
                Value::Number(serial(2020, 1, 5)),
            ]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 10)),
                Value::Number(serial(2020, 1, 6)),
            ]),
            Value::Number(-5.0)
        );
    }

    #[test]
    fn preserves_excel_1900_weekday_anchors() {
        assert_eq!(
            call_values(vec![Value::Number(1.0), Value::Number(1.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(7.0), Value::Number(7.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(8.0), Value::Number(8.0)]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_values(vec![Value::Number(2.0), Value::Number(6.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn applies_holidays_once_only_when_they_are_workdays_inside_the_interval() {
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(serial(2020, 1, 10)),
                Value::Number(serial(2020, 1, 8)),
            ]),
            Value::Number(4.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 6)),
                Value::Number(serial(2020, 1, 10)),
                Value::Number(serial(2020, 1, 11)),
            ]),
            Value::Number(5.0)
        );

        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2020, 1, 7))),
            ((0, 1), Value::Number(serial(2020, 1, 7))),
            ((1, 0), Value::Number(serial(2020, 1, 11))),
            ((1, 1), Value::Number(serial(2020, 2, 1))),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            NETWORKDAYS.call(
                &[
                    Arg::Value(Value::Number(serial(2020, 1, 6))),
                    Arg::Value(Value::Number(serial(2020, 1, 10))),
                    range_arg(&ctx, (0, 0), (1, 1)),
                ],
                &fn_ctx,
            ),
            Value::Number(4.0)
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
            NETWORKDAYS.call(
                &[
                    Arg::Value(Value::Number(serial(2020, 1, 6))),
                    Arg::Value(Value::Number(serial(2020, 1, 10))),
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
                Value::Number(serial(2020, 1, 6) + 0.25),
                Value::Number(serial(2020, 1, 6) + 0.75),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_values(vec![
                Value::Text(format!("{}", serial(2020, 1, 6))),
                Value::Text(format!("{}", serial(2020, 1, 10))),
                Value::Text(format!("{}", serial(2020, 1, 8) + 0.9)),
            ]),
            Value::Number(4.0)
        );
        assert_eq!(
            call_values(vec![Value::Boolean(true), Value::Number(2.0)]),
            Value::Number(1.0)
        );
    }

    #[test]
    fn uses_top_left_scalar_values_for_start_and_end_ranges() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(serial(2020, 1, 6))),
            ((0, 1), Value::Number(serial(2020, 1, 7))),
            ((0, 2), Value::Number(serial(2020, 1, 10))),
            ((0, 3), Value::Number(serial(2020, 1, 11))),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            NETWORKDAYS.call(
                &[
                    range_arg(&ctx, (0, 0), (0, 1)),
                    range_arg(&ctx, (0, 2), (0, 3)),
                ],
                &fn_ctx,
            ),
            Value::Number(5.0)
        );
    }

    #[test]
    fn rejects_invalid_start_end_and_holiday_dates_with_value() {
        for invalid in [
            Value::Number(0.0),
            Value::Number(0.9),
            Value::Number(-1.0),
            Value::Number(MAX_EXCEL_1900_SERIAL + 1.0),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NAN),
        ] {
            assert_eq!(
                call_values(vec![
                    invalid.clone(),
                    Value::Number(serial(2020, 1, 1)),
                ]),
                Value::Error(ErrorValue::Value)
            );
            assert_eq!(
                call_values(vec![
                    Value::Number(serial(2020, 1, 1)),
                    invalid.clone(),
                ]),
                Value::Error(ErrorValue::Value)
            );
            assert_eq!(
                call_values(vec![
                    Value::Number(serial(2020, 1, 1)),
                    Value::Number(serial(2020, 1, 2)),
                    invalid,
                ]),
                Value::Error(ErrorValue::Value)
            );
        }
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
                Value::Text("2020-01-01".to_string()),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Text("2020-01-02".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2020, 1, 2)),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        NETWORKDAYS.call(&args, &fn_ctx)
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
