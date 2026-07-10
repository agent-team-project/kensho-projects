#!/usr/bin/env python3
"""Validate CI-visible registry and decoupling contracts."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - depends on Python runtime.
    print("Python 3.11+ is required for tomllib", file=sys.stderr)
    sys.exit(2)


ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "SPEC.md"
FUNCTION_ROOT = ROOT / "xlite-core" / "src" / "functions"
CONFORMANCE_ROOT = ROOT / "xlite-core" / "tests" / "conformance"
CORE_CARGO_TOML = ROOT / "xlite-core" / "Cargo.toml"
PACKAGE_JSON = ROOT / "package.json"
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
COVERAGE_SCRIPT = ROOT / "scripts" / "check_rust_coverage.py"
COVERAGE_TEST = ROOT / "scripts" / "test_check_rust_coverage.py"
COVERAGE_LCOV_PATH = "target/llvm-cov/lcov.info"
COVERAGE_TARGETS = (
    "xlite-core/src/functions/",
    "xlite-core/src/eval.rs",
)
EXPECTED_SPEC_FUNCTION_COUNT = 148
SHARED_SKELETON_COVERAGE = {
    "SUM": "walking_skeleton.toml",
    "AVERAGE": "walking_skeleton.toml",
}
EXCLUDED_FUNCTION_FILES = {"mod.rs", "prelude.rs", "registry.rs"}
FUNCTION_NAME_RE = re.compile(
    r"fn\s+name\s*\(\s*&self\s*\)\s*->\s*&\s*'static\s+str\s*\{\s*"
    r'"([A-Z][A-Z0-9.]*)"\s*\}',
    re.S,
)
SPEC_FUNCTION_RE = re.compile(r"\*\*EL-\d+\*\*\s+([^·\n]+)")
FORMULA_FUNCTION_RE = re.compile(r"=\s*([A-Z][A-Z0-9.]*)\s*\(", re.I)

FORBIDDEN_CORE_DEPENDENCIES = {
    "actix-web",
    "axum",
    "hyper",
    "js-sys",
    "reqwest",
    "rocket",
    "surf",
    "tao",
    "tauri",
    "tauri-build",
    "tauri-plugin-dialog",
    "tauri-plugin-http",
    "tauri-plugin-shell",
    "tauri-plugin-updater",
    "tokio-tungstenite",
    "ureq",
    "wasm-bindgen",
    "web-sys",
    "wry",
}
FORBIDDEN_PACKAGE_DEPENDENCIES = {
    "@tauri-apps/plugin-http",
    "@tauri-apps/plugin-shell",
    "@tauri-apps/plugin-updater",
}


class Audit:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.notes: list[str] = []

    def pass_(self, message: str) -> None:
        self.notes.append(f"PASS {message}")

    def fail(self, message: str) -> None:
        self.failures.append(message)
        self.notes.append(f"FAIL {message}")

    def require(self, condition: bool, message: str) -> None:
        if condition:
            self.pass_(message)
        else:
            self.fail(message)


def relative(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def load_toml(path: Path) -> Any:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def parse_spec_functions() -> set[str]:
    text = SPEC.read_text(encoding="utf-8")
    try:
        function_section = text.split("### Epic F", 1)[1].split("### Epic N", 1)[0]
    except IndexError as exc:
        raise ValueError("could not locate SPEC Epic F-M function inventory") from exc

    functions: list[str] = []
    for match in SPEC_FUNCTION_RE.finditer(function_section):
        raw_name = match.group(1).strip()
        name = re.split(r"\s+\(", raw_name, maxsplit=1)[0].strip()
        if re.fullmatch(r"[A-Z][A-Z0-9.]*", name):
            functions.append(name)
    return set(functions)


def registered_functions() -> dict[str, list[Path]]:
    by_name: dict[str, list[Path]] = defaultdict(list)
    for path in sorted(FUNCTION_ROOT.rglob("*.rs")):
        if path.name in EXCLUDED_FUNCTION_FILES:
            continue
        text = path.read_text(encoding="utf-8")
        for match in FUNCTION_NAME_RE.finditer(text):
            by_name[match.group(1)].append(path)
    return by_name


def submitted_function_files() -> set[Path]:
    submitted: set[Path] = set()
    for path in sorted(FUNCTION_ROOT.rglob("*.rs")):
        if path.name in EXCLUDED_FUNCTION_FILES:
            continue
        text = path.read_text(encoding="utf-8")
        if "inventory::submit!" in text and "FunctionEntry(" in text:
            submitted.add(path)
    return submitted


def conformance_name_from_path(path: Path) -> str:
    return path.stem.removeprefix("fn_").upper().replace("_", ".")


def direct_conformance_coverage() -> dict[str, list[Path]]:
    by_name: dict[str, list[Path]] = defaultdict(list)
    for path in sorted(CONFORMANCE_ROOT.glob("fn_*.toml")):
        by_name[conformance_name_from_path(path)].append(path)
    return by_name


def formula_names(value: Any) -> Iterable[str]:
    if isinstance(value, dict):
        for child in value.values():
            yield from formula_names(child)
    elif isinstance(value, list):
        for child in value:
            yield from formula_names(child)
    elif isinstance(value, str):
        for match in FORMULA_FUNCTION_RE.finditer(value):
            yield match.group(1).upper()


def shared_skeleton_functions() -> set[str]:
    covered: set[str] = set()
    for function, filename in SHARED_SKELETON_COVERAGE.items():
        path = CONFORMANCE_ROOT / filename
        if not path.exists():
            continue
        data = load_toml(path)
        if function in set(formula_names(data)):
            covered.add(function)
    return covered


def dependency_keys(value: Any) -> Iterable[str]:
    if isinstance(value, dict):
        for key, child in value.items():
            if key in {"dependencies", "dev-dependencies", "build-dependencies"}:
                if isinstance(child, dict):
                    yield from child.keys()
            yield from dependency_keys(child)
    elif isinstance(value, list):
        for child in value:
            yield from dependency_keys(child)


def tracked_paths(pathspec: str) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", pathspec],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "git ls-files failed")
    return [line for line in result.stdout.splitlines() if line]


def validate_registry(audit: Audit) -> None:
    spec_functions = parse_spec_functions()
    audit.require(
        len(spec_functions) == EXPECTED_SPEC_FUNCTION_COUNT,
        f"REG-COMPLETE parsed {EXPECTED_SPEC_FUNCTION_COUNT} SPEC function names",
    )

    registered_by_name = registered_functions()
    registered_names = set(registered_by_name)
    duplicate_names = {
        name: paths for name, paths in registered_by_name.items() if len(paths) > 1
    }
    audit.require(not duplicate_names, "REG-DUP has no duplicate registered function names")
    for name, paths in sorted(duplicate_names.items()):
        audit.fail(
            f"REG-DUP duplicate function {name}: "
            + ", ".join(relative(path) for path in paths)
        )

    missing_registered = sorted(spec_functions - registered_names)
    extra_registered = sorted(registered_names - spec_functions)
    audit.require(
        not missing_registered and not extra_registered,
        "REG-COMPLETE registered functions match SPEC Epic F-M inventory",
    )
    if missing_registered:
        audit.fail(f"REG-COMPLETE missing registered functions: {missing_registered}")
    if extra_registered:
        audit.fail(f"REG-COMPLETE extra registered functions: {extra_registered}")

    submitted_files = submitted_function_files()
    unsubmitted = sorted(
        path
        for paths in registered_by_name.values()
        for path in paths
        if path not in submitted_files
    )
    audit.require(not unsubmitted, "REG-COMPLETE every function file self-registers")
    for path in unsubmitted:
        audit.fail(f"REG-COMPLETE {relative(path)} lacks inventory self-registration")

    direct_coverage = direct_conformance_coverage()
    duplicate_coverage = {
        name: paths for name, paths in direct_coverage.items() if len(paths) > 1
    }
    audit.require(not duplicate_coverage, "REG-DUP has no duplicate function conformance files")
    for name, paths in sorted(duplicate_coverage.items()):
        audit.fail(
            f"REG-DUP duplicate conformance coverage for {name}: "
            + ", ".join(relative(path) for path in paths)
        )

    shared_coverage = shared_skeleton_functions()
    missing_shared = sorted(set(SHARED_SKELETON_COVERAGE) - shared_coverage)
    audit.require(
        not missing_shared,
        "REG-COMPLETE shared skeleton conformance covers SUM and AVERAGE",
    )
    if missing_shared:
        audit.fail(f"REG-COMPLETE missing shared skeleton coverage: {missing_shared}")

    covered_names = set(direct_coverage) | shared_coverage
    missing_coverage = sorted(spec_functions - covered_names)
    extra_coverage = sorted(set(direct_coverage) - spec_functions)
    audit.require(
        not missing_coverage and not extra_coverage,
        "REG-COMPLETE conformance coverage matches SPEC functions",
    )
    if missing_coverage:
        audit.fail(f"REG-COMPLETE missing conformance coverage: {missing_coverage}")
    if extra_coverage:
        audit.fail(f"REG-COMPLETE extra conformance files: {extra_coverage}")


def validate_decoupling(audit: Audit) -> None:
    core_cargo = load_toml(CORE_CARGO_TOML)
    core_dependencies = {name.lower() for name in dependency_keys(core_cargo)}
    blocked_core = sorted(core_dependencies & FORBIDDEN_CORE_DEPENDENCIES)
    audit.require(
        not blocked_core,
        "DECOUPLE-01 xlite-core has no UI, Tauri, web, or network-service dependencies",
    )
    if blocked_core:
        audit.fail(f"DECOUPLE-01 forbidden xlite-core dependencies: {blocked_core}")

    package_json = json.loads(PACKAGE_JSON.read_text(encoding="utf-8"))
    package_dependencies: set[str] = set()
    for section in ("dependencies", "devDependencies", "optionalDependencies"):
        dependencies = package_json.get(section, {})
        if isinstance(dependencies, dict):
            package_dependencies.update(dependencies)
    blocked_package = sorted(package_dependencies & FORBIDDEN_PACKAGE_DEPENDENCIES)
    audit.require(
        not blocked_package,
        "DECOUPLE-01 package.json has no Tauri http/shell/updater plugins",
    )
    if blocked_package:
        audit.fail(f"DECOUPLE-01 forbidden package dependencies: {blocked_package}")

    tracked_dist = tracked_paths("ui/dist")
    audit.require(
        not tracked_dist,
        "DECOUPLE-01 ui/dist is generated by CI and not checked in",
    )
    if tracked_dist:
        audit.fail(f"DECOUPLE-01 tracked generated ui/dist files: {tracked_dist}")


def validate_coverage_gate(audit: Audit) -> None:
    workflow = CI_WORKFLOW.read_text(encoding="utf-8")
    coverage_script = relative(COVERAGE_SCRIPT)
    coverage_test = relative(COVERAGE_TEST)

    audit.require(COVERAGE_SCRIPT.exists(), "EL-364 coverage checker script exists")
    audit.require(COVERAGE_TEST.exists(), "EL-364 coverage checker tests exist")
    audit.require(
        bool(
            re.search(
                r"CARGO_LLVM_COV_VERSION:\s*[\"']?\d+\.\d+\.\d+[\"']?",
                workflow,
            )
        ),
        "EL-364 CI pins cargo-llvm-cov to an explicit semver version",
    )
    audit.require(
        bool(
            re.search(
                r"XLITE_CORE_COVERAGE_THRESHOLD:\s*[\"']?90(?:\.0+)?[\"']?",
                workflow,
            )
        ),
        "EL-364 CI defines a 90 percent coverage threshold",
    )

    required_snippets = {
        "EL-364 CI compiles the coverage checker": (
            f"python3 -m py_compile {coverage_script} {coverage_test}"
        ),
        "EL-364 CI runs the coverage checker tests": f"python3 -m unittest {coverage_test}",
        "EL-364 CI installs llvm-tools-preview": "llvm-tools-preview",
        "EL-364 CI installs cargo-llvm-cov": "cargo install cargo-llvm-cov --version",
        "EL-364 CI runs cargo llvm-cov for xlite-core": "cargo llvm-cov -p xlite-core",
        "EL-364 CI exports LCOV coverage": "--lcov",
        "EL-364 CI writes the configured LCOV path": COVERAGE_LCOV_PATH,
        "EL-364 CI runs the scoped coverage checker": f"python3 {coverage_script}",
        "EL-364 CI passes the threshold into the checker": (
            '--threshold "$XLITE_CORE_COVERAGE_THRESHOLD"'
        ),
    }
    for message, snippet in required_snippets.items():
        audit.require(snippet in workflow, message)

    for target in COVERAGE_TARGETS:
        audit.require(
            f"--include {target}" in workflow,
            f"EL-364 CI scopes coverage to {target}",
        )


def main() -> int:
    audit = Audit()
    try:
        validate_registry(audit)
        validate_decoupling(audit)
        validate_coverage_gate(audit)
    except Exception as exc:  # pragma: no cover - defensive CLI boundary.
        audit.fail(str(exc))

    print("CI contract validation")
    for note in audit.notes:
        print(f"  {note}")

    if audit.failures:
        print("\nFailures:", file=sys.stderr)
        for failure in audit.failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
