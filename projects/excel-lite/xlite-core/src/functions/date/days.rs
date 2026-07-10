use crate::functions::prelude::*;

pub struct Days;

impl Function for Days {
    fn name(&self) -> &'static str {
        "DAYS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() != 2 {
            return Value::Error(ErrorValue::Value);
        }

        match days(args, ctx) {
            Ok(days) => Value::Number(days),
            Err(error) => Value::Error(error),
        }
    }
}

fn days(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let end_date = ctx.to_serial_date(&args[0].as_value())?;
    let start_date = ctx.to_serial_date(&args[1].as_value())?;

    if !end_date.is_finite() || !start_date.is_finite() {
        return Err(ErrorValue::Num);
    }

    Ok(end_date - start_date)
}

static DAYS: Days = Days;
inventory::submit! { FunctionEntry(&DAYS) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_two_argument_arity() {
        assert_eq!(DAYS.name(), "DAYS");
        assert_eq!(DAYS.arity(), (2, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_days(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_days(vec![Value::Number(43_831.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_days(vec![
                Value::Number(43_831.0),
                Value::Number(43_830.0),
                Value::Number(43_829.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn subtracts_start_date_from_end_date() {
        assert_eq!(
            call_days(vec![Value::Number(43_861.0), Value::Number(43_831.0)]),
            Value::Number(30.0)
        );
    }

    #[test]
    fn returns_leap_year_span() {
        assert_eq!(
            call_days(vec![Value::Number(45_352.0), Value::Number(45_323.0)]),
            Value::Number(29.0)
        );
    }

    #[test]
    fn allows_negative_and_fractional_differences() {
        assert_eq!(
            call_days(vec![Value::Number(43_831.0), Value::Number(43_861.0)]),
            Value::Number(-30.0)
        );
        assert_eq!(
            call_days(vec![Value::Number(43_832.75), Value::Number(43_831.25)]),
            Value::Number(1.5)
        );
    }

    #[test]
    fn uses_scalar_top_left_value_and_serial_date_coercion() {
        assert_eq!(
            call_days(vec![
                Value::Text(" 43832.5 ".to_string()),
                Value::Text("43831.25".to_string()),
            ]),
            Value::Number(1.25)
        );
        assert_eq!(
            call_days(vec![Value::Boolean(true), Value::Blank]),
            Value::Number(1.0)
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
            DAYS.call(
                &[
                    Arg::Range(ctx.range_view(range)),
                    Arg::Value(Value::Number(43_831.25)),
                ],
                &fn_ctx,
            ),
            Value::Number(1.25)
        );
    }

    #[test]
    fn rejects_non_finite_serials() {
        assert_eq!(
            call_days(vec![Value::Number(f64::INFINITY), Value::Number(1.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_days(vec![Value::Number(1.0), Value::Number(f64::NAN)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_first_serial_date_coercion_error_left_to_right() {
        assert_eq!(
            call_days(vec![
                Value::Text("2020-01-31".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_days(vec![
                Value::Error(ErrorValue::Div0),
                Value::Text("2020-01-01".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_days(vec![
                Value::Number(43_831.0),
                Value::Text("2020-01-01".to_string()),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_days(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        DAYS.call(&args, &fn_ctx)
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
                Value::Number(43_832.5)
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
