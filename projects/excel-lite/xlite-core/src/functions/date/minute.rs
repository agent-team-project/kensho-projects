use crate::functions::prelude::*;

const MINUTES_PER_DAY: f64 = 1_440.0;

pub struct Minute;

impl Function for Minute {
    fn name(&self) -> &'static str {
        "MINUTE"
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

        if !serial.is_finite() || serial < 0.0 {
            return Value::Error(ErrorValue::Num);
        }

        let total_minutes = (serial.fract() * MINUTES_PER_DAY).floor() as i64;

        Value::Number((total_minutes % 60) as f64)
    }
}

static MINUTE: Minute = Minute;
inventory::submit! { FunctionEntry(&MINUTE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(MINUTE.name(), "MINUTE");
        assert_eq!(MINUTE.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_minute(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_minute(vec![Value::Number(0.5), Value::Number(0.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn extracts_minute_from_fractional_day() {
        assert_eq!(call_minute(vec![Value::Number(0.0)]), Value::Number(0.0));
        assert_eq!(call_minute(vec![Value::Number(0.5)]), Value::Number(0.0));
        assert_eq!(
            call_minute(vec![Value::Number(0.78125)]),
            Value::Number(45.0)
        );
        assert_eq!(
            call_minute(vec![Value::Number(1.78125)]),
            Value::Number(45.0)
        );
        assert_eq!(
            call_minute(vec![Value::Number(59.0 / MINUTES_PER_DAY)]),
            Value::Number(59.0)
        );
    }

    #[test]
    fn uses_scalar_serial_date_coercion() {
        assert_eq!(
            call_minute(vec![Value::Text(" 1.78125 ".to_string())]),
            Value::Number(45.0)
        );
        assert_eq!(call_minute(vec![Value::Boolean(true)]), Value::Number(0.0));
        assert_eq!(call_minute(vec![Value::Blank]), Value::Number(0.0));
    }

    #[test]
    fn uses_top_left_value_for_range_argument() {
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
            MINUTE.call(&[Arg::Range(ctx.range_view(range))], &fn_ctx),
            Value::Number(45.0)
        );
    }

    #[test]
    fn rejects_negative_and_non_finite_serials() {
        assert_eq!(
            call_minute(vec![Value::Number(-0.1)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_minute(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_minute(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_minute(vec![Value::Text("6:45 PM".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_minute(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_minute(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        MINUTE.call(&args, &fn_ctx)
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
                Value::Number(1.78125)
            } else {
                Value::Number(0.5)
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
