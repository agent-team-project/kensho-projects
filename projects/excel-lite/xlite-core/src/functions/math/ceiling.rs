use crate::functions::prelude::*;

pub struct Ceiling;

impl Function for Ceiling {
    fn name(&self) -> &'static str {
        "CEILING"
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

        if significance == 0.0 {
            return Value::Number(0.0);
        }

        if number > 0.0 && significance < 0.0 {
            return Value::Error(ErrorValue::Num);
        }

        if is_exact_multiple(number, significance) {
            return Value::Number(number);
        }

        Value::Number(round_ceiling(number, significance))
    }
}

fn is_exact_multiple(number: f64, significance: f64) -> bool {
    (number / significance).fract() == 0.0
}

fn round_ceiling(number: f64, significance: f64) -> f64 {
    let step = significance.abs();
    let quotient = number / step;
    let rounded = if number < 0.0 && significance < 0.0 {
        quotient.floor()
    } else {
        quotient.ceil()
    } * step;

    if rounded == 0.0 { 0.0 } else { rounded }
}

static CEILING: Ceiling = Ceiling;
inventory::submit! { FunctionEntry(&CEILING) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_exact_scalar_arity() {
        assert_eq!(CEILING.arity(), (2, Some(2)));
    }

    #[test]
    fn rounds_positive_numbers_up() {
        assert_number_close(
            call(Value::Number(4.42), Value::Number(0.05)),
            4.45,
        );
        assert_eq!(
            call(Value::Number(7.0), Value::Number(3.0)),
            Value::Number(9.0)
        );
    }

    #[test]
    fn leaves_exact_multiples_unchanged() {
        assert_eq!(
            call(Value::Number(2.5), Value::Number(0.5)),
            Value::Number(2.5)
        );
        assert_eq!(
            call(Value::Number(-4.0), Value::Number(2.0)),
            Value::Number(-4.0)
        );
        assert_eq!(
            call(Value::Number(-6.0), Value::Number(-3.0)),
            Value::Number(-6.0)
        );
    }

    #[test]
    fn rounds_negative_numbers_with_positive_significance_toward_zero() {
        assert_eq!(
            call(Value::Number(-2.5), Value::Number(2.0)),
            Value::Number(-2.0)
        );
        assert_eq!(
            call(Value::Number(-5.0), Value::Number(2.0)),
            Value::Number(-4.0)
        );
    }

    #[test]
    fn rounds_negative_numbers_with_negative_significance_away_from_zero() {
        assert_eq!(
            call(Value::Number(-2.5), Value::Number(-2.0)),
            Value::Number(-4.0)
        );
        assert_eq!(
            call(Value::Number(-5.0), Value::Number(-2.0)),
            Value::Number(-6.0)
        );
    }

    #[test]
    fn returns_num_for_positive_number_with_negative_significance() {
        assert_eq!(
            call(Value::Number(2.5), Value::Number(-2.0)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn returns_zero_for_zero_significance() {
        assert_eq!(
            call(Value::Number(2.5), Value::Number(0.0)),
            Value::Number(0.0)
        );
        assert_eq!(
            call(Value::Number(-2.5), Value::Number(0.0)),
            Value::Number(0.0)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_eq!(
            call(Value::Text(" 4.2 ".to_string()), Value::Text("2".to_string())),
            Value::Number(6.0)
        );
        assert_eq!(
            call(Value::Boolean(true), Value::Number(2.0)),
            Value::Number(2.0)
        );
        assert_eq!(
            call(Value::Number(5.0), Value::Boolean(true)),
            Value::Number(5.0)
        );
        assert_eq!(call(Value::Blank, Value::Number(3.0)), Value::Number(0.0));
        assert_eq!(call(Value::Number(3.0), Value::Blank), Value::Number(0.0));
    }

    #[test]
    fn propagates_coercion_errors() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Value::Number(1.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Number(1.0), Value::Text("not numeric".to_string())),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Ref), Value::Number(1.0)),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call(Value::Number(1.0), Value::Error(ErrorValue::Na)),
            Value::Error(ErrorValue::Na)
        );
    }

    fn call(number: Value, significance: Value) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        CEILING.call(&[Arg::Value(number), Arg::Value(significance)], &fn_ctx)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= 1e-12,
                "expected {expected}, got {actual}"
            ),
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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
