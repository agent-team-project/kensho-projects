# Delivery Plan

## 1. Delivery principle

Parallelize along stable coupling seams; serialize changes to the domain,
permission, event, API, editor-schema, and migration contracts.

Unlimited model tokens do not imply unlimited integration capacity. The first
walking slice is serial at the contract seam. Initial implementation WIP after
that slice is two product tracks plus one independent verification/review path;
it expands to three only when merge latency and review age remain within limits.
Recursive units are allowed only with a written charter, bounded surface,
integration owner, evidence contract, WIP limit, and sunset condition.

A **track** owns one stable product surface and its integration duty. A **unit**
is one concurrently executing implementation deliverable inside a track. After
M1, total release-bearing implementation WIP is at most four units across all
tracks, not four per track. It may rise to six only after two consecutive green
integration cycles with merge and review latency inside their measured healthy
bands, no red main interval, and an explicit integration-owner decision.
Promoted exploration code consumes the same six-unit ceiling. Research-only
analysis may run independently, but any finding requiring code consumes the
implementation ceiling.

## 2. Promotion into implementation

The current project mode is exploration. Promotion to exploitation requires:

- product/spec review approved;
- advisor critique disposition complete;
- domain vocabulary and lifecycle accepted;
- architecture spike risks identified;
- security threat model reviewed;
- research protocol frozen;
- implementation owner and integration owner named;
- walking-slice charter and the first two post-slice track charters written; and
- relative forecast approved as best-effort.

No code scaffolding is required for promotion. No feature implementation begins
before it.

## 3. Forecast initialization

The static specification does not assign calendar durations. Agent throughput,
parallelism, review absorption, and integration uncertainty vary too sharply for
day-based estimates written before kickoff to be useful.

| Milestone | Optimistic completion condition | Conservative completion condition |
|---|---|---|
| M0 contracts frozen | contracts agree on first review | reconciliation exposes and resolves contract drift |
| M1 walking slice | vertical interfaces hold on first integration | one or more load-bearing seams require redesign and replay |
| M2 durable/realtime core | walking-slice contracts generalize cleanly | concurrency, replay, or recovery evidence forces correction |
| M3 agent parity and structured briefs | public-plane and section contracts compose | parity or conflict handling requires another integration pass |
| M4 dogfood-ready core | core workflows pass as one system | security, accessibility, or operations findings require remediation |
| M5 core acceptance and research run | hard suites and treatment evidence agree | scale, recovery, or study integrity requires another evidence cycle |
| M6 v1-core release candidate | independent reviews approve | findings bounce to their owning milestone without weakening gates |

At exploitation kickoff, the owner records actual timestamp-based P50/P90
forecasts using current fleet throughput, integration capacity, and uncertainty.
Each gate supersedes them with observed evidence. They remain best-effort
predictions, never deadlines; a miss triggers reforecast and learning, never
gate reduction.

Workplane's delivery is modeled as one project whose M0-M6 milestones are
deliverables. Each milestone has a deliverable-scoped P50/P90; M6 also aligns
with the project forecast. Deliverable forecasts are diagnostic and do not
inflate the H4 terminal-project cohort.

## 4. Track charters

### Track A: contract seam and walking slice

**Owns before M1:** domain vocabulary, permission actions, event envelope,
OpenAPI mutation conventions, PostgreSQL transaction, generated client, and one
browser path.

**Does not own:** breadth beyond the walking-slice behavior.

**Deliverable:** one vertical human-and-agent transaction through real
PostgreSQL, API, generated client, and browser with immutable activity.

**Integration duty:** publish executable contract fixtures before feature
fan-out. One integration owner serializes changes to this seam.

**Initial WIP:** 1 until M1 exits.

### Track B: durable coordination core

**Owns:** project/deliverable/work/decision/forecast aggregates, event ledger,
outbox, projections, HTTP/SSE/WebSocket, identity, and authorization.

**Does not own:** UI composition, collaborative editing, or automation.

**Deliverable:** replayable coordination core plus human/agent parity contract.

**Integration duty:** keep OpenAPI, events, permissions, and examples executable
against the walking server.

**Initial WIP:** 2 after M1.

### Track C: web product and structured briefs

**Owns:** app shell, portfolio/project/judgment/work/review UX, versioned brief
sections, generated client integration, accessibility, and browser verification.

**Does not own:** duplicate domain types or direct database access.

**Deliverable:** complete required workflows through public API.

**Integration duty:** maintain Playwright workflow fixtures and visual evidence.

