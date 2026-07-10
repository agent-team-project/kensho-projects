# Walking Skeleton Conformance Corpus

This directory contains the Slice 0 acceptance corpus for the walking skeleton.
Formula cases follow the `SPEC.md` Appendix B `[[case]]` schema directly.

## Files

- `slice0.toml` preserves the original core smoke cases.
- `walking_skeleton.toml` covers the Slice 0 formula exit cases for `SUM`,
  `AVERAGE`, `IF`, arithmetic, references, and ranges.
- `recalc.toml` covers `RECALC-001` plus the Slice 0 SUM edit exit test.
- `cycle.toml` covers `CYCLE-001` and carries `timeout_ms = 1000` to make the
  no-hang requirement explicit.

## Enforcement

`scripts/validate_conformance_cases.py` validates every `*.toml` file in this
directory and rejects duplicate case ids across files. `cargo test -p
xlite-core` runs the same files through `tests/conformance_runner.rs`; adding a
new TOML file without runner coverage is therefore not possible silently.

Recalc cases use this workbook-state extension:

```toml
[[case]]
kind = "recalc"
setup = { A1 = 1, B1 = "=A1+1" }
formula = "=B1"
expect = { number = 2 }

[[case.steps]]
set = { A1 = 5 }
formula = "=B1"
expect = { number = 6 }
```
