use std::cmp::Ordering;

use crate::functions::prelude::*;

const MIN_ARGS: usize = 3;
const MAX_ARGS: usize = 4;

pub struct Hlookup;

impl Function for Hlookup {
    fn name(&self) -> &'static str {
        "HLOOKUP"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let lookup_value = args[0].as_value();
        let table = match &args[1] {
            Arg::Range(range) => range,
            Arg::Value(_) => return Value::Error(ErrorValue::Value),
        };

        let return_row = match return_row_index(&args[2], table.rows(), ctx) {
            Ok(index) => index,
            Err(error) => return Value::Error(error),
        };

        let range_lookup = match args.get(3) {
            Some(arg) => match ctx.to_bool(&arg.as_value()) {
                Ok(value) => value,
                Err(error) => return Value::Error(error),
            },
            None => true,
        };

        let matched_col = if range_lookup {
            match approximate_match_col(table, &lookup_value) {
                Ok(col) => col,
                Err(error) => return Value::Error(error),
            }
        } else {
            match exact_match_col(table, &lookup_value) {
                Ok(col) => col,
                Err(error) => return Value::Error(error),
            }
        };

        match matched_col {
            Some(col) => return_value(table, return_row, col),
            None => Value::Error(ErrorValue::Na),
        }
    }
}

fn return_row_index(
    arg: &Arg<'_>,
    table_rows: u32,
    ctx: &FnContext<'_>,
) -> Result<u32, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    let index = number.trunc();
    if !index.is_finite() || index < 1.0 {
        return Err(ErrorValue::Value);
    }
    if index > f64::from(table_rows) {
        return Err(ErrorValue::Ref);
    }

    Ok(index as u32 - 1)
}

fn exact_match_col(table: &RangeView<'_>, lookup_value: &Value) -> Result<Option<u32>, ErrorValue> {
    for col in 0..table.cols() {
        if exact_matches(lookup_value, &table.get(0, col))? {
            return Ok(Some(col));
        }
    }

    Ok(None)
}

fn exact_matches(lookup_value: &Value, candidate: &Value) -> Result<bool, ErrorValue> {
    if let Some(error) = lookup_value.as_error().or_else(|| candidate.as_error()) {
        return Err(error);
    }

    Ok(match (lookup_value, candidate) {
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::Text(pattern), Value::Text(text)) => wildcard_match(pattern, text),
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Blank, Value::Blank) => true,
        _ => false,
    })
}

fn approximate_match_col(
    table: &RangeView<'_>,
    lookup_value: &Value,
) -> Result<Option<u32>, ErrorValue> {
    let mut candidate = None;

    for col in 0..table.cols() {
        match compare_lookup_key(&table.get(0, col), lookup_value)? {
            Some(Ordering::Greater) => break,
            Some(Ordering::Less | Ordering::Equal) => candidate = Some(col),
            None => {}
        }
    }

    Ok(candidate)
}

