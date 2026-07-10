# Product Contract

## 1. Positioning

Workplane is a project and portfolio product for teams where people and software
agents plan, execute, review, and learn together.

Its nearest category is project management, but its product boundary differs
from conventional trackers:

- a task is not the top-level unit of value;
- a deadline is not a hidden quality override;
- an activity feed is not an audit trail unless it is complete and replayable;
- an agent is not an integration that bypasses ordinary permissions; and
- exploration is not a backlog of vague tasks waiting to become implementation.

The reference persona is a small human team (1-10) directing an agent fleet
(10-100), where human judgment and review absorption are scarce. The product
joins structured execution with a rich project brief. Jira's public
product model emphasizes goals, tasks, boards, timelines, dependencies,
automation, and reports. Notion's public model combines projects, tasks,
dependencies, timelines, and database records that are also rich pages. Workplane
uses those proven interaction shapes but differentiates on project contracts,
mode transitions, evidence, forecast calibration, and actor parity.

References:

- https://www.atlassian.com/software/jira/features
- https://www.atlassian.com/software/jira/guides/basic-roadmaps/overview
- https://www.notion.com/en-GB/product/projects
- https://www.notion.com/help/intro-to-databases
- https://www.notion.com/help/collaborate-with-people

## 2. Target users

### 2.1 Primary user: project owner in a human-agent team

The owner needs to state the outcome, decide whether work is exploratory or
exploitative, expose priority tradeoffs, coordinate dependencies, preserve
context, and know whether the current forecast is credible. The owner can be a
person or an agent with delegated authority.

### 2.2 Primary user: autonomous manager

The manager needs a stable machine-readable contract from which it can propose
work, dispatch contributors, request review, report forecast change, and
escalate decisions. It must not infer project truth from unstructured chat.

### 2.3 Primary user: fleet director/operator

The operator needs a quiet, legible portfolio view that answers:

- What outcomes are active, and why now?
- Which projects are exploring versus exploiting?
- What are the P50/P90 forecast, freshness, and deadline status?
- What is blocked, by what dependency or decision?
- What changed since the last review?
- Which claims have evidence and independent acceptance?
- What specifically is waiting for my judgment, and what is the cost of delay?

### 2.4 Secondary users

- Contributors need clear work, context, dependency, and submission contracts.
- Reviewers need immutable evidence and independence from implementation.
- Observers need accurate read-only status without operational noise.
- Researchers need trustworthy event and outcome data without scraping UI text.

### 2.5 Non-target users

V1 is not designed for public SaaS administration, customer support ticketing,
sales CRM, personal notes, general document wikis, payroll, or arbitrary
business-process automation.

## 3. Jobs to be done

### J1. Choose what deserves attention

When several plausible initiatives compete for finite absorption capacity, help
an operator compare value, urgency, uncertainty reduction, dependencies,
confidence, effort, and opportunity cost without hiding the decision in a score.

### J2. Turn intent into an outcome contract

When an initiative is selected, capture a project whose completion can be
judged from deliverables and evidence rather than task motion.

### J3. Explore without pretending to deliver

When material uncertainty remains, define hypotheses, experiments, bounds, and
decision criteria so learning can conclude, pivot, stop, or promote.

### J4. Exploit without losing context

When the path is sufficiently understood, connect the brief, deliverables,
dependencies, work, evidence, reviews, and forecast so execution remains
legible as agents and people change.

### J5. Reforecast honestly

When assumptions change, preserve the prior forecast and require a reason,
P50/P90, basis, review-after time, and impact statement. Make forecast change
visible without treating it as failure or silently moving the date.

### J6. Recover context quickly

When an actor joins, resumes, or reviews a project, provide the current contract,
recent decisions, open risks, pending gates, and relevant brief sections without
requiring transcript archaeology.

### J7. Learn from outcomes

When a project ends, compare intended outcome, forecast history, decisions,
evidence, accepted deliverables, interventions, and actual result. Feed learning
into future project setup without rewriting history.

## 4. Product model

### 4.1 Organization and portfolio

An organization is the authorization and data boundary. A portfolio is a
decision surface inside an organization. Projects can appear in one primary
portfolio and multiple saved views, but ownership and state remain singular.

### 4.2 Project

A project is a bounded initiative with:

- one outcome statement;
- one dominant mode;
- one accountable owner;
- a priority rationale;
- an intent target and current forecast;
- required deliverables;
- a brief;
- decisions and evidence;
- work and dependencies; and
- a terminal state or explicit hold.

