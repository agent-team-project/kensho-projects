#![doc = "Bitboard board model, FEN, move generation, and make/unmake."]

use std::error::Error;
use std::fmt;
use std::str::FromStr;

pub const MAX_LEGAL_MOVES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::White => 0,
            Self::Black => 1,
        }
    }

    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Pawn => 0,
            Self::Knight => 1,
            Self::Bishop => 2,
            Self::Rook => 3,
            Self::Queen => 4,
            Self::King => 5,
        }
    }

    #[must_use]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Pawn),
            1 => Some(Self::Knight),
            2 => Some(Self::Bishop),
            3 => Some(Self::Rook),
            4 => Some(Self::Queen),
            5 => Some(Self::King),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Square(pub u8);

impl Square {
    #[must_use]
    pub const fn new(index: u8) -> Option<Self> {
        if index < 64 { Some(Self(index)) } else { None }
    }

    #[must_use]
    pub const fn from_file_rank(file: u8, rank: u8) -> Option<Self> {
        if file < 8 && rank < 8 {
            Some(Self(rank * 8 + file))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[must_use]
    pub const fn file(self) -> u8 {
        self.0 & 7
    }

    #[must_use]
    pub const fn rank(self) -> u8 {
        self.0 >> 3
    }

    #[must_use]
    pub fn parse_name(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if bytes.len() != 2 {
            return None;
        }
        let file = bytes[0];
        let rank = bytes[1];
        if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
            return None;
        }
        Self::from_file_rank(file - b'a', rank - b'1')
    }

    #[must_use]
    pub fn name(self) -> String {
        let file = char::from(b'a' + self.file());
        let rank = char::from(b'1' + self.rank());
        format!("{file}{rank}")
    }

    #[must_use]
    pub const fn bit(self) -> u64 {
        1u64 << self.0
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl FromStr for Square {
    type Err = SquareParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse_name(input).ok_or_else(|| SquareParseError {
            input: input.to_owned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SquareParseError {
    pub input: String,
}

impl fmt::Display for SquareParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid square '{}'", self.input)
    }
}

impl Error for SquareParseError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bitboard(pub u64);

impl Bitboard {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn contains(self, sq: Square) -> bool {
        self.0 & sq.bit() != 0
    }

    pub fn set(&mut self, sq: Square) {
        self.0 |= sq.bit();
    }

    pub fn clear(&mut self, sq: Square) {
        self.0 &= !sq.bit();
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn iter(self) -> BitboardIter {
        BitboardIter(self.0)
    }
}

pub struct BitboardIter(u64);

impl Iterator for BitboardIter {
    type Item = Square;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(Square(index))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveKind {
    Quiet,
    DoublePawnPush,
    Capture,
    EnPassant,
    CastleKingside,
    CastleQueenside,
    Promotion(PieceKind),
    PromotionCapture(PieceKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub kind: MoveKind,
}

impl Move {
    #[must_use]
    pub const fn new(from: Square, to: Square, kind: MoveKind) -> Self {
        Self { from, to, kind }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.from, self.to)?;
        let promotion = match self.kind {
            MoveKind::Promotion(kind) | MoveKind::PromotionCapture(kind) => Some(kind),
            _ => None,
        };
        if let Some(kind) = promotion {
            let suffix = match kind {
                PieceKind::Knight => 'n',
                PieceKind::Bishop => 'b',
                PieceKind::Rook => 'r',
                PieceKind::Queen => 'q',
                PieceKind::Pawn | PieceKind::King => return Err(fmt::Error),
            };
            write!(f, "{suffix}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CastleRights {
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub side_to_move: Color,
    pub pieces: [[Bitboard; 6]; 2],
    pub occupied: [Bitboard; 2],
    pub all_occupied: Bitboard,
    pub castle_rights: CastleRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u16,
    pub fullmove_number: u16,
    pub zobrist: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Undo {
    pub captured: Option<Piece>,
    pub castle_rights: CastleRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u16,
    pub fullmove_number: u16,
    pub zobrist: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FenError {
    WrongFieldCount { found: usize },
    InvalidBoard(String),
    InvalidSideToMove(String),
    InvalidCastling(String),
    InvalidEnPassant(String),
    InvalidHalfmove(String),
    InvalidFullmove(String),
    InvalidPosition(PositionError),
}

impl fmt::Display for FenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongFieldCount { found } => {
                write!(f, "FEN must contain 6 fields, found {found}")
            }
            Self::InvalidBoard(message) => write!(f, "invalid FEN board: {message}"),
            Self::InvalidSideToMove(value) => write!(f, "invalid FEN side to move '{value}'"),
            Self::InvalidCastling(value) => write!(f, "invalid FEN castling rights '{value}'"),
            Self::InvalidEnPassant(value) => write!(f, "invalid FEN en-passant square '{value}'"),
            Self::InvalidHalfmove(value) => write!(f, "invalid FEN halfmove clock '{value}'"),
            Self::InvalidFullmove(value) => write!(f, "invalid FEN fullmove number '{value}'"),
            Self::InvalidPosition(err) => write!(f, "invalid FEN position: {err}"),
        }
    }
}

impl Error for FenError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionError {
    OverlappingPieces,
    OccupancyMismatch,
    InvalidZobrist { expected: u64, actual: u64 },
    MissingKing(Color),
    MultipleKings(Color),
    PawnOnInvalidRank(Color),
    AdjacentKings,
    OpponentKingInCheck,
    InvalidCastlingRights(String),
    InvalidEnPassantSquare(Square),
    TooManyPieces(Color),
}

impl fmt::Display for PositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OverlappingPieces => f.write_str("piece bitboards overlap"),
            Self::OccupancyMismatch => f.write_str("occupancy bitboards are not synchronized"),
            Self::InvalidZobrist { expected, actual } => {
                write!(f, "zobrist mismatch: expected {expected}, got {actual}")
            }
            Self::MissingKing(color) => write!(f, "{color:?} king is missing"),
            Self::MultipleKings(color) => write!(f, "{color:?} has multiple kings"),
            Self::PawnOnInvalidRank(color) => write!(f, "{color:?} pawn on first or eighth rank"),
            Self::AdjacentKings => f.write_str("kings are adjacent"),
            Self::OpponentKingInCheck => f.write_str("side not to move is in check"),
            Self::InvalidCastlingRights(message) => write!(f, "invalid castling rights: {message}"),
            Self::InvalidEnPassantSquare(square) => {
                write!(f, "invalid en-passant square {square}")
            }
            Self::TooManyPieces(color) => write!(f, "{color:?} has too many pieces"),
        }
    }
}

impl Error for PositionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MoveError {
    IllegalMove(Move),
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IllegalMove(mv) => write!(f, "illegal move {mv}"),
        }
    }
}

impl Error for MoveError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveList {
    len: usize,
    moves: [Move; MAX_LEGAL_MOVES],
}

impl MoveList {
    #[must_use]
    pub const fn new() -> Self {
        const EMPTY_MOVE: Move = Move {
            from: Square(0),
            to: Square(0),
            kind: MoveKind::Quiet,
        };
        Self {
            len: 0,
            moves: [EMPTY_MOVE; MAX_LEGAL_MOVES],
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn push(&mut self, mv: Move) {
        assert!(self.len < MAX_LEGAL_MOVES, "move list capacity exceeded");
        self.moves[self.len] = mv;
        self.len += 1;
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..self.len]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Move> {
        self.as_slice().iter()
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> IntoIterator for &'a MoveList {
    type IntoIter = std::slice::Iter<'a, Move>;
    type Item = &'a Move;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

#[must_use]
pub fn startpos() -> Position {
    parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
        .expect("built-in start position FEN must be valid")
}

pub fn parse_fen(input: &str) -> Result<Position, FenError> {
    let fields: Vec<&str> = input.split_whitespace().collect();
    if fields.len() != 6 {
        return Err(FenError::WrongFieldCount {
            found: fields.len(),
        });
    }

    let mut pos = Position {
        side_to_move: Color::White,
        pieces: [[Bitboard::empty(); 6]; 2],
        occupied: [Bitboard::empty(); 2],
        all_occupied: Bitboard::empty(),
        castle_rights: CastleRights::default(),
        en_passant: None,
        halfmove_clock: 0,
        fullmove_number: 1,
        zobrist: 0,
    };

    parse_board(fields[0], &mut pos)?;

    pos.side_to_move = match fields[1] {
        "w" => Color::White,
        "b" => Color::Black,
        value => return Err(FenError::InvalidSideToMove(value.to_owned())),
    };

    pos.castle_rights = parse_castle_rights(fields[2])?;

    pos.en_passant = if fields[3] == "-" {
        None
    } else {
        let square = Square::parse_name(fields[3])
            .ok_or_else(|| FenError::InvalidEnPassant(fields[3].to_owned()))?;
        let rank = square.rank();
        if rank != 2 && rank != 5 {
            return Err(FenError::InvalidEnPassant(fields[3].to_owned()));
        }
        Some(square)
    };

    pos.halfmove_clock = fields[4]
        .parse::<u16>()
        .map_err(|_| FenError::InvalidHalfmove(fields[4].to_owned()))?;
    pos.fullmove_number = fields[5]
        .parse::<u16>()
        .map_err(|_| FenError::InvalidFullmove(fields[5].to_owned()))?;
    if pos.fullmove_number == 0 {
        return Err(FenError::InvalidFullmove(fields[5].to_owned()));
    }

    sync_derived(&mut pos);
    validate_position(&pos).map_err(FenError::InvalidPosition)?;
    Ok(pos)
}

#[must_use]
pub fn to_fen(pos: &Position) -> String {
    let mut board = String::new();
    for rank in (0..8).rev() {
        let mut empty = 0u8;
        for file in 0..8 {
            let sq = Square::from_file_rank(file, rank).expect("file/rank in bounds");
            if let Some(piece) = piece_at(pos, sq) {
                if empty > 0 {
                    board.push(char::from(b'0' + empty));
                    empty = 0;
                }
                board.push(piece_to_fen(piece));
            } else {
                empty += 1;
            }
        }
        if empty > 0 {
            board.push(char::from(b'0' + empty));
        }
        if rank > 0 {
            board.push('/');
        }
    }

    let stm = match pos.side_to_move {
        Color::White => "w",
        Color::Black => "b",
    };

    let mut castling = String::new();
    if pos.castle_rights.white_kingside {
        castling.push('K');
    }
    if pos.castle_rights.white_queenside {
        castling.push('Q');
    }
    if pos.castle_rights.black_kingside {
        castling.push('k');
    }
    if pos.castle_rights.black_queenside {
        castling.push('q');
    }
    if castling.is_empty() {
        castling.push('-');
    }

    let ep = pos.en_passant.map_or_else(|| "-".to_owned(), Square::name);

    format!(
        "{board} {stm} {castling} {ep} {} {}",
        pos.halfmove_clock, pos.fullmove_number
    )
}

#[must_use]
pub fn piece_at(pos: &Position, sq: Square) -> Option<Piece> {
    let mask = sq.bit();
    let color = if pos.occupied[Color::White.index()].0 & mask != 0 {
        Color::White
    } else if pos.occupied[Color::Black.index()].0 & mask != 0 {
        Color::Black
    } else {
        return None;
    };

    for kind_index in 0..6 {
        if pos.pieces[color.index()][kind_index].0 & mask != 0 {
            return Some(Piece {
                color,
                kind: PieceKind::from_index(kind_index).expect("kind index in range"),
            });
        }
    }
    None
}

pub fn validate_position(pos: &Position) -> Result<(), PositionError> {
    let mut all_seen = 0u64;
    let mut expected_occupied = [0u64; 2];
    for color in [Color::White, Color::Black] {
        let mut piece_count = 0u32;
        for kind_index in 0..6 {
            let bits = pos.pieces[color.index()][kind_index].0;
            if all_seen & bits != 0 {
                return Err(PositionError::OverlappingPieces);
            }
            all_seen |= bits;
            expected_occupied[color.index()] |= bits;
            piece_count += bits.count_ones();
        }
        if piece_count > 16 {
            return Err(PositionError::TooManyPieces(color));
        }
    }

    if pos.occupied[Color::White.index()].0 != expected_occupied[Color::White.index()]
        || pos.occupied[Color::Black.index()].0 != expected_occupied[Color::Black.index()]
        || pos.all_occupied.0
            != (expected_occupied[Color::White.index()] | expected_occupied[Color::Black.index()])
    {
        return Err(PositionError::OccupancyMismatch);
    }

    let expected_hash = compute_zobrist(pos);
    if pos.zobrist != expected_hash {
        return Err(PositionError::InvalidZobrist {
            expected: expected_hash,
            actual: pos.zobrist,
        });
    }

    for color in [Color::White, Color::Black] {
        let kings = pos.pieces[color.index()][PieceKind::King.index()].count();
        if kings == 0 {
            return Err(PositionError::MissingKing(color));
        }
        if kings > 1 {
            return Err(PositionError::MultipleKings(color));
        }
        let pawns = pos.pieces[color.index()][PieceKind::Pawn.index()].0;
        if pawns & (RANK_1 | RANK_8) != 0 {
            return Err(PositionError::PawnOnInvalidRank(color));
        }
    }

    let white_king = king_square(pos, Color::White).expect("validated king exists");
    let black_king = king_square(pos, Color::Black).expect("validated king exists");
    if king_attacks(white_king) & black_king.bit() != 0 {
        return Err(PositionError::AdjacentKings);
    }

    if in_check(pos, pos.side_to_move.opposite()) {
        return Err(PositionError::OpponentKingInCheck);
    }

    validate_castle_rights(pos)?;
    validate_en_passant(pos)?;

    Ok(())
}

pub fn pseudo_legal_moves(pos: &Position, out: &mut MoveList) {
    out.clear();
    generate_pawn_moves(pos, out);
    generate_knight_moves(pos, out);
    generate_slider_moves(pos, out, PieceKind::Bishop);
    generate_slider_moves(pos, out, PieceKind::Rook);
    generate_slider_moves(pos, out, PieceKind::Queen);
    generate_king_moves(pos, out);
}

pub fn legal_moves(pos: &Position, out: &mut MoveList) {
    let mut pseudo = MoveList::new();
    pseudo_legal_moves(pos, &mut pseudo);
    out.clear();
    let side = pos.side_to_move;
    for &mv in &pseudo {
        if move_leaves_king_safe(pos, mv, side) {
            out.push(mv);
        }
    }
}

pub fn legal_captures(pos: &Position, out: &mut MoveList) {
    let mut legal = MoveList::new();
    legal_moves(pos, &mut legal);
    out.clear();
    for &mv in &legal {
        if matches!(
            mv.kind,
            MoveKind::Capture | MoveKind::EnPassant | MoveKind::PromotionCapture(_)
        ) {
            out.push(mv);
        }
    }
}

#[must_use]
pub fn is_square_attacked(pos: &Position, sq: Square, by: Color) -> bool {
    let by_index = by.index();
    let file = sq.file();
    let rank = sq.rank();

    let pawn_attackers = match by {
        Color::White => {
            let mut bits = 0u64;
            if file > 0 && rank > 0 {
                bits |= Square(sq.0 - 9).bit();
            }
            if file < 7 && rank > 0 {
                bits |= Square(sq.0 - 7).bit();
            }
            bits
        }
        Color::Black => {
            let mut bits = 0u64;
            if file > 0 && rank < 7 {
                bits |= Square(sq.0 + 7).bit();
            }
            if file < 7 && rank < 7 {
                bits |= Square(sq.0 + 9).bit();
            }
            bits
        }
    };
    if pawn_attackers & pos.pieces[by_index][PieceKind::Pawn.index()].0 != 0 {
        return true;
    }

    if knight_attacks(sq) & pos.pieces[by_index][PieceKind::Knight.index()].0 != 0 {
        return true;
    }

    if king_attacks(sq) & pos.pieces[by_index][PieceKind::King.index()].0 != 0 {
        return true;
    }

    attacked_by_slider(
        pos,
        sq,
        by,
        &BISHOP_DIRECTIONS,
        &[PieceKind::Bishop, PieceKind::Queen],
    ) || attacked_by_slider(
        pos,
        sq,
        by,
        &ROOK_DIRECTIONS,
        &[PieceKind::Rook, PieceKind::Queen],
    )
}

#[must_use]
pub fn in_check(pos: &Position, side: Color) -> bool {
    let Some(king) = king_square(pos, side) else {
        return false;
    };
    is_square_attacked(pos, king, side.opposite())
}

pub fn make_move(pos: &mut Position, mv: Move) -> Result<Undo, MoveError> {
    if !is_legal_move(pos, mv) {
        return Err(MoveError::IllegalMove(mv));
    }
    Ok(make_legal_move_unchecked(pos, mv))
}

pub fn unmake_move(pos: &mut Position, mv: Move, undo: Undo) {
    let moved_color = pos.side_to_move.opposite();
    pos.side_to_move = moved_color;

    let moved_piece = piece_at(pos, mv.to).expect("made move must leave a piece on target square");
    remove_piece(pos, mv.to, moved_piece);

    let original_kind = match mv.kind {
        MoveKind::Promotion(_) | MoveKind::PromotionCapture(_) => PieceKind::Pawn,
        _ => moved_piece.kind,
    };
    add_piece(
        pos,
        mv.from,
        Piece {
            color: moved_color,
            kind: original_kind,
        },
    );

    match mv.kind {
        MoveKind::CastleKingside => {
            let (rook_from, rook_to) = match moved_color {
                Color::White => (Square(7), Square(5)),
                Color::Black => (Square(63), Square(61)),
            };
            remove_piece(
                pos,
                rook_to,
                Piece {
                    color: moved_color,
                    kind: PieceKind::Rook,
                },
            );
            add_piece(
                pos,
                rook_from,
                Piece {
                    color: moved_color,
                    kind: PieceKind::Rook,
                },
            );
        }
        MoveKind::CastleQueenside => {
            let (rook_from, rook_to) = match moved_color {
                Color::White => (Square(0), Square(3)),
                Color::Black => (Square(56), Square(59)),
            };
            remove_piece(
                pos,
                rook_to,
                Piece {
                    color: moved_color,
                    kind: PieceKind::Rook,
                },
            );
            add_piece(
                pos,
                rook_from,
                Piece {
                    color: moved_color,
                    kind: PieceKind::Rook,
                },
            );
        }
        _ => {}
    }

    if let Some(captured) = undo.captured {
        let captured_square = match mv.kind {
            MoveKind::EnPassant => match moved_color {
                Color::White => Square(mv.to.0 - 8),
                Color::Black => Square(mv.to.0 + 8),
            },
            _ => mv.to,
        };
        add_piece(pos, captured_square, captured);
    }

    pos.castle_rights = undo.castle_rights;
    pos.en_passant = undo.en_passant;
    pos.halfmove_clock = undo.halfmove_clock;
    pos.fullmove_number = undo.fullmove_number;
    sync_occupancy(pos);
    pos.zobrist = undo.zobrist;
}

#[must_use]
pub fn is_legal_move(pos: &Position, mv: Move) -> bool {
    let mut moves = MoveList::new();
    legal_moves(pos, &mut moves);
    moves.as_slice().contains(&mv)
}

pub fn make_legal_move_unchecked(pos: &mut Position, mv: Move) -> Undo {
    apply_legal_move_unchecked(pos, mv, true)
}

#[doc(hidden)]
pub fn make_legal_move_unchecked_fast(pos: &mut Position, mv: Move) -> Undo {
    apply_legal_move_unchecked(pos, mv, false)
}

fn apply_legal_move_unchecked(pos: &mut Position, mv: Move, update_hash: bool) -> Undo {
    let color = pos.side_to_move;
    let moving_piece =
        piece_at(pos, mv.from).expect("unchecked move must have a piece on source square");
    debug_assert_eq!(moving_piece.color, color);

    let captured = match mv.kind {
        MoveKind::EnPassant => Some(Piece {
            color: color.opposite(),
            kind: PieceKind::Pawn,
        }),
        _ => piece_at(pos, mv.to),
    };

    let undo = Undo {
        captured,
        castle_rights: pos.castle_rights,
        en_passant: pos.en_passant,
        halfmove_clock: pos.halfmove_clock,
        fullmove_number: pos.fullmove_number,
        zobrist: pos.zobrist,
    };

    remove_piece(pos, mv.from, moving_piece);

    if let Some(captured_piece) = captured {
        let captured_square = match mv.kind {
            MoveKind::EnPassant => match color {
                Color::White => Square(mv.to.0 - 8),
                Color::Black => Square(mv.to.0 + 8),
            },
            _ => mv.to,
        };
        remove_piece(pos, captured_square, captured_piece);
    }

    let placed_kind = match mv.kind {
        MoveKind::Promotion(kind) | MoveKind::PromotionCapture(kind) => kind,
        _ => moving_piece.kind,
    };
    add_piece(
        pos,
        mv.to,
        Piece {
            color,
            kind: placed_kind,
        },
    );

    match mv.kind {
        MoveKind::CastleKingside => {
            let (rook_from, rook_to) = match color {
                Color::White => (Square(7), Square(5)),
                Color::Black => (Square(63), Square(61)),
            };
            let rook = Piece {
                color,
                kind: PieceKind::Rook,
            };
            remove_piece(pos, rook_from, rook);
            add_piece(pos, rook_to, rook);
        }
        MoveKind::CastleQueenside => {
            let (rook_from, rook_to) = match color {
                Color::White => (Square(0), Square(3)),
                Color::Black => (Square(56), Square(59)),
            };
            let rook = Piece {
                color,
                kind: PieceKind::Rook,
            };
            remove_piece(pos, rook_from, rook);
            add_piece(pos, rook_to, rook);
        }
        _ => {}
    }

    update_castle_rights(pos, color, moving_piece.kind, mv.from, mv.to, captured);
    pos.en_passant = match mv.kind {
        MoveKind::DoublePawnPush => Some(match color {
            Color::White => Square(mv.from.0 + 8),
            Color::Black => Square(mv.from.0 - 8),
        }),
        _ => None,
    };
    pos.halfmove_clock = if moving_piece.kind == PieceKind::Pawn || captured.is_some() {
        0
    } else {
        pos.halfmove_clock.saturating_add(1)
    };
    if color == Color::Black {
        pos.fullmove_number = pos.fullmove_number.saturating_add(1);
    }
    pos.side_to_move = color.opposite();
    if update_hash {
        sync_derived(pos);
    } else {
        sync_occupancy(pos);
    }
    undo
}

fn parse_board(board: &str, pos: &mut Position) -> Result<(), FenError> {
    let ranks: Vec<&str> = board.split('/').collect();
    if ranks.len() != 8 {
        return Err(FenError::InvalidBoard(format!(
            "expected 8 ranks, found {}",
            ranks.len()
        )));
    }

    for (fen_rank_index, rank_text) in ranks.iter().enumerate() {
        let rank = 7u8 - fen_rank_index as u8;
        let mut file = 0u8;
        for ch in rank_text.chars() {
            if let Some(skip) = ch.to_digit(10) {
                if skip == 0 || skip > 8 {
                    return Err(FenError::InvalidBoard(format!(
                        "invalid empty count '{ch}'"
                    )));
                }
                file = file.saturating_add(skip as u8);
                if file > 8 {
                    return Err(FenError::InvalidBoard(format!(
                        "rank {} has too many squares",
                        8 - fen_rank_index
                    )));
                }
                continue;
            }

            let piece = fen_to_piece(ch)
                .ok_or_else(|| FenError::InvalidBoard(format!("invalid piece '{ch}'")))?;
            if file >= 8 {
                return Err(FenError::InvalidBoard(format!(
                    "rank {} has too many squares",
                    8 - fen_rank_index
                )));
            }
            let sq = Square::from_file_rank(file, rank).expect("FEN file/rank in bounds");
            add_piece(pos, sq, piece);
            file += 1;
        }
        if file != 8 {
            return Err(FenError::InvalidBoard(format!(
                "rank {} has {file} squares",
                8 - fen_rank_index
            )));
        }
    }
    Ok(())
}

fn parse_castle_rights(input: &str) -> Result<CastleRights, FenError> {
    if input == "-" {
        return Ok(CastleRights::default());
    }
    let mut rights = CastleRights::default();
    for ch in input.chars() {
        match ch {
            'K' if !rights.white_kingside => rights.white_kingside = true,
            'Q' if !rights.white_queenside => rights.white_queenside = true,
            'k' if !rights.black_kingside => rights.black_kingside = true,
            'q' if !rights.black_queenside => rights.black_queenside = true,
            _ => return Err(FenError::InvalidCastling(input.to_owned())),
        }
    }
    Ok(rights)
}

fn fen_to_piece(ch: char) -> Option<Piece> {
    let color = if ch.is_ascii_uppercase() {
        Color::White
    } else {
        Color::Black
    };
    let kind = match ch.to_ascii_lowercase() {
        'p' => PieceKind::Pawn,
        'n' => PieceKind::Knight,
        'b' => PieceKind::Bishop,
        'r' => PieceKind::Rook,
        'q' => PieceKind::Queen,
        'k' => PieceKind::King,
        _ => return None,
    };
    Some(Piece { color, kind })
}

fn piece_to_fen(piece: Piece) -> char {
    let ch = match piece.kind {
        PieceKind::Pawn => 'p',
        PieceKind::Knight => 'n',
        PieceKind::Bishop => 'b',
        PieceKind::Rook => 'r',
        PieceKind::Queen => 'q',
        PieceKind::King => 'k',
    };
    match piece.color {
        Color::White => ch.to_ascii_uppercase(),
        Color::Black => ch,
    }
}

fn add_piece(pos: &mut Position, sq: Square, piece: Piece) {
    pos.pieces[piece.color.index()][piece.kind.index()].set(sq);
}

fn remove_piece(pos: &mut Position, sq: Square, piece: Piece) {
    pos.pieces[piece.color.index()][piece.kind.index()].clear(sq);
}

fn sync_occupancy(pos: &mut Position) {
    for color in [Color::White, Color::Black] {
        let mut bits = 0u64;
        for kind_index in 0..6 {
            bits |= pos.pieces[color.index()][kind_index].0;
        }
        pos.occupied[color.index()] = Bitboard(bits);
    }
    pos.all_occupied = Bitboard(pos.occupied[0].0 | pos.occupied[1].0);
}

fn sync_derived(pos: &mut Position) {
    sync_occupancy(pos);
    pos.zobrist = compute_zobrist(pos);
}

fn compute_zobrist(pos: &Position) -> u64 {
    let mut hash = 0u64;
    for color in [Color::White, Color::Black] {
        for kind_index in 0..6 {
            let mut bits = pos.pieces[color.index()][kind_index].0;
            while bits != 0 {
                let sq = bits.trailing_zeros() as u8;
                bits &= bits - 1;
                let key = 1 + u64::from(sq) + 64 * kind_index as u64 + 384 * color.index() as u64;
                hash ^= splitmix64(key);
            }
        }
    }
    if pos.side_to_move == Color::Black {
        hash ^= splitmix64(997);
    }
    if pos.castle_rights.white_kingside {
        hash ^= splitmix64(1_001);
    }
    if pos.castle_rights.white_queenside {
        hash ^= splitmix64(1_002);
    }
    if pos.castle_rights.black_kingside {
        hash ^= splitmix64(1_003);
    }
    if pos.castle_rights.black_queenside {
        hash ^= splitmix64(1_004);
    }
    if let Some(ep) = pos.en_passant {
        hash ^= splitmix64(1_100 + u64::from(ep.file()));
    }
    hash
}

const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn validate_castle_rights(pos: &Position) -> Result<(), PositionError> {
    let white_king = Piece {
        color: Color::White,
        kind: PieceKind::King,
    };
    let black_king = Piece {
        color: Color::Black,
        kind: PieceKind::King,
    };
    let white_rook = Piece {
        color: Color::White,
        kind: PieceKind::Rook,
    };
    let black_rook = Piece {
        color: Color::Black,
        kind: PieceKind::Rook,
    };

    if pos.castle_rights.white_kingside
        && (piece_at(pos, Square(4)) != Some(white_king)
            || piece_at(pos, Square(7)) != Some(white_rook))
    {
        return Err(PositionError::InvalidCastlingRights(
            "white kingside right without king on e1 and rook on h1".to_owned(),
        ));
    }
    if pos.castle_rights.white_queenside
        && (piece_at(pos, Square(4)) != Some(white_king)
            || piece_at(pos, Square(0)) != Some(white_rook))
    {
        return Err(PositionError::InvalidCastlingRights(
            "white queenside right without king on e1 and rook on a1".to_owned(),
        ));
    }
    if pos.castle_rights.black_kingside
        && (piece_at(pos, Square(60)) != Some(black_king)
            || piece_at(pos, Square(63)) != Some(black_rook))
    {
        return Err(PositionError::InvalidCastlingRights(
            "black kingside right without king on e8 and rook on h8".to_owned(),
        ));
    }
    if pos.castle_rights.black_queenside
        && (piece_at(pos, Square(60)) != Some(black_king)
            || piece_at(pos, Square(56)) != Some(black_rook))
    {
        return Err(PositionError::InvalidCastlingRights(
            "black queenside right without king on e8 and rook on a8".to_owned(),
        ));
    }
    Ok(())
}

fn validate_en_passant(pos: &Position) -> Result<(), PositionError> {
    let Some(ep) = pos.en_passant else {
        return Ok(());
    };
    if !is_valid_en_passant_square(pos, ep, pos.side_to_move) {
        return Err(PositionError::InvalidEnPassantSquare(ep));
    }
    Ok(())
}

fn is_valid_en_passant_square(pos: &Position, ep: Square, side_to_move: Color) -> bool {
    let Some((captured, origin, captured_piece)) = en_passant_context(ep, side_to_move) else {
        return false;
    };
    piece_at(pos, ep).is_none()
        && piece_at(pos, captured) == Some(captured_piece)
        && piece_at(pos, origin).is_none()
}

fn en_passant_context(ep: Square, side_to_move: Color) -> Option<(Square, Square, Piece)> {
    match side_to_move {
        Color::White if ep.rank() == 5 => Some((
            Square(ep.0 - 8),
            Square(ep.0 + 8),
            Piece {
                color: Color::Black,
                kind: PieceKind::Pawn,
            },
        )),
        Color::Black if ep.rank() == 2 => Some((
            Square(ep.0 + 8),
            Square(ep.0 - 8),
            Piece {
                color: Color::White,
                kind: PieceKind::Pawn,
            },
        )),
        _ => None,
    }
}

fn king_square(pos: &Position, side: Color) -> Option<Square> {
    let bits = pos.pieces[side.index()][PieceKind::King.index()].0;
    if bits.count_ones() == 1 {
        Some(Square(bits.trailing_zeros() as u8))
    } else {
        None
    }
}

fn generate_pawn_moves(pos: &Position, out: &mut MoveList) {
    let side = pos.side_to_move;
    let own = pos.occupied[side.index()].0;
    let enemy = pos.occupied[side.opposite().index()].0;
    let pawns = pos.pieces[side.index()][PieceKind::Pawn.index()];
    for from in pawns.iter() {
        let rank = from.rank();
        let file = from.file();
        match side {
            Color::White => {
                if rank < 7 {
                    let one = Square(from.0 + 8);
                    if pos.all_occupied.0 & one.bit() == 0 {
                        push_pawn_push(out, from, one, side);
                        if rank == 1 {
                            let two = Square(from.0 + 16);
                            if pos.all_occupied.0 & two.bit() == 0 {
                                out.push(Move::new(from, two, MoveKind::DoublePawnPush));
                            }
                        }
                    }
                    if file > 0 {
                        let to = Square(from.0 + 7);
                        push_pawn_capture(pos, out, from, to, side, enemy);
                    }
                    if file < 7 {
                        let to = Square(from.0 + 9);
                        push_pawn_capture(pos, out, from, to, side, enemy);
                    }
                }
            }
            Color::Black => {
                if rank > 0 {
                    let one = Square(from.0 - 8);
                    if pos.all_occupied.0 & one.bit() == 0 {
                        push_pawn_push(out, from, one, side);
                        if rank == 6 {
                            let two = Square(from.0 - 16);
                            if pos.all_occupied.0 & two.bit() == 0 {
                                out.push(Move::new(from, two, MoveKind::DoublePawnPush));
                            }
                        }
                    }
                    if file > 0 {
                        let to = Square(from.0 - 9);
                        push_pawn_capture(pos, out, from, to, side, enemy);
                    }
                    if file < 7 {
                        let to = Square(from.0 - 7);
                        push_pawn_capture(pos, out, from, to, side, enemy);
                    }
                }
            }
        }
    }
    debug_assert_eq!(own & enemy, 0);
}

fn push_pawn_push(out: &mut MoveList, from: Square, to: Square, side: Color) {
    let promotion_rank = match side {
        Color::White => 7,
        Color::Black => 0,
    };
    if to.rank() == promotion_rank {
        push_promotions(out, from, to, false);
    } else {
        out.push(Move::new(from, to, MoveKind::Quiet));
    }
}

fn push_pawn_capture(
    pos: &Position,
    out: &mut MoveList,
    from: Square,
    to: Square,
    side: Color,
    enemy: u64,
) {
    if enemy & to.bit() != 0 {
        if is_king_on(pos, to, side.opposite()) {
            return;
        }
        let promotion_rank = match side {
            Color::White => 7,
            Color::Black => 0,
        };
        if to.rank() == promotion_rank {
            push_promotions(out, from, to, true);
        } else {
            out.push(Move::new(from, to, MoveKind::Capture));
        }
        return;
    }
    if pos.en_passant == Some(to) && is_valid_en_passant_square(pos, to, side) {
        out.push(Move::new(from, to, MoveKind::EnPassant));
    }
}

fn push_promotions(out: &mut MoveList, from: Square, to: Square, capture: bool) {
    for kind in [
        PieceKind::Queen,
        PieceKind::Rook,
        PieceKind::Bishop,
        PieceKind::Knight,
    ] {
        let move_kind = if capture {
            MoveKind::PromotionCapture(kind)
        } else {
            MoveKind::Promotion(kind)
        };
        out.push(Move::new(from, to, move_kind));
    }
}

fn generate_knight_moves(pos: &Position, out: &mut MoveList) {
    let side = pos.side_to_move;
    let own = pos.occupied[side.index()].0;
    let enemy = pos.occupied[side.opposite().index()].0;
    for from in pos.pieces[side.index()][PieceKind::Knight.index()].iter() {
        let mut attacks = knight_attacks(from) & !own;
        while attacks != 0 {
            let to = Square(attacks.trailing_zeros() as u8);
            attacks &= attacks - 1;
            if piece_at(pos, to).is_some_and(|piece| piece.kind == PieceKind::King) {
                continue;
            }
            let kind = if enemy & to.bit() != 0 {
                MoveKind::Capture
            } else {
                MoveKind::Quiet
            };
            out.push(Move::new(from, to, kind));
        }
    }
}

fn generate_slider_moves(pos: &Position, out: &mut MoveList, kind: PieceKind) {
    let side = pos.side_to_move;
    let directions = match kind {
        PieceKind::Bishop => &BISHOP_DIRECTIONS[..],
        PieceKind::Rook => &ROOK_DIRECTIONS[..],
        PieceKind::Queen => &QUEEN_DIRECTIONS[..],
        _ => unreachable!("only sliders are generated here"),
    };
    for from in pos.pieces[side.index()][kind.index()].iter() {
        for &(df, dr) in directions {
            let mut file = from.file() as i8 + df;
            let mut rank = from.rank() as i8 + dr;
            while (0..8).contains(&file) && (0..8).contains(&rank) {
                let to = Square::from_file_rank(file as u8, rank as u8).expect("ray in bounds");
                if pos.occupied[side.index()].contains(to) {
                    break;
                }
                if pos.occupied[side.opposite().index()].contains(to) {
                    if !is_king_on(pos, to, side.opposite()) {
                        out.push(Move::new(from, to, MoveKind::Capture));
                    }
                    break;
                }
                out.push(Move::new(from, to, MoveKind::Quiet));
                file += df;
                rank += dr;
            }
        }
    }
}

fn generate_king_moves(pos: &Position, out: &mut MoveList) {
    let side = pos.side_to_move;
    let own = pos.occupied[side.index()].0;
    let enemy = pos.occupied[side.opposite().index()].0;
    let Some(from) = king_square(pos, side) else {
        return;
    };
    let mut attacks = king_attacks(from) & !own;
    while attacks != 0 {
        let to = Square(attacks.trailing_zeros() as u8);
        attacks &= attacks - 1;
        if is_king_on(pos, to, side.opposite()) {
            continue;
        }
        let kind = if enemy & to.bit() != 0 {
            MoveKind::Capture
        } else {
            MoveKind::Quiet
        };
        out.push(Move::new(from, to, kind));
    }
    generate_castles(pos, out, side, from);
}

fn generate_castles(pos: &Position, out: &mut MoveList, side: Color, king_from: Square) {
    let enemy = side.opposite();
    match side {
        Color::White if king_from == Square(4) => {
            if pos.castle_rights.white_kingside
                && piece_at(pos, Square(7))
                    == Some(Piece {
                        color: side,
                        kind: PieceKind::Rook,
                    })
                && pos.all_occupied.0 & (Square(5).bit() | Square(6).bit()) == 0
                && !is_square_attacked(pos, Square(4), enemy)
                && !is_square_attacked(pos, Square(5), enemy)
                && !is_square_attacked(pos, Square(6), enemy)
            {
                out.push(Move::new(king_from, Square(6), MoveKind::CastleKingside));
            }
            if pos.castle_rights.white_queenside
                && piece_at(pos, Square(0))
                    == Some(Piece {
                        color: side,
                        kind: PieceKind::Rook,
                    })
                && pos.all_occupied.0 & (Square(1).bit() | Square(2).bit() | Square(3).bit()) == 0
                && !is_square_attacked(pos, Square(4), enemy)
                && !is_square_attacked(pos, Square(3), enemy)
                && !is_square_attacked(pos, Square(2), enemy)
            {
                out.push(Move::new(king_from, Square(2), MoveKind::CastleQueenside));
            }
        }
        Color::Black if king_from == Square(60) => {
            if pos.castle_rights.black_kingside
                && piece_at(pos, Square(63))
                    == Some(Piece {
                        color: side,
                        kind: PieceKind::Rook,
                    })
                && pos.all_occupied.0 & (Square(61).bit() | Square(62).bit()) == 0
                && !is_square_attacked(pos, Square(60), enemy)
                && !is_square_attacked(pos, Square(61), enemy)
                && !is_square_attacked(pos, Square(62), enemy)
            {
                out.push(Move::new(king_from, Square(62), MoveKind::CastleKingside));
            }
            if pos.castle_rights.black_queenside
                && piece_at(pos, Square(56))
                    == Some(Piece {
                        color: side,
                        kind: PieceKind::Rook,
                    })
                && pos.all_occupied.0 & (Square(57).bit() | Square(58).bit() | Square(59).bit())
                    == 0
                && !is_square_attacked(pos, Square(60), enemy)
                && !is_square_attacked(pos, Square(59), enemy)
                && !is_square_attacked(pos, Square(58), enemy)
            {
                out.push(Move::new(king_from, Square(58), MoveKind::CastleQueenside));
            }
        }
        _ => {}
    }
}

fn move_leaves_king_safe(pos: &Position, mv: Move, side: Color) -> bool {
    let mut next = pos.clone();
    make_legal_move_unchecked_fast(&mut next, mv);
    !in_check(&next, side)
}

fn attacked_by_slider(
    pos: &Position,
    sq: Square,
    by: Color,
    directions: &[(i8, i8)],
    attackers: &[PieceKind],
) -> bool {
    for &(df, dr) in directions {
        let mut file = sq.file() as i8 + df;
        let mut rank = sq.rank() as i8 + dr;
        while (0..8).contains(&file) && (0..8).contains(&rank) {
            let ray_sq = Square::from_file_rank(file as u8, rank as u8).expect("ray in bounds");
            let mask = ray_sq.bit();
            if pos.all_occupied.0 & mask != 0 {
                let by_index = by.index();
                if attackers
                    .iter()
                    .any(|kind| pos.pieces[by_index][kind.index()].0 & mask != 0)
                {
                    return true;
                }
                break;
            }
            file += df;
            rank += dr;
        }
    }
    false
}

fn is_king_on(pos: &Position, sq: Square, color: Color) -> bool {
    pos.pieces[color.index()][PieceKind::King.index()].contains(sq)
}

fn knight_attacks(sq: Square) -> u64 {
    let mut attacks = 0u64;
    for (df, dr) in [
        (1, 2),
        (2, 1),
        (2, -1),
        (1, -2),
        (-1, -2),
        (-2, -1),
        (-2, 1),
        (-1, 2),
    ] {
        let file = sq.file() as i8 + df;
        let rank = sq.rank() as i8 + dr;
        if (0..8).contains(&file) && (0..8).contains(&rank) {
            attacks |= Square::from_file_rank(file as u8, rank as u8)
                .expect("knight target in bounds")
                .bit();
        }
    }
    attacks
}

fn king_attacks(sq: Square) -> u64 {
    let mut attacks = 0u64;
    for df in -1..=1 {
        for dr in -1..=1 {
            if df == 0 && dr == 0 {
                continue;
            }
            let file = sq.file() as i8 + df;
            let rank = sq.rank() as i8 + dr;
            if (0..8).contains(&file) && (0..8).contains(&rank) {
                attacks |= Square::from_file_rank(file as u8, rank as u8)
                    .expect("king target in bounds")
                    .bit();
            }
        }
    }
    attacks
}

fn update_castle_rights(
    pos: &mut Position,
    color: Color,
    moving_kind: PieceKind,
    from: Square,
    to: Square,
    captured: Option<Piece>,
) {
    if moving_kind == PieceKind::King {
        match color {
            Color::White => {
                pos.castle_rights.white_kingside = false;
                pos.castle_rights.white_queenside = false;
            }
            Color::Black => {
                pos.castle_rights.black_kingside = false;
                pos.castle_rights.black_queenside = false;
            }
        }
    }

    if moving_kind == PieceKind::Rook {
        clear_rook_castle_right(pos, color, from);
    }
    if captured.is_some_and(|piece| piece.kind == PieceKind::Rook) {
        clear_rook_castle_right(pos, color.opposite(), to);
    }
}

fn clear_rook_castle_right(pos: &mut Position, color: Color, square: Square) {
    match (color, square) {
        (Color::White, Square(0)) => pos.castle_rights.white_queenside = false,
        (Color::White, Square(7)) => pos.castle_rights.white_kingside = false,
        (Color::Black, Square(56)) => pos.castle_rights.black_queenside = false,
        (Color::Black, Square(63)) => pos.castle_rights.black_kingside = false,
        _ => {}
    }
}

const RANK_1: u64 = 0x0000_0000_0000_00ff;
const RANK_8: u64 = 0xff00_0000_0000_0000;
const BISHOP_DIRECTIONS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
const ROOK_DIRECTIONS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const QUEEN_DIRECTIONS: [(i8, i8); 8] = [
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

    fn legal_strings(pos: &Position) -> Vec<String> {
        let mut moves = MoveList::new();
        legal_moves(pos, &mut moves);
        let mut strings: Vec<String> = moves.iter().map(ToString::to_string).collect();
        strings.sort();
        strings
    }

    #[test]
    fn square_names_round_trip() {
        assert_eq!(Square::parse_name("a1"), Some(Square(0)));
        assert_eq!(Square::parse_name("h8"), Some(Square(63)));
        assert_eq!(Square(36).name(), "e5");
        assert!("i9".parse::<Square>().is_err());
    }

    #[test]
    fn startpos_fen_round_trips_and_has_twenty_moves() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let pos = parse_fen(fen).unwrap();
        assert_eq!(to_fen(&pos), fen);
        let mut moves = MoveList::new();
        legal_moves(&pos, &mut moves);
        assert_eq!(moves.len(), 20);
        assert_eq!(
            piece_at(&pos, Square::parse_name("e1").unwrap()),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::King
            })
        );
    }

    #[test]
    fn invalid_fen_returns_structured_errors() {
        assert!(matches!(
            parse_fen("8/8/8/8/8/8/8/8 w - - 0 1"),
            Err(FenError::InvalidPosition(PositionError::MissingKing(
                Color::White
            )))
        ));
        assert!(matches!(
            parse_fen("8/8/8/8/8/8/8/8 w - - 0"),
            Err(FenError::WrongFieldCount { found: 5 })
        ));
        assert!(matches!(
            parse_fen("8/8/8/8/8/8/8/7K w - - 0 0"),
            Err(FenError::InvalidFullmove(_))
        ));
    }

    #[test]
    fn occupied_en_passant_target_fens_are_rejected() {
        assert!(matches!(
            parse_fen("4k3/8/3R4/3pP3/8/8/8/4K3 w - d6 0 1"),
            Err(FenError::InvalidPosition(
                PositionError::InvalidEnPassantSquare(Square(43))
            ))
        ));
        assert!(matches!(
            parse_fen("4k3/8/8/8/3pP3/4b3/8/4K3 b - e3 0 1"),
            Err(FenError::InvalidPosition(
                PositionError::InvalidEnPassantSquare(Square(20))
            ))
        ));
    }

    #[test]
    fn occupied_en_passant_origin_fens_are_rejected() {
        assert!(matches!(
            parse_fen("4k3/3n4/8/3pP3/8/8/8/4K3 w - d6 0 1"),
            Err(FenError::InvalidPosition(
                PositionError::InvalidEnPassantSquare(Square(43))
            ))
        ));
        assert!(matches!(
            parse_fen("4k3/8/8/8/3pP3/8/4N3/4K3 b - e3 0 1"),
            Err(FenError::InvalidPosition(
                PositionError::InvalidEnPassantSquare(Square(20))
            ))
        ));
    }

    #[test]
    fn attacks_and_check_are_detected() {
        let pos = parse_fen("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1").unwrap();
        assert!(is_square_attacked(
            &pos,
            Square::parse_name("e1").unwrap(),
            Color::Black
        ));
        assert!(in_check(&pos, Color::White));
        let legal = legal_strings(&pos);
        assert_eq!(legal, vec!["e1d1", "e1e2", "e1f1"]);
    }

    #[test]
    fn castling_moves_rook_and_unmakes() {
        let original = parse_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        let mut pos = original.clone();
        let mv = Move::new(Square(4), Square(6), MoveKind::CastleKingside);
        assert!(is_legal_move(&pos, mv));
        let undo = make_move(&mut pos, mv).unwrap();
        assert_eq!(
            piece_at(&pos, Square(6)),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::King
            })
        );
        assert_eq!(
            piece_at(&pos, Square(5)),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Rook
            })
        );
        unmake_move(&mut pos, mv, undo);
        assert_eq!(pos, original);
    }

