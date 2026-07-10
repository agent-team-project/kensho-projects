use crate::{
    model::{col_to_letters, MAX_COLS, MAX_ROWS},
    syntax::{parse, CellRef},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Col,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefShift {
    pub sheet: u16,
    pub axis: Axis,
    /// Zero-based insertion/deletion index on the edited axis.
    pub at: u32,
    pub count: u32,
    pub kind: EditKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RewriteOutcome {
    Unchanged(String),
    Rewritten(String),
}

impl RewriteOutcome {
    pub fn raw(&self) -> &str {
        match self {
            Self::Unchanged(raw) | Self::Rewritten(raw) => raw,
        }
    }

    pub fn changed(&self) -> bool {
        matches!(self, Self::Rewritten(_))
    }
}

pub fn rewrite_formula(raw: &str, shift: RefShift) -> RewriteOutcome {
    if !raw.starts_with('=') || shift.count == 0 {
        return RewriteOutcome::Unchanged(raw.to_string());
    }

    let body = &raw[1..];
    if parse(body).is_err() {
        return RewriteOutcome::Unchanged(raw.to_string());
    }

    let tokens = scan_ref_tokens(body);
    if tokens.is_empty() {
        return RewriteOutcome::Unchanged(raw.to_string());
    }

    let replacements = plan_replacements(body, &tokens, shift);
    if replacements.is_empty() {
        return RewriteOutcome::Unchanged(raw.to_string());
    }

    let mut rewritten_body = String::with_capacity(body.len());
    let mut cursor = 0;
    for replacement in replacements {
        rewritten_body.push_str(&body[cursor..replacement.start]);
        rewritten_body.push_str(&replacement.text);
        cursor = replacement.end;
    }
    rewritten_body.push_str(&body[cursor..]);

    if parse(&rewritten_body).is_err() {
        return RewriteOutcome::Unchanged(raw.to_string());
    }

    RewriteOutcome::Rewritten(format!("={rewritten_body}"))
}

#[derive(Clone, Debug)]
struct RefToken {
    start: usize,
    end: usize,
    cell: CellRef,
    supported_sheet: bool,
}

#[derive(Clone, Debug)]
struct Replacement {
    start: usize,
    end: usize,
    text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefRewrite {
    Unchanged,
    Cell(CellRef),
    RefError,
}

fn scan_ref_tokens(body: &str) -> Vec<RefToken> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < body.len() {
        let ch = next_char(body, pos).expect("pos within string");
        if ch == '"' {
            pos = skip_string(body, pos);
            continue;
        }

        if is_ref_token_start(ch) && is_ref_boundary_before(body, pos) {
            let end = scan_token_end(body, pos);
            let text = &body[pos..end];
            if !is_function_call_shaped(body, end, text) {
                if let Some(token) = parse_ref_token(text, pos) {
                    tokens.push(token);
                }
            }
            pos = end;
        } else {
            pos += ch.len_utf8();
        }
    }
    tokens
}

fn plan_replacements(body: &str, tokens: &[RefToken], shift: RefShift) -> Vec<Replacement> {
    let mut replacements = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        if let Some(range_end_index) = range_end_token_index(body, tokens, index) {
            if let Some(replacement) =
                rewrite_range_token(body, &tokens[index], &tokens[range_end_index], shift)
            {
                replacements.push(replacement);
            }
            index = range_end_index + 1;
            continue;
        }

        let token = &tokens[index];
        if resolves_to_edited_sheet(token, shift) {
            match rewrite_cell(token.cell, shift) {
                RefRewrite::Unchanged => {}
                RefRewrite::Cell(cell) => replacements.push(Replacement {
                    start: token.start,
                    end: token.end,
                    text: format_cell_like(&body[token.start..token.end], cell),
                }),
                RefRewrite::RefError => replacements.push(Replacement {
                    start: token.start,
                    end: token.end,
                    text: "#REF!".to_string(),
                }),
            }
        }
        index += 1;
    }

    replacements
}

fn range_end_token_index(body: &str, tokens: &[RefToken], index: usize) -> Option<usize> {
    let colon = skip_ascii_whitespace(body, tokens[index].end);
    if !body[colon..].starts_with(':') {
        return None;
    }

    let after_colon = skip_ascii_whitespace(body, colon + 1);
    let next_index = index + 1;
    if tokens
        .get(next_index)
        .is_some_and(|token| token.start == after_colon)
    {
        Some(next_index)
    } else {
        None
    }
}

fn rewrite_range_token(
    body: &str,
    start_token: &RefToken,
    end_token: &RefToken,
    shift: RefShift,
) -> Option<Replacement> {
    if !resolves_to_edited_sheet(start_token, shift) || !resolves_to_edited_sheet(end_token, shift)
    {
        return None;
    }

    match rewrite_range(start_token.cell, end_token.cell, shift) {
        RangeRewrite::Unchanged => None,
        RangeRewrite::Range { start, end } => {
            let start_text = format_cell_like(&body[start_token.start..start_token.end], start);
            let end_text = format_cell_like(&body[end_token.start..end_token.end], end);
            Some(Replacement {
                start: start_token.start,
                end: end_token.end,
                text: format!(
                    "{start_text}{}{end_text}",
                    &body[start_token.end..end_token.start]
                ),
            })
        }
        RangeRewrite::RefError => Some(Replacement {
            start: start_token.start,
            end: end_token.end,
            text: "#REF!".to_string(),
        }),
    }
}

fn resolves_to_edited_sheet(token: &RefToken, shift: RefShift) -> bool {
    token.supported_sheet && token.cell.sheet.unwrap_or(shift.sheet) == shift.sheet
}

fn rewrite_cell(mut cell: CellRef, shift: RefShift) -> RefRewrite {
    let coord = axis_value(cell, shift.axis);
    let Some(new_coord) = rewrite_single_axis(coord, shift) else {
        return RefRewrite::RefError;
    };
    if new_coord == coord {
        return RefRewrite::Unchanged;
    }
    set_axis_value(&mut cell, shift.axis, new_coord);
    RefRewrite::Cell(cell)
}

fn rewrite_single_axis(coord: u32, shift: RefShift) -> Option<u32> {
    match shift.kind {
        EditKind::Insert => {
            if coord >= shift.at {
                checked_axis_add(coord, shift.count, shift.axis)
            } else {
                Some(coord)
            }
        }
        EditKind::Delete => {
            let delete_end = shift
                .at
                .checked_add(shift.count.saturating_sub(1))
                .unwrap_or(u32::MAX);
            if coord < shift.at {
                Some(coord)
            } else if coord <= delete_end {
                None
            } else {
                coord.checked_sub(shift.count)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangeRewrite {
    Unchanged,
    Range { start: CellRef, end: CellRef },
    RefError,
}

fn rewrite_range(mut start: CellRef, mut end: CellRef, shift: RefShift) -> RangeRewrite {
    let start_axis = axis_value(start, shift.axis);
    let end_axis = axis_value(end, shift.axis);
    let lo = start_axis.min(end_axis);
    let hi = start_axis.max(end_axis);

    let Some((new_lo, new_hi)) = rewrite_range_axis(lo, hi, shift) else {
        return RangeRewrite::RefError;
    };
    if (new_lo, new_hi) == (lo, hi) {
        return RangeRewrite::Unchanged;
    }

    if start_axis <= end_axis {
        set_axis_value(&mut start, shift.axis, new_lo);
        set_axis_value(&mut end, shift.axis, new_hi);
    } else {
        set_axis_value(&mut start, shift.axis, new_hi);
        set_axis_value(&mut end, shift.axis, new_lo);
    }

    RangeRewrite::Range { start, end }
}

fn rewrite_range_axis(lo: u32, hi: u32, shift: RefShift) -> Option<(u32, u32)> {
    match shift.kind {
        EditKind::Insert => {
            if shift.at <= lo {
                Some((
                    checked_axis_add(lo, shift.count, shift.axis)?,
                    checked_axis_add(hi, shift.count, shift.axis)?,
                ))
            } else if lo < shift.at && shift.at <= hi {
                Some((lo, checked_axis_add(hi, shift.count, shift.axis)?))
            } else {
                Some((lo, hi))
            }
        }
        EditKind::Delete => {
            let delete_end = shift
                .at
                .checked_add(shift.count.saturating_sub(1))
                .unwrap_or(u32::MAX);

            if delete_end < lo {
                Some((lo.checked_sub(shift.count)?, hi.checked_sub(shift.count)?))
            } else if shift.at > hi {
                Some((lo, hi))
            } else {
                let overlap_start = lo.max(shift.at);
                let overlap_end = hi.min(delete_end);
                let overlap_len = overlap_end - overlap_start + 1;
                let range_len = hi - lo + 1;
                if overlap_len >= range_len {
                    return None;
                }

                let remaining_len = range_len - overlap_len;
                let new_lo = if lo < shift.at { lo } else { shift.at };
                Some((new_lo, new_lo + remaining_len - 1))
            }
        }
    }
}

fn checked_axis_add(value: u32, amount: u32, axis: Axis) -> Option<u32> {
    let shifted = value.checked_add(amount)?;
    let max = match axis {
        Axis::Row => MAX_ROWS - 1,
        Axis::Col => MAX_COLS - 1,
    };
    (shifted <= max).then_some(shifted)
}

fn parse_ref_token(text: &str, start: usize) -> Option<RefToken> {
    let expr = parse(text).ok()?;
    let crate::syntax::Expr::Ref(cell) = expr else {
        return None;
    };

    Some(RefToken {
        start,
        end: start + text.len(),
        cell,
        supported_sheet: supported_sheet_prefix(text),
    })
}

fn supported_sheet_prefix(text: &str) -> bool {
    text.rsplit_once('!')
        .map(|(sheet, _)| sheet.eq_ignore_ascii_case("Sheet1"))
        .unwrap_or(true)
}

fn format_cell_like(original: &str, cell: CellRef) -> String {
    let prefix = original
        .rsplit_once('!')
        .map(|(sheet, _)| format!("{sheet}!"))
        .unwrap_or_default();
    format!(
        "{prefix}{}{}{}{}",
        if cell.col_abs { "$" } else { "" },
        col_to_letters(cell.col),
        if cell.row_abs { "$" } else { "" },
        cell.row + 1
    )
}

fn axis_value(cell: CellRef, axis: Axis) -> u32 {
    match axis {
        Axis::Row => cell.row,
        Axis::Col => cell.col,
    }
}

fn set_axis_value(cell: &mut CellRef, axis: Axis, value: u32) {
    match axis {
        Axis::Row => cell.row = value,
        Axis::Col => cell.col = value,
    }
}

fn skip_string(input: &str, start: usize) -> usize {
    let mut pos = start + 1;
    while pos < input.len() {
        let ch = next_char(input, pos).expect("pos within string");
        pos += ch.len_utf8();
        if ch == '"' {
            if input[pos..].starts_with('"') {
                pos += 1;
            } else {
                break;
            }
        }
    }
    pos
}

fn scan_token_end(input: &str, start: usize) -> usize {
    let mut pos = start;
    while pos < input.len() {
        let ch = next_char(input, pos).expect("pos within string");
        if !is_ref_token_char(ch) {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn skip_ascii_whitespace(input: &str, start: usize) -> usize {
    let mut pos = start;
    while pos < input.len() {
        let ch = next_char(input, pos).expect("pos within string");
        if !ch.is_ascii_whitespace() {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn is_function_call_shaped(input: &str, token_end: usize, token_text: &str) -> bool {
    !token_text.contains('!')
        && token_text
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
        && skip_ascii_whitespace(input, token_end) < input.len()
        && input[skip_ascii_whitespace(input, token_end)..].starts_with('(')
}

fn is_ref_boundary_before(input: &str, start: usize) -> bool {
    input[..start]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_ref_token_char(ch))
}

fn is_ref_token_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '$'
}

fn is_ref_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '$' | '!' | '_' | '.')
}

fn next_char(input: &str, pos: usize) -> Option<char> {
    input.get(pos..)?.chars().next()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn insert_col(at: u32) -> RefShift {
        RefShift {
            sheet: 0,
            axis: Axis::Col,
            at,
            count: 1,
            kind: EditKind::Insert,
        }
    }

    fn delete_col(at: u32, count: u32) -> RefShift {
        RefShift {
            sheet: 0,
            axis: Axis::Col,
            at,
            count,
            kind: EditKind::Delete,
        }
    }

    fn insert_row(at: u32) -> RefShift {
        RefShift {
            sheet: 0,
            axis: Axis::Row,
            at,
            count: 1,
            kind: EditKind::Insert,
        }
    }

    fn delete_row(at: u32, count: u32) -> RefShift {
        RefShift {
            sheet: 0,
            axis: Axis::Row,
            at,
            count,
            kind: EditKind::Delete,
        }
    }

    fn raw_after(raw: &str, shift: RefShift) -> String {
        rewrite_formula(raw, shift).raw().to_string()
    }

    #[test]
    fn single_refs_shift_or_become_ref_errors() {
        assert_eq!(raw_after("=A1", insert_col(0)), "=B1");
        assert_eq!(raw_after("=A1", delete_col(0, 1)), "=#REF!");
    }

    #[test]
    fn ranges_grow_only_for_insertions_inside_existing_bounds() {
        assert_eq!(raw_after("=SUM(A1:A2)", insert_row(1)), "=SUM(A1:A3)");
        assert_eq!(raw_after("=SUM(A1:A2)", insert_row(2)), "=SUM(A1:A2)");
    }

    #[test]
    fn absolute_markers_are_preserved_while_coordinates_shift() {
        assert_eq!(raw_after("=$A$1", insert_col(0)), "=$B$1");
        assert_eq!(raw_after("=A$1", insert_col(0)), "=B$1");
        assert_eq!(raw_after("=$A1", insert_col(0)), "=$B1");
    }

    #[test]
    fn ranges_shrink_or_become_ref_errors_on_delete() {
        assert_eq!(raw_after("=SUM(A1:A5)", delete_row(2, 1)), "=SUM(A1:A4)");
        assert_eq!(raw_after("=SUM(A1:A2)", delete_row(0, 2)), "=SUM(#REF!)");
    }

    #[test]
    fn unsupported_sheet_refs_stay_unchanged_while_local_refs_shift() {
        assert_eq!(raw_after("=Sheet2!A1+A1", insert_col(0)), "=Sheet2!A1+B1");
        assert_eq!(
            raw_after("=Sheet2!A1:Sheet2!A2+A1", insert_col(0)),
            "=Sheet2!A1:Sheet2!A2+B1"
        );
    }

    #[test]
    fn sheet1_prefix_and_spacing_are_preserved_for_supported_ranges() {
        assert_eq!(
            raw_after("=SUM(Sheet1!A1 : Sheet1!A2)", insert_row(1)),
            "=SUM(Sheet1!A1 : Sheet1!A3)"
        );
    }

    #[test]
    fn string_literals_and_parse_error_formulas_are_unchanged_except_direct_refs() {
        assert_eq!(
            raw_after(r#"=INDIRECT("A1")+A1"#, insert_col(0)),
            r#"=INDIRECT("A1")+B1"#
        );
        assert_eq!(raw_after("=SUM(A1:", insert_col(0)), "=SUM(A1:");
        assert_eq!(
            raw_after("=[Book1]Sheet1!A1+A1", insert_col(0)),
            "=[Book1]Sheet1!A1+A1"
        );
    }

    #[test]
    fn non_formulas_and_unchanged_formulas_report_unchanged() {
        assert_eq!(
            rewrite_formula("A1", insert_col(0)),
            RewriteOutcome::Unchanged("A1".to_string())
        );
        assert_eq!(
            rewrite_formula("=C1", insert_col(3)),
            RewriteOutcome::Unchanged("=C1".to_string())
        );
    }

    proptest! {
        #[test]
        fn rewrite_formula_does_not_panic(input in ".{0,128}") {
            let _ = rewrite_formula(&input, insert_col(0));
        }
    }
}
