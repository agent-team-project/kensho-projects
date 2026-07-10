# Consult 8 — Workplane pre-v0.5 package: final adversarial review

**Date:** 2026-07-10 · **Requested by:** Kan for James (msg 491f37f9, reply-to manager) · **Reviewer:** advisor (persistent seat), max effort
**Reviewed:** the complete package at `~/projects/kensho-projects/workplane/` — `SPEC.md` v0.1, `README.md`, `PROJECT.toml`, `docs/{PRODUCT,DOMAIN,ARCHITECTURE,UX,API,SECURITY,VERIFICATION,RESEARCH,DELIVERY}.md`, `contracts/{lifecycles,permissions,parity,acceptance}.yaml`, `backlog/README.md`, `docs/advisor/{disposition,initial-critique}.md` (initial-critique verified as a verbatim copy of Consult 7). All 19 files read in full (~5,200 lines).
**Scope of judgment:** material issues only — contradictions between core/full staging, undefined or unverifiable behavior, weak falsifiers, contract/backlog defects, hidden release dependencies, naming/ontology mistakes, feature-count ambition, and sufficiency for autonomous implementation.

---

## Verdict

# APPROVE WITH REQUIRED FIXES

The package is the strongest specification this org has produced — the thesis is sharp and falsifiable, the staging discipline (v1-core release-bearing; CRDT and ECA as promoted explorations with kill criteria and a no-dormant-surface rule) is internally consistent across SPEC/API/ARCHITECTURE/VERIFICATION/acceptance.yaml, the research protocol quantifies every hypothesis (H0–H8) with pre-registration and stop criteria, and the delivery plan correctly encodes "unlimited tokens do not imply unlimited integration capacity." Every Consult 7 disposition was genuinely incorporated, not performed. No thesis-level or architecture-level flaw was found.

What was found is a cluster of **contract-level contradictions between hand-written twin representations of the same truth** — the machine-readable `contracts/*.yaml` versus the prose authorities they must compile from, plus two forecasts and two waiver semantics published simultaneously. None of these is hard to fix; all of them are load-bearing, because M0 turns `contracts/` into executable registries and every track builds against them. If they land as-is, the org serializes a contradiction at its single most serialized seam. All required fixes are pre-M0 documentation/contract changes; none requires re-architecture. Per the disposition's own rule ("findings are either resolved or recorded with a decision before exploitation kickoff"), resolve F1–F7 before the kickoff decision.

---

## Findings, ordered by severity

### F1 — HIGH · contract defect: `contracts/lifecycles.yaml` contradicts `docs/DOMAIN.md` on both non-project state machines

The package's authority rule (README §Authority: DOMAIN.md wins for behavior) is violated by its own machine-readable precursor:

- **Deliverable states.** DOMAIN §3.5: `draft, ready, submitted, bounced, accepted, waived` — `Waived` is a state with policy requirements. lifecycles.yaml `deliverable.states`: `draft, ready, in_review, bounced, accepted, cancelled` — `submitted` renamed to `in_review`, **no `waived` state, no waiver transition anywhere**, and a `cancelled` state DOMAIN doesn't have. Both `waived` and `cancelled` are plausibly needed; today each file has exactly one.
- **Work-item states.** DOMAIN §11 declares the machine "fixed": `open → in_progress → in_review → done | cancelled` (5 states, bounce edge `in_review → in_progress`). lifecycles.yaml `work_item.states`: `backlog, ready, active, blocked, review, done, cancelled` (7 states, different names, plus `block/unblock` commands). Worse than a rename: DOMAIN §5.3 defines `blocked` as a **derived health projection** ("unresolved blocking dependency or hard gate"); the yaml makes it a manually-entered state — a direct philosophical contradiction, not just vocabulary drift.
- **Activation requirements.** DOMAIN §4.2 exploration activation requires decision criteria and a forecast; yaml `mode_contracts.exploration.required_for_activate` omits both (and lists `hypothesis, unknown, falsifier` with undefined AND/OR semantics where DOMAIN says "hypothesis/unknown"). DOMAIN exploitation activation requires forecast and priority rationale; yaml omits both and adds `gate`, which DOMAIN doesn't require.

