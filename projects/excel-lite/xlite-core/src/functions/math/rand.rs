use crate::functions::prelude::*;

pub struct Rand;

impl Function for Rand {
    fn name(&self) -> &'static str {
        "RAND"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }

    fn call(&self, _args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        Value::Number(ctx.rand())
    }
}

static RAND: Rand = Rand;
inventory::submit! { FunctionEntry(&RAND) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{eval, EvalContext};
    use crate::model::Coord;
    use crate::syntax::{parse, CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(RAND.name(), "RAND");
        assert_eq!(RAND.arity(), (0, Some(0)));
    }

    #[test]
    fn returns_default_context_random_value() {
        assert_eq!(call_with(&DefaultRandContext), Value::Number(0.5));
    }

    #[test]
    fn delegates_to_eval_context_random_hook() {
        assert_eq!(call_with(&FixedRandContext(0.125)), Value::Number(0.125));
        assert_eq!(call_with(&FixedRandContext(0.875)), Value::Number(0.875));
    }

    #[test]
    fn evaluator_rejects_arguments() {
        assert_eq!(eval_formula("=RAND(1)"), Value::Error(ErrorValue::Value));
    }

    fn call_with(ctx: &dyn EvalContext) -> Value {
        let fn_ctx = FnContext::new(ctx);
        RAND.call(&[], &fn_ctx)
    }

    fn eval_formula(formula: &str) -> Value {
        let expr = parse(formula).expect("parse test formula");
        let ctx = FunctionContext;
        eval(&expr, &ctx)
    }

    struct DefaultRandContext;

    impl EvalContext for DefaultRandContext {
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

    struct FixedRandContext(f64);

    impl EvalContext for FixedRandContext {
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

        fn rand(&self) -> f64 {
            self.0
        }
    }

    struct FunctionContext;

    impl EvalContext for FunctionContext {
        fn cell_value(&self, _r: CellRef) -> Value {
            Value::Blank
        }

        fn range_view(&self, r: RangeRef) -> RangeView<'_> {
            RangeView::new(self, r)
        }

        fn function(&self, name: &str) -> Option<&dyn Function> {
            if name == "RAND" {
                Some(&RAND)
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
