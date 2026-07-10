#!/usr/bin/env python3
"""Gate scoped Rust line coverage exported as LCOV by cargo llvm-cov."""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Sequence


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LCOV = ROOT / "target" / "llvm-cov" / "lcov.info"
DEFAULT_THRESHOLD = 90.0
DEFAULT_INCLUDES = (
    "xlite-core/src/functions/",
    "xlite-core/src/eval.rs",
)


@dataclass(frozen=True)
class FileCoverage:
    path: str
    covered_lines: int
    executable_lines: int

    @property
    def percent(self) -> float:
        if self.executable_lines == 0:
            return 0.0
        return (self.covered_lines / self.executable_lines) * 100.0


@dataclass(frozen=True)
class CoverageSummary:
    files: tuple[FileCoverage, ...]

    @property
    def covered_lines(self) -> int:
        return sum(file.covered_lines for file in self.files)

    @property
    def executable_lines(self) -> int:
        return sum(file.executable_lines for file in self.files)

    @property
    def percent(self) -> float:
        if self.executable_lines == 0:
            return 0.0
        return (self.covered_lines / self.executable_lines) * 100.0


def normalize_source_path(raw_path: str, root: Path) -> str:
    source = Path(raw_path.strip())
    if source.is_absolute():
        try:
            return source.resolve().relative_to(root.resolve()).as_posix()
        except ValueError:
            parts = source.parts
            if "xlite-core" in parts:
                return Path(*parts[parts.index("xlite-core") :]).as_posix()
            return source.as_posix()
    return source.as_posix().removeprefix("./")


def normalize_target(raw_target: str) -> str:
    return raw_target.replace("\\", "/").strip().removeprefix("./")


def matches_target(source_path: str, target: str) -> bool:
    if target.endswith("/"):
        return source_path.startswith(target)
    return source_path == target


def parse_lcov(lcov_path: Path, root: Path) -> dict[str, dict[int, int]]:
    files: dict[str, dict[int, int]] = {}
    current_path: str | None = None

    with lcov_path.open(encoding="utf-8") as handle:
        for line_number, raw_line in enumerate(handle, start=1):
            line = raw_line.strip()
            if not line:
                continue
            if line.startswith("SF:"):
                current_path = normalize_source_path(line[3:], root)
                files.setdefault(current_path, {})
                continue
            if line == "end_of_record":
                current_path = None
                continue
            if not line.startswith("DA:"):
                continue
            if current_path is None:
                raise ValueError(f"DA record before SF at {lcov_path}:{line_number}")

            fields = line[3:].split(",")
            if len(fields) < 2:
                raise ValueError(f"malformed DA record at {lcov_path}:{line_number}")
            try:
                source_line = int(fields[0])
                hit_count = int(fields[1])
            except ValueError as exc:
                raise ValueError(f"invalid DA record at {lcov_path}:{line_number}") from exc
            files[current_path][source_line] = files[current_path].get(source_line, 0) + hit_count

    return files


def code_without_strings_or_comments(line: str) -> str:
    code: list[str] = []
    in_string = False
    escaped = False
    index = 0

    while index < len(line):
        char = line[index]
        next_char = line[index + 1] if index + 1 < len(line) else ""

        if not in_string and char == "/" and next_char == "/":
            break
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            code.append(" ")
        elif char == '"':
            in_string = True
            code.append(" ")
        else:
            code.append(char)
        index += 1

    return "".join(code)


def find_braced_block_end(lines: Sequence[str], start_index: int) -> int:
    depth = 0
    opened = False

    for index in range(start_index, len(lines)):
        code = code_without_strings_or_comments(lines[index])
        for char in code:
            if char == "{":
                depth += 1
                opened = True
            elif char == "}":
                depth -= 1
                if opened and depth == 0:
                    return index + 1

    return len(lines)


def cfg_test_line_numbers(source_path: str, root: Path) -> set[int]:
    path = root / source_path
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return set()

    excluded: set[int] = set()
    index = 0
    while index < len(lines):
        if lines[index].strip() != "#[cfg(test)]":
            index += 1
            continue

        module_index = index + 1
        while module_index < len(lines):
            stripped = lines[module_index].strip()
            if not stripped or stripped.startswith("#[") or stripped.startswith("//"):
                module_index += 1
                continue
            break

        if module_index >= len(lines) or "mod tests" not in lines[module_index]:
            index += 1
            continue

        end_line = find_braced_block_end(lines, module_index)
        excluded.update(range(index + 1, end_line + 1))
        index = end_line

    return excluded


def summarize_lcov(
    lcov_path: Path,
    includes: Sequence[str],
    *,
    root: Path = ROOT,
) -> CoverageSummary:
    targets = tuple(normalize_target(target) for target in includes)
    file_lines = parse_lcov(lcov_path, root)
    files: list[FileCoverage] = []

    for source_path, line_counts in sorted(file_lines.items()):
        if not any(matches_target(source_path, target) for target in targets):
            continue
        excluded_lines = cfg_test_line_numbers(source_path, root)
        line_counts = {
            line_number: hit_count
            for line_number, hit_count in line_counts.items()
            if line_number not in excluded_lines
        }
        executable_lines = len(line_counts)
        if executable_lines == 0:
            continue
        covered_lines = sum(1 for hit_count in line_counts.values() if hit_count > 0)
        files.append(
            FileCoverage(
                path=source_path,
                covered_lines=covered_lines,
                executable_lines=executable_lines,
            )
        )

    return CoverageSummary(files=tuple(files))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Fail when scoped Rust LCOV line coverage is below the EL-364 threshold."
    )
    parser.add_argument(
        "--lcov",
        type=Path,
        default=DEFAULT_LCOV,
        help=f"Path to cargo llvm-cov LCOV output. Default: {DEFAULT_LCOV}",
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=DEFAULT_THRESHOLD,
        help=f"Minimum scoped line coverage percentage. Default: {DEFAULT_THRESHOLD}",
    )
    parser.add_argument(
        "--include",
        action="append",
        default=None,
        help=(
            "Source file or slash-terminated source directory to include. "
            "May be repeated; defaults to xlite-core functions and eval."
        ),
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.threshold < 0 or args.threshold > 100:
        parser.error("--threshold must be between 0 and 100")

    includes = tuple(args.include) if args.include is not None else DEFAULT_INCLUDES
    try:
        summary = summarize_lcov(args.lcov, includes)
    except OSError as exc:
        print(f"failed to read LCOV report: {exc}", file=sys.stderr)
        return 2
    except ValueError as exc:
        print(f"failed to parse LCOV report: {exc}", file=sys.stderr)
        return 2

    print("EL-364 scoped Rust coverage")
    print(f"  LCOV: {args.lcov}")
    print(f"  Includes: {', '.join(includes)}")
    print("  Excludes: inline #[cfg(test)] mod tests blocks")
    for file in summary.files:
        print(
            f"  {file.path}: {file.percent:.2f}% "
            f"({file.covered_lines}/{file.executable_lines} lines)"
        )
    print(
        f"  Total: {summary.percent:.2f}% "
        f"({summary.covered_lines}/{summary.executable_lines} lines)"
    )

    if summary.executable_lines == 0:
        print("coverage gate failed: no executable lines matched the configured includes", file=sys.stderr)
        return 1
    if summary.percent + 1e-9 < args.threshold:
        print(
            f"coverage gate failed: {summary.percent:.2f}% is below {args.threshold:.2f}%",
            file=sys.stderr,
        )
        return 1

    print(f"coverage gate passed: {summary.percent:.2f}% >= {args.threshold:.2f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