**Initial WIP:** 2 after M1; increase to 3 only after two consecutive green
integration cycles with review queue age inside its measured healthy band.

### Track D: collaboration exploration (`v1-full` candidate)

**Owns:** editor schema, Yjs server/provider, document grants, persistence,
snapshot/restore, comments/presence, convergence tests.

**Does not own:** structured project state or general file storage.

**Deliverable:** permissioned convergent brief editor and agent apply API.

**Integration duty:** version editor schema and publish test documents.

**Forecast duty:** its charter records track P50/P90 and review-after time before
implementation; "within forecast" always refers to that immutable/superseding
track forecast.

**Promotion:** starts only after M3 core structured briefs are dogfooded. Promote
only if concurrent editing is observed, core section conflicts are materially
costly, convergence/security cases pass, and operating the process does not
increase core recovery complexity. Otherwise stop and retain section locking.

**Initial WIP:** 1 exploration unit, outside release-bearing WIP.

### Track E: projections and operations

Starts after event envelope is frozen.

**Owns:** search, judgment inbox, outbox consumers, observability,
backup/restore, and scale/recovery harness.

**Deliverable:** rebuildable secondary behavior with crash/idempotency evidence.

**Initial WIP:** up to 2, subject to the global four-unit ceiling.

### Track F: automation exploration (`v1-full` candidate)

Starts only after the first core dogfood checkpoint. Agents subscribed to public
events are the default automation layer until measured repetition justifies ECA.

**Owns:** bounded trigger/condition/action schema, dry run, idempotent execution,
and kill criteria.

**Promotion:** requires at least three repeated manual patterns, no privileged
action vocabulary, and evidence that ECA reduces coordination cost. Stop on any
permission widening, recursive instability, or negative maintenance yield.

**Initial WIP:** 1 exploration unit after dogfood, outside release-bearing WIP.

### Track G: research and product verification

Independent from implementation.

**Owns:** contextual-case freeze, metric catalog, dogfood protocol, UX acceptance,
product audit, final evaluation.

**Does not own:** product implementation fixes.

**Deliverable:** reproducible evidence and independent verdicts.

**Initial WIP:** 1 research unit plus on-demand product verifier.

## 5. Serialization owners

One named integration owner at a time controls each:

| Contract | Change method |
|---|---|
| Domain vocabulary/lifecycle | decision + domain tests first |
| Permission action/resource list | threat review + matrix first |
| Event envelope/version policy | schema proposal + replay fixture first |
| OpenAPI mutation conventions | API review + generated diff first |
| Editor schema | migration/convergence fixture first |
| Migration sequence | one merge queue owner |

Feature PRs may add contract entries through those processes but may not change
the base convention opportunistically.

## 6. Milestones

### M0. Contract freeze

Deliver:

- accepted specs and advisor disposition;
- repository/toolchain skeleton;
- CI gate tiers;
- OpenAPI/event/schema lint;
- generated lifecycle/permission prose diff;
- domain/permission matrix test skeletons;
- reference organization fixture shape; and
- architecture decision records.

Exit: all skeleton gates run and fail only on explicitly unimplemented behavior.

### M1. Walking slice

One human can:

1. log in;
2. create an exploration project;
3. read it in portfolio/project UI;
4. record one decision;
5. see immutable activity; and
6. perform the equivalent flow with an agent token.

Uses real PostgreSQL, authorization, OpenAPI client, and browser. No in-memory
substitute counts.

Exit: functional, transaction/idempotency, permission, browser, and offline
Compose smoke green.

### M2. Durable realtime core

Deliver project lifecycle, forecasts, deliverables, work, dependencies, gates,
evidence, decisions, outbox, WebSocket/SSE, replay, and inbox projection.

Exit: 20-client conflict, replay checksum, outbox crash/retry, and revoke tests
green.

### M3. Agent parity and structured briefs

Deliver versioned brief sections, optimistic section locking, comments, entity
references, agent brief apply, and search text extraction.

Exit: concurrent section-conflict, permission, sanitization, revision restore,
and two-browser visual evidence green.

### M4. Complete product/dogfood-ready

Deliver portfolio priority and allocation drift, judgment queue, all
exploration/exploitation transitions, review workflow, search,
responsive/accessibility UX, observability, backup, and deployment docs.

Exit: all required end-to-end workflows green and product/security pre-review
approve dogfood.

### M5. Dogfood and hardening

Run shadow phase then treatment. Execute scale, recovery, migration, security,
accessibility, and computer-verification suites. Resolve findings through normal
gates.

Exit: treatment evidence complete or active with explicit research state; no
critical/high product or security finding.

