# ADR 0004: Optional collaboration remains absent from core

- Status: Accepted for M0/M1
- Requirement: M0-ADR-001
- Date: 2026-07-12

## Decision

M0 and M1 contain no CRDT process, endpoint, table, migration, feature flag, or
presence behavior. Core briefs will later use versioned sections with
optimistic locking. A collaboration process may exist only after the M3
dogfood observation and the separately reviewed full-track promotion gate.

## Consequences

Generated schemas and Compose have no dormant collaboration surface. If the
optional track is stopped, core requires no cleanup or compatibility reader.
