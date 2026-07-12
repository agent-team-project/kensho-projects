#!/usr/bin/env python3
"""Check M0 publication, pinning, dependency, topology, and non-scope hygiene."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
SECRET = re.compile(r"(?:gh[pousr]_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{24,}|-----BEGIN [A-Z ]*PRIVATE KEY-----)")
EXCLUDED = {".venv", "node_modules", "dist", "__pycache__", "runs"}


def main() -> int:
    errors: list[str] = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or any(part in EXCLUDED for part in path.parts):
            continue
        relative = path.relative_to(ROOT)
        text = path.read_text(encoding="utf-8", errors="ignore")
        if ("/" + "Users/") in text:
            errors.append(f"personal absolute path: {relative}")
        if SECRET.search(text):
            errors.append(f"secret-like value: {relative}")
    package = json.loads((ROOT / "web/package.json").read_text(encoding="utf-8"))
    for section in ("dependencies", "devDependencies"):
        for name, version in package.get(section, {}).items():
            if version.startswith(("^", "~", ">", "<", "*", "workspace:")):
                errors.append(f"unpinned npm dependency {name}={version}")
    compose = (ROOT / "compose.yaml").read_text(encoding="utf-8")
    for line in compose.splitlines():
        if "image:" in line and (":latest" in line or "image:" == line.strip()):
            errors.append(f"unpinned Compose image: {line.strip()}")
    compose_data = yaml.safe_load(compose)
    if set(compose_data.get("services", {})) != {"postgres"}:
        errors.append("M0 Compose must contain only the PostgreSQL service")
    postgres = compose_data.get("services", {}).get("postgres", {})
    if "@sha256:" not in postgres.get("image", ""):
        errors.append("PostgreSQL image must be pinned by immutable digest")
    if compose_data.get("networks", {}).get("default", {}).get("internal") is not True:
        errors.append("M0 runtime network must be internal/offline")
    forbidden_topology = {"state", "jobs", "inbox", "outbox", "daemon", "tokens"}
    for path in (ROOT / ".agent_team").rglob("*"):
        if path.is_file() and forbidden_topology & set(path.relative_to(ROOT / ".agent_team").parts):
            errors.append(f"runtime topology state published: {path.relative_to(ROOT)}")
    if errors:
        print("hygiene failed:\n" + "\n".join(f"- {error}" for error in errors), file=sys.stderr)
        return 1
    print("hygiene passed: exact dependency pins, no credentials/runtime state, scope boundary intact")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
