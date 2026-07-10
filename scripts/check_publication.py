#!/usr/bin/env python3
"""Fail closed on public-repository hygiene and contract syntax."""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path
from urllib.parse import unquote

import yaml


ROOT = Path(__file__).resolve().parents[1]
MAX_PUBLIC_FILE_BYTES = 5 * 1024 * 1024

SKIP_PARTS = {".git", "node_modules", "target"}
FORBIDDEN_PARTS = {
    ".claude",
    "budget",
    "daemon",
    "feedback",
    "inbox",
    "jobs",
    "outbox",
    "outcomes",
    "state",
    "worktrees",
}
TEXT_SUFFIXES = {
    "",
    ".css",
    ".html",
    ".json",
    ".md",
    ".py",
    ".rs",
    ".sh",
    ".toml",
    ".ts",
    ".txt",
    ".yaml",
    ".yml",
}

SECRET_PATTERNS = {
    "AWS access key": re.compile(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    "GitHub token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b"),
    "OpenAI-style secret": re.compile(r"\bsk-[A-Za-z0-9_-]{24,}\b"),
    "Slack token": re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
    "private key": re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
}

MARKDOWN_LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")


def candidate_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        relative = path.relative_to(ROOT)
        if any(part in SKIP_PARTS for part in relative.parts):
            continue
        if path.is_file():
            files.append(path)
    return sorted(files)


def check_paths(files: list[Path], errors: list[str]) -> None:
    nested_git = [path for path in ROOT.rglob(".git") if path != ROOT / ".git"]
    for path in nested_git:
        errors.append(f"nested Git metadata: {path.relative_to(ROOT)}")

    for path in files:
        relative = path.relative_to(ROOT)
        if ".agent_team" in relative.parts:
            agent_team_index = relative.parts.index(".agent_team")
            agent_relative = relative.parts[agent_team_index + 1 :]
            if agent_relative and agent_relative[0] in FORBIDDEN_PARTS:
                errors.append(f"agent runtime state is public: {relative}")
        if path.stat().st_size > MAX_PUBLIC_FILE_BYTES:
            errors.append(f"oversized public file ({path.stat().st_size} bytes): {relative}")


def check_text(path: Path, errors: list[str]) -> None:
    if path.suffix.lower() not in TEXT_SUFFIXES:
        return
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        errors.append(f"text-like file is not UTF-8: {path.relative_to(ROOT)}")
        return

    relative = path.relative_to(ROOT)
    if ("/" + "Users/") in text:
        errors.append(f"personal absolute path in {relative}")
    unsafe_bypass = "--dangerously-bypass-approvals" + "-and-sandbox"
    if unsafe_bypass in text:
        errors.append(f"unsafe Codex sandbox bypass in {relative}")
    for label, pattern in SECRET_PATTERNS.items():
        if pattern.search(text):
            errors.append(f"{label} pattern in {relative}")


def check_structured_files(files: list[Path], errors: list[str]) -> None:
    for path in files:
        relative = path.relative_to(ROOT)
        try:
            if path.suffix == ".toml":
                with path.open("rb") as handle:
                    tomllib.load(handle)
            elif path.suffix in {".yaml", ".yml"}:
                with path.open("r", encoding="utf-8") as handle:
                    yaml.safe_load(handle)
        except (OSError, tomllib.TOMLDecodeError, yaml.YAMLError) as error:
            errors.append(f"invalid structured file {relative}: {error}")


def check_markdown_links(files: list[Path], errors: list[str]) -> None:
    for path in files:
        if path.suffix != ".md":
            continue
        text = path.read_text(encoding="utf-8")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.strip().strip("<>")
            if not target or target.startswith(("#", "http://", "https://", "mailto:")):
                continue
            target = unquote(target.split("#", 1)[0])
            if not target:
                continue
            resolved = (path.parent / target).resolve()
            try:
                resolved.relative_to(ROOT.resolve())
            except ValueError:
                errors.append(f"link escapes repository in {path.relative_to(ROOT)}: {raw_target}")
                continue
            if not resolved.exists():
                errors.append(f"broken relative link in {path.relative_to(ROOT)}: {raw_target}")


def check_required_surface(errors: list[str]) -> None:
    required = [
        "README.md",
        "LICENSE",
        "SECURITY.md",
        "CONTRIBUTING.md",
        "projects/chess-engine/README.md",
        "projects/chess-engine/SPEC.md",
        "projects/excel-lite/README.md",
        "projects/excel-lite/SPEC.md",
        "projects/workplane/README.md",
        "projects/workplane/SPEC.md",
    ]
    for relative in required:
        if not (ROOT / relative).is_file():
            errors.append(f"missing required public file: {relative}")


def main() -> int:
    files = candidate_files()
    errors: list[str] = []
    check_paths(files, errors)
    for path in files:
        check_text(path, errors)
    check_structured_files(files, errors)
    check_markdown_links(files, errors)
    check_required_surface(errors)

    if errors:
        print("Publication check failed:", file=sys.stderr)
        for error in sorted(set(errors)):
            print(f"  - {error}", file=sys.stderr)
        return 1

    print(f"Publication check passed: {len(files)} files, no public-surface violations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
