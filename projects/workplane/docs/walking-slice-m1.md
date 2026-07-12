# M1 walking-slice seam

Status: pinned next transaction; product behavior is explicitly unimplemented
at M0. Authority: `DELIVERY.md` M1, `SPEC.md` principles P1/P5 and staged gate,
requirements `PR-CORE-01`, `PR-CORE-02`, and `PR-DEC-01`.

## One transaction, two actor kinds

The M1 test starts the production Go binary and production web build through
offline Compose against an empty **real PostgreSQL** database migrated from the
checked-in sequence. A human authenticates through the public session route,
creates one proposed exploration project through **public HTTP**, reads it in
the portfolio/project **production browser** path, records a continue decision,
and reads its immutable activity. The browser must call the **generated
TypeScript client**; direct database, private handler, fixture-only, or
browser-local mutation fails parity.

An equivalently scoped **agent token** repeats create/read/decision/activity
through the same OpenAPI operations. The human and agent outcomes differ only
in attributable actor identity. The agent carries a human principal and cannot
widen its delegated authority.

## Frozen command group

1. `POST /api/v1/session/login` establishes the human session.
2. `POST /api/v1/orgs/{org_id}/projects` accepts a complete exploration
   contract and an **idempotency** key.
3. `GET /api/v1/projects/{project_id}` returns the committed projection.
4. `POST /api/v1/projects/{project_id}/decisions` records the universal
   decision shape with an idempotency key and required `If-Match: "<version>"`;
   stale or missing versions deny before mutation and success returns the new
   version as `ETag`.
5. `GET /api/v1/projects/{project_id}/activity` returns immutable events in
   commit order.

The create command atomically writes the project and `project.created` event.
The decision command atomically writes the immutable decision and
`decision.recorded` event. Aggregate versions are contiguous. An identical
retry returns the original status/body/version/event group; a different body
under the same key returns a typed conflict and writes nothing.

Every protected browser mutation combines the `workplane_session` cookie with
`X-CSRF-Token`; reads use the session cookie. The equivalent agent calls use
`Authorization: Bearer <token>` and never a browser cookie or CSRF substitute.
These alternatives and the expected-version header are authoritative OpenAPI
surfaces carried by both checked-in generated clients.

## Required boundaries and denies

- **authorization** evaluates active principal, membership, organization,
  role, token scope, actor-kind policy, separation of duty, and state;
- revoked, disabled, cross-organization, and out-of-scope principals deny;
- missing idempotency key and stale expected version deny before mutation;
- transaction failure at any write boundary leaves no partial state or event;
- activity is projected from the committed event ledger, not constructed from
  request logs; and
- a clean, outbound-network-blocked **offline Compose** start is part of the M1
  exit gate.

## M1 evidence and exit

The exact commit must attach PostgreSQL migration output, API responses, event
rows, idempotent retry comparison, expected-deny results, production-browser
screenshot/DOM/console/network evidence, generated-client zero diff, and the
smoke manifest. Functional, transaction/idempotency, permission, browser, and
offline startup gates must all pass. No in-memory substitute, mock browser,
self-review, or hard-gate waiver counts.

## Deliberately deferred

No realtime transport, search, brief editor, broad lifecycle, portfolio
ranking, CRDT collaboration, ECA automation, scale fixture, or release claim is
part of this transaction. M1 evidence triggers contract repricing before Track
B/C fan-out.
