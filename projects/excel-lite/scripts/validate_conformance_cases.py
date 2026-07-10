#!/usr/bin/env python3
"""Validate conformance TOML shape without evaluating spreadsheet formulas."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on Python runtime.
    print("Python 3.11+ is required for tomllib", file=sys.stderr)
    sys.exit(2)


CELL_RE = re.compile(r"^[A-Z]+[1-9][0-9]*$")
EXPECT_KEYS = {"number", "text", "bool", "error", "blank"}


class ValidationError(Exception):
    pass


def fail(path: Path, message: str) -> None:
    raise ValidationError(f"{path}: {message}")


def is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def validate_cell_address(path: Path, case_id: str, field: str, value: Any) -> None:
    if not isinstance(value, str) or not CELL_RE.match(value):
        fail(path, f"{case_id}: {field} must be an A1 address")


def validate_cell_map(path: Path, case_id: str, field: str, value: Any) -> None:
    if not isinstance(value, dict):
        fail(path, f"{case_id}: {field} must be an inline table")
    for cell, contents in value.items():
        validate_cell_address(path, case_id, f"{field} key {cell!r}", cell)
        if not isinstance(contents, (int, float, str, bool)):
            fail(path, f"{case_id}: {field}.{cell} has unsupported value {contents!r}")


def validate_expect(path: Path, case_id: str, field: str, value: Any) -> None:
    if not isinstance(value, dict):
        fail(path, f"{case_id}: {field} must be an inline table")
    keys = EXPECT_KEYS.intersection(value.keys())
    if len(keys) != 1 or len(value) != 1:
        fail(path, f"{case_id}: {field} must contain exactly one of {sorted(EXPECT_KEYS)}")

    key = next(iter(keys))
    expected = value[key]
    if key == "number" and not is_number(expected):
        fail(path, f"{case_id}: {field}.number must be numeric")
    if key in {"text", "error"} and not isinstance(expected, str):
        fail(path, f"{case_id}: {field}.{key} must be a string")
    if key == "bool" and not isinstance(expected, bool):
        fail(path, f"{case_id}: {field}.bool must be a boolean")
    if key == "blank" and expected is not True:
        fail(path, f"{case_id}: {field}.blank must be true")


def validate_formula(path: Path, case_id: str, field: str, value: Any) -> None:
    if not isinstance(value, str) or not value.startswith("="):
        fail(path, f"{case_id}: {field} must be a formula string starting with '='")


def validate_step(path: Path, case_id: str, step: Any, index: int) -> None:
    if not isinstance(step, dict):
        fail(path, f"{case_id}: step {index} must be a table")

    step_id = f"{case_id}/step-{index}"
    validate_cell_map(path, step_id, "set", step.get("set"))
    validate_formula(path, step_id, "formula", step.get("formula"))
    validate_expect(path, step_id, "expect", step.get("expect"))


def validate_case(path: Path, case: Any, seen_ids: dict[str, Path]) -> int:
    if not isinstance(case, dict):
        fail(path, "each [[case]] entry must be a table")

    case_id = case.get("id")
    if not isinstance(case_id, str) or not case_id:
        fail(path, "case id is required")
    if case_id in seen_ids:
        fail(path, f"{case_id}: duplicate id also found in {seen_ids[case_id]}")
    seen_ids[case_id] = path

    if not isinstance(case.get("desc"), str) or not case["desc"]:
        fail(path, f"{case_id}: desc is required")
    if "kind" in case and not isinstance(case["kind"], str):
        fail(path, f"{case_id}: kind must be a string")

    validate_cell_map(path, case_id, "setup", case.get("setup"))
    validate_formula(path, case_id, "formula", case.get("formula"))
    validate_expect(path, case_id, "expect", case.get("expect"))

    if "target" in case:
        validate_cell_address(path, case_id, "target", case["target"])
    if "tol" in case and not is_number(case["tol"]):
        fail(path, f"{case_id}: tol must be numeric")
    if "timeout_ms" in case:
        timeout_ms = case["timeout_ms"]
        if not isinstance(timeout_ms, int) or isinstance(timeout_ms, bool) or timeout_ms <= 0:
            fail(path, f"{case_id}: timeout_ms must be a positive integer")
    if "deviation" in case and not isinstance(case["deviation"], str):
        fail(path, f"{case_id}: deviation must be a string")

    steps = case.get("steps", [])
    if not isinstance(steps, list):
        fail(path, f"{case_id}: steps must be an array of tables")
    if case.get("kind") == "recalc" and not steps:
        fail(path, f"{case_id}: recalc cases require at least one step")
    if case.get("kind") != "recalc" and steps:
        fail(path, f"{case_id}: only recalc cases may define steps")

    for index, step in enumerate(steps, start=1):
        validate_step(path, case_id, step, index)

    return 1


def validate(root: Path) -> int:
    if not root.exists():
        raise ValidationError(f"{root}: conformance directory does not exist")

    seen_ids: dict[str, Path] = {}
    case_count = 0
    files = sorted(root.glob("*.toml"))
    if not files:
        raise ValidationError(f"{root}: no TOML files found")

    for path in files:
        with path.open("rb") as handle:
            data = tomllib.load(handle)
        cases = data.get("case")
        if not isinstance(cases, list) or not cases:
            fail(path, "file must contain at least one [[case]]")
        for case in cases:
            case_count += validate_case(path, case, seen_ids)

    print(f"Validated {case_count} conformance cases across {len(files)} files")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "root",
        nargs="?",
        default="xlite-core/tests/conformance",
        type=Path,
        help="directory containing conformance TOML files",
    )
    args = parser.parse_args()
    try:
        return validate(args.root)
    except ValidationError as exc:
        print(exc, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
