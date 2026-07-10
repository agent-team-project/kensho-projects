# Seeded Backlog

This backlog seeds issue creation after specification approval. It is not an
authorization to dispatch. Every issue must satisfy `docs/DELIVERY.md`'s
definition of ready and link to a project deliverable.

The 120 entries are a full scope inventory. Before M1 exits, only M0 and the
walking-slice subset may be marked ready; every other entry is provisional and
must be repriced from slice evidence. Epic F's CRDT entries and Epic H's ECA
entries are exploration candidates and do not block `v1-core`.

IDs are stable specification ids. Implementation tickets may use a different
tracker prefix but must retain the seed id in their contract.

## Epic A - Contracts and repository foundation

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-001 | Initialize Go/React monorepo | - | Clean checkout runs formatting, unit, schema, and generated-file checks with one command. |
| PRJ-002 | Establish CI smoke/acceptance/release tiers | PRJ-001 | Each tier is machine-readable, documented, and exercised by a self-test that catches a missing gate. |
| PRJ-003 | Define requirement and acceptance-case registries | PRJ-001 | Duplicate/unknown requirement or case ids fail CI; one sample case executes. |
| PRJ-004 | Author OpenAPI skeleton and generation | PRJ-001 | Go server stubs and TypeScript client regenerate with zero diff and examples validate. |
| PRJ-005 | Author domain-event envelope schemas | PRJ-001 | JSON Schemas validate good/bad fixtures and enforce version/actor/causation fields. |
| PRJ-006 | Add deterministic clock/id/test fixture packages | PRJ-001 | Unit tests reproduce byte-identical aggregate/events across repeated runs. |
| PRJ-007 | Create PostgreSQL migration framework | PRJ-001 | Empty DB migrates, concurrent migrator is serialized, failed migration leaves known state. |
| PRJ-008 | Create reference organization fixture | PRJ-003, PRJ-007 | Fixture includes all actor/project modes/roles/states and has stable manifest digest. |
| PRJ-009 | Add evidence-manifest generator | PRJ-002 | A sample gate emits source/environment/command/result/artifact digests and rejects dirty source. |
| PRJ-010 | Record foundational ADRs | PRJ-001 | Stack, event model, authorization, optional collaboration, and clean-migration ADRs are approved before M1. |

## Epic B - Identity, tenancy, and authorization

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-011 | Organization and membership schema | PRJ-007 | Organization-scoped uniqueness/foreign keys prevent cross-org relation creation. |
| PRJ-012 | Human account and Argon2id authentication | PRJ-011 | Login/session tests cover known/unknown parity, hash parameters, rotation, and revoke. |
| PRJ-013 | Invite-only membership workflow | PRJ-012 | Single-use expiry, replay, wrong-org, disabled inviter, and accepted invite cases pass. |
| PRJ-014 | Session lifecycle and CSRF | PRJ-012 | Fixation/logout-all/password-reset/origin/CSRF corpus passes every browser mutation. |
| PRJ-015 | Agent service identities | PRJ-011 | Create/disable/list preserves attribution and cannot grant the creator more authority. |
| PRJ-016 | Scoped opaque service tokens | PRJ-015 | Display-once/hash/expiry/project restriction/revoke/rate tests pass. |
| PRJ-017 | Organization role policy | PRJ-011 | Generated matrix covers owner/admin/member/observer for every org action. |
| PRJ-018 | Project role and visibility policy | PRJ-017 | Private/org-visible policies pass API/query/search/realtime expected-deny corpus. |
| PRJ-019 | Separation-of-duty policy | PRJ-018 | Self-review, hard waiver, self-widening, and automation privilege attempts deny. |
| PRJ-020 | Permission matrix generator and coverage gate | PRJ-017, PRJ-018, PRJ-019 | CI fails when any action/resource/actor/state combination is unclassified. |