All judgment records use one decision primitive. Priority override, scope cut,
forecast authority, criteria override, soft-gate or deliverable waiver, promotion, pivot, stop,
completion, and cancellation differ by decision kind, not storage shape.

### 4.3 Deliverable and work item

A deliverable is an observable part of the outcome and owns acceptance criteria
and gates. A work item is an action that contributes to a deliverable or
experiment. Completing work does not accept a deliverable.

### 4.4 Brief and ledger

The brief is editable context: problem, approach, constraints, plans, and notes.
The ledger is immutable domain history. A brief can explain a decision; it
cannot replace the decision record.

### 4.5 Core product requirements

- `PR-CORE-01`: Project creation uses one public command for equivalently scoped
  human and agent principals and records one attributable event group.
- `PR-CORE-02`: Every authoritative UI mutation maps to a public operation and
  no agent-private or browser-private project state exists.
- `PR-DEC-01`: Every consequential judgment records the universal decision
  shape with question, choice, alternatives, rationale, evidence,
  consequences, actor, time, and supersession.
- `PR-BRIEF-01`: Human and agent edits use the same versioned section contract;
  stale same-section writes return a typed conflict and never silently replace
  text.
- `PR-BRIEF-02`: Revision restore creates a new attributed revision and retains
  the complete prior history.

## 5. Exploration product requirements

- `PR-EXP-01`: An exploration project must have at least one falsifiable
  hypothesis or explicit unknown.
- `PR-EXP-02`: It must define evidence sought, decision criteria, and bounds.
- `PR-EXP-03`: Experiments must record method, observation, evidence links, and
  conclusion separately.
- `PR-EXP-04`: A review checkpoint presents remaining uncertainty and expected
  value of more information.
- `PR-EXP-05`: Continue, pivot, stop, and promote are explicit decisions.
- `PR-EXP-06`: Pivot supersedes a hypothesis without deleting it.
- `PR-EXP-07`: Promotion records accepted uncertainty and creates an
  exploitation outcome/deliverable contract.
- `PR-EXP-08`: Exploration progress is expressed as evidence coverage and
  decision readiness, never percent complete.
- `PR-EXP-09`: Bounds emit attention but never auto-stop an experiment.
- `PR-EXP-10`: A stopped exploration remains searchable and can be cited by
  future projects.

## 6. Exploitation product requirements

- `PR-EXE-01`: An exploitation project must have at least one required
  deliverable before activation.
- `PR-EXE-02`: Every required deliverable has objective acceptance criteria.
- `PR-EXE-03`: Deliverable gates distinguish submitted, bounced, and accepted.
- `PR-EXE-04`: Reviewer findings are durable and resolved explicitly.
- `PR-EXE-05`: Project progress derives from accepted deliverables, not work
  item status.
- `PR-EXE-06`: Dependency cycles are rejected for blocking dependencies.
- `PR-EXE-07`: Project completion fails while any required deliverable or gate
  is incomplete.
- `PR-EXE-08`: Completion records actual outcome, residual risk, and follow-up.
- `PR-EXE-09`: Cancellation requires a reason and disposition for accepted
  deliverables and open evidence.
- `PR-EXE-10`: Release readiness is a report/gate, not an automatic tag or
  external side effect.

## 7. Forecast and deadline product requirements

Workplane distinguishes three distinct date concepts and actual completion:

1. **intent target** - when the owner wants the outcome;
2. **forecast** - current P50 and P90 completion predictions;
3. **deadline** - an external constraint with typed source such as contract,
   launch window, or demonstration; and
4. **actual completion** - observed terminal time.

- `PR-FC-01`: A project- or deliverable-scoped forecast records basis,
  assumptions, P50, P90, review interval, and author; one forecast is active per
  scope.
- `PR-FC-02`: Reforecasting never mutates the previous forecast.
- `PR-FC-03`: A reforecast requires one or more typed reasons and an impact note.
- `PR-FC-04`: Approaching or missing a target creates attention, not a state
  transition.
- `PR-FC-05`: The UI never labels a forecast as a promise or a miss as a quality
  failure.
- `PR-FC-06`: Scope removal is a decision linked to the forecast it affects.
- `PR-FC-07`: P50/P90 calibration is reported across projects.
- `PR-FC-08`: Forecast accuracy excludes periods where a project is formally
  held, with hold intervals reported separately.
