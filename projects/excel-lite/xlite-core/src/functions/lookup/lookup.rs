use std::cmp::Ordering;

use crate::functions::prelude::*;

const MIN_ARGS: usize = 2;
const MAX_ARGS: usize = 3;

pub struct Lookup;

impl Function for Lookup {
    fn name(&self) -> &'static str {
        "LOOKUP"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], _ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let lookup_value = args[0].as_value();
        if let Some(error) = lookup_value.as_error() {
            return Value::Error(error);
        }

        match args.len() {
            2 => two_argument_lookup(&lookup_value, &args[1]),
            3 => vector_lookup(&lookup_value, &args[1], Some(&args[2])),
            _ => Value::Error(ErrorValue::Value),
        }
    }
}

fn two_argument_lookup(lookup_value: &Value, array: &Arg<'_>) -> Value {
    match array {
        Arg::Value(_) => vector_lookup(lookup_value, array, None),
        Arg::Range(range) if range.rows() == 1 || range.cols() == 1 => {
            vector_lookup(lookup_value, array, None)
        }
        Arg::Range(range) if range.cols() > range.rows() => {
            array_lookup_row(lookup_value, range)
        }
        Arg::Range(range) => array_lookup_column(lookup_value, range),
    }
}

fn vector_lookup(
    lookup_value: &Value,
    lookup_vector: &Arg<'_>,
    result_vector: Option<&Arg<'_>>,
) -> Value {
    let lookup_values = match vector_values(lookup_vector, true) {
        Ok(values) => values,
        Err(error) => return Value::Error(error),
    };

    let result_values = match result_vector {
        Some(arg) => match vector_values(arg, false) {
            Ok(values) => Some(values),
            Err(error) => return Value::Error(error),
        },
        None => None,
    };

    if let Some(values) = &result_values {
        if values.len() != lookup_values.len() {
            return Value::Error(ErrorValue::Value);
        }
    }

    let Some(position) = approximate_position(lookup_value, &lookup_values) else {
        return Value::Error(ErrorValue::Na);
    };

    let selected = match &result_values {
        Some(values) => values[position].clone(),
        None => lookup_values[position].clone(),
    };
    return_value(selected)
}

fn array_lookup_row(lookup_value: &Value, range: &RangeView<'_>) -> Value {
    let mut lookup_values = Vec::with_capacity(range.cols() as usize);
    for col in 0..range.cols() {
        let value = range.get(0, col);
        if let Some(error) = value.as_error() {
            return Value::Error(error);
        }
        lookup_values.push(value);
    }

    let Some(position) = approximate_position(lookup_value, &lookup_values) else {
        return Value::Error(ErrorValue::Na);
    };

    return_value(range.get(range.rows() - 1, position as u32))
}

fn array_lookup_column(lookup_value: &Value, range: &RangeView<'_>) -> Value {
    let mut lookup_values = Vec::with_capacity(range.rows() as usize);
    for row in 0..range.rows() {
        let value = range.get(row, 0);
        if let Some(error) = value.as_error() {
            return Value::Error(error);
        }
        lookup_values.push(value);
    }

    let Some(position) = approximate_position(lookup_value, &lookup_values) else {
        return Value::Error(ErrorValue::Na);
    };

    return_value(range.get(position as u32, range.cols() - 1))
}

fn vector_values(arg: &Arg<'_>, propagate_errors: bool) -> Result<Vec<Value>, ErrorValue> {
    match arg {
        Arg::Value(value) => {
            if propagate_errors {
                if let Some(error) = value.as_error() {
                    return Err(error);
                }
            }
            Ok(vec![value.clone()])
        }
        Arg::Range(range) => {
            let rows = range.rows();
            let cols = range.cols();
            if rows > 1 && cols > 1 {
                return Err(ErrorValue::Value);
            }

            let len = rows.max(cols);
            let mut values = Vec::with_capacity(len as usize);
            for index in 0..len {
                let value = if rows == 1 {
                    range.get(0, index)
                } else {
                    range.get(index, 0)
                };
                if propagate_errors {
                    if let Some(error) = value.as_error() {
                        return Err(error);
                    }
                }
                values.push(value);
            }
            Ok(values)
        }
    }
}

