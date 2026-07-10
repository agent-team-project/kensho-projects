# Advisor Disposition

**Consultation:** `initial-critique.md`
**Status:** final review required fixes incorporated; `DIFF APPROVE`

## Accepted

| Recommendation | Disposition in this package |
|---|---|
| Name the product after its thesis | Renamed to **Workplane**: one work plane for human and agent actors. |
| Sharpen the scarce resource | Primary persona is 1-10 humans directing 10-100 agents; human judgment and absorption are explicit constraints. |
| Add a judgment queue | Core navigation, domain projection, backlog, browser path, and acceptance cases now include it. |
| Make decisions universal | Scope, lifecycle, waiver, priority, forecast, architecture, and mode judgments use one immutable decision primitive. |
| Treat portfolio as allocation | Portfolios expose target versus actual exploration/exploitation mix and drift. |
| Separate dates | Target, P50/P90 forecast, deadline, and actual are distinct types; no date triggers a lifecycle transition. |
| Stage without lowering the bar | `v1-core` is release-bearing; CRDT and ECA are detachable `v1-full` explorations with promotion and stop criteria. |
| Start with a walking slice | One serial real-DB/API/client/browser slice precedes two-track fan-out; a third track requires healthy review/merge latency. |
| Keep one serialized contract seam | Domain, permission, event, API, and error vocabulary have one integration owner and machine-readable precursors. |
| Demote weak historical comparison | Historical Kensho cases are contextual evidence, not a controlled baseline. |
| Pre-register falsifiers | Adoption, absorption cost, design yield, P50/P90 calibration, gate integrity, and integration debt have explicit metrics and stops. |
| Dogfood twice | First on a consequential Kensho project, second on Workplane's own backlog. |
| Map vocabularies | `DOMAIN.md` maps Kensho concepts to Workplane concepts and identifies intentional differences. |
| Prevent release hostage-taking | A pre-registered dogfood checkpoint can satisfy release evidence while longitudinal research continues. |
| Add freeze points | The package is `specified, pre-slice`; only M0/M1 backlog items may become ready before slice repricing. |

## Modified

The advisor recommended avoiding a deep ready backlog before the walking slice.
The user explicitly requested an entire ambitious project specification, so the
120-item list remains as a full scope inventory. It is not a ready queue: all
post-M1 items are provisional and must be split, merged, repriced, or stopped
from measured slice evidence.

The advisor proposed aggressive scope detachment. The full ambition remains
specified so successful explorations have a hard target, but only the core is
release-bearing until each detached track earns promotion.

## Declined

Compatibility upcasters, aliases, and transitional shims are not adopted.
A vocabulary or event schema correction at any version uses a coordinated clean
migration: archive or export the old ledger for audit, migrate authoritative
state, regenerate fixtures, and remove the old contract. No release reader
carries both meanings forward.

## Final review disposition

The advisor returned `APPROVE WITH REQUIRED FIXES` in `final-review.md`. The
review observed a moving working tree, so F2 and part of F5 had already been
resolved while it ran. The complete disposition is:

| Finding | Resolution |
|---|---|
| F1 lifecycle drift | `lifecycles.yaml` is canonical; deliverable/work/gate states and activation/completion predicates now match generated DOMAIN views. `blocked` is derived. |
| F2 dual forecasts | Static day estimates were removed by operator direction. PROJECT, SPEC, and DELIVERY now share one policy: create evidence-based timestamp P50/P90 forecasts at kickoff and supersede them after each gate. |
| F3 waiver ambiguity | Hard gates are never waivable. Soft-gate and deliverable waivers have separate human-only actions, decision qualifiers, transitions, and completion invariants. |
| F4 permission drift | Project roles match SECURITY; missing route actions and audit route are enumerated; colon-qualified actions are forbidden. |
| F5 staging leaks | Core brief behavior, conditional CRDT/ECA workflows, same-versus-separate-section behavior, and conditional navigation are explicit. |
| F6 undefined WIP | Track and unit are defined; global release-bearing WIP is 4, expandable to 6 only through a measured health gate. |
| F7 dogfood representation | Workplane is one project with M0-M6 milestone deliverables and deliverable-scoped forecasts excluded from H4 project counts. |
| F8-F9 hygiene | Work-item naming, token vocabulary, brief events, exploration navigation, batch route, revoke threshold, and collaboration-track forecast are fixed. |
| F10 treatment timing | Workplane never holds v0.5; naming is conditional, with the first consequential post-v0.5 epic as the pre-registered fallback. |

The advisor appended `DIFF APPROVE` after checking the fixes against current
bytes. The specification advisor gate is closed. Exploitation may begin only
through the remaining promotion checklist in `docs/DELIVERY.md` section 2.