- `PR-FC-09`: Owners cannot remove forecast history or change actual dates.
- `PR-FC-10`: No automation can approve a gate because a date elapsed.
- `PR-FC-11`: A stale forecast is surfaced when its review interval expires or
  a material invalidating event occurs.
- `PR-FC-12`: Deadline pressure can trigger a scope decision but never a gate
  transition or default waiver.
- `PR-FC-13`: Deliverable forecasts are reported separately and never inflate
  the terminal-project calibration cohort.

## 8. Priority product requirements

Priority inputs are structured but judgment remains explicit:

- outcome value: `low`, `medium`, `high`, `critical`;
- urgency: `none`, `time-sensitive`, `expiring`, `incident`;
- uncertainty reduction value: `low`, `medium`, `high`;
- confidence in value: 0.0 to 1.0;
- estimated effort range;
- dependency leverage;
- risk reduction;
- opportunity cost statement; and
- written rationale.

The product may sort or filter on any input and may display a documented
suggested rank. It must show the inputs, formula, and overrides. A project owner
or portfolio steward records the final rank and rationale. No model-generated
rank is authoritative.

## 9. Product surfaces

### 9.1 Portfolio

A dense table is the default. It supports grouping/filtering by mode, health,
owner, state, target horizon, forecast freshness, and dependency. Board and timeline are
alternate views over the same records.

The portfolio declares a target exploration/exploitation allocation and shows
ledger-derived actual allocation/drift. Drift creates attention; it never moves
or stops a project automatically.

### 9.2 Project overview

The first screen shows outcome, mode/state, owner, P50/P90 forecast and freshness,
deliverable acceptance, current decision/risk, and recent material change. It
does not lead with task activity.

### 9.3 Brief

The brief is a focused editor with project references, not a general workspace
builder. Structured fields remain outside the document canvas.

### 9.4 Work and deliverables

Work uses table and board views. Deliverables have a dedicated review surface
that puts criteria, evidence, findings, and verdict together.

### 9.5 Judgment queue

The differentiating operator view collects decisions, independent reviews,
policy-valid waivers, priority conflicts, stale forecasts, pivot reviews, and privileged
interventions that cannot progress without scarce human judgment. It sorts by
impact of waiting, not activity volume, and records time-to-judgment for research.

### 9.6 Decisions and evidence

Decisions are chronological and filterable by type. Evidence is reusable across
decisions and deliverables while preserving provenance.

### 9.7 Activity and inbox

Activity is a human projection of the immutable event ledger. The inbox is an
action queue derived from assignments, mentions, review requests, risks, and
failed automation.

## 10. Product success and failure

### Success

- A new actor can correctly explain project state after reading the generated
  brief in under five minutes.
- Human and agent actors can execute all ordinary workflows without separate
  control planes.
- Forecasts become measurably calibrated without gate quality declining.
- Exploration projects end in explicit decisions rather than decaying into
  forgotten backlogs.
- Portfolio review changes or confirms priority with visible rationale.
- The dogfood study produces complete evidence from product events.

### Failure

Any of these falsifies the product direction even if the software works:

- teams keep authoritative project state in chat or external files;
- agents need privileged hidden fields to operate;
- users treat forecasts as hard deadlines and bypass gates;
- project progress remains task-count theater;
- exploration mode becomes a label without decisions;
- event history cannot explain current state;
- the product increases manual coordination more than it reduces context loss;
  or
- research claims require reconstructing missing events.

## 11. Staged ambitious scope

The full target remains ambitious, but risky non-thesis components must earn
promotion:

- `v1-core`: project/portfolio domain, judgment queue, universal decisions,
  P50/P90 forecasts, gates/evidence, actor-neutral API, realtime committed
  events, search, operated recovery, and versioned rich briefs with section
  locking.
- collaboration exploration: Yjs co-editing/presence promotes into `v1-full`
  only after convergence, permission, restart, and two-browser gates.
- automation exploration: ECA rules begin after dogfood only if the ledger shows
  repeated deterministic actions that agents perform with material absorption
  cost.

The full behavior and verification contracts remain specified even when a track
is stopped. A stop is a successful exploration outcome, not a hidden scope cut.

## 12. Working name

The working product and repository name is **Workplane**: one shared work plane
for human and agent actors. It names the no-shadow-plane thesis, is greppable,
and avoids steering the build toward a Jira clone. Domain primitives remain
plain English.
