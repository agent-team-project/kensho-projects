# Research Protocol

## 1. Research question

Does a shared project-level system improve prioritization, forecast calibration,
context recovery, and evidence quality for a human-agent software organization
without reducing gate quality or increasing coordination overhead beyond its
benefit?

The research target is not whether Workplane can store tasks. It is whether the
project model changes delivery behavior in a measurable, desirable way.

## 2. Why this is harder than Excel-lite

Excel-lite tested whether Kensho could create breadth behind stable interfaces.
Its strongest evidence was objective functional correctness across a large
function library. Workplane adds coupled dimensions:

| Dimension | Excel-lite | Workplane |
|---|---|---|
| Primary difficulty | breadth and conformance | integration and organizational behavior |
| State | single local workbook | concurrent multi-user durable organization |
| Correctness oracle | spreadsheet cases | invariants, replay, permission, convergence, outcomes |
| UI | single-user desktop/webview | realtime multi-user operational web app |
| Identity | local user | human and agent principals; optional ECA has restricted delegated context |
| History | undo/file history | immutable domain, decision, evidence, and audit ledgers |
| Collaboration | out of scope | rich brief CRDT, comments, presence, realtime state |
| Security | local/offline | tenant isolation, auth, permissions, tokens, revocation |
| Research | build outcome | product outcome plus behavioral treatment evaluation |

Workplane must not manufacture difficulty through microservices or feature count.
The listed coupled properties are the challenge.

## 3. Pre-registered hypotheses

### H0. Primary thesis: judgment absorption

Workplane increases useful accepted output per hour of scarce human judgment
without increasing escaped defects, unauthorized action, or unresolved review
load. This is the governing hypothesis; H1-H8 explain where the effect comes
from and how it could fail.

H0 is supported only if, by the end of the fourth treatment, median useful
output rate is at least 20% above matched contextual cases, median absorption
cost per accepted deliverable is no higher, and H5 gate preservation holds. It
is contradicted if useful output falls, absorption cost rises by more than 15%
without a gate-quality gain, or H5 fails. Otherwise it remains unresolved.

This support label is descriptive case evidence, not a causal estimate. An
unresolved H0 alongside supported mechanism hypotheses is a valid publishable
outcome and must not be promoted to "supported" by narrative judgment.

### H1. Priority latency

Using Workplane reduces median time from a project becoming decision-ready to a
recorded promote/hold/stop/priority decision by at least 25% relative to the
matched historical contextual cases, with no causal claim.

### H2. Idle ambiguity

Using Workplane reduces unplanned intervals where no actor is progressing and no
explicit hold/block/decision is recorded by at least 30%.

### H3. Context recovery

A fresh manager/reviewer can answer a fixed project-state questionnaire with at
least 90% accuracy in under five minutes using Workplane, and performs better
than the contextual-case handoff artifacts.

### H4. Forecast calibration

After at least ten completed/terminal projects, P50 and P90 forecasts are
directionally calibrated: approximately half of outcomes complete by their P50
and at least 80% complete by their P90. Exact binomial intervals and sample size
are reported; these thresholds are directional until the cohort reaches 30.

The first dogfood project alone cannot confirm H4; it validates instrumentation
and produces an initial case.

H4 counts terminal projects only. Deliverable/milestone forecast calibration is
reported separately as diagnostic evidence and cannot inflate the cohort.

### H5. Gate preservation

Workplane does not increase the rate of waiver, red-gate merge, self-review, or
unresolved high-severity finding. The acceptable count of red-gate merges and
unauthorized self-review remains zero.

### H6. Exploration discipline

At least 90% of exploration projects reach an explicit continue/pivot/stop/
promote decision before exceeding both declared time and token bounds by 25%.

### H7. Shared control plane

At least 95% of project-state mutations during treatment occur through Workplane
commands. Any authoritative mutation available only through agent-private state
is a protocol failure.

After three treatment projects, manual source-of-truth interventions per active
project decline rather than merely moving into a new operator workflow.

### H8. Coordination cost

The time spent maintaining Workplane does not exceed 15% of project active time,
and operators report lower or equal coordination burden than contextual cases.

## 4. Null and negative outcomes

The product thesis is weakened or falsified if:

