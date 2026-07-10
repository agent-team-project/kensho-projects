# Local Chess Engine Kensho Evaluation

This repository is a local-only Kensho evaluation project. The goal is to build a
playable chess engine with a GUI while measuring whether Kensho can coordinate a
parallel agent organization against objective, machine-checkable gates.

Primary entry points:

- [SPEC.md](SPEC.md): build-ready specification and acceptance contract.
- [CLAUDE.md](CLAUDE.md): repo rules for agents working here.
- [backlog/README.md](backlog/README.md): first-pass epic and issue seed.
- `scripts/acceptance.sh`: single local runner for the current release gates.
- `scripts/release-transcript.sh`: captures the acceptance runner transcript
  under `target/release-evidence/`.
- [tests/fixtures/perft.epd](tests/fixtures/perft.epd): canonical move-generation oracle data.
- [tests/fixtures/uci-smoke.txt](tests/fixtures/uci-smoke.txt): UCI smoke-test transcript.
- [docs/release/kensho-evaluation-report.md](docs/release/kensho-evaluation-report.md):
  local Kensho evaluation report and known release gaps.

The delivered chess engine must run fully locally on macOS. The product must not
require cloud services, hosted APIs, accounts, network access, or deployment.
Kensho/agent-team is configured with `pm.provider = "none"` so orchestration state
also stays in this repository under `.agent_team/`.

All Kensho agents are configured to launch through
`scripts/codex-chatgpt-only.sh`. That wrapper forces the local Codex CLI to use
ChatGPT subscription/workspace login and strips API-key environment variables
before `codex exec` starts.

Agent feedback remains local runtime state. The public repository retains only
the summarized findings in [docs/feedback.md](docs/feedback.md) and the release
evaluation report.

Useful local release commands:

```sh
scripts/acceptance.sh
scripts/release-transcript.sh
cargo run -p chess-engine -- baseline-tournament --games 12 --max-plies 120 --engine-depth 3
```
