# Workplane evidence

`make evidence-smoke`, `make evidence-acceptance`, and
`make evidence-release` write immutable-run manifests under
`evidence/runs/<source-commit>/`. The run directory is intentionally ignored:
verifier artifacts belong to the exact checkout and durable job gate record,
not to a later source commit.

The generator refuses tracked dirty source. Every manifest records the commit,
dirty flag, environment/tool versions, suite timestamps, command-level result
and test count, plus SHA-256 digests for captured logs. The JSON contract is
checked in as `manifest.schema.json`.
