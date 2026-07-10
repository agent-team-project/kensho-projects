use crate::functions::prelude::*;
use crate::model::date::serial_to_ymd;

pub struct Day;

impl Function for Day {
    fn name(&self) -> &'static str {
        "DAY"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        let serial = match ctx.to_serial_date(&args[0].as_value()) {
            Ok(serial) => serial,
            Err(error) => return Value::Error(error),
        };

        if !serial.is_finite() || serial <= 0.0 {
            return Value::Error(ErrorValue::Num);
        }

        let (_, _, day) = serial_to_ymd(serial, ctx.date_system());
        Value::Number(day as f64)
    }
}

static DAY: Day = Day;
inventory::submit! { FunctionEntry(&DAY) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(DAY.name(), "DAY");
        assert_eq!(DAY.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_day(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_day(vec![Value::Number(43_831.0), Value::Number(1.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_day_from_date_serial() {
        assert_eq!(call_day(vec![Value::Number(43_831.0)]), Value::Number(1.0));
        assert_eq!(
            call_day(vec![Value::Number(45_366.0)]),
            Value::Number(15.0)
        );
    }

    #[test]
    fn uses_date_portion_and_preserves_excel_1900_phantom_day() {
        assert_eq!(
            call_day(vec![Value::Number(43_831.75)]),
            Value::Number(1.0)
        );
        assert_eq!(call_day(vec![Value::Number(60.0)]), Value::Number(29.0));
    }

    #[test]
    fn uses_scalar_top_left_value_and_serial_date_coercion() {
        assert_eq!(
            call_day(vec![Value::Text(" 45366 ".to_string())]),
            Value::Number(15.0)
        );
        assert_eq!(call_day(vec![Value::Boolean(true)]), Value::Number(1.0));

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
            DAY.call(&[Arg::Range(ctx.range_view(range))], &fn_ctx),
            Value::Number(15.0)
        );
    }

    #[test]
    fn rejects_zero_negative_and_non_finite_serials() {
        assert_eq!(call_day(vec![Value::Number(0.0)]), Value::Error(ErrorValue::Num));
        assert_eq!(
            call_day(vec![Value::Number(-1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_day(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_day(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_serial_date_coercion_errors() {
        assert_eq!(
            call_day(vec![Value::Text("2020-01-01".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_day(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_day(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DAY.call(&args, &fn_ctx)
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
                Value::Number(45_366.0)
            } else {
                Value::Number(43_831.0)
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