- decision or context-recovery metrics do not improve;
- forecast fields are routinely stale or ceremonial;
- users keep authoritative state in chats/files;
- deadline pressure correlates with weaker gates;
- exploration labels do not change decision behavior;
- event/permission gaps make metrics incomplete;
- maintenance overhead exceeds measured benefit; or
- actors game progress/forecast inputs to improve dashboards.

The evaluation report must state negative outcomes without reframing them as
success.

## 5. Study design

### 5.1 Phase A: contextual case freeze

Before treatment:

1. choose comparable completed Kensho projects;
2. freeze inclusion/exclusion criteria;
3. extract events from daemon/job/PR/manager artifacts using a versioned script;
4. document missing fields and confidence limits; and
5. publish contextual-case dataset digest before examining treatment results.

Candidate contextual cases:

- GPT-5.6 model-policy rollout: integration-heavy, many review bounces, explicit
  operator directive and reconciliation.
- Excel-lite final UI remediation: bounded external project with browser/native
  evidence and independent review.
- One v0.5 prerequisite project matched by approximate scope.

These historical cases are contextual evidence, not a controlled baseline.
Their incomplete instrumentation prevents causal claims. They are not pooled
blindly; results are reported per case and in aggregate only for genuinely
comparable metrics.

### 5.2 Phase B: shadow run

For one active project, populate Workplane from the existing source of truth but
do not let Workplane drive decisions. Measure ingestion gaps, state divergence,
operator maintenance, and whether the model can represent real work.

Shadow data cannot count as treatment. A shadow mismatch blocks promotion to
treatment until resolved.

### 5.3 Phase C: treatment dogfood

The first treatment must be consequential, bounded, and started in Workplane from
inception.

Preferred candidate: Kensho v0.5 naming and ontology wave, only if Workplane is
dogfood-ready before that wave begins. Workplane does not hold the v0.5 release
or naming wave. If the wave has begun, do not backfill it and call it treatment;
the first consequential post-v0.5 Kensho epic is registered before its first
implementation dispatch and becomes the treatment.

Treatment requirements:

- project created before implementation dispatch;
- owner and participants use Workplane as project source of truth;
- exploration/exploitation decision recorded in Workplane;
- forecasts/reforecasts recorded contemporaneously;
- deliverables/gates/evidence/reviews linked as they occur;
- chat may coordinate but cannot be authoritative project state;
- all manual interventions recorded; and
- terminal outcome and retrospective completed.

The second treatment is one Workplane implementation project with M0-M6 modeled
as milestone deliverables and deliverable-scoped forecasts. A control plane that
cannot govern its own delivery has failed the adoption thesis regardless of
external dogfood results.

### 5.4 Phase D: replication

Run at least three further projects across different shapes:

- one exploratory research project;
- one integration-heavy exploitation project;
- one high-parallelism exploitation project.

H4 calibration requires at least ten terminal projects and is a longitudinal
follow-up, not a v1 release blocker.

## 6. Unit of analysis

- project for outcome, forecast, and coordination metrics;
- deliverable for review/gate metrics;
- decision for decision latency;
- work item only for flow/idle metrics;
- actor session for context-recovery exercises; and
- event for protocol completeness.

Work-item count and token consumption are descriptive, not success metrics.

## 7. Metric definitions

### 7.1 Decision-ready time

First timestamp at which all declared decision criteria are satisfied or an
authorized actor marks the decision ready.

`decision_latency = decision_recorded_at - decision_ready_at`

Readiness overrides require rationale and are analyzed separately.

### 7.2 Unplanned idle interval

An interval longer than the project-specific threshold where:

- no active work/experiment changes;
- no review is running;
- no blocking dependency/gate is recorded;
- project is not held; and
- no pending decision is explicitly assigned.

The default threshold is 30 minutes for autonomous overnight work and one
working day for human-paced work. Sensitivity analysis reports both.

### 7.3 Context recovery

A blinded evaluator asks:

1. What outcome/mode/state is current?
2. What changed most recently and why?
3. What blocks progress?
4. What are the P50/P90 forecast, freshness, and deadline status?
5. Which deliverables/gates remain?
6. What decision is next?
7. What evidence supports the current approach?

Score exact rubric answers, time to final answer, and number of external
artifacts opened.

