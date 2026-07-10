#![doc = "Static evaluation for the local chess engine."]

use chess_core::{Bitboard, Color, PieceKind, Position, Square, piece_at};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Score(pub i32);

pub const MATE_SCORE_VALUE: i32 = 30_000;
pub const MATE_SCORE: Score = Score(MATE_SCORE_VALUE);
pub const TEMPO_BONUS: i32 = 12;

pub trait Evaluator: Send + Sync {
    fn evaluate(&self, pos: &Position) -> Score;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DefaultEvaluator;

impl Evaluator for DefaultEvaluator {
    fn evaluate(&self, pos: &Position) -> Score {
        let phase = game_phase(pos);
        let white = evaluate_color(pos, Color::White, phase);
        let black = evaluate_color(pos, Color::Black, phase);
        let side_score = match pos.side_to_move {
            Color::White => white - black,
            Color::Black => black - white,
        };
        Score(side_score + TEMPO_BONUS)
    }
}

#[must_use]
pub fn default_evaluator() -> DefaultEvaluator {
    DefaultEvaluator
}

#[must_use]
pub const fn material_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 320,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    }
}

fn evaluate_color(pos: &Position, color: Color, phase: i32) -> i32 {
    let mut opening = 0;
    let mut endgame = 0;

    for kind in [
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    ] {
        for square in pos.pieces[color.index()][kind.index()].iter() {
            opening += opening_value(kind) + piece_square_value(kind, square, color, true);
            endgame += endgame_value(kind) + piece_square_value(kind, square, color, false);
        }
    }

    let tapered = (opening * phase + endgame * (24 - phase)) / 24;
    tapered
        + mobility_score(pos, color)
        + pawn_structure_score(pos, color)
        + king_safety_score(pos, color, phase)
}

fn game_phase(pos: &Position) -> i32 {
    let mut phase = 0;
    for color in [Color::White, Color::Black] {
        phase += pos.pieces[color.index()][PieceKind::Knight.index()].count() as i32;
        phase += pos.pieces[color.index()][PieceKind::Bishop.index()].count() as i32;
        phase += 2 * pos.pieces[color.index()][PieceKind::Rook.index()].count() as i32;
        phase += 4 * pos.pieces[color.index()][PieceKind::Queen.index()].count() as i32;
    }
    phase.min(24)
}

const fn opening_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 325,
        PieceKind::Bishop => 335,
        PieceKind::Rook => 500,
        PieceKind::Queen => 910,
        PieceKind::King => 0,
    }
}

const fn endgame_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 120,
        PieceKind::Knight => 315,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 510,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    }
}

fn piece_square_value(kind: PieceKind, square: Square, color: Color, opening: bool) -> i32 {
    let file = i32::from(square.file());
    let rank = relative_rank(square, color);
    let center_file = (file * 2 - 7).abs();
    let center_rank = (rank * 2 - 7).abs();
    let center = 14 - center_file - center_rank;

    match kind {
        PieceKind::Pawn => rank * 6 - (file - 3).abs() * 2,
        PieceKind::Knight => center * 5 - 20,
        PieceKind::Bishop => center * 3 + diagonal_length_bonus(square),
        PieceKind::Rook => rank * 2 + rook_file_bonus(file),
        PieceKind::Queen => center * 2,
        PieceKind::King if opening => -center * 5 - rank * 6,
        PieceKind::King => center * 4,
    }
}

fn relative_rank(square: Square, color: Color) -> i32 {
    match color {
        Color::White => i32::from(square.rank()),
        Color::Black => i32::from(7 - square.rank()),
    }
}

fn diagonal_length_bonus(square: Square) -> i32 {
    let file = i32::from(square.file());
    let rank = i32::from(square.rank());
    let diagonal = 7 - (file - rank).abs();
    let anti_diagonal = 7 - (file + rank - 7).abs();
    (diagonal + anti_diagonal) / 2
}

const fn rook_file_bonus(file: i32) -> i32 {
    if file == 0 || file == 7 { -4 } else { 0 }
}

