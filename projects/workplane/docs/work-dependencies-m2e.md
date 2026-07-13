# M2E fixed work and dependency spine

M2E makes one bounded work plane observable through the public API. It does not
add project termination, portfolio breadth, experiments, inboxes, comments,
search, briefs, collaboration, or automation.

## Public contract

Humans and delegated agents use the same ten OpenAPI operations to create,
read, update, assign, and transition work; add and remove typed dependencies;
atomically transition a batch; and read a visible dependency graph. Required
create members and presence-aware update members are checked before a durable
write. Every mutation uses a target-bound idempotency key and every versioned
mutation uses current optimistic concurrency state.

The persisted lifecycle is closed:

```text
open --start--> in_progress --request_review--> in_review
in_review --bounce--> in_progress
in_review --accept--> done
open|in_progress|in_review --cancel--> cancelled
```

Starting requires an active organization assignee. Review and acceptance
require current project evidence. Bounce requires an open blocking finding on
the linked deliverable. `done` and `cancelled` are terminal for work, but work
acceptance does not accept its deliverable or terminate its project.

`blocked` is a derived response field, never a persisted lifecycle state. An
unfinished target of a `blocks` edge contributes `dependency`; an incomplete
hard gate on the linked deliverable contributes `hard_gate`. `relates` and
`caused-by` are descriptive and never participate in the blocking DAG.

## Atomicity and graph safety

Dependency edges are immutable, same-organization ordered pairs with one of
three fixed kinds. The command layer takes an organization-scoped graph lock,
checks direct and transitive cycles, and the PostgreSQL insert trigger takes
the same lock and independently rejects a closing blocking edge. Concurrent
inverse inserts therefore serialize to one committed edge and one stable
`dependency_cycle` response.

Batch transitions sort and lock every requested work item, validate the whole
final-state set, and only then write rows, events, outbox records, and one
idempotency result. Duplicate, missing, mixed-project, mixed-organization,
stale, unauthorized, invalid, or injected-fault commands leave no partial
state. A batch shares one command id while retaining one ordered domain event
per affected work aggregate.

Authorization is re-evaluated before stored idempotent success. Cross-project
dependency mutations require current edit authority on both endpoints. Graph
reads suppress an edge and its remote endpoint unless both projects are
currently readable. Cross-organization ids receive `not_found` and cannot
influence a local batch.

## Durable evidence

Migration `000008_m2_work_dependencies.up.sql` is additive and captures exact
pre-upgrade counts and digests for accepted M2D aggregates, ledger rows,
outbox rows, and consumer checkpoints before applying M2E DDL. Work and
dependency events rebuild into shadow projections; the active replay head
advances only when the rebuilt checksum matches live state.

`scripts/test_work.sh` runs the public contract against real PostgreSQL and the
production API/outbox processes. It covers both actor kinds, fixed lifecycle
and deliverable separation, hard-gate and dependency blocking, graph
non-disclosure, direct/transitive/concurrent cycles, exact add/remove retries,
atomic batch negatives and fault injection, current-authority denial, signed
SSE resume, process restart, replay corruption without head advancement, and
doctor detection of graph cycles and projection drift. Evidence is written to
`target/agent-evidence/m2e/` and is snapshotted by the exact-head smoke gate.
