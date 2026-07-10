use crate::functions::prelude::*;

pub struct Npv;

impl Function for Npv {
    fn name(&self) -> &'static str {
        "NPV"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(255))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(2..=255).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match npv(args, ctx) {
            Ok(value) => Value::Number(value),
            Err(error) => Value::Error(error),
        }
    }
}

fn npv(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let rate = ctx.to_number(&args[0].as_value())?;
    let discount_base = 1.0 + rate;
    if !rate.is_finite() || !discount_base.is_finite() || discount_base <= 0.0 {
        return Err(ErrorValue::Num);
    }

    let mut total = 0.0;
    let mut discount = 1.0;

    for arg in &args[1..] {
        for cash_flow in numeric_cash_flows(arg) {
            if !cash_flow.is_finite() {
                return Err(ErrorValue::Num);
            }

            discount *= discount_base;
            if !discount.is_finite() || discount == 0.0 {
                return Err(ErrorValue::Num);
            }

            total += cash_flow / discount;
            if !total.is_finite() {
                return Err(ErrorValue::Num);
            }
        }
    }

    Ok(total)
}

fn numeric_cash_flows(arg: &Arg<'_>) -> Vec<f64> {
    match arg {
        Arg::Value(Value::Number(number)) => vec![*number],
        Arg::Value(_) => Vec::new(),
        Arg::Range(range) => range
            .iter()
            .filter_map(|value| match value {
                Value::Number(number) => Some(number),
                Value::Text(_) | Value::Boolean(_) | Value::Blank | Value::Error(_) => None,
            })
            .collect(),
    }
}

static NPV: Npv = Npv;
inventory::submit! { FunctionEntry(&NPV) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(NPV.name(), "NPV");
        assert_eq!(NPV.arity(), (2, Some(255)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![Value::Number(0.1)]),
            Value::Error(ErrorValue::Value)
        );

        let args = vec![Arg::Value(Value::Number(1.0)); 256];
        let ctx = GridContext { cells: Vec::new() };
        let fn_ctx = FnContext::new(&ctx);
        assert_eq!(NPV.call(&args, &fn_ctx), Value::Error(ErrorValue::Value));
    }

    #[test]
    fn matches_microsoft_future_cash_flow_example() {
        assert_close(
            call_values(vec![
                Value::Number(0.1),
                Value::Number(-10000.0),
                Value::Number(3000.0),
                Value::Number(4200.0),
                Value::Number(6800.0),
            ]),
            1188.4434123352207,
        );
    }

    #[test]
    fn range_cash_flows_match_direct_cash_flows() {
        let cells = vec![
            ((0, 0), Value::Number(-10000.0)),
            ((1, 0), Value::Number(3000.0)),
            ((2, 0), Value::Number(4200.0)),
            ((3, 0), Value::Number(6800.0)),
        ];

        assert_close(
            call_rate_and_range(Value::Number(0.1), cells, range(0, 0, 3, 0)),
            1188.4434123352207,
        );
    }

    #[test]
    fn discounts_first_counted_cash_flow_one_period() {
        assert_close(
            call_values(vec![
                Value::Number(0.1),
                Value::Number(-100.0),
                Value::Number(60.0),
                Value::Number(60.0),
            ]),
            3.7565740045078755,
        );
    }

    #[test]
    fn ignores_cash_flow_blanks_logicals_text_numeric_text_and_errors() {
        let ctx = GridContext {
            cells: vec![
                ((0, 0), Value::Number(-100.0)),
                ((0, 1), Value::Blank),
                ((0, 2), Value::Boolean(true)),
                ((1, 0), Value::Text("60".to_string())),
                ((1, 1), Value::Error(ErrorValue::Div0)),
                ((1, 2), Value::Number(60.0)),
            ],
        };
        let fn_ctx = FnContext::new(&ctx);
        let args = vec![
            Arg::Value(Value::Number(0.1)),
            Arg::Range(RangeView::new(&ctx, range(0, 0, 1, 2))),
            Arg::Value(Value::Text("60".to_string())),
            Arg::Value(Value::Boolean(false)),
            Arg::Value(Value::Error(ErrorValue::Na)),
            Arg::Value(Value::Number(60.0)),
        ];

        assert_close(NPV.call(&args, &fn_ctx), 3.7565740045078755);
    }

    #[test]
    fn scans_ranges_row_major() {
        assert_close(
            call_rate_and_range(
                Value::Number(1.0),
                vec![
                    ((0, 0), Value::Number(100.0)),
                    ((0, 1), Value::Number(200.0)),
                    ((1, 0), Value::Number(300.0)),
                    ((1, 1), Value::Number(400.0)),
                ],
                range(0, 0, 1, 1),
            ),
            162.5,
        );
    }

    #[test]
    fn returns_zero_when_no_cash_flows_are_counted() {
        assert_eq!(
            call_values(vec![
                Value::Number(0.1),
                Value::Blank,
                Value::Boolean(true),
                Value::Text("100".to_string()),
                Value::Error(ErrorValue::Div0),
            ]),
            Value::Number(0.0)
        );
    }

    #[test]
    fn propagates_rate_errors_and_rejects_invalid_rate_domain() {
        assert_eq!(
            call_values(vec![Value::Error(ErrorValue::Div0), Value::Number(100.0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(vec![Value::Number(f64::INFINITY), Value::Number(100.0)]),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            call_values(vec![Value::Number(-1.0), Value::Number(100.0)]),
            Value::Error(ErrorValue::Num)
        );
    }

    #[test]
    fn rejects_non_finite_counted_cash_flows() {
        assert_eq!(
            call_values(vec![Value::Number(0.1), Value::Number(f64::INFINITY)]),
            Value::Error(ErrorValue::Num)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = GridContext { cells: Vec::new() };
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        NPV.call(&args, &fn_ctx)
    }

    fn call_rate_and_range(
        rate: Value,
        cells: Vec<((u32, u32), Value)>,
        range_ref: RangeRef,
    ) -> Value {
        let ctx = GridContext { cells };
        let fn_ctx = FnContext::new(&ctx);
        NPV.call(
            &[
                Arg::Value(rate),
                Arg::Range(RangeView::new(&ctx, range_ref)),
            ],
            &fn_ctx,
        )
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
