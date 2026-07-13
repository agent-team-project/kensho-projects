# Kensho Projects

Public, reproducible project snapshots built or specified through Kensho's
agent-organization workflow. Each project is designed around explicit module
boundaries, objective acceptance gates, and local-first operation.

This repository publishes product source, specifications, tests, selected
Kensho topology, and honest evaluation reports. Runtime transcripts, agent
mailboxes, local credentials, worktrees, build output, and nested repository
history are deliberately excluded.

## Projects

| Project | Status | What it tests | Primary evidence |
| --- | --- | --- | --- |
| [Chess Engine](projects/chess-engine/) | Working local product with documented strength gaps | Correctness-driven systems delivery: legal move generation, perft, UCI, search, and a native board | [Evaluation report](projects/chess-engine/docs/release/kensho-evaluation-report.md) |
| [Excel Lite](projects/excel-lite/) | Local demo ready; public launch not ready | Broad parallel implementation behind a stable formula engine and native spreadsheet shell | [Kensho retrospective](projects/excel-lite/docs/release/kensho-evaluation-report.md) and [PDF](projects/excel-lite/docs/release/kensho-excel-lite-retrospective.pdf) |
| [Workplane](projects/workplane/) | M1 walking slice implemented | Agent-native project and portfolio coordination with evidence, permissions, events, and empirical dogfooding | [Specification and M1 gates](projects/workplane/README.md) |

## Quick verification

Run repository hygiene and specification validation:

```sh
python3 -m pip install pyyaml
python3 scripts/check_publication.py
```

Run the chess acceptance harness:

```sh
cd projects/chess-engine
scripts/acceptance.sh
```

Run Excel Lite's source gates:

```sh
cd projects/excel-lite
python3 scripts/validate_conformance_cases.py
python3 scripts/audit_conformance_coverage.py
cargo test --workspace
npm ci
npm run check
npm test
```

Excel Lite's native Tauri package requires macOS. See its
[packaging guide](projects/excel-lite/docs/packaging-offline.md) for the local
bundle workflow and the launch report for known release blockers.

## Repository layout

```text
.
|-- projects/
|   |-- chess-engine/   # Rust engine, UCI, GUI, tests, and Kensho topology
|   |-- excel-lite/     # Rust calculation core, Svelte/Tauri UI, and evidence
|   `-- workplane/      # Build-ready product, architecture, and test contracts
|-- docs/research/      # Cross-project retrospectives and process findings
|-- scripts/            # Public-repository validation
`-- .github/workflows/  # Monorepo CI
```

## Kensho configuration

The implemented projects retain their checked-in `.agent_team` definitions to
make the delivery topology reviewable. These files are examples and provenance,
not a turnkey global installation. Paths are repository-relative and provider
credentials are absent. Running a team still requires a compatible local Kensho
and Codex installation.

Generated `.agent_team` state is not published. This includes jobs, events,
mailboxes, budgets, daemon state, worktrees, feedback stores, and outcome
ledgers. Evaluation reports summarize relevant process evidence without
publishing private runtime transcripts.

## Scope and claims

- All products are local-first and require no runtime cloud service.
- The reports distinguish measured behavior from missing or aspirational gates.
- Excel Lite is not represented as a publicly distributable v1 release.
- The chess engine is not represented as having a measured Elo rating.
- Workplane implements only its serialized M1 walking slice; broader durable-core
  and release claims remain unimplemented.

## License

Source and documentation in this repository are available under the
[MIT License](LICENSE), except third-party dependencies and referenced standards,
which retain their own terms.
