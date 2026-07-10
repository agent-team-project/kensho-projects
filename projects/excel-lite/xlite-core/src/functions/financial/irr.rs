use crate::functions::prelude::*;

const DEFAULT_GUESS: f64 = 0.1;
const MAX_ITERATIONS: usize = 20;
const RATE_TOLERANCE: f64 = 1e-7;

pub struct Irr;

impl Function for Irr {
    fn name(&self) -> &'static str {
        "IRR"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(2))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(1..=2).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match irr(args, ctx) {
            Ok(rate) => Value::Number(rate),
            Err(error) => Value::Error(error),
        }
    }
}

fn irr(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let cash_flows = cash_flows(&args[0], ctx)?;
    if !has_positive_and_negative(&cash_flows) {
        return Err(ErrorValue::Num);
    }

    let guess = match args.get(1) {
        Some(arg) => finite_number(&arg.as_value(), ctx)?,
        None => DEFAULT_GUESS,
    };

    solve_irr(&cash_flows, guess)
}

fn cash_flows(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<Vec<f64>, ErrorValue> {
    let values = ctx.number_values(arg)?;
    if values.iter().all(|number| number.is_finite()) {
        Ok(values)
    } else {
        Err(ErrorValue::Num)
    }
}

fn finite_number(value: &Value, ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let number = ctx.to_number(value)?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Num)
    }
}

fn has_positive_and_negative(values: &[f64]) -> bool {
    values.iter().any(|value| *value > 0.0) && values.iter().any(|value| *value < 0.0)
}

fn solve_irr(values: &[f64], guess: f64) -> Result<f64, ErrorValue> {
    let mut rate = guess;
    for _ in 0..MAX_ITERATIONS {
        let (residual, derivative) = residual_and_derivative(values, rate)?;
        if derivative == 0.0 || !derivative.is_finite() {
            return Err(ErrorValue::Num);
        }

        let next_rate = rate - residual / derivative;
        if !next_rate.is_finite() {
            return Err(ErrorValue::Num);
        }
        if (next_rate - rate).abs() <= RATE_TOLERANCE {
            return Ok(next_rate);
        }
        rate = next_rate;
    }

    Err(ErrorValue::Num)
}

fn residual_and_derivative(values: &[f64], rate: f64) -> Result<(f64, f64), ErrorValue> {
    let base = 1.0 + rate;
    if base <= 0.0 || !base.is_finite() {
        return Err(ErrorValue::Num);
    }

    let mut residual = 0.0;
    let mut derivative = 0.0;
    let mut denominator = 1.0;

    for (period, cash_flow) in values.iter().enumerate() {
        if period > 0 {
            denominator *= base;
            if denominator == 0.0 || !denominator.is_finite() {
                return Err(ErrorValue::Num);
            }
        }

        residual += cash_flow / denominator;
        if !residual.is_finite() {
            return Err(ErrorValue::Num);
        }

        if period > 0 {
            let derivative_denominator = denominator * base;
            if derivative_denominator == 0.0 || !derivative_denominator.is_finite() {
                return Err(ErrorValue::Num);
            }
            derivative -= period as f64 * cash_flow / derivative_denominator;
            if !derivative.is_finite() {
                return Err(ErrorValue::Num);
            }
        }
    }

    Ok((residual, derivative))
}

