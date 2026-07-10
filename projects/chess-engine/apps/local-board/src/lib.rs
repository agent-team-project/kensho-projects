#![doc = "Local desktop board application and UCI subprocess adapter."]

use chess_core::{
    Color, FenError, Move, MoveError, MoveKind, MoveList, Piece, PieceKind, Position, Square,
    legal_moves, make_move, parse_fen, piece_at, startpos, to_fen,
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Vec2};
use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

pub const APP_NAME: &str = "Kensho Local Board";
const ENGINE_SEARCH_DEPTH: u8 = 3;
const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);

#[must_use]
pub const fn app_name() -> &'static str {
    APP_NAME
}

#[must_use]
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest_dir)
        .to_path_buf()
}

#[must_use]
pub fn default_engine_path() -> PathBuf {
    let mut path = workspace_root()
        .join("target")
        .join("debug")
        .join("chess-engine");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoardOrientation {
    #[default]
    WhiteAtBottom,
    BlackAtBottom,
}

impl BoardOrientation {
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::WhiteAtBottom => Self::BlackAtBottom,
            Self::BlackAtBottom => Self::WhiteAtBottom,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlayMode {
    #[default]
    White,
    Black,
    Both,
}

impl PlayMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Black => "Black",
            Self::Both => "Both",
        }
    }

    #[must_use]
    pub const fn human_can_move(self, side: Color) -> bool {
        match self {
            Self::White => matches!(side, Color::White),
            Self::Black => matches!(side, Color::Black),
            Self::Both => true,
        }
    }

    #[must_use]
    pub const fn engine_side(self) -> Option<Color> {
        match self {
            Self::White => Some(Color::Black),
            Self::Black => Some(Color::White),
            Self::Both => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BasePosition {
    Startpos,
    Fen(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingPromotion {
    pub from: Square,
    pub to: Square,
    pub choices: Vec<PieceKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MoveInputOutcome {
    Selected(Square),
    Cleared,
    Applied(String),
    NeedsPromotion(PendingPromotion),
    Rejected(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardState {
    position: Position,
    base: BasePosition,
    positions: Vec<Position>,
    moves: Vec<String>,
    selected: Option<Square>,
    legal_targets: Vec<Square>,
    pending_promotion: Option<PendingPromotion>,
    play_mode: PlayMode,
    orientation: BoardOrientation,
    game_message: Option<String>,
    last_error: Option<String>,
}

impl Default for BoardState {
    fn default() -> Self {
        Self::new()
    }
}

impl BoardState {
    #[must_use]
    pub fn new() -> Self {
        let position = startpos();
        Self {
            position: position.clone(),
            base: BasePosition::Startpos,
            positions: vec![position],
            moves: Vec::new(),
            selected: None,
            legal_targets: Vec::new(),
            pending_promotion: None,
            play_mode: PlayMode::default(),
            orientation: BoardOrientation::default(),
            game_message: None,
            last_error: None,
        }
    }

    #[must_use]
    pub const fn position(&self) -> &Position {
        &self.position
    }

    #[must_use]
    pub fn fen(&self) -> String {
        to_fen(&self.position)
    }

    #[must_use]
    pub fn moves(&self) -> &[String] {
        &self.moves
    }

    #[must_use]
    pub const fn selected(&self) -> Option<Square> {
        self.selected
    }

    #[must_use]
    pub fn legal_targets(&self) -> &[Square] {
        &self.legal_targets
    }

    #[must_use]
    pub const fn pending_promotion(&self) -> Option<&PendingPromotion> {
        self.pending_promotion.as_ref()
    }

    #[must_use]
    pub const fn play_mode(&self) -> PlayMode {
        self.play_mode
    }

    pub fn set_play_mode(&mut self, mode: PlayMode) {
        self.play_mode = mode;
        if !self.is_human_turn() {
            self.clear_input();
        }
    }

    #[must_use]
    pub const fn orientation(&self) -> BoardOrientation {
        self.orientation
    }

    pub fn flip_board(&mut self) {
        self.orientation = self.orientation.flipped();
    }

    #[must_use]
    pub fn game_message(&self) -> Option<&str> {
        self.game_message.as_deref()
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[must_use]
    pub fn is_human_turn(&self) -> bool {
        self.play_mode.human_can_move(self.position.side_to_move)
    }

    #[must_use]
    pub fn is_engine_turn(&self) -> bool {
        self.play_mode.engine_side() == Some(self.position.side_to_move)
            && self.game_message.is_none()
    }

    pub fn new_game(&mut self) {
        let position = startpos();
        self.position = position.clone();
        self.base = BasePosition::Startpos;
        self.positions = vec![position];
        self.moves.clear();
        self.clear_input();
        self.game_message = None;
        self.last_error = None;
    }

    pub fn load_fen(&mut self, fen: &str) -> Result<(), FenError> {
        let position = parse_fen(fen)?;
        self.position = position.clone();
        self.base = BasePosition::Fen(fen.to_owned());
        self.positions = vec![position];
        self.moves.clear();
        self.clear_input();
        self.game_message = None;
        self.last_error = None;
        Ok(())
    }

    pub fn reset(&mut self) {
        self.new_game();
    }

    pub fn resign_or_reset(&mut self) {
        if self.game_message.is_some() {
            self.reset();
        } else {
            self.game_message = Some(format!("{:?} resigned", self.position.side_to_move));
            self.clear_input();
        }
    }

    pub fn undo(&mut self) -> bool {
        if self.moves.is_empty() || self.positions.len() <= 1 {
            self.last_error = Some("No move to undo".to_owned());
            return false;
        }
        self.moves.pop();
        self.positions.pop();
        self.position = self
            .positions
            .last()
            .expect("positions always keep a base position")
            .clone();
        self.clear_input();
        self.game_message = None;
        self.last_error = None;
        true
    }

    pub fn click_square(&mut self, square: Square) -> MoveInputOutcome {
        if self.pending_promotion.is_some() {
            return MoveInputOutcome::Rejected("Choose a promotion piece first".to_owned());
        }
        if !self.is_human_turn() {
            self.last_error = Some("Waiting for engine move".to_owned());
            return MoveInputOutcome::Rejected("Waiting for engine move".to_owned());
        }

        if let Some(selected) = self.selected {
            if selected == square {
                self.clear_input();
                return MoveInputOutcome::Cleared;
            }
            let outcome = self.try_move(selected, square, None);
            if !matches!(outcome, MoveInputOutcome::Rejected(_)) {
                return outcome;
            }
            if self.is_selectable_piece(square) {
                return self.select_square(square);
            }
            return outcome;
        }

        if self.is_selectable_piece(square) {
            self.select_square(square)
        } else {
            let message = "No movable piece on that square".to_owned();
            self.last_error = Some(message.clone());
            MoveInputOutcome::Rejected(message)
        }
    }

    pub fn choose_promotion(&mut self, kind: PieceKind) -> MoveInputOutcome {
        let Some(pending) = self.pending_promotion.take() else {
            let message = "No promotion is pending".to_owned();
            self.last_error = Some(message.clone());
            return MoveInputOutcome::Rejected(message);
        };
        self.try_move(pending.from, pending.to, Some(kind))
    }

    pub fn try_move(
        &mut self,
        from: Square,
        to: Square,
        promotion: Option<PieceKind>,
    ) -> MoveInputOutcome {
        let candidates = self.legal_candidates(from, to);
        if candidates.is_empty() {
            let message = format!("Illegal move {from}{to}");
            self.last_error = Some(message.clone());
            return MoveInputOutcome::Rejected(message);
        }

        if let Some(kind) = promotion {
            if let Some(mv) = candidates
                .into_iter()
                .find(|mv| promotion_piece(*mv) == Some(kind))
            {
                return self.apply_move(mv);
            }
            let message = format!("{kind:?} is not a legal promotion choice");
            self.last_error = Some(message.clone());
            return MoveInputOutcome::Rejected(message);
        }

        let promotion_choices = promotion_choices(&candidates);
        if !promotion_choices.is_empty() {
            let pending = PendingPromotion {
                from,
                to,
                choices: promotion_choices,
            };
            self.pending_promotion = Some(pending.clone());
            self.selected = Some(from);
            self.legal_targets = vec![to];
            return MoveInputOutcome::NeedsPromotion(pending);
        }

        self.apply_move(candidates[0])
    }

    pub fn apply_engine_move_text(&mut self, move_text: &str) -> MoveInputOutcome {
        if move_text == "0000" {
            let message = "Engine reported no legal move".to_owned();
            self.game_message = Some(message.clone());
            return MoveInputOutcome::Rejected(message);
        }
        let mut moves = MoveList::new();
        legal_moves(&self.position, &mut moves);
        let Some(mv) = moves
            .iter()
            .copied()
            .find(|candidate| candidate.to_string() == move_text)
        else {
            let message = format!("Engine returned illegal move {move_text}");
            self.last_error = Some(message.clone());
            return MoveInputOutcome::Rejected(message);
        };
        self.apply_move(mv)
    }

    #[must_use]
    pub fn position_command(&self) -> String {
        let mut command = match &self.base {
            BasePosition::Startpos => "position startpos".to_owned(),
            BasePosition::Fen(fen) => format!("position fen {fen}"),
        };
        if !self.moves.is_empty() {
            command.push_str(" moves ");
            command.push_str(&self.moves.join(" "));
        }
        command
    }

    #[must_use]
    pub fn square_at_view_cell(
        file_index: usize,
        rank_index: usize,
        orientation: BoardOrientation,
    ) -> Option<Square> {
        if file_index >= 8 || rank_index >= 8 {
            return None;
        }
        let file = file_index as u8;
        let rank = rank_index as u8;
        match orientation {
            BoardOrientation::WhiteAtBottom => Square::from_file_rank(file, 7 - rank),
            BoardOrientation::BlackAtBottom => Square::from_file_rank(7 - file, rank),
        }
    }

    #[must_use]
    pub fn view_cell_for_square(square: Square, orientation: BoardOrientation) -> (usize, usize) {
        match orientation {
            BoardOrientation::WhiteAtBottom => {
                (square.file() as usize, (7 - square.rank()) as usize)
            }
            BoardOrientation::BlackAtBottom => {
                ((7 - square.file()) as usize, square.rank() as usize)
            }
        }
    }

    fn select_square(&mut self, square: Square) -> MoveInputOutcome {
        self.selected = Some(square);
        self.legal_targets = self.legal_targets_from(square);
        self.last_error = None;
        MoveInputOutcome::Selected(square)
    }

    fn is_selectable_piece(&self, square: Square) -> bool {
        piece_at(&self.position, square)
            .is_some_and(|piece| piece.color == self.position.side_to_move)
    }

    fn legal_targets_from(&self, square: Square) -> Vec<Square> {
        let mut targets = BTreeSet::new();
        let mut moves = MoveList::new();
        legal_moves(&self.position, &mut moves);
        for mv in moves.iter().copied().filter(|mv| mv.from == square) {
            targets.insert(mv.to);
        }
        targets.into_iter().collect()
    }

    fn legal_candidates(&self, from: Square, to: Square) -> Vec<Move> {
        let mut moves = MoveList::new();
        legal_moves(&self.position, &mut moves);
        moves
            .iter()
            .copied()
            .filter(|mv| mv.from == from && mv.to == to)
            .collect()
    }

    fn apply_move(&mut self, mv: Move) -> MoveInputOutcome {
        match make_move(&mut self.position, mv) {
            Ok(_) => {
                let uci = mv.to_string();
                self.moves.push(uci.clone());
                self.positions.push(self.position.clone());
                self.clear_input();
                self.last_error = None;
                self.update_terminal_message();
                MoveInputOutcome::Applied(uci)
            }
            Err(err) => {
                let message = err.to_string();
                self.last_error = Some(message.clone());
                MoveInputOutcome::Rejected(message)
            }
        }
    }

    fn update_terminal_message(&mut self) {
        let mut moves = MoveList::new();
        legal_moves(&self.position, &mut moves);
        if !moves.is_empty() {
            self.game_message = None;
            return;
        }
        self.game_message = Some(format!(
            "{:?} has no legal moves",
            self.position.side_to_move
        ));
    }

    fn clear_input(&mut self) {
        self.selected = None;
        self.legal_targets.clear();
        self.pending_promotion = None;
    }
}

fn promotion_piece(mv: Move) -> Option<PieceKind> {
    match mv.kind {
        MoveKind::Promotion(kind) | MoveKind::PromotionCapture(kind) => Some(kind),
        _ => None,
    }
}

fn promotion_choices(moves: &[Move]) -> Vec<PieceKind> {
    let mut choices = Vec::new();
    for mv in moves {
        if let Some(kind) = promotion_piece(*mv)
            && !choices.contains(&kind)
        {
            choices.push(kind);
        }
    }
    choices
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UciScore {
    Centipawns(i32),
    Mate(i32),
}

impl fmt::Display for UciScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Centipawns(score) => write!(f, "{score} cp"),
            Self::Mate(moves) => write!(f, "mate {moves}"),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UciInfo {
    pub depth: Option<u8>,
    pub seldepth: Option<u8>,
    pub score: Option<UciScore>,
    pub nodes: Option<u64>,
    pub nps: Option<u64>,
    pub pv: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedUciLine {
    Id(String),
    Option(String),
    UciOk,
    ReadyOk,
    Info(UciInfo),
    BestMove {
        bestmove: Option<String>,
        ponder: Option<String>,
    },
    Message(String),
    Unknown(String),
}

#[must_use]
pub fn parse_uci_output_line(line: &str) -> ParsedUciLine {
    let trimmed = line.trim();
    if trimmed == "uciok" {
        return ParsedUciLine::UciOk;
    }
    if trimmed == "readyok" {
        return ParsedUciLine::ReadyOk;
    }
    if let Some(rest) = trimmed.strip_prefix("id ") {
        return ParsedUciLine::Id(rest.to_owned());
    }
    if let Some(rest) = trimmed.strip_prefix("option ") {
        return ParsedUciLine::Option(rest.to_owned());
    }
    if let Some(rest) = trimmed.strip_prefix("info string ") {
        return ParsedUciLine::Message(rest.to_owned());
    }
    if trimmed.starts_with("info ") {
        return ParsedUciLine::Info(parse_info_line(trimmed));
    }
    if trimmed.starts_with("bestmove ") {
        return parse_bestmove_line(trimmed);
    }
    ParsedUciLine::Unknown(trimmed.to_owned())
}

fn parse_info_line(line: &str) -> UciInfo {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut info = UciInfo::default();
    let mut index = 1;
    while let Some(token) = tokens.get(index).copied() {
        match token {
            "depth" => {
                info.depth = parse_token(tokens.get(index + 1).copied());
                index += 2;
            }
            "seldepth" => {
                info.seldepth = parse_token(tokens.get(index + 1).copied());
                index += 2;
            }
            "nodes" => {
                info.nodes = parse_token(tokens.get(index + 1).copied());
                index += 2;
            }
            "nps" => {
                info.nps = parse_token(tokens.get(index + 1).copied());
                index += 2;
            }
            "score" => {
                let kind = tokens.get(index + 1).copied();
                let value = tokens.get(index + 2).copied();
                info.score = match (kind, parse_token::<i32>(value)) {
                    (Some("cp"), Some(score)) => Some(UciScore::Centipawns(score)),
                    (Some("mate"), Some(score)) => Some(UciScore::Mate(score)),
                    _ => None,
                };
                index += 3;
            }
            "pv" => {
                info.pv = tokens[index + 1..]
                    .iter()
                    .map(|token| (*token).to_owned())
                    .collect();
                break;
            }
            _ => {
                index += 1;
            }
        }
    }
    info
}

fn parse_token<T: std::str::FromStr>(token: Option<&str>) -> Option<T> {
    token.and_then(|token| token.parse::<T>().ok())
}

fn parse_bestmove_line(line: &str) -> ParsedUciLine {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let bestmove = tokens
        .get(1)
        .copied()
        .filter(|move_text| *move_text != "0000")
        .map(str::to_owned);
    let ponder = tokens
        .windows(2)
        .find(|window| window[0] == "ponder")
        .map(|window| window[1].to_owned());
    ParsedUciLine::BestMove { bestmove, ponder }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EngineStatus {
    pub connected: bool,
    pub searching: bool,
    pub depth: Option<u8>,
    pub score: Option<UciScore>,
    pub nodes: Option<u64>,
    pub pv: Vec<String>,
    pub bestmove: Option<String>,
    pub last_error: Option<String>,
}

impl EngineStatus {
    pub fn apply_line(&mut self, line: &str) -> ParsedUciLine {
        let parsed = parse_uci_output_line(line);
        self.apply_parsed(&parsed);
        parsed
    }

    pub fn apply_parsed(&mut self, parsed: &ParsedUciLine) {
        match parsed {
            ParsedUciLine::UciOk | ParsedUciLine::ReadyOk => {
                self.connected = true;
                self.last_error = None;
            }
            ParsedUciLine::Info(info) => {
                if let Some(depth) = info.depth {
                    self.depth = Some(depth);
                }
                if let Some(score) = info.score {
                    self.score = Some(score);
                }
                if let Some(nodes) = info.nodes {
                    self.nodes = Some(nodes);
                }
                if !info.pv.is_empty() {
                    self.pv = info.pv.clone();
                }
            }
            ParsedUciLine::BestMove { bestmove, .. } => {
                self.searching = false;
                self.bestmove = bestmove.clone();
            }
            ParsedUciLine::Message(message) => {
                self.last_error = Some(message.clone());
            }
            ParsedUciLine::Id(_) | ParsedUciLine::Option(_) | ParsedUciLine::Unknown(_) => {}
        }
    }

    pub fn mark_search_started(&mut self) {
        self.searching = true;
        self.bestmove = None;
        self.last_error = None;
    }

    pub fn mark_disconnected(&mut self, message: String) {
        self.connected = false;
        self.searching = false;
        self.last_error = Some(message);
    }
}

#[derive(Debug)]
pub enum UciClientError {
    Spawn { path: PathBuf, source: io::Error },
    NotRunning,
    Io(io::Error),
}

impl fmt::Display for UciClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { path, source } => {
                write!(f, "failed to start engine '{}': {source}", path.display())
            }
            Self::NotRunning => f.write_str("UCI engine is not running"),
            Self::Io(err) => write!(f, "UCI engine I/O failed: {err}"),
        }
    }
}

impl Error for UciClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn { source, .. } | Self::Io(source) => Some(source),
            Self::NotRunning => None,
        }
    }
}

impl From<io::Error> for UciClientError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub enum UciEngineEvent {
    Stdout(String),
    Stderr(String),
    StdoutClosed,
    Exited(ExitStatus),
}

#[derive(Debug)]
pub struct UciClient {
    engine_path: PathBuf,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    events: Receiver<UciEngineEvent>,
}

impl UciClient {
    #[must_use]
    pub fn new(engine_path: PathBuf) -> Self {
        let (_tx, events) = mpsc::channel();
        Self {
            engine_path,
            child: None,
            stdin: None,
            events,
        }
    }

    #[must_use]
    pub fn engine_path(&self) -> &Path {
        &self.engine_path
    }

    pub fn set_engine_path(&mut self, engine_path: PathBuf) {
        if self.engine_path != engine_path {
            self.stop();
            self.engine_path = engine_path;
        }
    }

    #[must_use]
    pub fn is_running(&mut self) -> bool {
        self.poll_events();
        self.child.is_some()
    }

    pub fn start(&mut self) -> Result<(), UciClientError> {
        self.stop();
        let (tx, events) = mpsc::channel();
        let mut child = Command::new(&self.engine_path)
            .arg("uci")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| UciClientError::Spawn {
                path: self.engine_path.clone(),
                source,
            })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            UciClientError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "engine stdout unavailable",
            ))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            UciClientError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "engine stderr unavailable",
            ))
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            UciClientError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "engine stdin unavailable",
            ))
        })?;

        let stdout_tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if stdout_tx.send(UciEngineEvent::Stdout(line)).is_err() {
                            return;
                        }
                    }
                    Err(err) => {
                        let _ = stdout_tx
                            .send(UciEngineEvent::Stderr(format!("stdout read failed: {err}")));
                        return;
                    }
                }
            }
            let _ = stdout_tx.send(UciEngineEvent::StdoutClosed);
        });

        let stderr_tx = tx;
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) => {
                        if !line.trim().is_empty()
                            && stderr_tx.send(UciEngineEvent::Stderr(line)).is_err()
                        {
                            return;
                        }
                    }
                    Err(err) => {
                        let _ = stderr_tx
                            .send(UciEngineEvent::Stderr(format!("stderr read failed: {err}")));
                        return;
                    }
                }
            }
        });

        self.events = events;
        self.stdin = Some(stdin);
        self.child = Some(child);
        Ok(())
    }

    pub fn send_line(&mut self, line: &str) -> Result<(), UciClientError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(UciClientError::NotRunning);
        };
        writeln!(stdin, "{line}")?;
        stdin.flush()?;
        Ok(())
    }

    pub fn poll_events(&mut self) -> Vec<UciEngineEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child = None;
                    self.stdin = None;
                    events.push(UciEngineEvent::Exited(status));
                }
                Ok(None) => {}
                Err(err) => {
                    events.push(UciEngineEvent::Stderr(format!("engine wait failed: {err}")));
                }
            }
        }
        events
    }

    pub fn stop(&mut self) {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = writeln!(stdin, "quit");
            let _ = stdin.flush();
        }
        self.stdin = None;
        if let Some(mut child) = self.child.take() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }
}