### 7.4 Forecast accuracy

- P50 hit: actual completion on or before the contemporaneous P50;
- P90 hit: actual completion on or before the contemporaneous P90;
- absolute signed error to P50 and P90;
- P90-P50 spread;
- cohort calibration with exact binomial intervals;
- reforecast count, timing, staleness, and information gain;
- first-forecast and last-forecast error; and
- hold-adjusted active duration.

Scope changes are reported, not used to erase prior error.

### 7.5 Gate quality

- review bounce rate;
- findings by severity and acceptance criterion;
- escaped defect count;
- waiver count/reason;
- self-review attempt count;
- red-gate merge count;
- post-acceptance rollback; and
- evidence completeness.

More bounces can mean stronger review, so bounce rate is interpreted with escaped
defects and finding quality rather than minimized alone.

### 7.6 Coordination cost

- project-state update time;
- duplicate entry count;
- manual reconciliation interventions;
- source-of-truth divergence duration;
- notifications handled;
- operator active time; and
- subjective burden after each project using a fixed five-item scale.

### 7.7 Absorption cost, design yield, and integration debt

- `absorption_cost`: human minutes spent reviewing, reconciling, deciding, and
  recovering context per accepted deliverable;
- `design_yield`: accepted requirements or falsified assumptions per completed
  exploration unit, not document volume;
- `integration_debt`: open contract conflicts, red builds, unreconciled branches,
  projection divergence, and review queue age at each daily boundary; and
- `useful_output_rate`: accepted deliverable value points divided by human
  judgment hours, reported with raw counts so the weighting remains inspectable.

Value points are fixed by a blinded reviewer before outcome data is opened and
are used only for within-study sensitivity analysis.

## 8. Instrumentation contract

Research events are derived from domain events where possible. Additional
analytics events are allowed only for UI exposure/interaction and use a separate
schema.

Every metric definition lists:

- source event types/fields;
- inclusion/exclusion criteria;
- treatment phase;
- missing-data behavior;
- aggregation formula;
- version; and
- validation test.

The analysis repository/script is deterministic. A manifest records code commit,
dataset digest, query version, environment, and generated outputs.

No document body, comment body, token, or unnecessary personal content enters
analytics. Actor ids are pseudonymized in published datasets.

## 9. Anti-gaming controls

- Progress uses accepted deliverables, not task closures.
- Forecast history cannot be edited or deleted.
- Actual completion is domain-derived.
- Holds require reason and have explicit intervals.
- Scope change is a decision linked to affected forecast/deliverables.
- Confidence is numeric and evaluated later.
- Project splitting/merging requires a decision and linked lineage.
- Cancelled/stopped projects remain in denominator where the metric requires.
- Manual overrides are visible and analyzed separately.
- Research definitions freeze before treatment results are opened.

## 10. Analysis plan

For each hypothesis:

1. report raw case timelines;
2. report metric distributions and sample size;
3. compare contextual cases and treatment with uncertainty;
4. identify missing/incomparable data;
5. perform stated sensitivity analyses;
6. classify supported, contradicted, or unresolved; and
7. state the product/ontology change implied.

Small samples prohibit claims of statistical generality. The first evaluation
is a structured case study with quantitative measures. Longitudinal claims wait
for replication.

## 11. Release relationship

Software release requires instrumentation completeness and a valid first
treatment or explicit reason the treatment is still running. It does not require
positive results. Missing/manipulated data blocks the research gate; a negative
result does not block release but must create a product decision.

The study stops or pivots if any of the following persists across two treatment
projects: fewer than 80% authoritative mutations use Workplane, absorption cost
increases by more than 25% without a corresponding gate-quality gain, design
yield does not improve, integration debt grows for five consecutive days, or a
permission/gate-integrity failure occurs. Security or gate-integrity failure
stops treatment immediately. A stopped study is a valid result, not a release
failure to conceal.

## 12. Deliverables

- frozen protocol and metric catalog;
- contextual-case extraction scripts and manifest;
- shadow-run discrepancy report;
- treatment project export and digest;
- reproducible analysis notebook/script;
- charts/tables with accessible underlying data;
- qualitative intervention log;
- final evaluation report; and
- product decisions responding to negative/unresolved findings.