fn mobility_score(pos: &Position, color: Color) -> i32 {
    let mut score = 0;
    let own = pos.occupied[color.index()].0;

    for square in pos.pieces[color.index()][PieceKind::Knight.index()].iter() {
        score += 4 * count_knight_targets(square, own);
    }
    for square in pos.pieces[color.index()][PieceKind::Bishop.index()].iter() {
        score += 3 * count_slider_targets(pos, square, color, &BISHOP_DIRECTIONS);
    }
    for square in pos.pieces[color.index()][PieceKind::Rook.index()].iter() {
        score += 2 * count_slider_targets(pos, square, color, &ROOK_DIRECTIONS);
    }
    for square in pos.pieces[color.index()][PieceKind::Queen.index()].iter() {
        score += count_slider_targets(pos, square, color, &QUEEN_DIRECTIONS);
    }

    score
}

fn pawn_structure_score(pos: &Position, color: Color) -> i32 {
    let pawns = pos.pieces[color.index()][PieceKind::Pawn.index()];
    let enemy_pawns = pos.pieces[color.opposite().index()][PieceKind::Pawn.index()];
    let mut files = [0u8; 8];
    for square in pawns.iter() {
        files[usize::from(square.file())] += 1;
    }

    let mut score = 0;
    for square in pawns.iter() {
        let file = usize::from(square.file());
        let rank = relative_rank(square, color);

        if files[file] > 1 {
            score -= 14 * i32::from(files[file] - 1);
        }
        if adjacent_file_count(&files, file) == 0 {
            score -= 12;
        }
        if has_connected_pawn(pawns, square, color) {
            score += 8;
        }
        if is_passed_pawn(enemy_pawns, square, color) {
            score += 12 + rank * rank * 3;
        }
        if is_protected_by_pawn(pawns, square, color) {
            score += 6;
        }
    }

    score
}

fn king_safety_score(pos: &Position, color: Color, phase: i32) -> i32 {
    let Some(king) = single_piece_square(pos, color, PieceKind::King) else {
        return 0;
    };

    let file = i32::from(king.file());
    let rank = relative_rank(king, color);
    let pawns = pos.pieces[color.index()][PieceKind::Pawn.index()];
    let mut shelter = 0;
    let mut open_files = 0;

    for adjacent_file in (file - 1)..=(file + 1) {
        if !(0..8).contains(&adjacent_file) {
            continue;
        }
        if has_pawn_on_file_ahead(pawns, adjacent_file as u8, rank, color) {
            shelter += 1;
        } else {
            open_files += 1;
        }
    }

    let opening_weight = phase;
    let endgame_weight = 24 - phase;
    let opening_score = shelter * 9 - open_files * 10 - rank * 5;
    let endgame_score = piece_square_value(PieceKind::King, king, color, false);
    (opening_score * opening_weight + endgame_score * endgame_weight) / 24
}

fn single_piece_square(pos: &Position, color: Color, kind: PieceKind) -> Option<Square> {
    let bits = pos.pieces[color.index()][kind.index()].0;
    if bits.count_ones() == 1 {
        Some(Square(bits.trailing_zeros() as u8))
    } else {
        None
    }
}

fn adjacent_file_count(files: &[u8; 8], file: usize) -> u8 {
    let left = file.checked_sub(1).map_or(0, |left_file| files[left_file]);
    let right = files.get(file + 1).copied().unwrap_or(0);
    left + right
}

fn has_connected_pawn(pawns: Bitboard, square: Square, color: Color) -> bool {
    let file = i32::from(square.file());
    let rank = i32::from(square.rank());
    for df in [-1, 1] {
        let target_file = file + df;
        if !(0..8).contains(&target_file) {
            continue;
        }
        for rank_delta in [-1, 0, 1] {
            let target_rank = rank + rank_delta;
            if !(0..8).contains(&target_rank) {
                continue;
            }
            let target = Square::from_file_rank(target_file as u8, target_rank as u8)
                .expect("connected pawn target in bounds");
            if pawns.contains(target)
                && relative_rank(target, color) <= relative_rank(square, color) + 1
            {
                return true;
            }
        }
    }
    false
}

