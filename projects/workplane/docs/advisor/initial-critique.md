# Consult 7 — "Projects" (agent-native project & portfolio product): pre-implementation critique

**Date:** 2026-07-10 · **Asked by:** Kan for James (msg f307ef9d, reply-to manager) · **Advisor:** advisor (persistent seat)
**Reviewed:** `~/projects/kensho-projects/projects/` as of 2026-07-10 14:13 — `SPEC.md` v0.1, `README.md`, `PROJECT.toml`. The mapped `docs/*` and `backlog/` do not exist yet; this critique lands before they are authored, which is the right moment.
**Calibration baseline:** `excel-lite/SPEC.md` v1.0 (148-function fan-out, LibreOffice oracle, local single-user app) and `documentation/operating-model.md` (SQU-42 field constraints).

---

## 0. Verdict and executive summary

**This is the right study, pointed the right way. The spec's direction is correct; its problem is load.** The principles (one domain / two actor kinds; deadlines inform, gates govern; history is evidence; legible priority) are the correct thesis, and they independently match positions I already hold (P8, P9, P12). The non-goals list is genuinely good. The acceptance bar is objective and evidence-shaped.

The failure mode is that v1 as specified is **three products** — a coordination system (thesis-bearing), a collaborative document editor (Yjs + presence + convergence matrix), and an automation platform (ECA rules engine) — all held to a mature-product acceptance bar (§9: 1M-event perf fixtures, zero-serious a11y, security fuzzing, 15 E2E workflows) on a T+28d RC forecast, executed by a fleet whose review pool dead-letters at one stream today (#334) and whose proven capacity unit is build slots, not agents. The thesis-bearing 60% is at risk of being crushed under the table-stakes 40%.

**Top recommendations, in priority order:**

1. **Stage the acceptance bar; don't lower it.** Split v1-core (thesis-bearing) from v1-full, and map every §9 category to the earliest gate where it starts being enforced (replay determinism from slice 1; perf fixtures from dogfood-ready; full a11y matrix at RC). Big-bang acceptance at RC produces a bounce storm.
2. **Make the Yjs collaborative editor a detachable exploration track with pre-registered kill criteria** — apply the product's own exploration semantics to its riskiest component. v1-core briefs: markdown blocks + entity references + version history + section-level optimistic locking. Co-editing/presence must be cuttable at the dogfood-ready gate without dragging v1.
3. **Defer the ECA automation engine past dogfood.** Keep event subscriptions + the in-app inbox. Agents are the automation layer; a rules engine is the least novel component in the whole spec and partially contradicts its own thesis.
4. **Sharpen the persona** to "a few humans directing many agents" (1–10 humans, 10–100 agents; James+Kensho is the reference user) and add the operator **judgment queue** as a first-class view — the absorption instrument is the differentiating surface.
5. **Ontology fixes (small, load-bearing):** one universal `decision` primitive reused everywhere; an explicit gate-waiver-by-decision event, permanently visible; portfolio = allocation (declared target exploration/exploitation mix, ledger-measured drift), not just grouping.
6. **Demote the cross-project research baseline** (GPT-5.6 rollout) to context; pre-register within-study falsifiers that don't need it: adoption, absorption cost, design-yield, calibration.
7. **Start with 2–3 parallel tracks, not 4**, until the review pool is proven at that width; contract (event schema + commands + permissions) gets a single owner and is the one serialization point.
8. **Fix the naming stutter:** slug `projects` is ungreppable and collides with the generic noun everywhere. Recommend `workplane` for repo/slug (name the thesis, not the competitor); display name can stay "Projects" until the v0.5 wave ratifies. Keep product and Kensho vocabularies in separate registers with an explicit mapping table.
9. **Un-hostage the definition of done:** §10.3 ties v1 closure to the naming wave completing; add "or a recorded checkpoint review" so an externally-stalled wave can't hold the release.
10. **Truth in status:** README says "specification complete" while §11 says "phase is exploration." Pre-slice, the spec is a hypothesis. Say "specified, pre-slice"; mark every doc with its freeze point (P5: unmarked confidence hardens into false authority).

---

## 1. Q1 — Thesis and target user; what Excel-lite could not prove

**What Excel-lite proved:** the fleet can implement a large, externally-specified surface in parallel behind a stable interface, with objective conformance (LibreOffice as oracle), gated by review capacity. Breadth, correctness, parallel mechanics.

**What it structurally could not prove — the three claims this study should make:**

1. **Design under ontological freedom.** Excel-lite's ontology was handed to it by forty years of spreadsheet convention; acceptance was conformance to an external oracle. Projects has no oracle — the fleet must *invent* a domain model and have it survive contact with real use. The oracle moves inside: acceptance becomes invariants + dogfood revealed preference. This is the single biggest epistemic step and the spec under-names it (§2 lists "establish and preserve a coherent domain model," but as one bullet among six).
2. **Integration under concurrency.** Stateful, security-sensitive, concurrent subsystems whose interfaces can't be merged by fan-out. The spec names this correctly (§2).
3. **Operation.** Excel-lite shipped an artifact and never operated anything. Projects must be *run as a live service through its own dogfood*: schema migrations on a populated ledger, backup/restore, uptime, token revocation. The spec has the pieces (§9.6) but doesn't frame operation as a first-class claim — it should, because it's the capability Kensho's own future depends on.

**Thesis critique.** PROJECT.toml: "A shared project layer can improve human-agent prioritization, forecasting, evidence quality, and learning without weakening delivery gates." Four improvements at once is diffuse — each needs its own baseline and measure, and "improve" invites motivated reading. Sharpen to one primary claim with three subsidiaries:

> **Primary:** for a mixed human-agent org, one actor-neutral, evidence-gated project layer beats the status quo (tracker + files + chat) on **human absorption cost** and **evidence quality**, at **equal or better gate integrity**.
> Subsidiaries: forecasts become calibrated (P50/P90 coverage); exploration produces recorded, reusable decisions; agents coordinate in-product rather than around it.

**Target user.** "Humans and autonomous agents" is a category, not a persona. The sharp persona: **a small human team (1–10) directing an agent fleet (10–100), where human attention is the scarce resource** — James directing Kensho is the reference instance. This resolves feature disputes mechanically: presence cursors are a human-team feature (defer); the judgment queue is a fleet-director feature (core); mobile-native is neither (excluded, ✓). Also note the demo consequence: Excel-lite was legible in ten seconds; Projects is not. Its public artifact is the **research report plus the running tool** — the dogfood study *is* the demo. Plan comms accordingly.

## 2. Q2 — The ontology

Two kinds of things, and the spec mostly has both. **Containment (each level is the unit of a different act):**

| Concept | One-line contract | Unit of |
|---|---|---|
| **Org** | Boundary of identity, authority, and trust; actors and roles live here | Permissioning |
| **Portfolio** | The org's standing allocation of attention across projects, with a declared exploration/exploitation mix | Balancing |
| **Project** | A bounded bet on an outcome: mode, brief, forecast, deliverables, decisions, ledger. Projects *end* | Promotion / pivot / stop |
| **Deliverable** | An externally observable promise with acceptance criteria and gates; the bridge from work to value | Acceptance; forecasting |
| **Work item** | A bounded unit an actor can take; advances a deliverable or shouldn't exist | Execution |

**Cross-cutting records (the memory system):**

| Concept | Contract | Distinguish from |
|---|---|---|
| **Brief** | Forward-looking, editable statement of intent; one per scope; bounded | Ledger (backward-looking, append-only). Notion's rot is mixing them |
| **Forecast** | Versioned prediction (range + confidence) about a deliverable/project; reforecasting is normal | Target (owner intent) and deadline (external constraint) — three types, not one date field |
| **Decision** | First-class judgment record: question, options, choice, rationale, decider, revisit-conditions | Gate (recurring structural check). A gate verdict may *cite* a decision |
| **Evidence** | Immutable fact attached to the ledger (test run, review verdict, link, hash) | Opinion/comment. Gates consume evidence |
| **Gate** | Named precondition on a *transition* (not a status), demanding evidence | Status decoration. Date-blind by construction |
| **Ledger** | Append-only event history; every view is a projection of it | The database-as-mutable-truth model |

**Spec gaps against this model (all small, all load-bearing):**

- **Decision must be one universal primitive.** The spec scopes decisions mainly to exploration transitions and priority rationale. Make *every* judgment the same entity: mode change, promotion, gate waiver, scope cut, priority override, sunset, criteria override. One shape, one ledger event family, one view. This is the product's most novel primitive — don't fragment it.
- **Gate waiver semantics are missing.** §3-P4 forbids dates bypassing gates; §5.4 completion requires "no blocking gate." But real orgs need a legitimate waive path or pressure will find an ugly one (delete the gate = mutation, worse). Add: `gate waived by decision <id>`, requiring a role-gated decision, **permanently visible on the artifact** ("shipped with waived gate: X"). This is P9 encoded in the product: through the gate via recorded authority, never around it.
- **Portfolio should be an allocation, not a folder.** §5.1 has grouping and transparent priority; add a declared target mix (exploration share of capacity) and ledger-measured actual mix with drift surfaced. A view, not a scheduler — no auto-enforcement.
- **Fixed, minimal state machines.** DOMAIN.md should enumerate the exact lifecycle graphs (project: proposed → active(exploring|exploiting) → held → completed|stopped; work item: open → in_progress → in_review → done|cancelled) as *fixed* — configurable workflow graphs are the Jira tar pit the non-goals already refuse.
- Endorsements: no milestone entity (deliverable + target covers it); "work item" over "task" (avoids collision with agent-runtime task tooling); comments unstructured vs decisions structured.

## 3. Q3 — Exploration vs exploitation as first-class model

§5.3/§5.4 are strong: hypotheses, bounds-as-review-triggers (not forced shutdowns — correct), promote-retaining-history, decisions as the terminal act. Four additions:

1. **Pre-registration discipline, enforced by ledger ordering.** Decision criteria (promotion thresholds, kill criteria) are recorded *before* the evidence arrives — the ledger's ordering makes this checkable. Overriding criteria is allowed but is itself a visible decision ("promoted despite criteria"). This single mechanism is the anti-gaming core of the whole exploration model.
2. **Pivot budgets.** Unlimited pivots = zombie exploration. Each pivot decision must renew or shrink the bounds; at pivot N (2–3), force a portfolio-level review. Without this, "pivot" becomes the word for never stopping.
3. **Stop is a success state.** A stopped exploration with a recorded findings report *achieved its purpose* (bought information). Portfolio views must not render stops as failures; the exploration metric is **information yield** — hypotheses resolved per unit budget, and % of outcomes decided by pre-registered criteria vs overridden.
4. **Balance is measured, not enforced** (see §2 portfolio gap). Drift between declared and actual mix is an attention signal for the operator, consistent with §3-P6's no-opaque-scores.

Note the pleasing recursion: PROJECT.toml declares *this study itself* an exploration project. Then it must carry its own kill criteria — "what would make us stop this study early?" is currently unanswered anywhere in the package. Answer it (e.g., walking slice reveals the event-sourced core is beyond the fleet's review capacity; dogfood adoption fails at the pre-registered milestone and the friction log shows thesis-level, not implementation-level, causes).

## 4. Q4 — Best-effort deadlines without gate corruption

The spec's stance (P4 "deadlines inform; gates govern," P7 "quality beats schedule," §6 "no approval by timeout") is exactly right. Make it operational:

- **Three date types, never one field:** *target* (owner intent), *forecast* (current prediction), *deadline* (external constraint, typed with a source: demo, contract, window). The product renders the gaps (target↔P50, deadline↔P90); the gaps are the attention signal.
- **Two-point forecasts (P50/P90), versioned.** Cheapest honest form; a single date + "confidence: medium" invites false precision. Every reforecast is a ledger event with reason; prior forecasts are never overwritten (§8.6 already ✓).
- **Staleness beats slippage.** A forecast older than its review interval, or invalidated by an event (dependency slipped, scope grew), is surfaced as *stale* — a worse health state than a moved date. Reforecasting must be one cheap command; the product should make honesty cheaper than silence.
- **Dates may trigger attention, never transitions.** No date-conditioned state change anywhere — not in commands, not in the automation layer if one ever ships. Deadline pressure legitimately triggers a *scope decision* (cut a deliverable, recorded, citing the deadline), never a gate waiver-by-default.
- **Calibration is the only date-related score, and it attaches to forecasts, not people-as-punctual.** Measure P50/P90 coverage per project/type/actor; never rank actors by hitting dates (sandbagging follows immediately). Add to non-goals explicitly: **no velocity metrics, no punctuality leaderboards, no actor-level schedule scoring.**
- **Forecasts are authored by owners.** Anyone else moving a forecast is a recorded decision naming the authority (same shape as everything else).
- **Missed-deadline learning is automatic** because the ledger holds every forecast version: forecast-error and reforecast-timing distributions (§9.7 lists both ✓) plus calibration curves fall out as projections.

## 5. Q5 — Which technical challenges are essential

The test I applied: does the component bear one of the three claims (design / integration-under-concurrency / operation), and does it have an objectively checkable gate the pipeline can verify?

| Capability | Verdict | Notes |
|---|---|---|
| Event-sourced ledger + deterministic replay | **INCLUDE — core** | *The* thesis-bearing structure. Replay-checksum acceptance (§9.1) is a superb deterministic gate. Version events from day 1 (upcasting), or dogfood-driven ontology changes become migration hell |
| Multi-writer optimistic concurrency, typed conflicts | **INCLUDE — core** | The 20-writer no-silent-lost-update test is the concurrency evidence |
| Actor-neutral permissioned API (tokens, idempotency, revocation) | **INCLUDE — core** | The thesis itself; see §6 |
| Realtime = live projections (WS/SSE on committed events) | **INCLUDE — core** | Hard boundary 2 (notifications, never a second truth) is exactly right |
| Operated service: migrations on live ledger, backup/restore | **INCLUDE — core** | The operation claim. §9.6 ✓ |
| Permissions | **INCLUDE — minimal** | RBAC per project + org boundary + generated allow/deny matrix (§9.3 ✓). No custom permission designer, no row-level ACL matrix |
| Search | **INCLUDE — as projection only** | Postgres FTS; hard boundary 5 ✓. Add non-goal: no external search engine |
| Dependencies | **INCLUDE primitive, REJECT solver** | Edges + cycle detection + blocked/critical-path *views*. Spec's "warnings without automatic scheduling claims" ✓ |
| Rich briefs (blocks, entity refs, versions) | **INCLUDE — structural** | The valuable part is briefs-linked-to-ledger (embeds resolve read-only; mutations via API — hard boundary 3 ✓) |
| **Yjs realtime co-editing + presence** | **DETACH — exploration track with kill criteria** | See below |
| **ECA automation engine** | **DEFER past dogfood** | Keep event subscriptions + inbox. See below |
| Offline/local-first structured DB | **REJECT** | Spec already rejects ✓ — contradicts shared-ledger thesis; CRDT-sync is a research project |
| PostgreSQL + Compose (vs Go single-binary + SQLite) | **ENDORSE Postgres** | Deliberate deviation from Kensho's single-binary habit, and epistemically necessary: SQLite's single-writer would make the 20-writer concurrency evidence vacuous. FTS and LISTEN/NOTIFY are native. Price: keep it ONE compose file, one migration tool |
| OTel, a11y, perf fixtures | **INCLUDE, staged** | Enforce from dogfood-ready / RC gates, not slice 1 |

**The Yjs case (recommendation 2, argued).** It is the one component that adds a second *process*, a second *consistency model* (CRDT beside event-sourced optimistic concurrency — the spec spends hard boundaries 3–4 legislating the seam), a second *stack*, and the hardest E2E matrix (partition/reconnect convergence) — while being **peripheral to the domain by the spec's own architecture** (the brief cannot change project state). Agents don't co-type; the persona's humans are few. The honest research value is "can the fleet integrate a gnarly third-party stateful subsystem" — a real integration claim, which is why I say *detach with kill criteria* rather than *cut*: run it as its own exploration track (hypothesis: convergence matrix green by dogfood-ready within budget X), and if killed, v1 ships with versioned briefs + section locking and loses nothing thesis-critical. What must not happen is the convergence matrix blocking v1-core.

**The automation-engine case (recommendation 3, argued).** The spec's own P1 says one command surface for all actors — and an agent subscribed to committed events, acting through ordinary commands under its own token, is *strictly more general* than an allowlisted ECA rules engine. For this study, a rules engine spends scarce review capacity on the least novel component in the package. The genuinely valuable §5.7 pieces are the **event subscription substrate** and the **inbox** (the attention surface) — keep both in v1-core. If the engine ships later, the spec's boundary 6 (automation as an ordinary actor invoking idempotent commands) is already the right design.

## 6. Q6 — One product, two actor kinds, no shadow control plane

§3-P1 and §5.5 are correct. Strengthen from principle to *enforced mechanism*:

1. **The UI is a client of the public API — literally, and audited.** Add to §9: an automated parity audit. Every UI mutation maps to a public command; every public command is UI-reachable or explicitly registered headless. Parity rot is how shadow planes are born — one "temporary" internal endpoint at a time.
2. **Provenance is uniform and delegation is visible.** Every event records actor id, actor kind, request id, origin (§5.5 ✓) — make **principal mandatory for agent actors** (the human/role authority the agent acts under). This is P8 productized: authority is part of the actor's contract, and no actor widens its own.
3. **Human-in-the-loop is authorization policy, not interface accident.** Where policy wants human judgment (gate waiver, project cancellation, token grants), encode `requires actor kind: human` + role in the permission matrix — data, not a UI-only button. Then "what can agents not do" is a queryable, testable, reviewable artifact rather than folklore.
4. **No bulk backdoors.** Agents replay the ledger and page projections like any client; no direct projection writes, no gate-skipping batch endpoints. If an operation is too tedious through the public API, that is a product defect to fix *for everyone* — the tedium signal is a research observation (it feeds §9.7's manual-intervention count).
5. **Same data plane, different attention projections.** Humans get absorption surfaces — above all the **judgment queue** ("what is blocked on my decision?"), plus drift and gate-health views. Agents get structured event streams and queries. The five opinionated views I'd ship: judgment queue, forecast-vs-target drift, gate health/bounces, portfolio mix, decision log.
6. **Name the dogfood temptation now:** when the product is awkward, the fleet will coordinate in Kensho mailboxes and files instead. That friction is *the* research signal — the adapter should log every such fallback as a manual coordination intervention (§9.7 already counts them ✓).

## 7. Q7 — Acceptance, research metrics, and what falsifies the thesis

**Acceptance:** §9 is admirably objective; my only structural complaint is timing (recommendation 1 — stage enforcement per gate, or RC becomes a bounce storm; the operating model's lesson is that gates work when they're the *same commands at every step*, which requires them to exist early).

**Research design honesty.** The named baseline (GPT-5.6 model-policy rollout, measured retrospectively from Kensho ledgers) differs from the treatment (naming wave, run in Projects) in work type, period, and fleet maturity — n=1 vs n=1 with confounds. **Demote it to context; do not present this as a controlled experiment.** The claim the study can honestly make is an instrumented case study with pre-registered falsifiers. The strong falsifiers are within-study:

- **Adoption (the falsifier with teeth):** by the pre-registered milestone, the fleet manages real work in Projects — naming wave from its kickoff, and Projects' own remaining backlog self-hosted at dogfood-ready — with manual coordination interventions *declining week over week*. If the fleet reverts to mailboxes and files, the thesis as built is falsified. Then use the friction log to distinguish "product is bad" from "thesis is wrong" — a decision, recorded.
- **Absorption cost:** operator minutes/day to answer "what needs my judgment?" via the product vs. today's status scraping. The product must reduce it; if supervising *through* the product costs more, the primary claim fails (P2 made measurable).
- **Design yield (the fleet-can-design claim):** count ontology-reversal decisions after dogfood begins (schema migrations that change meaning, not add). Target is not zero — zero means dogfood taught nothing — target is *convergence* (declining rate). A sustained high reversal rate falsifies claim 1.
- **Calibration on the study's own forecasts:** the T+3/T+14/T+28 forecasts get P50/P90 form and are scored at the end (§9.7 ✓). The study must practice its product's own medicine.
- **Gate integrity under pressure:** zero waived-without-decision events; bounce rates comparable to Excel-lite where comparable. Rising waivers as dates approach = the product failed its own P4.
- **Integration-debt tripwires (claim 2 / Q8):** cross-track rework PRs, contract-change ripple count. If ≫ Excel-lite at comparable scope, the decomposition model doesn't generalize beyond embarrassingly-parallel domains — a finding worth publishing either way.

§9.7's closing rule — negative results don't block release; missing or manipulated evidence does — is the best sentence in the spec. Keep it verbatim.

## 8. Q8 — Decomposition for parallel Kensho units

Parallelize along coupling seams, serialize within them (P10). Here the seams are unusually clean *if* one thing is held sacred:

- **The contract is the load-bearing seam and the single serialization point:** event schema + command/API surface + permission matrix + error taxonomy. One owner. Everything else consumes *generated* artifacts (OpenAPI clients, JSON Schemas — spec ✓; make codegen the only legal way to consume the contract, so drift breaks builds, not integration). Contract changes are a serialized, highest-bar PR class.
- **Walking slice is SERIAL — one unit, no fan-out** until one command flows end-to-end: command → domain → ledger → projection → API → UI list → agent SDK call. Excel-lite's lesson generalized: parallelism after the spine exists, never to create it. (Spec §11's slice-first posture ✓.)
- **Then fan out along:** (A) core domain + ledger + migrations; (B) API/auth + parity audit; (C) web UI against contract-generated fakes; (D) agent SDK + Kensho dogfood adapter; (E) search/views projections. Detachable: (F) briefs-collab exploration; (G) automation, if ever.
- **Expected serialization points — plan them, don't discover them:** contract changes; DB migrations (single owner inside A); authz semantics (live in the contract, *not* in track B, or B becomes a chokepoint); the UI design system (first UI unit owns tokens/components, then screens parallelize); the E2E harness (one owner early).
- **Capacity: start at 2–3 tracks, not PROJECT.toml's 4.** Field-tested constraints: capacity is build slots (~3 concurrent builds on the reference laptop; Go is cheaper but Postgres+Playwright E2E is not free), review absorption is the real ceiling, and the verify/review pool dead-letters at one stream today (#334). Widen only after two tracks sustain bounded merged-PR latency. Every track carries the P11 bundle: charter, WIP cap, explicit deliverable, integration duty, evidence obligations, debt ledger. Depth 1 ✓, merge-only-on-green ✓ (PROJECT.toml).

## 9. Q9 — Scope traps and non-goals

The spec's §6 already refuses the big ones (workflow designer, plugins/custom code, SaaS multi-tenancy, offline-first, approval-by-timeout, feature-parity chasing, aliases/shims). Traps still open, each one Jira/Notion gravity:

1. **Custom fields** — not currently excluded. The schema-less field designer poisons queries, permissions, and migrations forever. Extend by typed contract evolution instead. **Add to non-goals explicitly.**
2. **Editor scope creep** — presence → cursors → in-doc comment threads → suggest mode → templates. The brief has *one job*: intent, linked to structure. The detach-with-kill-criteria posture (§5) is the fence.
3. **Report/dashboard builder** — projections are code; ship the five opinionated views; new views arrive as PRs, not as a query designer.
4. **Notification preference matrices** — inbox + one digest. Preference empires are absorption-hostile.
5. **Velocity/punctuality scoring** — story points, sprint burndowns, actor leaderboards. Directly corrupts §4's anti-gaming design. **Add to non-goals.**
6. **Import-from-Jira** — a compatibility magnet that smuggles the foreign ontology back in. Non-goal for the study.
7. **Ontology maximalism** — risk registers, OKR trees, time tracking, capacity planners. The §2 ontology is sufficient; every addition must be earned by recorded dogfood pain.
8. **External search/queue infrastructure** — Postgres does FTS and outbox; adding Elasticsearch/Redis/NATS is operational surface with no thesis value.

## 10. Q10 — Working name and first dogfood project

**Name.** "Projects" is maximally plain (P12-compliant in spirit) but fails in practice: it is ungreppable, stutters in its own path (`kensho-projects/projects/`), and collides with the generic noun in every sentence about it — a name that can't be searched or distinguished steers nothing. Names are prompts cuts both ways: `jira-lite` would steer the fleet toward cloning Jira; `projects` steers toward nothing. **Name the thesis, not the competitor: recommend `workplane`** — one work plane for two actor kinds; encodes the no-shadow-control-plane claim; boring-adjacent, zero collision, greppable. (Fallback with the same virtues: `worktable`.) Display name may stay "Projects" until the v0.5 naming wave ratifies the final choice — but change the *slug/repo/module* now, before imports and logs fossilize the stutter.

**Vocabulary discipline (this matters more than the name):** the product ontology and the Kensho runtime ontology are separate registers, meeting only at the dogfood adapter. Add the mapping table to DOMAIN.md with one rule: **same concept → same word** (gate, evidence, decision, brief are genuinely shared concepts — keep the words identical); **different concept → visibly different word** (Kensho `job` ≠ product `work item`; good that they differ). Two vocabulary efforts run concurrently (the wave renames Kensho while Projects names itself) — without the mapping table, gate/brief/decision will silently mean four things.

**First dogfood ✓ with a sequencing fix.** The v0.5 naming wave (PROJECT.toml ✓) is the right first project: real, bounded, near-term, decision-heavy rather than code-heavy — it exercises decisions, briefs, forecasts, and gates without touching the pipeline-heavy paths, and it tests the product beyond software work. Two fixes: (a) §9.7 requires the dogfood run "from inception" — so either hold the wave's kickoff until dogfood-ready (T+14), or define inception as the wave's kickoff decision, recorded in Projects. Choose now, not retroactively. (b) Pre-register the second dogfood: **Projects' own remaining backlog migrates into Projects at dogfood-ready** — self-hosting is the adoption falsifier with teeth. And fix the §10.3 hostage clause (recommendation 9): "completed or reached a recorded terminal decision *or checkpoint review*," so a wave parked on Kensho-external causes can't hold v1 hostage.

## 11. The package, challenged

The README's ten-document map is more right than wrong — but staged and slimmed:

- **Collapse:** PRODUCT.md to a thin persona/jobs note or fold into SPEC (§3–§6 already own scope; duplication = drift). UX.md to the five views + three golden paths (human triage, agent work execution, promotion decision) — the UI is a client; workflows fall out of the ontology.
- **Schema-first, not prose-first:** API.md's source of truth is the OpenAPI + JSON Schema files; prose is commentary. SECURITY.md's core is the role/action/resource matrix *as data* with generated tests (§9.3 already implies this — make the matrix file the spec).
- **Highest bar:** RESEARCH.md (the epistemic payload — pre-registered falsifiers, frozen definitions, the §7 metric set) and DELIVERY.md (tracks mapped to §8 seams; §9 categories mapped to gates; capacity 2–3).
- **Backlog: seed only to contract-freeze + slice-1 depth.** §11 admits forecasts are low-confidence before the slice; a deep pre-slice backlog is waterfall debt at declared-low confidence — the slice re-prices everything. Excel-lite could seed 148 issues up front *because* the units were disjoint and oracle-checked; Projects cannot.
- **Add:** a freeze-point header on every doc (frozen pre-slice vs re-priced post-slice); the study's own kill criteria (§3 above); the Kensho↔product vocabulary mapping; `docs/advisor/` ✓ already mapped — this document can be lifted there as-is by the package owner.

## 12. Ranked risks, with stop conditions

1. **Yjs track consumes the build** → detach + kill criteria; if the convergence matrix isn't green by dogfood-ready, v1 ships without co-editing (§5).
2. **Acceptance-debt bounce storm at RC** → stage the bar per gate (§0.1).
3. **Review-pool saturation at 4 tracks** → start 2–3; widen only on bounded merged-PR latency (§8).
4. **Baseline overclaim discredits the research** → demote to context; within-study falsifiers (§7).
5. **Ontology churn outruns migration capacity** → event versioning/upcasting from day 1; design-yield metric watches convergence (§7).
6. **Dogfood adoption fails** → that is a *result*, not an embarrassment — publish it, with the friction log arbitrating product-bad vs thesis-wrong (§7).
7. **DoD hostage to the naming wave** → checkpoint-review clause (§10).
8. **Two-vocabulary confusion mid-wave** → mapping table, same-concept-same-name rule (§10).

**Bottom line:** approve the direction; stage the load. The spec's own best ideas — exploration contracts, pre-registered criteria, evidence-gated transitions — should be applied reflexively to the two components most likely to sink it, and to the study itself. Done that way, this is a genuinely harder and more epistemically valuable successor to Excel-lite: it tests whether the fleet can *design*, *integrate under concurrency*, and *operate* — and it leaves behind the coordination tool Kensho itself needs next.

---
*Advisor positions applied: P1 (judgment artifacts), P2 (absorption constraint), P4 (durable artifact discipline), P5 (mark decision conditions), P7/P8 (ownership and authority contracts), P9 (through the gate, never around), P10 (coupling seams), P11 (bounded sub-units), P12 (names are prompts). New positions distilled from this consult: P13 (one plane, two actor kinds), P14 (dates trigger attention, never transitions), P15 (stage the bar, detach the risk).*
