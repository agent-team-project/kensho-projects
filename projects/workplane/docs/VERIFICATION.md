# Verification Contract

## 1. Purpose

Every material product claim must map to an executable case and durable
evidence. A unit test, API test, rendered-browser test, security test, recovery
test, and research metric answer different questions; none substitutes for
another.

The core system is accepted against real PostgreSQL, the real Go server, and a
production frontend build. If collaboration is promoted, its process becomes a
required release environment for `v1-full`. Mocks are allowed for narrow unit
tests only.

## 2. Verification principles

1. Requirements have stable ids or a stable claim-family id in
   `contracts/traceability.yaml`.
2. Acceptance cases fail without the target behavior.
3. Fixtures are deterministic and versioned.
4. Every gate records source commit, environment, command, start/end, result,
   counts, and artifact digests.
5. Reviewers consume immutable evidence from the exact reviewed commit.
6. Visual claims require rendered-browser evidence.
7. Concurrency claims require multiple real clients/processes.
8. Permission claims test expected denies as aggressively as allows.
9. Recovery claims kill/restart real processes and compare checksums.
10. Research claims identify source events and analysis version.

## 3. Acceptance case format

Machine-readable cases live under `tests/cases/` when implementation begins.
The required schema is:

```yaml
id: PROJECT-PROMOTE-001
requirement: PR-EXP-07
layer: api-integration
title: Promotion retains exploration and creates exploitation contract
given:
  fixture: exploration-ready
  actor: project-owner
  expected_version: 12
when:
  command: promote_project
  input_fixture: promotion-valid
then:
  status: 200
  events:
    - decision.recorded
    - project.promoted
    - deliverable.created
  invariants:
    - project.mode == exploitation
    - old hypotheses remain queryable
    - active deliverables >= 1
evidence:
  - response.json
  - events.json
  - projection-checksum.txt
```

The runner rejects duplicate case ids, unknown requirement ids, missing expected
denies, and evidence paths outside the run directory.

## 4. Test environments

### Unit

In-process pure domain/policy/editor-schema tests with deterministic clock/id
sources. No network or database.

### Integration

Real PostgreSQL and process-local server. Each suite receives an isolated schema
or database. Migrations run from empty.

### System

Docker Compose production build with server, PostgreSQL, and telemetry
collector; add the collaboration process only for promoted/full gates. Tests
use public HTTP/WebSocket/SSE interfaces.

### Browser

Installed Chromium plus at least one WebKit/Firefox-family engine in CI where
supported. Release computer verification uses installed Chrome on macOS and
captures desktop/narrow screenshots.

### Reference laptop

Performance and offline release evidence records exact Apple Silicon hardware,
RAM, OS, Docker, browser, Go, Node, and PostgreSQL versions.

## 5. Domain suite

### 5.1 Lifecycle table

For every project mode/state pair, generate tests for every lifecycle command.
Expected allow/deny is a checked-in table. At minimum:

| Case | Expected |
|---|---|
| proposed exploration activates with complete contract | allow |
| proposed exploration activates without hypothesis/unknown | deny |
| proposed exploitation activates without required deliverable | deny |
| active exploration completes | deny |
| active exploration stops with decision | allow |
| active exploitation stops | deny |
| active exploitation completes with unaccepted deliverable | deny |
| active exploitation completes with hard gate failed | deny |
| active exploitation completes with all gates accepted | allow |
| held project completes | deny |
| terminal project mutates outcome | deny |

### 5.2 Exploration table

- continue records decision and bound extension if applicable;
- pivot supersedes hypothesis and retains prior evidence;
- stop records terminal state and unresolved uncertainty;
- promote atomically changes mode and creates deliverables;
- promote without residual uncertainty statement denies;
- experiment bound emits attention but does not stop work;
- evidence supports/contradicts without deleting observations; and
- exploration never produces completion percentage.

### 5.3 Deliverable/review table

