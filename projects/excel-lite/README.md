# Excel Lite

Excel Lite is a fully local macOS spreadsheet application with a headless Rust
calculation engine and a Svelte/Tauri desktop shell. It was built as a Kensho
dogfood project around an unusually broad, machine-checkable function library.

## Current status

The calculation core and controlled local demo are working. The project is not
yet represented as a publicly distributable v1 application. Read the
[launch-readiness report](docs/release/kensho-evaluation-report.md) for the
measured gates and blockers, including sheet reachability, unsaved-work safety,
native CI, signing, and notarization.

Key evidence at the audited revision:

- 148 registered worksheet functions;
- 1,585 conformance cases across 152 files;
- 1,341 Rust tests and 38 frontend tests;
- 96.29% aggregate function/evaluator line coverage;
- native formula recalculation, file-dialog, package, and offline evidence;
- a locally built arm64 `.app` and `.dmg`.

## Architecture

- `xlite-core/` - parser, evaluator, sparse workbook, recalc graph, function
  registry, native/CSV/XLSX I/O, history, and structural reference shifting.
- `xlite-app/` - thin Tauri command adapter and native bundle configuration.
- `ui/` - Svelte grid, formula bar, formatting, clipboard, and file workflows.
- `xlite-core/tests/conformance/` - data-driven formula acceptance corpus.
- `docs/verification/` - tracked native and browser verification evidence.
- `.agent_team/` - sanitized Kensho topology and agent definitions; runtime
  state is intentionally excluded.

The authoritative product contract is [SPEC.md](SPEC.md).

## Verify the source

```sh
python3 scripts/validate_conformance_cases.py
python3 scripts/audit_conformance_coverage.py
python3 scripts/validate_ci_contracts.py
cargo fmt --check
cargo test --workspace
npm ci
npm run check
npm test
npm run build
npm run verify:offline
```

On macOS, build the native package with:

```sh
npm run tauri:build
```

Local artifacts are not automatically Developer ID signed or notarized. See
[docs/packaging-offline.md](docs/packaging-offline.md) before distributing one.

## Browser preview

`npm run dev` starts a UI-development preview. It intentionally uses a browser
command client and is not a full web edition: native formula evaluation and
file operations remain Tauri capabilities.

## License

MIT. See the repository root `LICENSE`.
