# Workplane

An agent-native project and portfolio system for humans and autonomous software
organizations.

**Status:** specified, pre-slice; implementation not started.

Workplane is not a Jira or Notion clone. It joins project briefs, deliverables,
work, evidence, decisions, forecasts, and portfolio priority in one shared
system used by human and agent actors through the same permissions and domain
commands.

The product is intentionally more difficult than Excel-lite. Excel-lite tested
breadth and parallel implementation behind a stable calculation interface.
Workplane tests integration: concurrent users, realtime state, collaborative
documents, durable event history, authorization, agent participation, search,
automation, calibrated forecasting, and an empirical research protocol.

## Specification map

- `SPEC.md` - authoritative build contract and acceptance bar.
- `PROJECT.toml` - machine-readable project intent and best-effort forecast.
- `docs/PRODUCT.md` - users, jobs, product principles, scope, and non-goals.
- `docs/DOMAIN.md` - vocabulary, lifecycle, invariants, and event model.
- `docs/ARCHITECTURE.md` - system shape, module boundaries, and interfaces.
- `docs/UX.md` - information architecture and end-to-end workflows.
- `docs/API.md` - human/agent command, query, realtime, and error contracts.
- `docs/SECURITY.md` - identities, permissions, threat model, and security gates.
- `docs/VERIFICATION.md` - executable behavior suites and evidence contract.
- `docs/RESEARCH.md` - hypotheses, metrics, dogfood protocol, and falsification.
- `docs/DELIVERY.md` - parallelization plan, forecasts, gates, and release policy.
- `contracts/` - machine-readable lifecycle, operation/permission, parity,
  traceability, and staged acceptance precursors that M0 turns into executable
  registries.
- `backlog/README.md` - seeded epics and independently deliverable issues.
- `docs/advisor/` - durable advisor consultations and dispositions.

## Authority

`SPEC.md` is the product acceptance authority. Detailed documents may refine a
requirement but may not weaken it. Closed state/action vocabularies in
`contracts/lifecycles.yaml` and `contracts/permissions.yaml` are canonical and
generate their prose tables at M0. Domain invariants in `docs/DOMAIN.md` govern
remaining behavior, `docs/SECURITY.md` governs remaining access semantics, and
the stricter quality requirement wins everywhere else.

`docs/advisor/` is a historical decision record, not delivery authority. Any
calendar estimates quoted inside those reviews are superseded by the current
no-precommitted-duration policy in `SPEC.md`, `PROJECT.toml`, and
`docs/DELIVERY.md`.

No implementation should begin until the specification review gate is approved
and an exploitation kickoff is recorded. Forecasts start at that kickoff, not
at the date these documents were written.

## Freeze points

This package is a pre-slice hypothesis, not settled product law. Documents are
frozen for the first walking slice only after specification review. The slice
must produce an explicit contract-repricing decision before broad fan-out.

The full scope inventory is intentionally ambitious. `v1-core` is the
thesis-bearing dogfood gate; `v1-full` adds promoted exploration tracks such as
CRDT co-editing and declarative automation. Staging controls integration risk;
it does not lower the final verification bar.
