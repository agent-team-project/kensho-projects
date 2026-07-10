# Conformance Completeness Audit

Scope: EL-367 / SPEC Epic R conformance coverage and documented deviation pins.
The audit source is `scripts/audit_conformance_coverage.py`, which compares:

- SPEC Epics F-M function inventory in `SPEC.md`.
- Rust function implementations under `xlite-core/src/functions/*/*.rs` that declare
  `Function::name()` and self-register with `inventory::submit!`.
- Dedicated conformance files under `xlite-core/tests/conformance/fn_*.toml`.
- Appendix A deviation pins required by EL-367.

## Machine Audit Output

Command:

```sh
python3 scripts/audit_conformance_coverage.py
```

Output:

```text
# Conformance Coverage Audit

## Summary
- SPEC Epics F-M functions: 148
- Registered function implementations: 148
- Dedicated `fn_*.toml` files: 146
- Total conformance TOML files: 152
- Total conformance cases: 1578
- Deviation-tagged cases: 24

## Dedicated Function Coverage
- Missing dedicated `fn_*.toml` coverage: AVERAGE, SUM
- Missing dedicated coverage outside shared exceptions: none
- Orphan dedicated `fn_*.toml` files: none
- Duplicate registered function names: none
- SPEC functions missing from the registry: none
- Registered functions outside SPEC Epics F-M: none

## Shared Coverage Exceptions
- AVERAGE: 4 shared cases (engine_contract.toml: AGG-005, AGG-006; slice0.toml: SLICE0-AVERAGE-001; walking_skeleton.toml: FN-AVERAGE-001)
- SUM: 10 shared cases (cycle.toml: CYCLE-RANGE-SELF; engine_contract.toml: ERRPROP-004, AGG-001, AGG-002; recalc.toml: RECALC-SLICE0-001, RECALC-006; slice0.toml: SLICE0-SUM-001; walking_skeleton.toml: FN-SUM-001, FN-SUM-002, RANGE-001)

## Appendix A Deviation Pins
- pass: A.1 circular references (CYCLE-001 in cycle.toml)
- pass: A.2 precedence: unary minus (PREC-001 in engine_contract.toml)
- pass: A.2 precedence: power associativity (PREC-002 in engine_contract.toml)
- pass: A.3 Excel 1900 phantom leap day (DATE-SER-003 in documented_deviations.toml)
- pass: A.4 cross-sheet references (XSHEET-001 in documented_deviations.toml)
- pass: A.5 unknown functions (NAME-001 in documented_deviations.toml)

## Advisory Case-Count Findings
Dedicated `fn_*.toml` files below the SPEC Section 5.1 target of 5 cases:
- FALSE: 4
- NA: 4
- NOW: 3
- TODAY: 3
- TRUE: 4

## Hard Audit Issues
- none
```

## Findings

All 148 SPEC Epics F-M functions are present in the static registration audit,
there are no duplicate registered function names, and there are no registered
functions outside the SPEC inventory. There are no orphan dedicated `fn_*.toml`
files.

There are 146 dedicated function conformance files. The two registered functions
without dedicated files are intentional historical shared-coverage exceptions:

- `SUM` is covered by walking-skeleton, Slice 0, engine-contract, recalc, and
  cycle cases.
- `AVERAGE` is covered by walking-skeleton, Slice 0, and aggregation-contract
  cases.

This means old or duplicate backlog items asking only for `fn_sum.toml` or
`fn_average.toml` are documentation/organization choices, not function
implementation gaps. New function implementation jobs should not be spawned for
either `SUM` or `AVERAGE` on the basis of missing dedicated files alone.

The audit found three clear EL-367 deviation pin omissions and fixed them in
`xlite-core/tests/conformance/documented_deviations.toml`:

- `DATE-SER-003` pins Appendix A.3, the Excel 1900 phantom leap-day serial.
- `XSHEET-001` pins Appendix A.4, cross-sheet references resolving to `#REF!`.
- `NAME-001` pins Appendix A.5, unknown functions resolving to `#NAME?`.

Appendix A.6 locale behavior remains documented as an `en-US` scope constraint.
It is not treated as an unpinned runtime deviation by this audit because the
current conformance schema evaluates formulas after parsing and has no locale
mode parameter.

## Follow-Up

No new function implementation jobs are recommended from this audit.

One small conformance top-up job is justified if strict SPEC Section 5.1 case
counts are enforced: add enough cases for `FALSE`, `NA`, `NOW`, `TODAY`, and
`TRUE` to reach the five-case dedicated-file target. This is separate from the
function inventory completeness question and should not be dispatched as
implementation work.
