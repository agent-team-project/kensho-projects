---
name: assign-worker
description: Dispatch one local chess-engine backlog issue as a Kensho job through the local_slice pipeline.
user_invocable: true
---

# Assign A Local Worker

Use this skill when the manager needs to dispatch one local backlog issue such as
`CHESS-019`. This repo has no external PM provider.

All worker dispatches must use the repo default Codex wrapper. Do not add
`--runtime codex` unless you also pass
`--runtime-bin scripts/codex-chatgpt-only.sh`.

## Preflight

1. Confirm the issue ID exists in `backlog/README.md`.
2. Confirm the work is small enough for one branch.
3. Write a kickoff file that names the issue, relevant `SPEC.md` sections,
   expected tests, and validation commands.

## Dispatch

Preferred command:

```sh
agent-team job create CHESS-019 \
  --id chess-019 \
  --pipeline local_slice \
  --kickoff-file /tmp/chess-019-kickoff.md \
  --dispatch \
  --workspace worktree
```

If dispatch fails because the daemon is not running, create the job without
`--dispatch`, then ask the operator to start the daemon and tick the team:

```sh
agent-team daemon start
agent-team tick
```

## Follow-up

If the job already exists, do not create a duplicate. Use inbox or job comments
where available, or update the kickoff only if the manager explicitly decides to
re-scope the job.
