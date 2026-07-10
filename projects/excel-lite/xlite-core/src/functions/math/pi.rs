use crate::functions::prelude::*;

pub struct Pi;

impl Function for Pi {
    fn name(&self) -> &'static str {
        "PI"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (0, Some(0))
    }

    fn call(&self, _args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        Value::Number(std::f64::consts::PI)
    }
}

static PI: Pi = Pi;
inventory::submit! { FunctionEntry(&PI) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    struct EmptyContext;

    impl EvalContext for EmptyContext {
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

    #[test]
    fn pi_metadata_is_zero_arity() {
        assert_eq!(PI.name(), "PI");
        assert_eq!(PI.arity(), (0, Some(0)));
    }

    #[test]
    fn pi_returns_constant() {
        let ctx = EmptyContext;
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(PI.call(&[], &fn_ctx), Value::Number(std::f64::consts::PI));
    }
}
