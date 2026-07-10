use crate::functions::prelude::*;

const SECONDS_PER_DAY: f64 = 86_400.0;

pub struct Second;

impl Function for Second {
    fn name(&self) -> &'static str {
        "SECOND"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match second_from_serial(ctx.to_serial_date(&args[0].as_value())) {
            Ok(second) => Value::Number(second as f64),
            Err(error) => Value::Error(error),
        }
    }
}

fn second_from_serial(serial: Result<f64, ErrorValue>) -> Result<u8, ErrorValue> {
    let serial = serial?;
    if !serial.is_finite() || serial < 0.0 {
        return Err(ErrorValue::Num);
    }

    let total_seconds = (serial.fract() * SECONDS_PER_DAY).floor() as i64;
    Ok((total_seconds % 60) as u8)
}

static SECOND: Second = Second;
inventory::submit! { FunctionEntry(&SECOND) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(SECOND.name(), "SECOND");
        assert_eq!(SECOND.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_second(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_second(vec![Value::Number(0.5), Value::Number(0.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn extracts_second_from_fractional_day() {
        assert_eq!(call_second(vec![Value::Number(0.0)]), Value::Number(0.0));
        assert_eq!(call_second(vec![Value::Number(0.5)]), Value::Number(0.0));
        assert_eq!(
            call_second(vec![Value::Number(0.700_208_333_333_333_3)]),
            Value::Number(18.0)
        );
        assert_eq!(
            call_second(vec![Value::Number(1.700_208_333_333_333_3)]),
            Value::Number(18.0)
        );
        assert_eq!(
            call_second(vec![Value::Number(59.0 / SECONDS_PER_DAY)]),
            Value::Number(59.0)
        );
    }

    #[test]
    fn uses_scalar_serial_date_coercion() {
        assert_eq!(
            call_second(vec![Value::Text(" 1.7002083333333333 ".to_string())]),
            Value::Number(18.0)
        );
        assert_eq!(call_second(vec![Value::Boolean(true)]), Value::Number(0.0));
        assert_eq!(call_second(vec![Value::Blank]), Value::Number(0.0));
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
            SECOND.call(&[Arg::Range(ctx.range_view(range))], &fn_ctx),
            Value::Number(18.0)
        );
    }

    #[test]
    fn rejects_negative_and_non_finite_serials() {
        assert_eq!(
            call_second(vec![Value::Number(-0.1)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_second(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_second(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call_second(vec![Value::Text("6:45 PM".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_second(vec![Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_second(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        SECOND.call(&args, &fn_ctx)
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
                Value::Number(1.700_208_333_333_333_3)
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
