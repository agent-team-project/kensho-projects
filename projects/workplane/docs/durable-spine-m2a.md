# M2A durable event-delivery and replay spine

This unit adds only the first post-M1 Track B durability boundary. The five M1
public operations, generated client, authorization rules, browser flow,
idempotency responses, and activity representation are unchanged. Realtime
transport, new aggregate breadth, search, briefs, automation, and release/scale
claims remain out of scope.

## Commit path

`domain_events` owns an `AFTER INSERT` database trigger that creates exactly one
immutable `outbox_records` row. Project state, decision state, event,
idempotency response, and outbox therefore commit or roll back together in the
existing serializable command transaction. The application role may insert an
event but cannot update/delete the event ledger or insert/update/delete outbox
payloads. A database-owned security-definer trigger is the only outbox writer.

## Delivery and checkpoints

The production `outbox` Compose service uses `FOR UPDATE SKIP LOCKED` with a
maximum five-minute lease. Delivery is at least once and identified by event ID.
The first publish effect is unique per consumer/event; later attempts are
recorded as duplicates. Checkpoint and outbox acknowledgement commit together.
A higher eligible event may publish while an earlier row is leased, but it
cannot advance the checkpoint until every earlier logical effect exists. The
retry then advances monotonically without losing either effect.

## Replay and integrity

Replay reads the immutable ledger in sequence order under a serializable
snapshot, validates contiguous aggregate versions and the registered v1 event
types, and builds empty run-scoped project, decision, and activity shadow
tables. It compares normalized live/rebuilt row counts and a SHA-256 checksum
before atomically replacing `projection_heads.m1-canonical`. An unknown event
type/version records `unknown_event_schema` with its exact sequence and event ID;
the failed generation is not installed.

The integrity doctor checks aggregate gaps/duplicates, event/outbox coupling,
outbox envelope identity and digest, unknown schemas, checkpoint
staleness/identity, and active-head checksum drift. Database triggers separately
reject event/outbox mutation and backwards or identity-changing checkpoints.

## Reproduction and evidence

Run from `projects/workplane`:

```sh
make bootstrap
make smoke
```

Exact-head reproduction uses `make evidence-smoke`. Durable artifacts are
written beneath `target/agent-evidence/m2/`: migration output, outbox rows,
checkpoint transitions, crash/retry and reorder timelines, repeated replay
checksums/counts, application-role denies, unknown-schema failure/head identity,
and integrity-doctor negatives. The smoke evidence manifest includes SHA-256
digests for each of these artifacts.
