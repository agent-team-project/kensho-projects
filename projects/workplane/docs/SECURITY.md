# Security and Permission Contract

## 1. Security objectives

1. An actor can access only organizations, projects, documents, events, search
   results, and realtime messages authorized to that actor.
2. Human and agent actions, including delegated ECA execution, are attributable
   and revocable.
3. Rich content cannot execute script or create an authorization side channel.
4. Retries, concurrency, and automation cannot duplicate privileged effects.
5. Secrets do not enter events, logs, traces, exports, or client bundles.
6. A local self-hosted deployment is secure by default, not only when manually
   hardened.

## 2. Protected assets

- project outcomes, briefs, work, comments, decisions, and evidence;
- membership, role, and visibility policy;
- password hashes, sessions, service tokens, document grants, and signing keys;
- immutable domain and security audit ledgers;
- backups and exports;
- automation definitions and execution context; and
- research data that may expose actor behavior.

## 3. Trust boundaries

- Browser and agent clients are untrusted.
- API server is trusted to authenticate, authorize, validate, and transact.
- PostgreSQL is trusted storage but still protected by least-privilege roles.
- If promoted, the collaboration process is semi-trusted and receives
  document-scoped grants only.
- If promoted, the automation executor is semi-trusted and acts through
  allowlisted commands.
- Rich text, links, comments, evidence, imported backups, and event payloads are
  untrusted input.
- Reverse proxy and local network are not assumed trustworthy.

## 4. Identity model

```text
Actor
  human       -> password/session, interactive
  agent       -> service identity + scoped opaque token
Execution context
  automation  -> restricted agent delegation tied to rule version/event
```

Actors are disabled, never deleted from attribution. Display name changes do
not alter historical actor id.

Every agent identity has a mandatory principal: the human or delegated role
whose authority it exercises. Events and audits record actor and principal.
Changing principal is an administrator decision and does not rewrite history.

### 4.1 Human authentication

- No public registration in v1.
- Organization owner creates invites with single-use, short expiry.
- Passwords require a documented minimum length and are hashed with Argon2id
  using versioned parameters.
- Login responses are constant-shape for unknown/known accounts.
- Progressive rate limits apply by account and source.
- Session ids are random, stored hashed, rotated on login/privilege change, and
  invalidated on password reset or explicit revoke.
- Cookies are `Secure` outside explicit localhost development, `HttpOnly`, and
  `SameSite=Lax` or stricter.
- State-changing browser requests require CSRF token and origin validation.

MFA/passkeys are post-v1 unless promoted by threat review. Their absence must be
documented in release risk.

### 4.2 Agent tokens

- 256-bit random opaque tokens, displayed once, stored as keyed hash.
- Prefix identifies token record without being a secret.
- Claims live server-side: actor, org, scopes, project restrictions, expiry,
  created-by, last-used, and revoked-at.
- Default expiry is 24 hours for short-lived task tokens and 30 days maximum
  for long-lived service tokens; policy may be stricter.
- Token creation requires organization admin plus explicit delegation record.
- Tokens cannot create tokens or widen their own scope.
- Revocation invalidates API requests, SSE, WebSocket, and future document
  grants immediately.
- For work-item and dependency operations, the delegated human's current
  organization and project policy is authoritative. A direct agent project
  membership is optional; when present, its role is an additional narrowing
  cap and can never widen human authority.
- Token action scopes and project restrictions are additional bounds; either
  may reduce authority.
- Project restrictions are a fail-closed allowlist. Zero project ids is
  organization-scoped; one or more ids permits only operations targeting a
  listed existing project and therefore denies organization-level project
  creation.
- A token without an active principal is invalid.

### 4.3 Automation execution context

Automation claims include rule id/version, triggering event id, organization,
and allowed action. They expire after one execution and cannot authenticate to
public transports.

## 5. Roles and actions

### 5.1 Organization roles

- `owner`: all organization actions; at least one required.
- `admin`: membership, service identities, portfolios, policy, exports.
- `member`: create/read according to project policy.
- `observer`: read explicitly visible projects.

### 5.2 Project roles

- `owner`: outcome, mode, forecast, participants, lifecycle.
- `steward`: portfolio priority and cross-project dependency.
- `contributor`: brief, work, evidence, submission.
- `reviewer`: verdicts/findings; cannot mutate implementation evidence.
- `viewer`: read/comment where allowed.

### 5.3 Action vocabulary