impl Drop for UciClient {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
pub struct LocalBoardApp {
    board: BoardState,
    engine: UciClient,
    engine_path_input: String,
    status: EngineStatus,
    last_engine_start: Option<Instant>,
}

impl Default for LocalBoardApp {
    fn default() -> Self {
        let engine_path = default_engine_path();
        Self {
            board: BoardState::new(),
            engine: UciClient::new(engine_path.clone()),
            engine_path_input: engine_path.display().to_string(),
            status: EngineStatus::default(),
            last_engine_start: None,
        }
    }
}

impl eframe::App for LocalBoardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_engine();
        self.request_engine_move_if_needed();
        if self.status.searching || self.board.is_engine_turn() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("controls").show(ui, |ui| {
            self.show_controls(ui);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.show_board(ui);
        });
        self.show_promotion_picker(ui.ctx());
    }
}

impl LocalBoardApp {
    fn show_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                self.board.new_game();
                self.send_position_to_engine();
            }
            if ui.button("Flip").clicked() {
                self.board.flip_board();
            }
            if ui.button("Undo").clicked() && self.board.undo() {
                self.send_position_to_engine();
            }
            if ui.button("Reset/Resign").clicked() {
                self.board.resign_or_reset();
                self.send_position_to_engine();
            }

            egui::ComboBox::from_id_salt("play_mode")
                .selected_text(self.board.play_mode().label())
                .show_ui(ui, |ui| {
                    for mode in [PlayMode::White, PlayMode::Black, PlayMode::Both] {
                        if ui
                            .selectable_value(&mut self.board.play_mode, mode, mode.label())
                            .clicked()
                        {
                            self.board.set_play_mode(mode);
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Engine");
            ui.text_edit_singleline(&mut self.engine_path_input);
            if ui.button("Start").clicked() {
                self.restart_engine();
            }
            if self.status.searching
                && ui.button("Stop").clicked()
                && let Err(err) = self.engine.send_line("stop")
            {
                self.status.mark_disconnected(err.to_string());
            }
        });

        ui.horizontal_wrapped(|ui| {
            let state = if self.status.searching {
                "searching"
            } else if self.status.connected {
                "idle"
            } else {
                "offline"
            };
            ui.label(format!("Engine: {state}"));
            if let Some(depth) = self.status.depth {
                ui.label(format!("depth {depth}"));
            }
            if let Some(score) = self.status.score {
                ui.label(format!("score {score}"));
            }
            if let Some(nodes) = self.status.nodes {
                ui.label(format!("nodes {nodes}"));
            }
            if !self.status.pv.is_empty() {
                ui.label(format!("pv {}", self.status.pv.join(" ")));
            }
        });

        if let Some(message) = self.board.game_message() {
            ui.label(message);
        }
        if let Some(error) = self
            .board
            .last_error()
            .or(self.status.last_error.as_deref())
        {
            ui.colored_label(Color32::from_rgb(180, 36, 36), error);
        }
    }

    fn show_board(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let board_size = available.x.min(available.y).clamp(280.0, 720.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(board_size), Sense::click());
        let painter = ui.painter_at(rect);
        let tile = board_size / 8.0;

        for rank_index in 0..8 {
            for file_index in 0..8 {
                let Some(square) = BoardState::square_at_view_cell(
                    file_index,
                    rank_index,
                    self.board.orientation(),
                ) else {
                    continue;
                };
                let min = Pos2::new(
                    rect.left() + file_index as f32 * tile,
                    rect.top() + rank_index as f32 * tile,
                );
                let square_rect = Rect::from_min_size(min, Vec2::splat(tile));
                let is_light = (square.file() + square.rank()) % 2 == 0;
                let base = if is_light {
                    Color32::from_rgb(232, 218, 185)
                } else {
                    Color32::from_rgb(110, 143, 95)
                };
                painter.rect_filled(square_rect, 0.0, base);

                if self.board.selected() == Some(square) {
                    painter.rect_filled(
                        square_rect.shrink(tile * 0.08),
                        3.0,
                        Color32::from_rgba_unmultiplied(236, 202, 70, 95),
                    );
                } else if self.board.legal_targets().contains(&square) {
                    painter.circle_filled(
                        square_rect.center(),
                        tile * 0.15,
                        Color32::from_rgba_unmultiplied(45, 80, 45, 120),
                    );
                }

                if let Some(piece) = piece_at(self.board.position(), square) {
                    painter.text(
                        square_rect.center(),
                        Align2::CENTER_CENTER,
                        piece_symbol(piece),
                        FontId::proportional(tile * 0.72),
                        piece_color(piece.color),
                    );
                }
            }
        }

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && rect.contains(pos)
        {
            let file = ((pos.x - rect.left()) / tile).floor() as usize;
            let rank = ((pos.y - rect.top()) / tile).floor() as usize;
            if let Some(square) =
                BoardState::square_at_view_cell(file, rank, self.board.orientation())
            {
                let outcome = self.board.click_square(square);
                if matches!(outcome, MoveInputOutcome::Applied(_)) {
                    self.send_position_to_engine();
                }
            }
        }
    }

    fn show_promotion_picker(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.board.pending_promotion().cloned() else {
            return;
        };
        egui::Window::new("Promotion")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for kind in pending.choices {
                        let piece = Piece {
                            color: self.board.position().side_to_move,
                            kind,
                        };
                        if ui
                            .button(egui::RichText::new(piece_symbol(piece)).size(32.0))
                            .clicked()
                            && matches!(
                                self.board.choose_promotion(kind),
                                MoveInputOutcome::Applied(_)
                            )
                        {
                            self.send_position_to_engine();
                        }
                    }
                });
            });
    }

    fn poll_engine(&mut self) {
        for event in self.engine.poll_events() {
            match event {
                UciEngineEvent::Stdout(line) => {
                    let parsed = self.status.apply_line(&line);
                    if let ParsedUciLine::BestMove {
                        bestmove: Some(bestmove),
                        ..
                    } = parsed
                        && self.board.is_engine_turn()
                    {
                        self.board.apply_engine_move_text(&bestmove);
                    }
                }
                UciEngineEvent::Stderr(line) => {
                    self.status.mark_disconnected(line);
                }
                UciEngineEvent::StdoutClosed => {}
                UciEngineEvent::Exited(status) => {
                    self.status
                        .mark_disconnected(format!("Engine exited with {status}"));
                }
            }
        }
    }

    fn request_engine_move_if_needed(&mut self) {
        if !self.board.is_engine_turn() || self.status.searching {
            return;
        }
        if !self.ensure_engine_ready() {
            return;
        }
        if self.send_position_to_engine() {
            match self
                .engine
                .send_line(&format!("go depth {ENGINE_SEARCH_DEPTH}"))
            {
                Ok(()) => self.status.mark_search_started(),
                Err(err) => self.status.mark_disconnected(err.to_string()),
            }
        }
    }

    fn ensure_engine_ready(&mut self) -> bool {
        if self.engine.is_running() {
            return true;
        }
        if self
            .last_engine_start
            .is_some_and(|started| started.elapsed() < Duration::from_secs(1))
        {
            return false;
        }
        self.restart_engine()
    }

    fn restart_engine(&mut self) -> bool {
        let path = PathBuf::from(self.engine_path_input.trim());
        self.engine.set_engine_path(path);
        self.last_engine_start = Some(Instant::now());
        match self.engine.start() {
            Ok(()) => {
                self.status = EngineStatus::default();
                if let Err(err) = self.engine.send_line("uci") {
                    self.status.mark_disconnected(err.to_string());
                    return false;
                }
                if let Err(err) = self.engine.send_line("isready") {
                    self.status.mark_disconnected(err.to_string());
                    return false;
                }
                true
            }
            Err(err) => {
                self.status.mark_disconnected(err.to_string());
                false
            }
        }
    }

    fn send_position_to_engine(&mut self) -> bool {
        if !self.engine.is_running() && !self.ensure_engine_ready() {
            return false;
        }
        match self.engine.send_line(&self.board.position_command()) {
            Ok(()) => true,
            Err(err) => {
                self.status.mark_disconnected(err.to_string());
                false
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmokeOptions {
    pub engine_path: PathBuf,
}

impl Default for SmokeOptions {
    fn default() -> Self {
        Self {
            engine_path: default_engine_path(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmokeReport {
    pub checks: Vec<String>,
    pub bestmove: String,
}

#[derive(Debug)]
pub enum SmokeError {
    Io(io::Error),
    Fen(FenError),
    Move(MoveError),
    Uci(UciClientError),
    MissingEngine(PathBuf),
    EngineBuildFailed(ExitStatus),
    Timeout(&'static str, Duration),
    EngineExited(String),
    Unexpected(String),
}

impl fmt::Display for SmokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "GUI smoke I/O failed: {err}"),
            Self::Fen(err) => write!(f, "GUI smoke FEN setup failed: {err}"),
            Self::Move(err) => write!(f, "GUI smoke move setup failed: {err}"),
            Self::Uci(err) => write!(f, "GUI smoke UCI failed: {err}"),
            Self::MissingEngine(path) => {
                write!(f, "engine binary does not exist at '{}'", path.display())
            }
            Self::EngineBuildFailed(status) => {
                write!(f, "cargo build -p chess-engine failed with {status}")
            }
            Self::Timeout(label, timeout) => {
                write!(f, "timed out waiting for {label} after {timeout:?}")
            }
            Self::EngineExited(message) => write!(f, "engine exited during smoke: {message}"),
            Self::Unexpected(message) => f.write_str(message),
        }
    }
}

impl Error for SmokeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Fen(err) => Some(err),
            Self::Move(err) => Some(err),
            Self::Uci(err) => Some(err),
            Self::MissingEngine(_)
            | Self::EngineBuildFailed(_)
            | Self::Timeout(_, _)
            | Self::EngineExited(_)
            | Self::Unexpected(_) => None,
        }
    }
}

impl From<io::Error> for SmokeError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FenError> for SmokeError {
    fn from(value: FenError) -> Self {
        Self::Fen(value)
    }
}

impl From<MoveError> for SmokeError {
    fn from(value: MoveError) -> Self {
        Self::Move(value)
    }
}

impl From<UciClientError> for SmokeError {
    fn from(value: UciClientError) -> Self {
        Self::Uci(value)
    }
}

pub fn run_smoke(options: SmokeOptions) -> Result<SmokeReport, SmokeError> {
    let engine_path = absolute_engine_path(&options.engine_path)?;
    ensure_smoke_engine(&engine_path)?;

    let mut checks = Vec::new();
    let mut board = BoardState::new();
    if !matches!(
        board.click_square(square("e2")),
        MoveInputOutcome::Selected(selected)
            if selected == square("e2")
    ) {
        return Err(SmokeError::Unexpected(
            "clicking e2 did not select the pawn".to_owned(),
        ));
    }
    expect_applied(board.click_square(square("e4")), "play e2e4")?;
    checks.push("legal mouse move e2e4 applied".to_owned());

    board.new_game();
    if !matches!(
        board.try_move(square("e2"), square("e5"), None),
        MoveInputOutcome::Rejected(_)
    ) {
        return Err(SmokeError::Unexpected(
            "illegal move e2e5 was not rejected".to_owned(),
        ));
    }
    checks.push("illegal move e2e5 rejected".to_owned());

    board.load_fen("4k3/P6p/8/8/8/8/7P/4K3 w - - 0 1")?;
    if !matches!(
        board.try_move(square("a7"), square("a8"), None),
        MoveInputOutcome::NeedsPromotion(_)
    ) {
        return Err(SmokeError::Unexpected(
            "promotion move did not enter pending promotion state".to_owned(),
        ));
    }
    expect_applied(board.choose_promotion(PieceKind::Queen), "choose queen")?;
    checks.push("promotion selection applied a7a8q".to_owned());

    board.new_game();
    expect_applied(
        board.try_move(square("e2"), square("e4"), None),
        "play e2e4",
    )?;
    if !board.undo() || !board.moves().is_empty() {
        return Err(SmokeError::Unexpected(
            "undo did not restore startpos".to_owned(),
        ));
    }
    board.resign_or_reset();
    if board.game_message().is_none() {
        return Err(SmokeError::Unexpected(
            "reset/resign did not set a local game message".to_owned(),
        ));
    }
    board.reset();
    checks.push("new game, undo, and reset/resign paths exercised".to_owned());

    let missing = workspace_root()
        .join("target")
        .join("debug")
        .join("__missing_chess_engine__");
    let mut client = UciClient::new(missing);
    if client.start().is_ok() {
        return Err(SmokeError::Unexpected(
            "missing engine path unexpectedly started".to_owned(),
        ));
    }
    client.set_engine_path(engine_path);
    handshake(&mut client)?;
    checks.push("UCI subprocess recovered from failed start and handshook".to_owned());

    let mut status = EngineStatus::default();
    board.new_game();
    expect_applied(
        board.try_move(square("e2"), square("e4"), None),
        "play e2e4",
    )?;
    client.send_line(&board.position_command())?;
    client.send_line("go depth 2")?;
    status.mark_search_started();
    let bestmove = wait_for_bestmove(&mut client, &mut status)?;
    if !matches!(
        board.apply_engine_move_text(&bestmove),
        MoveInputOutcome::Applied(_)
    ) {
        return Err(SmokeError::Unexpected(format!(
            "engine bestmove {bestmove} was not legal in GUI state"
        )));
    }
    if status.depth.is_none() || status.nodes.is_none() {
        return Err(SmokeError::Unexpected(
            "engine status did not include depth and nodes".to_owned(),
        ));
    }
    checks.push("engine bestmove/status parsed and applied".to_owned());

    client.stop();
    handshake(&mut client)?;
    checks.push("engine child restart after stop succeeded".to_owned());
    client.stop();

    Ok(SmokeReport { checks, bestmove })
}

pub fn launch_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|_cc| Ok(Box::<LocalBoardApp>::default())),
    )
}