**Failure mode if unfixed:** M0's `lifecycle_contract` gate compiles the yaml into the executable registry; Track B implements DOMAIN's aggregates; Track C builds board columns from the yaml; the walking slice can't even name its states. **Fix:** decide each divergence explicitly (recommendation: DOMAIN's 5-state work item + derived blocked; deliverable set `draft, ready, submitted, bounced, accepted, waived, cancelled` with the waiver transition from F3); then make one representation generated from the other and add a machine diff to the M0 contract-freeze gate. Twin hand-written artifacts of one truth will drift again — this package proves it four times (F1, F2, F3, F4).

### F2 — HIGH · two published forecasts: SPEC §11 + PROJECT.toml contradict DELIVERY §3 on every shared milestone

| Milestone | SPEC §11 / PROJECT.toml (P50/P90) | DELIVERY §3 (P50/P90) |
|---|---|---|
| Walking slice | T+2 / T+3 | M1: T+3 / T+5 |
| Dogfood-ready | T+10 / T+14 | M4: T+14 / T+21 |
| Release candidate | T+21 / T+28 | M6: T+28 / T+42 |

The package that defines PR-FC-02 ("reforecasting never mutates the previous forecast — it supersedes it, with basis") is itself carrying two simultaneous current forecasts with no supersession or basis. Beyond the embarrassment, it's operationally live: DoD and milestone reviews reference "the forecast," and the research protocol will score calibration against it. DELIVERY's numbers look like the honest re-estimate; SPEC/toml look stale. **Fix:** one forecast supersedes the other, with a one-line basis, in all three files.

### F3 — HIGH · waiver semantics are fragmented across five files and unverifiable as written

- DOMAIN §3.5: "A **required** hard gate cannot be waived" (qualifier implies unrequired hard gates can be).
- VERIFICATION §5.3: "hard gate cannot be waived" (no qualifier — never).
- acceptance.yaml `hard_failures`: `hard_gate_waiver` (never, by anyone).
- SECURITY §6.1: "A **project owner** cannot waive a hard gate" (implies someone else can).
- permissions.yaml: `human_required: [decision.record:hard_risk_acceptance, decision.record:human_only_waiver]` and `never_allowed.automation: [decision.record:waiver]` — a **qualifier grammar (`action:qualifier`) that is defined nowhere**; there is no enum of waiver kinds, so the §9.3 generated allow/deny matrix cannot be complete.
- lifecycles.yaml: no waiver transition exists at all (F1).
- Undefined interaction: DOMAIN §4.4 completion requires "all required deliverables accepted **or policy-valid waived**" AND "every hard gate passes." If a required deliverable is waived, do its hard gates still count? If yes, waiver is meaningless; if no, a hard gate can be bypassed by waiving its deliverable — precisely the leak the whole design exists to prevent. Can a deliverable be waived *while* one of its hard gates is failing?

**Fix:** one authoritative waiver section (DOMAIN, cross-referenced by SECURITY/permissions/lifecycles/VERIFICATION): (a) hard gates are never waivable by anyone — make DOMAIN drop "required" and SECURITY §6.1 say "no actor" (the current sentence reads as a separation-of-duty rule, not the absolute it should be); (b) deliverable waiver is legal only when no hard gate on it is failing-or-pending, requires the policy-designated human decision kind, and excludes that deliverable's soft gates from the completion check — state this as a command invariant; (c) define the decision-kind/qualifier enum so the permission matrix generator has a closed vocabulary; (d) add the waive transition to the deliverable lifecycle contract.

### F4 — MEDIUM · permission contract drift: roles and action vocabulary don't match their authority docs

