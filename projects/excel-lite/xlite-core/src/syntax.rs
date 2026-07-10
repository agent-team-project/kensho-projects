use crate::model::{parse_coord, Coord, ErrorValue, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Literal(crate::model::Value),
    Ref(CellRef),
    Range(RangeRef),
    Unary {
        op: UnaryOp,
        rhs: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Pos,
    Percent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellRef {
    pub sheet: Option<u16>,
    pub col: u32,
    pub row: u32,
    pub col_abs: bool,
    pub row_abs: bool,
}

impl CellRef {
    pub fn coord(self) -> Coord {
        Coord {
            row: self.row,
            col: self.col,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RangeRef {
    pub start: CellRef,
    pub end: CellRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub pos: usize,
    pub message: String,
}

impl ParseError {
    pub fn new(kind: ParseErrorKind, pos: usize, message: impl Into<String>) -> Self {
        Self {
            kind,
            pos,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseErrorKind {
    UnexpectedToken,
    UnexpectedEnd,
    InvalidReference,
    InvalidNumber,
    UnterminatedString,
}

pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let body = input.strip_prefix('=').unwrap_or(input);
    let tokens = Lexer::new(body).lex()?;
    let mut parser = Parser { tokens, current: 0 };
    let expr = parser.parse_expr()?;
    parser.expect(TokenKind::Eof)?;
    Ok(expr)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalReference {
    Cell(CellRef),
    Range(RangeRef),
}

pub fn parse_local_reference_text(input: &str) -> Result<LocalReference, ParseError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(invalid_reference(0, "empty reference text"));
    }
    if text.contains('[') || text.contains(']') {
        return Err(invalid_reference(
            0,
            "external workbook references are unsupported",
        ));
    }

    let Some((start_text, end_text)) = text.split_once(':') else {
        return parse_local_cell_reference(text, 0).map(LocalReference::Cell);
    };
    if end_text.contains(':') {
        return Err(invalid_reference(
            start_text.len() + 1,
            "reference text contains too many range separators",
        ));
    }

    let start = parse_local_cell_reference(start_text, 0)?;
    let end = parse_local_cell_reference(end_text, start_text.len() + 1)?;
    Ok(LocalReference::Range(RangeRef { start, end }))
}

fn parse_local_cell_reference(text: &str, pos: usize) -> Result<CellRef, ParseError> {
    let text = text.trim();
    let (sheet, addr) = if let Some((sheet_text, addr)) = text.rsplit_once('!') {
        if !sheet_text.eq_ignore_ascii_case("Sheet1") {
            return Err(invalid_reference(
                pos,
                "only Sheet1 references are supported",
            ));
        }
        (Some(0), addr)
    } else {
        (None, text)
    };

    let Some((coord, col_abs, row_abs)) = parse_ref_addr(addr) else {
        return Err(invalid_reference(pos, "invalid A1 reference"));
    };
    Ok(CellRef {
        sheet,
        col: coord.col,
        row: coord.row,
        col_abs,
        row_abs,
    })
}

fn invalid_reference(pos: usize, message: impl Into<String>) -> ParseError {
    ParseError::new(ParseErrorKind::InvalidReference, pos, message)
}

#[derive(Clone, Debug, PartialEq)]
struct Token {
    kind: TokenKind,
    pos: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Number(f64),
    Str(String),
    Bool(bool),
    Error(ErrorValue),
    CellRef(CellRef),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Amp,
    Percent,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    LParen,
    RParen,
    Comma,
    Colon,
    Eof,
}

const ERROR_LITERAL_CODES: &[&str] = &[
    "#DIV/0!", "#VALUE!", "#CIRC!", "#NULL!", "#NAME?", "#REF!", "#NUM!", "#N/A",
];

struct Lexer<'a> {
    input: &'a str,
    chars: Vec<char>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            let pos = self.pos;
            match ch {
                ch if ch.is_ascii_whitespace() => {
                    self.bump();
                }
                '0'..='9' | '.' if self.starts_number() => {
                    tokens.push(Token {
                        kind: self.lex_number()?,
                        pos,
                    });
                }
                '"' => tokens.push(Token {
                    kind: self.lex_string()?,
                    pos,
                }),
                '#' => tokens.push(Token {
                    kind: self.lex_error_literal()?,
                    pos,
                }),
                '+' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Plus,
                        pos,
                    });
                }
                '-' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Minus,
                        pos,
                    });
                }
                '*' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Star,
                        pos,
                    });
                }
                '/' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Slash,
                        pos,
                    });
                }
                '^' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Caret,
                        pos,
                    });
                }
                '&' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Amp,
                        pos,
                    });
                }
                '%' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Percent,
                        pos,
                    });
                }
                '=' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Eq,
                        pos,
                    });
                }
                '<' => {
                    self.bump();
                    let kind = match self.peek() {
                        Some('=') => {
                            self.bump();
                            TokenKind::Le
                        }
                        Some('>') => {
                            self.bump();
                            TokenKind::Ne
                        }
                        _ => TokenKind::Lt,
                    };
                    tokens.push(Token { kind, pos });
                }
                '>' => {
                    self.bump();
                    let kind = match self.peek() {
                        Some('=') => {
                            self.bump();
                            TokenKind::Ge
                        }
                        _ => TokenKind::Gt,
                    };
                    tokens.push(Token { kind, pos });
                }
                '(' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::LParen,
                        pos,
                    });
                }
                ')' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::RParen,
                        pos,
                    });
                }
                ',' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Comma,
                        pos,
                    });
                }
                ':' => {
                    self.bump();
                    tokens.push(Token {
                        kind: TokenKind::Colon,
                        pos,
                    });
                }
                ch if ch.is_ascii_alphabetic() || ch == '$' => tokens.push(Token {
                    kind: self.lex_word()?,
                    pos,
                }),
                _ => {
                    return Err(ParseError::new(
                        ParseErrorKind::UnexpectedToken,
                        pos,
                        format!("unexpected character {ch:?}"),
                    ));
                }
            }
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            pos: self.input.len(),
        });
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        Some(ch)
    }

    fn starts_number(&self) -> bool {
        match self.peek() {
            Some('0'..='9') => true,
            Some('.') => matches!(self.peek_next(), Some('0'..='9')),
            _ => false,
        }
    }

    fn lex_number(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.pos;
        if self.peek() == Some('.') {
            self.bump();
        }
        while matches!(self.peek(), Some('0'..='9')) {
            self.bump();
        }
        if self.peek() == Some('.') {
            self.bump();
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }

        let text: String = self.chars[start..self.pos].iter().collect();
        let number = text.parse::<f64>().map_err(|_| {
            ParseError::new(
                ParseErrorKind::InvalidNumber,
                start,
                format!("invalid number {text:?}"),
            )
        })?;
        Ok(TokenKind::Number(number))
    }

    fn lex_string(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.pos;
        self.bump();
        let mut out = String::new();
        while let Some(ch) = self.bump() {
            match ch {
                '"' if self.peek() == Some('"') => {
                    self.bump();
                    out.push('"');
                }
                '"' => return Ok(TokenKind::Str(out)),
                _ => out.push(ch),
            }
        }
        Err(ParseError::new(
            ParseErrorKind::UnterminatedString,
            start,
            "unterminated string",
        ))
    }

    fn lex_error_literal(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.pos;
        for code in ERROR_LITERAL_CODES {
            if self.starts_with_chars(code) {
                for _ in code.chars() {
                    self.bump();
                }
                let error = ErrorValue::from_code(code).expect("canonical error code");
                return Ok(TokenKind::Error(error));
            }
        }

        Err(ParseError::new(
            ParseErrorKind::UnexpectedToken,
            start,
            "unknown error literal",
        ))
    }

    fn starts_with_chars(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(offset, ch)| self.chars.get(self.pos + offset) == Some(&ch))
    }

    fn lex_word(&mut self) -> Result<TokenKind, ParseError> {
        let start = self.pos;
        while matches!(self.peek(), Some(ch) if ch.is_ascii_alphanumeric() || matches!(ch, '$' | '!' | '_' | '.'))
        {
            self.bump();
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let upper = text.to_ascii_uppercase();
        if is_function_identifier_token(&text) && self.next_non_whitespace_is_lparen() {
            return Ok(TokenKind::Ident(upper));
        }

        if let Some(cell_ref) = parse_cell_ref_token(&text) {
            return Ok(TokenKind::CellRef(cell_ref));
        }

        match upper.as_str() {
            "TRUE" => Ok(TokenKind::Bool(true)),
            "FALSE" => Ok(TokenKind::Bool(false)),
            _ => Ok(TokenKind::Ident(upper)),
        }
    }

    fn next_non_whitespace_is_lparen(&self) -> bool {
        let mut pos = self.pos;
        while matches!(self.chars.get(pos), Some(ch) if ch.is_ascii_whitespace()) {
            pos += 1;
        }
        self.chars.get(pos) == Some(&'(')
    }
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_concat()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Eq => BinaryOp::Eq,
                TokenKind::Ne => BinaryOp::Ne,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Ge => BinaryOp::Ge,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_concat()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_concat(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_addsub()?;
        while matches!(self.peek_kind(), TokenKind::Amp) {
            self.advance();
            let rhs = self.parse_addsub()?;
            expr = Expr::Binary {
                op: BinaryOp::Concat,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_addsub(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_muldiv()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_muldiv()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_muldiv(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_power()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_power()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_power(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        while matches!(self.peek_kind(), TokenKind::Caret) {
            self.advance();
            let rhs = self.parse_unary()?;
            expr = Expr::Binary {
                op: BinaryOp::Pow,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek_kind() {
            TokenKind::Plus => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Pos,
                    rhs: Box::new(self.parse_unary()?),
                })
            }
            TokenKind::Minus => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    rhs: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        while matches!(self.peek_kind(), TokenKind::Percent) {
            self.advance();
            expr = Expr::Unary {
                op: UnaryOp::Percent,
                rhs: Box::new(expr),
            };
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.advance().clone();
        match token.kind {
            TokenKind::Number(number) => Ok(Expr::Literal(crate::model::Value::Number(number))),
            TokenKind::Str(text) => Ok(Expr::Literal(crate::model::Value::Text(text))),
            TokenKind::Bool(value) => Ok(Expr::Literal(crate::model::Value::Boolean(value))),
            TokenKind::Error(error) => Ok(Expr::Literal(Value::Error(error))),
            TokenKind::CellRef(start) => {
                if matches!(self.peek_kind(), TokenKind::Colon) {
                    self.advance();
                    let end = match self.advance().kind {
                        TokenKind::CellRef(cell_ref) => cell_ref,
                        _ => {
                            return Err(ParseError::new(
                                ParseErrorKind::InvalidReference,
                                self.previous_pos(),
                                "expected cell reference after ':'",
                            ));
                        }
                    };
                    Ok(Expr::Range(RangeRef { start, end }))
                } else {
                    Ok(Expr::Ref(start))
                }
            }
            TokenKind::Ident(name) => {
                self.expect(TokenKind::LParen)?;
                let mut args = Vec::new();
                if matches!(self.peek_kind(), TokenKind::RParen) {
                    self.advance();
                    return Ok(Expr::Call { name, args });
                }

                loop {
                    if matches!(self.peek_kind(), TokenKind::Comma | TokenKind::RParen) {
                        args.push(Expr::Literal(crate::model::Value::Blank));
                    } else {
                        args.push(self.parse_expr()?);
                    }

                    match self.peek_kind() {
                        TokenKind::Comma => {
                            self.advance();
                        }
                        TokenKind::RParen => {
                            self.advance();
                            break;
                        }
                        _ => {
                            return Err(ParseError::new(
                                ParseErrorKind::UnexpectedToken,
                                self.peek().pos,
                                "expected ',' or ')'",
                            ));
                        }
                    }
                }
                Ok(Expr::Call { name, args })
            }
            TokenKind::LParen => {
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            TokenKind::Eof => Err(ParseError::new(
                ParseErrorKind::UnexpectedEnd,
                token.pos,
                "unexpected end of formula",
            )),
            _ => Err(ParseError::new(
                ParseErrorKind::UnexpectedToken,
                token.pos,
                "expected expression",
            )),
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<(), ParseError> {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&kind) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::new(
                ParseErrorKind::UnexpectedToken,
                self.peek().pos,
                format!("expected {kind:?}"),
            ))
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn previous_pos(&self) -> usize {
        self.tokens
            .get(self.current.saturating_sub(1))
            .map(|token| token.pos)
            .unwrap_or_default()
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.current];
        if !matches!(token.kind, TokenKind::Eof) {
            self.current += 1;
        }
        token
    }
}

fn parse_cell_ref_token(text: &str) -> Option<CellRef> {
    let (sheet, addr) = if let Some((sheet_text, addr)) = text.rsplit_once('!') {
        let sheet = if sheet_text.eq_ignore_ascii_case("Sheet1") {
            Some(0)
        } else {
            Some(1)
        };
        (sheet, addr)
    } else {
        (None, text)
    };

    let (coord, col_abs, row_abs) = parse_ref_addr(addr)?;
    Some(CellRef {
        sheet,
        col: coord.col,
        row: coord.row,
        col_abs,
        row_abs,
    })
}

fn is_function_identifier_token(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
}

fn parse_ref_addr(addr: &str) -> Option<(Coord, bool, bool)> {
    let mut chars = addr.chars().peekable();
    let col_abs = if chars.peek() == Some(&'$') {
        chars.next();
        true
    } else {
        false
    };

    let mut col_text = String::new();
    while let Some(ch) = chars.peek().copied() {
        if !ch.is_ascii_alphabetic() {
            break;
        }
        col_text.push(ch);
        chars.next();
    }

    let row_abs = if chars.peek() == Some(&'$') {
        chars.next();
        true
    } else {
        false
    };

    let mut row_text = String::new();
    while let Some(ch) = chars.peek().copied() {
        if !ch.is_ascii_digit() {
            break;
        }
        row_text.push(ch);
        chars.next();
    }

    if col_text.is_empty() || row_text.is_empty() || chars.next().is_some() {
        return None;
    }

    let coord = parse_coord(&format!("{col_text}{row_text}"))?;
    Some((coord, col_abs, row_abs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_call_with_range() {
        let expr = parse("SUM(A1:A3)").unwrap();
        match expr {
            Expr::Call { name, args } => {
                assert_eq!(name, "SUM");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Expr::Range(_)));
            }
            other => panic!("unexpected expr: {other:?}"),
        }
    }

    #[test]
    fn parses_comparison() {
        let expr = parse("A1>0").unwrap();
        assert!(matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::Gt,
                ..
            }
        ));
    }

    #[test]
    fn parses_slice1_precedence_operators() {
        let expr = parse("-2^2&50%").unwrap();
        assert!(matches!(
            expr,
            Expr::Binary {
                op: BinaryOp::Concat,
                ..
            }
        ));

        let expr = parse("2^3^2").unwrap();
        match expr {
            Expr::Binary {
                op: BinaryOp::Pow,
                lhs,
                ..
            } => assert!(matches!(
                *lhs,
                Expr::Binary {
                    op: BinaryOp::Pow,
                    ..
                }
            )),
            other => panic!("unexpected expr: {other:?}"),
        }
    }

    #[test]
    fn parses_cell_ref_shaped_identifier_as_call_when_followed_by_lparen() {
        let expr = parse("LOG10(10)").unwrap();
        match expr {
            Expr::Call { name, args } => {
                assert_eq!(name, "LOG10");
                assert_eq!(args.len(), 1);
                assert_eq!(args[0], Expr::Literal(crate::model::Value::Number(10.0)));
            }
            other => panic!("unexpected expr: {other:?}"),
        }
    }

    #[test]
    fn parses_cell_ref_shaped_identifier_as_call_with_space_before_lparen() {
        let expr = parse("LOG10 (10)").unwrap();
        match expr {
            Expr::Call { name, args } => {
                assert_eq!(name, "LOG10");
                assert_eq!(args.len(), 1);
            }
            other => panic!("unexpected expr: {other:?}"),
        }
    }

    #[test]
    fn keeps_cell_refs_and_ranges_when_not_followed_by_lparen() {
        assert!(matches!(parse("A1").unwrap(), Expr::Ref(_)));
        assert!(matches!(parse("LOG10").unwrap(), Expr::Ref(_)));
        assert!(matches!(parse("$A$1").unwrap(), Expr::Ref(_)));
        assert!(matches!(parse("A1:B2").unwrap(), Expr::Range(_)));
    }

    #[test]
    fn parses_dotted_function_names() {
        let expr = parse("ERROR.TYPE(A1)").unwrap();
        match expr {
            Expr::Call { name, .. } => assert_eq!(name, "ERROR.TYPE"),
            other => panic!("unexpected expr: {other:?}"),
        }
    }

    #[test]
    fn parses_escaped_string() {
        let expr = parse(r#""a ""quote""""#).unwrap();
        assert_eq!(
            expr,
            Expr::Literal(crate::model::Value::Text("a \"quote\"".to_string()))
        );
    }

    #[test]
    fn parses_canonical_error_literals() {
        for (formula, expected) in [
            ("#NULL!", ErrorValue::Null),
            ("#DIV/0!", ErrorValue::Div0),
            ("#VALUE!", ErrorValue::Value),
            ("#REF!", ErrorValue::Ref),
            ("#NAME?", ErrorValue::Name),
            ("#NUM!", ErrorValue::Num),
            ("#N/A", ErrorValue::Na),
            ("#CIRC!", ErrorValue::Circular),
        ] {
            assert_eq!(parse(formula), Ok(Expr::Literal(Value::Error(expected))));
        }
    }

    #[test]
    fn parses_local_reference_text_cells_and_ranges() {
        let cell = parse_local_reference_text("$A$1").unwrap();
        assert_eq!(
            cell,
            LocalReference::Cell(CellRef {
                sheet: None,
                col: 0,
                row: 0,
                col_abs: true,
                row_abs: true,
            })
        );

        let range = parse_local_reference_text("Sheet1!B2:$C$3").unwrap();
        assert_eq!(
            range,
            LocalReference::Range(RangeRef {
                start: CellRef {
                    sheet: Some(0),
                    col: 1,
                    row: 1,
                    col_abs: false,
                    row_abs: false,
                },
                end: CellRef {
                    sheet: None,
                    col: 2,
                    row: 2,
                    col_abs: true,
                    row_abs: true,
                },
            })
        );
    }

    #[test]
    fn rejects_unsupported_reference_text_deterministically() {
        for text in [
            "",
            "Sheet2!A1",
            "[Book1]Sheet1!A1",
            "R1C1",
            "A1:B2:C3",
            "A0",
            "XFE1",
        ] {
            let error = parse_local_reference_text(text).unwrap_err();
            assert_eq!(error.kind, ParseErrorKind::InvalidReference, "{text}");
        }
    }
}
