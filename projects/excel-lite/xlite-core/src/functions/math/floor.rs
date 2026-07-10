use crate::functions::prelude::*;

pub struct Floor;

impl Function for Floor {
    fn name(&self) -> &'static str {
        "FLOOR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let significance = match ctx.to_number(&args[1].as_value()) {
            Ok(significance) => significance,
            Err(error) => return Value::Error(error),
        };

        match floor(number, significance) {
            Ok(result) => Value::Number(result),
            Err(error) => Value::Error(error),
        }
    }
}

fn floor(number: f64, significance: f64) -> Result<f64, ErrorValue> {
    if significance == 0.0 {
        return Ok(0.0);
    }

    if number % significance == 0.0 {
        return Ok(number);
    }

    if number > 0.0 && significance < 0.0 {
        return Err(ErrorValue::Num);
    }

    let rounded = (number / significance).floor() * significance;
    Ok(if rounded == 0.0 { 0.0 } else { rounded })
}

static FLOOR: Floor = Floor;
inventory::submit! { FunctionEntry(&FLOOR) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(FLOOR.name(), "FLOOR");
        assert_eq!(FLOOR.arity(), (2, Some(2)));
    }

    #[test]
    fn rounds_positive_numbers_down_to_positive_significance() {
        assert_eq!(
            call(Value::Number(7.5), Value::Number(2.0)),
            Value::Number(6.0)
        );
        assert_eq!(
            call(Value::Number(8.9), Value::Number(0.5)),
            Value::Number(8.5)
        );
    }

    #[test]
    fn preserves_exact_multiples() {
        assert_eq!(
            call(Value::Number(8.0), Value::Number(2.0)),
            Value::Number(8.0)
        );
        assert_eq!(
            call(Value::Number(-8.0), Value::Number(2.0)),
            Value::Number(-8.0)
        );
    }

    #[test]
    fn rounds_negative_numbers_away_from_zero_with_positive_significance() {
        assert_eq!(
            call(Value::Number(-7.0), Value::Number(2.0)),
            Value::Number(-8.0)
        );
    }

    #[test]
    fn rounds_negative_numbers_toward_zero_with_negative_significance() {
        assert_eq!(
            call(Value::Number(-7.0), Value::Number(-2.0)),
            Value::Number(-6.0)
        );
    }

    #[test]
    fn rejects_positive_numbers_with_negative_significance() {
        assert_eq!(
            call(Value::Number(7.0), Value::Number(-2.0)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn zero_significance_returns_zero() {
        assert_eq!(
            call(Value::Number(7.0), Value::Number(0.0)),
            Value::Number(0.0)
        );
        assert_eq!(
            call(Value::Number(-7.0), Value::Number(0.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion_for_both_args() {
        assert_eq!(
            call(Value::Text(" 8.7 ".to_string()), Value::Boolean(true)),
            Value::Number(8.0)
        );
        assert_eq!(
            call(Value::Boolean(true), Value::Text("0.5".to_string())),
            Value::Number(1.0)
        );
        assert_eq!(call(Value::Blank, Value::Number(2.0)), Value::Number(0.0));
        assert_eq!(call(Value::Number(7.0), Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors_from_each_argument() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Value::Number(1.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Number(8.0), Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Ref), Value::Number(1.0)),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call(Value::Number(8.0), Value::Error(ErrorValue::Div0)),
            Value::Error(ErrorValue::Div0)
        );
    }

    fn call(number: Value, significance: Value) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        FLOOR.call(&[Arg::Value(number), Arg::Value(significance)], &fn_ctx)
    }

    struct TestContext;

    impl EvalContext for TestContext {
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
