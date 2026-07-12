#!/usr/bin/env python3
"""Negative tests for every required evidence field and constrained value."""

from __future__ import annotations

import copy
import tempfile
from pathlib import Path
from typing import Any, Callable

from evidence import sha256, validate_manifest


TOP_LEVEL_REQUIRED = {
    "schema_version",
    "source_commit",
    "dirty",
    "suite",
    "environment",
    "started_at",
    "finished_at",
    "summary",
    "gates",
}
GATE_REQUIRED = {
    "name",
    "command",
    "started_at",
    "finished_at",
    "result",
    "command_executions",
    "artifacts",
}


def expect_invalid(label: str, manifest: dict[str, Any], root: Path) -> None:
    try:
        validate_manifest(manifest, artifact_root=root)
    except ValueError:
        print(f"evidence negative passed: {label}")
        return
    raise AssertionError(f"evidence negative false green: {label}")


def changed(base: dict[str, Any], mutation: Callable[[dict[str, Any]], None]) -> dict[str, Any]:
    manifest = copy.deepcopy(base)
    mutation(manifest)
    return manifest


def add_overlapping_gate(manifest: dict[str, Any]) -> None:
    overlapping = copy.deepcopy(manifest["gates"][0])
    overlapping.update(
        name="overlapping-gate",
        started_at="2026-07-12T19:00:00.500000Z",
        finished_at="2026-07-12T19:00:01Z",
    )
    manifest["gates"].append(overlapping)
    manifest["summary"] = {"gates": 2, "command_executions": 2, "passed": 2, "failed": 0}


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="workplane-evidence-selftest-") as directory:
        root = Path(directory)
        artifact = root / "gate.log"
        artifact.write_text("one verified command execution\n", encoding="utf-8")
        base: dict[str, Any] = {
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
            "started_at": "2026-07-12T19:00:00Z",
            "finished_at": "2026-07-12T19:00:01Z",
            "summary": {"gates": 1, "command_executions": 1, "passed": 1, "failed": 0},
            "gates": [{
                "name": "self-test",
                "command": "true",
                "started_at": "2026-07-12T19:00:00Z",
                "finished_at": "2026-07-12T19:00:01Z",
                "result": "pass",
                "command_executions": 1,
                "artifacts": [{"path": "gate.log", "sha256": sha256(artifact)}],
            }],
        }
        validate_manifest(base, artifact_root=root)

        for field in sorted(TOP_LEVEL_REQUIRED):
            expect_invalid(f"missing top-level {field}", changed(base, lambda item, field=field: item.pop(field)), root)
        for field in sorted(GATE_REQUIRED):
            expect_invalid(f"missing gate {field}", changed(base, lambda item, field=field: item["gates"][0].pop(field)), root)

        mutations: list[tuple[str, Callable[[dict[str, Any]], None]]] = [
            ("invalid commit", lambda item: item.update(source_commit="not-a-commit")),
            ("dirty source", lambda item: item.update(dirty=True)),
            ("invalid suite", lambda item: item.update(suite="banana")),
            ("empty environment", lambda item: item.update(environment={})),
            ("invalid timestamp", lambda item: item.update(started_at="yesterday")),
            ("reversed manifest timestamps", lambda item: item.update(finished_at="2026-07-12T18:59:59Z")),
            ("gate outside suite interval", lambda item: item["gates"][0].update(
                started_at="2026-07-12T20:00:00Z",
                finished_at="2026-07-12T20:00:01Z",
            )),
            ("overlapping sequential gates", add_overlapping_gate),
            ("invalid result", lambda item: item["gates"][0].update(result="banana")),
            ("invalid command count", lambda item: item["gates"][0].update(command_executions=-9)),
            ("empty artifacts", lambda item: item["gates"][0].update(artifacts=[])),
            ("invalid artifact digest", lambda item: item["gates"][0]["artifacts"][0].update(sha256="bad")),
            ("artifact digest mismatch", lambda item: item["gates"][0]["artifacts"][0].update(sha256="0" * 64)),
            ("summary mismatch", lambda item: item["summary"].update(passed=0)),
        ]
        for label, mutation in mutations:
            expect_invalid(label, changed(base, mutation), root)

    print("evidence manifest self-test passed: schema, semantics, counts, and artifacts fail closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
