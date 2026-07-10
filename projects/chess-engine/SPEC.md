# Chess Engine + Local GUI: Build-Ready Specification

## 1. Overview, Goals, And Demonstration Purpose

This project is a public, local-only Kensho evaluation. Kensho should coordinate
many agent workers across stable module boundaries to build a complete chess
engine and playable local GUI. The build is successful only when the delivered
software passes objective chess correctness, protocol, tactical, strength, and UI
acceptance gates.

The delivered product is:

- A UCI-compliant chess engine executable.
- A local desktop board GUI that launches or connects to the engine over UCI.
- A headless test harness that verifies move generation, search behavior, UCI,
  tactical solving, persistence, and GUI smoke behavior without network access.

The evaluation of Kensho is:

- Can it decompose a large project into independent slices with clear ownership?
- Can it preserve stable interfaces while many agents edit different modules?
- Can reviewer and auditor agents catch objective failures before integration?
- Can the final repo prove correctness from local commands rather than prose?

All product runtime behavior must be local. No cloud deployment, hosted service,
account login, paid API, telemetry backend, or network call is allowed.

## 2. Non-Goals And Out Of Scope

- No cloud deployment or web-hosted chess site.
- No multiplayer server, account system, matchmaking, or persistent remote storage.
- No opening-book download, online tablebases, or online engine API.
- No Syzygy/endgame tablebase support in v1.
- No neural-network evaluation in v1.
- No Chess960, variants, bughouse, puzzle database editor, or tournament server.
- No attempt to beat modern top engines. The target is a credible hobby engine,
  not Stockfish-class play.
- No GPU acceleration.
- No runtime dependency on external UCI engines. Optional local gauntlets may use
  engines already installed on the developer machine, but acceptance cannot
  require network retrieval.

## 3. Architecture And Module Map

The implementation should be a Rust workspace with headless engine crates and a
desktop GUI app. The exact crate names may vary, but the ownership boundaries and
interfaces below are mandatory unless a reviewed architecture issue changes them.

### Workspace Shape

```text
crates/
  chess-core/        Board, FEN, move representation, make/unmake, legal moves.
  chess-perft/       Perft runner, divide output, fixture parser.
  chess-eval/        Static evaluation.
  chess-search/      Alpha-beta, iterative deepening, quiescence, TT, time control.
  chess-uci/         UCI protocol parser and engine loop.
  chess-engine/      CLI binary wiring core + eval + search + UCI.
apps/
  local-board/       Desktop GUI, talks to chess-engine through UCI.
tests/
  fixtures/          Perft, UCI, tactical, and GUI smoke fixtures.
```

### Core Types

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color { White, Black }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceKind { Pawn, Knight, Bishop, Rook, Queen, King }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Piece { pub color: Color, pub kind: PieceKind }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Square(pub u8); // 0 = a1, 63 = h8

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bitboard(pub u64);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
```

The board representation must be bitboard-based. Mailbox or array helpers are
allowed only as derived caches, never as the source of truth.

### FEN And Position Interface

```rust
pub fn startpos() -> Position;
pub fn parse_fen(input: &str) -> Result<Position, FenError>;
pub fn to_fen(pos: &Position) -> String;
pub fn piece_at(pos: &Position, sq: Square) -> Option<Piece>;
pub fn validate_position(pos: &Position) -> Result<(), PositionError>;
```

Acceptance:

- `parse_fen(to_fen(pos))` round-trips for every perft fixture.
- Invalid FEN returns structured errors, not panics.
- Castling rights, en-passant square, halfmove, and fullmove fields are preserved.

### Move Generation Interface

```rust
pub const MAX_LEGAL_MOVES: usize = 256;

pub struct MoveList {
    len: usize,
    moves: [Move; MAX_LEGAL_MOVES],
}

