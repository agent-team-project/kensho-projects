# Domain Model

## 1. Vocabulary

| Term | Meaning |
|---|---|
| Organization | Durable authorization and data boundary. |
| Actor | A human or agent service identity. Automation is a restricted execution context, not an actor kind. |
| Principal | Human or delegated role whose authority an agent exercises. |
| Portfolio | A prioritized view and governance boundary over projects. |
| Project | A bounded initiative with an outcome, mode, owner, forecast, deliverables, and history. |
| Mode | The dominant contract: `exploration` or `exploitation`. |
| Outcome | The observable change the project exists to produce. |
| Deliverable | An observable contribution to an outcome with acceptance criteria. |
| Work item | An action contributing to a deliverable or experiment. |
| Experiment | A bounded method for reducing a stated uncertainty. |
| Hypothesis | A falsifiable statement or explicit unknown. |
| Gate | A named condition whose verdict controls acceptance or transition. |
| Evidence | Attributable material supporting a claim, gate, or decision. |
| Finding | A review objection that must be resolved or explicitly withdrawn with attribution. |
| Decision | Immutable choice with alternatives, rationale, evidence, and consequences. |
| Brief | Collaboratively editable project context. |
| Ledger | Immutable ordered domain events and security audit records. |
| Intent target | Desired outcome date. |
| Forecast | Current evidence-based P50/P90 completion prediction. |
| Deadline | External constraint with a typed source; triggers attention only. |
| Reforecast | A new forecast that preserves and explains the prior forecast. |
| Hold | An explicit interval during which active progress is suspended. |
| Promotion | Decision changing exploration into exploitation. |
| Pivot | Decision replacing the active hypothesis or approach while preserving history. |

`Work item` is the only product term. A project is never called an org, job,
epic, workspace, or instance.

`contracts/lifecycles.yaml` is the canonical closed vocabulary for lifecycle
states, transitions, and activation predicates. The checked-in
`docs/generated/lifecycles.md` file is its generated documentation view. M0 CI
regenerates and diffs that view; prose here remains authoritative for
invariants not expressible in the contract schema.

## 2. Identifiers and tenancy

All persisted entities use time-sortable opaque UUIDs. Human-readable keys are
secondary and unique within an organization, for example `PRJ-142`.

Every tenant-owned table contains `org_id`. Every lookup begins with the
authorized organization scope. IDs are not security capabilities.

```go
type ID string

type ActorRef struct {
    OrgID     ID
    ActorID   ID
    ActorKind ActorKind // human | agent
    PrincipalID *ID     // required for agent actors
}

type Version uint64
```

Aggregate versions start at 1 and increment once per committed command group.

## 3. Core aggregates

### 3.1 Organization

```go
type Organization struct {
    ID        ID
    Slug      string
    Name      string
    Version   Version
    CreatedAt time.Time
}
```

Organization owns memberships, service identities, role grants, portfolios,
retention settings, and token policy.

### 3.2 Portfolio

```go
type Portfolio struct {
    ID          ID
    OrgID       ID
    Name        string
    Description string
    StewardID   ID
    TargetExplorationShare float64
    Version     Version
}

type PortfolioEntry struct {
    PortfolioID ID
    ProjectID   ID
    Rank        int
    Rationale   string
    Inputs      PriorityInputs
    DecidedBy   ActorRef
    DecidedAt   time.Time
}
```

A project has one primary portfolio. Saved views may combine portfolios but do
not create duplicate entries.

### 3.3 Project

```go
type Project struct {
    ID              ID
    OrgID           ID
    Key             string
    Title           string
    Outcome         string
    Mode            ProjectMode
    State           ProjectState
    OwnerID         ID
    PrimaryPortfolioID ID
    IntentTargetAt  *time.Time
    ActiveForecastID *ID
    Visibility      Visibility
    Version         Version
    CreatedAt       time.Time
    UpdatedAt       time.Time
}

type ProjectMode string
const (
    Exploration  ProjectMode = "exploration"
    Exploitation ProjectMode = "exploitation"
)

type ProjectState string
const (
    Proposed  ProjectState = "proposed"
    Active    ProjectState = "active"
    Held      ProjectState = "held"
    Completed ProjectState = "completed"
    Stopped   ProjectState = "stopped"
    Cancelled ProjectState = "cancelled"
)
```

### 3.4 Exploration aggregate

```go
type Hypothesis struct {
    ID              ID
    ProjectID       ID
    Statement       string
    Falsifier       string
    Status          HypothesisStatus // active | supported | contradicted | superseded | unresolved
    SupersedesID    *ID
    Version         Version
}

type Experiment struct {
    ID              ID
    ProjectID       ID
    HypothesisID    ID
    Question        string
    Method          string
    EvidenceCriteria string
    TokenBound      *int64
    TimeBound       *time.Duration
    State           ExperimentState
    Version         Version
}
```

