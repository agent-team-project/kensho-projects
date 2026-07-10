#!/usr/bin/env python3
"""Dry-run scaffold for regenerating conformance expectations with LibreOffice."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on Python runtime.
    print("Python 3.11+ is required for tomllib", file=sys.stderr)
    sys.exit(2)


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ROOT = Path("xlite-core/tests/conformance")
DEFAULT_TARGET = "Z1"
SOFFICE_CANDIDATES = ("soffice", "libreoffice")
MACOS_SOFFICE = "/Applications/LibreOffice.app/Contents/MacOS/soffice"


class UserError(Exception):
    """A predictable command-line failure that should not print a traceback."""

    def __init__(self, message: str, exit_code: int = 1) -> None:
        super().__init__(message)
        self.exit_code = exit_code


@dataclass(frozen=True)
class CasePlan:
    path: Path
    case_id: str
    desc: str
    target: str
    step_count: int


@dataclass(frozen=True)
class SOfficeResult:
    executable: str | None
    error: str | None


def relative(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def resolve_root(root: Path) -> Path:
    return root if root.is_absolute() else REPO_ROOT / root


def resolve_soffice(configured: str | None) -> SOfficeResult:
    if configured:
        executable = find_configured_executable(configured)
        if executable:
            return SOfficeResult(executable=executable, error=None)
        return SOfficeResult(
            executable=None,
            error=(
                f"configured LibreOffice executable is not runnable: {configured!r}"
            ),
        )

    for candidate in SOFFICE_CANDIDATES:
        executable = shutil.which(candidate)
        if executable:
            return SOfficeResult(executable=executable, error=None)

    return SOfficeResult(
        executable=None,
        error=(
            "LibreOffice Calc executable not found. Expected `soffice` or "
            "`libreoffice` on PATH."
        ),
    )


def find_configured_executable(value: str) -> str | None:
    if os.sep not in value and (os.altsep is None or os.altsep not in value):
        return shutil.which(value)

    path = Path(value).expanduser()
    if path.is_file() and os.access(path, os.X_OK):
        return str(path)
    return None


def load_case_plans(root: Path, selected_ids: set[str]) -> list[CasePlan]:
    if not root.exists():
        raise UserError(f"{relative(root)} does not exist")
    if not root.is_dir():
        raise UserError(f"{relative(root)} is not a directory")

    files = sorted(root.glob("*.toml"))
    if not files:
        raise UserError(f"{relative(root)} contains no .toml files")

    plans: list[CasePlan] = []
    seen_ids: dict[str, Path] = {}
    for path in files:
        data = load_toml(path)
        cases = data.get("case")
        if not isinstance(cases, list):
            raise UserError(f"{relative(path)} does not contain a [[case]] array")
        for case in cases:
            plan = case_plan(path, case)
            if plan.case_id in seen_ids:
                first = relative(seen_ids[plan.case_id])
                raise UserError(
                    f"{relative(path)} duplicates case id {plan.case_id!r} "
                    f"from {first}"
                )
            seen_ids[plan.case_id] = path
            if selected_ids and plan.case_id not in selected_ids:
                continue
            plans.append(plan)

    if selected_ids:
        matched = {plan.case_id for plan in plans}
        missing = sorted(selected_ids - matched)
        if missing:
            raise UserError(f"no conformance case matched: {', '.join(missing)}")

    return plans


def load_toml(path: Path) -> dict[str, Any]:
    try:
        with path.open("rb") as handle:
            data = tomllib.load(handle)
    except tomllib.TOMLDecodeError as exc:
        raise UserError(f"{relative(path)} is not valid TOML: {exc}") from exc
    if not isinstance(data, dict):
        raise UserError(f"{relative(path)} did not parse to a TOML table")
    return data


def case_plan(path: Path, case: Any) -> CasePlan:
    if not isinstance(case, dict):
        raise UserError(f"{relative(path)} contains a non-table [[case]] entry")

    case_id = case.get("id")
    if not isinstance(case_id, str) or not case_id:
        raise UserError(f"{relative(path)} contains a case without a string id")

    desc = case.get("desc", "")
    if not isinstance(desc, str):
        desc = ""

    target = case.get("target", DEFAULT_TARGET)
    if not isinstance(target, str) or not target:
        raise UserError(f"{relative(path)}:{case_id} has an invalid target")

    steps = case.get("steps", [])
    if not isinstance(steps, list):
        raise UserError(f"{relative(path)}:{case_id} has non-list steps")

    return CasePlan(
        path=path,
        case_id=case_id,
        desc=desc,
        target=target,
        step_count=len(steps),
    )


def print_plan(plans: list[CasePlan], root: Path, soffice: SOfficeResult) -> None:
    files = sorted({plan.path for plan in plans})
    step_count = sum(plan.step_count for plan in plans)
    print("Oracle regeneration scaffold")
    print(f"Conformance root: {relative(root)}")
    print(f"Files selected: {len(files)}")
    print(f"Cases selected: {len(plans)}")
    print(f"Recalc steps selected: {step_count}")
    if soffice.executable:
        print(f"LibreOffice executable: {soffice.executable}")
    else:
        print("LibreOffice executable: not found")

    by_file: dict[Path, int] = {}
    for plan in plans:
        by_file[plan.path] = by_file.get(plan.path, 0) + 1
    for path in files[:8]:
        print(f"  {relative(path)}: {by_file[path]} case(s)")
    if len(files) > 8:
        print(f"  ... {len(files) - 8} more file(s)")

    print("No conformance files were modified.")


def missing_soffice_message(soffice: SOfficeResult) -> str:
    detail = soffice.error or "LibreOffice executable not found."
    return "\n".join(
        [
            detail,
            "Install LibreOffice Calc 24.8+ and ensure `soffice` is on PATH,",
            "or pass --soffice /absolute/path/to/soffice.",
            f"macOS example: --soffice {MACOS_SOFFICE}",
            "No conformance files were modified.",
        ]
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Inventory conformance cases and prepare for LibreOffice-backed "
            "expected-value regeneration."
        ),
        epilog=(
            "Default behavior is a dry run: the script reads "
            "xlite-core/tests/conformance/*.toml, reports the cases that would "
            "be evaluated in LibreOffice Calc, and writes nothing. Future "
            "oracle updates must use --write explicitly. The command performs "
            "no network I/O."
        ),
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=DEFAULT_ROOT,
        help="conformance TOML directory (default: %(default)s)",
    )
    parser.add_argument(
        "--case-id",
        action="append",
        default=[],
        help="limit the plan to one case id; may be passed more than once",
    )
    parser.add_argument(
        "--soffice",
        help=(
            "path or command name for LibreOffice/soffice; defaults to searching "
            "`soffice` then `libreoffice` on PATH"
        ),
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="plan only and do not write files (default)",
    )
    parser.add_argument(
        "--write",
        action="store_true",
        help="allow conformance TOML updates after the oracle adapter is implemented",
    )
    return parser.parse_args(argv)


def run(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.write and args.dry_run:
        raise UserError("--write and --dry-run cannot be used together")

    root = resolve_root(args.root)
    selected_ids = set(args.case_id)
    plans = load_case_plans(root, selected_ids)
    soffice = resolve_soffice(args.soffice)

    print_plan(plans, root, soffice)
    sys.stdout.flush()

    if not soffice.executable:
        raise UserError(missing_soffice_message(soffice), exit_code=2)

    if args.write:
        raise UserError(
            "--write is reserved for the LibreOffice evaluation adapter; "
            "this scaffold verified inputs but did not mutate files.",
            exit_code=3,
        )

    print(
        "Dry run complete. A future --write run must evaluate each case formula "
        f"in its target cell (default {DEFAULT_TARGET}) and update expect tables."
    )
    return 0


def main() -> int:
    try:
        return run(sys.argv[1:])
    except UserError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return exc.exit_code


if __name__ == "__main__":
    raise SystemExit(main())
