#!/usr/bin/env python3
"""Audit function conformance coverage and documented deviation pins."""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on Python runtime.
    print("Python 3.11+ is required for tomllib", file=sys.stderr)
    sys.exit(2)


FUNCTION_NAME_RE = re.compile(
    r"fn\s+name\s*\(\s*&self\s*\)\s*->\s*&\s*'static\s+str\s*\{\s*"
    r'"([A-Z0-9.]+)"\s*\}',
    re.DOTALL,
)
SPEC_FUNCTION_RE = re.compile(r"\*\*EL-\d+\*\*\s+([A-Z][A-Z0-9.]*)")
SHARED_COVERAGE_EXCEPTIONS = {"AVERAGE", "SUM"}
MIN_DEDICATED_CASES = 5


@dataclass(frozen=True)
class CaseRef:
    file: str
    case_id: str


@dataclass(frozen=True)
class DeviationPin:
    label: str
    required_case_ids: tuple[str, ...]
    required_deviation: str


REQUIRED_DEVIATION_PINS = (
    DeviationPin("A.1 circular references", ("CYCLE-001",), "A.1"),
    DeviationPin("A.2 precedence: unary minus", ("PREC-001",), "A.2"),
    DeviationPin("A.2 precedence: power associativity", ("PREC-002",), "A.2"),
    DeviationPin("A.3 Excel 1900 phantom leap day", ("DATE-SER-003",), "A.3"),
    DeviationPin("A.4 cross-sheet references", ("XSHEET-001",), "A.4"),
    DeviationPin("A.5 unknown functions", ("NAME-001",), "A.5"),
)


def load_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def load_cases(conformance_dir: Path) -> dict[Path, list[dict[str, Any]]]:
    cases_by_file: dict[Path, list[dict[str, Any]]] = {}
    for path in sorted(conformance_dir.glob("*.toml")):
        data = load_toml(path)
        cases = data.get("case", [])
        if isinstance(cases, list):
            cases_by_file[path] = cases
    return cases_by_file


def spec_functions(spec_path: Path) -> set[str]:
    text = spec_path.read_text()
    start = text.index("### Epic F")
    end = text.index("### Epic N", start)
    return set(SPEC_FUNCTION_RE.findall(text[start:end]))


def registered_functions(
    functions_dir: Path,
) -> tuple[dict[str, Path], list[Path], dict[str, list[Path]]]:
    all_registered: dict[str, list[Path]] = defaultdict(list)
    unregistered_files: list[Path] = []
    for path in sorted(functions_dir.glob("*/*.rs")):
        text = path.read_text()
        match = FUNCTION_NAME_RE.search(text)
        if not match:
            continue
        if "inventory::submit!" not in text:
            unregistered_files.append(path)
            continue
        all_registered[match.group(1)].append(path)
    registered = {name: paths[0] for name, paths in all_registered.items()}
    duplicates = {name: paths for name, paths in all_registered.items() if len(paths) > 1}
    return registered, unregistered_files, duplicates


def fn_file_name(path: Path) -> str:
    name = path.stem.removeprefix("fn_").upper()
    if name == "ERROR_TYPE":
        return "ERROR.TYPE"
    return name


def dedicated_fn_files(conformance_dir: Path) -> dict[str, Path]:
    return {fn_file_name(path): path for path in sorted(conformance_dir.glob("fn_*.toml"))}


def formula_strings(value: Any) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        strings: list[str] = []
        for child in value.values():
            strings.extend(formula_strings(child))
        return strings
    if isinstance(value, list):
        strings = []
        for child in value:
            strings.extend(formula_strings(child))
        return strings
    return []


def shared_case_refs(
    cases_by_file: dict[Path, list[dict[str, Any]]],
    function_name: str,
) -> list[CaseRef]:
    pattern = re.compile(rf"\b{re.escape(function_name)}\s*\(", re.IGNORECASE)
    refs: list[CaseRef] = []
    for path, cases in cases_by_file.items():
        if path.name.startswith("fn_"):
            continue
        for case in cases:
            case_id = str(case.get("id", ""))
            haystack = formula_strings(case.get("formula")) + formula_strings(case.get("setup"))
            haystack.extend(formula_strings(case.get("steps")))
            if case_id.startswith(f"FN-{function_name}-") or any(
                pattern.search(item) for item in haystack
            ):
                refs.append(CaseRef(path.name, case_id))
    return refs


def deviation_cases(
    cases_by_file: dict[Path, list[dict[str, Any]]],
) -> dict[str, tuple[str, str]]:
    by_id: dict[str, tuple[str, str]] = {}
    for path, cases in cases_by_file.items():
        for case in cases:
            case_id = case.get("id")
            deviation = case.get("deviation")
            if isinstance(case_id, str) and isinstance(deviation, str):
                by_id[case_id] = (path.name, deviation)
    return by_id


def grouped_case_counts(
    cases_by_file: dict[Path, list[dict[str, Any]]],
    dedicated_files: dict[str, Path],
) -> dict[str, int]:
    counts: dict[str, int] = {}
    for name, path in dedicated_files.items():
        counts[name] = len(cases_by_file.get(path, []))
    return counts


def append_issue(issues: list[str], condition: bool, message: str) -> None:
    if condition:
        issues.append(message)


def markdown_list(items: list[str]) -> str:
    if not items:
        return "- none"
    return "\n".join(f"- {item}" for item in items)


