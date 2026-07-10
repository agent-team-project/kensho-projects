use crate::functions::prelude::*;

const MAX_EXCEL_1900_SERIAL: f64 = 2_958_465.0;

pub struct Weekday;

impl Function for Weekday {
    fn name(&self) -> &'static str {
        "WEEKDAY"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(1..=2).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match weekday(args, ctx) {
            Ok(weekday) => Value::Number(weekday as f64),
            Err(error) => Value::Error(error),
        }
    }
}

fn weekday(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let serial = ctx.to_serial_date(&args[0].as_value())?;
    let return_type = match args.get(1) {
        Some(arg) => ctx.to_number(&arg.as_value())?,
        None => 1.0,
    };

    let serial_day = serial_day(serial)?;
    let style = return_style(return_type)?;
    Ok(style.weekday(serial_day))
}

fn serial_day(serial: f64) -> Result<i64, ErrorValue> {
    if !serial.is_finite() {
        return Err(ErrorValue::Num);
    }

    let day = serial.trunc();
    if !(1.0..=MAX_EXCEL_1900_SERIAL).contains(&day) {
        return Err(ErrorValue::Num);
    }

    Ok(day as i64)
}

fn return_style(return_type: f64) -> Result<ReturnStyle, ErrorValue> {
    if !return_type.is_finite() {
        return Err(ErrorValue::Num);
    }

    let truncated = return_type.trunc();
    if truncated < i32::MIN as f64 || truncated > i32::MAX as f64 {
        return Err(ErrorValue::Num);
    }

    match truncated as i32 {
        1 | 17 => Ok(ReturnStyle::OneBased { first_day: 1 }),
        2 | 11 => Ok(ReturnStyle::OneBased { first_day: 2 }),
        3 => Ok(ReturnStyle::ZeroBasedMonday),
        12..=16 => Ok(ReturnStyle::OneBased {
            first_day: i64::from(truncated as i32 - 9),
        }),
        _ => Err(ErrorValue::Num),
    }
}

enum ReturnStyle {
    OneBased { first_day: i64 },
    ZeroBasedMonday,
}

impl ReturnStyle {
    fn weekday(&self, serial_day: i64) -> i64 {
        let sunday_based = sunday_based_weekday(serial_day);
        match self {
            Self::OneBased { first_day } => (sunday_based - first_day).rem_euclid(7) + 1,
            Self::ZeroBasedMonday => (sunday_based - 2).rem_euclid(7),
        }
    }
}

fn sunday_based_weekday(serial_day: i64) -> i64 {
    (serial_day - 1).rem_euclid(7) + 1
}

static WEEKDAY: Weekday = Weekday;
inventory::submit! { FunctionEntry(&WEEKDAY) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_one_or_two_argument_arity() {
        assert_eq!(WEEKDAY.name(), "WEEKDAY");
        assert_eq!(WEEKDAY.arity(), (1, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_weekday(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_weekday(vec![
                Value::Number(39_492.0),
                Value::Number(1.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_documented_microsoft_examples() {
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0)]),
            Value::Number(5.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(2.0)]),
            Value::Number(4.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(3.0)]),
            Value::Number(3.0)
        );
    }

    #[test]
    fn supports_all_excel_return_type_families() {
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(11.0)]),
            Value::Number(4.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(12.0)]),
            Value::Number(3.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(13.0)]),
            Value::Number(2.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(14.0)]),
            Value::Number(1.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(15.0)]),
            Value::Number(7.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(16.0)]),
            Value::Number(6.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(17.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn preserves_excel_1900_serial_weekday_anchors() {
        assert_eq!(call_weekday(vec![Value::Number(1.0)]), Value::Number(1.0));
        assert_eq!(
            call_weekday(vec![Value::Number(60.0)]),
            Value::Number(4.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(61.0)]),
            Value::Number(5.0)
        );
    }

    #[test]
    fn truncates_serial_and_return_type_toward_zero() {
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.75)]),
            Value::Number(5.0)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(2.9)]),
            Value::Number(4.0)
        );
    }

    #[test]
    fn uses_scalar_top_left_values_and_coercion() {
        assert_eq!(
            call_weekday(vec![Value::Text(" 39492 ".to_string())]),
            Value::Number(5.0)
        );
        assert_eq!(call_weekday(vec![Value::Boolean(true)]), Value::Number(1.0));

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
            WEEKDAY.call(
                &[
                    Arg::Range(ctx.range_view(serial_range)),
                    Arg::Range(ctx.range_view(return_type_range)),
                ],
                &fn_ctx,
            ),
            Value::Number(4.0)
        );
    }

    #[test]
    fn rejects_invalid_serials() {
        assert_eq!(
            call_weekday(vec![Value::Number(0.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(0.9)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(-1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(2_958_466.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_return_types() {
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(0.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(10.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(18.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_weekday(vec![Value::Number(39_492.0), Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_weekday(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_weekday(vec![
                Value::Number(39_492.0),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_weekday(vec![Value::Error(ErrorValue::Ref), Value::Text("bad".to_string())]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_weekday(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        WEEKDAY.call(&args, &fn_ctx)
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
                (0, 0) => Value::Number(39_492.0),
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
