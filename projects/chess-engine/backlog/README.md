# Local Backlog Seed

This backlog mirrors the issue breakdown in `SPEC.md`. The repo uses
`pm.provider = "none"`, so these issue IDs are local identifiers for Kensho jobs
rather than Linear or GitHub tickets.

Dispatch pattern:

```sh
agent-team job create CHESS-001 \
  --id chess-001 \
  --pipeline local_slice \
  --kickoff "Implement CHESS-001 from backlog/README.md and SPEC.md." \
  --dispatch \
  --workspace worktree
```

Do not add `--runtime codex` to dispatch commands unless you also pass
`--runtime-bin scripts/codex-chatgpt-only.sh`.
Omitting runtime flags uses the repo default wrapper and ChatGPT subscription
auth.

## Epic A: Project Skeleton And Harness

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-001 | Rust workspace, toolchain, formatting | `cargo metadata`, `cargo fmt --check`, and empty `cargo test --workspace` pass locally. |
| CHESS-002 | Local acceptance runner | One command runs available local gates and reports missing future gates as pending. |
| CHESS-003 | Perft fixture parser | Reads `tests/fixtures/perft.epd` and exposes exact expected counts by ID/depth. |
| CHESS-004 | UCI transcript harness | Runs an engine subprocess against `tests/fixtures/uci-smoke.txt` and validates required tokens. |
| CHESS-005 | Baseline bot harness | Runs deterministic baseline games locally and emits PGN plus score summary. |

## Epic B: Core Board Model

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-006 | Core primitives | Square, color, piece, bitboard, and move unit tests pass. |
| CHESS-007 | FEN parse/render | All perft FENs round-trip exactly where canonical formatting permits. |
| CHESS-008 | Position invariants and Zobrist | Validation catches malformed positions; identical positions hash identically. |
| CHESS-009 | Quiet/capture make-unmake | Random legal quiet/capture sequences restore original positions. |
| CHESS-010 | Special move make-unmake | Castling, en-passant, and promotion make/unmake tests pass. |

## Epic C: Move Generation

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-011 | Pawn generation | Pawn push, capture, double push, and promotion fixtures pass. |
| CHESS-012 | Knight and king generation | Knight, king, and king-adjacency tests pass. |
| CHESS-013 | Sliding attacks | Bishop, rook, queen blockers and captures pass. |
| CHESS-014 | Attack detection | Check, double check, and attacked-square fixtures pass. |
| CHESS-015 | Legal filters | Pins and check-evasion fixtures pass. |
| CHESS-016 | Castling legality | Rights, blockers, in-check, through-check, and rook movement pass. |
| CHESS-017 | En-passant legality | EP including discovered-check illegality passes. |
| CHESS-018 | Perft divide and stats | Divide output sums to perft and stats counters match fixtures where provided. |
| CHESS-019 | Mandatory perft suite | Mandatory perft rows in `tests/fixtures/perft.epd` pass exactly. |

## Epic D: UCI And CLI

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-020 | UCI parser | Parser tests cover required commands and invalid input. |
| CHESS-021 | UCI engine loop | `uci`, `isready`, `position`, `go`, `stop`, and `quit` smoke tests pass. |
| CHESS-022 | CLI commands | `perft`, `perft-suite`, `bench`, and `uci-smoke` commands exist. |
| CHESS-023 | Invalid command robustness | Malformed UCI never panics and does not corrupt engine state. |

## Epic E: Search And Evaluation

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-024 | Basic alpha-beta | Finds mate-in-one and wins hanging queen over material-only eval. |
| CHESS-025 | Iterative deepening | Emits stable `info depth` and PV lines before `bestmove`. |
| CHESS-026 | Quiescence | Reduces obvious horizon failures in capture fixtures. |
| CHESS-027 | Transposition table | TT tests cover exact/lower/upper replacement and hashfull reporting. |
| CHESS-028 | Move ordering | Node counts improve over unordered search on fixed tactical fixtures. |
| CHESS-029 | Time management | `go movetime`, clock controls, and `stop` meet timing tolerances. |
| CHESS-030 | Material and PST eval | Symmetry and material delta tests pass. |
| CHESS-031 | Positional eval | Mobility, king safety, and pawn-structure fixtures pass. |
| CHESS-032 | Tactical threshold | EPD tactical suite reaches the release threshold. |

## Epic F: GUI

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-033 | GUI shell and board | Local app launches and renders a board with pieces. |
| CHESS-034 | Move input | Mouse input, legal highlighting, and promotion picker work. |
| CHESS-035 | UCI subprocess | GUI talks to engine only through UCI. |
| CHESS-036 | Game controls | New game, flip, undo, reset, and engine status work. |
| CHESS-037 | GUI smoke tests | Local GUI smoke path passes without network or server. |

## Epic G: Release And Evaluation Report

| ID | Title | Acceptance |
| --- | --- | --- |
| CHESS-038 | Baseline tournament | Engine beats deterministic baselines by required margin. |
| CHESS-039 | Optional UCI gauntlet | Local-only gauntlet runner records reproducible Elo evidence when opponents exist. |
| CHESS-040 | Release transcript | A single local transcript proves all release gates. |
| CHESS-041 | Kensho evaluation report | Report summarizes jobs, reviews, bounces, gate failures, and final product quality. |
