use crate::functions::prelude::*;

pub struct Pmt;

impl Function for Pmt {
    fn name(&self) -> &'static str {
        "PMT"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (3, Some(5))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if args.len() < 3 || args.len() > 5 {
            return Value::Error(ErrorValue::Value);
        }

        let rate = match ctx.to_number(&args[0].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let nper = match ctx.to_number(&args[1].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let pv = match ctx.to_number(&args[2].as_value()) {
            Ok(number) => number,
            Err(error) => return Value::Error(error),
        };
        let fv = match args.get(3) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(number) => number,
                Err(error) => return Value::Error(error),
            },
            None => 0.0,
        };
        let payment_type = match args.get(4) {
            Some(arg) => match ctx.to_number(&arg.as_value()) {
                Ok(number) => number,
                Err(error) => return Value::Error(error),
            },
            None => 0.0,
        };

        if ![rate, nper, pv, fv, payment_type]
            .into_iter()
            .all(f64::is_finite)
        {
            return Value::Error(ErrorValue::Num);
        }

        if nper == 0.0 {
            return Value::Error(ErrorValue::Num);
        }

        let payment_type = payment_type.trunc();
        if payment_type != 0.0 && payment_type != 1.0 {
            return Value::Error(ErrorValue::Num);
        }

        let payment = if rate == 0.0 {
            -(pv + fv) / nper
        } else {
            let factor = (1.0 + rate).powf(nper);
            if !factor.is_finite() {
                return Value::Error(ErrorValue::Num);
            }

            let timing = 1.0 + rate * payment_type;
            let denominator = timing * (factor - 1.0);
            if !denominator.is_finite() || denominator == 0.0 {
                return Value::Error(ErrorValue::Num);
            }

            -((fv + pv * factor) * rate) / denominator
        };

        if payment.is_finite() {
            Value::Number(payment)
        } else {
            Value::Error(ErrorValue::Num)
        }
    }
}

static PMT: Pmt = Pmt;
inventory::submit! { FunctionEntry(&PMT) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{eval, EvalContext};
    use crate::model::Coord;
    use crate::syntax::{parse, CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(PMT.name(), "PMT");
        assert_eq!(PMT.arity(), (3, Some(5)));
    }

    #[test]
    fn matches_documented_payment_examples() {
        assert_number_close(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
            ]),
            -1037.0320893591636,
        );
        assert_number_close(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            -1030.1643271779772,
        );
        assert_number_close(
            call(&[
                Value::Number(0.06 / 12.0),
                Value::Number(18.0 * 12.0),
                Value::Number(0.0),
                Value::Number(50000.0),
            ]),
            -129.0811608679954,
        );
    }

    #[test]
    fn supports_zero_rate() {
        assert_eq!(
            call(&[
                Value::Number(0.0),
                Value::Number(10.0),
                Value::Number(1000.0),
            ]),
            Value::Number(-100.0)
        );
    }

    #[test]
    fn defaults_optional_arguments_to_zero() {
        assert_eq!(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
            ]),
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ])
        );
    }

    #[test]
    fn truncates_payment_type_toward_zero() {
        assert_eq!(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(1.9),
            ]),
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ])
        );
        assert_eq!(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(-0.9),
            ]),
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ])
        );
    }

    #[test]
    fn rejects_invalid_numeric_domains() {
        assert_eq!(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(0.0),
                Value::Number(2.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Number(0.0),
                Value::Number(10000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_arguments_and_results() {
        assert_eq!(
            call(&[
                Value::Number(f64::INFINITY),
                Value::Number(10.0),
                Value::Number(10000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call(&[
                Value::Number(10.0),
                Value::Number(10000.0),
                Value::Number(10000.0),
            ]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn uses_scalar_top_left_number_coercion_left_to_right() {
        assert_number_close(
            eval_formula("=PMT(A1:B2,10,10000)"),
            -1037.0320893591636,
        );
        assert_eq!(
            call(&[
                Value::Text("not numeric".to_string()),
                Value::Error(ErrorValue::Div0),
                Value::Number(10000.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call(&[
                Value::Number(0.08 / 12.0),
                Value::Error(ErrorValue::Div0),
                Value::Text("not numeric".to_string()),
            ]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn returns_value_for_direct_wrong_arity() {
        assert_eq!(PMT.call(&[], &fn_ctx()), Value::Error(ErrorValue::Value));
        assert_eq!(
            call(&[
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(eval_formula("=PMT(0,1)"), Value::Error(ErrorValue::Value));
        assert_eq!(
            eval_formula("=PMT(0,1,1,0,0,0)"),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call(values: &[Value]) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.iter().cloned().map(Arg::Value).collect();
        PMT.call(&args, &fn_ctx)
    }

    fn eval_formula(formula: &str) -> Value {
        let expr = parse(formula).expect("parse test formula");
        let ctx = TestContext;
        eval(&expr, &ctx)
    }

    fn fn_ctx() -> FnContext<'static> {
        static CTX: TestContext = TestContext;
        FnContext::new(&CTX)
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(actual) => assert!(
                (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
                "expected {expected}, got {actual}"
            ),
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    struct TestContext;

    impl EvalContext for TestContext {
        fn cell_value(&self, r: CellRef) -> Value {
            if r.coord().row == 0 && r.coord().col == 0 {
                Value::Number(0.08 / 12.0)
            } else {
                Value::Blank
            }
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            if name.eq_ignore_ascii_case(PMT.name()) {
                Some(&PMT)
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
