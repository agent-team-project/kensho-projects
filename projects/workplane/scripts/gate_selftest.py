#!/usr/bin/env python3
"""Prove gate tiers cannot silently omit an M0 gate or false-green later stages."""

from __future__ import annotations

from gate import GATES, TIERS
from stage_status import OMISSIONS


REQUIRED_SMOKE_GATES = {
    "contracts",
    "registry-mutations",
    "evidence-manifest-self-test",
    "gate-tier-self-test",
    "generated-zero-diff",
    "generated-drift-mutation",
    "hygiene",
    "go-format",
    "go-test",
    "go-vet",
    "frontend-lint",
    "frontend-typecheck",
    "frontend-unit",
    "frontend-build",
    "compose-config",
    "postgres-migration-fixture",
    "repository-publication",
}


def gate_names(tier: str) -> set[str]:
    return {name for family in TIERS[tier] for name, _ in GATES[family]}


def main() -> int:
    smoke = gate_names("smoke")
    acceptance = gate_names("acceptance")
    release = gate_names("release")
    if smoke != REQUIRED_SMOKE_GATES:
        raise SystemExit(f"smoke gate drift: missing={sorted(REQUIRED_SMOKE_GATES - smoke)} extra={sorted(smoke - REQUIRED_SMOKE_GATES)}")
    if not smoke < acceptance < release:
        raise SystemExit("gate tiers must be strict supersets: smoke < acceptance < release")
    if "acceptance-stage-completeness" not in acceptance or "release-stage-completeness" not in release:
        raise SystemExit("later tiers lack explicit completeness sentinels")
    if not OMISSIONS["acceptance"] or not OMISSIONS["release"]:
        raise SystemExit("later-stage omissions must remain explicit until implemented")
    print("gate self-test passed: complete smoke and honest staged supersets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
