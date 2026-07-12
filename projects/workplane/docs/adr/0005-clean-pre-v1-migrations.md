# ADR 0005: Clean pre-v1 migration policy

- Status: Accepted for M0/M1
- Requirement: M0-ADR-001
- Date: 2026-07-12

## Decision

Before v1, contract changes update schema, migrations, fixtures, generated
artifacts, and current stored test data together. Migrations are ordered and
forward-only; a destructive migration requires archive/checksum and restore
evidence. No compatibility alias, dual schema, or indefinite upcaster survives
the migration.

## Consequences

Every released schema is tested from empty PostgreSQL. M0 establishes migration
version 1 and a deterministic reference organization; M1 extends it through
the same serialized owner.