Experiments can exceed bounds only after an attention event and a continue
decision. A timeout never terminates an experiment automatically.

### 3.5 Deliverable aggregate

```go
type Deliverable struct {
    ID          ID
    ProjectID   ID
    Title       string
    Description string
    Required    bool
    Weight      uint16 // 1..1000; explicit, not inferred from work count
    State       DeliverableState
    Criteria    []AcceptanceCriterion
    ActiveForecastID *ID
    Version     Version
}

type DeliverableState string
const (
    Draft     DeliverableState = "draft"
    Ready     DeliverableState = "ready"
    Submitted DeliverableState = "submitted"
    Bounced   DeliverableState = "bounced"
    Accepted  DeliverableState = "accepted"
    Waived    DeliverableState = "waived"
    Cancelled DeliverableState = "cancelled"
)
```

Hard gates are never waivable by any actor. A deliverable can enter `waived`
only when all hard gates attached to it have passed, no blocking finding is
open, and a policy-designated human records a `deliverable-waiver` decision
with rationale and residual risk. Waiver excludes that deliverable and its soft
gates from completion/progress denominators; it never changes or bypasses a
hard-gate verdict. A required deliverable cannot be cancelled except as part of
project cancellation.

### 3.6 Work item aggregate

```go
type WorkItem struct {
    ID            ID
    ProjectID     ID
    DeliverableID *ID
    ExperimentID  *ID
    ParentID      *ID
    Title         string
    Description   string
    State         WorkState
    Priority      WorkPriority
    AssigneeID    *ID
    Version       Version
}

type Dependency struct {
    FromWorkItemID ID
    ToWorkItemID   ID
    Kind           DependencyKind // blocks | relates | caused-by
}
```

Blocking dependencies form a directed acyclic graph within an organization.
Cross-project dependencies are allowed. `relates` and `caused-by` do not
participate in cycle detection.

### 3.7 Gate, evidence, finding, and verdict

```go
type Gate struct {
    ID            ID
    DeliverableID ID
    Name          string
    Kind          GateKind // automated | review | approval
    Hard          bool
    State         GateState
    Version       Version
}

type GateState string
const (
    GatePending GateState = "pending"
    GatePassed  GateState = "passed"
    GateFailed  GateState = "failed"
    GateWaived  GateState = "waived" // soft gates only
)

type Evidence struct {
    ID          ID
    OrgID       ID
    ProjectID   ID
    Kind        EvidenceKind // report | test-run | link | image | observation | measurement
    Title       string
    Body        string
    URI         *string
    Digest      *string
    ProducedBy  ActorRef
    ProducedAt  time.Time
    SupersedesID *ID
}

type ReviewVerdict struct {
    ID            ID
    GateID        ID
    Reviewer      ActorRef
    Result        VerdictResult // approve | bounce
    EvidenceIDs   []ID
    FindingIDs    []ID
    CreatedAt     time.Time
}
```

Evidence is immutable. Corrections supersede. Findings have `open`, `resolved`,
or `withdrawn` state and record resolution evidence.

A soft gate can enter `waived` only through `gate.soft_waive` plus a
policy-designated human `soft-gate` waiver decision with rationale and residual
risk. A hard gate rejects that command in every state.

### 3.8 Forecast

```go
type Forecast struct {
    ID             ID
    ProjectID      ID
    DeliverableID  *ID
    P50At          time.Time
    P90At          time.Time
    ReviewAfter    time.Time
    Basis          string
    Assumptions    []string
    ReasonCodes    []ForecastReason
    Impact         string
    SupersedesID   *ID
    CreatedBy      ActorRef
    CreatedAt      time.Time
}

type Deadline struct {
    ID           ID
    ProjectID    ID
    At           time.Time
    Source       DeadlineSource // contract | launch-window | demonstration | regulation | other
    Description  string
    CreatedBy    ActorRef
    CreatedAt    time.Time
    SupersedesID *ID
}
```

Forecast reason codes are additive: `scope-change`, `new-evidence`,
`dependency-change`, `capacity-change`, `quality-finding`, `incident`,
`estimate-correction`, `hold-change`, and `other`.

`DeliverableID == nil` denotes the project forecast; otherwise the forecast is
scoped to a deliverable in that project. There is one active forecast per scope.
Deliverable forecasts support milestone planning but do not count as terminal
projects in project-level calibration cohorts.

`P50At` is the owner's median prediction; `P90At` is the date by which the owner
predicts completion with 90% probability under listed assumptions. `P50At` must
not exceed `P90At`. Staleness occurs at `ReviewAfter` or when a material event
invalidates a listed assumption.

### 3.9 Decision

