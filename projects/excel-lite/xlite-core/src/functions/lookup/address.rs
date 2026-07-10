use crate::functions::prelude::*;
use crate::model::{Coord, MAX_COLS, MAX_ROWS};

const MIN_ARGS: usize = 2;
const MAX_ARGS: usize = 5;

pub struct Address;

impl Function for Address {
    fn name(&self) -> &'static str {
        "ADDRESS"
    }

    fn arity(&self) -> (usize, Option<usize>) {
        (MIN_ARGS, Some(MAX_ARGS))
    }

    fn call(&self, args: &[Arg<'_>], ctx: &FnContext<'_>) -> Value {
        if !(MIN_ARGS..=MAX_ARGS).contains(&args.len()) {
            return Value::Error(ErrorValue::Value);
        }

        let row = match coerce_index(&args[0], ctx, MAX_ROWS) {
            Ok(row) => row,
            Err(error) => return Value::Error(error),
        };
        let col = match coerce_index(&args[1], ctx, MAX_COLS) {
            Ok(col) => col,
            Err(error) => return Value::Error(error),
        };
        let abs_num = match args.get(2) {
            Some(arg) => match coerce_abs_num(arg, ctx) {
                Ok(abs_num) => abs_num,
                Err(error) => return Value::Error(error),
            },
            None => 1,
        };
        let a1 = match args.get(3) {
            Some(arg) => match ctx.to_bool(&arg.as_value()) {
                Ok(a1) => a1,
                Err(error) => return Value::Error(error),
            },
            None => true,
        };
        let sheet_text = match args.get(4) {
            Some(arg) => match ctx.to_text(&arg.as_value()) {
                Ok(sheet_text) => Some(sheet_text),
                Err(error) => return Value::Error(error),
            },
            None => None,
        };

        let coord = Coord {
            row: row - 1,
            col: col - 1,
        };
        let address = if a1 {
            format_a1(coord, abs_num)
        } else {
            format_r1c1(row, col, abs_num)
        };

        match sheet_text {
            Some(sheet_text) => Value::Text(format!("{}!{}", format_sheet_name(&sheet_text), address)),
            None => Value::Text(address),
        }
    }
}

fn coerce_index(arg: &Arg<'_>, ctx: &FnContext<'_>, max: u32) -> Result<u32, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    let index = number.trunc();
    if !index.is_finite() || !(1.0..=f64::from(max)).contains(&index) {
        return Err(ErrorValue::Value);
    }

    Ok(index as u32)
}

fn coerce_abs_num(arg: &Arg<'_>, ctx: &FnContext<'_>) -> Result<u8, ErrorValue> {
    let number = ctx.to_number(&arg.as_value())?;
    let abs_num = number.trunc();
    if !abs_num.is_finite() || !(1.0..=4.0).contains(&abs_num) {
        return Err(ErrorValue::Value);
    }

    Ok(abs_num as u8)
}

fn format_a1(coord: Coord, abs_num: u8) -> String {
    let base = coord.to_a1();
    let digit_index = base
        .find(|ch: char| ch.is_ascii_digit())
        .unwrap_or(base.len());
    let (col, row) = base.split_at(digit_index);

    match abs_num {
        1 => format!("${}${}", col, row),
        2 => format!("{}${}", col, row),
        3 => format!("${}{}", col, row),
        4 => base,
        _ => unreachable!("abs_num is validated before formatting"),
    }
}

fn format_r1c1(row: u32, col: u32, abs_num: u8) -> String {
    match abs_num {
        1 => format!("R{}C{}", row, col),
        2 => format!("R{}C[{}]", row, col),
        3 => format!("R[{}]C{}", row, col),
        4 => format!("R[{}]C[{}]", row, col),
        _ => unreachable!("abs_num is validated before formatting"),
    }
}

fn format_sheet_name(sheet_text: &str) -> String {
    if is_simple_sheet_name(sheet_text) {
        sheet_text.to_string()
    } else {
        format!("'{}'", sheet_text.replace('\'', "''"))
    }
}

fn is_simple_sheet_name(sheet_text: &str) -> bool {
    let mut chars = sheet_text.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
}

