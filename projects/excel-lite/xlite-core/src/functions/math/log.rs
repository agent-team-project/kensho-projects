use crate::functions::prelude::*;

pub struct Log;

impl Function for Log {
    fn name(&self) -> &'static str {
        "LOG"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let number = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let base = match args.get(1) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(base) => base,
                Err(error) => return Value::Error(error),
            },
            None => 10.0,
        };

        if number <= 0.0 || base <= 0.0 || base == 1.0 {
            return Value::Error(ErrorValue::Num);
        }

        let result = number.log(base);
        if result.is_finite() {
            Value::Number(result)
        } else {
            Value::Error(ErrorValue::Num)
        }
    }
}

static LOG: Log = Log;
inventory::submit! { FunctionEntry(&LOG) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{eval, EvalContext};
    use crate::model::Coord;
    use crate::syntax::{parse, CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(LOG.name(), "LOG");
        assert_eq!(LOG.arity(), (1, Some(2)));
    }

    #[test]
    fn defaults_to_base_ten() {
        assert_number_close(call(Value::Number(10.0), None), 1.0);
    }

    #[test]
    fn supports_explicit_base() {
        assert_number_close(call(Value::Number(8.0), Some(Value::Number(2.0))), 3.0);
    }

    #[test]
    fn supports_non_integer_base() {
        assert_number_close(
            call(Value::Number(86.0), Some(Value::Number(std::f64::consts::E))),
            86.0_f64.ln(),
        );
    }

    #[test]
    fn invalid_domains_return_num_error() {
        assert_eq!(call(Value::Number(0.0), None), Value::Error(ErrorValue::Num));
        assert_eq!(
            call(Value::Number(-1.0), None),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(Value::Number(10.0), Some(Value::Number(0.0))),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(Value::Number(10.0), Some(Value::Number(-2.0))),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(Value::Number(10.0), Some(Value::Number(1.0))),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_number_coercion() {
        assert_number_close(
            call(
                Value::Text(" 100 ".to_string()),
                Some(Value::Text("10".to_string())),
            ),
            2.0,
        );
        assert_number_close(call(Value::Boolean(true), Some(Value::Number(10.0))), 0.0);
    }

    #[test]
    fn blank_arguments_coerce_to_zero_before_domain_check() {
        assert_eq!(call(Value::Blank, None), Value::Error(ErrorValue::Num));
        assert_eq!(
            call(Value::Number(10.0), Some(Value::Blank)),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn propagates_coercion_errors_from_each_argument() {
        assert_eq!(
            call(Value::Text("not numeric".to_string()), Some(Value::Number(10.0))),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(
                Value::Number(10.0),
                Some(Value::Text("not numeric".to_string()))
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(Value::Error(ErrorValue::Div0), Some(Value::Number(10.0))),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call(Value::Number(10.0), Some(Value::Error(ErrorValue::Ref))),
            Value::Error(ErrorValue::Ref)
        );
    }

    #[test]
    fn evaluator_rejects_wrong_arity() {
        assert_eq!(eval_formula("=LOG()"), Value::Error(ErrorValue::Value));
        assert_eq!(
            eval_formula("=LOG(10,10,10)"),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call(number: Value, base: Option<Value>) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        let mut args = vec![Arg::Value(number)];
        if let Some(base) = base {
            args.push(Arg::Value(base));
        }
        LOG.call(&args, &fn_ctx)
    }

    fn eval_formula(formula: &str) -> Value {
        let expr = parse(formula).expect("parse test formula");
        let ctx = TestContext;
        eval(&expr, &ctx)
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

    struct TestContext;

    impl EvalContext for TestContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            if name == "LOG" {
                Some(&LOG)
            } else {
                None
            }
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
