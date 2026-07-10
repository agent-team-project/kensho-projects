# Contributing

Contributions should preserve the repository's central rule: claims need
objective, reproducible evidence.

## Before opening a change

1. Read the target project's `SPEC.md` and local contributor instructions.
2. Keep the change within one project unless a root policy or CI update is
   genuinely required.
3. Add or update the strongest practical test for changed behavior.
4. Run `python3 scripts/check_publication.py` from the repository root.
5. Run the target project's documented validation commands.

Do not commit generated build directories, agent runtime state, credentials,
local absolute paths, or evaluation claims that cannot be reproduced.

## Project-specific gates

- Chess Engine: `projects/chess-engine/scripts/acceptance.sh`
- Excel Lite: the commands in `projects/excel-lite/.github/workflows/ci.yml`
  are mirrored by the root workflow.
- Workplane: keep TOML/YAML contracts parseable and preserve traceability among
  the specification, contracts, and verification plan.

## Pull requests

Describe what changed, why it is needed, and the exact commands used to verify
it. Call out known gaps directly. Do not lower a hard acceptance gate merely to
make a change pass.