- submit requires criteria/evidence specified by gates;
- independent gate rejects submitter as reviewer;
- bounce requires actionable finding;
- resubmit retains prior verdict/finding;
- approve closes only resolved findings;
- hard gate cannot be waived;
- soft-gate waiver records an authorized human decision/residual risk;
- deliverable waiver denies unless every attached hard gate passed, no blocking
  finding is open, and the human-only waiver contract is complete;
- accepted deliverable cannot be silently edited; and
- project progress changes only on acceptance/valid waiver.

### 5.4 Forecast table

- P50 must not exceed P90;
- project and deliverable scopes each permit exactly one active forecast;
- a deliverable forecast must belong to its owning project and cannot replace
  the project forecast;
- P50/P90 are absolute instants and each has a recorded basis/review-after time;
- reforecast supersedes without mutation;
- reason/impact/basis required;
- stale forecast creates judgment-queue attention;
- target, forecast, deadline, and actual remain distinct types;
- target miss creates attention only;
- holds appear in history but do not alter forecast records;
- scope removal requires decision;
- actual completion derives from completion command; and
- no elapsed-time command approves/completes/cancels.

### 5.5 Property and model tests

- 100,000 generated valid command sequences preserve all invariants.
- Invalid sequences return typed errors and never partially mutate state.
- Blocking dependency DAG remains acyclic under generated add/remove commands.
- Priority rank transactions remain a complete unique ordering.
- Event versions are contiguous and command groups atomic.

## 6. Persistence, replay, and idempotency suite

Required cases:

- migration from empty and each released schema;
- transaction rollback at every write boundary;
- aggregate version conflict under concurrent transactions;
- identical idempotent retry returns original status/body/version/events;
- mismatched body under same key returns idempotency conflict;
- process crash before/after commit and response persistence;
- append-only event table rejects update/delete under app role;
- projection rebuild from events matches canonical checksum;
- unknown event schema stops replay with exact position; and
- replay produces no realtime/automation external effect.

Fault injection points are named and enumerable. CI runs deterministic fault
cases; nightly/release runs randomized kill timing.

## 7. API contract suite

- Every OpenAPI example executes against the real server.
- Generated Go/TypeScript clients compile and round-trip examples.
- Unknown fields, invalid types, oversized fields/lists, and malformed ids fail
  with stable problem codes.
- ETag/If-Match and explicit expected-version behavior agree.
- Pagination has no duplicates/missing rows under stable snapshot.
- Cursor actor/org/filter binding and expiry are enforced.
- 404/403 behavior does not leak resource existence.
- All mutations require idempotency key and all GETs reject mutation bodies.
- Human session and equivalently scoped agent token produce equivalent domain
  result/event group.
- Every interactive UI mutation maps to a public API operation in
  `contracts/parity.yaml`; every agent-essential operation has a verified UI or
  documented machine-only rationale.
- The parity audit fails on an undocumented private endpoint, direct database
  mutation, or UI-only authoritative state.

## 8. Permission suite

Generate a Cartesian matrix over:

- actor kind: human, agent;
- execution context: direct or promoted ECA rule/event context;
- actor status: active, disabled/revoked;
- org role;
- project role;
- project visibility;
- token scope/restriction;
- action;
- resource state; and
- self-review relation.

The checked-in matrix marks expected allow/deny and rationale. Coverage fails if
an action/resource is absent. Run the same deny corpus through HTTP, WebSocket,
SSE, search, collaboration grants, export, and research endpoints.

## 9. Realtime suite

- only committed events delivered;
- no event from rolled-back transaction;
- per-aggregate order under concurrent commands;
- reconnect resumes from cursor without missing events;
- duplicate delivery is deduplicatable by event id;
- expired cursor returns snapshot-required;
- filters reduce but cannot broaden visibility;
- role/token revoke closes live connection under five seconds;
- slow consumer receives bounded backpressure and clean disconnect;
- outbox worker crash/restart creates no missing domain effect; and
- 50-client p95/p99 lag target measured with raw samples.

## 10. Brief and collaboration suite

