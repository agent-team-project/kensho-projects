# ADR 0001: Modular monolith stack

- Status: Accepted for M0/M1
- Requirement: M0-ADR-001
- Date: 2026-07-12

## Decision

Use one Go 1.25.6 API/application process, PostgreSQL 17.5 as the only durable
store, and a React 19/TypeScript web client. The browser consumes the generated
public client. Commands are repository-local `make` targets; runtime requires
no paid service or cloud dependency.

## Consequences

The first product transaction can cross every production-shaped boundary
without prematurely distributing the system. SQLite and in-memory repositories
cannot satisfy integration acceptance. New processes require an explicit ADR
and milestone decision.
