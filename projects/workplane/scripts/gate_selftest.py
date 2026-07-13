#!/usr/bin/env python3
"""Prove gate tiers cannot silently omit an M1 gate or false-green later stages."""

from __future__ import annotations

import tempfile
from pathlib import Path

from evidence import sha256, validate_manifest
from gate import GATES, TIERS, snapshot_extra_artifacts
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
    "m1-upgrade-outbox-backfill",
    "m1-m2-durable-browser-offline",
    "repository-publication",
}


def gate_names(tier: str) -> set[str]:
    return {name for family in TIERS[tier] for name, _ in GATES[family]}


def assert_extra_artifact_snapshot_is_immutable() -> None:
    with tempfile.TemporaryDirectory(prefix="workplane-gate-selftest-") as directory:
        root = Path(directory)
        mutable = root / "target/agent-evidence/m2/replay.json"
        mutable.parent.mkdir(parents=True)
        mutable.write_text("first smoke run\n", encoding="utf-8")
        artifact_root = root / "evidence/runs" / ("a" * 40) / "artifacts"
        snapshots = snapshot_extra_artifacts(
            "fixture",
            artifact_root,
            source_root=root,
            artifact_globs={"fixture": ["target/agent-evidence/m2/*.json"]},
        )
        if len(snapshots) != 1:
            raise AssertionError(f"extra artifact snapshot count={len(snapshots)}, want 1")
        snapshot = root / snapshots[0]["path"]
        manifest = {
            "schema_version": 1,
            "source_commit": "a" * 40,
            "dirty": False,
            "suite": "smoke",
            "environment": {
                "os": "test-os",
                "hardware": "test-hardware",
                "python": "test-python",
                "go": "test-go",
                "node": "test-node",
                "docker": "test-docker",
            },
            "started_at": "2026-07-13T04:00:00Z",
            "finished_at": "2026-07-13T04:00:01Z",
            "summary": {"gates": 1, "command_executions": 1, "passed": 1, "failed": 0},
            "gates": [{
                "name": "fixture",
                "command": "fixture-smoke",
                "started_at": "2026-07-13T04:00:00Z",
                "finished_at": "2026-07-13T04:00:01Z",
                "result": "pass",
                "command_executions": 1,
                "artifacts": snapshots,
            }],
        }
        validate_manifest(manifest, artifact_root=root)
        mutable.write_text("later smoke run\n", encoding="utf-8")
        validate_manifest(manifest, artifact_root=root)
        if snapshot.read_text(encoding="utf-8") != "first smoke run\n" or sha256(snapshot) != snapshots[0]["sha256"]:
            raise AssertionError("later smoke run invalidated a prior exact-head artifact snapshot")


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
    assert_extra_artifact_snapshot_is_immutable()
    print("gate self-test passed: complete smoke, immutable evidence snapshots, and honest staged supersets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
