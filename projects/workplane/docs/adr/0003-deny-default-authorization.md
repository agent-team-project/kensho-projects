# ADR 0003: Deny-default server authorization

- Status: Accepted for M0/M1
- Requirement: M0-ADR-001
- Date: 2026-07-12

## Decision

Authorization evaluates the closed action vocabulary in
`contracts/permissions.yaml` and the exact operation map in
`contracts/operation-permissions.yaml`. Evaluation is server-side, deny by
default, organization-bounded, token-scope-restricting, actor-kind aware, and
separation-of-duty aware. UI visibility is never a security boundary.

## Consequences

Every OpenAPI operation declares the same actions as its operation-map row.
Expected-deny templates expand across every action and operation from M0.
Human-required actions cannot be approximated by UI-only buttons.
