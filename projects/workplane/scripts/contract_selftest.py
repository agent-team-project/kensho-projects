#!/usr/bin/env python3
"""Execute every registered M0 mutation against the production contract paths."""

from __future__ import annotations

import json
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

import yaml


ROOT = Path(__file__).resolve().parents[1]


def copy_contract_sandbox(parent: Path) -> Path:
    sandbox = parent / "workplane"
    shutil.copytree(
        ROOT,
        sandbox,
        ignore=shutil.ignore_patterns(".venv", "node_modules", "dist", "runs", "__pycache__", "*.pyc"),
    )
    return sandbox


def load_cases() -> list[dict[str, Any]]:
    cases = []
    for path in sorted((ROOT / "tests/cases").glob("*.yaml")):
        case = yaml.safe_load(path.read_text(encoding="utf-8"))
        if case.get("mutation"):
            cases.append(case)
    return cases


def mutate_unknown_requirement(sandbox: Path) -> None:
    path = sandbox / "tests/cases/M0-REGISTRY-001.yaml"
    case = yaml.safe_load(path.read_text(encoding="utf-8"))
    case["requirement"] = "UNKNOWN-999"
    path.write_text(yaml.safe_dump(case, sort_keys=False), encoding="utf-8")


def mutate_duplicate_requirement_id(sandbox: Path) -> None:
    path = sandbox / "contracts/requirements.yaml"
    contract = yaml.safe_load(path.read_text(encoding="utf-8"))
    contract["requirements"][1]["id"] = contract["requirements"][0]["id"]
    path.write_text(yaml.safe_dump(contract, sort_keys=False), encoding="utf-8")


def mutate_generated_prose(sandbox: Path) -> None:
    path = sandbox / "docs/generated/permissions.md"
    path.write_text(path.read_text(encoding="utf-8") + "twin author drift\n", encoding="utf-8")


def mutate_dirty_source(sandbox: Path) -> None:
    subprocess.run(["git", "init", "--quiet"], cwd=sandbox, check=True)
    subprocess.run(["git", "config", "user.name", "Workplane Mutation Runner"], cwd=sandbox, check=True)
    subprocess.run(["git", "config", "user.email", "mutation@workplane.invalid"], cwd=sandbox, check=True)
    subprocess.run(["git", "add", "--all"], cwd=sandbox, check=True)
    subprocess.run(["git", "commit", "--quiet", "-m", "mutation baseline"], cwd=sandbox, check=True)
    path = sandbox / "README.md"
    path.write_text(path.read_text(encoding="utf-8") + "dirty mutation\n", encoding="utf-8")


def mutate_cross_org_denial(sandbox: Path) -> None:
    path = sandbox / "tests/registries/expected-denies.generated.json"
    registry = json.loads(path.read_text(encoding="utf-8"))
    for entry in [*registry["actions"], *registry["operations"]]:
        entry["deny_templates"] = [
            template for template in entry["deny_templates"] if template != "DENY-CROSS-ORG"
        ]
    path.write_text(json.dumps(registry, indent=2, sort_keys=True) + "\n", encoding="utf-8")


MUTATIONS: dict[str, Callable[[Path], None]] = {
    "unknown_requirement": mutate_unknown_requirement,
    "duplicate_requirement_id": mutate_duplicate_requirement_id,
    "generated_prose_drift": mutate_generated_prose,
    "dirty_source": mutate_dirty_source,
    "remove_cross_org_denial": mutate_cross_org_denial,
}


def run_case(case: dict[str, Any]) -> None:
    operation = case["mutation"]["operation"]
    expected = case.get("then", {}).get("error_contains")
    command = case.get("when", {}).get("command")
    if operation not in MUTATIONS:
        raise AssertionError(f"{case['id']}: mutation operation {operation!r} has no runner")
    if not isinstance(command, str) or not command:
        raise AssertionError(f"{case['id']}: registered mutation lacks a production command")
    if case.get("then", {}).get("result") != "fail" or not isinstance(expected, str):
        raise AssertionError(f"{case['id']}: mutation case must assert a failing result and error")
    if expected != case["mutation"].get("expected_error"):
        raise AssertionError(f"{case['id']}: mutation and then expectations differ")

    with tempfile.TemporaryDirectory(prefix=f"workplane-{operation}-") as directory:
        sandbox = copy_contract_sandbox(Path(directory))
        MUTATIONS[operation](sandbox)
        if command.startswith("python3 "):
            command = f"{shlex.quote(sys.executable)} {command.removeprefix('python3 ')}"
        completed = subprocess.run(
            command,
            cwd=sandbox,
            shell=True,
            executable="/bin/sh",
            text=True,
            capture_output=True,
            timeout=120,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            raise AssertionError(f"{case['id']}: production command false-green for {operation}")
        if expected not in output:
            raise AssertionError(f"{case['id']}: expected {expected!r}, got: {output.strip()}")
    print(f"mutation passed: {case['id']} -> {operation}")


def main() -> int:
    cases = load_cases()
    if set(MUTATIONS) != {case["mutation"]["operation"] for case in cases}:
        raise AssertionError("registered mutation cases and production runners differ")
    for case in cases:
        run_case(case)
    print(f"contract mutation self-tests passed: {len(cases)} registered production-path mutations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
