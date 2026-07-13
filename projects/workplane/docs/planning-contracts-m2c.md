# M2C planning-contract spine

M2C adds the smallest observable planning contract on top of the accepted M1,
M2A, and M2B spines. It does not add terminal project commands, portfolio
breadth, work or dependency graphs, judgment queues, search, structured briefs,
automation, collaborative editing, or release and scale claims.

## Non-terminal lifecycle and promotion

Projects move only among `proposed`, `active`, and `held`. Activation requires
a current project forecast and a reason. Hold and resume also require reasons.
Promotion is one serializable command that records the universal decision,
creates the required deliverables, changes exploration to exploitation, and
emits one attributable event group under a shared command identifier. No M2C
date or forecast command changes lifecycle state.

## Deliverables and time beliefs

Deliverables have stable identifiers, explicit observable acceptance criteria,
weights, requiredness, and revisions. Accepted, waived, and cancelled rows are
immutable in both the public command layer and PostgreSQL.

Forecasts are append-only P50/P90 history scoped to a project or deliverable.
Each row records its basis, assumptions, reason codes, impact, review instant,
and supersession link. A separate constrained head identifies the sole current
row per scope. Targets express intent; deadlines express an external constraint
and typed source. Past dates create attention, never automatic transitions or
gate bypasses.

## One public plane and durable outcomes

Human sessions and scoped agent bearer tokens call the same generated
operations, authorization boundary, optimistic-version check, idempotency
store, database transaction, event ledger, outbox, replay projection, and
realtime delivery path. Private-project non-members receive not-found; revoked,
restricted, cross-organization, and self-widening authority cases fail before
writes.

Run `make smoke` for the complete accepted gate. The dedicated real-PostgreSQL
M2C harness is `scripts/test_planning.sh`; its load-bearing artifacts are written
to `target/agent-evidence/m2c/` and copied into the exact-commit evidence run by
`make evidence-smoke`.
