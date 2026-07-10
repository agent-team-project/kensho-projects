# Architecture

## 1. Architecture goals

The architecture must make the difficult behavior explicit and testable:

- authoritative structured state under concurrent mutation;
- replayable event history;
- realtime projections without a second source of truth;
- versioned rich documents isolated from project commands;
- identical authorization for human and agent actors;
- deterministic automation only if its exploration is promoted;
- search that cannot cross permission boundaries; and
- one-command self-hosted operation.

The design favors a modular monolith over microservices. Distribution is used
only where a mature collaboration runtime provides a stronger core than a
custom implementation.

## 2. Technology choices

### 2.1 Backend: Go

Go provides simple concurrency, strong HTTP/runtime tooling, fast builds, and a
stack familiar to Kensho. The domain and application layers use the standard
library where practical. Runtime dependencies should be few and explicit:

- PostgreSQL driver and migration tool;
- generated query layer or thin typed repository layer;
- WebSocket/SSE transport;
- OpenTelemetry instrumentation;
- Argon2id password hashing; and
- a small policy/validation library only if it removes demonstrable complexity.

No general dependency injection framework, ORM with hidden persistence, or
embedded workflow engine is permitted.

### 2.2 Database: PostgreSQL

PostgreSQL owns transactions, constraints, durable events, relational
projections, full-text search, and outbox leases. One database reduces
cross-system consistency risk while preserving clear schemas.

Target PostgreSQL 16 or newer. SQL must remain portable across supported major
versions. Row-level security may provide defense in depth, but application
authorization remains mandatory and is tested independently.

### 2.3 Web: React and TypeScript

Use React, TypeScript in strict mode, Vite, TanStack Query, TanStack Router,
TanStack Table, and dnd-kit or equivalent proven libraries. Use Lucide icons.
The app is an operational tool: dense, restrained, keyboard accessible, and
optimized for scanning and repeated action.

Generated OpenAPI types are the transport source of truth. Handwritten frontend
types may model view state but may not duplicate server DTO contracts.

### 2.4 Brief editor core and CRDT exploration

`v1-core` uses a constrained Tiptap/ProseMirror schema persisted as versioned
structured JSON sections. Section writes use optimistic concurrency and retain
immutable revisions. This provides rich briefs, references, comments, and
version history without adding a second distributed consistency model before
the thesis-bearing dogfood.

Simultaneous co-editing/presence is a detachable exploration track.

Yjs provides network-agnostic CRDT shared types and editor integrations. Tiptap
provides a ProseMirror-based editor and Yjs collaboration extension. Use the
open-source editor and collaboration components; no paid cloud service is a
runtime dependency.

If promoted, the collaboration process may use the open-source Hocuspocus server or an
equivalent minimal Yjs WebSocket server after a license and operational review.
The API server remains the authorization issuer and document metadata owner.

References:

- https://docs.yjs.dev/
- https://tiptap.dev/docs/editor/extensions/functionality/collaboration
- https://tiptap.dev/docs/examples/advanced/collaborative-editing

### 2.5 Observability: OpenTelemetry

Use OpenTelemetry for traces and bounded-cardinality metrics. Structured logs
carry trace and request ids. Never use user ids, project ids, document text, or
tokens as unbounded metric labels.

Reference: https://opentelemetry.io/docs/concepts/signals/metrics/

## 3. Repository shape

```text
workplane/
  cmd/
    workplane-server/       Go API, workers, and realtime binary
    workplane-admin/        maintenance CLI: migrate, backup, restore, replay
  internal/
    domain/                aggregates, commands, invariants, domain events
    application/           use cases and transaction orchestration
    auth/                  sessions, tokens, authorization, policy
    store/                 PostgreSQL repositories and migrations
    events/                event schemas, outbox, replay, subscriptions
    realtime/              WebSocket/SSE session and fan-out
    automation/            optional after ECA exploration promotion
    search/                permission-aware indexing and query
    judgment/              judgment and notification projections
    telemetry/             traces, metrics, structured logging
    export/                backup/export and restore validation
  web/
    src/
      app/                 routes, shell, command palette
      features/            portfolio, project, work, review, brief, judgment
      components/          shared domain-agnostic UI
      api/                 generated client and realtime client
      editor/              Tiptap schema, references, comments; optional presence
      test/                fixtures and accessibility helpers
  collab/                  optional, only after exploration promotion
    src/                   Yjs server, persistence adapter, auth verification
  api/
    openapi.yaml
    events/                versioned JSON Schemas
  migrations/
  fixtures/
    reference-org/
    scale/
    security/
  tests/
    contract/
    integration/
    concurrency/
    e2e/
    performance/
    recovery/
  deploy/
    compose.yaml
    otel/
  docs/
  backlog/
```

