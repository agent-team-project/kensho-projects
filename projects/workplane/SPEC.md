# Workplane - Build-Ready Specification

**Status:** M1 walking slice plus M2A durable delivery/replay spine implemented; remaining durable-core and release scope remains specified
**Version:** 0.1
**Audience:** Kensho managers, builders, reviewers, product verifiers, and
research auditors

This document is the acceptance authority for v1. Detailed contracts live in
the linked documents, but no detailed document may weaken this specification.

## 1. Product thesis

Workplane is a self-hosted project and portfolio system designed for work shared
by humans and autonomous agents. It makes five things explicit and connected:

1. why a project deserves attention;
2. whether it is exploring uncertainty or exploiting established knowledge;
3. what observable deliverables constitute success;
4. what evidence supports decisions and completion; and
5. what the current forecast means, including uncertainty and reforecasts.

Workplane is not a generic issue tracker. Its primary claim is:

> For a small human team directing a large agent fleet, one actor-neutral,
> evidence-gated project layer reduces human absorption cost and improves
> evidence quality at equal or better gate integrity.

Subsidiary claims are that P50/P90 forecasts become calibrated, exploration
produces reusable decisions, agents coordinate through the public product plane,
and Kensho can operate a live stateful service through its own dogfood.

## 2. Demonstration and research purpose

Excel-lite demonstrated large-scale parallel breadth through 148 independent
function implementations. Workplane must demonstrate a different and harder
capability: coordinated delivery across stateful, security-sensitive,
concurrent subsystems whose interfaces cannot be merged by simple fan-out.

The build tests whether Kensho can:

- establish and preserve a coherent domain model across many implementation
  tracks;
- ship multi-user realtime behavior with deterministic durable history;
- let human and agent actors use one permissioned command surface;
- integrate a collaborative brief editor without making documents the source
  of truth for structured project state;
- measure its own prioritization and forecasting behavior; and
- dogfood the product on a consequential Kensho project.

The reference persona is 1-10 humans directing 10-100 agents, with human
judgment and review absorption as the scarce resource. James directing Kensho
is the first concrete instance. The public demonstration is the running product
plus its reproducible dogfood report, not a ten-second feature demo.

The research thesis is falsifiable. A polished app is not sufficient if its
instrumented dogfood study does not produce trustworthy evidence.

## 3. Product principles

### P1. One domain, two actor kinds

Humans and agents are both actors. They use the same project commands,
permission checks, activity ledger, and evidence rules. Agent-only operational
metadata may exist, but there is no shadow project state available only to
agents.

### P2. Projects are outcome contracts

A project owns an outcome, mode, priority rationale, best-effort forecast,
deliverables, evidence, decisions, and work. Completion is derived from accepted
deliverables, never from task count or prose status.

### P3. Exploration and exploitation are different contracts

Exploration reduces uncertainty and ends in a recorded decision: continue,
pivot, stop, or promote. Exploitation delivers an accepted outcome. A project
has one dominant mode at a time; mode changes require a decision with evidence.

### P4. Deadlines inform; gates govern

A target date expresses intent. A forecast expresses current P50/P90 beliefs
and their basis. A deadline expresses an external constraint. Staleness or a
miss triggers visibility, reforecasting, and learning. No date automatically
approves, merges, completes, cancels, or bypasses a gate.

### P5. History is evidence

Every domain mutation produces an immutable, attributable event. Current views
are projections. Project briefs provide context but do not silently mutate
structured state.

### P6. Priority stays legible

The product may calculate transparent indicators, but it may not hide priority
inside an opaque score. Expected value, urgency, uncertainty reduction,
confidence, effort, dependencies, and opportunity cost remain inspectable with
the decision rationale.

### P7. Quality beats schedule

Forecasts are best-effort. When reality changes, scope or forecast changes
openly. Acceptance criteria and security/review gates do not contract to meet a
date.

### P8. One universal decision primitive

Promotion, pivot, stop, priority override, scope change, forecast authority,
soft-gate or deliverable waiver, completion, cancellation, and criteria override all use the same
immutable decision shape. Judgment is not scattered across special-case status
fields.