Actions are stable strings grouped by resource. The complete checked-in prose
view is generated at `docs/generated/permissions.md` from the canonical
`contracts/permissions.yaml`; CI rejects any generated diff.

Unknown actions deny. Action strings have no colon-qualified variants; each
privileged operation is a closed action of its own. Renamed actions are clean
cuts with all policy fixtures updated; no alias grants remain.

`contracts/operation-permissions.yaml` maps every API operation to these actions
and any contextual parent-resource check. API/contract lint fails when either
side has an unmapped entry.

The machine-readable permission matrix can declare `human_required`. V1 uses it
for token issuance, privileged membership change, hard-risk acceptance,
organization deletion, deliverable waiver, and soft-gate waiver. Hard-gate
waiver is absent from the action vocabulary. This is server policy, not a UI
convention.

## 6. Authorization evaluation

Every command/query evaluates:

1. authenticated and active actor;
2. active organization membership;
3. organization boundary;
4. resource visibility;
5. organization and project roles;
6. token scopes/restrictions for agent actors;
7. action-specific separation-of-duty constraints; and
8. resource state constraints.

Authorization occurs after generic shape validation but before sensitive field
validation that could reveal hidden state. Errors do not disclose resource
existence across boundaries.

### 6.1 Separation of duty

- A reviewer cannot approve a gate for a deliverable it submitted when the gate
  requires independence.
- An ECA execution context cannot issue permission, token, approval, waiver, completion,
  cancellation, or destructive export command.
- An agent cannot widen its own role or token.
- No actor can waive a hard gate.
- Restoring a brief snapshot requires project owner and produces an audit event.

## 7. Database security

- API uses a non-owner database role with no schema DDL.
- Migration role is separate and invoked only by the migration job.
- If deployed, the collaboration role can access only Yjs update/snapshot tables
  and cannot read project/membership tables.
- Backup role is read-only and invoked by the admin CLI.
- Connections require TLS outside localhost-only Compose.
- Queries always bind parameters; dynamic sort/filter values use allowlists.
- Organization id is present in keys/indexes and tested on every repository.
- PostgreSQL row-level security is recommended defense in depth after a focused
  performance and correctness evaluation; it does not replace app policy.

## 8. Rich content and optional collaboration security

Core rich briefs use versioned sections and ordinary authorization/version
checks. Document grants, presence, Yjs updates, and collaboration-specific
controls below apply only after that exploration track is promoted. Their hard
security bar is not reduced by optional staging.

- Tiptap schema allowlists node/mark types and attributes.
- Raw HTML is not accepted or rendered.
- Links allow `https`, `http`, and explicit safe internal schemes; `javascript`,
  `data` except controlled image rendering, and unknown schemes are rejected.
- Images are decoded/re-encoded server-side, size/dimension limited, stripped of
  metadata, and served from a separate immutable path with correct content type.
- Content Security Policy blocks inline script, remote script, object/embed,
  framing, and unexpected connections.
- Document grants are signed, document/actor/org scoped, permission scoped,
  nonce-bearing, and at most five minutes.
- Collaboration connection revalidates revocation on permission events and
  periodic heartbeat.
- Presence contains actor id/display name/color only, never token/session data.
- Yjs updates have per-message and per-minute size limits and are validated by
  applying to an isolated document before persistence.
- Snapshot restore never overwrites structured project state.

## 9. Web security

- Strict CSP with nonce-based app bootstrap if needed.
- CORS disabled by default; explicit origins only when agent/browser deployment
  requires it.
- CSRF protection for cookie-authenticated mutations.
- `X-Content-Type-Options: nosniff`, strict referrer policy, frame ancestors,
  and permissions policy.
- Server-generated error text is rendered as text, not HTML.
- Markdown/rich exports are sanitized independently of in-app rendering.
- Redirects accept only validated relative paths.
- No server-side URL fetch in v1, eliminating a large SSRF surface.

## 10. API abuse resistance

- Body, field, list, batch, query, and pagination limits are explicit in OpenAPI.
- Rate limits distinguish login, token issue, search, export, collaboration, and
  ordinary commands.
- Expensive exports/search/backup use bounded asynchronous jobs.
- Idempotency record binds actor, route, body digest, and organization.
- Optimistic concurrency protects domain updates; database uniqueness protects
  create races.
- WebSocket/SSE subscriptions cap filters, buffered bytes, and unacked events.
- Automation rules cap historical dry-run range, executions per event, and
  recursive causation depth.

