# Kensho Chess-Engine Retrospective

Date of analysis: 2026-07-07
Product repo: `projects/chess-engine`
Kensho source repo: external Kensho source checkout, not published here

## Project Overview, Motivation, And Requirements

This chess-engine project was not only a product build. It was a dogfood evaluation of Kensho as an agent-team framework: could a configured organization of AI workers, adversarial reviewers, a manager, and scheduled loops decompose a non-trivial systems project, preserve module boundaries, enforce objective gates, and ship locally reproducible software rather than plausible-looking prose?

The human setup was intentionally lightweight. It was basically two instructions:

1. Ask Codex to set up the Kensho environment and create an agent-team template for a chess engine.
2. Tell the team to get started, and to message the human only for project submissions or meaningful checkpoint decisions.

That matters because the evaluation was not "can a human manually shepherd every implementation detail?" It was closer to "can a bootstrapped agent organization take a clear local spec, run mostly autonomously, use review gates, and surface only the decisions that need a project-level owner?"

Chess is a good test domain for that because it has hard correctness oracles and clear integration seams. Move generation can be judged by exact perft node counts; UCI behavior can be checked with transcripts; search can be tested against tactical positions; GUI work can be smoke-tested through local user workflows; and strength claims can be constrained by local tournaments rather than subjective demos. A team that "looks productive" but gets chess rules wrong fails immediately.

The product target was a complete local Rust chess stack:

- A bitboard-based `chess-core` with FEN parse/render, legal move generation, special moves, attack detection, make/unmake, validation, and perft.
- A deterministic evaluator/searcher with alpha-beta, iterative deepening, quiescence, transposition table support, move ordering, time controls, and stop responsiveness.
- A UCI-compliant `chess-engine` executable with parser, protocol loop, CLI commands, UCI smoke harness, tactics harness, perft suite, bench, and baseline tournament runner.
- A native local desktop GUI (`apps/local-board`) that renders a board, accepts legal mouse moves, rejects illegal moves, handles promotion/new/undo/reset/flip/play-mode controls, and talks to the engine through a UCI subprocess rather than private in-process APIs.
- A release harness that proves local-only behavior: no cloud services, hosted APIs, account login, remote engines, telemetry backend, or network dependency.

The acceptance bar was intentionally objective: mandatory standard perft rows, UCI compliance checks, tactical-suite thresholds, baseline tournament floor, GUI launch and workflow smoke tests, and honest release reporting. That matters for the retrospective because it makes "did the agent organization actually build working software?" measurable rather than aesthetic.

## Verdict

A chartered Kensho agent organization did build a real, local, working chess engine and GUI end-to-end. The product builds, passes workspace tests, passes the mandatory perft bar I ran, speaks UCI, launches the GUI process, passes a GUI subprocess smoke path, and plays legal non-trivial moves.

It did not meet the full original strength/release ambition. The repo contains only a 2-position tactical micro-suite rather than the required 300+ tactical positions, has no cutechess opponent evidence, and should not claim 1800+ Elo or strong club-level play. Process-wise, the adversarial review gate earned its keep, but the team ran effectively serially and required manual approval, bounce repair, and manager nudges.

## Evidence Summary

Commands were run from `projects/chess-engine`:

| Area | Command | Result |
| --- | --- | --- |
| Acceptance runner | `scripts/acceptance.sh` | Exit 0. `required_failures: 0`, `pending_or_future_gates: 3`. It ran fmt, clippy, workspace tests, UCI smoke, perft depth 4, 6-game baseline smoke, GUI subprocess smoke, optional gauntlet availability. |
| Tests | included in `scripts/acceptance.sh` via `cargo test --workspace` | 49 unit tests passed across core, engine, eval, perft, search, UCI, and local-board; doc-test stubs passed. |
| Full mandatory perft, depth <=5 | `cargo run --release -p chess-engine -- perft-suite tests/fixtures/perft.epd --max-depth 5` | 30 cases passed, 0 failures. Covers all mandatory positions through depth 5. |
| Startpos depth 6 | `cargo run --release -p chess-engine -- perft --fen "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1" --depth 6` | `nodes 119060324`, matching the mandatory oracle. |
| Tactical micro-suite | `cargo run --release -p chess-engine -- tactics tests/fixtures/tactics/search-micro.epd --movetime 1000` | 2 cases passed, 0 failures. This is not the full tactical gate. |
| Baseline strength floor | `cargo run --release -p chess-engine -- baseline-tournament --games 200 --max-plies 120 --engine-depth 3 --out-dir target/release-evidence/baseline-tournament-200` | 200 wins, 0 draws, 0 losses; score 400/400 against first-legal, capture-greedy, and pawn-push baselines. This is a weak deterministic floor, not an Elo claim. |
| UCI play probe | piped `uci`, `isready`, three `position ... go depth 3` commands into `target/release/chess-engine uci` | Legal best moves returned: `b8c6`, `g8f6`, `f6e4`; info/PV lines emitted. |
| GUI smoke | from acceptance: `cargo run -p local-board -- --smoke --engine target/debug/chess-engine` | Passed: legal move, illegal rejection, promotion, new/undo/reset, UCI handshake, bestmove/status parsing, child restart. |
| Native GUI build | `cargo build --release -p local-board` | Passed. |
| Native GUI launch | wrapper launched `target/release/local-board` for 5 seconds, then terminated it | Process stayed up, no stdout/stderr startup error. I did not capture pixels or manually interact with the native window. |
| Outcomes report | `agent-team outcomes report` | 6 jobs, 6 done, 0 failed, effective concurrency 1.00, peak concurrency 1, capacity 6, 5 bounces, 20 review rounds, 73,195,960 tokens. |
| Epic view | `agent-team outcomes report --by-epic` | Failed: `unknown flag: --by-epic`. |
| Feedback | `agent-team feedback ls` plus local/upstream feedback files | 21 local chess feedback items and several upstream Kensho follow-up items found. |

Checks I did not complete:

| Missing check | Reason and impact |
| --- | --- |
| Extended perft depth 6 for Kiwipete/CPW rows | The mandatory release bar is covered without the multi-billion-node extended rows. Extended rows remain optional/slow evidence. |
| Full tactical threshold | Only 2 tactical positions exist under `tests/fixtures/tactics`; SPEC requires at least 300 and >=85% at 1000 ms. |
| Cutechess/Elo gauntlet | `scripts/uci-gauntlet.sh` skipped because `cutechess-cli` is not installed and no local opponents were configured. |
| Visual GUI QA by screenshot | The process launch succeeded and smoke passed, but I did not verify rendered pixels. Upstream feedback says a manager's macOS screencapture attempt failed with `could not create image from display`. |

## Scorecard

### Delivery

| Dimension | Score | Evidence |
| --- | --- | --- |
| Build and tests | Pass | `scripts/acceptance.sh` passed fmt, clippy, and `cargo test --workspace`; 49 unit tests passed. |
| Move-generation correctness | Strong pass for mandatory bar | Perft through all mandatory depth-5 rows passed; startpos depth 6 returned 119,060,324. |
| UCI protocol | Pass for tested surface | UCI smoke passed 20 commands and 15 expected tokens; scripted play returned legal best moves and info/PV. Stop behavior has explicit tests and review probes. |
| Search/tactics | Partial | Finds mate/winning queen in the 2-position micro-suite. No 300-position suite, no serious tactical threshold evidence. |
| Baseline strength | Partial floor | 200/200 wins against weak deterministic baselines at depth 3. This proves it is not random or trivial, but not club strength. |
| GUI | Partial/pass | Native release binary builds and starts; headless smoke verifies local board state, UCI subprocess, promotion, illegal move rejection, and child restart. No visual/pixel QA. |
| Local-only constraint | Pass from scan and behavior | No runtime network/cloud requirement found; UCI gauntlet intentionally skips without local tools. |
| Code quality | Real implementation with rough edges | Real bitboard core, perft, search, UCI, and egui GUI exist. Rough edges: very large single-file modules, limited tactical corpus, acceptance defaults weaker than spec, GUI visual verification thin. |