### P9. Operation is part of the product

Migrations on a populated ledger, backup/restore, revocation, recovery, and
replay are user-facing correctness. A feature-complete app that cannot be safely
operated fails the study.

## 4. Users and actor classes

- **Fleet director/operator:** creates portfolios and projects, sets direction, resolves
  priority conflicts, and approves sensitive actions.
- **Project owner:** owns outcome, forecast, integration, and completion.
- **Contributor:** performs work and submits evidence.
- **Reviewer:** evaluates deliverables and gates independently.
- **Agent service identity:** acts through scoped tokens under the same project
  roles and commands as a person.
- **Observer:** reads permitted state and reports without mutation rights.

An actor can hold several roles in different projects. Production policy should
keep implementer and reviewer distinct for the same gated deliverable.

Agent actors must record a principal: the human or delegated role whose
authority they exercise. Human-required judgments are permission data, not
buttons that merely happen to exist only in the UI.

## 5. V1 scope and staging

The complete target is `v1-full`. The first dogfood gate is `v1-core`.

`v1-core` contains the thesis-bearing project/portfolio domain, universal
decisions, P50/P90 forecasts, deliverables/gates/evidence, public human/agent
API parity, event ledger/replay, realtime projections, judgment queue, search,
versioned rich briefs, permissions, and operated-service recovery.

`v1-full` adds separately promoted exploration tracks: simultaneous CRDT brief
co-editing/presence and a bounded declarative automation engine. Their
specifications and hard gates remain in this package. They may not delay the
core dogfood checkpoint; failure to promote records a stop decision and moves
the capability to a later project rather than weakening its acceptance bar.

### 5.1 Portfolio and priority

- Multiple portfolios within one organization.
- Projects grouped by portfolio, owner, mode, health, state, and time horizon.
- Transparent priority inputs and a human/agent-authored rationale.
- Declared target exploration/exploitation allocation and measured actual mix,
  with drift shown as attention rather than auto-enforced scheduling.
- Dependency and capacity warnings without automatic scheduling claims.
- Portfolio table, board, and timeline views.

### 5.2 Project contract

- Title, outcome, mode, state, owner, participants, tags, and visibility.
- Intent target, current P50/P90 forecast, review-after time, forecast basis,
  typed deadline, and actual completion.
- Deliverables with acceptance criteria and evidence gates.
- Work items and typed dependencies.
- Rich collaborative brief.
- Decision ledger, evidence attachments/links, comments, and activity history.
- Judgment queue showing decisions, reviews, waivers, and interventions blocked
  on scarce human attention.
- Hold, resume, reforecast, complete, cancel, pivot, and promote operations.

### 5.3 Exploration

- Hypotheses, experiments, observations, evidence, and decision criteria.
- Explicit uncertainty statement and expected value of information.
- Time/token/resource bounds treated as review triggers, not forced shutdowns.
- Decision criteria and kill/promotion thresholds pre-registered before evidence;
  overrides are universal decisions and remain visible.
- Every pivot renews or shrinks bounds; the third pivot requires portfolio-level
  review.
- Decisions: continue, pivot, stop, or promote to exploitation.
- Promotion creates or transforms the exploitation contract while retaining the
  complete exploration history.

### 5.4 Exploitation

- Outcome broken into independently accepted deliverables.
- Work dependencies, review gates, release readiness, and residual-risk record.
- Progress derived from deliverable acceptance, weighted only by explicit
  deliverable weights.
- Completion requires all required deliverables accepted and no blocking gate.

### 5.5 Shared human/agent operation

- Browser UI and versioned HTTP API expose equivalent domain commands.
- Scoped service tokens identify an agent and its delegated authority.
- Idempotency keys make retries safe.
- Optimistic concurrency prevents silent lost updates.
- Realtime subscriptions expose the same committed domain events used by UI
  projections.
- Every action records actor kind, actor id, request id, and origin.

### 5.6 Collaboration and knowledge

