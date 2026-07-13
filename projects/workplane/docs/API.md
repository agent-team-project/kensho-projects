# API and Realtime Contract

## 1. Protocol principles

- Base path: `/api/v1`.
- JSON uses `snake_case` and UTF-8.
- Timestamps are RFC 3339 UTC.
- IDs are opaque strings.
- Mutations require `Idempotency-Key`.
- Updates require `If-Match: "<version>"` or an explicit `expected_version`.
- Responses include `ETag` and `X-Request-ID` where applicable.
- Human sessions and agent tokens call the same routes. Promoted ECA executes
  under a restricted agent service identity with rule/event context.
- Every agent request resolves a mandatory principal from token delegation;
  events expose both agent actor and principal.
- Errors use one stable problem format.
- OpenAPI is authoritative and generated clients are checked in.

## 2. Authentication

### 2.1 Human session

Browser requests use a secure, HttpOnly, SameSite session cookie and CSRF token
for mutations. A successful login issues `workplane_session` in `Set-Cookie`
and returns the corresponding non-secret `csrf_token` in the JSON session body;
the browser client accepts the cookie with same-origin credentials and sends
that token as `X-CSRF-Token`. Login, logout, session listing, and session
revocation are the only unauthenticated/authentication routes.

### 2.2 Agent token

Agents use `Authorization: Bearer <token>`. Tokens are opaque, shown once,
stored hashed, and scoped by organization, service identity, actions, optional
project ids, expiry, and IP/rate policy.

### 2.3 Automation execution context

Automation is not a third principal kind and does not hold a reusable bearer
token. The executor creates a signed internal context tied to an owning agent
service identity, rule id/version, triggering event id, and human principal.

## 3. Common resource envelope

```json
{
  "data": {},
  "meta": {
    "request_id": "01...",
    "version": 7
  }
}
```

Collections use cursor pagination:

```json
{
  "data": [],
  "page": {
    "next_cursor": "opaque-or-null",
    "has_more": false
  },
  "meta": { "request_id": "01..." }
}
```

Cursors bind to organization, actor, sort, and filters and expire after a
documented retention period.

## 4. Error format

```json
{
  "type": "https://workplane.local/problems/version-conflict",
  "title": "Project version changed",
  "status": 409,
  "code": "version_conflict",
  "detail": "Expected version 8; current version is 10.",
  "request_id": "01...",
  "errors": [
    { "field": "forecast.p90_at", "code": "stale_value" }
  ],
  "current": {
    "version": 10,
    "changed_fields": ["forecast"]
  }
}
```

Required codes include `invalid_request`, `unauthenticated`, `forbidden`,
`not_found`, `version_conflict`, `idempotency_conflict`, `invariant_violation`,
`dependency_cycle`, `gate_blocked`, `rate_limited`, `cursor_expired`,
`snapshot_required`, and `service_unavailable`.

`404` is used instead of `403` when revealing resource existence would leak
cross-organization or private-project information.

## 5. Organizations and identities

```text
POST   /api/v1/session/login
POST   /api/v1/session/logout
GET    /api/v1/session
GET    /api/v1/sessions
DELETE /api/v1/sessions/{session_id}

GET    /api/v1/orgs/{org_id}
PATCH  /api/v1/orgs/{org_id}
DELETE /api/v1/orgs/{org_id}
GET    /api/v1/orgs/{org_id}/members
POST   /api/v1/orgs/{org_id}/members
PATCH  /api/v1/orgs/{org_id}/members/{actor_id}
GET    /api/v1/orgs/{org_id}/audit

GET    /api/v1/orgs/{org_id}/service-identities
POST   /api/v1/orgs/{org_id}/service-identities
PATCH  /api/v1/orgs/{org_id}/service-identities/{id}
POST   /api/v1/orgs/{org_id}/service-identities/{id}/tokens
GET    /api/v1/orgs/{org_id}/service-identities/{id}/tokens
DELETE /api/v1/orgs/{org_id}/service-identities/{id}/tokens/{token_id}
```

Token creation returns the plaintext token exactly once. Listing returns
metadata and prefix only.

## 6. Portfolios and priority