## 11. Event, audit, and telemetry hygiene

Domain events contain business data necessary for replay but never credentials,
password hashes, session ids, bearer tokens, CSRF tokens, signing keys, full
brief bodies, or unbounded attachment bytes.

Security audit records include:

- login success/failure category;
- session/token issue/revoke;
- membership/role/visibility change;
- authorization denial category;
- brief grant/restore;
- evidence supersession and policy-valid soft-gate/deliverable waiver;
- automation enable/disable/retry;
- export/backup/restore; and
- organization deletion maintenance.

Audit records are append-only and have a separate retention policy. Logs and
traces redact authorization headers, cookies, query secrets, document bodies,
and evidence content.

## 12. Backup and export security

- Backup/export creation requires explicit action and reason.
- Artifacts are encrypted at rest when leaving the host and checksummed.
- Download links are single-purpose, short-lived, and actor bound.
- Restore validates archive path traversal, symlinks, size, schema, digest, and
  organization identity before mutation.
- Import occurs in staging schema/storage and becomes active only after full
  validation.
- Export manifests record creator, scope, time, and expiry.

## 13. Supply-chain security

- Lockfiles committed for Go, npm, and collaboration dependencies.
- CI runs vulnerability, license, secret, and generated-artifact checks.
- Container bases and tool versions are pinned by digest/version.
- Build produces SBOM and provenance metadata.
- No install script executes unreviewed downloaded code during runtime startup.
- Rich-editor extension list is explicit; no dynamic plugin loading.

## 14. Threat scenarios and required mitigations

| Threat | Required defense |
|---|---|
| Guess opaque id from another org | scoped repository + authorization + optional RLS + 404 |
| Agent token leaked | short expiry, hashed storage, scope, revoke, rate limit, audit |
| Agent widens authority | no token/role actions in agent scope; actor cannot grant self |
| XSS in brief/comment | schema allowlist, sanitization, CSP, safe links, text errors |
| Unauthorized Yjs connection (full) | short document grant, signature, expiry, revocation |
| Search leaks private snippet/count | authorized id join before rank/snippet/facet |
| Realtime leaks event after revoke | permission event, immediate close, auth recheck |
| Automation loops | causation depth, event/rule idempotency, action allowlist, rate cap |
| Duplicate approval retry | idempotency + gate version + independence check transaction |
| Deadline triggers approval | no date-driven privileged action in domain/automation vocabulary |
| Malicious restore archive | staged validation, path/size/digest/schema checks |
| Event/log secret leakage | payload schema review, redaction tests, canary secret corpus |

## 15. Security acceptance gates

### Identity and authorization

- Generated allow/deny matrix covers every actor kind, role, scope, resource
  visibility, action, state, and revocation combination.
- Cross-organization corpus runs against API, WebSocket, SSE, search, brief,
  comments, exports, and backup metadata with zero leak.
- Self-widening tests fail for human and agent actors and every ECA context.
- Separation-of-duty tests fail self-review and unauthorized waiver.

### Web/content

- OWASP-style XSS corpus and product-specific editor nodes produce no executable
  content in editor, activity, search snippets, comments, or exports.
- CSRF and origin tests cover every cookie-authenticated mutation.
- CSP test observes zero blocked legitimate resource and blocks injected script,
  object, frame, and external connection attempts.

### Token/session

- Token entropy/hash/display-once/revoke/expiry tests pass.
- Revocation terminates existing realtime and collaboration access within one
  heartbeat, target under five seconds locally.
- Session fixation, rotation, logout-all, password-reset, and rate-limit tests
  pass.

### Data/recovery

- Backup/restore fuzz corpus cannot escape staging or bypass digest/schema
  checks.
- Event/log/trace canary-secret scan finds zero canary occurrence.
- Dependency audit contains no untriaged critical/high vulnerability at release.

### Independent review

An independent security reviewer reads threat model, permission matrix,
collaboration boundary, API diff, dependency/SBOM report, and dynamic test
evidence. Release requires `SECURITY: APPROVE` with no open critical/high
finding.

## 16. Residual risks expected in v1

- Built-in password authentication without MFA has higher account-takeover risk.
- Single-node deployment is not highly available.
- Organization owners remain trusted administrators.
- Collaborative-editor dependencies add supply-chain and schema complexity.
- Small image evidence expands content-processing surface.

Each release records whether these remain accepted and what evidence would force
promotion of a mitigation.