- Core versioned rich-brief sections with optimistic locking, immutable
  revisions, typed same-section conflicts, comments, and restore.
- Optional promoted CRDT brief editing with presence and conflict-free merge.
- Brief blocks: headings, paragraphs, lists, checklists, code, quote, table,
  links, and references to project entities.
- Comments and mentions on projects, deliverables, work items, decisions, and
  brief selections.
- Full-text search across structured records and rendered brief text.
- Version history and restore for briefs; immutable decisions and evidence are
  corrected by superseding events rather than rewritten.

### 5.7 Automation and inbox

- Optional promoted deterministic event-condition-action rules over an
  allowlisted action set.
- Optional ECA dry-run preview before enabling a rule.
- Optional ECA at-least-once execution with idempotent effects and a ledger.
- In-app inbox for assignments, mentions, review requests, forecast risk, and
  automation failures.
- No arbitrary code, shell, template execution, or unbounded webhook fan-out.

The inbox and event subscriptions are `v1-core`. The ECA rule compiler/executor
is `v1-full` and begins only after dogfood demonstrates repeated deterministic
actions whose agent implementation has material absorption cost.

### 5.8 Deployment and operations

- Self-hosted local deployment through one Docker Compose command.
- PostgreSQL is the durable system of record.
- No paid service or external network is required at runtime.
- Backup, restore, schema migration, health, metrics, traces, and structured
  logs are documented and tested.

## 6. Explicit non-goals

- No attempt at Jira, Notion, Linear, or Confluence feature parity.
- No billing, marketplace, plugins, arbitrary custom code, or public SaaS
  multi-tenancy.
- No automatic resource allocation or opaque AI priority score.
- No general workflow-language designer in v1.
- No email, Slack, calendar, or third-party PM synchronization in v1.
- No binary attachment library; evidence uses text, links, and small images
  within a configured size limit.
- No mobile-native app. The web UI must remain usable for review and updates on
  a narrow viewport.
- No complete offline-first structured project database. Temporary brief edits
  may buffer locally, but durable project commands require the server.
- No hard deadline enforcement, automatic project cancellation, or approval by
  timeout.
- No compatibility aliases or dual schemas before v1. Superseded concepts are
  updated directly with migrations and fixtures.
- No custom-field/schema designer, workflow designer, dashboard/query builder,
  velocity/story-point metric, punctuality leaderboard, or actor-level schedule
  score.
- No Jira/Notion import, external search engine, Redis/NATS queue, generalized
  capacity optimizer, or critical-path scheduling solver.

## 7. System shape and hard boundaries

`v1-core` is a modular monolith:

- a Go API/application server;
- PostgreSQL for domain state, event history, projections, search, and outbox;
- a React/TypeScript web application;
- versioned structured rich briefs with section-level optimistic locking; and
- generated OpenAPI clients and shared JSON Schemas at module boundaries.

Hard boundaries:

1. Domain commands commit structured state, events, projections, and outbox
   records atomically in PostgreSQL.
2. WebSocket messages are notifications of committed events, never a second
   source of truth.
3. The rich brief cannot directly change project state. Entity embeds
   resolve read-only data; structured mutations call the normal API.
4. If promoted, the collaboration service accepts short-lived document grants
   issued only after the API server authorizes the actor.
5. Search is a projection and can be rebuilt from authoritative state.
6. If promoted, ECA automation consumes committed events and invokes ordinary
   idempotent domain commands under a restricted agent identity carrying
   rule/event execution context.
7. Authorization is enforced server-side for every command and query. UI
   visibility is not a security boundary.
8. Every UI mutation maps to a public command. Every public command is either UI
   reachable or explicitly registered as headless with rationale. CI audits
   parity and forbids internal mutation endpoints.

If the collaboration exploration is promoted, a self-hosted Yjs process is
added behind document-scoped grants. It remains detachable and cannot become a
structured-state authority.

Full interfaces and module ownership are defined in `docs/ARCHITECTURE.md` and
`docs/API.md`.

## 8. Required end-to-end workflows

