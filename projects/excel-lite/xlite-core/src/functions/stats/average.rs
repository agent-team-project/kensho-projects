use crate::functions::prelude::*;

pub struct Average;

impl Function for Average {
    fn name(&self) -> &'static str {
        "AVERAGE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, None)
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        let mut total = 0.0;
        let mut count = 0usize;
        for arg in args {
            match ctx.try_iter_numbers(arg) {
                Ok(numbers) => {
                    for number in numbers {
                        count += 1;
                        total += number;
                    }
                }
                Err(error) => return Value::Error(error),
            }
        }

        if count == 0 {
            Value::Error(ErrorValue::Div0)
        } else {
            Value::Number(total / count as f64)
        }
    }
}

static AVERAGE: Average = Average;
inventory::submit! { FunctionEntry(&AVERAGE) }
