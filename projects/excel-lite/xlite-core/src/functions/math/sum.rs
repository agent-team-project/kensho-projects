use crate::functions::prelude::*;

pub struct Sum;

impl Function for Sum {
    fn name(&self) -> &'static str {
        "SUM"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, None)
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let mut total = 0.0;
        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => total += numbers.sum::<f64>(),
                Err(error) => return Value::Error(error),
            }
        }
        Value::Number(total)
    }
}

static SUM: Sum = Sum;
inventory::submit! { FunctionEntry(&SUM) }
