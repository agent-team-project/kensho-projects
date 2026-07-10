#![doc = "Command-line chess engine application and local release harnesses."]

use chess_core::{
    Color, Move, MoveKind, MoveList, PieceKind, Position, Square, in_check, legal_moves,
    make_legal_move_unchecked, piece_at, startpos,
};
use chess_eval::material_value;
use chess_search::{DefaultSearcher, SearchLimits, Searcher, StopToken};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

pub const ENGINE_NAME: &str = "Kensho Chess Engine";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_BASELINE_GAMES: usize = 12;
pub const DEFAULT_BASELINE_MAX_PLIES: usize = 120;
pub const DEFAULT_BASELINE_ENGINE_DEPTH: u8 = 3;

const ADJUDICATION_MARGIN_CP: i32 = 300;
const BASELINE_BOTS: [BaselineBot; 3] = [
    BaselineBot::FirstLegal,
    BaselineBot::CaptureGreedy,
    BaselineBot::PawnPush,
];

#[must_use]
pub fn version_line() -> String {
    format!("{ENGINE_NAME} {ENGINE_VERSION}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaselineTournamentConfig {
    pub games: usize,
    pub max_plies: usize,
    pub engine_depth: u8,
    pub output_dir: PathBuf,
}

impl Default for BaselineTournamentConfig {
    fn default() -> Self {
        Self {
            games: DEFAULT_BASELINE_GAMES,
            max_plies: DEFAULT_BASELINE_MAX_PLIES,
            engine_depth: DEFAULT_BASELINE_ENGINE_DEPTH,
            output_dir: PathBuf::from("target/release-evidence/baseline-tournament"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameOutcome {
    EngineWin,
    EngineLoss,
    Draw,
}

impl GameOutcome {
    #[must_use]
    pub const fn score_x2(self) -> u32 {
        match self {
            Self::EngineWin => 2,
            Self::Draw => 1,
            Self::EngineLoss => 0,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::EngineWin => "engine-win",
            Self::EngineLoss => "engine-loss",
            Self::Draw => "draw",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameReport {
    pub game_number: usize,
    pub baseline: String,
    pub engine_color: Color,
    pub outcome: GameOutcome,
    pub reason: String,
    pub plies: usize,
    pub material_balance_cp: i32,
    pub moves: Vec<String>,
    pub san_moves: Vec<String>,
}

impl GameReport {
    #[must_use]
    pub fn result_token(&self) -> &'static str {
        match (self.engine_color, self.outcome) {
            (_, GameOutcome::Draw) => "1/2-1/2",
            (Color::White, GameOutcome::EngineWin) | (Color::Black, GameOutcome::EngineLoss) => {
                "1-0"
            }
            (Color::Black, GameOutcome::EngineWin) | (Color::White, GameOutcome::EngineLoss) => {
                "0-1"
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaselineTournamentReport {
    pub config: BaselineTournamentConfig,
    pub games: Vec<GameReport>,
    pub pgn_path: PathBuf,
    pub summary_path: PathBuf,
}

impl BaselineTournamentReport {
    #[must_use]
    pub fn wins(&self) -> usize {
        self.games
            .iter()
            .filter(|game| game.outcome == GameOutcome::EngineWin)
            .count()
    }

    #[must_use]
    pub fn losses(&self) -> usize {
        self.games
            .iter()
            .filter(|game| game.outcome == GameOutcome::EngineLoss)
            .count()
    }

    #[must_use]
    pub fn draws(&self) -> usize {
        self.games
            .iter()
            .filter(|game| game.outcome == GameOutcome::Draw)
            .count()
    }

    #[must_use]
    pub fn engine_score_x2(&self) -> u32 {
        self.games.iter().map(|game| game.outcome.score_x2()).sum()
    }

    #[must_use]
    pub fn engine_score_percent(&self) -> f64 {
        if self.games.is_empty() {
            0.0
        } else {
            f64::from(self.engine_score_x2()) * 50.0 / self.games.len() as f64
        }
    }

    #[must_use]
    pub fn summary_text(&self) -> String {
        let bot_names = BASELINE_BOTS
            .iter()
            .map(|bot| bot.name())
            .collect::<Vec<_>>()
            .join(", ");
        let mut text = String::new();
        text.push_str("Kensho deterministic baseline tournament\n");
        text.push_str(&format!("engine: {}\n", version_line()));
        text.push_str(&format!("engine_depth: {}\n", self.config.engine_depth));
        text.push_str(&format!("max_plies: {}\n", self.config.max_plies));
        text.push_str(&format!("games: {}\n", self.games.len()));
        text.push_str(&format!("baselines: {bot_names}\n"));
        text.push_str(&format!(
            "score: {}/{} ({:.1}%)\n",
            self.engine_score_x2(),
            self.games.len() * 2,
            self.engine_score_percent()
        ));
        text.push_str(&format!(
            "record: {} wins, {} draws, {} losses\n",
            self.wins(),
            self.draws(),
            self.losses()
        ));
        text.push_str(&format!("pgn: {}\n", self.pgn_path.display()));
        text.push_str(&format!("summary: {}\n", self.summary_path.display()));
        text.push_str(
            "release_strength_claim: not-claimed; SPEC.md requires 200 baseline games at >=95% plus the tactical threshold before a release strength claim.\n",
        );
        text.push_str("games_detail:\n");
        for game in &self.games {
            text.push_str(&format!(
                "  game {}: baseline={}, engine_color={}, result={}, outcome={}, plies={}, material_balance_cp={}, reason={}\n",
                game.game_number,
                game.baseline,
                color_name(game.engine_color),
                game.result_token(),
                game.outcome.label(),
                game.plies,
                game.material_balance_cp,
                game.reason
            ));
        }
        text
    }
}

#[derive(Debug)]
pub enum BaselineTournamentError {
    Io(io::Error),
    NoGames,
    NoLegalMove { game_number: usize, side: Color },
}

impl fmt::Display for BaselineTournamentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "baseline tournament I/O failed: {err}"),
            Self::NoGames => f.write_str("baseline tournament requires at least one game"),
            Self::NoLegalMove { game_number, side } => write!(
                f,
                "game {game_number} could not select a legal move for {side:?}"
            ),
        }
    }
}

impl Error for BaselineTournamentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::NoGames | Self::NoLegalMove { .. } => None,
        }
    }
}

impl From<io::Error> for BaselineTournamentError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn run_baseline_tournament(
    config: BaselineTournamentConfig,
) -> Result<BaselineTournamentReport, BaselineTournamentError> {
    if config.games == 0 {
        return Err(BaselineTournamentError::NoGames);
    }

    fs::create_dir_all(&config.output_dir)?;
    let mut games = Vec::with_capacity(config.games);
    for index in 0..config.games {
        let baseline = BASELINE_BOTS[index % BASELINE_BOTS.len()];
        let engine_color = if index % 2 == 0 {
            Color::White
        } else {
            Color::Black
        };
        games.push(play_baseline_game(
            index + 1,
            baseline,
            engine_color,
            &config,
        )?);
    }

    let pgn_path = config.output_dir.join("baseline-tournament.pgn");
    let summary_path = config.output_dir.join("baseline-summary.txt");
    let report = BaselineTournamentReport {
        config,
        games,
        pgn_path,
        summary_path,
    };

    fs::write(&report.pgn_path, render_pgn(&report))?;
    fs::write(&report.summary_path, report.summary_text())?;
    Ok(report)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BaselineBot {
    FirstLegal,
    CaptureGreedy,
    PawnPush,
}

impl BaselineBot {
    const fn name(self) -> &'static str {
        match self {
            Self::FirstLegal => "first-legal",
            Self::CaptureGreedy => "capture-greedy",
            Self::PawnPush => "pawn-push",
        }
    }

    fn choose_move(self, pos: &Position) -> Option<Move> {
        match self {
            Self::FirstLegal => sorted_legal_moves(pos).into_iter().next(),
            Self::CaptureGreedy => choose_scored_move(pos, capture_greedy_score),
            Self::PawnPush => choose_scored_move(pos, pawn_push_score),
        }
    }
}

fn play_baseline_game(
    game_number: usize,
    baseline: BaselineBot,
    engine_color: Color,
    config: &BaselineTournamentConfig,
) -> Result<GameReport, BaselineTournamentError> {
    let mut pos = startpos();
    let mut searcher = DefaultSearcher::default();
    searcher.new_game();
    let mut moves = Vec::new();
    let mut san_moves = Vec::new();

    for _ply in 0..config.max_plies {
        let mut legal = MoveList::new();
        legal_moves(&pos, &mut legal);
        if legal.is_empty() {
            let outcome = terminal_outcome(&pos, engine_color);
            return Ok(finish_game(FinishedGame {
                game_number,
                baseline,
                engine_color,
                outcome,
                reason: terminal_reason(&pos),
                pos: &pos,
                moves,
                san_moves,
            }));
        }

        let side = pos.side_to_move;
        let mv = if side == engine_color {
            choose_engine_move(&mut searcher, &pos, config.engine_depth)
        } else {
            baseline.choose_move(&pos)
        }
        .ok_or(BaselineTournamentError::NoLegalMove { game_number, side })?;

        san_moves.push(san_for_move(&pos, mv));
        moves.push(mv.to_string());
        make_legal_move_unchecked(&mut pos, mv);
    }

    let balance = material_balance(&pos, engine_color);
    let outcome = if balance >= ADJUDICATION_MARGIN_CP {
        GameOutcome::EngineWin
    } else if balance <= -ADJUDICATION_MARGIN_CP {
        GameOutcome::EngineLoss
    } else {
        GameOutcome::Draw
    };
    Ok(finish_game(FinishedGame {
        game_number,
        baseline,
        engine_color,
        outcome,
        reason: format!(
            "adjudicated after {} plies by material margin {balance} cp",
            config.max_plies
        ),
        pos: &pos,
        moves,
        san_moves,
    }))
}

struct FinishedGame<'a> {
    game_number: usize,
    baseline: BaselineBot,
    engine_color: Color,
    outcome: GameOutcome,
    reason: String,
    pos: &'a Position,
    moves: Vec<String>,
    san_moves: Vec<String>,
}

fn finish_game(game: FinishedGame<'_>) -> GameReport {
    GameReport {
        game_number: game.game_number,
        baseline: game.baseline.name().to_owned(),
        engine_color: game.engine_color,
        outcome: game.outcome,
        reason: game.reason,
        plies: game.moves.len(),
        material_balance_cp: material_balance(game.pos, game.engine_color),
        moves: game.moves,
        san_moves: game.san_moves,
    }
}

fn choose_engine_move(searcher: &mut DefaultSearcher, pos: &Position, depth: u8) -> Option<Move> {
    let legal = sorted_legal_moves(pos);
    searcher.set_position(pos.clone(), Vec::new());
    let result = searcher.search(SearchLimits::fixed_depth(depth.max(1)), StopToken::new());
    result
        .best_move
        .filter(|best_move| legal.contains(best_move))
        .or_else(|| legal.first().copied())
}

fn sorted_legal_moves(pos: &Position) -> Vec<Move> {
    let mut legal = MoveList::new();
    legal_moves(pos, &mut legal);
    let mut moves = legal.as_slice().to_vec();
    moves.sort_by_key(ToString::to_string);
    moves
}

fn choose_scored_move(pos: &Position, score: fn(&Position, Move) -> i32) -> Option<Move> {
    let mut moves = sorted_legal_moves(pos)
        .into_iter()
        .map(|mv| (score(pos, mv), mv.to_string(), mv))
        .collect::<Vec<_>>();
    moves.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    moves.first().map(|(_, _, mv)| *mv)
}

fn capture_greedy_score(pos: &Position, mv: Move) -> i32 {
    capture_value(pos, mv) * 10 + promotion_value(mv)
}

fn pawn_push_score(pos: &Position, mv: Move) -> i32 {
    let Some(piece) = piece_at(pos, mv.from) else {
        return 0;
    };
    let pawn_advance = if piece.kind == PieceKind::Pawn {
        relative_rank(mv.to, piece.color) * 12
    } else {
        0
    };
    promotion_value(mv) * 10 + capture_value(pos, mv) + pawn_advance
}

fn capture_value(pos: &Position, mv: Move) -> i32 {
    match mv.kind {
        MoveKind::EnPassant => material_value(PieceKind::Pawn),
        MoveKind::Capture | MoveKind::PromotionCapture(_) => piece_at(pos, mv.to)
            .map(|piece| material_value(piece.kind))
            .unwrap_or(0),
        _ => 0,
    }
}

fn promotion_value(mv: Move) -> i32 {
    promotion_piece(mv).map_or(0, material_value)
}

fn terminal_outcome(pos: &Position, engine_color: Color) -> GameOutcome {
    if !in_check(pos, pos.side_to_move) {
        return GameOutcome::Draw;
    }
    if pos.side_to_move == engine_color {
        GameOutcome::EngineLoss
    } else {
        GameOutcome::EngineWin
    }
}

fn terminal_reason(pos: &Position) -> String {
    if in_check(pos, pos.side_to_move) {
        "checkmate".to_owned()
    } else {
        "stalemate".to_owned()
    }
}

fn material_balance(pos: &Position, color: Color) -> i32 {
    material_total(pos, color) - material_total(pos, color.opposite())
}

fn material_total(pos: &Position, color: Color) -> i32 {
    let mut total = 0;
    for kind in [
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    ] {
        total += material_value(kind) * pos.pieces[color.index()][kind.index()].count() as i32;
    }
    total
}

fn san_for_move(pos: &Position, mv: Move) -> String {
    let Some(piece) = piece_at(pos, mv.from) else {
        return mv.to_string();
    };

    let mut san = match mv.kind {
        MoveKind::CastleKingside => "O-O".to_owned(),
        MoveKind::CastleQueenside => "O-O-O".to_owned(),
        _ => {
            let capture = is_capture(mv);
            let mut san = String::new();
            if piece.kind == PieceKind::Pawn {
                if capture {
                    san.push(file_char(mv.from.file()));
                }
            } else {
                san.push(piece_letter(piece.kind));
                san.push_str(&disambiguation(pos, mv, piece.kind));
            }
            if capture {
                san.push('x');
            }
            san.push_str(&mv.to.name());
            if let Some(promotion) = promotion_piece(mv) {
                san.push('=');
                san.push(piece_letter(promotion));
            }
            san
        }
    };

    let mut next = pos.clone();
    make_legal_move_unchecked(&mut next, mv);
    if in_check(&next, next.side_to_move) {
        let mut replies = MoveList::new();
        legal_moves(&next, &mut replies);
        san.push(if replies.is_empty() { '#' } else { '+' });
    }

    san
}

fn disambiguation(pos: &Position, mv: Move, kind: PieceKind) -> String {
    let mut legal = MoveList::new();
    legal_moves(pos, &mut legal);
    let competing = legal
        .iter()
        .copied()
        .filter(|candidate| candidate.from != mv.from && candidate.to == mv.to)
        .filter(|candidate| piece_at(pos, candidate.from).is_some_and(|piece| piece.kind == kind))
        .collect::<Vec<_>>();

    if competing.is_empty() {
        return String::new();
    }

    let same_file = competing
        .iter()
        .any(|candidate| candidate.from.file() == mv.from.file());
    let same_rank = competing
        .iter()
        .any(|candidate| candidate.from.rank() == mv.from.rank());
    match (same_file, same_rank) {
        (false, _) => file_char(mv.from.file()).to_string(),
        (true, false) => rank_char(mv.from.rank()).to_string(),
        (true, true) => format!("{}{}", file_char(mv.from.file()), rank_char(mv.from.rank())),
    }
}

fn is_capture(mv: Move) -> bool {
    matches!(
        mv.kind,
        MoveKind::Capture | MoveKind::EnPassant | MoveKind::PromotionCapture(_)
    )
}

fn promotion_piece(mv: Move) -> Option<PieceKind> {
    match mv.kind {
        MoveKind::Promotion(kind) | MoveKind::PromotionCapture(kind) => Some(kind),
        _ => None,
    }
}

fn piece_letter(kind: PieceKind) -> char {
    match kind {
        PieceKind::Knight => 'N',
        PieceKind::Bishop => 'B',
        PieceKind::Rook => 'R',
        PieceKind::Queen => 'Q',
        PieceKind::King => 'K',
        PieceKind::Pawn => 'P',
    }
}

fn relative_rank(square: Square, color: Color) -> i32 {
    match color {
        Color::White => i32::from(square.rank()),
        Color::Black => i32::from(7 - square.rank()),
    }
}

fn file_char(file: u8) -> char {
    char::from(b'a' + file)
}

fn rank_char(rank: u8) -> char {
    char::from(b'1' + rank)
}

fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "white",
        Color::Black => "black",
    }
}

fn render_pgn(report: &BaselineTournamentReport) -> String {
    let mut pgn = String::new();
    for game in &report.games {
        pgn.push_str(&format!(
            "[Event \"Kensho deterministic baseline tournament\"]\n\
             [Site \"Local\"]\n\
             [Date \"????.??.??\"]\n\
             [Round \"{}\"]\n\
             [White \"{}\"]\n\
             [Black \"{}\"]\n\
             [Result \"{}\"]\n\
             [Baseline \"{}\"]\n\
             [EngineDepth \"{}\"]\n\
             [MaxPlies \"{}\"]\n\
             [Termination \"{}\"]\n\n",
            game.game_number,
            player_name(
                Color::White,
                game.engine_color,
                &game.baseline,
                report.config.engine_depth
            ),
            player_name(
                Color::Black,
                game.engine_color,
                &game.baseline,
                report.config.engine_depth
            ),
            game.result_token(),
            game.baseline,
            report.config.engine_depth,
            report.config.max_plies,
            game.reason
        ));
        pgn.push_str(&render_movetext(&game.san_moves, game.result_token()));
        pgn.push_str("\n\n");
    }
    pgn
}

fn player_name(side: Color, engine_color: Color, baseline: &str, depth: u8) -> String {
    if side == engine_color {
        format!("{ENGINE_NAME} depth {depth}")
    } else {
        format!("deterministic baseline {baseline}")
    }
}

fn render_movetext(san_moves: &[String], result: &str) -> String {
    let mut movetext = String::new();
    for (index, san) in san_moves.iter().enumerate() {
        if index % 2 == 0 {
            if !movetext.is_empty() {
                movetext.push(' ');
            }
            movetext.push_str(&format!("{}. {san}", index / 2 + 1));
        } else {
            movetext.push(' ');
            movetext.push_str(san);
        }
    }
    if !movetext.is_empty() {
        movetext.push(' ');
    }
    movetext.push_str(result);
    movetext
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_contains_name_and_version() {
        let version = version_line();

        assert!(version.contains(ENGINE_NAME));
        assert!(version.contains(ENGINE_VERSION));
    }

    #[test]
    fn baseline_tournament_writes_summary_and_pgn() {
        let output_dir =
            std::env::temp_dir().join(format!("kensho-baseline-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&output_dir);

        let report = run_baseline_tournament(BaselineTournamentConfig {
            games: 2,
            max_plies: 6,
            engine_depth: 1,
            output_dir: output_dir.clone(),
        })
        .unwrap();

        assert_eq!(report.games.len(), 2);
        assert!(report.summary_path.exists());
        assert!(report.pgn_path.exists());
        assert!(
            std::fs::read_to_string(&report.summary_path)
                .unwrap()
                .contains("release_strength_claim: not-claimed")
        );
        assert!(
            std::fs::read_to_string(&report.pgn_path)
                .unwrap()
                .contains("[Event \"Kensho deterministic baseline tournament\"]")
        );

        let _ = std::fs::remove_dir_all(output_dir);
    }
}