## Epic C - Domain command and event core

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-021 | Project aggregate and lifecycle | PRJ-005, PRJ-007 | Full mode/state transition table and invalid-transition atomicity pass. |
| PRJ-022 | Portfolio aggregate and entries | PRJ-021 | Primary portfolio/rank and target-versus-actual allocation invariants pass with complete atomic reorder. |
| PRJ-023 | Hypothesis aggregate | PRJ-021 | Falsifier requirement, supersession, immutable history, and status transitions pass. |
| PRJ-024 | Experiment/observation aggregate | PRJ-023 | Bounds create attention but never auto-stop; observations/evidence remain immutable. |
| PRJ-025 | Deliverable and criteria aggregate | PRJ-021 | Required/weight/state/criteria invariants and accepted-edit protection pass. |
| PRJ-026 | Gate, verdict, finding, and evidence aggregates | PRJ-025, PRJ-019 | Bounce/resubmit/approve/waiver/independence matrix passes. |
| PRJ-027 | Work item aggregate | PRJ-021 | State/assignment/parent/deliverable/experiment links enforce domain rules. |
| PRJ-028 | Typed dependency graph | PRJ-027 | Generated DAG operations reject every blocking cycle including cross-project. |
| PRJ-029 | Forecast, target, and deadline aggregates | PRJ-021 | Project/deliverable P50/P90 scope uniqueness, immutable reforecast, staleness, typed deadline, reasons, holds, and target-attention cases pass. |
| PRJ-030 | Decision aggregate | PRJ-021 | Alternatives/evidence/consequence/supersession and kind-specific requirements pass. |
| PRJ-031 | Project promotion command | PRJ-023, PRJ-024, PRJ-025, PRJ-030 | Promotion atomically preserves exploration and creates valid exploitation contract/events. |
| PRJ-032 | Project completion/stop/cancel commands | PRJ-026, PRJ-029, PRJ-030 | Terminal commands enforce every SPEC prerequisite and record actual outcome/risk. |

## Epic D - Persistence, idempotency, replay, and projections

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-033 | Aggregate repositories and unit of work | PRJ-007, PRJ-021 | Real DB command commit/rollback/version tests pass without leaking SQL into domain. |
| PRJ-034 | Append-only domain event store | PRJ-005, PRJ-033 | Contiguous version/command group/actor fields enforced; app role cannot update/delete. |
| PRJ-035 | Idempotency store and middleware | PRJ-033 | Same key/body returns original result; changed body conflicts; crash boundaries pass. |
| PRJ-036 | Transactional outbox | PRJ-034 | Commit coupling and lease/retry tests prove no missing record under injected faults. |
| PRJ-037 | Current project/portfolio projections | PRJ-033 | Canonical query fixture matches domain state at every command version. |
| PRJ-038 | Activity projection | PRJ-034 | Human-readable activity covers all event families without leaking secrets. |
| PRJ-039 | Full event replay engine | PRJ-034, PRJ-037, PRJ-038 | Empty rebuild equals live canonical checksum; unknown schema stops at exact event. |
| PRJ-040 | Projection consumer checkpoints | PRJ-036 | Duplicate/reordered eligible delivery remains idempotent and resumes after crash. |
| PRJ-041 | Fault-injection framework | PRJ-033, PRJ-036 | Every named transaction/outbox boundary can be deterministically killed in tests. |
| PRJ-042 | Event/projection integrity doctor | PRJ-039 | Doctor detects seeded gap, duplicate, checksum drift, and stale consumer cursor. |

## Epic E - HTTP, agent API, and realtime

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-043 | Common HTTP envelopes, errors, request ids | PRJ-004 | OpenAPI examples execute and problem codes remain stable across typed errors. |
| PRJ-044 | Project command/query endpoints | PRJ-032, PRJ-035, PRJ-043 | All lifecycle/create/update examples pass human and agent parity tests. |
| PRJ-045 | Exploration endpoints | PRJ-024, PRJ-031, PRJ-043 | Hypothesis/experiment/decision/promotion workflows pass public API only. |
| PRJ-046 | Deliverable/review/evidence endpoints | PRJ-026, PRJ-043 | Submit-bounce-resubmit-approve workflow passes with immutable evidence. |
| PRJ-047 | Work/dependency batch endpoints | PRJ-027, PRJ-028, PRJ-043 | Board batch is all-or-nothing and returns exact conflict/cycle information. |
| PRJ-048 | Forecast/priority endpoints | PRJ-022, PRJ-029, PRJ-043 | Reforecast and reorder preserve history and require rationales/versions. |
| PRJ-049 | Cursor pagination/filter library | PRJ-043 | Stable snapshot has no duplicate/missing row; actor/org/filter-bound expiry passes. |
| PRJ-050 | Organization WebSocket event stream | PRJ-036, PRJ-018 | Commit/order/resume/filter/backpressure/revoke/isolation suite passes. |
| PRJ-051 | Agent SSE event stream | PRJ-036, PRJ-016 | Last-Event-ID resume/snapshot-required/scope/revoke suite passes. |
| PRJ-052 | Generated agent/client examples | PRJ-044, PRJ-050, PRJ-051 | A standalone example agent executes create/update/review subscription without private APIs. |

