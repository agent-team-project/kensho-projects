use crate::functions::prelude::*;
use crate::model::date::serial_to_ymd;

const MIN_SUPPORTED_YEAR: i32 = 1900;
const MAX_SUPPORTED_YEAR: i32 = 9999;
const MIN_SUPPORTED_SERIAL: f64 = 1.0;
const MAX_SUPPORTED_SERIAL: f64 = 2_958_465.0;

pub struct Datedif;

impl Function for Datedif {
    fn name(&self) -> &'static str {
        "DATEDIF"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 3 {
            return Value::Error(ErrorValue::Value);
        }

        match datedif(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn datedif(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let start_serial = ctx.to_serial_date(&args[0].as_value())?;
    let end_serial = ctx.to_serial_date(&args[1].as_value())?;
    let unit = ctx.to_text(&args[2].as_value())?;

    let start = supported_date(start_serial, ctx.date_system())?;
    let end = supported_date(end_serial, ctx.date_system())?;
    if start.serial_day > end.serial_day {
        return Err(ErrorValue::Num);
    }

    let result = if unit.eq_ignore_ascii_case("Y") {
        complete_years(&start, &end)
    } else if unit.eq_ignore_ascii_case("M") {
        complete_months(&start, &end)
    } else if unit.eq_ignore_ascii_case("D") {
        (end.serial_day - start.serial_day) as i32
    } else {
        return Err(ErrorValue::Num);
    };

    Ok(f64::from(result))
}

#[derive(Clone, Copy)]
struct DateParts {
    serial_day: f64,
    year: i32,
    month: u32,
    day: u32,
}

fn supported_date(serial: f64, date_system: DateSystem) -> Result<DateParts, ErrorValue> {
    if !serial.is_finite() || serial <= 0.0 {
        return Err(ErrorValue::Num);
    }

    let serial_day = serial.floor();
    if !(MIN_SUPPORTED_SERIAL..=MAX_SUPPORTED_SERIAL).contains(&serial_day) {
        return Err(ErrorValue::Num);
    }

    let (year, month, day) = serial_to_ymd(serial_day, date_system);
    if !(MIN_SUPPORTED_YEAR..=MAX_SUPPORTED_YEAR).contains(&year) {
        return Err(ErrorValue::Num);
    }

    Ok(DateParts {
        serial_day,
        year,
        month,
        day,
    })
}

fn complete_years(start: &DateParts, end: &DateParts) -> i32 {
    let mut years = end.year - start.year;
    if (end.month, end.day) < (start.month, start.day) {
        years -= 1;
    }

    years
}

fn complete_months(start: &DateParts, end: &DateParts) -> i32 {
    let mut months = (end.year - start.year) * 12 + end.month as i32 - start.month as i32;
    if end.day < start.day {
        months -= 1;
    }

    months
}

static DATEDIF: Datedif = Datedif;
inventory::submit! { FunctionEntry(&DATEDIF) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::model::date::ymd_to_serial;
    use crate::syntax::{CellRef, RangeRef};

    const SYS: DateSystem = DateSystem::Excel1900;

    #[test]
    fn reports_exact_three_argument_arity() {
        assert_eq!(DATEDIF.name(), "DATEDIF");
        assert_eq!(DATEDIF.arity(), (3, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_datedif(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2020, 1, 2)),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2020, 1, 2)),
                Value::Text("D".to_string()),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_whole_day_difference_from_date_portions() {
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2001, 6, 1)),
                Value::Number(serial(2002, 8, 15)),
                Value::Text("D".to_string()),
            ]),
            Value::Number(440.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1) + 0.75),
                Value::Number(serial(2020, 1, 2) + 0.25),
                Value::Text("D".to_string()),
            ]),
            Value::Number(1.0)
        );
    }

    #[test]
    fn returns_complete_months() {
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(serial(2020, 2, 29)),
                Value::Text("M".to_string()),
            ]),
            Value::Number(0.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 31)),
                Value::Number(serial(2020, 3, 31)),
                Value::Text("M".to_string()),
            ]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 15)),
                Value::Number(serial(2021, 3, 14)),
                Value::Text("M".to_string()),
            ]),
            Value::Number(13.0)
        );
    }

    #[test]
    fn returns_complete_years_before_and_on_anniversary_dates() {
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2001, 1, 1)),
                Value::Number(serial(2003, 1, 1)),
                Value::Text("Y".to_string()),
            ]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 6, 15)),
                Value::Number(serial(2023, 6, 14)),
                Value::Text("Y".to_string()),
            ]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 6, 15)),
                Value::Number(serial(2023, 6, 15)),
                Value::Text("Y".to_string()),
            ]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn returns_zero_for_same_start_and_end_date() {
        for unit in ["Y", "M", "D"] {
            assert_eq!(
                call_datedif(vec![
                    Value::Number(serial(2020, 1, 31)),
                    Value::Number(serial(2020, 1, 31)),
                    Value::Text(unit.to_string()),
                ]),
                Value::Number(0.0)
            );
        }
    }

    #[test]
    fn accepts_supported_units_case_insensitively() {
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2021, 1, 1)),
                Value::Text("y".to_string()),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2020, 2, 1)),
                Value::Text("m".to_string()),
            ]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2020, 1, 2)),
                Value::Text("d".to_string()),
            ]),
            Value::Number(1.0)
        );
    }

    #[test]
    fn uses_scalar_top_left_values_and_coercions() {
        assert_eq!(
            call_datedif(vec![
                Value::Text(" 43831 ".to_string()),
                Value::Text("43862".to_string()),
                Value::Text("M".to_string()),
            ]),
            Value::Number(1.0)
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
        let end_range = RangeRef {
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
        let unit_range = RangeRef {
            start: CellRef {
                sheet: None,
                col: 4,
                row: 0,
                col_abs: false,
                row_abs: false,
            },
            end: CellRef {
                sheet: None,
                col: 5,
                row: 0,
                col_abs: false,
                row_abs: false,
            },
        };

        assert_eq!(
            DATEDIF.call(
                &[
                    Arg::Range(ctx.range_view(start_range)),
                    Arg::Range(ctx.range_view(end_range)),
                    Arg::Range(ctx.range_view(unit_range)),
                ],
                &fn_ctx,
            ),
            Value::Number(1.0)
        );
    }

    #[test]
    fn rejects_unsupported_units() {
        for unit in ["MD", "YM", "YD", "Q", ""] {
            assert_eq!(
                call_datedif(vec![
                    Value::Number(serial(2020, 1, 1)),
                    Value::Number(serial(2021, 1, 1)),
                    Value::Text(unit.to_string()),
                ]),
                Value::Error(ErrorValue::Num)
            );
        }
    }

    #[test]
    fn rejects_invalid_dates_and_reversed_ranges() {
        for value in [
            Value::Number(0.0),
            Value::Number(0.9),
            Value::Number(-1.0),
            Value::Number(MAX_SUPPORTED_SERIAL + 1.0),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NAN),
        ] {
            assert_eq!(
                call_datedif(vec![
                    value.clone(),
                    Value::Number(serial(2020, 1, 1)),
                    Value::Text("D".to_string()),
                ]),
                Value::Error(ErrorValue::Num)
            );
            assert_eq!(
                call_datedif(vec![
                    Value::Number(serial(2020, 1, 1)),
                    value,
                    Value::Text("D".to_string()),
                ]),
                Value::Error(ErrorValue::Num)
            );
        }

        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 2)),
                Value::Number(serial(2020, 1, 1)),
                Value::Text("D".to_string()),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_first_coercion_error_left_to_right_before_semantic_checks() {
        assert_eq!(
            call_datedif(vec![
                Value::Text("2020-01-01".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Text("D".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(0.0),
                Value::Error(ErrorValue::Div0),
                Value::Text("D".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Error(ErrorValue::Ref),
                Value::Text("D".to_string()),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(serial(2020, 1, 1)),
                Value::Number(serial(2020, 1, 2)),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn preserves_excel_1900_serial_semantics() {
        assert_eq!(
            call_datedif(vec![
                Value::Number(59.0),
                Value::Number(61.0),
                Value::Text("D".to_string()),
            ]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_datedif(vec![
                Value::Number(60.0),
                Value::Number(serial(1900, 3, 29)),
                Value::Text("M".to_string()),
            ]),
            Value::Number(1.0)
        );
    }

    fn call_datedif(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DATEDIF.call(&args, &fn_ctx)
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
                (0, 0) => Value::Number(serial(2020, 1, 1) + 0.75),
                (0, 2) => Value::Text("43862".to_string()),
                (0, 4) => Value::Text("m".to_string()),
                _ => Value::Error(ErrorValue::Ref),
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
