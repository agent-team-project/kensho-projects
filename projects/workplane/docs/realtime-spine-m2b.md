# M2B resumable realtime delivery spine

M2B adds only the remaining Track B transport boundary on top of M2A. It does
not add aggregate breadth, UI composition, briefs, search, CRDT collaboration,
automation, or release/scale claims.

## One committed source

The production outbox process runs the existing `projection-v1` consumer and a
dedicated `realtime-v1` consumer. WebSocket and SSE read only immutable outbox
records whose realtime logical effect exists at or behind that consumer's
monotonic checkpoint. Before advancing, the consumer establishes a PostgreSQL
commit horizon over domain-event writers, so an allocated lower sequence cannot
become visible after a higher checkpoint; rolled-back allocation gaps remain
safe to cross. The transported JSON value is the complete M2A canonical
outbox envelope; neither transport creates a second event shape or source of
truth. Event id remains the logical deduplication identity.

## Resume contract

Every ready, heartbeat, and event position is an HMAC-signed opaque cursor bound
to actor, organization, transport, and the canonicalized event-type filter.
Event cursors also bind the exact event id. Resume is exclusive of the supplied
position. A bad binding, expired signature, unknown future/identity position,
or position older than `realtime_retention.minimum_cursor_sequence` returns
`409 snapshot_required` with current-state query links. The immutable ledger is
not pruned when the public resume window advances.

## Live authority and bounded delivery

Each poll re-authenticates the session or agent token and reloads principal,
delegated principal, organization membership/role, token scopes/restrictions,
expiry, and revocation. Each envelope then rechecks organization, project
restriction, current project visibility, creator/participant access, and the
intersection of agent and delegated roles. Authority loss emits
`permission_changed` and closes. Cross-organization, private-project, and
unlisted-project envelopes are examined only inside the server and never put on
the wire.

WebSocket permits at most eight unacknowledged event cursors and closes with an
explicit `rate_limited` / `slow_consumer` outcome. SSE uses an eight-message
producer buffer plus bounded write deadlines and emits the same explicit
outcome before ending when the consumer cannot keep up. A WebSocket failure
cursor repeats the last event successfully written, so exclusive resume cannot
skip the first unread event. Filters only reduce delivery and are part of the
cursor binding.

## Reproduction

`scripts/test_realtime.sh` builds the production image with network-disabled
dependency resolution, starts real PostgreSQL plus the production API/outbox
services, commits events through public commands, restarts both processes, and
then runs exact-envelope, resume, snapshot-required, authority-canary, revoke,
commit-before-publish, inverted-commit-order, and slow-consumer-resume cases. Artifacts are written beneath
`target/agent-evidence/m2b/` and are copied and hashed by `make evidence-smoke`.