`v1-core` is incomplete unless automated browser/API acceptance covers every
non-conditional workflow. A promoted `v1-full` track must additionally cover
its conditional workflow:

1. Create an exploration project with hypothesis, bounds, decision criteria,
   intent target, and forecast.
2. Add experiments and evidence, then record a continue decision.
3. Pivot an exploration project without deleting its prior hypothesis.
4. Promote exploration into exploitation with deliverables and accepted
   residual uncertainty.
5. Create an exploitation project directly and decompose its outcome into
   gated deliverables and dependent work.
6. Reforecast with reason, P50/P90, basis, review-after time, and impact while
   retaining the previous forecast.
7. Submit deliverable evidence, request independent review, bounce it with
   findings, resubmit, and approve it.
8. Complete a project only after all required deliverables and gates pass.
9. Have a human and an agent concurrently update distinct project fields and
   observe both changes in realtime.
10. Produce a version conflict for concurrent edits to the same structured
    field and resolve it without data loss.
11. Concurrently edit separate brief sections and prove both commit; then issue
    a stale same-section edit and prove a typed conflict prevents loss. For
    promoted `v1-full` collaboration, disconnect/reconnect two browser contexts
    and prove CRDT convergence.
12. For promoted `v1-full` ECA, create an automation in dry-run, enable it,
    trigger it twice, and prove one idempotent effect with a complete execution
    ledger.
13. Search for terms present in a project, decision, evidence item, comment, and
    brief; enforce visibility in every result.
14. Revoke an agent token and prove new API access fails immediately and every
    realtime connection closes within five seconds on the reference deployment.
15. Backup a populated organization, restore it into an empty deployment, and
    prove domain/event/brief checksums and permissions match.

## 9. Objective acceptance bar

### 9.1 Correctness and history

- Domain invariant and state-machine suites pass for every command.
- Replaying the immutable event ledger into empty projections yields the same
  canonical checksum as the live projections for the reference dataset.
- Property tests generate at least 100,000 valid command sequences with zero
  invariant violation or panic.
- Every accepted command has exactly one attributable domain event group and
  one stable request id.
- Idempotent replay of every mutating API request produces no duplicate event or
  side effect.

### 9.2 Concurrency and realtime

- A 20-client structured-update test produces no silent lost update; stale
  writes receive a typed version conflict.
- At 50 connected local clients, committed domain events reach subscribers at
  p95 under 500 ms and p99 under 1,500 ms on the reference laptop.
- If collaboration is promoted, a partition/reconnect matrix for two- and
  three-client briefs converges to identical Yjs state vectors in 100% of
  seeded trials.
- Server restart during outbox delivery produces no missing committed event and
  no duplicate domain effect.

### 9.3 Security and permissions

- The generated role/action/resource matrix passes with 100% expected allow and
  deny results across human, agent, and revoked actors, plus restricted ECA
  execution contexts when that track is promoted.
- Cross-organization reads and writes fail in API, realtime, search, brief, and
  export paths.
- Stored rich content passes the sanitization corpus with no executable script,
  event handler, unsafe URL, or privilege-changing embed.
- Secret scanning, dependency auditing, static analysis, CSRF checks, token
  rotation/revocation, and rate-limit tests are green.
- The activity ledger records every security-sensitive action and never records
  credential material.

### 9.4 Performance and scale

On a documented Apple Silicon reference laptop with 10,000 projects, 250,000
work items, 1,000,000 domain events, and 100,000 searchable brief/comment
records:

- portfolio table query p95 is under 250 ms;
- project overview query p95 is under 200 ms;
- full-text search p95 is under 500 ms;
- create/update command p95 is under 300 ms excluding client network time;
- event-ledger pagination p95 is under 300 ms; and
- the web application becomes interactable in under 2.5 seconds on a warm local
  deployment.

Every performance report includes fixture seed, hardware, database settings,
sample count, p50/p95/p99, and raw output.

### 9.5 UX and accessibility

- Playwright acceptance passes at 1440x900, 1024x768, and 390x844 without
  incoherent overlap, clipped commands, or unreachable required workflows.
