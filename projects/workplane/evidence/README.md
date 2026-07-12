# Workplane evidence

`make evidence-smoke`, `make evidence-acceptance`, and
`make evidence-release` write immutable-run manifests under
`evidence/runs/<source-commit>/`. The run directory is intentionally ignored:
verifier artifacts belong to the exact checkout and durable job gate record,
not to a later source commit.

The generator refuses tracked dirty source. Every manifest records the commit,
dirty flag, environment/tool versions, suite timestamps, command-level result,
and an honestly named `command_executions` count (one recorded process launch
per gate), plus non-empty SHA-256-verified log artifacts. Emitted manifests are
validated against the checked-in Draft 2020-12 `manifest.schema.json` and its
timestamp/count/digest semantics before they are written.
