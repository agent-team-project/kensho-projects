use crate::functions::prelude::*;

const HOURS_PER_DAY: f64 = 24.0;
const MAX_HOUR: f64 = 23.0;

pub struct Hour;

impl Function for Hour {
    fn name(&self) -> &'static str {
        "HOUR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(1))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 1 {
            return Value::Error(ErrorValue::Value);
        }

        match hour_from_serial(ctx.to_serial_date(&args[0].as_value())) {
            Ok(hour) => Value::Number(hour as f64),
            Err(error) => Value::Error(error),
        }
    }
}

fn hour_from_serial(serial: Result<f64, ErrorValue>) -> Result<u8, ErrorValue> {
    let serial = serial?;
    if !serial.is_finite() || serial < 0.0 {
        return Err(ErrorValue::Num);
    }

    let hour = (serial.fract() * HOURS_PER_DAY).floor().min(MAX_HOUR);
    Ok(hour as u8)
}

static HOUR: Hour = Hour;
inventory::submit! { FunctionEntry(&HOUR) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_one_argument_arity() {
        assert_eq!(HOUR.name(), "HOUR");
        assert_eq!(HOUR.arity(), (1, Some(1)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_hour(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_hour(vec![Value::Number(0.0), Value::Number(0.5)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_hour_from_fractional_day() {
        assert_eq!(call_hour(vec![Value::Number(0.0)]), Value::Number(0.0));
        assert_eq!(call_hour(vec![Value::Number(0.5)]), Value::Number(12.0));
        assert_eq!(call_hour(vec![Value::Number(0.75)]), Value::Number(18.0));
        assert_eq!(call_hour(vec![Value::Number(1.75)]), Value::Number(18.0));
        assert_eq!(
            call_hour(vec![Value::Number(1.999_988_425_925_926)]),
            Value::Number(23.0)
        );
    }

    #[test]
    fn uses_scalar_top_left_value_and_serial_date_coercion() {
        assert_eq!(
            call_hour(vec![Value::Text(" 1.75 ".to_string())]),
            Value::Number(18.0)
        );
        assert_eq!(call_hour(vec![Value::Boolean(true)]), Value::Number(0.0));
        assert_eq!(call_hour(vec![Value::Blank]), Value::Number(0.0));
    }

    #[test]
    fn rejects_negative_and_non_finite_serials() {
        assert_eq!(
            call_hour(vec![Value::Number(-0.25)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_hour(vec![Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_hour(vec![Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_serial_date_coercion_errors() {
        assert_eq!(
            call_hour(vec![Value::Text("6:45 PM".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_hour(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call_hour(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        HOUR.call(&args, &fn_ctx)
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