```go
type Decision struct {
    ID           ID
    ProjectID    ID
    Kind         DecisionKind
    Qualifier    *DecisionQualifier
    Question     string
    Choice       string
    Alternatives []Alternative
    Rationale    string
    EvidenceIDs  []ID
    Consequences []string
    DecidedBy    ActorRef
    DecidedAt    time.Time
    SupersedesID *ID
}
```

Decision kinds include `priority`, `continue`, `pivot`, `stop`, `promote`,
`scope`, `forecast`, `waiver`, `risk-acceptance`, `complete`, `cancel`, and
`architecture`.
Waiver qualifiers are the closed set `deliverable` and `soft-gate`; criteria
override is a `scope` decision, not a waiver. `hard-gate` is deliberately absent.

Every criteria override, mode/lifecycle judgment, soft-gate or deliverable waiver, scope cut,
priority override, deadline response, and privileged intervention uses this one
shape. A waiver points to its decision and remains permanently visible on
the deliverable/project release state.

## 4. Project lifecycle

### 4.1 Common transitions

```text
proposed -> active
proposed -> cancelled
active   -> held
held     -> active
active   -> completed   (exploitation only)
active   -> stopped     (exploration only)
active   -> cancelled
held     -> cancelled
```

Terminal projects are immutable except for comments, retrospective evidence,
and a corrective decision that explicitly reopens into a new project. V1 does
not reopen a terminal project in place.

### 4.2 Activation

Exploration activation requires:

- outcome or decision question;
- active hypothesis/unknown;
- decision criteria;
- owner;
- bounds; and
- forecast.

Exploitation activation requires:

- outcome;
- at least one required deliverable;
- acceptance criteria on each required deliverable;
- owner;
- forecast; and
- priority rationale.

### 4.3 Promotion

Promotion is one transaction that:

1. appends a `promote` decision;
2. records supported/contradicted/unresolved hypotheses;
3. records accepted residual uncertainty;
4. changes mode to exploitation;
5. creates the initial required deliverables; and
6. emits `project.promoted` after commit.

Promotion never deletes experiments, evidence, or prior forecasts.

Decision criteria, promotion thresholds, and stop criteria must be recorded
before the evidence they evaluate. An override is legal only through a decision
that names the original criterion and remains visible in the ledger.

Every pivot renews or shrinks experiment bounds. A third pivot requires a
portfolio-steward decision before more experiment work begins.

### 4.4 Completion

Completion is legal only when:

- mode is exploitation;
- all required deliverables are `accepted` or policy-valid `waived`;
- every hard gate passes;
- no open blocking finding exists;
- actual outcome and residual risks are recorded; and
- a completion decision exists in the same command group.

A waived deliverable satisfies the deliverable-presence check only through the
waiver invariant in section 3.5. Hard gates on that deliverable still must have
passed. Soft gates are excluded only after the waiver decision commits.

## 5. Progress and health

### 5.1 Exploitation progress

If deliverable weights are explicit, progress is accepted required weight divided
by total required weight. Otherwise every required deliverable weighs equally.
Submitted and bounced deliverables contribute zero accepted progress.

### 5.2 Exploration readiness

Exploration has no completion percentage. It reports:

- hypothesis evidence status;
- decision-criterion coverage;
- active experiments;
- bound consumption;
- remaining uncertainty; and
- next decision checkpoint.

A stopped exploration with a findings/decision record is a successful
information outcome. Portfolio views report information yield and do not render
`stopped` as failed.

### 5.3 Health

Health is a projection with explained signals, not an editable color:

- `on-track`: no material signal;
- `attention`: P90 spread wide, target risk, bound threshold, or stale
  decision;
- `blocked`: unresolved blocking dependency or hard gate;
- `held`: explicit state; and
- `unknown`: insufficient current forecast/evidence.

Each health label lists the signals that produced it.

## 6. Priority model

```go
type PriorityInputs struct {
    OutcomeValue              Level
    Urgency                   Urgency
    UncertaintyReductionValue Level
    ValueConfidence           float64
    EffortLowDays             float64
    EffortHighDays            float64
    DependencyLeverage        Level
    RiskReduction             Level
    OpportunityCost           string
}
```

A suggested rank may use a versioned documented formula, but stored final rank
is a decision. Formula version and input snapshot are recorded. Overrides require
rationale. A formula change never rewrites historical priority decisions.

## 7. Event model

Every successful mutating command appends one or more events with a shared
`command_id` and strictly increasing aggregate version.

```go
type DomainEvent struct {
    EventID          ID
    OrgID            ID
    AggregateType    string
    AggregateID      ID
    AggregateVersion Version
    EventType        string
    SchemaVersion    uint16
    Actor             ActorRef
    CommandID         ID
    CorrelationID     ID
    CausationID       *ID
    OccurredAt        time.Time
    Payload           json.RawMessage
}
```

Required event families:

