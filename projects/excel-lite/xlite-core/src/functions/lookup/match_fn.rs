use std::cmp::Ordering;

use crate::functions::prelude::*;

pub struct MatchFn;

impl Function for MatchFn {
    fn name(&self) -> &'static str {
        "MATCH"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (2, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(2..=3).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let lookup_value = args[0].as_value();
        if let Some(error) = lookup_value.as_error() {
            return Value::Error(error);
        }

        let values = match lookup_vector(&args[1]) {
            Ok(values) => values,
            Err(error) => return Value::Error(error),
        };

        let match_type = match args.get(2) {
            Some(arg) => match coerce_match_type(arg, ctx) {
                Ok(match_type) => match_type,
                Err(error) => return Value::Error(error),
            },
            None => MatchType::Ascending,
        };

        let position = match match_type {
            MatchType::Exact => exact_position(&lookup_value, &values),
            MatchType::Ascending => approximate_position(&lookup_value, &values, ApproxOrder::Ascending),
            MatchType::Descending => approximate_position(&lookup_value, &values, ApproxOrder::Descending),
        };

        match position {
            Some(position) => Value::Number(position as f64),
            None => Value::Error(ErrorValue::Na),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchType {
    Exact,
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApproxOrder {
    Ascending,
    Descending,
}

fn coerce_match_type(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<MatchType, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    if !number.is_finite() {
        return Err(ErrorValue::Value);
    }

    match number.trunc() {
        -1.0 => Ok(MatchType::Descending),
        0.0 => Ok(MatchType::Exact),
        1.0 => Ok(MatchType::Ascending),
        _ => Err(ErrorValue::Value),
    }
}

fn lookup_vector(arg: &Arg<'_>) -> Result<Vec<Value>, ErrorValue> {
    match arg {
        Arg::Value(Value::Error(error)) => Err(*error),
        Arg::Value(value) => Ok(vec![value.clone()]),
        Arg::Range(range) => {
            let rows = range.rows();
            let cols = range.cols();
            if rows > 1 && cols > 1 {
                return Err(ErrorValue::Value);
            }

            let mut values = Vec::with_capacity(rows.max(cols) as usize);
            if rows == 1 {
                for col in 0..cols {
                    push_lookup_value(&mut values, range.get(0, col))?;
                }
            } else {
                for row in 0..rows {
                    push_lookup_value(&mut values, range.get(row, 0))?;
                }
            }
            Ok(values)
        }
    }
}

fn push_lookup_value(values: &mut Vec<Value>, value: Value) -> Result<(), ErrorValue> {
    if let Value::Error(error) = value {
        return Err(error);
    }
    values.push(value);
    Ok(())
}

fn exact_position(lookup_value: &Value, values: &[Value]) -> Option<usize> {
    values
        .iter()
        .position(|value| exact_matches(lookup_value, value))
        .map(|index| index + 1)
}

fn exact_matches(lookup_value: &Value, candidate: &Value) -> bool {
    match (lookup_value, candidate) {
        (Value::Number(lhs), Value::Number(rhs)) => lhs == rhs,
        (Value::Text(pattern), Value::Text(text)) => wildcard_match(pattern, text),
        (Value::Boolean(lhs), Value::Boolean(rhs)) => lhs == rhs,
        (Value::Blank, Value::Blank) => true,
        _ => false,
    }
}

fn approximate_position(
    lookup_value: &Value,
    values: &[Value],
    order: ApproxOrder,
) -> Option<usize> {
    let mut candidate = None;

    for (index, value) in values.iter().enumerate() {
        let Some(ordering) = compare_same_type(value, lookup_value) else {
            continue;
        };

        match order {
            ApproxOrder::Ascending => {
                if ordering == Ordering::Greater {
                    break;
                }
                candidate = Some(index + 1);
            }
            ApproxOrder::Descending => {
                if ordering == Ordering::Less {
                    break;
                }
                candidate = Some(index + 1);
            }
        }
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

#[derive(Debug, PartialEq, Eq)]
enum WildcardToken {
    Literal(String),
    AnyChar,
    AnyChars,
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let tokens = parse_wildcard_pattern(pattern);
    let text: Vec<_> = text.chars().map(lower_char).collect();
    let mut matched = vec![vec![false; text.len() + 1]; tokens.len() + 1];
    matched[0][0] = true;

    for i in 1..=tokens.len() {
        if matches!(tokens[i - 1], WildcardToken::AnyChars) {
            matched[i][0] = matched[i - 1][0];
        }
    }

    for i in 1..=tokens.len() {
        for j in 1..=text.len() {
            matched[i][j] = match &tokens[i - 1] {
                WildcardToken::Literal(literal) => {
                    literal == &text[j - 1] && matched[i - 1][j - 1]
                }
                WildcardToken::AnyChar => matched[i - 1][j - 1],
                WildcardToken::AnyChars => matched[i - 1][j] || matched[i][j - 1],
            };
        }
    }

    matched[tokens.len()][text.len()]
}

fn parse_wildcard_pattern(pattern: &str) -> Vec<WildcardToken> {
    let mut tokens = Vec::new();
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '?' => tokens.push(WildcardToken::AnyChar),
            '*' => tokens.push(WildcardToken::AnyChars),
            '~' => match chars.peek().copied() {
                Some('?') | Some('*') | Some('~') => {
                    let literal = chars.next().expect("peeked character exists");
                    tokens.push(WildcardToken::Literal(lower_char(literal)));
                }
                _ => tokens.push(WildcardToken::Literal(lower_char('~'))),
            },
            _ => tokens.push(WildcardToken::Literal(lower_char(ch))),
        }
    }

    tokens
}

fn lower_char(ch: char) -> String {
    ch.to_lowercase().collect()
}

fn lower_text(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

static MATCH: MatchFn = MatchFn;
inventory::submit! { FunctionEntry(&MATCH) }

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
    fn reports_match_metadata() {
        assert_eq!(MATCH.name(), "MATCH");
        assert_eq!(MATCH.arity(), (2, Some(3)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_values(vec![Value::Number(1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Number(0.0),
                Value::Number(1.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn propagates_direct_scalar_errors() {
        assert_eq!(
            call_values(vec![
                Value::Error(ErrorValue::Ref),
                Value::Number(1.0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Error(ErrorValue::Div0),
                Value::Number(0.0),
            ]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_values(vec![
                Value::Number(1.0),
                Value::Number(1.0),
                Value::Error(ErrorValue::Na),
            ]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn exact_match_returns_first_matching_position_by_same_type() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(5.0)),
            ((1, 0), text("5")),
            ((2, 0), num(5.0)),
        ]);

        assert_eq!(match_range(&ctx, num(5.0), (0, 0), (2, 0), 0.0), num(1.0));
        assert_eq!(match_range(&ctx, text("5"), (0, 0), (2, 0), 0.0), num(2.0));
    }

    #[test]
    fn exact_text_match_is_case_insensitive() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("Alpha")),
            ((1, 0), text("Beta")),
        ]);

        assert_eq!(
            match_range(&ctx, text("alpha"), (0, 0), (1, 0), 0.0),
            num(1.0)
        );
    }

    #[test]
    fn exact_text_wildcards_match_entire_text_and_support_tilde_escapes() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("Alpha")),
            ((1, 0), text("Alpine")),
            ((2, 0), text("file*")),
            ((3, 0), text("what?")),
            ((4, 0), text("Q~1")),
        ]);

        assert_eq!(
            match_range(&ctx, text("Al*"), (0, 0), (4, 0), 0.0),
            num(1.0)
        );
        assert_eq!(
            match_range(&ctx, text("A?pine"), (0, 0), (4, 0), 0.0),
            num(2.0)
        );
        assert_eq!(
            match_range(&ctx, text("file~*"), (0, 0), (4, 0), 0.0),
            num(3.0)
        );
        assert_eq!(
            match_range(&ctx, text("what~?"), (0, 0), (4, 0), 0.0),
            num(4.0)
        );
        assert_eq!(
            match_range(&ctx, text("Q~~1"), (0, 0), (4, 0), 0.0),
            num(5.0)
        );
        assert_eq!(
            match_range(&ctx, text("Al"), (0, 0), (4, 0), 0.0),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn approximate_ascending_returns_last_less_than_or_equal_same_type_position() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(10.0)),
            ((1, 0), text("skip")),
            ((2, 0), num(20.0)),
            ((3, 0), num(40.0)),
        ]);

        assert_eq!(
            match_range(&ctx, num(25.0), (0, 0), (3, 0), 1.0),
            num(3.0)
        );
    }

    #[test]
    fn approximate_descending_returns_last_greater_than_or_equal_same_type_position() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(50.0)),
            ((1, 0), Value::Boolean(false)),
            ((2, 0), num(40.0)),
            ((3, 0), num(10.0)),
        ]);

        assert_eq!(
            match_range(&ctx, num(40.0), (0, 0), (3, 0), -1.0),
            num(3.0)
        );
    }

    #[test]
    fn approximate_mode_compares_text_booleans_and_blanks_by_same_type() {
        let text_ctx = TestContext::with_cells(vec![
            ((0, 0), text("alpha")),
            ((1, 0), text("Beta")),
            ((2, 0), text("delta")),
        ]);
        assert_eq!(
            match_range(&text_ctx, text("cat"), (0, 0), (2, 0), 1.0),
            num(2.0)
        );

        let bool_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(false)),
            ((1, 0), Value::Boolean(true)),
        ]);
        assert_eq!(
            match_range(&bool_ctx, Value::Boolean(true), (0, 0), (1, 0), 1.0),
            num(2.0)
        );

        let blank_ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Blank),
            ((1, 0), Value::Blank),
            ((2, 0), num(1.0)),
        ]);
        assert_eq!(
            match_range(&blank_ctx, Value::Blank, (0, 0), (2, 0), 1.0),
            num(2.0)
        );
    }

    #[test]
    fn approximate_mode_stops_only_on_comparable_sorted_boundary() {
        let ascending = TestContext::with_cells(vec![
            ((0, 0), num(1.0)),
            ((1, 0), text("not comparable")),
            ((2, 0), num(3.0)),
            ((3, 0), num(5.0)),
        ]);
        assert_eq!(
            match_range(&ascending, num(4.0), (0, 0), (3, 0), 1.0),
            num(3.0)
        );

        let descending = TestContext::with_cells(vec![
            ((0, 0), num(5.0)),
            ((1, 0), Value::Boolean(true)),
            ((2, 0), num(3.0)),
            ((3, 0), num(1.0)),
        ]);
        assert_eq!(
            match_range(&descending, num(4.0), (0, 0), (3, 0), -1.0),
            num(1.0)
        );
    }

    #[test]
    fn returns_na_when_no_match_or_no_candidate_is_found() {
        let ctx = TestContext::with_cells(vec![((0, 0), num(10.0)), ((1, 0), num(20.0))]);

        assert_eq!(
            match_range(&ctx, num(15.0), (0, 0), (1, 0), 0.0),
            Value::Error(ErrorValue::Na)
        );
        assert_eq!(
            match_range(&ctx, num(5.0), (0, 0), (1, 0), 1.0),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn validates_lookup_array_shape_and_scalar_array_behavior() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), num(1.0)),
            ((0, 1), num(2.0)),
            ((1, 0), num(3.0)),
            ((1, 1), num(4.0)),
        ]);
        let fn_ctx = FnContext::new(&ctx);

        assert_eq!(
            MATCH.call(
                &[
                    Arg::Value(num(2.0)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(num(0.0)),
                ],
                &fn_ctx,
            ),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_values(vec![num(7.0), num(7.0), num(0.0)]),
            num(1.0)
        );
    }

    #[test]
    fn accepts_horizontal_ranges_in_left_to_right_order() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("first")),
            ((0, 1), text("second")),
            ((0, 2), text("third")),
        ]);

        assert_eq!(
            match_range(&ctx, text("third"), (0, 0), (0, 2), 0.0),
            num(3.0)
        );
    }

    #[test]
    fn validates_match_type_after_truncating_toward_zero() {
        let ctx = TestContext::with_cells(vec![((0, 0), num(2.0))]);

        assert_eq!(
            match_range(&ctx, num(2.0), (0, 0), (0, 0), 0.9),
            num(1.0)
        );
        assert_eq!(
            match_range(&ctx, num(2.0), (0, 0), (0, 0), 2.0),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            match_range(&ctx, num(2.0), (0, 0), (0, 0), f64::INFINITY),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            match_range(&ctx, num(2.0), (0, 0), (0, 0), f64::NAN),
            Value::Error(ErrorValue::Value)
        );
    }

    fn call_values(values: Vec<Value>) -> Value {
        let ctx = TestContext::default();
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        MATCH.call(&args, &fn_ctx)
    }

    fn match_range(
        ctx: &TestContext,
        lookup_value: Value,
        start: (u32, u32),
        end: (u32, u32),
        match_type: f64,
    ) -> Value {
        let fn_ctx = FnContext::new(ctx);
        MATCH.call(
            &[
                Arg::Value(lookup_value),
                range_arg(ctx, start, end),
                Arg::Value(num(match_type)),
            ],
            &fn_ctx,
        )
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