```text
GET    /api/v1/orgs/{org_id}/portfolios
POST   /api/v1/orgs/{org_id}/portfolios
GET    /api/v1/portfolios/{portfolio_id}
PATCH  /api/v1/portfolios/{portfolio_id}
GET    /api/v1/portfolios/{portfolio_id}/projects
POST   /api/v1/portfolios/{portfolio_id}/rank
GET    /api/v1/portfolios/{portfolio_id}/timeline
GET    /api/v1/portfolios/{portfolio_id}/capacity
```

Ranking is a mutation command containing the complete intended order for the
affected range, priority input snapshots, and rationale. The server rejects
duplicate/missing ids and version conflicts.

## 7. Projects

```text
GET    /api/v1/orgs/{org_id}/projects
POST   /api/v1/orgs/{org_id}/projects
GET    /api/v1/projects/{project_id}
PATCH  /api/v1/projects/{project_id}
POST   /api/v1/projects/{project_id}/activate
POST   /api/v1/projects/{project_id}/hold
POST   /api/v1/projects/{project_id}/resume
POST   /api/v1/projects/{project_id}/promote
POST   /api/v1/projects/{project_id}/complete
POST   /api/v1/projects/{project_id}/stop
POST   /api/v1/projects/{project_id}/cancel
GET    /api/v1/projects/{project_id}/activity
GET    /api/v1/projects/{project_id}/health
GET    /api/v1/projects/{project_id}/context-brief
```

### 7.1 Create exploration project

```json
{
  "title": "Kensho naming ontology",
  "outcome": "Choose and validate one coherent v0.5 vocabulary.",
  "mode": "exploration",
  "owner_id": "actor_...",
  "primary_portfolio_id": "portfolio_...",
  "intent_target_at": "2026-07-18T16:00:00Z",
  "priority": {
    "outcome_value": "high",
    "urgency": "time-sensitive",
    "uncertainty_reduction_value": "high",
    "value_confidence": 0.8,
    "effort_low_days": 1,
    "effort_high_days": 3,
    "dependency_leverage": "high",
    "risk_reduction": "medium",
    "opportunity_cost": "Delays feature work that would otherwise use old terms.",
    "rationale": "Vocabulary must settle before the v0.5 mechanical wave."
  },
  "forecast": {
    "p50_at": "2026-07-12T12:00:00Z",
    "p90_at": "2026-07-14T18:00:00Z",
    "review_after": "2026-07-11T18:00:00Z",
    "basis": "Advisor proposal exists; stakeholder review remains.",
    "assumptions": ["No new foundational conflict is found."]
  },
  "exploration": {
    "hypotheses": [{
      "statement": "Plain project vocabulary reduces agent ambiguity.",
      "falsifier": "Blind prompt trials show no reduction in interpretation errors."
    }],
    "decision_criteria": ["Operator acceptance", "Blind prompt trial", "Migration feasibility"],
    "time_bound_hours": 24,
    "token_bound": 10000000
  }
}
```

## 8. Forecasts and targets

```text
GET  /api/v1/projects/{project_id}/forecasts
POST /api/v1/projects/{project_id}/forecasts
GET  /api/v1/deliverables/{deliverable_id}/forecasts
POST /api/v1/deliverables/{deliverable_id}/forecasts
POST /api/v1/projects/{project_id}/target
POST /api/v1/projects/{project_id}/deadline
```

### 8.1 Reforecast

```json
{
  "p50_at": "2026-07-15T09:00:00Z",
  "p90_at": "2026-07-17T18:00:00Z",
  "review_after": "2026-07-14T18:00:00Z",
  "basis": "The API vocabulary review found two additional migration seams.",
  "assumptions": ["No compatibility layer will be retained."],
  "reason_codes": ["new-evidence", "estimate-correction"],
  "impact": "The v0.5 implementation start moves by two days; quality gates are unchanged."
}
```

The project and deliverable routes use the same forecast schema. Deliverable
forecasts remain linked to the owning project but are a distinct active scope.
The response includes prior/new forecast ids and a `decision.recorded` event
when scope or target changes in the same command group.

## 9. Exploration

```text
GET    /api/v1/projects/{project_id}/hypotheses
POST   /api/v1/projects/{project_id}/hypotheses
POST   /api/v1/hypotheses/{id}/supersede
GET    /api/v1/projects/{project_id}/experiments
POST   /api/v1/projects/{project_id}/experiments
PATCH  /api/v1/experiments/{id}
POST   /api/v1/experiments/{id}/observations
POST   /api/v1/projects/{project_id}/decisions/continue
POST   /api/v1/projects/{project_id}/decisions/pivot
```

