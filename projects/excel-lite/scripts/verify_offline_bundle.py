#!/usr/bin/env python3
"""Verify the macOS package configuration stays self-contained/offline."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from ipaddress import ip_address
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
TAURI_CONFIG = ROOT / "xlite-app" / "tauri.conf.json"
CAPABILITIES = ROOT / "xlite-app" / "capabilities" / "default.json"
PACKAGE_JSON = ROOT / "package.json"
APP_CARGO_TOML = ROOT / "xlite-app" / "Cargo.toml"
SAMPLE_WORKBOOK = ROOT / "examples" / "first-run-sample.xlite"
SAMPLE_RESOURCE = "../examples/first-run-sample.xlite"
EXPECTED_CAPABILITY_PERMISSIONS = ["core:default", "dialog:allow-open", "dialog:allow-save"]

REMOTE_URL_RE = re.compile(r"https?://[^\s\"'<>`)]+")
TEXT_SUFFIXES = {
    ".css",
    ".html",
    ".js",
    ".json",
    ".plist",
    ".svg",
    ".toml",
    ".txt",
    ".xml",
}
MAX_TEXT_SCAN_BYTES = 2_000_000

ALLOWED_METADATA_URLS = {
    "https://schema.tauri.app/config/2",
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd",
    "http://www.w3.org/1999/xlink",
    "http://www.w3.org/1999/xhtml",
    "http://www.w3.org/2000/svg",
}
ALLOWED_DIAGNOSTIC_URL_PREFIXES = ("https://svelte.dev/e/",)

TAURI_NETWORK_PLUGINS = {
    "@tauri-apps/plugin-http",
    "@tauri-apps/plugin-shell",
    "@tauri-apps/plugin-updater",
    "tauri-plugin-http",
    "tauri-plugin-shell",
    "tauri-plugin-updater",
}


class Verifier:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.notes: list[str] = []

    def pass_(self, message: str) -> None:
        self.notes.append(f"PASS {message}")

    def skip(self, message: str) -> None:
        self.notes.append(f"SKIP {message}")

    def fail(self, message: str) -> None:
        self.failures.append(message)
        self.notes.append(f"FAIL {message}")

    def require(self, condition: bool, message: str) -> None:
        if condition:
            self.pass_(message)
        else:
            self.fail(message)


def load_json(path: Path, verifier: Verifier) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        verifier.fail(f"{relative(path)} is missing")
    except json.JSONDecodeError as error:
        verifier.fail(f"{relative(path)} is not valid JSON: {error}")
    return None


def relative(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def json_walk(value: Any, path: tuple[str, ...] = ()) -> Iterable[tuple[tuple[str, ...], Any]]:
    yield path, value
    if isinstance(value, dict):
        for key, child in value.items():
            yield from json_walk(child, (*path, str(key)))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from json_walk(child, (*path, str(index)))


def path_label(path: tuple[str, ...]) -> str:
    return ".".join(path) if path else "<root>"


def find_urls(text: str) -> Iterable[str]:
    for match in REMOTE_URL_RE.finditer(text):
        yield match.group(0).rstrip(".,;]")


def is_loopback_url(url: str) -> bool:
    parsed = urlparse(url)
    host = parsed.hostname
    if host is None:
        return False
    if host == "localhost":
        return True
    try:
        return ip_address(host).is_loopback
    except ValueError:
        return False


def allowed_config_url(path: tuple[str, ...], url: str) -> bool:
    if path == ("$schema",) and url in ALLOWED_METADATA_URLS:
        return True
    return path == ("build", "devUrl") and is_loopback_url(url)


def scan_json_urls(
    verifier: Verifier,
    source: Path,
    value: Any,
    *,
    allow_config_dev_url: bool = False,
) -> None:
    for path, child in json_walk(value):
        if not isinstance(child, str):
            continue
        for url in find_urls(child):
            if url in ALLOWED_METADATA_URLS:
                continue
            if allow_config_dev_url and allowed_config_url(path, url):
                continue
            verifier.fail(f"{relative(source)}:{path_label(path)} references remote URL {url}")


def is_allowed_static_metadata_url(url: str) -> bool:
    return url in ALLOWED_METADATA_URLS or any(
        url.startswith(prefix) for prefix in ALLOWED_DIAGNOSTIC_URL_PREFIXES
    )


def scan_text_urls(verifier: Verifier, source: Path, text: str) -> None:
    for url in find_urls(text):
        if is_allowed_static_metadata_url(url):
            continue
        verifier.fail(f"{relative(source)} references remote URL {url}")


def verify_tauri_config(verifier: Verifier) -> dict[str, Any] | None:
    config = load_json(TAURI_CONFIG, verifier)
    if not isinstance(config, dict):
        return None

    scan_json_urls(verifier, TAURI_CONFIG, config, allow_config_dev_url=True)

    build = config.get("build")
    bundle = config.get("bundle")
    verifier.require(isinstance(build, dict), "Tauri build config is present")
    verifier.require(isinstance(bundle, dict), "Tauri bundle config is present")
    if not isinstance(build, dict) or not isinstance(bundle, dict):
        return config

    verifier.require(bundle.get("active") is True, "Tauri bundle.active is true")

    targets = bundle.get("targets")
    target_set = set(targets) if isinstance(targets, list) else {targets}
    verifier.require(
        target_set == {"app", "dmg"},
        "Tauri bundle targets are macOS-focused app/dmg outputs",
    )

    resources = bundle.get("resources")
    verifier.require(
        isinstance(resources, list) and SAMPLE_RESOURCE in resources,
        f"Tauri bundle resources include {SAMPLE_RESOURCE}",
    )

    dev_url = build.get("devUrl")
    verifier.require(
        isinstance(dev_url, str) and is_loopback_url(dev_url),
        "Tauri devUrl is loopback-only for local development",
    )

    frontend_dist = build.get("frontendDist")
    verifier.require(
        isinstance(frontend_dist, str)
        and not frontend_dist.startswith(("http://", "https://")),
        "Tauri frontendDist points at local build output",
    )

    for path, value in json_walk(config):
        if not any("updater" in part.lower() for part in path):
            continue
        if value not in (False, None, "", [], {}):
            verifier.fail(f"{relative(TAURI_CONFIG)}:{path_label(path)} enables updater behavior")

    verifier.pass_("Tauri config has no enabled updater settings")
    return config


def verify_capabilities(verifier: Verifier) -> None:
    capabilities = load_json(CAPABILITIES, verifier)
    if not isinstance(capabilities, dict):
        return

    scan_json_urls(verifier, CAPABILITIES, capabilities)
    permissions = capabilities.get("permissions")
    verifier.require(
        permissions == EXPECTED_CAPABILITY_PERMISSIONS,
        "default capability permissions remain minimal: core plus dialog open/save only",
    )


def verify_dependencies(verifier: Verifier) -> None:
    package_json = load_json(PACKAGE_JSON, verifier)
    if isinstance(package_json, dict):
        dependency_names: set[str] = set()
        for section in ("dependencies", "devDependencies", "optionalDependencies"):
            dependencies = package_json.get(section, {})
            if isinstance(dependencies, dict):
                dependency_names.update(dependencies)
        blocked = sorted(dependency_names & TAURI_NETWORK_PLUGINS)
        verifier.require(
            not blocked,
            "package.json does not add Tauri updater/http/shell plugins",
        )
        if blocked:
            verifier.fail(f"package.json includes network-capable Tauri plugins: {blocked}")

    cargo_text = APP_CARGO_TOML.read_text(encoding="utf-8")
    blocked_cargo = sorted(plugin for plugin in TAURI_NETWORK_PLUGINS if plugin in cargo_text)
    verifier.require(
        not blocked_cargo,
        "xlite-app/Cargo.toml does not add Tauri updater/http/shell plugins",
    )
    if blocked_cargo:
        verifier.fail(f"xlite-app/Cargo.toml includes network-capable plugins: {blocked_cargo}")


def verify_sample_workbook(verifier: Verifier) -> None:
    sample = load_json(SAMPLE_WORKBOOK, verifier)
    if not isinstance(sample, dict):
        return

    scan_json_urls(verifier, SAMPLE_WORKBOOK, sample)

    verifier.require(sample.get("format_version") == 1, "sample workbook format_version is 1")
    verifier.require(sample.get("app") == "excel-lite", "sample workbook app marker is excel-lite")
    verifier.require(sample.get("date_system") == "1900", "sample workbook uses Excel 1900 dates")

    sheets = sample.get("sheets")
    verifier.require(isinstance(sheets, list) and len(sheets) == 1, "sample workbook has one sheet")
    if not isinstance(sheets, list) or not sheets:
        return

    sheet = sheets[0]
    cells = sheet.get("cells") if isinstance(sheet, dict) else None
    verifier.require(isinstance(cells, dict) and len(cells) > 0, "sample workbook has cells")
    if not isinstance(cells, dict):
        return

    expected = {
        "A1": "Item",
        "D2": "=B2*C2",
        "D3": "=B3*C3",
        "D5": "=SUM(D2:D3)",
    }
    for address, raw in expected.items():
        cell = cells.get(address)
        verifier.require(
            isinstance(cell, dict) and cell.get("raw") == raw,
            f"sample workbook includes {address} = {raw}",
        )


def scan_tree_for_remote_urls(verifier: Verifier, root: Path, label: str) -> None:
    if not root.exists():
        verifier.skip(f"{label} not present; build artifacts will be checked when available")
        return

    scanned = 0
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        if path.suffix.lower() not in TEXT_SUFFIXES:
            continue
        if path.stat().st_size > MAX_TEXT_SCAN_BYTES:
            verifier.skip(f"{relative(path)} too large for text URL scan")
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            verifier.skip(f"{relative(path)} is not UTF-8 text")
            continue
        scanned += 1
        scan_text_urls(verifier, path, text)
    verifier.pass_(f"{label} text URL scan completed ({scanned} files)")


def built_app_bundles() -> list[Path]:
    roots = [ROOT / "target", ROOT / "xlite-app" / "target"]
    apps: list[Path] = []
    for root in roots:
        if root.exists():
            apps.extend(root.glob("**/bundle/macos/*.app"))
    return sorted(set(apps))


def verify_bundled_resources(verifier: Verifier) -> None:
    apps = built_app_bundles()
    if not apps:
        verifier.skip("no built .app bundle found; resource presence will be checked after tauri:build")
        return

    sample_name = SAMPLE_WORKBOOK.name
    for app in apps:
        resources_dir = app / "Contents" / "Resources"
        bundled_samples = list(resources_dir.rglob(sample_name)) if resources_dir.exists() else []
        verifier.require(
            bool(bundled_samples),
            f"{relative(app)} contains bundled {sample_name} resource",
        )
        scan_tree_for_remote_urls(verifier, app, f"{relative(app)} bundle")


def verify_no_tracked_ui_dist(verifier: Verifier) -> None:
    result = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", "ui/dist"],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        verifier.skip("git metadata unavailable; tracked ui/dist check skipped")
        return

    tracked = [line for line in result.stdout.splitlines() if line]
    verifier.require(
        not tracked,
        "ui/dist is generated by npm run build and is not checked in",
    )
    if tracked:
        verifier.fail(f"tracked generated ui/dist files found: {tracked}")


def main() -> int:
    verifier = Verifier()
    verify_tauri_config(verifier)
    verify_capabilities(verifier)
    verify_dependencies(verifier)
    verify_sample_workbook(verifier)
    verify_no_tracked_ui_dist(verifier)
    scan_tree_for_remote_urls(verifier, ROOT / "ui" / "dist", "ui/dist")
    verify_bundled_resources(verifier)

    print("Offline bundle verification")
    for note in verifier.notes:
        print(f"  {note}")

    if verifier.failures:
        print("\nFailures:", file=sys.stderr)
        for failure in verifier.failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