## Epic F - Structured briefs and collaboration exploration

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-053 | Brief schema and canonical JSON | PRJ-001, PRJ-010 | Allowed blocks round-trip; unknown/unsafe nodes fail or render safe placeholder. |
| PRJ-054 | Versioned section persistence | PRJ-053, PRJ-007 | Section revisions, non-overlap commits, stale conflict, and immutable restore cases pass. |
| PRJ-055 | Brief command/query API and authorization | PRJ-016, PRJ-018, PRJ-054 | Human/agent parity and wrong actor/org/project/revision expected-deny cases pass. |
| PRJ-056 | Revision history and restore | PRJ-054, PRJ-055 | Restore creates an attributable new revision and preserves every prior source revision. |
| PRJ-057 | Inline comments and mentions | PRJ-053, PRJ-055 | Section comments resolve/reopen; mentions require explicit authorized actor id. |
| PRJ-058 | Structured entity references | PRJ-044, PRJ-053 | References render current authorized label and never execute commands/leak hidden data. |
| PRJ-059 | Agent structured brief apply | PRJ-052, PRJ-055 | Node-id operations obey section revision conflicts and the same permission contract as UI edits. |
| PRJ-060 | Collaboration exploration gate | PRJ-054, dogfood | Concurrent-edit frequency, conflict cost, operating burden, and kill criteria produce promote/stop decision. |
| PRJ-061 | Optional Yjs process and persistence | PRJ-060=promote, PRJ-007 | Grants, sync, ordered updates, snapshot/restart, duplicate/reorder, and corruption cases pass. |
| PRJ-062 | Optional CRDT convergence/presence matrix | PRJ-061, PRJ-057, PRJ-059 | All seeded two/three-client partition trials converge and presence leaks no authority. |
| PRJ-063 | Rich-content sanitization and CSP | PRJ-053, PRJ-057 | XSS/link corpus is inert in editor, comment, activity, search, and export. |
| PRJ-064 | Brief export and plain-text extraction | PRJ-056 | Markdown/JSON/plain export is deterministic, sanitized, and version/digest labeled. |

## Epic G - Web application and workflows

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-065 | App shell, routing, org switcher, command palette | PRJ-004, PRJ-012 | Keyboard navigation and loading/deny/error shell pass desktop/narrow browser tests. |
| PRJ-066 | Portfolio virtualized table | PRJ-022, PRJ-049, PRJ-065 | 10k-project fixture scroll/filter/sort/select meets layout/performance/accessibility gates. |
| PRJ-067 | Portfolio board and saved views | PRJ-066 | Table/board share filters/selection; drag has keyboard/menu atomic alternative. |
| PRJ-068 | Portfolio timeline and forecast bands | PRJ-048, PRJ-065 | Intent/forecast/hold/actual render with accessible table and no single-date ambiguity. |
| PRJ-069 | Priority comparison/reorder | PRJ-048, PRJ-066 | Rationale/input/formula/override shown; concurrent reorder conflicts without partial rank. |
| PRJ-070 | Project creation flow | PRJ-044, PRJ-045, PRJ-065 | Proposed/activate and exploration/exploitation validation workflows pass at all viewports. |
| PRJ-071 | Project overview | PRJ-037, PRJ-065 | Outcome/forecast/deliverables/readiness/blockers/material-change fit first desktop viewport. |
| PRJ-072 | Exploration evidence map | PRJ-045, PRJ-065 | Hypothesis-experiment-evidence-criteria graph is keyboard/readable and shows no percent. |
| PRJ-073 | Exploration decision checkpoint | PRJ-045, PRJ-070 | Continue/pivot/stop/promote show exact required fields and committed history. |
| PRJ-074 | Deliverables and acceptance UI | PRJ-046, PRJ-065 | Criteria/evidence/gates/findings/verdict remain visible in one review workflow. |
| PRJ-075 | Work table and board | PRJ-047, PRJ-065 | Table/board batch/assignment/dependency/conflict cases pass keyboard/pointer. |
| PRJ-076 | Project timeline and dependency view | PRJ-047, PRJ-048, PRJ-065 | Work/dependency/forecast distinctions remain legible and have accessible data view. |
| PRJ-077 | Forecast/reforecast UI | PRJ-048, PRJ-065 | Side-by-side prior/new P50/P90/basis/reason/impact, freshness, target, deadline, and history pass. |
| PRJ-078 | Decisions/evidence/activity UI | PRJ-038, PRJ-046, PRJ-065 | Immutable/superseded provenance and filters render without edit-in-place. |
| PRJ-079 | Brief editor integration | PRJ-055, PRJ-057, PRJ-058, PRJ-065 | Two-browser section conflict/resolve/comment/restore workflow passes; CRDT behavior is conditional on PRJ-060 promotion. |
| PRJ-080 | Judgment and review queue UI | PRJ-050, PRJ-065 | Decision/review/risk/forecast items explain why human judgment is needed, deep-link, and return to prior context. |
| PRJ-081 | Responsive narrow workflows | PRJ-070..PRJ-080 | 390x844 create/read/comment/review/reforecast has no clipped or unreachable control. |
| PRJ-082 | Accessibility and keyboard completion | PRJ-065..PRJ-081 | Required workflows keyboard-only; zero critical/serious automated violations. |