- **Roles:** SECURITY §5.2 project roles: `owner, steward, contributor, reviewer, viewer`. permissions.yaml `roles.project`: `[owner, contributor, reviewer, observer]` — **`steward` missing entirely** (yet DOMAIN's `Portfolio.StewardID` and the third-pivot portfolio-steward review depend on it), and `viewer` vs `observer` is an unresolved name for the same role. README says SECURITY.md wins for access control; the yaml — the matrix-generator input — must follow it.
- **Actions vs API surface:** API.md routes with no defined permission action: `POST /projects/{id}/target` and `/deadline` (project.edit? project.reforecast?), hypotheses/experiments/observations (§9), comments on non-brief resources (`brief.comment` covers only briefs; API §12 allows comments on any resource_type), `inbox/{id}/ack`, `judgment-queue/{id}/claim` (resolve correctly maps to the underlying command, but claim is itself a mutation), `work-items/{id}/transition` (work.edit?). Conversely `audit.read` exists in both vocabularies but **no audit read route exists in API.md**. The parity manifest's `operation_without_permission_action` CI failure will catch this at M0 — but the contract-freeze gate requires the permission contract to be *complete*, so enumerate now rather than churn M0.

### F5 — MEDIUM · staging leaks in the acceptance authority itself (SPEC)

- **§5.6 bullet 1** — "Realtime multi-user brief editing with presence and conflict-free merge" — is the v1-full CRDT feature listed as unqualified scope. §5.7 got an explicit staging sentence ("The inbox and event subscriptions are v1-core. The ECA rule compiler/executor is v1-full…"); §5.6 did not. An autonomous builder reading §5.6 alone scopes CRDT into core. Add the parallel sentence: "Versioned sections with section-level locking are v1-core; simultaneous co-editing/presence is the v1-full collaboration exploration."
- **§8 workflow 11 (core half) specifies the wrong behavior:** "Concurrently edit separate brief sections and prove typed conflicts prevent loss." Separate-section concurrent edits must **commit without conflict** (that is the point of section locking — ARCHITECTURE §8.0 and VERIFICATION §10.1 both say so); it's *same-section stale* edits that must produce the typed conflict. Since SPEC is the acceptance authority and detailed docs "may not weaken it," a literal reading makes VERIFICATION §10.1 non-compliant. Reword to: "Concurrently edit separate sections and prove both commit; produce a same-section stale edit and prove a typed conflict prevents loss."
- **UX §2.1 sidebar lists `Automations` unconditionally** (and §11/§13 reference automation items). The package's own dormant-surface rule (acceptance.yaml `forbidden_dormant_surfaces`, VERIFICATION §21 "no dormant endpoint, migration, process, or feature flag") forbids shipping an Automations nav entry in an unpromoted core. Mark it promotion-conditional.

### F6 — MEDIUM · the global concurrency budget is never stated as one number

DELIVERY §1 says "two product tracks plus one independent verification/review path," expanding to three on health. But §4's per-track WIP (B=2, C=2, E=2, plus D=1 and F=1 outside release-bearing WIP, plus G) means "two tracks" can be **4 concurrent implementation units immediately after M1 and 6 at three tracks** — against a field-tested reference of ~3 concurrent builds per laptop and a review pool that is the proven binding constraint. PROJECT.toml caps *tracks* (2→3); nothing caps *units*, and only Track C carries an expansion health condition (review queue age < 1 day). "Track" vs "WIP unit" semantics are undefined. **Fix:** state the total release-bearing implementation WIP as one number (recommendation: ≤4 initially), apply the C-style expansion rule (merge latency + review queue age) globally, and define track/unit vocabulary in DELIVERY §1.

### F7 — MEDIUM · the study's own delivery plan is not representable in the product (second-dogfood gap)

Forecasts attach only to projects (DOMAIN §3.8: one active project-level forecast; deliverables have no date/forecast fields — though UX §7.1 casually displays per-deliverable "forecast impact"). The second treatment is Workplane's own backlog, whose plan is **seven milestones each carrying P50/P90** (DELIVERY §3). Modeled as one project with milestone-deliverables, those per-milestone forecasts cannot be recorded; modeled as seven sequential projects, the convention is heavyweight — and the choice **materially changes H4's calibration cohort accrual** (1 terminal project vs 7). RESEARCH's own falsifier "whether the model can represent real work" (shadow phase) will trip on the package's own plan. **Fix (pre-decide, don't discover):** either add an optional `deliverable_id` scope to Forecast in v1-core (small, contract-level), or record the milestones-as-projects modeling decision now, with the H4 implication stated.

### F8 — LOW cluster · naming/ontology hygiene (the package's own discipline, applied)

- **DOMAIN §1:** "`Task` may appear in user-facing copy as a familiar synonym for work item." Two names for one concept in the product's own surface violates the one-concept-one-name rule this package elsewhere enforces; agents read UI copy too. Pick `work item` everywhere.
- **SECURITY §4.2:** token expiry defaults are specified "for **workers**" and "for **persistent managers**" — Kensho runtime vocabulary leaking into the product's security policy, the exact register mixing DOMAIN §12 forbids. Say "short-lived task tokens / long-lived service tokens."
- **DOMAIN §7:** the brief event family is written for the v1-full model ("document updates remain in the Yjs store") — in core, section writes produce revisions plus "a brief event" (ARCHITECTURE §8.0) whose event type is never named. Name it (e.g. `brief.section_updated` with revision id/digest) and make the Yjs parenthetical promotion-conditional.
- **UX §2.2:** exploration-mode navigation is unspecified — the evidence map (§6.1) and experiments live in no named tab.

### F9 — LOW cluster · small contract/spec gaps

- API §11 asserts "batch board movement uses one endpoint" but the route is not enumerated.
- SPEC workflow 14 "prove API and realtime access stop **immediately**" vs realtime suite "revoke closes live connection under five seconds" (SECURITY §15: "within one heartbeat, target under five seconds") — define *immediately* as ≤5 s locally in the SPEC, or the workflow is unverifiable as written.
- SPEC §12 promotes the CRDT track "within its forecast," and PROJECT.toml repeats it — but Track D never publishes a forecast. Require the track charter to record its own P50/P90 at start, or the promotion criterion has no denominator.
- H0's support threshold (≥20% median useful-output gain over "matched contextual cases") leans on the contextual baseline the package itself demoted to non-causal context (§5.1). The design honestly permits "unresolved"; keep it honest under narrative pressure — an unresolved H0 with supported H2/H3/H5/H7 is a publishable, respectable outcome.

### F10 — SEQUENCING (outside the package, decide now) · the preferred first treatment is likely to disqualify itself

RESEARCH §5.3 correctly requires the naming wave to be treatment **only if Workplane is dogfood-ready before the wave begins** (no backfill — good). But dogfood-ready is P50 T+14/P90 T+21 *after a kickoff that hasn't happened*, and the naming wave is a v0.5 prerequisite that presumably wants to start soon. Unless Kensho deliberately holds the wave ~3+ weeks, the preferred treatment evaporates and the fallback ("next comparable Kensho epic, record why") applies. The package handles both branches; **the decision** — hold the wave for the tool, or pre-select the fallback treatment now — **is Kai/James's, is time-sensitive, and is currently unmade.** PROJECT.toml's unconditional `first_dogfood = "Kensho v0.5 naming and ontology wave"` should carry the conditional.

---

## Sufficiency for autonomous implementation

**Yes, after F1–F7.** The test I applied: could a worker take a backlog item and know what to build, and could a verifier/reviewer know what green means, without asking a human? The package passes: requirement IDs (PR-*) exist and are referenced by acceptance cases and the parity manifest; the acceptance-case schema (VERIFICATION §3) is executable and evidence-bound; lifecycle/permission/parity/staged-acceptance are machine contracts with named M0 gates; environments, fixtures (including the 1M-event scale fixture), fault-injection points, and one-command gate entrypoints (§20) are specified; every backlog item carries dependencies and a hard acceptance sentence; the DoR (DELIVERY §12) plus wave discipline (only M0/M1 ready pre-slice) prevents premature dispatch. The staged-enforcement table (SPEC §9.8) closes the bounce-storm risk from Consult 7. The one systemic weakness is the twin-authoring pattern that produced F1–F4: the fix (single source + generated diff at contract freeze) is itself a required fix above.

**Feature-count check (asked explicitly):** the ambition is coupled, not enumerated — the hard parts (event-sourced ledger + replay checksums, multi-writer conflicts, actor parity with real authz, operated recovery, instrumented research) are exactly the thesis-bearing difficulty, and RESEARCH §2 explicitly disavows manufactured difficulty. The two components that were feature-count risks in Consult 7 (CRDT, ECA) are now correctly quarantined behind promotion gates with kill criteria and a no-dormant-surface rule. Remaining trimmable garnish is minor (saved personal portfolio views, column-reorder persistence) and not worth a finding.

**What is genuinely excellent, for the record:** the universal decision primitive (SPEC P8, DOMAIN §3.9); dates-never-transition encoded as a lifecycle contract rule *and* a command invariant *and* a hard failure; H0–H8 with numeric thresholds, shadow phase, and quantified study stop criteria (RESEARCH §11); the judgment queue as the differentiating surface with its own golden path; the parity manifest with CI failure classes; "a project with many task updates but no accepted deliverable must look stalled, not busy" (UX §4) — the whole thesis in one sentence.

---

## Disposition of required fixes

| # | Severity | Fix before | One-line action |
|---|---|---|---|
| F1 | HIGH | M0 / kickoff | Reconcile lifecycles.yaml ↔ DOMAIN (states, blocked-as-derived, activation lists); single source + generated diff gate |
| F2 | HIGH | kickoff | One forecast supersedes the other across SPEC §11 / PROJECT.toml / DELIVERY §3, with basis |
| F3 | HIGH | M0 / kickoff | One waiver spec: hard gates never waivable; waived-deliverable × completion invariant; qualifier enum; lifecycle transition |
| F4 | MEDIUM | M0 | permissions.yaml roles follow SECURITY §5.2 (add steward; settle viewer/observer); complete action↔route mapping; resolve audit.read |
| F5 | MEDIUM | kickoff | Tag SPEC §5.6 CRDT bullet v1-full; fix workflow 11 wording; make Automations nav promotion-conditional |
| F6 | MEDIUM | kickoff | State the global release-bearing WIP number + global expansion rule; define track vs unit |
| F7 | MEDIUM | M0 | Pre-decide milestone representability (deliverable-scoped forecast vs milestones-as-projects); note H4 impact |
| F8–F9 | LOW | M0–M1 | Naming hygiene (Task synonym, worker/manager leak, brief event name); batch route; ≤5s "immediately"; Track D forecast; H0 honesty note |
| F10 | DECISION | now | Kai/James: hold the naming wave for dogfood-ready, or pre-select the fallback treatment; condition PROJECT.toml `first_dogfood` |

**Verdict: APPROVE WITH REQUIRED FIXES** — resolve F1–F7 (and decide F10) before the exploitation-kickoff decision; F8–F9 may land during M0–M1. No re-review of the whole package is needed; a diff-review of the fix commit satisfies the disposition's advisor gate.

---
*Positions applied: P4 (durable artifact), P7 (one path, one owner — generalized to one truth, one representation), P9 (waivers through recorded authority), P10 (seams/serialization), P12 (names are prompts; register separation), P13 (one plane), P14 (dates trigger attention, never transitions), P15 (stage the bar, detach the risk). New position distilled: P16 — twin artifacts must be generated, never transcribed (four independent drift instances in one package: F1, F2, F3/F4, F5-workflow-11).*

---

# Addendum — diff confirmation: **DIFF APPROVE**

**Date:** 2026-07-10 ~13:52Z · **Requested by:** Kan (msgs b99b6c56, 06b20668) · **Snapshot reviewed:** working tree at max mtime 13:50:54Z (permissions.yaml, API.md, DOMAIN.md). The tree was edited live throughout the original review (three fix cohorts: ~13:36–13:40Z, ~13:47–13:49Z, ~13:50–13:51Z); this confirmation covers the tree as of this snapshot, and later edits fall to the normal M0 gates.

**Every finding is resolved in current bytes, verified surface by surface — not from the disposition table's word:**

- **F1 ✓** `lifecycles.yaml` now declares `source_of_truth: true` with `generated_views: [docs/DOMAIN.md]`, and DOMAIN states M0 CI regenerates the views — P16 adopted literally. Deliverable machine `draft/ready/submitted/bounced/accepted/waived/cancelled`; work item is DOMAIN's fixed 5-state graph with `derived_health_states: [blocked]`; activation predicates match DOMAIN exactly (including `any_of_for_activate: [active_hypothesis, explicit_unknown]` with a conditional falsifier requirement — cleaner than either file had it).
- **F2 ✓** One P50/P90 series (3/5, 14/21, 28/42) in SPEC §11, PROJECT.toml, and DELIVERY §3, stated as the post-scope re-estimate.
- **F3 ✓ — the strongest resolution in the set.** Hard gates are never waivable **by construction at four layers**: DOMAIN prose ("never waivable by any actor"), a new gate lifecycle (`pending/passed/failed/waived` with soft-only waive transitions and `hard_gate_waiver: forbidden`), permissions (`{subject: any_actor, forbidden_relation: hard_gate_waive}`), and the decision-kind vocabulary itself (`risk-acceptance` added; "`hard-gate` is deliberately absent" — the waiver cannot even be *recorded*). Deliverable waiver requires all attached hard gates passed + no open blocking finding + policy-designated human + decision + rationale + residual risk, which makes the completion invariant ("all hard gates passed") coherent for waived deliverables. Colon-qualifiers are gone (`actions.qualifiers: forbidden`); `deliverable.waive` and `gate.soft_waive` are first-class human-required actions; API routes exist and "reject hard gates in every state."
- **F4 ✓** Project roles now `owner/steward/contributor/reviewer/viewer` matching SECURITY §5.2; the action vocabulary grew the missing families (project lifecycle verbs, `project.target.write`/`project.deadline.write`, `comment.*`, `inbox.*`, `judgment.*`, `deliverable.cancel/reforecast`, `finding.withdraw`); a route→action map covers every route under the rule `every_authorization_target_is_one_enumerated_action_string`; `GET /orgs/{org_id}/audit` exists.
- **F5 ✓** SPEC §5.6 splits core sections from "optional promoted CRDT"; workflow 11 now reads "prove both commit; then issue a stale same-section edit and prove a typed conflict prevents loss"; UX sidebar marks `Automations*` as existing only after ECA promotion and adds an Experiments tab (exploration mode only).
- **F6 ✓** "Unit" defined; global release-bearing WIP ≤ 4 across all tracks ("not four per track"), rising to 6 only after two consecutive green integration cycles, median merge latency < 1 day, review-queue p90 < 1 day, no red main, and an explicit integration-owner decision; promoted exploration code consumes the same ceiling.
- **F7 ✓** `Forecast.DeliverableID` added (nil = project forecast); DELIVERY models Workplane as one project with M0–M6 milestone deliverables carrying deliverable-scoped P50/P90; RESEARCH: "H4 counts terminal projects only," milestone calibration reported separately.
- **F8/F9 ✓** "Work item is the only product term"; SECURITY speaks of short-lived task tokens / long-lived service tokens; `brief.section_updated` is in the core event family with Yjs wording promotion-conditional; `work-items/batch-transition` enumerated; workflow 14 now "new API access fails immediately and every realtime connection closes within five seconds"; Track D's charter must record its own P50/P90 before start.
- **F10 ✓** PROJECT.toml: `dogfood_strategy = "conditional preferred treatment with post-v0.5 fallback; never hold v0.5 for Workplane"` — the sequencing decision is made and recorded the right way around (the product study yields to the org's release, not vice versa).

**Non-blocking residuals** (fold into any next touch; none gates kickoff): SECURITY §6.1 still says "a project owner cannot waive a hard gate" — true but weaker than the any-actor rule now in force; tighten to "no actor." acceptance.yaml (untouched since 13:33Z) remains consistent but should gain the traceability contract in its contract-freeze gate list when next edited. ARCHITECTURE.md was not in the final cohorts; its automation section is consistent with the execution-context model, but it should be swept once by the M0 regeneration gate like everything else.

**Also verified though not requested:** the automation-as-execution-context refactor (actor kinds reduced to `human | agent`; ECA runs as a restricted agent execution context carrying rule/event/human-principal) landed consistently across DOMAIN, SPEC §7/§9.3, API §2.3, SECURITY §6.1, VERIFICATION §8, and permissions.yaml — an ontology improvement beyond my findings, and the correct P13 shape.

**Verdict: DIFF APPROVE.** The specification advisor gate is closed. Exploitation kickoff may proceed on the recorded promotion checklist (DELIVERY §2). The live-editing pattern that made this review a moving target should not recur post-kickoff: after M0, contract changes flow through the serialized owner and the regeneration gate, not in-place sweeps.