fn is_protected_by_pawn(pawns: Bitboard, square: Square, color: Color) -> bool {
    let file = i32::from(square.file());
    let rank = i32::from(square.rank());
    let source_rank = match color {
        Color::White => rank - 1,
        Color::Black => rank + 1,
    };
    if !(0..8).contains(&source_rank) {
        return false;
    }

    for source_file in [file - 1, file + 1] {
        if !(0..8).contains(&source_file) {
            continue;
        }
        let source = Square::from_file_rank(source_file as u8, source_rank as u8)
            .expect("protecting pawn source in bounds");
        if pawns.contains(source) {
            return true;
        }
    }
    false
}

fn is_passed_pawn(enemy_pawns: Bitboard, square: Square, color: Color) -> bool {
    let file = i32::from(square.file());
    let rank = i32::from(square.rank());
    for enemy in enemy_pawns.iter() {
        let enemy_file = i32::from(enemy.file());
        if (enemy_file - file).abs() > 1 {
            continue;
        }
        let enemy_rank = i32::from(enemy.rank());
        match color {
            Color::White if enemy_rank > rank => return false,
            Color::Black if enemy_rank < rank => return false,
            _ => {}
        }
    }
    true
}

fn has_pawn_on_file_ahead(
    pawns: Bitboard,
    file: u8,
    king_relative_rank: i32,
    color: Color,
) -> bool {
    for pawn in pawns.iter() {
        if pawn.file() != file {
            continue;
        }
        let pawn_rank = relative_rank(pawn, color);
        if pawn_rank > king_relative_rank && pawn_rank <= king_relative_rank + 3 {
            return true;
        }
    }
    false
}

fn count_knight_targets(square: Square, own: u64) -> i32 {
    let mut count = 0;
    for (df, dr) in KNIGHT_DELTAS {
        let file = i32::from(square.file()) + df;
        let rank = i32::from(square.rank()) + dr;
        if !(0..8).contains(&file) || !(0..8).contains(&rank) {
            continue;
        }
        let target =
            Square::from_file_rank(file as u8, rank as u8).expect("knight target in bounds");
        if own & target.bit() == 0 {
            count += 1;
        }
    }
    count
}

fn count_slider_targets(
    pos: &Position,
    square: Square,
    color: Color,
    directions: &[(i32, i32)],
) -> i32 {
    let mut count = 0;
    for &(df, dr) in directions {
        let mut file = i32::from(square.file()) + df;
        let mut rank = i32::from(square.rank()) + dr;
        while (0..8).contains(&file) && (0..8).contains(&rank) {
            let target =
                Square::from_file_rank(file as u8, rank as u8).expect("slider target in bounds");
            if let Some(piece) = piece_at(pos, target) {
                if piece.color != color && piece.kind != PieceKind::King {
                    count += 1;
                }
                break;
            }
            count += 1;
            file += df;
            rank += dr;
        }
    }
    count
}

const KNIGHT_DELTAS: [(i32, i32); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];
const BISHOP_DIRECTIONS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const ROOK_DIRECTIONS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const QUEEN_DIRECTIONS: [(i32, i32); 8] = [
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
];

#[cfg(test)]
mod tests {
    use super::*;
    use chess_core::parse_fen;

    fn eval(fen: &str) -> i32 {
        DefaultEvaluator.evaluate(&parse_fen(fen).unwrap()).0
    }

    #[test]
    fn material_advantage_is_positive_for_side_to_move() {
        let white_queen = eval("4k3/8/8/8/8/8/8/4KQ2 w - - 0 1");
        let black_queen = eval("4kq2/8/8/8/8/8/8/4K3 b - - 0 1");

        assert!(white_queen > 850);
        assert!(black_queen > 850);
        assert!((white_queen - black_queen).abs() < 60);
    }

    #[test]
    fn side_to_move_symmetry_keeps_tempo_only() {
        let white_to_move = eval("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
        let black_to_move = eval("4k3/8/8/8/8/8/8/4K3 b - - 0 1");

        assert_eq!(white_to_move, TEMPO_BONUS);
        assert_eq!(black_to_move, TEMPO_BONUS);
    }

    #[test]
    fn advanced_passed_pawn_scores_above_home_rank_pawn() {
        let advanced = eval("4k3/p7/8/3P4/8/8/8/4K3 w - - 0 1");
        let undeveloped = eval("4k3/p7/8/8/8/8/3P4/4K3 w - - 0 1");

        assert!(advanced > undeveloped + 35);
    }
}