`internal/domain` imports no database, HTTP, WebSocket, editor, or UI package.
`web/components` imports no project feature or API module. `collab` cannot access
domain tables directly except its dedicated document-update schema.

## 4. Component model

```text
Browser / Agent Client
        |
        | HTTPS + JSON / WebSocket / SSE
        v
+--------------------------+
| Go workplane-server      |
| auth -> application      |
| domain -> transaction    |
| queries -> projections   |
+------------+-------------+
             |
             | SQL transaction
             v
+--------------------------+
| PostgreSQL               |
| aggregates + events      |
| projections + outbox     |
| search + judgment/inbox  |
+------------+-------------+
             |
             | leased committed events
             v
+--------------------------+       short-lived document grant
| workers/realtime         |-------------------------------+
+--------------------------+                               |
                                                           v
Browser editor <------ optional Yjs WebSocket ------> collaboration process
                                                |
                                                v
                                      PostgreSQL Yjs updates
```

## 5. Domain/application interfaces

### 5.1 Commands

```go
type Command interface {
    CommandName() string
    AggregateRef() AggregateRef
    ValidateShape() error
}

type CommandEnvelope struct {
    CommandID      domain.ID
    CorrelationID  domain.ID
    Actor          domain.ActorRef
    ExpectedVersion *domain.Version
    IdempotencyKey string
    ReceivedAt     time.Time
}

type CommandResult struct {
    AggregateRef domain.AggregateRef
    Version      domain.Version
    EventIDs     []domain.ID
    Representation any
}

type CommandHandler[C Command] interface {
    Handle(ctx context.Context, tx UnitOfWork, env CommandEnvelope, cmd C) (CommandResult, error)
}
```

For agent actors `ActorRef.PrincipalID` is mandatory and identifies the human or
delegated role whose authority is exercised.

Command handlers load aggregates, authorize, validate invariants, append events,
update current projections, and add outbox records inside one transaction.

### 5.2 Unit of work

```go
type UnitOfWork interface {
    Projects() ProjectRepository
    Deliverables() DeliverableRepository
    WorkItems() WorkItemRepository
    Events() EventAppender
    Outbox() OutboxWriter
    Idempotency() IdempotencyRepository
}

type TransactionRunner interface {
    Within(ctx context.Context, fn func(UnitOfWork) error) error
}
```

Repositories are aggregate-oriented. UI query projections use separate query
interfaces and never expose database rows directly.

### 5.3 Queries

```go
type QueryService interface {
    Portfolio(ctx context.Context, actor ActorRef, q PortfolioQuery) (PortfolioPage, error)
    Project(ctx context.Context, actor ActorRef, id ID) (ProjectView, error)
    ProjectActivity(ctx context.Context, actor ActorRef, q ActivityQuery) (ActivityPage, error)
    Search(ctx context.Context, actor ActorRef, q SearchQuery) (SearchPage, error)
    Inbox(ctx context.Context, actor ActorRef, q InboxQuery) (InboxPage, error)
    JudgmentQueue(ctx context.Context, actor ActorRef, q JudgmentQuery) (JudgmentPage, error)
}
```

All queries take the authorized actor explicitly. No repository method accepts a
bare resource id without organization scope.

## 6. Transaction and event flow

For every mutating request:

1. authenticate actor;
2. parse and shape-validate request;
3. begin database transaction;
4. resolve idempotency key;
5. authorize action against current resource/membership;
6. lock or version-check the aggregate;
7. run domain invariant checks;
8. write aggregate changes;
9. append ordered domain events;
10. update transactionally fresh projections;
11. append outbox records;
12. persist idempotent response;
13. commit; and
14. publish committed outbox records asynchronously.

No event is published before commit. No successful response is returned before
the idempotent response is durable.

### 6.1 Outbox

```go
type OutboxRecord struct {
    ID          ID
    EventID     ID
    Topic       string
    Payload     []byte
    AvailableAt time.Time
    Attempts    uint16
    LeaseOwner  *string
    LeaseUntil  *time.Time
    DeliveredAt *time.Time
}
```