### Process Efficiency

Official outcome totals:

| Metric | Value |
| --- | ---: |
| Jobs | 6 |
| Done / failed | 6 / 0 |
| Total tokens | 73,195,960 |
| Runtime duration sum | 11,308,273 ms, about 188.5 min |
| Wall-clock from first job created to final pipeline done | 2026-07-07 11:04:20Z to 14:55:34Z, about 3h51m |
| Effective concurrency / peak concurrency | 1.00 / 1 |
| Configured worker replicas / reviewer replicas | 6 / 3 |
| Review rounds | 20 |
| Outcome bounce count | 5 |
| Gate-file substantive failed review signatures | 4 |

Difficulty-normalized slice data:

| Slice | Difficulty class | Pipeline cycle time | Tokens | Review rounds | Bounces | Notes |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| `chess-001` scaffold | Simple | 8.7m | 2.48M | 2 | 0 | Workspace skeleton only. |
| `chess-core-001` core/perft | Hard correctness | 98.2m | 36.31M | 6 | 3 | Dominated cost. Review caught en-passant bugs outside canonical perft. |
| `chess-search-001` eval/search | Medium-hard | 50.2m | 10.01M | 4 | 1 | Too broad; bounced for missing fixture-backed tactical support. |
| `chess-uci-cli-001` UCI/CLI | Medium-hard | 29.3m | 14.60M | 4 | 1 | Bounced for timed/clock `stop` non-interruptibility. |
| `chess-gui-001` GUI | Medium | 24.6m | 5.13M | 2 | 0 | Needed manager nudges during implementation but passed review. |
| `chess-release-001` release harness | Medium | 16.4m | 4.66M | 2 | 0 | Made pending gates explicit instead of hiding them. |

The hard core slice consumed about half of all project tokens and most bounces. The configured parallel capacity did not translate into throughput because the manager dispatched coarse serial slices and the pipeline had a manual approval gate after each review.

### The Adversarial Gate

The gate earned its keep.

Substantive review failures before pass:

| Job | Gate finding | Type | Outcome |
| --- | --- | --- | --- |
| `chess-core-001` | En-passant target occupancy bug | Real chess correctness bug | Fixed before merge. |
| `chess-core-001` | En-passant origin square not validated | Real chess correctness bug outside standard perft | Fixed before merge. |
| `chess-search-001` | Missing tactical fixture support | Acceptance/test coverage gap | Fixed before merge by adding EPD `bm` parser and micro-suite. |
| `chess-uci-cli-001` | Timed UCI stop not interruptible | Real protocol compliance bug | Fixed before merge with async input/search handling. |

Escaped defects:

- I found no product defect that was approved in one merged slice and then fixed in a later merged product slice. The fixes above happened before the relevant merge commits.
- There were unproven release claims/gaps, but `CHESS-RELEASE-001` explicitly marked them pending rather than presenting them as passed. That is not an escaped defect; it is incomplete acceptance coverage.
- The strongest escaped-defect class was in Kensho itself, not the chess product: bounce/re-dispatch and deliverable-verifier races surfaced during the project and were then fixed upstream.

## What Worked

1. Objective gates drove real quality. Perft forced the core to a strong baseline. Reviewers went beyond canonical perft and found malformed en-passant FEN cases that would otherwise have shipped.

2. The module split was useful. Even though the work was serialized, the crate layout made later UCI, search, and GUI work build on stable local APIs.

3. Review findings were concrete and actionable. The UCI bounce included an exact repro: `go movetime 2000`, `stop`, `quit` should return promptly but did not. The fix added unit and transcript coverage.

4. Local-only constraints held. The product did not require cloud services, API keys, accounts, or hosted engines. Optional cutechess was correctly skipped.

