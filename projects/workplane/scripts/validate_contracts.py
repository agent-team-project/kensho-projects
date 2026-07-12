#!/usr/bin/env python3
"""Fail-closed validation for Workplane's M0 executable registries."""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any

import yaml
from jsonschema import Draft202012Validator
from openapi_spec_validator import validate_spec


ROOT = Path(__file__).resolve().parents[1]
HTTP_METHODS = {"get", "post", "put", "patch", "delete", "head", "options"}
CASE_KEYS = {"id", "requirement", "layer", "title", "given", "when", "then", "evidence", "mutation"}
RUNTIME_DIRS = {"cmd", "internal", "web", "migrations"}
FORBIDDEN_RUNTIME_TERMS = ("yjs", "crdt", "automation_rule", "eca_")


class UniqueKeyLoader(yaml.SafeLoader):
    """YAML loader that rejects duplicate keys instead of last-write-wins."""


def _construct_mapping(loader: UniqueKeyLoader, node: yaml.MappingNode, deep: bool = False) -> dict[Any, Any]:
    mapping: dict[Any, Any] = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise ValueError(f"duplicate YAML key {key!r} at line {key_node.start_mark.line + 1}")
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


UniqueKeyLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_mapping)


class ContractError(Exception):
    pass


def load_yaml(path: Path) -> Any:
    try:
        with path.open(encoding="utf-8") as handle:
            return yaml.load(handle, Loader=UniqueKeyLoader)
    except (OSError, yaml.YAMLError, ValueError) as error:
        raise ContractError(f"{path.relative_to(ROOT)}: {error}") from error


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"{path.relative_to(ROOT)}: {error}") from error


def unique_ids(items: list[dict[str, Any]], field: str, label: str) -> set[str]:
    values = [item.get(field) for item in items]
    missing = [index for index, value in enumerate(values) if not isinstance(value, str) or not value]
    if missing:
        raise ContractError(f"{label}: missing {field} at indexes {missing}")
    duplicates = sorted({value for value in values if values.count(value) > 1})
    if duplicates:
        raise ContractError(f"{label}: duplicate {field}: {', '.join(duplicates)}")
    return set(values)


def operation_key(method: str, path: str) -> str:
    return f"{method.upper()} {path}"


