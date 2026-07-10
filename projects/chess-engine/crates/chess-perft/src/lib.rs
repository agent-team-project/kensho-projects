#![doc = "Perft runner and fixture parser for chess-core positions."]

use chess_core::{
    FenError, Move, MoveKind, MoveList, Position, in_check, legal_moves,
    make_legal_move_unchecked_fast, parse_fen, unmake_move,
};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PerftResult {
    pub nodes: u64,
    pub captures: u64,
    pub en_passant: u64,
    pub castles: u64,
    pub promotions: u64,
    pub checks: u64,
    pub checkmates: u64,
}

impl PerftResult {
    fn add_assign(&mut self, other: Self) {
        self.nodes += other.nodes;
        self.captures += other.captures;
        self.en_passant += other.en_passant;
        self.castles += other.castles;
        self.promotions += other.promotions;
        self.checks += other.checks;
        self.checkmates += other.checkmates;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerftCase {
    pub id: String,
    pub phase: PerftPhase,
    pub depth: u8,
    pub nodes: u64,
    pub fen: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerftPhase {
    Mandatory,
    Extended,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuiteCaseReport {
    pub id: String,
    pub phase: PerftPhase,
    pub depth: u8,
    pub expected: u64,
    pub actual: u64,
    pub fen: String,
}

impl SuiteCaseReport {
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.expected == self.actual
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SuiteReport {
    pub cases: Vec<SuiteCaseReport>,
}

impl SuiteReport {
    #[must_use]
    pub fn passed(&self) -> usize {
        self.cases.iter().filter(|case| case.passed()).count()
    }

    #[must_use]
    pub fn failed(&self) -> usize {
        self.cases.len() - self.passed()
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        self.failed() == 0
    }
}

#[derive(Debug)]
pub enum PerftError {
    Io(io::Error),
    Parse { line: usize, message: String },
    Fen { line: usize, source: FenError },
    SuiteMismatch(SuiteReport),
}

impl fmt::Display for PerftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read perft suite: {err}"),
            Self::Parse { line, message } => {
                write!(f, "invalid perft suite line {line}: {message}")
            }
            Self::Fen { line, source } => write!(f, "invalid FEN on line {line}: {source}"),
            Self::SuiteMismatch(report) => write!(
                f,
                "perft suite mismatch: {} passed, {} failed",
                report.passed(),
                report.failed()
            ),
        }
    }
}

impl Error for PerftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Fen { source, .. } => Some(source),
            Self::Parse { .. } | Self::SuiteMismatch(_) => None,
        }
    }
}

impl From<io::Error> for PerftError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[must_use]
pub fn perft(pos: &mut Position, depth: u8) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut moves = MoveList::new();
    legal_moves(pos, &mut moves);
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0u64;
    for &mv in &moves {
        let undo = make_legal_move_unchecked_fast(pos, mv);
        nodes += perft(pos, depth - 1);
        unmake_move(pos, mv, undo);
    }
    nodes
}

#[must_use]
pub fn perft_stats(pos: &mut Position, depth: u8) -> PerftResult {
    if depth == 0 {
        return PerftResult {
            nodes: 1,
            ..PerftResult::default()
        };
    }

    let mut moves = MoveList::new();
    legal_moves(pos, &mut moves);
    let mut result = PerftResult::default();

    for &mv in &moves {
        let undo = make_legal_move_unchecked_fast(pos, mv);
        if depth == 1 {
            result.nodes += 1;
            accumulate_leaf_stats(pos, mv, &mut result);
        } else {
            result.add_assign(perft_stats(pos, depth - 1));
        }
        unmake_move(pos, mv, undo);
    }

    result
}

#[must_use]
pub fn perft_divide(pos: &mut Position, depth: u8) -> Vec<(Move, u64)> {
    let mut moves = MoveList::new();
    legal_moves(pos, &mut moves);
    let mut divide = Vec::with_capacity(moves.len());
    for &mv in &moves {
        let undo = make_legal_move_unchecked_fast(pos, mv);
        let nodes = if depth == 0 { 0 } else { perft(pos, depth - 1) };
        unmake_move(pos, mv, undo);
        divide.push((mv, nodes));
    }
    divide
}

