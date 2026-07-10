# Excel-lite backlog

Source of truth: `../SPEC.md`. Section 8 has the full EPIC → ISSUE breakdown (215 issues, 18 epics).

## Build order (6 incremental, independently-reviewable slices)
1. Walking skeleton — grid + type a number + a 3-function engine + recalc
2. Engine hardening — dependency graph, incremental recalc, cycle detection, error values
3. Function library fan-out — 148 functions, one file/PR each (the flagship parallel epic; interfaces frozen after slice 1)
4. File format + undo/redo
5. UI polish
6. Packaging

The function-library fan-out (slice 3) is the parallelism engine — dozens of disjoint one-function issues dispatchable concurrently once the `Function`/`FnContext`/`Arg`/`RangeView` interfaces are frozen.