- Keyboard-only acceptance covers global navigation, project creation, work
  movement, brief editing, command palette, review, and reforecasting.
- Automated accessibility reports have zero critical or serious violations;
  manual checks cover focus order, landmarks, names, error association, contrast,
  reduced motion, and screen-reader announcements for realtime changes.
- Browser screenshots and video evidence are stored for every release workflow.

### 9.6 Reliability and deployment

- Fresh install, each approved clean pre-v1 schema migration, backup/restore,
  graceful shutdown, unclean restart, and health/readiness tests pass. No
  compatibility reader or alias survives a migration.
- The runtime makes no external network request in the offline deployment test.
- A release artifact starts with one documented command and reaches ready state
  from an empty machine with only Docker installed.
- OpenTelemetry traces correlate API command, database transaction, outbox
  delivery, and realtime notification, plus automation execution only when
  promoted, without including document bodies or secrets.

### 9.7 Research evidence

- The dogfood project is run from inception inside Workplane, not reconstructed
  after completion.
- Contextual-case and treatment definitions are frozen before treatment data is
  read; historical cases are not described as a controlled baseline.
- The evaluation reports forecast error, reforecast timing, idle intervals,
  decision latency, review bounce rate, gate outcomes, context-recovery time,
  project-state divergence, and manual coordination interventions.
- A public report states which hypotheses were supported, contradicted, or
  unresolved. Negative results do not block software release, but missing or
  manipulated evidence does.

### 9.8 Staged enforcement

The acceptance bar is staged, never deferred wholesale to release:

| Gate begins | Mandatory evidence |
|---|---|
| Contract freeze | requirement/case/schema lint, permission vocabulary, research digest |
| Walking slice | real DB transaction, idempotency, event/replay seed, UI/API parity, browser smoke |
| Durable core | lifecycle/property, conflict, outbox crash/retry, revoke, realtime order |
| Dogfood-ready | complete v1-core workflows, security matrix, search, backup/restore, a11y, offline operation |
| Full-track promotion | CRDT convergence or ECA value/behavior gates for that track |
| Release candidate | scale, recovery, full browser matrix, independent security/product/research reviews |

The same command families run from their first gate onward so acceptance debt
cannot accumulate into an RC bounce storm.

## 10. Definition of done

V1 is done only when:

1. every required workflow in section 8 passes through public interfaces;
2. every acceptance category in section 9 is green with durable evidence;
3. the selected dogfood project has completed, reached a recorded terminal
   decision, or reached a pre-registered checkpoint review in Workplane;
4. an independent product review and security review approve the release;
5. backup/restore and offline self-hosted installation are independently
   reproduced; and
6. the research evaluation is published, including negative findings.

No deadline, token budget, demo date, or amount of completed code weakens this
definition.

## 11. Delivery posture

The current phase is exploration and specification. This package is a
pre-slice hypothesis, not complete product law. Implementation starts only
after a reviewed decision promotes the project to exploitation. This static
specification assigns no calendar duration to a deliverable. At kickoff, the
owner creates timestamp-based P50/P90 forecasts from current fleet throughput,
review absorption, and integration uncertainty, then supersedes them with
evidence at each milestone. Quality gates remain fixed regardless of forecast
movement.

The parallelization and issue plan is in `docs/DELIVERY.md` and
`backlog/README.md`.

## 12. Study and detachable-track stop criteria

The Workplane study stops or pivots if the walking slice shows the contract/event
core cannot be absorbed with two tracks, if dogfood repeatedly returns to
mailboxes/files for thesis-level reasons, if maintenance cost exceeds measured
absorption benefit after shadow review, or if gate integrity worsens under date
pressure.

The CRDT collaboration track promotes only when convergence, permission,
restart recovery, and two-browser acceptance are green within its forecast. If
not, `v1-core` keeps versioned section-locked briefs and the track records a stop
decision.

The ECA automation track starts only after first dogfood and promotes only if
the ledger shows repeated deterministic actions whose agent implementation has
material absorption cost. Otherwise agents subscribed to public events remain
the automation layer.