5. The release harness was honest. `scripts/acceptance.sh` did not pretend depth-4 perft, a 2-position tactical suite, or a 6-game smoke tournament satisfied the full spec.

6. Feedback routing produced durable framework signal. Chess-related feedback directly produced or connected to upstream Kensho work, including GH198 for bounce/re-dispatch races and GH192 for report deliverable contracts.

## What Did Not Work

1. The team was over-provisioned but effectively serial. Outcomes report: effective concurrency 1.00, peak 1, capacity 6. Six worker replicas, three reviewer replicas, and one auditor did not improve throughput.

2. The initial slices were too large. `CHESS-CORE-001` bundled board model, FEN, make/unmake, all legal move generation, and perft into one huge branch; `CHESS-SEARCH-001` bundled CHESS-024 through CHESS-032. This inflated review cost and made bounces expensive.

3. Bounce retry state was brittle. Multiple feedback items report retry workers starting from clean `main` instead of the rejected commit, requiring manual cherry-picks. UCI retry needed explicit manager messages to preserve the first implementation.

4. The deliverable verifier confused reviewer/report roles with implementation roles. During `CHESS-UCI-CLI-001`, an exited reviewer emitted `deliverable_missing` and flipped the job failed even though a bounced worker was running. This is a framework bug, not a worker quality problem.

5. Manual manager approvals serialized every slice. Each of the 6 jobs has `manual_gate_approved`. No slice merged autonomously after a passing review.

6. GUI implementation had an idle/partial-edit incident. Events show two manager messages after the worker had only deleted `apps/local-board/src/lib.rs` and appeared idle. It recovered, but only after intervention.

7. Review/agent environments lacked reliable context variables. Feedback repeatedly cites missing `MAIN_REPO`, missing `AGENT_TEAM_PIPELINE_STEP`, and reviewers needing to infer branch/worktree targets from job TOML/events.

8. Validation UX was poor for long perft. Agents reported 9-10 minute perft runs with no per-row progress output, making healthy work hard to distinguish from a hang.

9. The final product still lacks the tactical data needed for a serious strength claim. Two tactical positions are not enough to characterize engine strength.

10. The native GUI was not visually verified. Build, launch, and smoke evidence are meaningful, but they do not prove layout quality.

## Topology Findings

### Used vs. Idle

Used:

- `worker`: used for all six implementation jobs.
- `reviewer`: used for all six review jobs.
- `manager`: used as manual approval/merge/bounce coordinator.

Idle or mostly unused:

- `auditor`: configured with one replica and a weekly schedule, but the project completed the same day; no auditor work appears in outcomes.
- `weekly-audit`: configured, did not fire.
- Extra worker/reviewer replicas: configured but unused; peak concurrency was 1.
- `comms` and `ticket-manager` agent files existed from the template but were not part of `teams.chess`.

### Binding Bottleneck

The bottleneck was not worker capacity. It was the combination of:

- Manager/manual approval after every review.
- Serial dispatch of coarse slices.
- Bounce retry mechanics that lost branch context.
- Objective validation time concentrated inside giant slices.

The cargo lock was not the binding constraint in the final run. The topology configured two cargo lock slots, but only one job ran at a time.

### Static Configuration Cost

The team did not evolve its topology during the project. It kept the static initial configuration: 6 workers, 3 reviewers, 1 auditor, and one `local_slice` pipeline. Concrete costs:

- Dead capacity: effective concurrency 1.00 with 6 worker capacity.
- Overhead: every job carried enough prompt/process machinery for a larger team without parallel work to amortize it.
- Poor slice fit: core/search were too large, but topology did not split or specialize roles midstream.
- No automatic verifier lane: objective gate running was left to workers/reviewers and acceptance scripts, not a dedicated deterministic verifier step.

### Missing Capabilities