    #[test]
    fn illegal_castle_through_check_is_omitted() {
        let pos = parse_fen("r3k2r/8/8/8/8/5r2/8/R3K2R w KQkq - 0 1").unwrap();
        let legal = legal_strings(&pos);
        assert!(!legal.contains(&"e1g1".to_owned()));
        assert!(legal.contains(&"e1c1".to_owned()));
    }

    #[test]
    fn en_passant_discovered_check_is_illegal() {
        let pos = parse_fen("4k3/8/8/r2pP2K/8/8/8/8 w - d6 0 1").unwrap();
        let legal = legal_strings(&pos);
        assert!(!legal.contains(&"e5d6".to_owned()));
    }

    #[test]
    fn occupied_en_passant_target_is_not_generated_or_made() {
        let mut pos = parse_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        add_piece(
            &mut pos,
            Square::parse_name("d6").unwrap(),
            Piece {
                color: Color::White,
                kind: PieceKind::Rook,
            },
        );
        sync_derived(&mut pos);

        assert!(matches!(
            validate_position(&pos),
            Err(PositionError::InvalidEnPassantSquare(Square(43)))
        ));
        let before = pos.clone();
        let mv = Move::new(
            Square::parse_name("e5").unwrap(),
            Square::parse_name("d6").unwrap(),
            MoveKind::EnPassant,
        );
        let legal = legal_strings(&pos);
        assert!(!legal.contains(&"e5d6".to_owned()));
        assert!(!is_legal_move(&pos, mv));
        assert_eq!(make_move(&mut pos, mv), Err(MoveError::IllegalMove(mv)));
        assert_eq!(pos, before);
    }

