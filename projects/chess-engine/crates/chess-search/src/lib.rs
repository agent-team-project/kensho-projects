#![doc = "Deterministic single-thread chess search."]

use chess_core::{
    Color, FenError, Move, MoveKind, MoveList, PieceKind, Position, Square, in_check, legal_moves,
    make_legal_move_unchecked, piece_at, unmake_move,
};
use chess_eval::{DefaultEvaluator, Evaluator, MATE_SCORE_VALUE, Score, material_value};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const INF: i32 = 32_000;
const MAX_PLY: usize = 96;
const MAX_QUIESCENCE_PLY: u8 = 8;
const MATE_THRESHOLD: i32 = MATE_SCORE_VALUE - 1_000;
const DEFAULT_HASH_ENTRIES: usize = 1 << 16;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchLimits {
    pub depth: Option<u8>,
    pub nodes: Option<u64>,
    pub movetime: Option<Duration>,
    pub white_time: Option<Duration>,
    pub black_time: Option<Duration>,
    pub white_increment: Option<Duration>,
    pub black_increment: Option<Duration>,
    pub moves_to_go: Option<u16>,
    pub infinite: bool,
}

impl SearchLimits {
    #[must_use]
    pub const fn fixed_depth(depth: u8) -> Self {
        Self {
            depth: Some(depth),
            nodes: None,
            movetime: None,
            white_time: None,
            black_time: None,
            white_increment: None,
            black_increment: None,
            moves_to_go: None,
            infinite: false,
        }
    }

    #[must_use]
    pub const fn fixed_nodes(nodes: u64) -> Self {
        Self {
            depth: None,
            nodes: Some(nodes),
            movetime: None,
            white_time: None,
            black_time: None,
            white_increment: None,
            black_increment: None,
            moves_to_go: None,
            infinite: false,
        }
    }