The canonical promotion route is section 7's project lifecycle route. Its body
includes decision, residual uncertainty, and initial deliverables; it is
atomic. There is no duplicate exploration-specific promotion route.

## 10. Deliverables, gates, evidence, and review

```text
GET    /api/v1/projects/{project_id}/deliverables
POST   /api/v1/projects/{project_id}/deliverables
GET    /api/v1/deliverables/{id}
PATCH  /api/v1/deliverables/{id}
POST   /api/v1/deliverables/{id}/criteria
GET    /api/v1/deliverables/{id}/gates
POST   /api/v1/deliverables/{id}/gates
POST   /api/v1/deliverables/{id}/submit
POST   /api/v1/deliverables/{id}/bounce
POST   /api/v1/deliverables/{id}/resubmit
POST   /api/v1/deliverables/{id}/approve
POST   /api/v1/deliverables/{id}/waive
POST   /api/v1/deliverables/{id}/cancel

POST   /api/v1/projects/{project_id}/evidence
GET    /api/v1/projects/{project_id}/evidence
POST   /api/v1/evidence/{id}/supersede

POST   /api/v1/gates/{gate_id}/verdicts
POST   /api/v1/gates/{gate_id}/waive
GET    /api/v1/gates/{gate_id}/verdicts
GET    /api/v1/deliverables/{id}/findings
POST   /api/v1/findings/{finding_id}/resolve
POST   /api/v1/findings/{finding_id}/withdraw
```

An independent gate rejects any actor whose effective identity created the
deliverable, made the current submission, or produced its verdict evidence.
Submit and resubmit require current evidence satisfying every stable gate
contract; bounce requires an open actionable finding; approval requires every
hard gate passed and every finding closed. `gates/{id}/waive` rejects hard gates
in every state. Both waiver routes require a human actor and embed the universal
waiver decision payload with rationale, linked evidence, consequences, and
residual risk; neither accepts a free-form action qualifier.

## 11. Work and dependencies

```text
GET    /api/v1/projects/{project_id}/work-items
POST   /api/v1/projects/{project_id}/work-items
GET    /api/v1/work-items/{id}
PATCH  /api/v1/work-items/{id}
POST   /api/v1/work-items/{id}/transition
POST   /api/v1/work-items/{id}/assign
POST   /api/v1/work-items/{id}/dependencies
DELETE /api/v1/work-items/{id}/dependencies/{dependency_id}
POST   /api/v1/projects/{project_id}/work-items/batch-transition
GET    /api/v1/projects/{project_id}/dependency-graph
```

Batch board movement uses one endpoint with expected versions for every item.
Partial mutation is forbidden: the batch commits or fails atomically.

## 12. Decisions, comments, inbox, and judgment queue

```text
GET    /api/v1/projects/{project_id}/decisions
POST   /api/v1/projects/{project_id}/decisions
POST   /api/v1/decisions/{id}/supersede

GET    /api/v1/{resource_type}/{resource_id}/comments
POST   /api/v1/{resource_type}/{resource_id}/comments
PATCH  /api/v1/comments/{id}

GET    /api/v1/inbox
POST   /api/v1/inbox/{item_id}/ack
POST   /api/v1/inbox/ack-batch

GET    /api/v1/judgment-queue
POST   /api/v1/judgment-queue/{item_id}/claim
POST   /api/v1/judgment-queue/{item_id}/resolve
```

Comment edits preserve revision history. Mentions resolve explicit actor ids;
plain `@text` is not a notification until selected and authorized.

Judgment items expose the decision/review action required, required authority,
impact of waiting, source event, recommendation/evidence, claim state, and
resolution. Resolving an item must execute or link the ordinary public decision
or review command; acknowledging a notification cannot resolve judgment.

## 13. Briefs and optional collaboration

```text
GET  /api/v1/projects/{project_id}/brief/metadata
GET  /api/v1/projects/{project_id}/brief/sections
PATCH /api/v1/projects/{project_id}/brief/sections/{section_id}
GET  /api/v1/projects/{project_id}/brief/revisions
GET  /api/v1/projects/{project_id}/brief/snapshots
POST /api/v1/projects/{project_id}/brief/snapshots
POST /api/v1/projects/{project_id}/brief/restore
GET  /api/v1/projects/{project_id}/brief/export
POST /api/v1/projects/{project_id}/brief/apply
POST /api/v1/projects/{project_id}/brief/grant
```