### 10.1 Core structured-brief matrix

- section updates require the expected section revision;
- non-overlapping section edits from two clients both commit;
- same-section stale edit returns typed conflict with both revisions;
- conflict resolution creates a new revision and never overwrites history;
- agent structured apply uses the same section contract as the UI;
- restore creates a new revision linked to the restored source; and
- comments/entity references never reveal unauthorized content.

### 10.2 Optional CRDT convergence matrix

This suite becomes release-blocking only if the collaboration exploration is
promoted into `v1-full`.

For two and three clients, test:

- concurrent insert at same location;
- concurrent delete/edit;
- formatting overlap;
- list/table structure edits;
- comment creation/resolution;
- disconnect, independent edits, reconnect;
- server restart during buffered updates;
- update reordering and duplicate delivery;
- snapshot plus later updates; and
- agent structured-apply concurrent with human edit.

Every seeded trial compares final Yjs state vector and canonical editor JSON for
all clients/server.

### 10.3 Optional collaboration security/schema

- expired/wrong-org/wrong-document/read-only/revoked grants deny;
- malformed/oversized Yjs updates deny without corrupting document;
- unknown editor nodes render safe placeholder and block destructive save;
- sanitization corpus cannot create executable output;
- restore creates a new snapshot/event and can itself be restored; and
- entity references never reveal unauthorized labels/content.

## 11. Search suite

- exact/stemmed phrase behavior against documented language config;
- every indexed entity type and brief extraction;
- index update/supersession/deletion/archive behavior;
- source/index version and staleness;
- rebuild checksum from authoritative state;
- cross-org/private result, count, facet, and snippet leakage corpus;
- result link resolves to authorized resource; and
- 100,000-record p95 target plus 1,000 concurrent updates during query.

## 12. Optional automation suite

This suite becomes release-blocking only if the post-dogfood automation
exploration is promoted into `v1-full`. Until then, automation endpoints and
tables must not exist in the production core.

- rule parser accepts only documented events/fields/operators/actions;
- invalid/privileged action denies at compile time;
- dry-run has zero mutation and lists matching event ids/commands;
- enable requires current successful dry-run;
- one event plus rule version produces one idempotent effect despite retry;
- recursive causation stops at configured depth;
- disabled/version-changed rule behavior is deterministic;
- failed execution creates inbox/ledger record and bounded retry;
- ECA execution context cannot widen authority or approve/complete/waive; and
- historical event replay does not execute automations.

## 13. Browser workflow suite

Each SPEC section 8 workflow is a named Playwright case against production
build. Required additional behavior:

- browser refresh preserves committed state and route;
- two contexts see realtime update without manual refresh;
- stale structured edit shows conflict resolution, not silent overwrite;
- brief reconnect shows unsynced/synced status and converges;
- keyboard alternative exists for every drag action;
- loading/empty/deny/error/disconnected/search-stale states render;
- narrow layout supports required review/reforecast/comment flows;
- judgment queue completes an ask-context-decide-return workflow entirely by
  keyboard and shows why the item needs a human;
- portfolio shows target versus actual explore/exploit allocation drift;
- light/dark/reduced-motion/200%-zoom visual checks; and
- console has no error/warning, page error, failed request, hydration error, or
  unhandled rejection except explicitly asserted fault cases.

Visual evidence includes stable viewport size, screenshot digest, DOM/a11y
snapshot, and layout assertions for header/sidebar/tabs/toolbars/tables/dialogs.

## 14. Accessibility suite

- axe or equivalent: zero critical/serious findings;
- keyboard-only required workflows;
- focus trap/return for every dialog/sheet;
- semantic grid/table/board/timeline alternatives;
- live-region announcements for save, realtime, conflict, error, and review;
- contrast measured for every semantic state in light/dark;
- screen-reader manual script on portfolio, project, review, brief, inbox; and
- no content/action available only on hover, color, pointer drag, or animation.

## 15. Performance suite

The deterministic scale fixture contains:

