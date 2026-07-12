#!/usr/bin/env python3
"""Prove important M0 validators fail when their inputs are mutated."""

from __future__ import annotations

import copy
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from evidence import reject_dirty_status  # noqa: E402
from validate_contracts import ContractError, unique_ids  # noqa: E402


def expect_failure(label: str, expected: str, function: object) -> None:
    try:
        function()  # type: ignore[operator]
    except (ContractError, ValueError) as error:
        if expected not in str(error):
            raise AssertionError(f"{label}: wrong error: {error}") from error
        return
    raise AssertionError(f"{label}: mutation produced a false green")


def main() -> int:
    requirements = [{"id": "M0-REGISTRY-001"}, {"id": "M0-CONTRACT-001"}]
    duplicate = copy.deepcopy(requirements)
    duplicate[1]["id"] = duplicate[0]["id"]
    expect_failure("duplicate id", "duplicate id", lambda: unique_ids(duplicate, "id", "mutation"))

    known = {item["id"] for item in requirements}

    def unknown_requirement() -> None:
        mutated_reference = "UNKNOWN-999"
        if mutated_reference not in known:
            raise ContractError(f"unknown requirement id: {mutated_reference}")

    expect_failure("unknown requirement", "unknown requirement id", unknown_requirement)
    expect_failure("dirty source", "source tree is dirty", lambda: reject_dirty_status(" M tracked-file"))
    print("contract mutation self-tests passed: duplicate-id, unknown-id, dirty-source")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
