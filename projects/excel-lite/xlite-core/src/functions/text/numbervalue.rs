use crate::functions::prelude::*;

const DEFAULT_DECIMAL_SEPARATOR: char = '.';
const DEFAULT_GROUP_SEPARATOR: char = ',';

pub struct Numbervalue;

impl Function for Numbervalue {
    fn name(&self) -> &'static str {
        "NUMBERVALUE"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (1, Some(3))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(1..=3).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        match numbervalue(args, ctx) {
            Ok(number) => Value::Number(number),
            Err(error) => Value::Error(error),
        }
    }
}

fn numbervalue(args: &[Arg<'_>], ctx: &FnContext<'_>) -> Result<f64, ErrorValue> {
    let text = ctx.to_text(&args[0].as_value())?;
    let decimal_separator =
        separator_arg(args.get(1), DEFAULT_DECIMAL_SEPARATOR, ctx)?;
    let group_separator = separator_arg(args.get(2), DEFAULT_GROUP_SEPARATOR, ctx)?;

    validate_separators(decimal_separator, group_separator)?;
    parse_number_text(&text, decimal_separator, group_separator)
}

fn separator_arg(
    arg: Option<&Arg<'_>>,
    default: char,
    ctx: &FnContext<'_>,
) -> Result<char, ErrorValue> {
    let Some(arg) = arg else {
        return Ok(default);
    };

    let text = ctx.to_text(&arg.as_value())?;
    text.chars().next().ok_or(ErrorValue::Value)
}

fn validate_separators(
    decimal_separator: char,
    group_separator: char,
) -> Result<(), ErrorValue> {
    if decimal_separator == group_separator
        || is_forbidden_separator(decimal_separator)
        || is_forbidden_separator(group_separator)
    {
        Err(ErrorValue::Value)
    } else {
        Ok(())
    }
}

fn is_forbidden_separator(ch: char) -> bool {
    matches!(ch, '%' | '+' | '-') || ch.is_ascii_digit()
}

fn parse_number_text(
    text: &str,
    decimal_separator: char,
    group_separator: char,
) -> Result<f64, ErrorValue> {
    let mut compact: String = text.chars().filter(|ch| *ch != ' ').collect();
    if compact.is_empty() {
        return Ok(0.0);
    }

    let percent_count = strip_trailing_percents(&mut compact);
    if compact.is_empty() {
        return Err(ErrorValue::Value);
    }

    let normalized =
        normalize_decimal_text(&compact, decimal_separator, group_separator)?;
    let mut number = normalized
        .parse::<f64>()
        .map_err(|_| ErrorValue::Value)?;
    if !number.is_finite() {
        return Err(ErrorValue::Value);
    }

    for _ in 0..percent_count {
        number /= 100.0;
    }

    if number.is_finite() {
        Ok(number)
    } else {
        Err(ErrorValue::Value)
    }
}

fn strip_trailing_percents(text: &mut String) -> usize {
    let mut count = 0;
    while text.ends_with('%') {
        text.pop();
        count += 1;
    }
    count
}

fn normalize_decimal_text(
    text: &str,
    decimal_separator: char,
    group_separator: char,
) -> Result<String, ErrorValue> {
    let mut normalized = String::with_capacity(text.len());
    let mut saw_decimal = false;

    for ch in text.chars() {
        if ch == decimal_separator {
            if saw_decimal {
                return Err(ErrorValue::Value);
            }
            saw_decimal = true;
            normalized.push('.');
        } else if ch == group_separator {
            if saw_decimal {
                return Err(ErrorValue::Value);
            }
        } else {
            normalized.push(ch);
        }
    }

    Ok(normalized)
}

static NUMBERVALUE: Numbervalue = Numbervalue;
inventory::submit! { FunctionEntry(&NUMBERVALUE) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalContext;
    use crate::functions::build_registry;
    use crate::model::Coord;
    use crate::syntax::{CellRef, RangeRef};

    #[test]
    fn reports_metadata_and_registers() {
        assert_eq!(NUMBERVALUE.name(), "NUMBERVALUE");
        assert_eq!(NUMBERVALUE.arity(), (1, Some(3)));
        assert!(build_registry().contains_key("NUMBERVALUE"));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_numbervalue(vec![]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_numbervalue(vec![
                text("1"),
                text("."),
                text(","),
                text("extra"),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn parses_microsoft_examples_and_percent_suffixes() {
        assert_number_close(
            call_numbervalue(vec![text("2.500,27"), text(","), text(".")]),
            2500.27,
        );
        assert_number_close(call_numbervalue(vec![text("3.5%")]), 0.035);
        assert_number_close(call_numbervalue(vec![text("9%%")]), 0.0009);
    }

    #[test]
    fn returns_zero_for_empty_text_and_ignores_ascii_spaces() {
        assert_eq!(call_numbervalue(vec![text("")]), Value::Number(0.0));
        assert_eq!(call_numbervalue(vec![text("   ")]), Value::Number(0.0));
        assert_eq!(
            call_numbervalue(vec![text(" 3 000 ")]),
            Value::Number(3000.0)
        );
    }

    #[test]
    fn uses_only_first_character_of_separator_arguments() {
        assert_number_close(
            call_numbervalue(vec![text("1b234a56"), text("a-more"), text("b-more")]),
            1234.56,
        );
    }

    #[test]
    fn validates_separator_arguments() {
        assert_eq!(
            call_numbervalue(vec![text("1"), Value::Blank]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_numbervalue(vec![text("1"), text("."), text(".")]),
            Value::Error(ErrorValue::Value)
        );
        for separator in ["%", "+", "-", "1"] {
            assert_eq!(
                call_numbervalue(vec![text("1"), text(separator), text(",")]),
                Value::Error(ErrorValue::Value)
            );
            assert_eq!(
                call_numbervalue(vec![text("1"), text("."), text(separator)]),
                Value::Error(ErrorValue::Value)
            );
        }
    }

    #[test]
    fn rejects_duplicate_decimal_separator() {
        assert_eq!(
            call_numbervalue(vec![text("1.2.3")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_group_separator_after_decimal_separator() {
        assert_eq!(
            call_numbervalue(vec![text("1.2,3")]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn rejects_invalid_and_non_finite_numeric_text() {
        for value in ["not a number", "1/1/2008", "12:30", "%", "NaN", "1e309"] {
            assert_eq!(
                call_numbervalue(vec![text(value)]),
                Value::Error(ErrorValue::Value),
                "{value}"
            );
        }
    }

    #[test]
    fn propagates_text_coercion_errors_from_all_arguments() {
        assert_eq!(
            call_numbervalue(vec![Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_numbervalue(vec![text("1"), Value::Error(ErrorValue::Ref)]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_numbervalue(vec![text("1"), text("."), Value::Error(ErrorValue::Na)]),
            Value::Error(ErrorValue::Na)
        );
    }

    #[test]
    fn preserves_leading_signs() {
        assert_number_close(call_numbervalue(vec![text("-1,234.5%")]), -12.345);
        assert_number_close(call_numbervalue(vec![text("+1,234.5")]), 1234.5);
    }

    fn call_numbervalue(values: Vec<Value>) -> Value {
        let ctx = TestContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        NUMBERVALUE.call(&args, &fn_ctx)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn assert_number_close(actual: Value, expected: f64) {
        match actual {
            Value::Number(number) => {
                let scale = expected.abs().max(1.0);
                assert!(
                    (number - expected).abs() <= 1e-12 * scale,
                    "expected {expected}, got {number}"
                );
            }
            other => panic!("expected number {expected}, got {other:?}"),
        }
    }

    struct TestContext;

    impl EvalContext for TestContext {
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
}