- `organization.*`, `membership.*`, `service_identity.*`;
- `portfolio.*`, `priority.*`;
- `project.created|activated|held|resumed|promoted|completed|stopped|cancelled`;
- `project.outcome_changed|owner_changed|visibility_changed`;
- `hypothesis.*`, `experiment.*`, `observation.*`;
- `deliverable.*`, `gate.*`, `review.*`, `finding.*`, `evidence.*`;
- `work_item.*`, `dependency.*`;
- `forecast.created`, `forecast.superseded`, `forecast.stale`, `target.changed`, `deadline.changed`;
- `decision.recorded`, `decision.superseded`;
- `comment.*`, `mention.created`;
- `judgment_item.created|claimed|resolved`, `inbox.*`, and optional `automation.*`; and
- `brief.section_updated`, `brief.snapshot_created|restored`; after collaboration
  promotion, binary Yjs updates remain in the Yjs store and snapshot events
  retain their digest/index.

Event payload schemas are versioned JSON Schemas. Before v1, schema changes use
a coordinated ledger migration: archive and checksum the prior ledger, produce
one current-schema ledger, regenerate projections/fixtures, and remove old
readers/writers. There are no aliases, upcaster chains, or dual payload paths.

## 8. Command invariants

1. Every mutation is authorized against actor, organization, resource, and
   action.
2. The command supplies expected aggregate version unless it is an idempotent
   create with a unique key.
3. A repeated idempotency key with identical body returns the original result.
4. A repeated idempotency key with a different body returns `409`.
5. Aggregate version gaps and duplicate versions are impossible.
6. Events and current projections commit in one database transaction.
7. Blocking dependency cycles are rejected before commit.
8. An actor cannot review its own submitted gate when independence is required.
9. Targets, forecasts, deadlines, and elapsed dates cannot cause approval,
   completion, cancellation, waiver, or any other domain transition.
10. Deleting an actor never deletes attribution; the actor becomes inactive.
11. Project mode changes only through promotion or a new project.
12. Immutable evidence and decisions are superseded, never updated or deleted.
13. Brief content cannot execute structured commands implicitly.
14. Search and realtime never reveal resources the actor cannot query directly.
15. Hard gates are never waivable; deliverable waiver requires every attached
    hard gate passed and the closed human-only waiver policy.

## 9. Persistence model

Authoritative relational tables store current aggregates. `domain_events` is
append-only and independently replayable. The same transaction updates:

- aggregate tables;
- `domain_events`;
- read projections that require transactional freshness;
- `search_dirty` markers; and
- `outbox` records.

Asynchronous workers update full-text search, judgment/inbox projections,
analytics, and external realtime delivery. Every worker uses idempotent event
checkpoints.

Core rich briefs are versioned structured documents split into stable sections.
Section writes use optimistic versions, and document revisions are immutable.

If the CRDT collaboration exploration is promoted, Yjs updates are stored
separately by document id, sequence, actor, and digest. Periodic snapshots compact
updates without deleting the audit index.
Structured brief references contain entity ids and display hints but resolve
current values through authorized queries.

## 10. Time and deletion

All persisted timestamps are UTC. User time zones affect rendering only.

V1 supports archive, not destructive deletion, for projects and durable
evidence. Organization deletion is an explicit operator-only maintenance action
outside ordinary product flows and produces an export plus deletion audit.

## 11. Work item lifecycle

The work state machine is fixed. `blocked` is a derived health signal, never a
work-item state:

```text
open -> in_progress -> in_review -> done
  |          |             |
  +----------+-------------+-> cancelled
in_review -> in_progress
```

Custom workflow graphs are not supported. A work item linked to a review-gated
deliverable may reach `done` only after its own work review, but deliverable
acceptance remains a separate command.

## 12. Kensho/Workplane vocabulary mapping

Same concepts use the same word; different concepts use visibly different
words.

| Kensho runtime concept | Workplane concept | Rule |
|---|---|---|
| org | organization | Same trust/identity boundary; use org in URIs only where contracted. |
| role | actor role | Same authority concept, scoped differently. |
| seat/occupancy | agent service identity/session | Different: Workplane persists identity, not runtime process lifecycle. |
| job | work item | Different: Kensho job is orchestration execution; Workplane work item is product planning. |
| unit/track | project participant group | Different: Workplane does not manage Kensho org topology. |
| project | project | Same bounded outcome initiative. |
| brief | brief | Same editable intent/context concept. |
| decision | decision | Same immutable judgment primitive. |
| evidence | evidence | Same attributable support for a claim. |
| gate | gate | Same transition precondition consuming evidence. |
| ledger | ledger | Same append-only history principle. |
| inbox | inbox | Same actor attention queue, different transport implementation. |

The dogfood adapter may link corresponding ids but cannot translate one concept
into another with an alias or shadow record.