Core section writes and `brief/apply` use expected section/document revisions
and create immutable revisions. `brief/apply` is the agent-safe structured
editor route and cannot mutate structured project state.

`brief/grant`, WebSocket URL, Yjs state vectors, and CRDT snapshots exist only
after the collaboration exploration is promoted into `v1-full`.

Grant response:

```json
{
  "websocket_url": "ws://127.0.0.1:8081/collab",
  "document_id": "brief_...",
  "grant": "short-lived-jwt",
  "expires_at": "2026-07-10T13:15:00Z",
  "permissions": ["read", "write", "comment"],
  "schema_version": 1
}
```

## 14. Search

```text
GET /api/v1/search?q=...&types=project,decision,evidence&portfolio_id=...
```

Filters include entity type, project, portfolio, mode, state, actor, date range,
tag, and evidence kind. Results include safe snippets, source/index versions,
and a stable resource URL. Search never returns an unauthorized count or facet.

## 15. Automation exploration

The following routes are added only after the post-dogfood ECA exploration is
promoted. Public event subscriptions and inbox APIs are core and are the default
automation surface for agent actors.

```text
GET    /api/v1/orgs/{org_id}/rules
POST   /api/v1/orgs/{org_id}/rules
GET    /api/v1/rules/{id}
PATCH  /api/v1/rules/{id}
POST   /api/v1/rules/{id}/dry-run
POST   /api/v1/rules/{id}/enable
POST   /api/v1/rules/{id}/disable
GET    /api/v1/rules/{id}/executions
POST   /api/v1/rule-executions/{id}/retry
```

Dry-run returns matching event ids and rendered ordinary commands. Enable
requires a version and a successful dry-run against the current rule version.

## 16. Realtime WebSocket

Connection:

```text
GET /api/v1/realtime?cursor=<opaque>
Sec-WebSocket-Protocol: workplane.v1
```

Server frames:

```json
{
  "type": "event",
  "cursor": "opaque",
  "event": {
    "event_id": "evt_...",
    "event_type": "forecast.created",
    "aggregate_type": "project",
    "aggregate_id": "prj_...",
    "aggregate_version": 12,
    "occurred_at": "2026-07-10T13:00:00Z",
    "summary": { "forecast_id": "fc_..." }
  }
}
```

Other frame types: `ready`, `heartbeat`, `permission_changed`, `snapshot_required`,
`rate_limited`, and `error`. Clients send `ack` cursor and optional subscription
filters. Filters reduce traffic but never broaden authorization.

When an actor/token is revoked, the server emits `permission_changed` and closes
the connection after the frame. A reconnect with the revoked credential fails.

## 17. Agent event stream

```text
GET /api/v1/events?cursor=<opaque>&types=review.requested,work_item.assigned
Accept: text/event-stream
```

SSE ids are cursors. `Last-Event-ID` is supported. If the cursor is outside
retention the response is `409 snapshot_required` with query links for current
state.

## 18. Exports and research queries

```text
POST /api/v1/orgs/{org_id}/exports
GET  /api/v1/exports/{id}
GET  /api/v1/projects/{project_id}/research-metrics
GET  /api/v1/portfolios/{portfolio_id}/research-metrics
```

Exports are asynchronous, permission-scoped, checksummed, and expire. Research
queries return documented aggregates and raw event ids used, not undocumented
scores.

## 19. Clean evolution policy

A breaking API/event change updates server, OpenAPI, generated clients,
fixtures, tests, docs, and authoritative stored data in one coordinated change.
Only the latest contract is supported. No deprecated duplicate route, alias
field, old payload reader, dual event writer, version-negotiation branch, or
compatibility shim is retained in any release.

## 20. Public-plane parity manifest

The repository contains a generated manifest with one row per mutating OpenAPI
operation:

```yaml
- operation_id: reforecastProject
  ui_reachable: true
  ui_route: /projects/:id/timeline
  headless: false
- operation_id: applyBriefOperations
  ui_reachable: false
  headless: true
  rationale: Agent-safe equivalent of editor operations; ordinary permissions apply.
```

CI fails for an unlisted operation, a UI mutation without a public operation, or
an internal-only mutation route. Headless does not mean privileged.