- A deterministic verifier step that runs product gates, streams progress, and writes machine-readable evidence before LLM review.
- Bounce retry semantics that preserve the rejected branch/commit as the base.
- Branch/worktree handoff metadata that reviewers can consume without inference.
- Role-aware deliverable contracts so no-code reviewers and report-only jobs are not judged like implementation branches.
- A lightweight GUI visual QA facility for native apps.
- Dynamic topology or scheduler logic to shrink/expand teams and split monster tickets.

## Hypotheses For Better Bootstrapping

These are testable A/B hypotheses for a self-contained systems-programming project with objective local gates.

1. Start with fewer idle replicas: 3 workers, 2 reviewers, 1 verifier, 1 manager beats 6 workers, 3 reviewers, 1 auditor for projects under one day.
   - Test: replay the same chess backlog with baseline topology vs. slim topology.
   - Measure: effective concurrency, wall-clock, tokens per merged slice, bounces, human interventions.
   - Expected result: same or better delivery quality, lower token/context overhead, less idle capacity.

2. Split core into staged correctness slices before search/GUI.
   - Proposed split: primitives/FEN; make/unmake; pseudo-legal generation; legal filtering/special moves; perft suite/perft-divide.
   - Test against the current one-shot `CHESS-CORE-001`.
   - Expected result: fewer tokens per review round and cheaper bounces. The risk is more integration overhead; the verifier lane should control that.

3. Add a machine verifier step before LLM review.
   - Verifier runs fmt, clippy, tests, perft with progress, UCI smoke, tactical suite as applicable, and writes `target/agent-evidence/<job>.json`.
   - Test: compare reviewer token use and bounce precision with and without verifier evidence.
   - Expected result: reviewers spend less time rediscovering command output and more time checking correctness gaps outside the scripted gates.

4. Preserve rejected branch state on bounce.
   - On bounce, redispatch the worker on top of the rejected commit and include an explicit previous-commit field.
   - Test: inject a two-round UCI-style bounce and measure manual messages/cherry-picks.
   - Expected result: eliminates the clean-main retry failure seen in `chess-search-001`, `chess-core-001`, and `chess-uci-cli-001` feedback.

5. Replace always-manual approval with policy-gated auto-merge for low-risk slices.
   - Auto-merge when verifier is green, reviewer gate is pass, diff scope matches ticket, and no release claim is involved.
   - Keep manual approval for core legality, release claims, failed reviews, and GUI visual acceptance.
   - Test: compare wall-clock and escaped defects.
   - Expected result: lower cycle time without lowering quality if the verifier gate is strong.

6. Make progress output part of long-running gate contracts.
   - Perft suite should print each row as it completes and support `--phase mandatory`.
   - Test: measure false stale/manager nudge rates on long validation.
   - Expected result: fewer stalled-worker ambiguities and better reviewer trust.

7. Require a real tactical corpus before claiming search strength.
   - Bootstrap should seed 300 tactical EPD positions or explicitly scope strength claims out.
   - Test: compare final release honesty and search quality.
   - Expected result: fewer overclaims and stronger search regressions.

## Recommended Starter Topology

For this project archetype, I would bootstrap a smaller, verifier-heavy topology. The important changes are: fewer replicas, a deterministic verifier step, branch-preserving bounce behavior, and an acceptance auditor that runs after merges rather than weekly.

