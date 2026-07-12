#!/usr/bin/env python3
"""Evidence-manifest primitives with completeness and clean-source checks."""

from __future__ import annotations

import hashlib
import json
import platform
import subprocess
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]


def now() -> str:
    return datetime.now(UTC).isoformat().replace("+00:00", "Z")


def reject_dirty_status(status: str) -> None:
    if status.strip():
        raise ValueError("source tree is dirty; commit or restore tracked changes before evidence generation")


def source_commit() -> str:
    status = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=normal"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout
    reject_dirty_status(status)
    return subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip()


def sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def environment() -> dict[str, str]:
    return {
        "os": platform.platform(),
        "hardware": platform.machine(),
        "python": platform.python_version(),
        "go": tool_version(["go", "version"]),
        "node": tool_version(["node", "--version"]),
        "docker": tool_version(["docker", "--version"]),
    }


def tool_version(command: list[str]) -> str:
    try:
        return subprocess.run(command, check=True, text=True, capture_output=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def validate_manifest(manifest: dict[str, Any]) -> None:
    required = {"schema_version", "source_commit", "dirty", "suite", "environment", "started_at", "finished_at", "gates"}
    missing = required - set(manifest)
    if missing:
        raise ValueError(f"incomplete evidence manifest; missing {sorted(missing)}")
    if manifest["dirty"] is not False or not manifest["source_commit"]:
        raise ValueError("evidence manifest must identify a clean exact commit")
    if not manifest["gates"]:
        raise ValueError("evidence manifest must contain at least one gate")
    gate_required = {"name", "command", "started_at", "finished_at", "result", "tests", "artifacts"}
    for gate in manifest["gates"]:
        gate_missing = gate_required - set(gate)
        if gate_missing:
            raise ValueError(f"incomplete gate {gate.get('name')}; missing {sorted(gate_missing)}")
        for artifact in gate["artifacts"]:
            if set(artifact) != {"path", "sha256"}:
                raise ValueError(f"incomplete artifact record in gate {gate['name']}")


def write_manifest(suite: str, started_at: str, gates: list[dict[str, Any]], commit: str) -> Path:
    manifest = {
        "schema_version": 1,
        "source_commit": commit,
        "dirty": False,
        "suite": suite,
        "environment": environment(),
        "started_at": started_at,
        "finished_at": now(),
        "gates": gates,
    }
    validate_manifest(manifest)
    output = ROOT / "evidence/runs" / commit / f"{suite}.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return output