pub fn pseudo_legal_moves(pos: &Position, out: &mut MoveList);
pub fn legal_moves(pos: &Position, out: &mut MoveList);
pub fn legal_captures(pos: &Position, out: &mut MoveList);
pub fn is_square_attacked(pos: &Position, sq: Square, by: Color) -> bool;
pub fn in_check(pos: &Position, side: Color) -> bool;
```

Required legal-move coverage:

- Pawn single push, double push, captures, promotions to N/B/R/Q, promotion captures.
- En passant, including discovered-check illegality.
- Castling, including rights removal, blockers, check-through-check, and rook movement.
- Pins, double check, king adjacency, discovered checks, check evasion.
- Checkmate and stalemate produce zero legal moves without special terminal nodes in perft.

### Make/Unmake Interface

```rust
#[derive(Clone, Copy, Debug)]
pub struct Undo {
    pub captured: Option<Piece>,
    pub castle_rights: CastleRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u16,
    pub fullmove_number: u16,
    pub zobrist: u64,
}

pub fn make_move(pos: &mut Position, mv: Move) -> Result<Undo, MoveError>;
pub fn unmake_move(pos: &mut Position, mv: Move, undo: Undo);
pub fn is_legal_move(pos: &Position, mv: Move) -> bool;
```

Acceptance:

- `make_move` rejects illegal moves and never leaves `Position` partially mutated.
- `unmake_move` restores byte-for-byte equality for all fields including Zobrist.
- Random legal playouts of at least 10,000 move/unmake pairs preserve invariants.

### Perft Harness Interface

```rust
pub struct PerftResult {
    pub nodes: u64,
    pub captures: u64,
    pub en_passant: u64,
    pub castles: u64,
    pub promotions: u64,
    pub checks: u64,
    pub checkmates: u64,
}