```toml
# Recommended local-only topology for a self-contained Rust engine project.

[locks.cargo]
slots = 2
scope = "team"

[channels.supervisor]
scope = "team"

[channels.review]
scope = "team"

[instances.manager]
agent       = "manager"
ephemeral   = false
brief       = true
runtime     = "codex"
runtime_bin = "scripts/codex-chatgpt-only.sh"
description = "Owns backlog slicing, dependency ordering, merge policy, and release claims."

[[instances.manager.triggers]]
event = "user_invocation"

[[instances.manager.triggers]]
event        = "agent.dispatch"
match.target = "manager"

[instances.worker]
agent        = "worker"
ephemeral    = true
replicas     = 3
runtime      = "codex"
runtime_bin  = "scripts/codex-chatgpt-only.sh"
token_budget = "30M"
time_budget  = "45m"
description  = "Implements one narrow issue in an isolated worktree and commits local changes."

[[instances.worker.triggers]]
event        = "agent.dispatch"
match.target = "worker"

[instances.verifier]
agent        = "verifier"
ephemeral    = true
replicas     = 1
runtime      = "codex"
runtime_bin  = "scripts/codex-chatgpt-only.sh"
token_budget = "4M"
time_budget  = "20m"
description  = "Runs deterministic local gates and writes machine-readable evidence; does not edit product code."

[[instances.verifier.triggers]]
event        = "agent.dispatch"
match.target = "verifier"

[instances.reviewer]
agent        = "reviewer"
ephemeral    = true
replicas     = 2
runtime      = "codex"
runtime_bin  = "scripts/codex-chatgpt-only.sh"
token_budget = "10M"
time_budget  = "30m"
description  = "Adversarial reviewer. Checks diff, evidence, SPEC.md, and edge cases outside scripted gates."

[[instances.reviewer.triggers]]
event        = "agent.dispatch"
match.target = "reviewer"

[instances.acceptance_auditor]
agent        = "auditor"
ephemeral    = true
replicas     = 1
runtime      = "codex"
runtime_bin  = "scripts/codex-chatgpt-only.sh"
token_budget = "8M"
time_budget  = "30m"
description  = "Runs after merge/release checkpoints; verifies claims are not stronger than evidence."

[[instances.acceptance_auditor.triggers]]
event        = "chess.release.candidate"
match.kind   = "acceptance_audit"

[pipelines.local_slice]
trigger.event = "chess.slice.ready"
auto_advance  = true
redispatch_on_reentry = true
bounce_base = "rejected_branch"

[[pipelines.local_slice.steps]]
id           = "implement"
target       = "worker"
workspace    = "worktree"
timeout      = "45m"
token_budget = "30M"
time_budget  = "45m"
locks        = ["cargo"]
max_attempts = 1
instructions = """
Implement exactly one narrow issue. Commit locally. Do not weaken existing
gates. Record branch, commit, commands, and results.
"""

[[pipelines.local_slice.steps]]
id           = "verify"
target       = "verifier"
after        = ["implement"]
workspace    = "repo"
timeout      = "20m"
token_budget = "4M"
time_budget  = "20m"
locks        = ["cargo"]
instructions = """
Checkout the worker commit in a temporary verification worktree. Run the issue's
declared deterministic gates with progress output. Write evidence JSON and a
short markdown summary. Do not edit product code.
"""

[[pipelines.local_slice.steps]]
id           = "review"
target       = "reviewer"
after        = ["verify"]
workspace    = "repo"
timeout      = "30m"
token_budget = "10M"
time_budget  = "30m"
retry_on_crash = true
max_attempts   = 1
instructions = """
Review the diff and verifier evidence against SPEC.md. Bounce only for real
content, correctness, acceptance, or scope defects. Cosmetic/mechanical failures
belong to verifier gates.
"""

[[pipelines.local_slice.steps]]
id        = "approve"
target    = "manager"
after     = ["review"]
workspace = "repo"
gate      = "policy"
instructions = """
Auto-approve narrow slices when verifier and reviewer pass and diff scope is
clean. Require manual approval for release claims, failed reviews, core legality
changes, and GUI visual acceptance.
"""

[teams.chess]
description = "Local chess-engine team with deterministic verification first."
instances   = ["manager", "worker", "verifier", "reviewer", "acceptance_auditor"]
pipelines   = ["local_slice"]
channels    = ["supervisor", "review"]
```

Why each role exists:

- `worker`: still needed for code.
- `verifier`: separates command execution from adversarial reasoning and gives reviewers trustworthy evidence.
- `reviewer`: proved valuable by finding real bugs outside scripted gates.
- `manager`: should slice work and adjudicate release claims, not manually approve every green low-risk slice.
- `acceptance_auditor`: should run at release checkpoints, not weekly, because short projects finish before weekly schedules matter.