    #[must_use]
    pub const fn fixed_movetime(movetime: Duration) -> Self {
        Self {
            depth: None,
            nodes: None,
            movetime: Some(movetime),
            white_time: None,
            black_time: None,
            white_increment: None,
            black_increment: None,
            moves_to_go: None,
            infinite: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchInfo {
    pub depth: u8,
    pub seldepth: u8,
    pub score: Score,
    pub nodes: u64,
    pub nps: u64,
    pub hashfull: Option<u16>,
    pub pv: Vec<Move>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub ponder: Option<Move>,
    pub info: Vec<SearchInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct StopToken {
    stopped: Arc<AtomicBool>,
}

impl StopToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.stopped.store(false, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

pub trait Searcher {
    fn new_game(&mut self);
    fn set_position(&mut self, pos: Position, history: Vec<Move>);
    fn search(&mut self, limits: SearchLimits, stop: StopToken) -> SearchResult;

    fn set_hash_size_mb(&mut self, _mb: usize) {}

    fn set_thread_count(&mut self, _threads: u16) {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TtEntry {
    pub key: u64,
    pub depth: u8,
    pub score: Score,
    pub bound: Bound,
    pub best_move: Option<Move>,
}

#[derive(Clone, Debug)]
pub struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
    used: usize,
}

impl TranspositionTable {
    #[must_use]
    pub fn new(entries: usize) -> Self {
        Self {
            entries: vec![None; entries.max(1)],
            used: 0,
        }
    }

    #[must_use]
    pub fn with_size_mb(mb: usize) -> Self {
        let bytes = mb.max(1).saturating_mul(1024 * 1024);
        let entry_size = std::mem::size_of::<Option<TtEntry>>().max(1);
        Self::new((bytes / entry_size).max(1))
    }

    pub fn clear(&mut self) {
        self.entries.fill(None);
        self.used = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn hashfull(&self) -> u16 {
        ((self.used.min(self.entries.len()) * 1_000) / self.entries.len()) as u16
    }

    #[must_use]
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        let entry = self.entries[self.index(key)]?;
        (entry.key == key).then_some(entry)
    }

    pub fn store(
        &mut self,
        key: u64,
        depth: u8,
        score: Score,
        bound: Bound,
        best_move: Option<Move>,
    ) -> bool {
        let index = self.index(key);
        let entry = TtEntry {
            key,
            depth,
            score,
            bound,
            best_move,
        };

        match self.entries[index] {
            None => {
                self.entries[index] = Some(entry);
                self.used += 1;
                true
            }
            Some(existing) if existing.key == key || depth >= existing.depth => {
                self.entries[index] = Some(entry);
                true
            }
            Some(_) => false,
        }
    }

    fn index(&self, key: u64) -> usize {
        key as usize % self.entries.len()
    }
}

#[derive(Clone, Debug)]
pub struct SingleThreadSearcher<E = DefaultEvaluator> {
    evaluator: E,
    position: Position,
    tt: TranspositionTable,
    killers: [[Option<Move>; 2]; MAX_PLY],
    history: [[i32; 64]; 64],
    move_ordering: bool,
    quiescence: bool,
    nodes: u64,
    seldepth: u8,
}

pub type DefaultSearcher = SingleThreadSearcher<DefaultEvaluator>;

impl Default for DefaultSearcher {
    fn default() -> Self {
        Self::new(DefaultEvaluator)
    }
}

impl<E: Evaluator> SingleThreadSearcher<E> {
    #[must_use]
    pub fn new(evaluator: E) -> Self {
        Self::with_hash_entries(evaluator, DEFAULT_HASH_ENTRIES)
    }

    #[must_use]
    pub fn with_hash_entries(evaluator: E, hash_entries: usize) -> Self {
        Self {
            evaluator,
            position: chess_core::startpos(),
            tt: TranspositionTable::new(hash_entries),
            killers: [[None; 2]; MAX_PLY],
            history: [[0; 64]; 64],
            move_ordering: true,
            quiescence: true,
            nodes: 0,
            seldepth: 0,
        }
    }

    pub fn set_hash_size_mb(&mut self, mb: usize) {
        self.tt = TranspositionTable::with_size_mb(mb);
    }

    pub fn set_move_ordering_enabled(&mut self, enabled: bool) {
        self.move_ordering = enabled;
    }

    pub fn set_quiescence_enabled(&mut self, enabled: bool) {
        self.quiescence = enabled;
    }

    #[must_use]
    pub const fn transposition_table(&self) -> &TranspositionTable {
        &self.tt
    }

    fn reset_search_state(&mut self) {
        self.nodes = 0;
        self.seldepth = 0;
        self.killers = [[None; 2]; MAX_PLY];
    }

    fn search_root(
        &mut self,
        pos: &mut Position,
        depth: u8,
        control: &SearchControl,
        previous_best: Option<Move>,
    ) -> Result<RootSearch, SearchAbort> {
        let mut legal = MoveList::new();
        legal_moves(pos, &mut legal);
        if legal.is_empty() {
            let score = terminal_score(pos, 0);
            return Ok(RootSearch {
                best_move: None,
                score,
            });
        }

        let tt_move = self.tt.probe(pos.zobrist).and_then(|entry| entry.best_move);
        let ordered = self.ordered_moves(pos, legal.as_slice(), previous_best, tt_move, 0);
        let mut alpha = -INF;
        let beta = INF;
        let mut best_score = -INF;
        let mut best_move = ordered.first().map(|scored| scored.mv);

        for scored in ordered {
            self.check_abort(control)?;
            let undo = make_legal_move_unchecked(pos, scored.mv);
            let score = -self.negamax(pos, i32::from(depth) - 1, -beta, -alpha, 1, control)?;
            unmake_move(pos, scored.mv, undo);

            if score > best_score {
                best_score = score;
                best_move = Some(scored.mv);
            }
            if score > alpha {
                alpha = score;
            }
        }

        self.tt.store(
            pos.zobrist,
            depth,
            Score(score_to_tt(best_score, 0)),
            Bound::Exact,
            best_move,
        );
        Ok(RootSearch {
            best_move,
            score: best_score,
        })
    }

    fn negamax(
        &mut self,
        pos: &mut Position,
        depth: i32,
        mut alpha: i32,
        beta: i32,
        ply: u8,
        control: &SearchControl,
    ) -> Result<i32, SearchAbort> {
        self.observe_node(ply, control)?;

        let in_check_now = in_check(pos, pos.side_to_move);
        if depth <= 0 && !in_check_now {
            if self.quiescence {
                return self.quiescence(pos, alpha, beta, ply, 0, control);
            }
            return Ok(self.evaluator.evaluate(pos).0);
        }

        let search_depth = if in_check_now && depth <= 0 { 1 } else { depth };
        let original_alpha = alpha;
        let tt_entry = self.tt.probe(pos.zobrist);
        if let Some(entry) = tt_entry
            && i32::from(entry.depth) >= search_depth
        {
            let score = score_from_tt(entry.score.0, ply);
            match entry.bound {
                Bound::Exact => return Ok(score),
                Bound::Lower if score >= beta => return Ok(score),
                Bound::Upper if score <= alpha => return Ok(score),
                Bound::Lower | Bound::Upper => {}
            }
        }

        let mut legal = MoveList::new();
        legal_moves(pos, &mut legal);
        if legal.is_empty() {
            return Ok(terminal_score(pos, ply));
        }

        let tt_move = tt_entry.and_then(|entry| entry.best_move);
        let ordered = self.ordered_moves(pos, legal.as_slice(), None, tt_move, ply);
        let mut best_score = -INF;
        let mut best_move = None;

        for scored in ordered {
            let undo = make_legal_move_unchecked(pos, scored.mv);
            let score = -self.negamax(pos, search_depth - 1, -beta, -alpha, ply + 1, control)?;
            unmake_move(pos, scored.mv, undo);

            if score > best_score {
                best_score = score;
                best_move = Some(scored.mv);
            }
            if score > alpha {
                alpha = score;
                if alpha >= beta {
                    self.record_cutoff(scored.mv, search_depth, ply);
                    self.tt.store(
                        pos.zobrist,
                        search_depth as u8,
                        Score(score_to_tt(alpha, ply)),
                        Bound::Lower,
                        best_move,
                    );
                    return Ok(alpha);
                }
            }
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else {
            Bound::Exact
        };
        self.tt.store(
            pos.zobrist,
            search_depth as u8,
            Score(score_to_tt(best_score, ply)),
            bound,
            best_move,
        );
        Ok(best_score)
    }

    fn quiescence(
        &mut self,
        pos: &mut Position,
        mut alpha: i32,
        beta: i32,
        ply: u8,
        qply: u8,
        control: &SearchControl,
    ) -> Result<i32, SearchAbort> {
        self.observe_node(ply, control)?;
        let in_check_now = in_check(pos, pos.side_to_move);

        if !in_check_now {
            let stand_pat = self.evaluator.evaluate(pos).0;
            if stand_pat >= beta {
                return Ok(beta);
            }
            alpha = alpha.max(stand_pat);
            if qply >= MAX_QUIESCENCE_PLY {
                return Ok(alpha);
            }
        }

        let mut legal = MoveList::new();
        legal_moves(pos, &mut legal);
        if legal.is_empty() {
            return Ok(terminal_score(pos, ply));
        }

        let quiet_checks = qply == 0;
        let moves = self.quiescence_moves(pos, legal.as_slice(), in_check_now, quiet_checks);
        if moves.is_empty() {
            return Ok(alpha);
        }
        let ordered = self.ordered_moves(pos, &moves, None, None, ply);

        for scored in ordered {
            let undo = make_legal_move_unchecked(pos, scored.mv);
            let score = -self.quiescence(pos, -beta, -alpha, ply + 1, qply + 1, control)?;
            unmake_move(pos, scored.mv, undo);

            if score >= beta {
                return Ok(beta);
            }
            alpha = alpha.max(score);
        }

        Ok(alpha)
    }

    fn quiescence_moves(
        &self,
        pos: &mut Position,
        legal: &[Move],
        in_check_now: bool,
        quiet_checks: bool,
    ) -> Vec<Move> {
        let mut moves = Vec::new();
        for &mv in legal {
            if in_check_now || is_capture_or_promotion(mv) {
                moves.push(mv);
                continue;
            }
            if quiet_checks && self.gives_check(pos, mv) {
                moves.push(mv);
            }
        }
        moves
    }

    fn gives_check(&self, pos: &mut Position, mv: Move) -> bool {
        let opponent = pos.side_to_move.opposite();
        let undo = make_legal_move_unchecked(pos, mv);
        let gives_check = in_check(pos, opponent);
        unmake_move(pos, mv, undo);
        gives_check
    }

    fn ordered_moves(
        &self,
        pos: &Position,
        moves: &[Move],
        pv_move: Option<Move>,
        tt_move: Option<Move>,
        ply: u8,
    ) -> Vec<ScoredMove> {
        let mut scored: Vec<ScoredMove> = moves
            .iter()
            .map(|&mv| ScoredMove {
                mv,
                score: self.move_score(pos, mv, pv_move, tt_move, ply),
            })
            .collect();
        if self.move_ordering {
            scored.sort_by(|left, right| {
                right
                    .score
                    .cmp(&left.score)
                    .then_with(|| move_order_key(left.mv).cmp(&move_order_key(right.mv)))
            });
        }
        scored
    }

    fn move_score(
        &self,
        pos: &Position,
        mv: Move,
        pv_move: Option<Move>,
        tt_move: Option<Move>,
        ply: u8,
    ) -> i32 {
        if !self.move_ordering {
            return 0;
        }
        if Some(mv) == pv_move {
            return 3_000_000;
        }
        if Some(mv) == tt_move {
            return 2_800_000;
        }

        let mut score = 0;
        if let Some((victim, attacker)) = capture_pieces(pos, mv) {
            score += 1_500_000 + material_value(victim) * 16 - material_value(attacker);
        }
        if let Some(promotion) = promotion_piece(mv) {
            score += 1_200_000 + material_value(promotion);
        }
        if !is_capture_or_promotion(mv) {
            let ply_index = usize::from(ply).min(MAX_PLY - 1);
            if self.killers[ply_index][0] == Some(mv) {
                score += 900_000;
            } else if self.killers[ply_index][1] == Some(mv) {
                score += 800_000;
            }
            score += self.history[mv.from.index()][mv.to.index()];
        }
        score
    }

    fn record_cutoff(&mut self, mv: Move, depth: i32, ply: u8) {
        if is_capture_or_promotion(mv) {
            return;
        }
        let ply_index = usize::from(ply).min(MAX_PLY - 1);
        if self.killers[ply_index][0] != Some(mv) {
            self.killers[ply_index][1] = self.killers[ply_index][0];
            self.killers[ply_index][0] = Some(mv);
        }
        let bonus = depth.saturating_mul(depth).max(1);
        let entry = &mut self.history[mv.from.index()][mv.to.index()];
        *entry = entry.saturating_add(bonus);
    }

    fn principal_variation(&self, root: &Position, first: Option<Move>, depth: u8) -> Vec<Move> {
        let mut pos = root.clone();
        let mut pv = Vec::new();
        let mut seen = Vec::new();

        if let Some(mv) = first {
            if !contains_legal_move(&pos, mv) {
                return pv;
            }
            let _undo = make_legal_move_unchecked(&mut pos, mv);
            pv.push(mv);
        }

        while pv.len() < usize::from(depth) {
            if seen.contains(&pos.zobrist) {
                break;
            }
            seen.push(pos.zobrist);

            let Some(entry) = self.tt.probe(pos.zobrist) else {
                break;
            };
            let Some(mv) = entry.best_move else {
                break;
            };
            if !contains_legal_move(&pos, mv) {
                break;
            }
            let _undo = make_legal_move_unchecked(&mut pos, mv);
            pv.push(mv);
        }
        pv
    }

    fn observe_node(&mut self, ply: u8, control: &SearchControl) -> Result<(), SearchAbort> {
        self.nodes = self.nodes.saturating_add(1);
        self.seldepth = self.seldepth.max(ply);
        self.check_abort(control)
    }

    fn check_abort(&self, control: &SearchControl) -> Result<(), SearchAbort> {
        if control.stop.is_stopped() {
            return Err(SearchAbort);
        }
        if control
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(SearchAbort);
        }
        if control
            .node_limit
            .is_some_and(|node_limit| self.nodes >= node_limit)
        {
            return Err(SearchAbort);
        }
        Ok(())
    }
}

impl<E: Evaluator> Searcher for SingleThreadSearcher<E> {
    fn new_game(&mut self) {
        self.tt.clear();
        self.history = [[0; 64]; 64];
        self.killers = [[None; 2]; MAX_PLY];
    }

    fn set_position(&mut self, pos: Position, history: Vec<Move>) {
        self.position = pos;
        let _ = history;
    }

    fn search(&mut self, limits: SearchLimits, stop: StopToken) -> SearchResult {
        self.reset_search_state();
        let mut pos = self.position.clone();
        let mut root_moves = MoveList::new();
        legal_moves(&pos, &mut root_moves);
        let mut best_move = root_moves.as_slice().first().copied();
        let started = Instant::now();
        let control = SearchControl {
            deadline: deadline_for(&limits, &pos, started),
            node_limit: limits.nodes,
            stop,
        };
        let max_depth = max_depth_for(&limits);
        let mut info = Vec::new();
        let mut previous_best = None;
        let mut last_score = terminal_score(&pos, 0);

        if root_moves.is_empty() {
            return SearchResult {
                best_move: None,
                ponder: None,
                info: vec![SearchInfo {
                    depth: 0,
                    seldepth: 0,
                    score: Score(last_score),
                    nodes: 0,
                    nps: 0,
                    hashfull: Some(self.tt.hashfull()),
                    pv: Vec::new(),
                }],
            };
        }

        for depth in 1..=max_depth {
            match self.search_root(&mut pos, depth, &control, previous_best) {
                Ok(root) => {
                    if let Some(mv) = root.best_move {
                        best_move = Some(mv);
                        previous_best = Some(mv);
                    }
                    last_score = root.score;
                    let pv = self.principal_variation(&self.position, best_move, depth);
                    let elapsed = started.elapsed();
                    info.push(SearchInfo {
                        depth,
                        seldepth: self.seldepth,
                        score: Score(last_score),
                        nodes: self.nodes,
                        nps: nodes_per_second(self.nodes, elapsed),
                        hashfull: Some(self.tt.hashfull()),
                        pv,
                    });
                }
                Err(SearchAbort) => break,
            }
        }

        if let Some(latest) = info.last_mut() {
            latest.nodes = self.nodes;
            latest.nps = nodes_per_second(self.nodes, started.elapsed());
            latest.hashfull = Some(self.tt.hashfull());
        }

        if info.is_empty() {
            let pv = best_move.into_iter().collect();
            info.push(SearchInfo {
                depth: 0,
                seldepth: self.seldepth,
                score: Score(last_score),
                nodes: self.nodes,
                nps: nodes_per_second(self.nodes, started.elapsed()),
                hashfull: Some(self.tt.hashfull()),
                pv,
            });
        }

        let ponder = info.last().and_then(|latest| latest.pv.get(1)).copied();
        SearchResult {
            best_move,
            ponder,
            info,
        }
    }

    fn set_hash_size_mb(&mut self, mb: usize) {
        SingleThreadSearcher::set_hash_size_mb(self, mb);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScoredMove {
    mv: Move,
    score: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RootSearch {
    best_move: Option<Move>,
    score: i32,
}

#[derive(Clone, Debug)]
struct SearchControl {
    deadline: Option<Instant>,
    node_limit: Option<u64>,
    stop: StopToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SearchAbort;

fn max_depth_for(limits: &SearchLimits) -> u8 {
    if let Some(depth) = limits.depth {
        return depth.clamp(1, (MAX_PLY - 1) as u8);
    }
    if limits.movetime.is_some()
        || limits.white_time.is_some()
        || limits.black_time.is_some()
        || limits.nodes.is_some()
        || limits.infinite
    {
        return (MAX_PLY - 1) as u8;
    }
    4
}

fn deadline_for(limits: &SearchLimits, pos: &Position, now: Instant) -> Option<Instant> {
    if let Some(movetime) = limits.movetime {
        return Some(now + movetime);
    }
    if limits.infinite {
        return None;
    }

    let (remaining, increment) = match pos.side_to_move {
        Color::White => (limits.white_time, limits.white_increment),
        Color::Black => (limits.black_time, limits.black_increment),
    };
    let remaining = remaining?;
    let moves_to_go = u128::from(limits.moves_to_go.unwrap_or(30).max(1));
    let base_ms = remaining.as_millis() / moves_to_go;
    let increment_ms = increment.map_or(0, |duration| duration.as_millis() / 2);
    let mut allocation_ms = (base_ms + increment_ms).max(1);
    let remaining_ms = remaining.as_millis();
    if remaining_ms > 20 {
        allocation_ms = allocation_ms.min(remaining_ms - 10);
    }
    Some(now + Duration::from_millis(allocation_ms.min(u128::from(u64::MAX)) as u64))
}

fn terminal_score(pos: &Position, ply: u8) -> i32 {
    if in_check(pos, pos.side_to_move) {
        -mate_score_for_ply(ply)
    } else {
        0
    }
}

fn mate_score_for_ply(ply: u8) -> i32 {
    MATE_SCORE_VALUE - i32::from(ply)
}

fn score_to_tt(score: i32, ply: u8) -> i32 {
    if score >= MATE_THRESHOLD {
        score + i32::from(ply)
    } else if score <= -MATE_THRESHOLD {
        score - i32::from(ply)
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: u8) -> i32 {
    if score >= MATE_THRESHOLD {
        score - i32::from(ply)
    } else if score <= -MATE_THRESHOLD {
        score + i32::from(ply)
    } else {
        score
    }
}

fn nodes_per_second(nodes: u64, elapsed: Duration) -> u64 {
    let millis = elapsed.as_millis().max(1);
    ((u128::from(nodes) * 1_000) / millis).min(u128::from(u64::MAX)) as u64
}

fn contains_legal_move(pos: &Position, mv: Move) -> bool {
    let mut legal = MoveList::new();
    legal_moves(pos, &mut legal);
    legal.as_slice().contains(&mv)
}

fn is_capture_or_promotion(mv: Move) -> bool {
    matches!(
        mv.kind,
        MoveKind::Capture
            | MoveKind::EnPassant
            | MoveKind::Promotion(_)
            | MoveKind::PromotionCapture(_)
    )
}

fn promotion_piece(mv: Move) -> Option<PieceKind> {
    match mv.kind {
        MoveKind::Promotion(kind) | MoveKind::PromotionCapture(kind) => Some(kind),
        _ => None,
    }
}

fn capture_pieces(pos: &Position, mv: Move) -> Option<(PieceKind, PieceKind)> {
    let attacker = piece_at(pos, mv.from)?.kind;
    let victim = match mv.kind {
        MoveKind::EnPassant => PieceKind::Pawn,
        MoveKind::Capture | MoveKind::PromotionCapture(_) => piece_at(pos, mv.to)?.kind,
        _ => return None,
    };
    Some((victim, attacker))
}

fn move_order_key(mv: Move) -> u16 {
    let promotion = promotion_piece(mv).map_or(0, |kind| kind.index() as u16 + 1);
    u16::from(mv.from.0) << 10 | u16::from(mv.to.0) << 4 | promotion
}

#[must_use]
pub fn move_from_uci(pos: &Position, text: &str) -> Option<Move> {
    let mut legal = MoveList::new();
    legal_moves(pos, &mut legal);
    legal
        .iter()
        .copied()
        .find(|candidate| candidate.to_string() == text)
}

#[must_use]
pub fn square(name: &str) -> Option<Square> {
    Square::parse_name(name)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TacticalFixture {
    pub line: usize,
    pub id: Option<String>,
    pub fen: String,
    pub best_moves: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TacticalCase {
    pub line: usize,
    pub id: Option<String>,
    pub fen: String,
    pub expected: Vec<Move>,
    pub best_move: Option<Move>,
    pub depth: u8,
    pub score: Score,
    pub nodes: u64,
    pub pv: Vec<Move>,
}

impl TacticalCase {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.best_move
            .is_some_and(|best_move| self.expected.contains(&best_move))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TacticalSuiteReport {
    pub cases: Vec<TacticalCase>,
}

impl TacticalSuiteReport {
    #[must_use]
    pub fn passed(&self) -> usize {
        self.cases.iter().filter(|case| case.passed()).count()
    }

    #[must_use]
    pub fn failed(&self) -> usize {
        self.cases.len() - self.passed()
    }

    #[must_use]
    pub fn pass_rate(&self) -> f64 {
        if self.cases.is_empty() {
            0.0
        } else {
            self.passed() as f64 / self.cases.len() as f64
        }
    }

    #[must_use]
    pub fn failure_summary(&self) -> String {
        let mut summary = String::new();
        for case in self.cases.iter().filter(|case| !case.passed()) {
            let id = case.id.as_deref().unwrap_or("unnamed");
            let expected = join_moves(&case.expected);
            let returned = case
                .best_move
                .map_or_else(|| "none".to_owned(), |best_move| best_move.to_string());
            let pv = join_moves(&case.pv);
            summary.push_str(&format!(
                "{id} line {}: fen '{}', expected [{}], returned {}, depth {}, score {}, nodes {}, pv [{}]\n",
                case.line, case.fen, expected, returned, case.depth, case.score.0, case.nodes, pv
            ));
        }
        summary
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TacticalSuiteError {
    Io {
        path: String,
        message: String,
    },
    Parse {
        path: Option<String>,
        line: usize,
        kind: TacticalParseError,
    },
    InvalidFen {
        path: Option<String>,
        line: usize,
        fen: String,
        source: FenError,
    },
    IllegalBestMove {
        path: Option<String>,
        line: usize,
        fen: String,
        best_move: String,
    },
}

impl fmt::Display for TacticalSuiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "failed to read {path}: {message}"),
            Self::Parse { path, line, kind } => {
                write!(f, "{}: {kind}", source_location(path, *line))
            }
            Self::InvalidFen {
                path,
                line,
                fen,
                source,
            } => write!(
                f,
                "{}: invalid tactical FEN '{}': {source}",
                source_location(path, *line),
                fen
            ),
            Self::IllegalBestMove {
                path,
                line,
                fen,
                best_move,
            } => write!(
                f,
                "{}: best move '{}' is not legal in '{}'",
                source_location(path, *line),
                best_move,
                fen
            ),
        }
    }
}

impl Error for TacticalSuiteError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TacticalParseError {
    MissingFenFields,
    MissingBestMove,
    EmptyBestMove,
}

impl fmt::Display for TacticalParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFenFields => f.write_str("EPD row must start with four FEN fields"),
            Self::MissingBestMove => f.write_str("EPD row is missing a bm operation"),
            Self::EmptyBestMove => f.write_str("bm operation must list at least one move"),
        }
    }
}

impl Error for TacticalParseError {}

pub fn parse_tactical_epd(input: &str) -> Result<Vec<TacticalFixture>, TacticalSuiteError> {
    parse_tactical_epd_with_path(input, None)
}

pub fn run_tactical_suite(
    path: &Path,
    limits: SearchLimits,
) -> Result<TacticalSuiteReport, TacticalSuiteError> {
    let input = fs::read_to_string(path).map_err(|err| TacticalSuiteError::Io {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let fixtures = parse_tactical_epd_with_path(&input, Some(path))?;
    let mut searcher = DefaultSearcher::default();
    solve_tactical_fixtures_with_path(&mut searcher, &fixtures, limits, Some(path))
}

pub fn solve_tactical_fixtures<S: Searcher>(
    searcher: &mut S,
    fixtures: &[TacticalFixture],
    limits: SearchLimits,
) -> Result<TacticalSuiteReport, TacticalSuiteError> {
    solve_tactical_fixtures_with_path(searcher, fixtures, limits, None)
}

fn solve_tactical_fixtures_with_path<S: Searcher>(
    searcher: &mut S,
    fixtures: &[TacticalFixture],
    limits: SearchLimits,
    path: Option<&Path>,
) -> Result<TacticalSuiteReport, TacticalSuiteError> {
    let mut cases = Vec::with_capacity(fixtures.len());
    for fixture in fixtures {
        let pos = chess_core::parse_fen(&fixture.fen).map_err(|source| {
            TacticalSuiteError::InvalidFen {
                path: path_string(path),
                line: fixture.line,
                fen: fixture.fen.clone(),
                source,
            }
        })?;
        let mut expected = Vec::with_capacity(fixture.best_moves.len());
        for best_move in &fixture.best_moves {
            let Some(mv) = move_from_uci(&pos, best_move) else {
                return Err(TacticalSuiteError::IllegalBestMove {
                    path: path_string(path),
                    line: fixture.line,
                    fen: fixture.fen.clone(),
                    best_move: best_move.clone(),
                });
            };
            expected.push(mv);
        }

        searcher.new_game();
        searcher.set_position(pos, Vec::new());
        let result = searcher.search(limits.clone(), StopToken::new());
        let latest = result.info.last();
        cases.push(TacticalCase {
            line: fixture.line,
            id: fixture.id.clone(),
            fen: fixture.fen.clone(),
            expected,
            best_move: result.best_move,
            depth: latest.map_or(0, |info| info.depth),
            score: latest.map_or(Score(0), |info| info.score),
            nodes: latest.map_or(0, |info| info.nodes),
            pv: latest.map_or_else(Vec::new, |info| info.pv.clone()),
        });
    }
    Ok(TacticalSuiteReport { cases })
}

fn parse_tactical_epd_with_path(
    input: &str,
    path: Option<&Path>,
) -> Result<Vec<TacticalFixture>, TacticalSuiteError> {
    let mut fixtures = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let fixture = parse_tactical_epd_line(trimmed, line_number).map_err(|kind| {
            TacticalSuiteError::Parse {
                path: path_string(path),
                line: line_number,
                kind,
            }
        })?;
        fixtures.push(fixture);
    }
    Ok(fixtures)
}

fn parse_tactical_epd_line(
    line: &str,
    line_number: usize,
) -> Result<TacticalFixture, TacticalParseError> {
    let mut rest = line;
    let mut fen_parts = Vec::with_capacity(4);
    for _ in 0..4 {
        let Some((field, remaining)) = take_token(rest) else {
            return Err(TacticalParseError::MissingFenFields);
        };
        fen_parts.push(field);
        rest = remaining;
    }

    let fen = format!(
        "{} {} {} {} 0 1",
        fen_parts[0], fen_parts[1], fen_parts[2], fen_parts[3]
    );
    let (best_moves, id) = parse_epd_operations(rest)?;
    Ok(TacticalFixture {
        line: line_number,
        id,
        fen,
        best_moves,
    })
}

fn parse_epd_operations(
    operations: &str,
) -> Result<(Vec<String>, Option<String>), TacticalParseError> {
    let mut best_moves = Vec::new();
    let mut id = None;
    for operation in operations.split(';').map(str::trim) {
        if operation.is_empty() {
            continue;
        }
        let Some((opcode, payload)) = take_token(operation) else {
            continue;
        };
        match opcode {
            "bm" => {
                let payload = payload.trim();
                if payload.is_empty() {
                    return Err(TacticalParseError::EmptyBestMove);
                }
                best_moves = payload.split_whitespace().map(str::to_owned).collect();
            }
            "id" => {
                let payload = payload.trim();
                if !payload.is_empty() {
                    id = Some(parse_epd_string(payload));
                }
            }
            _ => {}
        }
    }

    if best_moves.is_empty() {
        Err(TacticalParseError::MissingBestMove)
    } else {
        Ok((best_moves, id))
    }
}

fn parse_epd_string(input: &str) -> String {
    let trimmed = input.trim();
    if let Some(stripped) = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        stripped.replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        trimmed.to_owned()
    }
}

fn take_token(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map_or(trimmed.len(), |(index, _)| index);
    Some((&trimmed[..end], &trimmed[end..]))
}

fn path_string(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

fn source_location(path: &Option<String>, line: usize) -> String {
    match path {
        Some(path) => format!("{path}:{line}"),
        None => format!("line {line}"),
    }
}

fn join_moves(moves: &[Move]) -> String {
    moves
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_core::{parse_fen, startpos};
    use std::path::PathBuf;
    use std::thread;

    fn search_best(fen: &str, depth: u8) -> (Move, SearchResult) {
        let mut searcher = DefaultSearcher::default();
        searcher.set_position(parse_fen(fen).unwrap(), Vec::new());
        let result = searcher.search(SearchLimits::fixed_depth(depth), StopToken::new());
        (result.best_move.unwrap(), result)
    }

    #[test]
    fn finds_mate_in_one() {
        let fen = "7k/5K2/6Q1/8/8/8/8/8 w - - 0 1";
        let (best, _) = search_best(fen, 2);
        let mut pos = parse_fen(fen).unwrap();
        let undo = make_legal_move_unchecked(&mut pos, best);
        let mut replies = MoveList::new();
        legal_moves(&pos, &mut replies);

        assert!(in_check(&pos, pos.side_to_move));
        assert!(replies.is_empty());
        unmake_move(&mut pos, best, undo);
    }

    #[test]
    fn prefers_winning_hanging_queen() {
        let fen = "4k3/8/8/8/4q3/8/4R3/4K3 w - - 0 1";
        let (best, _) = search_best(fen, 2);

        assert_eq!(best.to_string(), "e2e4");
    }

    #[test]
    fn iterative_deepening_emits_info_and_pv() {
        let mut searcher = DefaultSearcher::default();
        searcher.set_position(startpos(), Vec::new());
        let result = searcher.search(SearchLimits::fixed_depth(3), StopToken::new());

        assert_eq!(result.info.len(), 3);
        assert_eq!(result.info[0].depth, 1);
        assert_eq!(result.info[2].depth, 3);
        assert!(result.info.iter().all(|info| !info.pv.is_empty()));
        assert_eq!(
            result.best_move,
            result.info.last().unwrap().pv.first().copied()
        );
    }

    #[test]
    fn quiescence_reduces_capture_horizon_score() {
        let fen = "4k3/4r3/8/8/4q3/8/4R3/4K3 w - - 0 1";
        let pos = parse_fen(fen).unwrap();
        let mut quiet = DefaultSearcher::default();
        quiet.set_position(pos.clone(), Vec::new());
        quiet.set_quiescence_enabled(false);
        let quiet_score = quiet
            .search(SearchLimits::fixed_depth(1), StopToken::new())
            .info
            .last()
            .unwrap()
            .score
            .0;

        let mut noisy = DefaultSearcher::default();
        noisy.set_position(pos, Vec::new());
        let noisy_score = noisy
            .search(SearchLimits::fixed_depth(1), StopToken::new())
            .info
            .last()
            .unwrap()
            .score
            .0;

        assert!(noisy_score < quiet_score - 200);
    }

    #[test]
    fn transposition_table_replaces_by_depth_and_reports_hashfull() {
        let mv = Move::new(Square(0), Square(1), MoveKind::Quiet);
        let mut tt = TranspositionTable::new(2);

        assert!(tt.store(2, 4, Score(10), Bound::Exact, Some(mv)));
        assert_eq!(tt.probe(2).unwrap().bound, Bound::Exact);
        assert_eq!(tt.hashfull(), 500);
        assert!(!tt.store(4, 3, Score(20), Bound::Lower, None));
        assert_eq!(tt.probe(2).unwrap().score, Score(10));
        assert!(tt.store(4, 5, Score(20), Bound::Lower, None));
        assert!(tt.probe(2).is_none());
        assert_eq!(tt.probe(4).unwrap().bound, Bound::Lower);
        assert!(tt.store(4, 6, Score(5), Bound::Upper, Some(mv)));
        assert_eq!(tt.probe(4).unwrap().bound, Bound::Upper);
    }

    #[test]
    fn move_ordering_reduces_nodes_on_tactical_position() {
        let fen = "r3k2r/pppq1ppp/2npbn2/4p3/2B1P3/2NP1N2/PPP2PPP/R2Q1RK1 w kq - 0 1";
        let pos = parse_fen(fen).unwrap();

        let mut ordered = DefaultSearcher::default();
        ordered.set_position(pos.clone(), Vec::new());
        let ordered_nodes = ordered
            .search(SearchLimits::fixed_depth(3), StopToken::new())
            .info
            .last()
            .unwrap()
            .nodes;

        let mut unordered = DefaultSearcher::default();
        unordered.set_position(pos, Vec::new());
        unordered.set_move_ordering_enabled(false);
        let unordered_nodes = unordered
            .search(SearchLimits::fixed_depth(3), StopToken::new())
            .info
            .last()
            .unwrap()
            .nodes;

        assert!(ordered_nodes < unordered_nodes);
    }

    #[test]
    fn fixed_movetime_returns_before_tolerance() {
        let mut searcher = DefaultSearcher::default();
        searcher.set_position(startpos(), Vec::new());
        let started = Instant::now();
        let result = searcher.search(
            SearchLimits::fixed_movetime(Duration::from_millis(25)),
            StopToken::new(),
        );

        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(result.best_move.is_some());
        assert!(!result.info.is_empty());
    }

    #[test]
    fn stop_token_returns_promptly() {
        let stop = StopToken::new();
        let worker_stop = stop.clone();
        let handle = thread::spawn(move || {
            let mut searcher = DefaultSearcher::default();
            searcher.set_position(startpos(), Vec::new());
            searcher.search(SearchLimits::fixed_depth(95), worker_stop)
        });

        thread::sleep(Duration::from_millis(10));
        let started = Instant::now();
        stop.stop();
        let result = handle.join().unwrap();

        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(result.best_move.is_some());
    }

    #[test]
    fn fixed_node_limit_stops_after_budget() {
        let mut searcher = DefaultSearcher::default();
        searcher.set_position(startpos(), Vec::new());
        let result = searcher.search(SearchLimits::fixed_nodes(200), StopToken::new());

        assert!(result.info.last().unwrap().nodes >= 200);
        assert!(result.info.last().unwrap().nodes < 400);
    }

    #[test]
    fn parses_epd_best_move_tags() {
        let fixtures = parse_tactical_epd(
            r#"
            # comment
            7k/5K2/6Q1/8/8/8/8/8 w - - bm g6g8 g6g7; id "mate choices";
            "#,
        )
        .unwrap();

        assert_eq!(fixtures.len(), 1);
        assert_eq!(fixtures[0].line, 3);
        assert_eq!(fixtures[0].id.as_deref(), Some("mate choices"));
        assert_eq!(fixtures[0].fen, "7k/5K2/6Q1/8/8/8/8/8 w - - 0 1");
        assert_eq!(fixtures[0].best_moves, ["g6g8", "g6g7"]);
    }

    #[test]
    fn fixture_backed_tactical_micro_suite_solves_all_cases() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/tactics/search-micro.epd");
        let report = run_tactical_suite(&path, SearchLimits::fixed_depth(3)).unwrap();

        assert_eq!(report.cases.len(), 2);
        assert_eq!(report.failed(), 0, "{}", report.failure_summary());
    }
}
