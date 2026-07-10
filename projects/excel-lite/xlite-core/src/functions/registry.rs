use std::collections::HashMap;

use crate::eval::{Arg, EvalResult, FnContext};
use crate::model::Value;

/// How the evaluator should lower an argument expression before a function call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionArgMode {
    /// Evaluate the expression to its scalar value.
    Value,
    /// Preserve a direct reference expression as a range view.
    Reference,
}

/// Uniform interface implemented by every worksheet function.
///
/// Implementations are stateless, registered through `inventory`, and receive
/// all coercion/environment behavior through `FnContext` so individual function
/// files do not reimplement spreadsheet semantics.
pub trait Function: Send + Sync {
    /// Uppercase canonical function name, for example `SUM`.
    fn name(&self) -> &'static str;
    /// Inclusive arity bounds. `None` means variadic upper bound.
    fn arity(&self) -> (usize, Option<usize>);
    /// Evaluate with arity already checked. Errors are returned as values.
    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value;
    /// Per-argument lowering mode. Defaults keep existing scalar behavior.
    fn argument_mode(&self, _index: usize) -> FunctionArgMode {
        FunctionArgMode::Value
    }
    /// Evaluate to either a scalar or a reference result.
    fn call_result(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> EvalResult {
        EvalResult::Value(self.call(args, ctx))
    }
}

/// Link-time registration entry for one function implementation.
pub struct FunctionEntry(pub &'static dyn Function);

inventory::collect!(FunctionEntry);

pub fn build_registry() -> HashMap<&'static str, &'static dyn Function> {
    inventory::iter::<FunctionEntry>
        .into_iter()
        .map(|entry| (entry.0.name(), entry.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice0_functions_self_register() {
        let registry = build_registry();
        assert!(registry.contains_key("SUM"));
        assert!(registry.contains_key("AVERAGE"));
        assert!(registry.contains_key("IF"));
    }
}
