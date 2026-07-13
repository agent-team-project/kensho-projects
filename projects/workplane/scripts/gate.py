#!/usr/bin/env python3
"""Run stable Workplane gate tiers and optionally write exact-commit evidence."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

from evidence import now, sha256, source_commit, write_manifest


ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[1]
PYTHON = ".venv/bin/python3"

GATES: dict[str, list[tuple[str, str]]] = {
    "contract": [
        ("contracts", f"{PYTHON} scripts/validate_contracts.py"),
        ("registry-mutations", f"{PYTHON} scripts/contract_selftest.py"),
        ("evidence-manifest-self-test", f"{PYTHON} scripts/evidence_selftest.py"),
        ("gate-tier-self-test", f"{PYTHON} scripts/gate_selftest.py"),
        ("generated-zero-diff", f"{PYTHON} scripts/generate.py --check"),
        ("generated-drift-mutation", f"{PYTHON} scripts/generate.py --self-test"),
        ("hygiene", f"{PYTHON} scripts/check_hygiene.py"),
    ],
    "unit": [
        ("go-format", "test -z \"$(gofmt -l cmd internal)\""),
        ("go-test", "go test ./cmd/... ./internal/..."),
        ("go-vet", "go vet ./cmd/... ./internal/..."),
        ("frontend-lint", "npm --prefix web run lint"),
        ("frontend-typecheck", "npm --prefix web run typecheck"),
        ("frontend-unit", "npm --prefix web test"),
        ("frontend-build", "npm --prefix web run build"),
    ],
    "integration": [
        ("compose-config", "docker compose config --quiet"),
        ("postgres-migration-fixture", "scripts/test_postgres.sh"),
        ("m1-upgrade-outbox-backfill", "scripts/test_m1_upgrade.sh"),
        ("m1-m2-durable-browser-offline", "scripts/test_m1.sh"),
        ("m2b-realtime-authority-resume-faults", "scripts/test_realtime.sh"),
        ("m2c-planning-m2d-evidence-review", "scripts/test_planning.sh"),
    ],
    "publication": [
        ("repository-publication", f"{PYTHON} ../../scripts/check_publication.py"),
    ],
    "acceptance-status": [
        ("acceptance-stage-completeness", f"{PYTHON} scripts/stage_status.py acceptance"),
    ],
    "release-status": [
        ("release-stage-completeness", f"{PYTHON} scripts/stage_status.py release"),
    ],
}

EXTRA_ARTIFACT_GLOBS = {
    "m1-m2-durable-browser-offline": [
        "target/agent-evidence/m1/migration.log",
        "target/agent-evidence/m1/event-rows.txt",
        "target/agent-evidence/m2/*.json",
        "target/agent-evidence/m2/migration.log",
    ],
    "m2b-realtime-authority-resume-faults": [
        "target/agent-evidence/m2b/*.json",
        "target/agent-evidence/m2b/*.txt",
        "target/agent-evidence/m2b/migration.log",
    ],
    "m2c-planning-m2d-evidence-review": [
        "target/agent-evidence/m2c/*.json",
        "target/agent-evidence/m2c/migration.log",
    ],
}

TIERS = {
    "smoke": ["contract", "unit", "integration", "publication"],
    "acceptance": ["contract", "unit", "integration", "publication", "acceptance-status"],
    "release": ["contract", "unit", "integration", "publication", "acceptance-status", "release-status"],
}


def snapshot_extra_artifacts(
    name: str,
    artifact_root: Path,
    *,
    source_root: Path = ROOT,
    artifact_globs: dict[str, list[str]] = EXTRA_ARTIFACT_GLOBS,
) -> list[dict[str, str]]:
    snapshots = []
    for pattern in artifact_globs.get(name, []):
        for artifact in sorted(source_root.glob(pattern)):
            if not artifact.is_file():
                continue
            relative = artifact.relative_to(source_root)
            snapshot = artifact_root / "extra" / name / relative
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(artifact, snapshot)
            snapshots.append({"path": str(snapshot.relative_to(source_root)), "sha256": sha256(snapshot)})
    return snapshots


def run(suite: str, evidence: bool) -> int:
    commit = source_commit() if evidence else ""
    started = now()
    records = []
    artifact_root = ROOT / "evidence/runs" / (commit or "local") / "artifacts"
    artifact_root.mkdir(parents=True, exist_ok=True)
    for family in TIERS[suite]:
        for name, command in GATES[family]:
            gate_started = now()
            print(f"[{suite}] {name}: {command}", flush=True)
            completed = subprocess.run(command, cwd=ROOT, shell=True, executable="/bin/sh", text=True, capture_output=True)
            log = artifact_root / f"{name}.log"
            log.write_text(completed.stdout + completed.stderr, encoding="utf-8")
            sys.stdout.write(completed.stdout)
            sys.stderr.write(completed.stderr)
            artifacts = [{"path": str(log.relative_to(ROOT)), "sha256": sha256(log)}]
            artifacts.extend(snapshot_extra_artifacts(name, artifact_root))
            records.append({
                "name": name,
                "command": command,
                "started_at": gate_started,
                "finished_at": now(),
                "result": "pass" if completed.returncode == 0 else "fail",
                "command_executions": 1,
                "artifacts": artifacts,
            })
            if completed.returncode:
                if evidence:
                    path = write_manifest(suite, started, records, commit)
                    print(f"failing evidence manifest: {path.relative_to(ROOT)}")
                print(f"{suite} failed at {name}; log: {log.relative_to(ROOT)}", file=sys.stderr)
                return completed.returncode
    if evidence:
        path = write_manifest(suite, started, records, commit)
        print(f"evidence manifest: {path.relative_to(ROOT)}")
    print(f"{suite} passed: {len(records)} gates")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("suite", choices=sorted(TIERS))
    parser.add_argument("--evidence", action="store_true")
    args = parser.parse_args()
    return run(args.suite, args.evidence)


if __name__ == "__main__":
    raise SystemExit(main())