Workers lease with `FOR UPDATE SKIP LOCKED`. Delivery is at least once. Every
consumer checkpoints event id and implements idempotent application.

### 6.2 Replay

The admin CLI can truncate rebuildable projections and replay all events into
empty projections. Replay disables external/realtime side effects. Replay emits
a canonical checksum report and fails on an unknown event schema.

## 7. Realtime architecture

The UI opens one organization-scoped WebSocket after session authentication.
Agents may use SSE with a durable cursor. Both consume the same event fan-out.

Realtime guarantees:

- only committed events;
- per-aggregate order;
- organization isolation;
- resumable cursor within retention;
- snapshot-required response when cursor is too old;
- authorization recheck on permission-change events; and
- no guarantee of exactly-once delivery.

Clients deduplicate by event id and refetch affected query projections. The
event payload can contain safe summary fields but is not assumed to be a full
projection.

## 8. Collaborative brief architecture

### 8.0 Core brief contract

Core briefs are structured editor documents divided into stable sections. Each
section has an id/version; writes require expected version and produce immutable
document revisions plus a brief event. Separate-section concurrent edits can
commit. Same-section stale edits receive a typed conflict and a merge/reapply
surface.

The remainder of section 8 applies only if the CRDT collaboration exploration
is promoted into `v1-full`. Its acceptance bar remains fixed while optional.

### 8.1 Separation from structured state

Each project has one Yjs document with named fragments for `body` and comments.
Project title, outcome, forecast, deliverables, gates, work, and decisions are
not stored in Yjs.

Entity-reference nodes store:

```ts
type EntityReference = {
  entityType: "project" | "deliverable" | "work_item" | "decision" | "evidence";
  entityId: string;
  labelAtInsert: string;
};
```

References render current authorized labels from the API. They do not execute
commands or expose unauthorized content.

### 8.2 Document grants

The browser requests a short-lived signed grant from the API server after
ordinary authorization. Claims include actor, org, document, permissions,
schema version, expiry, and nonce. The collaboration process validates the
signature and rejects expired/revoked grants.

Agent document editing uses the same grant and Yjs update protocol or a bounded
server-side JSON-to-Yjs command. It may not write directly to update tables.

### 8.3 Persistence and compaction

The collaboration process stores ordered binary Yjs updates with actor and
digest. A snapshot worker creates state-vector snapshots after a configurable
update count/size. Compaction may remove redundant update payloads only after a
verified snapshot and backup, while retaining update metadata and snapshot
history.

### 8.4 Schema evolution

Editor schema version is recorded per snapshot. Unsupported nodes render as a
safe read-only placeholder and block destructive save. Before v1, schema
changes migrate documents directly; no permanent dual editor schema is kept.

## 9. Authorization architecture

Authorization is deny by default and evaluates:

```text
actor status
AND organization membership
AND organization role
AND project role or visibility
AND token scopes (for agent and promoted ECA contexts)
AND action-specific constraints
```

Application policy functions are pure and table-tested. PostgreSQL RLS, if
enabled, uses transaction-local actor/org settings as defense in depth. The
collaboration process receives only document-scoped grants and cannot query
membership policy itself.

Human-required judgments are encoded in the permission matrix as actor-kind and
role requirements. The frontend is not the enforcement mechanism. Agent actors
must carry a visible principal and cannot widen their own authority.

Full policy is in `docs/SECURITY.md`.

## 10. Search architecture

PostgreSQL full-text search indexes:

- project title/outcome/tags;
- deliverables and criteria;
- work title/description;
- decision question/choice/rationale;
- evidence title/body;
- comments; and
- sanitized plain text extracted from brief snapshots.

Search records contain `org_id`, visibility, project id, entity type/id,
language configuration, text vector, and source version. Queries first restrict
organization, then join authorized project ids, then rank. Search snippets are
generated only after authorization.

Indexing is asynchronous but exposes source/index version and staleness. A
rebuild command derives all records from authoritative tables and current brief
snapshots.

## 11. Automation exploration architecture

The judgment queue, inbox, and public event subscriptions ship in `v1-core`. The ECA compiler and
executor below start only after the first dogfood checkpoint and only if a
recorded exploration decision finds repeated deterministic actions whose agent
implementation has material absorption cost. An unpromoted track does not block
core release.

Rules use a bounded declarative model:

```go
type Rule struct {
    Trigger EventPattern
    Conditions []Condition
    Actions []AllowedAction
    Enabled bool
    Version Version
}
```

Allowed actions initially include: create inbox item, assign work, add comment,
request review, set non-authoritative tag, and create attention. Completing,
approving, waiving, deleting, changing permissions, issuing tokens, or invoking
external network are not automatable in v1.

The rule compiler validates fields/types against event schemas. Dry-run returns
matched historical events and proposed commands without mutation. Execution
uses event id plus rule version as idempotency key.

## 12. Frontend architecture

### 12.1 Data flow

- TanStack Query owns server projections and cache invalidation.
- Local component state owns transient UI only.
- Mutations use generated commands with idempotency key and expected version.
- Realtime events invalidate/narrowly patch queries by entity reference.
- Version conflicts open a resolution surface with current and attempted values.
- Core editor state remains in the editor/revision model, not query-cache shadow
  state. If CRDT collaboration is promoted, live editor state remains in Yjs.

CI generates a parity manifest mapping every UI mutation to a public OpenAPI
operation. Every public mutation must be UI-reachable or explicitly declared
`headless` with rationale. Undeclared internal mutation routes fail the gate.

### 12.2 Feature boundaries

Each feature owns routes, queries, commands, view models, and tests. Shared
components remain domain-agnostic. Cross-feature actions call command services,
not import internal state from another feature.

### 12.3 Failure states

Every route defines loading, empty, permission-denied, offline, stale,
version-conflict, and server-error states. Realtime disconnect shows last
confirmed time and retries without presenting stale state as current.

## 13. Deployment

Docker Compose starts:

- `workplane-server`;
- PostgreSQL;
- OpenTelemetry Collector; and
- an optional local observability profile.

After collaboration-track promotion it also starts `workplane-collab`.

Migrations run as a distinct pre-start job under a database advisory lock.
Services run as non-root, expose health/readiness endpoints, use read-only
containers where possible, and persist data in named volumes.

No external DNS, telemetry exporter, CDN, font, analytics script, or cloud
object store is required. The offline acceptance test blocks outbound network.

## 14. Backup and recovery

`workplane-admin backup` creates a versioned archive containing:

- PostgreSQL logical dump;
- brief documents/revisions and, after collaboration promotion, Yjs
  snapshot/update data;
- uploaded small evidence assets;
- manifest with schema versions, counts, and SHA-256 digests; and
- no plaintext tokens or password hashes beyond the encrypted database dump.

Restore validates manifest/digests, migrates to the current schema, rebuilds
search, verifies event/projection checksums, and emits a recovery report.

## 15. Observability

Required traces:

- HTTP request -> command -> database transaction -> outbox;
- outbox -> projection/search/inbox/realtime consumer;
- automation trigger -> command when that track is promoted;
- document grant -> collaboration session when that track is promoted; and
- backup/restore/replay phases.

Required bounded metrics:

- command latency/error/version-conflict by command type;
- query latency by query type;
- outbox depth/age/retry;
- realtime connections/delivery lag;
- document sessions/update bytes/compaction lag when collaboration is promoted;
- automation execution outcome when automation is promoted;
- search index staleness; and
- forecast/research metrics as product analytics events.

## 16. Architecture verification seams

Every major seam has an executable contract:

| Seam | Contract test |
|---|---|
| Domain -> application | generated command sequences preserve invariants |
| Application -> PostgreSQL | transaction rollback and idempotency suite |
| Events -> projections | full replay checksum |
| Outbox -> consumers | crash/retry/no-missing-effect suite |
| API -> clients | OpenAPI conformance and example suite |
| API -> realtime | committed order, resume, revoke, isolation suite |
| API -> collaboration (full) | grant scope/expiry/revoke matrix |
| Yjs clients -> store (full) | partition/reconnect convergence suite |
| Search -> authorization | visibility leakage corpus |
| Automation -> commands (full) | dry-run and idempotent execution suite |
| Frontend -> API | Playwright workflow suite against real services |

## 17. Deliberate serialization points

Parallel teams must not independently change these surfaces:

1. domain vocabulary and lifecycle;
2. event envelope and schema-version policy;
3. permission action/resource vocabulary;
4. OpenAPI mutation semantics;
5. Yjs editor schema; and
6. migration ordering.

Changes require an architecture decision, contract updates first, and one
integration owner. Feature implementation behind a stable contract may proceed
in parallel.