    #[test]
    fn occupied_en_passant_origin_is_not_generated_or_made() {
        let mut white_to_move = parse_fen("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1").unwrap();
        add_piece(
            &mut white_to_move,
            Square::parse_name("d7").unwrap(),
            Piece {
                color: Color::Black,
                kind: PieceKind::Knight,
            },
        );
        sync_derived(&mut white_to_move);
        assert_invalid_en_passant_move_is_rejected(
            white_to_move,
            Square::parse_name("e5").unwrap(),
            Square::parse_name("d6").unwrap(),
            Square(43),
        );

        let mut black_to_move = parse_fen("4k3/8/8/8/3pP3/8/8/4K3 b - e3 0 1").unwrap();
        add_piece(
            &mut black_to_move,
            Square::parse_name("e2").unwrap(),
            Piece {
                color: Color::White,
                kind: PieceKind::Knight,
            },
        );
        sync_derived(&mut black_to_move);
        assert_invalid_en_passant_move_is_rejected(
            black_to_move,
            Square::parse_name("d4").unwrap(),
            Square::parse_name("e3").unwrap(),
            Square(20),
        );
    }

    fn assert_invalid_en_passant_move_is_rejected(
        mut pos: Position,
        from: Square,
        to: Square,
        ep: Square,
    ) {
        assert!(matches!(
            validate_position(&pos),
            Err(PositionError::InvalidEnPassantSquare(square)) if square == ep
        ));
        let before = pos.clone();
        let mv = Move::new(from, to, MoveKind::EnPassant);
        let legal = legal_strings(&pos);
        assert!(!legal.contains(&mv.to_string()));
        assert!(!is_legal_move(&pos, mv));
        assert_eq!(make_move(&mut pos, mv), Err(MoveError::IllegalMove(mv)));
        assert_eq!(pos, before);
    }

    #[test]
    fn promotions_are_generated_and_unmade() {
        let original = parse_fen("4k3/P6p/8/8/8/8/7P/4K3 w - - 0 1").unwrap();
        let legal = legal_strings(&original);
        assert!(legal.contains(&"a7a8q".to_owned()));
        assert!(legal.contains(&"a7a8n".to_owned()));
        let mut pos = original.clone();
        let mv = Move::new(
            Square(48),
            Square(56),
            MoveKind::Promotion(PieceKind::Queen),
        );
        let undo = make_move(&mut pos, mv).unwrap();
        assert_eq!(
            piece_at(&pos, Square(56)),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Queen
            })
        );
        unmake_move(&mut pos, mv, undo);
        assert_eq!(pos, original);
    }

    #[test]
    fn quiet_capture_and_clock_unmake_restores_position() {
        let mut pos = startpos();
        let original = pos.clone();
        let mv = Move::new(Square(12), Square(28), MoveKind::DoublePawnPush);
        let undo = make_move(&mut pos, mv).unwrap();
        assert_eq!(pos.en_passant, Some(Square(20)));
        unmake_move(&mut pos, mv, undo);
        assert_eq!(pos, original);
    }
}