pub fn perft(pos: &mut Position, depth: u8) -> u64;
pub fn perft_stats(pos: &mut Position, depth: u8) -> PerftResult;
pub fn perft_divide(pos: &mut Position, depth: u8) -> Vec<(Move, u64)>;
pub fn run_perft_suite(path: &Path, max_depth: Option<u8>) -> Result<SuiteReport, PerftError>;
```

Perft counts leaf nodes only. A mismatch at any mandatory depth is a release
blocker.

### Evaluation Interface

```rust
pub trait Evaluator: Send + Sync {
    fn evaluate(&self, pos: &Position) -> Score; // centipawns, positive for side to move
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Score(pub i32);
```

Minimum v1 evaluation terms:

- Material with phase-aware values.
- Piece-square tables.
- Mobility for sliders and knights.
- King safety basics.
- Pawn structure: passed, isolated, doubled, connected pawns.
- Tempo term.
- Checkmate score normalization for search.

### Search Interface

```rust
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

pub struct SearchInfo {
    pub depth: u8,
    pub seldepth: u8,
    pub score: Score,
    pub nodes: u64,
    pub nps: u64,
    pub hashfull: Option<u16>,
    pub pv: Vec<Move>,
}

pub struct SearchResult {
    pub best_move: Option<Move>,
    pub ponder: Option<Move>,
    pub info: Vec<SearchInfo>,
}

pub trait Searcher {
    fn new_game(&mut self);
    fn set_position(&mut self, pos: Position, history: Vec<Move>);
    fn search(&mut self, limits: SearchLimits, stop: StopToken) -> SearchResult;
}
```

Required search features:

- Negamax alpha-beta.
- Iterative deepening.
- Quiescence search over forcing captures and checks.
- Transposition table with exact/lower/upper bounds.
- Move ordering: PV move, TT move, captures by MVV-LVA or SEE, killer moves,
  history heuristic, promotions, checks.
- Time management for fixed depth, fixed movetime, and clock/increment controls.
- Stop token checked often enough that `stop` returns `bestmove` within 200 ms.

### UCI Protocol Interface

```rust
pub enum UciCommand {
    Uci,
    Debug(bool),
    IsReady,
    SetOption { name: String, value: Option<String> },
    UciNewGame,
    Position { startpos: bool, fen: Option<String>, moves: Vec<String> },
    Go(SearchLimits),
    Stop,
    Quit,
}

pub fn parse_uci_line(line: &str) -> Result<UciCommand, UciParseError>;
pub fn run_uci_loop<R: BufRead, W: Write>(input: R, output: W, engine: impl Searcher) -> Result<(), UciError>;
```

Required options:

- `Hash` in MB, default 64, min 1, max 4096.
- `Threads`, default 1. Multi-threaded search is optional; accepting the option
  and running single-threaded is acceptable if documented.
- `Move Overhead` in ms, default 30.

### CLI Interface

```text
chess-engine uci
chess-engine perft --fen "<fen>" --depth 5
chess-engine perft-suite tests/fixtures/perft.epd --max-depth 5
chess-engine bench
chess-engine tactics tests/fixtures/tactics/*.epd --movetime 1000
chess-engine uci-smoke tests/fixtures/uci-smoke.txt
```

### GUI Interface

The GUI must be a local desktop app. It must communicate with the engine through
UCI, not through in-process private APIs. This preserves GUI independence and
keeps the engine verifiable by external UCI tools.

Required GUI features:

- Board rendering with legal move input by mouse.
- Play as white, black, or both sides.
- New game, flip board, undo, resign/reset.
- Engine status: searching/idle, depth, score, nodes, PV.
- Legal move highlighting.
- Promotion selection.
- UCI engine path setting with default to the workspace-built engine.
- No network panel, login, or cloud sync.

## 4. Tech Stack And Rationale

Recommended stack:

- Rust stable, pinned with `rust-toolchain.toml`.
- Cargo workspace with separate crates for core, perft, evaluation, search, UCI,
  engine binary, and GUI.
- `eframe`/`egui` for the desktop GUI unless a later architecture issue proves a
  better local option. It avoids a web server and can ship as a native macOS app.
- `serde`/`toml` only for local config files if needed.
- No database. Local files only.

Rust is preferred because move generation and search are CPU-bound, and Rust
provides predictable performance, strong type boundaries, and memory safety
without a garbage collector. The stable crate split gives Kensho agents
parallel work surfaces that can compile and test independently.

## 5. Acceptance Bar

### Perft Targets

The canonical fixture file is `tests/fixtures/perft.epd`. Counts are from
Chessprogramming Wiki Perft Results, last checked during project setup on
2026-07-07. The mandatory release gate is exact node equality for:

- Start position through depth 6.
- Kiwipete through depth 5.
- CPW positions 3, 4, 5, and 6 through depth 5.

Extended gates add Kiwipete depth 6, CPW position 4 depth 6, and CPW position 6
depth 6. Extended gates may run in a slower profile, but must be available.

| ID | FEN | Mandatory Depths | Node Counts |
| --- | --- | --- | --- |
| startpos | `rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1` | 1-6 | 20, 400, 8902, 197281, 4865609, 119060324 |
| kiwipete | `r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1` | 1-5 | 48, 2039, 97862, 4085603, 193690690 |
| cpw3 | `8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1` | 1-5 | 14, 191, 2812, 43238, 674624 |
| cpw4 | `r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1` | 1-5 | 6, 264, 9467, 422333, 15833292 |
| cpw5 | `rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8` | 1-5 | 44, 1486, 62379, 2103487, 89941194 |
| cpw6 | `r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10` | 1-5 | 46, 2079, 89890, 3894594, 164075551 |

Any wrong count is a real bug. No exception is allowed.

### UCI Compliance Checklist

The engine passes when a local harness proves:

- `uci` returns `id name`, `id author`, supported `option` rows, and `uciok`.
- `isready` returns `readyok`, including while idle after `ucinewgame`.
- `position startpos` and `position fen <fen>` both set legal positions.
- `position ... moves ...` applies long algebraic UCI moves including promotion.
- `go depth N` returns one `bestmove`.
- `go movetime X` respects X plus `Move Overhead`.
- `go wtime btime winc binc movestogo` searches and returns within budget.
- `stop` interrupts infinite or timed search and emits the current `bestmove`.
- `quit` exits cleanly.
- Valid commands produce no stderr.
- Invalid commands are reported or ignored without panic.

### Tactical Suite

The tactical suite uses EPD files with `bm` best-move tags. Acceptance:

- Minimum 300 tactical positions in `tests/fixtures/tactics/` before release.
- Solver command: `chess-engine tactics tests/fixtures/tactics/*.epd --movetime 1000`.
- Pass threshold: at least 85 percent exact best-move matches at 1 second per
  position on the developer laptop.
- Mate-in-one and forced recapture micro-suites must score 100 percent.
- The harness must print failed FENs, expected moves, returned move, depth,
  score, nodes, and PV.

### Strength Target

The objective target is a credible casual-human engine, aiming around 1800+
club-level Elo. Because no cloud service is allowed, strength is measured
locally:

- Required floor: beat the repo's deterministic baseline bots by at least 95
  percent over 200 games at 1+0.1 time control.
- Required floor: tactical suite threshold above.
- Required claim gate: a local `cutechess-cli` gauntlet file can run against
  UCI engines already present on the machine. No acceptance step may download
  opponents. The reported Elo claim must include opponent paths, versions,
  time control, openings file, game count, score, and error bars.

### GUI Acceptance

- App launches without a local server.
- A human can play a complete legal game against the engine.
- Illegal moves are rejected visually and never sent as legal moves to the engine.
- Promotion UI appears and sends the chosen promotion move.
- Engine search status and best line update during the engine turn.
- App can start a new game and can recover if the engine child process exits.

### Kensho Evaluation Acceptance

The Kensho evaluation is successful when:

- Work is sliced into local jobs with one clear issue per job.
- At least one worker and one reviewer are involved for every non-trivial slice.
- Review gates cite command output or exact file/line findings.
- No accepted slice regresses mandatory perft, UCI smoke, or existing unit tests.
- The final report can be reproduced from local commands and repository files.

## 6. Test Strategy By Module

- `chess-core`: unit tests for square parsing, bitboards, FEN round-trip,
  attack maps, pin/check cases, make/unmake invariants.
- `chess-perft`: fixture-driven exact perft, divide output for debugging,
  stats counters for captures, en-passant, castles, promotions, checks.
- `chess-eval`: symmetric positions evaluate to opposite scores after color
  flip; material deltas match expected centipawns; pawn-structure fixtures.
- `chess-search`: mate-in-one/two, stalemate avoidance, time-stop behavior,
  transposition-table replacement behavior, deterministic search under fixed seed.
- `chess-uci`: parser unit tests, transcript smoke tests, invalid command tests.
- `chess-engine`: CLI integration tests for `perft`, `perft-suite`, `uci-smoke`,
  `bench`, and `tactics`.
- `local-board`: GUI smoke tests for launch, move input, promotion dialog, engine
  process failure, and no network calls.

Perft is the central correctness gate. Search, eval, and GUI work cannot be
considered done if move generation is not exact.

## 7. Incremental Milestones

1. Kensho setup and spec freeze.
   - `.agent_team` local-only topology exists.
   - `SPEC.md`, fixtures, backlog, and repo instructions are committed.

2. Walking skeleton.
   - Rust workspace compiles.
   - Engine binary prints version and accepts `uci`.
   - Empty GUI window launches locally.

3. Board and FEN.
   - Bitboard position representation.
   - FEN parse/render.
   - Position validation and square/piece helpers.

4. Legal move generation and make/unmake.
   - All piece moves and special moves implemented.
   - Mandatory perft depths pass through startpos depth 5 and Kiwipete depth 4.

5. Full perft acceptance.
   - Mandatory perft suite passes.
   - Divide tooling exists for debugging mismatches.

6. UCI-compliant engine loop.
   - `uci`, `isready`, `position`, `go`, `stop`, and `quit` pass smoke tests.

7. Search.
   - Alpha-beta, iterative deepening, quiescence, TT, move ordering, time control.
   - Mate and tactical micro-suites pass.

8. Evaluation.
   - Material, PST, mobility, king safety, pawn structure, tempo.
   - Tactical threshold reaches release target.

9. GUI.
   - Playable local board using UCI subprocess.
   - Game controls and engine status.

10. Strength and release packaging.
    - Baseline bots defeated.
    - Optional local gauntlet report.
    - Release instructions and reproducible validation transcript.

Each milestone must be independently reviewable and must keep prior gates green.

## 8. First-Pass Epic To Issue Breakdown

Issue IDs are seeded in `backlog/README.md`. Each issue must remain small enough
for one worker branch and one reviewer pass.

### Epic A: Project Skeleton And Harness

- CHESS-001: Create Rust workspace, toolchain, formatting, and CI-equivalent local scripts.
- CHESS-002: Add command runner for all local acceptance gates.
- CHESS-003: Add fixture parser for perft EPD rows.
- CHESS-004: Add UCI transcript harness.
- CHESS-005: Add deterministic baseline bot harness.

### Epic B: Core Board Model

- CHESS-006: Square, color, piece, move, and bitboard primitives.
- CHESS-007: FEN parse/render with structured errors.
- CHESS-008: Position invariants and Zobrist hashing.
- CHESS-009: Make/unmake for quiet moves, captures, and clocks.
- CHESS-010: Make/unmake for castling, en-passant, and promotion.

### Epic C: Move Generation

- CHESS-011: Pawn move generation.
- CHESS-012: Knight and king move generation.
- CHESS-013: Sliding-piece attack generation.
- CHESS-014: Attack detection and check state.
- CHESS-015: Legal move filtering for pins and check evasions.
- CHESS-016: Castling legality.
- CHESS-017: En-passant legality, including discovered check.
- CHESS-018: Perft divide and stats.
- CHESS-019: Mandatory perft suite pass.

### Epic D: UCI And CLI

- CHESS-020: UCI parser and command model.
- CHESS-021: UCI engine loop and option handling.
- CHESS-022: CLI commands for perft, perft-suite, bench, and uci-smoke.
- CHESS-023: Robust invalid-command behavior.

### Epic E: Search And Evaluation

- CHESS-024: Basic negamax alpha-beta.
- CHESS-025: Iterative deepening and PV reporting.
- CHESS-026: Quiescence search.
- CHESS-027: Transposition table.
- CHESS-028: Move ordering heuristics.
- CHESS-029: Time management and stop token.
- CHESS-030: Material and piece-square evaluation.
- CHESS-031: Mobility, king safety, and pawn structure.
- CHESS-032: Tactical suite harness and threshold pass.

### Epic F: GUI

- CHESS-033: Local GUI shell and board renderer.
- CHESS-034: Mouse move input, legal highlighting, and promotion picker.
- CHESS-035: UCI subprocess adapter.
- CHESS-036: Game controls and engine status panel.
- CHESS-037: GUI smoke tests.

### Epic G: Release And Evaluation Report

- CHESS-038: Baseline bot tournament runner.
- CHESS-039: Optional local UCI gauntlet runner.
- CHESS-040: Reproducible release validation transcript.
- CHESS-041: Kensho evaluation report with throughput, reviews, bounces, and gates.

## 9. Risks And Mitigations

- Risk: Move generation bugs hide until late.
  - Mitigation: perft-first milestones, divide tooling, and mandatory fixture gates
    before search/GUI work is accepted.

- Risk: Agents collide in shared files.
  - Mitigation: crate/module ownership by epic; issue descriptions name expected
    files; reviewers reject broad drive-by rewrites.

- Risk: Search becomes non-deterministic and hard to review.
  - Mitigation: deterministic single-thread default, fixed seeds for randomized
    test helpers, and stable transcript output for tests.

- Risk: GUI bypasses UCI and couples to engine internals.
  - Mitigation: GUI acceptance requires UCI subprocess communication.

- Risk: Strength target becomes subjective.
  - Mitigation: tactical thresholds, baseline tournament floor, and optional
    local gauntlet reports with all parameters recorded.

- Risk: Product accidentally depends on network or hosted services.
  - Mitigation: no provider integrations in `.agent_team`, no runtime network
    features, GUI smoke tests for local launch, and release checklist includes a
    no-network run.

- Risk: Kensho appears successful from logs but not from product quality.
  - Mitigation: final evaluation is tied to perft, UCI, tactical, baseline, and
    GUI gates rather than narrative summaries.
