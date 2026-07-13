# M2D evidence-review spine

M2D adds the smallest immutable evidence and evidence-gated review contract on
top of the accepted M1, M2A, M2B, and M2C spines. It does not add terminal
project commands, portfolio breadth, hypotheses or experiments, work and
dependency graphs, inbox or judgment queues, comments, search, structured
briefs, Track C UI, collaborative editing, automation, scale, or release claims.

## Immutable evidence and stable gates

Each evidence record declares a kind, title, claim, source, optional content or
URI, bounded string metadata, typed links to project-owned review targets, a
PostgreSQL-computed integrity digest, and the producing actor plus delegated
human principal. Evidence rows and links are append-only. A correction is a new
record pointing to its predecessor; the project transaction lock prevents a
supersession chain from branching. Only the current leaf can satisfy a
submission, finding disposition, verdict, or waiver.

A deliverable gate freezes its automated/review/approval kind, hard/soft
classification, independence policy, and exact evidence requirements when the
deliverable first enters review. Verdict rows are immutable. A failed verdict
creates actionable findings in the same transaction; a later pass retains and
supersedes the failure. Finding resolution or withdrawal retains a separate
attributed action linked to current evidence.

## Review lifecycle and authority

The public cycle is `ready -> submitted -> bounced -> submitted -> accepted`.
An optional deliverable may separately move from `draft` or `ready` to the
immutable `cancelled` state; required deliverables cannot use that command.
Submit and resubmit create immutable submission records and move a constrained
submission head. Bounce requires one current actionable finding. Resubmit
requires every finding closed plus linked resolution evidence. Approval
requires every hard gate passed and no open finding.

Separation of duty compares effective identities, including an agent's
delegated human principal. An independent reviewer cannot be the deliverable
creator, current submitter, producer of any evidence retained by the current
submission, or producer of verdict evidence. Denials occur before any durable
write, including idempotency and token-usage residue.

Hard gates are never waivable. A soft gate or deliverable waiver is a
policy-designated human judgment represented by the same universal decision
shape as every consequential judgment. It requires current linked evidence,
rationale, alternatives, consequences, and residual risk; every hard gate must
already pass and no blocking finding may remain. Agent waiver attempts fail
before writes.

## One public and durable plane

Human cookie/CSRF and agent bearer clients call the same OpenAPI operations,
generated adapters, idempotency check, `If-Match` version contract, serializable
PostgreSQL transaction, event ledger, outbox, replay projection, integrity
doctor, and realtime consumers. The M2D event families are evidence, gate,
verdict, finding, submission, bounce/resubmit/acceptance, decision, and waiver.
Replay rebuilds all review projections, including optional-deliverable
cancellation, into the same atomic shadow generation
as prior milestones and fails closed on unknown schema versions or event types.

Run `make smoke` for the complete accepted gate. The real-PostgreSQL M2D cases
run inside `scripts/test_planning.sh`; load-bearing artifacts named `m2d-*.json`
are written to `target/agent-evidence/m2c/` and copied into the clean exact-head
manifest by `make evidence-smoke`.