## Epic H - Search, judgment queue, analytics, and automation exploration

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-083 | Search projection/indexer | PRJ-036, PRJ-064 | All entity/brief types index with source version and rebuild checksum. |
| PRJ-084 | Permission-aware search query/snippets | PRJ-018, PRJ-083 | Cross-org/private result/count/facet/snippet corpus has zero leak. |
| PRJ-085 | Global search UI | PRJ-065, PRJ-084 | Keyboard search/filter/result navigation covers every indexed entity and stale state. |
| PRJ-086 | Judgment queue projection | PRJ-036, PRJ-038 | Decision/review/attention/failure events create one idempotent item with reason and required authority. |
| PRJ-087 | Judgment queue API and acknowledgement | PRJ-086 | Cursor/claim/resolve/permission/retention tests preserve verdict and decision distinction. |
| PRJ-088 | Post-dogfood automation exploration gate | PRJ-086, first-dogfood | Repeated patterns, current agent-on-events cost, security boundary, and kill criteria produce promote/stop decision. |
| PRJ-089 | Optional declarative automation schema/compiler | PRJ-088=promote, PRJ-005, PRJ-019 | Unknown field/operator/privileged action fails compile with location. |
| PRJ-090 | Optional automation dry-run and executor | PRJ-089, PRJ-039, PRJ-036 | Dry run is mutation-free; event+rule version causes one effect under retry and bounded causation depth. |
| PRJ-091 | Optional automation management UI | PRJ-090 | Create/dry-run/enable/disable/failure/retry flows show exact events/commands. |
| PRJ-092 | Fixed portfolio/project analytics | PRJ-034, PRJ-029 | Forecast/flow/gate/decision metrics match hand-calculated fixtures. |
| PRJ-093 | Context brief generator | PRJ-037, PRJ-038, PRJ-086 | Generated brief answers fixed state rubric from authorized current projections. |
| PRJ-094 | Material-change classifier | PRJ-005, PRJ-038 | Versioned rules identify forecast/scope/gate/decision/blocker changes and ignore noise. |

