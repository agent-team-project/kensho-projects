use crate::functions::prelude::*;

const MAX_TIME_ARGUMENT: f64 = 32_767.0;
const SECONDS_PER_DAY: i64 = 86_400;

pub struct Time;

impl Function for Time {
    fn name(&self) -> &'static str {
        "TIME"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 3 {
            return Value::Error(ErrorValue::Value);
        }

        let hour = match time_argument(&args[0], ctx) {
            Ok(hour) => hour,
            Err(error) => return Value::Error(error),
        };
        let minute = match time_argument(&args[1], ctx) {
            Ok(minute) => minute,
            Err(error) => return Value::Error(error),
        };
        let second = match time_argument(&args[2], ctx) {
            Ok(second) => second,
            Err(error) => return Value::Error(error),
        };

        let total_seconds = hour * 3_600 + minute * 60 + second;
        let wrapped_seconds = total_seconds % SECONDS_PER_DAY;

        Value::Number(wrapped_seconds as f64 / SECONDS_PER_DAY as f64)
    }
}

fn time_argument(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<i64, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    let truncated = number.trunc();

    if !truncated.is_finite() || !(0.0..=MAX_TIME_ARGUMENT).contains(&truncated) {
        return Err(ErrorValue::Num);
    }

    Ok(truncated as i64)
}

static TIME: Time = Time;
inventory::submit! { FunctionEntry(&TIME) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_three_argument_arity() {
        assert_eq!(TIME.name(), "TIME");
        assert_eq!(TIME.arity(), (3, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_time(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_time(vec![Value::Number(12.0), Value::Number(0.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_time(vec![
                Value::Number(12.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn returns_decimal_day_fraction_for_noon() {
        assert_eq!(
            call_time(vec![
                Value::Number(12.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Number(0.5)
        );
    }

    #[test]
    fn wraps_hours_minutes_and_seconds_into_one_day() {
        assert_eq!(
            call_time(vec![
                Value::Number(27.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Number(0.125)
        );
        assert_close(
            call_time(vec![
                Value::Number(0.0),
                Value::Number(750.0),
                Value::Number(0.0),
            ]),
            0.5208333333333334,
        );
        assert_close(
            call_time(vec![
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(2000.0),
            ]),
            2000.0 / 86_400.0,
        );
    }

    #[test]
    fn truncates_fractional_arguments_toward_zero() {
        assert_eq!(
            call_time(vec![
                Value::Number(1.9),
                Value::Number(2.9),
                Value::Number(3.9),
            ]),
            Value::Number(3723.0 / 86_400.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call_time(vec![
                Value::Text(" 1 ".to_string()),
                Value::Boolean(true),
                Value::Blank,
            ]),
            Value::Number(3660.0 / 86_400.0)
        );
        assert_eq!(
            call_time(vec![
                Value::Boolean(false),
                Value::Text("30".to_string()),
                Value::Text("15".to_string()),
            ]),
            Value::Number(1815.0 / 86_400.0)
        );
    }

    #[test]
    fn rejects_truncated_arguments_outside_valid_range() {
        assert_eq!(
            call_time(vec![
                Value::Number(-1.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_time(vec![
                Value::Number(0.0),
                Value::Number(32_768.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_time(vec![
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(f64::INFINITY),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_time(vec![
                Value::Number(f64::NAN),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_time(vec![
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_time(vec![
                Value::Number(0.0),
                Value::Error(ErrorValue::Ref),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_time(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        TIME.call(&args, &fn_ctx)
    }

    fn assert_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => {
                assert!(
                    (actual - expected).abs() <= 1e-12,
                    "expected {expected}, got {actual}"
                );
            }
            other => panic!("expected number {expected}, got {other:?}"),
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
            CellId {
                sheet: 0,
                coord: Coord { row: 0, col: 0 },
            }
        }
    }
}