pub fn parse_perft_suite(path: &Path) -> Result<Vec<PerftCase>, PerftError> {
    let content = fs::read_to_string(path)?;
    let mut cases = Vec::new();
    for (line_index, raw_line) in content.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 5 {
            return Err(PerftError::Parse {
                line: line_number,
                message: format!("expected 5 pipe-delimited fields, found {}", parts.len()),
            });
        }
        let phase = match parts[1] {
            "mandatory" => PerftPhase::Mandatory,
            "extended" => PerftPhase::Extended,
            value => {
                return Err(PerftError::Parse {
                    line: line_number,
                    message: format!("unknown phase '{value}'"),
                });
            }
        };
        let depth = parts[2].parse::<u8>().map_err(|_| PerftError::Parse {
            line: line_number,
            message: format!("invalid depth '{}'", parts[2]),
        })?;
        let nodes = parts[3].parse::<u64>().map_err(|_| PerftError::Parse {
            line: line_number,
            message: format!("invalid node count '{}'", parts[3]),
        })?;
        parse_fen(parts[4]).map_err(|source| PerftError::Fen {
            line: line_number,
            source,
        })?;
        cases.push(PerftCase {
            id: parts[0].to_owned(),
            phase,
            depth,
            nodes,
            fen: parts[4].to_owned(),
        });
    }
    Ok(cases)
}

pub fn run_perft_suite(path: &Path, max_depth: Option<u8>) -> Result<SuiteReport, PerftError> {
    let cases = parse_perft_suite(path)?;
    let mut report = SuiteReport::default();

    for case in cases {
        if case.phase != PerftPhase::Mandatory {
            continue;
        }
        if max_depth.is_some_and(|limit| case.depth > limit) {
            continue;
        }
        let mut pos = parse_fen(&case.fen).map_err(|source| PerftError::Fen { line: 0, source })?;
        let actual = perft(&mut pos, case.depth);
        report.cases.push(SuiteCaseReport {
            id: case.id,
            phase: case.phase,
            depth: case.depth,
            expected: case.nodes,
            actual,
            fen: case.fen,
        });
    }

    if report.is_success() {
        Ok(report)
    } else {
        Err(PerftError::SuiteMismatch(report))
    }
}

fn accumulate_leaf_stats(pos_after_move: &Position, mv: Move, result: &mut PerftResult) {
    match mv.kind {
        MoveKind::Capture | MoveKind::PromotionCapture(_) => result.captures += 1,
        MoveKind::EnPassant => {
            result.captures += 1;
            result.en_passant += 1;
        }
        _ => {}
    }
    if matches!(
        mv.kind,
        MoveKind::CastleKingside | MoveKind::CastleQueenside
    ) {
        result.castles += 1;
    }
    if matches!(
        mv.kind,
        MoveKind::Promotion(_) | MoveKind::PromotionCapture(_)
    ) {
        result.promotions += 1;
    }
    let checked_side = pos_after_move.side_to_move;
    if in_check(pos_after_move, checked_side) {
        result.checks += 1;
        let mut replies = MoveList::new();
        legal_moves(pos_after_move, &mut replies);
        if replies.is_empty() {
            result.checkmates += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_core::{MoveKind, PieceKind, Square, startpos};

    #[test]
    fn startpos_counts_match_known_early_depths() {
        let mut pos = startpos();
        assert_eq!(perft(&mut pos, 0), 1);
        assert_eq!(perft(&mut pos, 1), 20);
        assert_eq!(perft(&mut pos, 2), 400);
        assert_eq!(perft(&mut pos, 3), 8_902);
    }

    #[test]
    fn divide_sums_to_perft() {
        let mut pos = startpos();
        let divide = perft_divide(&mut pos, 3);
        let sum: u64 = divide.iter().map(|(_, nodes)| *nodes).sum();
        assert_eq!(sum, perft(&mut startpos(), 3));
    }

    #[test]
    fn stats_count_special_leaf_moves() {
        let mut pos = parse_fen("1r2k2r/P6p/8/3pP3/8/8/8/R3K2R w KQk d6 0 1").unwrap();
        let stats = perft_stats(&mut pos, 1);
        assert!(stats.en_passant > 0);
        assert!(stats.castles > 0);
        assert!(stats.promotions > 0);
        assert!(stats.captures >= stats.en_passant);

        let divide = perft_divide(&mut pos, 1);
        assert!(divide.iter().any(|(mv, _)| mv.kind == MoveKind::EnPassant));
        assert!(divide.iter().any(|(mv, _)| {
            mv.from == Square(48)
                && mv.to == Square(56)
                && mv.kind == MoveKind::Promotion(PieceKind::Queen)
        }));
    }

    #[test]
    fn parses_fixture_rows() {
        let path = Path::new("../../tests/fixtures/perft.epd");
        let cases = parse_perft_suite(path).unwrap();
        assert!(cases.iter().any(|case| {
            case.id == "startpos"
                && case.phase == PerftPhase::Mandatory
                && case.depth == 6
                && case.nodes == 119_060_324
        }));
    }
}