## Epic I - Operations, security, scale, and recovery

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-095 | Docker Compose production profile | PRJ-007, PRJ-012 | Fresh Docker-only host reaches ready with one command and no external runtime request; promoted processes are profile-gated. |
| PRJ-096 | Health/readiness/graceful shutdown | PRJ-095 | Dependency/readiness/drain/SIGTERM/SIGKILL behavior passes documented probes. |
| PRJ-097 | OpenTelemetry traces/metrics/log correlation | PRJ-036, PRJ-050, PRJ-095 | Required flows correlate request/event/consumer without secret/high-cardinality data. |
| PRJ-098 | Backup archive and manifest | PRJ-039, PRJ-056 | Concurrent populated backup has schema/count/digest manifest and no plaintext token. |
| PRJ-099 | Staged restore and validation | PRJ-098 | Empty restore matches event/projection/document/permission checksums; malicious corpus denies. |
| PRJ-100 | Deterministic scale fixture generator | PRJ-008, PRJ-039, PRJ-056 | Required million-event fixture has stable manifest and generates within documented bound. |
| PRJ-101 | Performance harness and budgets | PRJ-100, PRJ-066, PRJ-083 | p50/p95/p99/raw/query plans and absolute/regression gates run reproducibly. |
| PRJ-102 | Concurrency and soak harness | PRJ-050, PRJ-100 | 100 sessions/20 writers/restart soak has zero invariant/data leak; add CRDT partitions if PRJ-060 promotes. |
| PRJ-103 | Security dynamic corpus | PRJ-020, PRJ-063, PRJ-084, PRJ-095 | Auth/CSRF/XSS/injection/revoke/rate/cross-org/canary-secret suites pass. |
| PRJ-104 | SBOM, provenance, license, vulnerability gates | PRJ-001, PRJ-095 | Release artifacts have verifiable SBOM/provenance and no untriaged high/critical issue. |
| PRJ-105 | Recovery/fault release suite | PRJ-041, PRJ-096, PRJ-099 | Randomized process/DB/telemetry failures recover with complete checksum evidence. |

## Epic J - Research, dogfood, and release

| ID | Title | Depends | Hard acceptance |
|---|---|---|---|
| PRJ-106 | Freeze metric catalog and protocol digest | spec | Definitions/queries/fixtures signed before treatment data access. |
| PRJ-107 | Build contextual-case extraction tooling | PRJ-106 | Historical case artifacts produce manifest, missing-data report, hand-checked metrics, and an explicit no-causal-claim label. |
| PRJ-108 | Run shadow project | PRJ-093, PRJ-106, dogfood-ready | State divergence/maintenance/source-of-truth gaps recorded; no treatment claim. |
| PRJ-109 | Select and register treatment project | PRJ-108 | Consequential project starts in Workplane before implementation; inclusion rationale frozen. |
| PRJ-110 | Execute exploration treatment phase | PRJ-109 | Hypotheses/experiments/bounds/evidence/decision recorded contemporaneously. |
| PRJ-111 | Execute exploitation treatment phase | PRJ-110 | Deliverables/work/gates/forecasts/reviews/completion recorded contemporaneously. |
| PRJ-112 | Context-recovery blinded exercise | PRJ-093, PRJ-111 | Fixed rubric/time/external-artifact count scored by independent evaluator. |
| PRJ-113 | Reproducible research analysis | PRJ-107, PRJ-111 | Manifest rerun reproduces tables; hypotheses classified without post-hoc definition edits. |
| PRJ-114 | Independent product review | all product epics | Reviewer reproduces required workflows and issues PRODUCT approve/bounce ledger. |
| PRJ-115 | Independent security review | PRJ-103, PRJ-104, PRJ-105 | Reviewer reproduces high-risk cases and issues SECURITY approve/bounce ledger. |
| PRJ-116 | Advisor ontology/outcome review | PRJ-113, PRJ-114 | Advisor judges whether product behavior supports or distorts the stated ontology. |
| PRJ-117 | Release evidence manifest | all gates | One immutable commit has complete smoke/acceptance/release artifacts/digests. |
| PRJ-118 | Self-hosted release artifact | PRJ-095, PRJ-104, PRJ-117 | Clean offline install/backup/restore independently reproduced. |
| PRJ-119 | Evaluation and release report | PRJ-113..PRJ-118 | Public report includes negative/unresolved findings, limits, commands, and artifact links. |
| PRJ-120 | Post-v1 project decision | PRJ-119 | Continue/pivot/stop decision records evidence, residual risks, and next research question. |

## Dependency waves and readiness

```text
Ready set 0: contract review only (PRJ-001..010, PRJ-106)
Ready set 1: one serial walking slice selected from 011..055
Provisional wave 2: durable core + web structured briefs (remaining 011..087)
Provisional wave 3: operations/scale/security + shadow study (092..108)
Conditional exploration: CRDT (060..062), ECA (088..091)
Provisional wave 4: treatment, independent review, release (109..120)
```

Within a wave, dispatch only issues whose dependencies and contract owners are
clear. WIP is controlled by reviewer/integration absorption, not issue count.