fn absolute_engine_path(path: &Path) -> Result<PathBuf, io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn ensure_smoke_engine(path: &Path) -> Result<(), SmokeError> {
    if path.exists() {
        return Ok(());
    }

    let status = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("chess-engine")
        .current_dir(workspace_root())
        .status()?;
    if !status.success() {
        return Err(SmokeError::EngineBuildFailed(status));
    }
    if path.exists() {
        Ok(())
    } else {
        Err(SmokeError::MissingEngine(path.to_path_buf()))
    }
}

fn handshake(client: &mut UciClient) -> Result<(), SmokeError> {
    client.start()?;
    client.send_line("uci")?;
    wait_for_match(client, "uciok", |parsed| {
        matches!(parsed, ParsedUciLine::UciOk)
    })?;
    client.send_line("isready")?;
    wait_for_match(client, "readyok", |parsed| {
        matches!(parsed, ParsedUciLine::ReadyOk)
    })?;
    Ok(())
}

fn wait_for_bestmove(
    client: &mut UciClient,
    status: &mut EngineStatus,
) -> Result<String, SmokeError> {
    let deadline = Instant::now() + SMOKE_TIMEOUT;
    while Instant::now() < deadline {
        for event in client.poll_events() {
            match event {
                UciEngineEvent::Stdout(line) => {
                    let parsed = status.apply_line(&line);
                    if let ParsedUciLine::BestMove { bestmove, .. } = parsed {
                        return bestmove.ok_or_else(|| {
                            SmokeError::Unexpected("engine returned bestmove 0000".to_owned())
                        });
                    }
                }
                UciEngineEvent::Stderr(line) => {
                    return Err(SmokeError::EngineExited(line));
                }
                UciEngineEvent::StdoutClosed => {}
                UciEngineEvent::Exited(status) => {
                    return Err(SmokeError::EngineExited(status.to_string()));
                }
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    Err(SmokeError::Timeout("bestmove", SMOKE_TIMEOUT))
}

fn wait_for_match<F>(
    client: &mut UciClient,
    label: &'static str,
    mut predicate: F,
) -> Result<ParsedUciLine, SmokeError>
where
    F: FnMut(&ParsedUciLine) -> bool,
{
    let deadline = Instant::now() + SMOKE_TIMEOUT;
    let mut status = EngineStatus::default();
    while Instant::now() < deadline {
        for event in client.poll_events() {
            match event {
                UciEngineEvent::Stdout(line) => {
                    let parsed = status.apply_line(&line);
                    if predicate(&parsed) {
                        return Ok(parsed);
                    }
                }
                UciEngineEvent::Stderr(line) => {
                    return Err(SmokeError::EngineExited(line));
                }
                UciEngineEvent::StdoutClosed => {}
                UciEngineEvent::Exited(status) => {
                    return Err(SmokeError::EngineExited(status.to_string()));
                }
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    Err(SmokeError::Timeout(label, SMOKE_TIMEOUT))
}

fn expect_applied(outcome: MoveInputOutcome, label: &str) -> Result<(), SmokeError> {
    if matches!(outcome, MoveInputOutcome::Applied(_)) {
        Ok(())
    } else {
        Err(SmokeError::Unexpected(format!(
            "{label} returned {outcome:?}"
        )))
    }
}

fn square(name: &str) -> Square {
    Square::parse_name(name).expect("hard-coded square names are valid")
}

fn piece_symbol(piece: Piece) -> &'static str {
    match (piece.color, piece.kind) {
        (Color::White, PieceKind::King) => "\u{2654}",
        (Color::White, PieceKind::Queen) => "\u{2655}",
        (Color::White, PieceKind::Rook) => "\u{2656}",
        (Color::White, PieceKind::Bishop) => "\u{2657}",
        (Color::White, PieceKind::Knight) => "\u{2658}",
        (Color::White, PieceKind::Pawn) => "\u{2659}",
        (Color::Black, PieceKind::King) => "\u{265A}",
        (Color::Black, PieceKind::Queen) => "\u{265B}",
        (Color::Black, PieceKind::Rook) => "\u{265C}",
        (Color::Black, PieceKind::Bishop) => "\u{265D}",
        (Color::Black, PieceKind::Knight) => "\u{265E}",
        (Color::Black, PieceKind::Pawn) => "\u{265F}",
    }
}

fn piece_color(color: Color) -> Color32 {
    match color {
        Color::White => Color32::from_rgb(248, 248, 238),
        Color::Black => Color32::from_rgb(30, 30, 30),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_app_name() {
        assert_eq!(app_name(), "Kensho Local Board");
    }

    #[test]
    fn coordinate_mapping_respects_orientation() {
        assert_eq!(
            BoardState::square_at_view_cell(0, 0, BoardOrientation::WhiteAtBottom),
            Some(square("a8"))
        );
        assert_eq!(
            BoardState::square_at_view_cell(7, 7, BoardOrientation::WhiteAtBottom),
            Some(square("h1"))
        );
        assert_eq!(
            BoardState::square_at_view_cell(0, 0, BoardOrientation::BlackAtBottom),
            Some(square("h1"))
        );
        assert_eq!(
            BoardState::view_cell_for_square(square("a1"), BoardOrientation::WhiteAtBottom),
            (0, 7)
        );
        assert_eq!(
            BoardState::view_cell_for_square(square("a1"), BoardOrientation::BlackAtBottom),
            (7, 0)
        );
    }

    #[test]
    fn move_input_selects_and_applies_legal_move() {
        let mut board = BoardState::new();

        assert_eq!(
            board.click_square(square("e2")),
            MoveInputOutcome::Selected(square("e2"))
        );
        assert!(board.legal_targets().contains(&square("e4")));
        assert_eq!(
            board.click_square(square("e4")),
            MoveInputOutcome::Applied("e2e4".to_owned())
        );
        assert_eq!(board.moves(), &["e2e4".to_owned()]);
        assert_eq!(board.selected(), None);
    }

    #[test]
    fn illegal_move_is_rejected() {
        let mut board = BoardState::new();

        assert!(matches!(
            board.try_move(square("e2"), square("e5"), None),
            MoveInputOutcome::Rejected(_)
        ));
        assert!(board.moves().is_empty());
    }

    #[test]
    fn promotion_requires_choice_and_applies_choice() {
        let mut board = BoardState::new();
        board.load_fen("4k3/P6p/8/8/8/8/7P/4K3 w - - 0 1").unwrap();

        let outcome = board.try_move(square("a7"), square("a8"), None);
        let MoveInputOutcome::NeedsPromotion(pending) = outcome else {
            panic!("expected promotion state");
        };
        assert_eq!(
            pending.choices,
            vec![
                PieceKind::Queen,
                PieceKind::Rook,
                PieceKind::Bishop,
                PieceKind::Knight
            ]
        );
        assert_eq!(
            board.choose_promotion(PieceKind::Queen),
            MoveInputOutcome::Applied("a7a8q".to_owned())
        );
        assert_eq!(board.moves(), &["a7a8q".to_owned()]);
    }

    #[test]
    fn position_command_tracks_startpos_and_history() {
        let mut board = BoardState::new();

        board.try_move(square("e2"), square("e4"), None);
        board.try_move(square("e7"), square("e5"), None);

        assert_eq!(
            board.position_command(),
            "position startpos moves e2e4 e7e5"
        );
    }

    #[test]
    fn smokeable_app_state_handles_new_undo_and_reset() {
        let mut board = BoardState::new();

        board.try_move(square("e2"), square("e4"), None);
        assert!(board.undo());
        assert!(board.moves().is_empty());
        board.resign_or_reset();
        assert_eq!(board.game_message(), Some("White resigned"));
        board.reset();
        assert!(board.game_message().is_none());
        assert_eq!(board.fen(), to_fen(&startpos()));
    }

    #[test]
    fn uci_output_parses_status_and_bestmove() {
        let parsed = parse_uci_output_line(
            "info depth 4 seldepth 7 score cp -23 nodes 1024 nps 2048 pv e2e4 e7e5",
        );
        let ParsedUciLine::Info(info) = parsed else {
            panic!("expected info line");
        };
        assert_eq!(info.depth, Some(4));
        assert_eq!(info.seldepth, Some(7));
        assert_eq!(info.score, Some(UciScore::Centipawns(-23)));
        assert_eq!(info.nodes, Some(1024));
        assert_eq!(info.nps, Some(2048));
        assert_eq!(info.pv, vec!["e2e4".to_owned(), "e7e5".to_owned()]);

        assert_eq!(
            parse_uci_output_line("bestmove e2e4 ponder e7e5"),
            ParsedUciLine::BestMove {
                bestmove: Some("e2e4".to_owned()),
                ponder: Some("e7e5".to_owned())
            }
        );
    }

    #[test]
    fn engine_status_updates_from_parsed_lines() {
        let mut status = EngineStatus::default();

        status.apply_line("uciok");
        status.mark_search_started();
        status.apply_line("info depth 2 score mate 1 nodes 99 pv e2e4");
        status.apply_line("bestmove e2e4");

        assert!(status.connected);
        assert!(!status.searching);
        assert_eq!(status.depth, Some(2));
        assert_eq!(status.score, Some(UciScore::Mate(1)));
        assert_eq!(status.nodes, Some(99));
        assert_eq!(status.pv, vec!["e2e4".to_owned()]);
        assert_eq!(status.bestmove, Some("e2e4".to_owned()));
    }
}