- 1 organization, 1,000 actors, 100 service identities;
- 50 portfolios;
- 10,000 projects split across modes/states;
- 50,000 deliverables and gates;
- 250,000 work items and 500,000 dependencies;
- 100,000 evidence/decision/comment/brief search records;
- 1,000,000 domain events; and
- 100 concurrent realtime sessions for soak profile.

Measure cold/warm p50/p95/p99, throughput, allocation/memory, DB query plan, row
counts, and errors. The gate uses SPEC thresholds. A regression budget fails if
p95 worsens more than 20% from the accepted baseline even when under absolute
threshold, unless an approved decision explains it.

## 16. Recovery and operations suite

- clean install from empty volume;
- outbound-network-blocked startup/runtime;
- migration interruption and safe retry;
- SIGTERM graceful drain;
- SIGKILL API/outbox/collab at randomized points;
- PostgreSQL restart during command/update;
- backup populated fixture while writes continue;
- restore into empty deployment;
- event/projection/document/permission/checksum equivalence;
- search/judgment/inbox rebuild after restore;
- corrupted/truncated/path-traversal/oversized backup rejection; and
- telemetry pipeline unavailable without product failure.

## 17. Security suite

Run every gate in `docs/SECURITY.md`, plus:

- static analysis and dependency/license/SBOM/provenance;
- dynamic auth/session/CSRF/rate-limit tests;
- XSS/link/image/editor corpus in every renderer/export/search path;
- SQL/filter/order injection corpus;
- canary secrets scanned from DB events, logs, traces, exports, browser; and
- independent reviewer reproduction of high-risk cases.

## 18. Research verification

- Protocol/metric definitions digest frozen before treatment data access.
- Every metric query has a fixture with hand-calculated expected output.
- Baseline extraction reports missing/inferred data explicitly.
- Treatment completeness compares product events to daemon/PR ground truth.
- Analysis rerun from manifest produces byte-identical tables or documented
  nondeterministic chart metadata only.
- Negative/unresolved hypotheses remain in final report.
- Workplane's own backlog is the second treatment and authoritative mutations
  are reconciled against repository/daemon records.
- P50/P90 calibration, absorption cost, design yield, gate integrity, and
  integration debt each have hand-calculated fixtures.

## 19. Evidence manifest

Every CI/manual run writes:

```json
{
  "schema_version": 1,
  "source_commit": "...",
  "dirty": false,
  "suite": "release",
  "environment": { "os": "...", "hardware": "..." },
  "started_at": "...",
  "finished_at": "...",
  "gates": [
    {
      "name": "domain",
      "command": "go test ./internal/domain/...",
      "result": "pass",
      "tests": 1234,
      "artifacts": [{ "path": "...", "sha256": "..." }]
    }
  ]
}
```

The verifier signs the manifest digest in the job gate record. Review fails if
the source commit differs from the reviewed head or required artifacts are
missing.

## 20. Planned top-level commands

The implementation must provide stable developer commands, likely through
`make` or `just`:

```text
check              formatting, lint, type, schema, generated diff
test-unit          Go/TypeScript pure tests
test-integration   PostgreSQL/API/events/auth
test-contract      OpenAPI/events/permission/case schema
test-collab        optional promoted Yjs convergence/security
test-e2e           production-build Playwright workflows
test-security      static/dynamic security corpus
test-recovery      kill/restart/backup/restore
test-performance   deterministic scale fixture and thresholds
test-research      metric fixture and reproducibility
verify-acceptance  all acceptance gates and evidence manifest
verify-release     clean/offline/full release evidence
```

Exact tool names may change at M0, but one-command gates and their semantics may
not be fragmented across undocumented scripts.

## 21. Staged acceptance

`v1-core` must pass every section except the explicitly optional CRDT and ECA
suites. `v1-full` must pass core plus every promoted exploration suite. A
non-promoted exploration leaves no dormant endpoint, migration, process, or
feature flag in core. Stopping a track with evidence is a successful study
outcome and cannot be represented as a skipped required test.