def render_report(root: Path) -> tuple[str, int]:
    spec = spec_functions(root / "SPEC.md")
    registered, unregistered_files, duplicate_registered = registered_functions(
        root / "xlite-core/src/functions"
    )
    dedicated = dedicated_fn_files(root / "xlite-core/tests/conformance")
    cases_by_file = load_cases(root / "xlite-core/tests/conformance")
    deviations = deviation_cases(cases_by_file)

    registered_names = set(registered)
    dedicated_names = set(dedicated)
    missing_dedicated = sorted(registered_names - dedicated_names)
    unexpected_missing = [
        name for name in missing_dedicated if name not in SHARED_COVERAGE_EXCEPTIONS
    ]
    orphan_dedicated = sorted(dedicated_names - registered_names)
    missing_from_registry = sorted(spec - registered_names)
    extra_registered = sorted(registered_names - spec)

    shared_refs = {
        name: shared_case_refs(cases_by_file, name) for name in SHARED_COVERAGE_EXCEPTIONS
    }
    case_counts = grouped_case_counts(cases_by_file, dedicated)
    below_min = sorted(
        (name, count)
        for name, count in case_counts.items()
        if count < MIN_DEDICATED_CASES
    )

    pin_lines: list[str] = []
    issues: list[str] = []
    for pin in REQUIRED_DEVIATION_PINS:
        present = []
        for case_id in pin.required_case_ids:
            current = deviations.get(case_id)
            if current is None:
                issues.append(f"missing deviation pin case {case_id} ({pin.label})")
                continue
            file_name, deviation = current
            if deviation != pin.required_deviation:
                issues.append(
                    f"{case_id} has deviation {deviation!r}, expected {pin.required_deviation!r}"
                )
                continue
            present.append(f"{case_id} in {file_name}")
        status = "pass" if len(present) == len(pin.required_case_ids) else "fail"
        pin_lines.append(f"{status}: {pin.label} ({', '.join(present) or 'missing'})")

    append_issue(
        issues,
        bool(unregistered_files),
        "function files with Function::name() but no inventory::submit!: "
        + ", ".join(str(path) for path in unregistered_files),
    )
    duplicate_lines = [
        f"{name}: {', '.join(str(path) for path in paths)}"
        for name, paths in sorted(duplicate_registered.items())
    ]
    append_issue(
        issues,
        bool(duplicate_registered),
        "duplicate registered function names: " + "; ".join(duplicate_lines),
    )
    append_issue(
        issues,
        bool(missing_from_registry),
        "SPEC functions missing from registry: " + ", ".join(missing_from_registry),
    )
    append_issue(
        issues,
        bool(extra_registered),
        "registered functions not listed in SPEC Epics F-M: " + ", ".join(extra_registered),
    )
    append_issue(
        issues,
        bool(unexpected_missing),
        "registered functions missing dedicated fn_ coverage: "
        + ", ".join(unexpected_missing),
    )
    append_issue(
        issues,
        bool(orphan_dedicated),
        "fn_ coverage files without registered functions: " + ", ".join(orphan_dedicated),
    )
    for name in sorted(SHARED_COVERAGE_EXCEPTIONS):
        append_issue(
            issues,
            name in missing_dedicated and not shared_refs[name],
            f"shared coverage exception {name} has no shared conformance cases",
        )

    shared_lines = []
    for name in sorted(SHARED_COVERAGE_EXCEPTIONS):
        refs = shared_refs[name]
        grouped: dict[str, list[str]] = defaultdict(list)
        for ref in refs:
            grouped[ref.file].append(ref.case_id)
        parts = [f"{file}: {', '.join(ids)}" for file, ids in sorted(grouped.items())]
        shared_lines.append(f"{name}: {len(refs)} shared cases ({'; '.join(parts)})")

    below_min_lines = [f"{name}: {count}" for name, count in below_min]
    total_cases = sum(len(cases) for cases in cases_by_file.values())
    deviation_case_count = len(deviations)

    report = f"""# Conformance Coverage Audit

## Summary
- SPEC Epics F-M functions: {len(spec)}
- Registered function implementations: {len(registered_names)}
- Dedicated `fn_*.toml` files: {len(dedicated_names)}
- Total conformance TOML files: {len(cases_by_file)}
- Total conformance cases: {total_cases}
- Deviation-tagged cases: {deviation_case_count}

## Dedicated Function Coverage
- Missing dedicated `fn_*.toml` coverage: {', '.join(missing_dedicated) or 'none'}
- Missing dedicated coverage outside shared exceptions: {', '.join(unexpected_missing) or 'none'}
- Orphan dedicated `fn_*.toml` files: {', '.join(orphan_dedicated) or 'none'}
- Duplicate registered function names: {'; '.join(duplicate_lines) or 'none'}
- SPEC functions missing from the registry: {', '.join(missing_from_registry) or 'none'}
- Registered functions outside SPEC Epics F-M: {', '.join(extra_registered) or 'none'}

## Shared Coverage Exceptions
{markdown_list(shared_lines)}

## Appendix A Deviation Pins
{markdown_list(pin_lines)}

## Advisory Case-Count Findings
Dedicated `fn_*.toml` files below the SPEC Section 5.1 target of {MIN_DEDICATED_CASES} cases:
{markdown_list(below_min_lines)}

## Hard Audit Issues
{markdown_list(issues)}
"""
    return report, 1 if issues else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root",
    )
    args = parser.parse_args()
    report, status = render_report(args.root.resolve())
    print(report, end="")
    return status


if __name__ == "__main__":
    raise SystemExit(main())
