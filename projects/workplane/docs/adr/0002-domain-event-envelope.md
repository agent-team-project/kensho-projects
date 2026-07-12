# ADR 0002: Versioned domain-event envelope

- Status: Accepted for M0/M1
- Requirement: M0-ADR-001
- Date: 2026-07-12

## Decision

Every accepted mutation will emit one attributable event group using
`contracts/domain-event.schema.json`. Envelope schema version, aggregate
version, command/request identity, actor and principal, organization, origin,
causation/correlation, timestamp, and payload remain distinct fields. Event
names use `<aggregate>.<past-tense-action>`.

## Consequences

M1 must write current state and events in one PostgreSQL transaction. Unknown
schema versions stop replay at an exact position. The schema is not a license
to add a second event-derived truth during M0.