static ADDRESS: Address = Address;
inventory::submit! { FunctionEntry(&ADDRESS) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        eval::EvalContext,
        syntax::{CellRef, RangeRef},
    };

    #[test]
    fn metadata_matches_contract() {
        assert_eq!(ADDRESS.name(), "ADDRESS");
        assert_eq!(ADDRESS.arity(), (2, Some(5)));
    }

    #[test]
    fn rejects_wrong_direct_arity() {
        assert_eq!(call_address(vec![num(1.0)]), Value::Error(ErrorValue::Value));
        assert_eq!(
            call_address(vec![
                num(1.0),
                num(1.0),
                num(1.0),
                Value::Boolean(true),
                text("Sheet1"),
                num(6.0),
            ]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn formats_a1_absolute_modes() {
        assert_eq!(call_address(vec![num(2.0), num(3.0)]), text("$C$2"));
        assert_eq!(
            call_address(vec![num(2.0), num(3.0), num(2.0)]),
            text("C$2")
        );
        assert_eq!(
            call_address(vec![num(2.0), num(3.0), num(3.0)]),
            text("$C2")
        );
        assert_eq!(
            call_address(vec![num(2.0), num(3.0), num(4.0)]),
            text("C2")
        );
    }

    #[test]
    fn formats_r1c1_absolute_modes() {
        assert_eq!(
            call_address(vec![
                num(2.0),
                num(3.0),
                num(1.0),
                Value::Boolean(false),
            ]),
            text("R2C3")
        );
        assert_eq!(
            call_address(vec![
                num(2.0),
                num(3.0),
                num(2.0),
                Value::Boolean(false),
            ]),
            text("R2C[3]")
        );
        assert_eq!(
            call_address(vec![
                num(2.0),
                num(3.0),
                num(3.0),
                Value::Boolean(false),
            ]),
            text("R[2]C3")
        );
        assert_eq!(
            call_address(vec![
                num(2.0),
                num(3.0),
                num(4.0),
                Value::Boolean(false),
            ]),
            text("R[2]C[3]")
        );
    }

    #[test]
    fn truncates_numeric_arguments_toward_zero() {
        assert_eq!(
            call_address(vec![num(2.9), num(3.9), num(4.8)]),
            text("C2")
        );
        assert_eq!(
            call_address(vec![num(MAX_ROWS as f64 + 0.9), num(MAX_COLS as f64), num(4.0)]),
            text("XFD1048576")
        );
    }

    #[test]
    fn validates_row_column_and_abs_num() {
        assert_eq!(
            call_address(vec![num(0.9), num(1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(1.0), num(0.9)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(MAX_ROWS as f64 + 1.0), num(1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(1.0), num(MAX_COLS as f64 + 1.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(0.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(5.0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(f64::INFINITY), num(1.0)]),
            Value::Error(ErrorValue::Value)
        );
    }

    #[test]
    fn coerces_optional_arguments() {
        assert_eq!(
            call_address(vec![
                Value::Text("2".to_string()),
                Value::Text("3".to_string()),
                Value::Text("2".to_string()),
                num(0.0),
            ]),
            text("R2C[3]")
        );
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(1.0), Value::Blank]),
            text("R1C1")
        );
    }

    #[test]
    fn prefixes_and_quotes_sheet_text() {
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(1.0), Value::Boolean(true), text("Sheet2")]),
            text("Sheet2!$A$1")
        );
        assert_eq!(
            call_address(vec![
                num(2.0),
                num(3.0),
                num(1.0),
                Value::Boolean(false),
                text("EXCEL SHEET"),
            ]),
            text("'EXCEL SHEET'!R2C3")
        );
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(1.0), Value::Boolean(true), text("O'Brien")]),
            text("'O''Brien'!$A$1")
        );
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(1.0), Value::Boolean(true), text("Book[1]")]),
            text("'Book[1]'!$A$1")
        );
    }

    #[test]
    fn propagates_coercion_errors_left_to_right() {
        assert_eq!(
            call_address(vec![Value::Error(ErrorValue::Ref), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Ref)
        );
        assert_eq!(
            call_address(vec![num(1.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_address(vec![Value::Text("x".to_string()), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            call_address(vec![num(1.0), num(1.0), num(1.0), Value::Error(ErrorValue::Div0)]),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            call_address(vec![
                num(1.0),
                num(1.0),
                num(1.0),
                Value::Boolean(true),
                Value::Error(ErrorValue::Ref),
            ]),
            Value::Error(ErrorValue::Ref)
        );
    }

    fn call_address(values: Vec<Value>) -> Value {
        let ctx = DummyContext;
        let fn_ctx = FnContext::new(&ctx);
        let args: Vec<_> = values.into_iter().map(Arg::Value).collect();
        ADDRESS.call(&args, &fn_ctx)
    }

    fn num(value: f64) -> Value {
        Value::Number(value)
    }

    fn text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    struct DummyContext;

    impl EvalContext for DummyContext {
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
            CellId::new(0, Coord { row: 0, col: 0 })
        }
    }
}
