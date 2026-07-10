# Project Instructions

This repo is a local-only benchmark for Kensho/agent-team. All agents must treat
[SPEC.md](SPEC.md) as the source of truth.

## Hard Rules

- The chess engine and GUI must run locally on macOS.
- No runtime cloud services, hosted APIs, accounts, telemetry collectors, or network calls.
- No Linear, GitHub Issues, GitHub Projects, or external PM workflow is required for this repo.
- All Kensho agents must use the local Codex CLI through
  `scripts/codex-chatgpt-only.sh`, which forces ChatGPT subscription auth.
- Never use `OPENAI_API_KEY`, `CODEX_API_KEY`, API-key login, or Platform
  usage-based auth for agents in this repo.
- File detailed local feedback about Kensho itself with
  `agent-team feedback submit --route local --category <category> "<one sentence>"`.
  Use `friction`, `bug`, `idea`, `docs`, or `incident`, and be candid about
  what felt confusing, brittle, slow, helpful, or trust-building.
- Keep implementation work behind the module boundaries and interfaces in `SPEC.md`.
- Prefer small, reviewable branches that satisfy one backlog issue at a time.
- Do not weaken or remove acceptance tests to make a slice pass.
- Do not introduce large dependencies unless the spec explicitly allows them or the issue justifies them.
- Use `rg` for code search when available.

## Expected Validation

As the codebase appears, the normal validation target must become:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p chess-engine -- perft-suite tests/fixtures/perft.epd
cargo run -p chess-engine -- uci-smoke tests/fixtures/uci-smoke.txt
```

If a command is not available yet, the worker must state which milestone owns it
and run the strongest currently available local validation instead.

## Work Style

Workers implement. Reviewers review. Auditors file issues or findings. Managers
coordinate and approve. Avoid cross-role edits that hide defects from review.