### M6. Release candidate

Deliver SBOM/provenance, release artifacts, backup/restore evidence, benchmark
report, advisor/product/security/research verdicts, and release notes.

Exit: complete SPEC section 9 and definition of done.

## 7. Gate tiers

### Smoke (every PR)

- formatting/lint/typecheck;
- focused unit/contract tests;
- migrations apply to empty DB;
- OpenAPI/event/schema lint;
- secret/dependency diff scan; and
- changed-route/component smoke.

### Acceptance (integration milestones)

- full Go and frontend suites;
- PostgreSQL integration;
- generated permission matrix;
- replay/idempotency/outbox tests;
- Playwright required workflows;
- Yjs convergence only if collaboration has been promoted;
- accessibility scan;
- container/offline startup; and
- visual screenshots/console/network evidence.

### Release

- all acceptance gates from clean checkout;
- scale/performance;
- concurrency/recovery/backup-restore;
- dynamic security and independent review;
- multi-viewport keyboard/manual accessibility;
- research protocol/evidence audit;
- SBOM/provenance/license;
- clean installation, approved pre-v1 destructive migration, and restore; and
- release-readiness report.

Infra failures are classified and owned, but do not substitute for a green gate.

## 8. Pull request and integration policy

- One behavior/contract responsibility per PR.
- PR body names project, deliverable, acceptance criteria, evidence, and risk.
- Contract changes land before or atomically with consumers; no speculative
  duplicate implementation.
- Database migrations are forward-only and tested apply/rollback where rollback
  is supported. Destructive migration includes backup/restore fixture.
- Generated files are regenerated and diffed in the same PR.
- Reviewer uses immutable commit and reads verifier evidence first.
- Content bounce returns to the same deliverable; infrastructure failure does
  not become a content bounce.
- Merge only on green and approval. No deadline exception.
- Worktree/branch cleanup follows verified merge.

## 9. Recursive unit policy

A track may create a unit only when:

- its surface is decoupled by an accepted interface;
- coordination load exceeds the track owner, not because a mechanism is broken;
- charter names deliverable, authority, WIP, integration duty, evidence, and
  sunset;
- recursive depth remains 1; and
- integration owner accepts the additional absorption load.

Unit sunset occurs after deliverable acceptance or two consecutive declared
checkpoints with no accepted output, whichever triggers review first.

## 10. Forecast and deadline operating policy

At least at milestone boundaries and whenever assumptions materially change,
the owner records:

- actual elapsed/active/held time;
- current P50/P90 completion predictions;
- forecast basis, freshness, and review-after time;
- changed assumptions;
- scope change decision, if any;
- quality/gate status; and
- next review time.

Forecast misses are not hidden. Review asks whether decomposition, uncertainty,
capacity, dependency, or estimate was wrong. The answer becomes evidence for
future forecasts.

## 11. Risk register

| Risk | Signal | Mitigation/decision |
|---|---|---|
| Domain churn blocks tracks | repeated cross-track contract edits | freeze M0, one serialization owner |
| Jira/Notion scope imitation | features without thesis/acceptance link | require product requirement and non-goal review |
| Event sourcing complexity | projection/replay divergence | current relational state plus append-only events, replay gate |
| Collaboration consumes project | editor work dominates milestones | strict block schema and isolated track; structured state first |
| Permission leaks | cross-org test failures | deny default, generated matrix, security track |
| Agent shadow state | authoritative mutations outside API | parity gate and treatment protocol |
| Deadline gaming | late reforecasts/scope deletion | immutable history, anti-gaming metrics |
| Automation becomes code platform | requests for scripts/webhooks | keep agents-on-events as default; ECA must earn promotion |
| Research becomes post-hoc story | metrics changed after data | preregistration and dataset digest |
| Parallelism overwhelms review | growing unreviewed queue | WIP caps based on absorption |
| Self-hosted setup becomes brittle | fresh machine smoke fails | Compose, pinned deps, offline release gate |
| Scale target drives premature tuning | optimization before traces | walking slice, profile, then measured work |

## 12. Definition of ready for an issue

- linked project/deliverable;
- scope and non-scope;
- stable interface/owner;
- objective acceptance;
- dependencies;
- required gate tier;
- test/evidence location; and
- integration/review owner.

An issue without these is not ready for autonomous dispatch.

The 120-item backlog is a complete scope inventory, not 120 ready tickets. Only
M0 and the M1 walking slice may be marked ready before the slice exits. Every
later item is provisional and must be repriced, split, merged, or stopped from
walking-slice evidence before autonomous dispatch.