fn approximate_position(lookup_value: &Value, values: &[Value]) -> Option<usize> {
    let mut candidate = None;

    for (index, value) in values.iter().enumerate() {
        let Some(ordering) = compare_same_type(value, lookup_value) else {
            continue;
        };

        if ordering == Ordering::Greater {
            break;
        }
        candidate = Some(index);
    }

    candidate
}

fn compare_same_type(candidate: &Value, lookup_value: &Value) -> Option<Ordering> {
    match (candidate, lookup_value) {
        (Value::Number(candidate), Value::Number(lookup)) => candidate.partial_cmp(lookup),
        (Value::Text(candidate), Value::Text(lookup)) => {
            Some(lower_text(candidate).cmp(&lower_text(lookup)))
        }
        (Value::Boolean(candidate), Value::Boolean(lookup)) => Some(candidate.cmp(lookup)),
        (Value::Blank, Value::Blank) => Some(Ordering::Equal),
        _ => None,
    }
}

fn lower_text(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

fn return_value(value: Value) -> Value {
    match value {
        Value::Error(error) => Value::Error(error),
        Value::Blank => Value::Number(0.0),
        value => value,
    }
}

static LOOKUP: Lookup = Lookup;
inventory::submit! { FunctionEntry(&LOOKUP) }

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        eval::EvalContext,
        model::Coord,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(LOOKUP.name(), "LOOKUP");
        assert_eq!(LOOKUP.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_values(vec![num(1.0)]), err(ErrorValue::Value));
        assert_eq!(
            call_values(vec![num(1.0), num(1.0), num(1.0), num(1.0)]),
            err(ErrorValue::Value)
        );
    }

    #[test]
    fn vector_form_returns_last_less_than_or_equal_result() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(4.14)),
            ((1, 0), num(4.19)),
            ((2, 0), num(5.17)),
            ((0, 1), text("red")),
            ((1, 1), text("blue")),
            ((2, 1), text("green")),
        ]);

        assert_eq!(
            lookup_range(
                &ctx,
                num(4.19),
                range_arg(&ctx, (0, 0), (2, 0)),
                Some(range_arg(&ctx, (0, 1), (2, 1))),
            ),
            text("blue")
        );
        assert_eq!(
            lookup_range(
                &ctx,
                num(4.5),
                range_arg(&ctx, (0, 0), (2, 0)),
                Some(range_arg(&ctx, (0, 1), (2, 1))),
            ),
            text("blue")
        );
    }

    #[test]
    fn vector_form_returns_na_when_no_candidate_exists() {
        let ctx = TestContext::with_cells(vec![((0, 0), num(10.0)), ((1, 0), num(20.0))]);

        assert_eq!(
            lookup_range(
                &ctx,
                num(5.0),
                range_arg(&ctx, (0, 0), (1, 0)),
                None,
            ),
            err(ErrorValue::Na)
        );
        assert_eq!(
            lookup_range(
                &ctx,
                text("5"),
                range_arg(&ctx, (0, 0), (1, 0)),
                None,
            ),
            err(ErrorValue::Na)
        );
    }

    #[test]
    fn omitted_result_vector_returns_matched_lookup_value() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(10.0)),
            ((1, 0), num(20.0)),
            ((2, 0), num(30.0)),
        ]);

        assert_eq!(
            lookup_range(
                &ctx,
                num(25.0),
                range_arg(&ctx, (0, 0), (2, 0)),
                None,
            ),
            num(20.0)
        );
    }

    #[test]
    fn scalar_vectors_behave_as_one_item_vectors() {
        assert_eq!(call_values(vec![num(7.0), num(7.0)]), num(7.0));
        assert_eq!(call_values(vec![num(8.0), num(7.0), text("hit")]), text("hit"));
        assert_eq!(
            call_values(vec![num(6.0), num(7.0), text("miss")]),
            err(ErrorValue::Na)
        );
    }

    #[test]
    fn horizontal_lookup_and_result_vectors_are_supported() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("alpha")),
            ((0, 1), text("beta")),
            ((0, 2), text("delta")),
            ((1, 0), num(1.0)),
            ((1, 1), num(2.0)),
            ((1, 2), num(3.0)),
        ]);

        assert_eq!(
            lookup_range(
                &ctx,
                text("charlie"),
                range_arg(&ctx, (0, 0), (0, 2)),
                Some(range_arg(&ctx, (1, 0), (1, 2))),
            ),
            num(2.0)
        );
    }

    #[test]
    fn rejects_invalid_vector_shapes_and_length_mismatch() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(1.0)),
            ((0, 1), num(2.0)),
            ((1, 0), num(3.0)),
            ((1, 1), num(4.0)),
        ]);

        assert_eq!(
            lookup_range(
                &ctx,
                num(1.0),
                range_arg(&ctx, (0, 0), (1, 1)),
                Some(range_arg(&ctx, (0, 0), (1, 0))),
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            lookup_range(
                &ctx,
                num(1.0),
                range_arg(&ctx, (0, 0), (1, 0)),
                Some(range_arg(&ctx, (0, 0), (0, 0))),
            ),
            err(ErrorValue::Value)
        );
    }

    #[test]
    fn compares_only_same_broad_type_with_case_insensitive_text() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(40.0)),
            ((1, 0), text("42")),
            ((2, 0), num(50.0)),
            ((0, 1), text("num40")),
            ((1, 1), text("text42")),
            ((2, 1), text("num50")),
        ]);

        assert_eq!(
            lookup_range(
                &ctx,
                num(42.0),
                range_arg(&ctx, (0, 0), (2, 0)),
                Some(range_arg(&ctx, (0, 1), (2, 1))),
            ),
            text("num40")
        );
        assert_eq!(
            lookup_range(
                &ctx,
                text("42"),
                range_arg(&ctx, (0, 0), (2, 0)),
                Some(range_arg(&ctx, (0, 1), (2, 1))),
            ),
            text("text42")
        );

        let text_ctx = TestContext::with_cells(vec![
            ((0, 0), text("Alpha")),
            ((1, 0), text("beta")),
            ((0, 1), text("A")),
            ((1, 1), text("B")),
        ]);
        assert_eq!(
            lookup_range(
                &text_ctx,
                text("BETA"),
                range_arg(&text_ctx, (0, 0), (1, 0)),
                Some(range_arg(&text_ctx, (0, 1), (1, 1))),
            ),
            text("B")
        );
    }

    #[test]
    fn orders_booleans_false_before_true_and_matches_blanks() {
        let bool_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(false)),
            ((1, 0), Value::Boolean(true)),
            ((0, 1), text("false")),
            ((1, 1), text("true")),
        ]);
        assert_eq!(
            lookup_range(
                &bool_ctx,
                Value::Boolean(true),
                range_arg(&bool_ctx, (0, 0), (1, 0)),
                Some(range_arg(&bool_ctx, (0, 1), (1, 1))),
            ),
            text("true")
        );

        let blank_ctx = TestContext::with_cells(vec![((0, 1), text("blank"))]);
        assert_eq!(
            lookup_range(
                &blank_ctx,
                Value::Blank,
                range_arg(&blank_ctx, (0, 0), (0, 0)),
                Some(range_arg(&blank_ctx, (0, 1), (0, 1))),
            ),
            text("blank")
        );
    }

    #[test]
    fn array_form_chooses_search_orientation_by_shape() {
        let wide = TestContext::with_cells(vec![
            ((0, 0), num(10.0)),
            ((0, 1), num(20.0)),
            ((0, 2), num(30.0)),
            ((1, 0), text("ten")),
            ((1, 1), text("twenty")),
            ((1, 2), text("thirty")),
        ]);
        assert_eq!(
            lookup_range(&wide, num(25.0), range_arg(&wide, (0, 0), (1, 2)), None),
            text("twenty")
        );

        let square = TestContext::with_cells(vec![
            ((0, 0), num(10.0)),
            ((1, 0), num(20.0)),
            ((2, 0), num(30.0)),
            ((0, 2), text("ten")),
            ((1, 2), text("twenty")),
            ((2, 2), text("thirty")),
        ]);
        assert_eq!(
            lookup_range(&square, num(25.0), range_arg(&square, (0, 0), (2, 2)), None),
            text("twenty")
        );
    }

    #[test]
    fn selected_blank_result_returns_zero() {
        let ctx = TestContext::with_cells(vec![((0, 0), text("key"))]);

        assert_eq!(
            lookup_range(
                &ctx,
                text("key"),
                range_arg(&ctx, (0, 0), (0, 0)),
                Some(range_arg(&ctx, (0, 1), (0, 1))),
            ),
            num(0.0)
        );

        let blank_ctx = TestContext::default();
        assert_eq!(
            lookup_range(
                &blank_ctx,
                Value::Blank,
                range_arg(&blank_ctx, (0, 0), (0, 0)),
                None,
            ),
            num(0.0)
        );
    }

    #[test]
    fn propagates_lookup_and_selected_result_errors() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(1.0)),
            ((1, 0), Value::Error(ErrorValue::Ref)),
            ((0, 1), text("ok")),
            ((1, 1), Value::Error(ErrorValue::Div0)),
        ]);

        assert_eq!(
            call_values(vec![Value::Error(ErrorValue::Name), num(1.0)]),
            err(ErrorValue::Name)
        );
        assert_eq!(
            lookup_range(&ctx, num(2.0), range_arg(&ctx, (0, 0), (1, 0)), None),
            err(ErrorValue::Ref)
        );

        let result_ctx = TestContext::with_cells(vec![
            ((0, 0), num(1.0)),
            ((1, 0), num(2.0)),
            ((0, 1), text("ok")),
            ((1, 1), Value::Error(ErrorValue::Div0)),
        ]);
        assert_eq!(
            lookup_range(
                &result_ctx,
                num(2.0),
                range_arg(&result_ctx, (0, 0), (1, 0)),
                Some(range_arg(&result_ctx, (0, 1), (1, 1))),
            ),
            err(ErrorValue::Div0)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        LOOKUP.call(&args, &fn_ctx)
    }

    fn lookup_range(
        ctx: &TestContext,
        lookup_value: Value,
        lookup_arg: Arg<'_>,
        result_arg: Option<Arg<'_>>,
    ) -> Value {
        let fn_ctx = FnContext::new(ctx);
        let mut args = vec![Arg::Value(lookup_value), lookup_arg];
        if let Some(arg) = result_arg {
            args.push(arg);
        }
        LOOKUP.call(&args, &fn_ctx)
    }

    fn range_arg(ctx: &TestContext, start: (u32, u32), end: (u32, u32)) -> Arg<'_> {
        Arg::Range(RangeView::new(
            ctx,
            RangeRef {
                start: cell_ref(start),
                end: cell_ref(end),
            },
        ))
    }

    fn cell_ref((row, col): (u32, u32)) -> CellRef {
        CellRef {
            sheet: None,
            col,
            row,
            col_abs: false,
            row_abs: false,
        }
    }

    fn num(value: f64) -> Value {
        Value::Number(value)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn err(error: ErrorValue) -> Value {
        Value::Error(error)
    }

    #[derive(Default)]
    struct TestContext {
        cells: HashMap<(u32, u32), Value>,
    }

    impl TestContext {
        fn with_cells(cells: Vec<((u32, u32), Value)>) -> Self {
            Self {
                cells: cells.into_iter().collect(),
            }
        }
    }

    impl EvalContext for TestContext {
        fn cell_value(&self, r: CellRef) -> Value {
            self.cells
                .get(&(r.row, r.col))
                .cloned()
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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