def validate() -> dict[str, int]:
    errors: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            errors.append(message)

    requirements = load_yaml(ROOT / "contracts/requirements.yaml")
    requirement_items = requirements.get("requirements", [])
    requirement_ids = unique_ids(requirement_items, "id", "requirements")
    check(requirements.get("authority_commit") == "40517a9f161432aae31ecca0c194a33a664a9a35", "requirements: wrong authority commit")
    contract_freeze_ids = {item["id"] for item in requirement_items if item.get("begins_at") == "contract-freeze"}
    documented_product_ids = set(re.findall(r"PR-[A-Z]+-[0-9]{2}", (ROOT / "docs/PRODUCT.md").read_text(encoding="utf-8")))
    registered_product_ids = {requirement_id for requirement_id in requirement_ids if requirement_id.startswith("PR-")}
    check(documented_product_ids == registered_product_ids, f"requirements: product registry drift missing={sorted(documented_product_ids - registered_product_ids)} extra={sorted(registered_product_ids - documented_product_ids)}")

    with (ROOT / "PROJECT.toml").open("rb") as handle:
        project_contract = tomllib.load(handle)
    check(project_contract.get("mode") == "exploitation", "PROJECT: M0 kickoff mode must be exploitation")
    check(project_contract.get("delivery", {}).get("max_release_bearing_units_initial") == 1, "PROJECT: Track A release-bearing WIP must remain 1 through M1")

    cases: list[dict[str, Any]] = []
    for path in sorted((ROOT / "tests/cases").glob("*.yaml")):
        case = load_yaml(path)
        extra = set(case) - CASE_KEYS
        check(not extra, f"{path.relative_to(ROOT)}: unknown case keys {sorted(extra)}")
        missing = CASE_KEYS - {"mutation"} - set(case)
        check(not missing, f"{path.relative_to(ROOT)}: missing case keys {sorted(missing)}")
        check(case.get("layer") in {"contract", "unit", "integration", "system", "browser"}, f"{path.relative_to(ROOT)}: invalid layer")
        check(isinstance(case.get("given"), dict) and isinstance(case.get("when"), dict) and isinstance(case.get("then"), dict), f"{path.relative_to(ROOT)}: given/when/then must be mappings")
        check(isinstance(case.get("evidence"), list), f"{path.relative_to(ROOT)}: evidence must be a list")
        cases.append(case)
    case_ids = unique_ids(cases, "id", "acceptance cases")
    unknown_requirements = sorted({case.get("requirement") for case in cases} - requirement_ids)
    check(not unknown_requirements, f"acceptance cases: unknown requirement id(s): {unknown_requirements}")
    covered_requirements = {case.get("requirement") for case in cases}
    check(contract_freeze_ids <= covered_requirements, f"acceptance cases: uncovered contract-freeze requirements {sorted(contract_freeze_ids - covered_requirements)}")
    mutation_ops = {case.get("mutation", {}).get("operation") for case in cases}
    check({"unknown_requirement", "generated_prose_drift"} <= mutation_ops, "acceptance cases: required fail-without-fix mutations are absent")

    lifecycles = load_yaml(ROOT / "contracts/lifecycles.yaml")
    check(lifecycles.get("source_of_truth") is True and lifecycles.get("default") == "deny", "lifecycles: must be deny-default source of truth")
    check(lifecycles.get("generated_views") == ["docs/generated/lifecycles.md"], "lifecycles: generated view path is not canonical")
    date_rules = [rule for rule in lifecycles.get("rules", []) if rule.get("id") == "dates-never-transition"]
    check(len(date_rules) == 1, "lifecycles: dates-never-transition rule must exist exactly once")
    if date_rules:
        check(date_rules[0].get("effect") == "attention_only", "lifecycles: dates may only create attention")
        check({"activate", "complete", "cancel", "approve", "waive"} <= set(date_rules[0].get("forbidden_effects", [])), "lifecycles: date-driven transition forbid-list is incomplete")
    for machine in ("project", "deliverable", "gate", "work_item"):
        states = lifecycles.get(machine, {}).get("states", [])
        check(len(states) == len(set(states)) and states, f"lifecycles: {machine} states must be non-empty and unique")
        for transition in lifecycles.get(machine, {}).get("transitions", []):
            check(transition.get("from") in states and transition.get("to") in states, f"lifecycles: {machine} transition references unknown state")
    check(lifecycles.get("gate", {}).get("hard_gate_waiver") == "forbidden", "lifecycles: hard gate waiver must be forbidden")

    permissions = load_yaml(ROOT / "contracts/permissions.yaml")
    check(permissions.get("source_of_truth") is True and permissions.get("default") == "deny", "permissions: must be deny-default source of truth")
    check(permissions.get("generated_views") == ["docs/generated/permissions.md"], "permissions: generated view path is not canonical")
    actions_by_family = permissions.get("actions", {})
    actions = [action for family in actions_by_family.values() for action in family]
    check(len(actions) == len(set(actions)), "permissions: action strings must be globally unique")
    check(all(re.fullmatch(r"[a-z_]+(?:\.[a-z_]+)+", action) for action in actions), "permissions: action syntax is not closed dotted vocabulary")
    check(set(permissions.get("human_required", [])) <= set(actions), "permissions: human_required references unknown action")
    check(permissions.get("action_syntax", {}).get("qualifiers") == "forbidden", "permissions: action qualifiers must remain forbidden")

    operation_permissions = load_yaml(ROOT / "contracts/operation-permissions.yaml")
    mapped_operations = operation_permissions.get("operations", {})
    mapped_actions = {action for values in mapped_operations.values() for action in values}
    check(mapped_actions <= set(actions), f"operation permissions: unknown actions {sorted(mapped_actions - set(actions))}")
    check(operation_permissions.get("default") == "deny", "operation permissions: default must be deny")

    openapi = load_yaml(ROOT / "contracts/openapi.yaml")
    check(openapi.get("openapi") == "3.1.0", "OpenAPI: version must be 3.1.0")
    try:
        validate_spec(openapi)
    except Exception as error:  # validator exposes version-specific exception types
        errors.append(f"OpenAPI: standards validation failed: {error}")
    operation_ids: list[str] = []
    openapi_operations: dict[str, list[str]] = {}
    for path, path_item in openapi.get("paths", {}).items():
        for method, operation in path_item.items():
            if method not in HTTP_METHODS:
                continue
            key = operation_key(method, path)
            operation_ids.append(operation.get("operationId"))
            declared_actions = operation.get("x-permission-actions", [])
            openapi_operations[key] = declared_actions
            check(key in mapped_operations, f"OpenAPI: operation {key} has no operation-permission entry")
            check(mapped_operations.get(key) == declared_actions, f"OpenAPI: action list for {key} differs from operation-permissions.yaml")
            check(set(declared_actions) <= set(actions), f"OpenAPI: {key} references unknown permission action")
            if method in {"post", "put", "patch", "delete"} and key != "POST /api/v1/session/login":
                serialized = json.dumps(operation.get("parameters", []), sort_keys=True)
                check("IdempotencyKey" in serialized, f"OpenAPI: mutation {key} lacks Idempotency-Key")
            if method == "get":
                check("requestBody" not in operation, f"OpenAPI: GET {path} has a mutation body")
    check(None not in operation_ids and len(operation_ids) == len(set(operation_ids)), "OpenAPI: operationId values must be present and unique")

    event_schema = load_json(ROOT / "contracts/domain-event.schema.json")
    required_event_fields = {"event_id", "event_type", "schema_version", "organization_id", "aggregate_id", "aggregate_version", "request_id", "actor", "occurred_at", "payload"}
    check(required_event_fields <= set(event_schema.get("required", [])), "event schema: envelope required fields are incomplete")
    check(event_schema.get("additionalProperties") is False, "event schema: envelope must reject unknown fields")
    case_schema = load_json(ROOT / "contracts/acceptance-case.schema.json")
    check(case_schema.get("additionalProperties") is False, "case schema: must reject unknown fields")
    try:
        Draft202012Validator.check_schema(event_schema)
        Draft202012Validator.check_schema(case_schema)
        event_validator = Draft202012Validator(event_schema)
        event_errors = sorted(event_validator.iter_errors(load_json(ROOT / "fixtures/domain-event.json")), key=lambda item: list(item.path))
        check(not event_errors, f"event schema: reference event is invalid: {[error.message for error in event_errors]}")
        case_validator = Draft202012Validator(case_schema)
        schema_case_errors = [f"{case.get('id')}: {error.message}" for case in cases for error in case_validator.iter_errors(case)]
        check(not schema_case_errors, f"case schema: invalid cases {schema_case_errors}")
    except Exception as error:
        errors.append(f"JSON Schema: validation failed: {error}")

    parity = load_yaml(ROOT / "contracts/parity.yaml")
    capabilities = parity.get("seed_capabilities", [])
    unique_ids(capabilities, "capability_id", "parity capabilities")
    for capability in capabilities:
        check(capability.get("requirement_id") in requirement_ids, f"parity: unknown requirement {capability.get('requirement_id')}")
        check(capability.get("permission_action") in actions, f"parity: unknown action {capability.get('permission_action')}")

    traceability = load_yaml(ROOT / "contracts/traceability.yaml")
    claim_families = traceability.get("claim_families", [])
    unique_ids(claim_families, "id", "traceability claim families")
    check(all(item.get("authority") and item.get("suites") and item.get("begins_at") for item in claim_families), "traceability: every claim family needs authority, suites, and begins_at")

    staged = load_yaml(ROOT / "contracts/acceptance.yaml")
    freeze_requires = set(staged.get("gates", {}).get("contract-freeze", {}).get("requires", []))
    expected_freeze = {"requirement_registry", "case_registry", "lifecycle_contract", "permission_contract", "operation_permission_contract", "parity_contract", "traceability_contract", "research_digest"}
    check(expected_freeze <= freeze_requires, f"staged acceptance: contract-freeze omissions {sorted(expected_freeze - freeze_requires)}")
    check({"crdt_endpoint", "crdt_process", "eca_endpoint", "eca_table", "eca_feature_flag"} <= set(staged.get("editions", {}).get("v1-core", {}).get("forbidden_dormant_surfaces", [])), "staged acceptance: dormant surface forbid-list incomplete")

    expected_denies = load_yaml(ROOT / "contracts/expected-denies.yaml")
    templates = expected_denies.get("templates", [])
    template_scopes = {template.get("applies_to") for template in templates}
    check(expected_denies.get("coverage") == "every_action_and_openapi_operation", "expected denies: coverage must include actions and operations")
    check({"all_actions", "all_resource_actions", "human_required_actions"} <= template_scopes, "expected denies: coverage templates incomplete")
    check(all(template.get("expected") == "deny" and template.get("rationale") for template in templates), "expected denies: every template needs deny and rationale")
    expanded_denies = load_json(ROOT / "tests/registries/expected-denies.generated.json")
    expanded_actions = {entry.get("action") for entry in expanded_denies.get("actions", [])}
    expanded_operations = {entry.get("operation") for entry in expanded_denies.get("operations", [])}
    check(expanded_actions == set(actions), f"expected denies: expanded action coverage differs: missing={sorted(set(actions) - expanded_actions)} extra={sorted(expanded_actions - set(actions))}")
    check(expanded_operations == set(mapped_operations), f"expected denies: expanded operation coverage differs: missing={sorted(set(mapped_operations) - expanded_operations)} extra={sorted(expanded_operations - set(mapped_operations))}")
    check(all(entry.get("deny_templates") for entry in expanded_denies.get("actions", [])), "expected denies: every action needs an expected-deny case")
    check(all(entry.get("deny_templates") for entry in expanded_denies.get("operations", [])), "expected denies: every operation needs an expected-deny case")

    fixture = load_json(ROOT / "fixtures/reference-organization.json")
    check(fixture.get("schema_version") == 1 and fixture.get("organization", {}).get("id"), "reference fixture: versioned organization is required")
    principals = fixture.get("principals", [])
    unique_ids(principals, "id", "reference fixture principals")
    check({principal.get("kind") for principal in principals} == {"human", "agent"}, "reference fixture: human and agent principals are required")
    catalogs = fixture.get("catalogs", {})
    check(set(catalogs.get("principal_kinds", [])) == set(permissions.get("principals", {}).get("kinds", [])), "reference fixture: principal-kind catalog differs from permissions")
    check(set(catalogs.get("organization_roles", [])) == set(permissions.get("roles", {}).get("organization", [])), "reference fixture: organization-role catalog differs from permissions")
    check(set(catalogs.get("project_roles", [])) == set(permissions.get("roles", {}).get("project", [])), "reference fixture: project-role catalog differs from permissions")
    check(set(catalogs.get("project_modes", [])) == set(lifecycles.get("project", {}).get("modes", [])), "reference fixture: project-mode catalog differs from lifecycles")
    check(set(catalogs.get("project_states", [])) == set(lifecycles.get("project", {}).get("states", [])), "reference fixture: project-state catalog differs from lifecycles")

    required_adrs = {f"000{number}" for number in range(1, 6)}
    present_adrs = {path.name.split("-", 1)[0] for path in (ROOT / "docs/adr").glob("*.md")}
    check(required_adrs <= present_adrs, f"ADRs: missing foundational records {sorted(required_adrs - present_adrs)}")
    seam = re.sub(r"[\s*]+", " ", (ROOT / "docs/walking-slice-m1.md").read_text(encoding="utf-8"))
    for boundary in ("real PostgreSQL", "public HTTP", "generated TypeScript client", "production browser", "authorization", "idempotency", "offline Compose", "agent token"):
        check(boundary in seam, f"walking-slice seam: missing boundary {boundary!r}")

    topology_root = ROOT / ".agent_team"
    topology_files = sorted(path for path in topology_root.rglob("*") if path.is_file())
    check(bool(topology_files), "topology: no checked-in Workplane definitions")
    for path in topology_files:
        relative_parts = path.relative_to(topology_root).parts
        check(not ({"state", "jobs", "inbox", "outbox", "daemon", "tokens"} & set(relative_parts)), f"topology: runtime path published: {path.relative_to(ROOT)}")
        text = path.read_text(encoding="utf-8")
        check(("/" + "Users/") not in text and "token =" not in text.lower(), f"topology: machine path or credential field in {path.relative_to(ROOT)}")

    for directory in RUNTIME_DIRS:
        base = ROOT / directory
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or {"node_modules", "dist"} & set(path.parts):
                continue
            lowered = path.as_posix().lower() + "\n" + path.read_text(encoding="utf-8", errors="ignore").lower()
            for term in FORBIDDEN_RUNTIME_TERMS:
                check(term not in lowered, f"non-scope: dormant runtime surface {term!r} in {path.relative_to(ROOT)}")

    for path in sorted(ROOT.rglob("*.toml")):
        try:
            tomllib.loads(path.read_text(encoding="utf-8"))
        except tomllib.TOMLDecodeError as error:
            errors.append(f"{path.relative_to(ROOT)}: invalid TOML: {error}")

    if errors:
        raise ContractError("\n".join(f"- {error}" for error in errors))

    return {
        "requirements": len(requirement_ids),
        "cases": len(case_ids),
        "actions": len(actions),
        "operations": len(openapi_operations),
        "claim_families": len(claim_families),
    }


def main() -> int:
    try:
        counts = validate()
    except ContractError as error:
        print(f"contract validation failed:\n{error}", file=sys.stderr)
        return 1
    print("contract validation passed: " + ", ".join(f"{key}={value}" for key, value in counts.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