static IRR: Irr = Irr;
inventory::submit! { FunctionEntry(&IRR) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(IRR.name(), "IRR");
        assert_eq!(IRR.arity(), (1, Some(2)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_values(vec![
                Value::Number(-100.0),
                Value::Number(0.1),
                Value::Number(0.2)
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn matches_microsoft_examples() {
        let cells = vec![
            ((0, 0), Value::Number(-70000.0)),
            ((1, 0), Value::Number(12000.0)),
            ((2, 0), Value::Number(15000.0)),
            ((3, 0), Value::Number(18000.0)),
            ((4, 0), Value::Number(21000.0)),
            ((5, 0), Value::Number(26000.0)),
        ];

        assert_close(
            call_range(cells.clone(), range(0, 0, 4, 0), None),
            -0.02124484827341099,
        );
        assert_close(
            call_range(cells.clone(), range(0, 0, 5, 0), None),
            0.08663094803653165,
        );
        assert_close(
            call_range(cells, range(0, 0, 2, 0), Some(Value::Number(-0.1))),
            -0.44350694133474056,
        );
    }

    #[test]
    fn omitted_guess_matches_explicit_default_guess() {
        let cells = simple_cash_flows();

        let omitted = call_range(cells.clone(), range(0, 0, 2, 0), None);
        let explicit = call_range(cells, range(0, 0, 2, 0), Some(Value::Number(0.1)));

        assert_close(omitted.clone(), 0.1306623862918075);
        assert_eq!(omitted, explicit);
    }

    #[test]
    fn range_ignores_text_logicals_and_blanks_in_row_major_order() {
        let cells = vec![
            ((0, 0), Value::Number(-100.0)),
            ((0, 1), Value::Text("skip".to_string())),
            ((0, 2), Value::Boolean(true)),
            ((1, 0), Value::Number(60.0)),
            ((1, 2), Value::Number(60.0)),
        ];
        let ctx = GridContext {
            cells: cells.clone(),
        };
        let fn_ctx = FnContext::new(&ctx);
        let arg = Arg::Range(RangeView::new(&ctx, range(0, 0, 1, 2)));

        assert_eq!(cash_flows(&arg, &fn_ctx), Ok(vec![-100.0, 60.0, 60.0]));
        assert_close(call_range(cells, range(0, 0, 1, 2), None), 0.1306623862918075);
    }

    #[test]
    fn propagates_range_errors() {
        let cells = vec![
            ((0, 0), Value::Number(-100.0)),
            ((1, 0), Value::Error(ErrorValue::Div0)),
            ((2, 0), Value::Number(120.0)),
        ];

        assert_eq!(
            call_range(cells, range(0, 0, 2, 0), None),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn direct_scalar_values_use_numeric_coercion() {
        assert_eq!(
            call_values(vec![Value::Text("-100".to_string())]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![Value::Text("not numeric".to_string())]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
    }

    #[test]
    fn rejects_missing_sign_mix() {
        assert_eq!(
            call_range(
                vec![
                    ((0, 0), Value::Number(10.0)),
                    ((1, 0), Value::Number(20.0)),
                ],
                range(0, 0, 1, 0),
                None,
            ),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_range(
                vec![
                    ((0, 0), Value::Number(-10.0)),
                    ((1, 0), Value::Number(-20.0)),
                ],
                range(0, 0, 1, 0),
                None,
            ),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_cash_flows_and_guess() {
        assert_eq!(
            call_range(
                vec![
                    ((0, 0), Value::Number(-100.0)),
                    ((1, 0), Value::Number(f64::INFINITY)),
                    ((2, 0), Value::Number(120.0)),
                ],
                range(0, 0, 2, 0),
                None,
            ),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_range(
                simple_cash_flows(),
                range(0, 0, 2, 0),
                Some(Value::Number(f64::INFINITY)),
            ),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_invalid_iteration_domain() {
        assert_eq!(
            call_range(
                vec![
                    ((0, 0), Value::Number(-100.0)),
                    ((1, 0), Value::Number(120.0)),
                ],
                range(0, 0, 1, 0),
                Some(Value::Number(-1.0)),
            ),
            Value::Error(ErrorValue::Num)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext { cells: Vec::new() };
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        IRR.call(&args, &fn_ctx)
    }

    fn call_range(
        cells: Vec<((u32, u32), Value)>,
        range_ref: RangeRef,
        guess: Option<Value>,
    ) -> Value {
        let ctx = GridContext { cells };
        let fn_ctx = FnContext::new(&ctx);
        let mut args = vec![Arg::Range(RangeView::new(&ctx, range_ref))];
        if let Some(value) = guess {
            args.push(Arg::Value(value));
        }
        IRR.call(&args, &fn_ctx)
    }

    fn assert_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(number) => assert!(
                (number - expected).abs() <= 1e-9 * expected.abs().max(1.0),
                "expected {expected}, got {number}"
            ),
            other => panic!("expected numeric result, got {other:?}"),
        }
    }

    fn simple_cash_flows() -> Vec<((u32, u32), Value)> {
        vec![
            ((0, 0), Value::Number(-100.0)),
            ((1, 0), Value::Number(60.0)),
            ((2, 0), Value::Number(60.0)),
        ]
    }

    fn range(start_row: u32, start_col: u32, end_row: u32, end_col: u32) -> RangeRef {
        RangeRef {
            start: cell(start_row, start_col),
            end: cell(end_row, end_col),
        }
    }

    fn cell(row: u32, col: u32) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    struct GridContext {
        cells: Vec<((u32, u32), Value)>,
    }

    impl EvalContext for GridContext {
        fn cell_value(&self, r: CellRef) -> Value {
            self.cells
                .iter()
                .find_map(|((row, col), value)| {
                    (*row == r.row && *col == r.col).then(|| value.clone())
                })
                .unwrap_or(Value::Blank)
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
