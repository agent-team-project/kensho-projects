#!/usr/bin/env python3
"""Evidence-manifest primitives with completeness and clean-source checks."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from jsonschema import Draft202012Validator, FormatChecker


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


def parse_timestamp(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def validate_manifest(manifest: dict[str, Any], artifact_root: Path = ROOT) -> None:
    schema = json.loads((ROOT / "evidence/manifest.schema.json").read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    errors = sorted(
        Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(manifest),
        key=lambda error: [str(part) for part in error.absolute_path],
    )
    if errors:
        details = "; ".join(
            f"{'/'.join(str(part) for part in error.absolute_path) or '<root>'}: {error.message}"
            for error in errors
        )
        raise ValueError(f"evidence manifest schema violation: {details}")

    started = parse_timestamp(manifest["started_at"])
    finished = parse_timestamp(manifest["finished_at"])
    if finished < started:
        raise ValueError("evidence manifest finished_at precedes started_at")

    gates = manifest["gates"]
    for gate in gates:
        if parse_timestamp(gate["finished_at"]) < parse_timestamp(gate["started_at"]):
            raise ValueError(f"evidence gate {gate['name']} finished_at precedes started_at")
        for artifact in gate["artifacts"]:
            path = (artifact_root / artifact["path"]).resolve()
            try:
                path.relative_to(artifact_root.resolve())
            except ValueError as error:
                raise ValueError(f"evidence artifact escapes root: {artifact['path']}") from error
            if not path.is_file():
                raise ValueError(f"evidence artifact is missing: {artifact['path']}")
            if sha256(path) != artifact["sha256"]:
                raise ValueError(f"evidence artifact digest mismatch: {artifact['path']}")

    expected_summary = {
        "gates": len(gates),
        "command_executions": sum(gate["command_executions"] for gate in gates),
        "passed": sum(gate["result"] == "pass" for gate in gates),
        "failed": sum(gate["result"] == "fail" for gate in gates),
    }
    if manifest["summary"] != expected_summary:
        raise ValueError(f"evidence manifest summary mismatch: expected {expected_summary}")


def write_manifest(suite: str, started_at: str, gates: list[dict[str, Any]], commit: str) -> Path:
    manifest = {
        "schema_version": 1,
        "source_commit": commit,
        "dirty": False,
        "suite": suite,
        "environment": environment(),
        "started_at": started_at,
        "finished_at": now(),
        "summary": {
            "gates": len(gates),
            "command_executions": sum(gate["command_executions"] for gate in gates),
            "passed": sum(gate["result"] == "pass" for gate in gates),
            "failed": sum(gate["result"] == "fail" for gate in gates),
        },
        "gates": gates,
    }
    validate_manifest(manifest)
    output = ROOT / "evidence/runs" / commit / f"{suite}.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return output


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate clean-source evidence preconditions.")
    parser.add_argument("suite", choices=("smoke", "acceptance", "release"))
    args = parser.parse_args()
    commit = source_commit()
    print(f"evidence preflight passed: suite={args.suite} source_commit={commit}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