fn compare_lookup_key(
    candidate: &Value,
    lookup_value: &Value,
) -> Result<Option<Ordering>, ErrorValue> {
    if let Some(error) = candidate.as_error().or_else(|| lookup_value.as_error()) {
        return Err(error);
    }

    Ok(match (candidate, lookup_value) {
        (Value::Number(candidate), Value::Number(lookup)) => candidate.partial_cmp(lookup),
        (Value::Text(candidate), Value::Text(lookup)) => {
            Some(lower_text(candidate).cmp(&lower_text(lookup)))
        }
        (Value::Boolean(candidate), Value::Boolean(lookup)) => Some(candidate.cmp(lookup)),
        (Value::Blank, Value::Blank) => Some(Ordering::Equal),
        _ => None,
    })
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
                WildcardToken::Literal(literal) => literal == &text[j - 1] && matched[i - 1][j - 1],
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

fn return_value(table: &RangeView<'_>, row: u32, col: u32) -> Value {
    match table.get(row, col) {
        Value::Blank => Value::Number(0.0),
        value => value,
    }
}

static HLOOKUP: Hlookup = Hlookup;
inventory::submit! { FunctionEntry(&HLOOKUP) }

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
        assert_eq!(HLOOKUP.name(), "HLOOKUP");
        assert_eq!(HLOOKUP.arity(), (3, Some(4)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(
            call_args(TestContext::default(), vec![]),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_args(
                TestContext::default(),
                vec![
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_args(
                TestContext::default(),
                vec![
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Number(3.0)),
                    Arg::Value(Value::Boolean(false)),
                    Arg::Value(Value::Number(5.0)),
                ],
            ),
            err(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_scalar_table_arguments() {
        assert_eq!(
            call_args(
                TestContext::default(),
                vec![
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                    Arg::Value(Value::Number(1.0)),
                ],
            ),
            err(ErrorValue::Value)
        );
    }

    #[test]
    fn validates_and_truncates_row_index() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("sku")),
            ((1, 0), text("first")),
            ((2, 0), text("second")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("sku")),
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(2.9)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("first")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("sku")),
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(0.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("sku")),
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(f64::INFINITY)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Value)
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("sku")),
                    range_arg(&ctx, (0, 0), (2, 0)),
                    Arg::Value(Value::Number(4.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Ref)
        );
    }

    #[test]
    fn exact_mode_returns_first_case_insensitive_text_match() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("SKU-2")),
            ((1, 0), text("first")),
            ((0, 1), text("sku-2")),
            ((1, 1), text("second")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("sku-2")),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("first")
        );
    }

    #[test]
    fn exact_mode_supports_lookup_value_wildcards_and_tilde_escapes() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), text("Alpha")),
            ((1, 0), text("star")),
            ((0, 1), text("Alpine")),
            ((1, 1), text("question")),
            ((0, 2), text("file*")),
            ((1, 2), text("literal star")),
            ((0, 3), text("what?")),
            ((1, 3), text("literal question")),
            ((0, 4), text("Q~1")),
            ((1, 4), text("literal tilde")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("Al*")),
                    range_arg(&ctx, (0, 0), (1, 4)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("star")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("A?pine")),
                    range_arg(&ctx, (0, 0), (1, 4)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("question")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("file~*")),
                    range_arg(&ctx, (0, 0), (1, 4)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("literal star")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("what~?")),
                    range_arg(&ctx, (0, 0), (1, 4)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("literal question")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("Q~~1")),
                    range_arg(&ctx, (0, 0), (1, 4)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("literal tilde")
        );
    }

    #[test]
    fn exact_mode_keeps_numbers_and_numeric_text_distinct() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(42.0)),
            ((1, 0), text("number")),
            ((0, 1), text("42")),
            ((1, 1), text("text")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("42")),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            text("text")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Number(42.0)),
                    range_arg(&ctx, (0, 1), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            err(ErrorValue::Na)
        );
    }

    #[test]
    fn approximate_mode_returns_last_comparable_key_less_than_or_equal() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), text("a")),
            ((0, 1), Value::Number(20.0)),
            ((1, 1), text("b")),
            ((0, 2), Value::Number(30.0)),
            ((1, 2), text("c")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Number(25.0)),
                    range_arg(&ctx, (0, 0), (1, 2)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("b")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Number(5.0)),
                    range_arg(&ctx, (0, 0), (1, 2)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            err(ErrorValue::Na)
        );
    }

    #[test]
    fn approximate_mode_compares_only_same_broad_type() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Number(10.0)),
            ((1, 0), text("number")),
            ((0, 1), text("20")),
            ((1, 1), text("text")),
            ((0, 2), text("30")),
            ((1, 2), text("later text")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(text("25")),
                    range_arg(&ctx, (0, 0), (1, 2)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("text")
        );
    }

    #[test]
    fn approximate_mode_orders_booleans_false_before_true() {
        let ctx = TestContext::with_cells(vec![
            ((0, 0), Value::Boolean(false)),
            ((1, 0), text("false col")),
            ((0, 1), Value::Boolean(true)),
            ((1, 1), text("true col")),
        ]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Boolean(true)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("true col")
        );
        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Boolean(false)),
                    range_arg(&ctx, (0, 0), (1, 1)),
                    Arg::Value(Value::Number(2.0)),
                ],
            ),
            text("false col")
        );
    }

    #[test]
    fn exact_blank_match_and_blank_return_cell_returns_zero() {
        let ctx = TestContext::with_cells(vec![((0, 0), Value::Blank)]);

        assert_eq!(
            call_hlookup(
                &ctx,
                vec![
                    Arg::Value(Value::Blank),
                    range_arg(&ctx, (0, 0), (1, 0)),
                    Arg::Value(Value::Number(2.0)),
                    Arg::Value(Value::Boolean(false)),
                ],
            ),
            Value::Number(0.0)
        );
    }

    fn call_args<'a>(ctx: TestContext, args: Vec<Arg<'a>>) -> Value {
        let fn_ctx = FnContext::new(&ctx);
        HLOOKUP.call(&args, &fn_ctx)
    }

    fn call_hlookup(ctx: &TestContext, args: Vec<Arg<'_>>) -> Value {
        let fn_ctx = FnContext::new(ctx);
        HLOOKUP.call(&args, &fn_ctx)
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