## Framework Bug Ledger

| Bug or gap | Evidence from this project | Severity | Attribution | Status observed in Kensho source |
| --- | --- | --- | --- | --- |
| Bounce/re-dispatch race can false-fail active bounced jobs | Upstream feedback `fb-20260707t135735...`: after bouncing `CHESS-UCI-CLI-001`, exited reviewer emitted `deliverable_missing` and flipped job failed while bounced worker was running. Local events show two `deliverable_missing` entries for reviewer. | High | Kensho framework | Fixed upstream by GH198 / PR #201, commit `12b16b08 fix(daemon): bounce/re-dispatch races`. |
| Bounce retry worktree starts from clean `main` instead of rejected commit | Feedback for `chess-core-001`, `chess-search-001`, and `chess-uci-cli-001`; local UCI event messages instructed worker to cherry-pick prior implementation. | High | Kensho framework | Fixed upstream by GH198 / PR #201 according to source git log. |
| Deliverable verifier applies branch/PR expectations to no-code reviewer/report roles | `CHESS-UCI-CLI-001` reviewer got `deliverable_missing`; upstream GH192 says report-only/research jobs were blocked by PR-style deliverable expectations. | High | Kensho framework | Report contracts fixed by GH192 / PR #194, commit `5e9c99b3`. Reviewer false-fail covered by GH198. |
| Missing `MAIN_REPO` and pipeline env vars in agent/reviewer contexts | Many local feedback items: CHESS-001, core, search, release reviewers had to infer repo and step before recording gates. | Medium | Kensho framework/config | Not confirmed fixed in source during this analysis. |
| Review handoff opens on `main` while deliverable is in separate worktree/branch | GUI, core, release feedback; reviewers had to inspect job TOML/events to find target. | Medium | Kensho framework | Partly addressed by bounce/branch work, but I did not confirm a general reviewer UX fix. |
| `agent-team outcomes report --by-epic` unavailable in installed CLI | Command returned `unknown flag: --by-epic`. | Medium | Kensho reporting gap | Source log shows later `feat(outcomes): epic-level spend attribution`, but the CLI used here did not expose it. |
| Long objective gates provide no progress | Feedback: perft ran 9-10 minutes with no per-row output during agent work. | Medium | Product harness plus framework UX | Open in chess product unless perft suite gains progress/phase output. |
| Native GUI visual QA not automatable from Codex session | Upstream feedback says macOS screencapture failed after native eframe launch. | Medium | Tooling/environment gap | Open. |
| GUI worker partial edit/idleness required manager nudges | Local events at 14:18 and 14:19 warned worker that `lib.rs` was deleted and no replacement source existed. | Medium | Model/worker execution plus missing watchdog | Product recovered; no framework fix confirmed. |
| Rust toolchain/shared cache friction | Feedback says concurrent cargo metadata/fmt/test against `stable` triggered rustup component rename conflicts; my first sandboxed analysis run also hit `sccache: Operation not permitted` until permissions changed. | Low/Medium | Environment/config | Pinning installed toolchain and avoiding sandboxed sccache solved analysis run; no general Kensho fix confirmed. |

## Final Assessment

The outcome is good for correctness-first local software, but not good enough for the original strength marketing bar. The project produced a functioning chess engine with objectively correct mandatory move generation, useful UCI behavior, and a local GUI path. The biggest product gap is not "does it work"; it does. The gap is strength evidence and polished GUI validation.

The process lesson is sharper: the review gate was high-value, while the topology was too heavy and too static. For this project shape, the next bootstrap should be smaller, verifier-first, branch-preserving on bounces, and more aggressive about narrow slices. The highest-leverage framework fixes are already visible from the dogfood: fix bounce/re-dispatch state, make deliverable contracts role-aware, pass reliable repo/step context into agents, and add progressful deterministic gate evidence before LLM review.
