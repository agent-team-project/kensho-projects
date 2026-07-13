#!/usr/bin/env python3
"""Fail-closed validation for Workplane's M0 executable registries."""

from __future__ import annotations

import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any

import yaml
from jsonschema import Draft202012Validator, FormatChecker
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


def file_digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


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
    check(project_contract.get("delivery", {}).get("max_recursive_depth") == 0, "PROJECT: recursive worker depth must remain zero through M1")

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
    cases_by_id = {case["id"]: case for case in cases}
    unknown_requirements = sorted({case.get("requirement") for case in cases} - requirement_ids)
    check(not unknown_requirements, f"acceptance cases: unknown requirement id(s): {unknown_requirements}")
    covered_requirements = {case.get("requirement") for case in cases}
    check(contract_freeze_ids <= covered_requirements, f"acceptance cases: uncovered contract-freeze requirements {sorted(contract_freeze_ids - covered_requirements)}")
    mutation_ops = {case.get("mutation", {}).get("operation") for case in cases if case.get("mutation")}
    required_mutations = {
        "unknown_requirement",
        "duplicate_requirement_id",
        "generated_prose_drift",
        "dirty_source",
        "remove_cross_org_denial",
    }
    check(required_mutations == mutation_ops, f"acceptance cases: fail-without-fix mutation drift missing={sorted(required_mutations - mutation_ops)} extra={sorted(mutation_ops - required_mutations)}")
    bootstrap_case = cases_by_id.get("M0-SESSION-BOOTSTRAP-001", {})
    check(
        bootstrap_case.get("when", {}).get("command")
        == "go test ./internal/api/generated -run TestHumanSessionBootstrap -count=1",
        "acceptance cases: human session bootstrap must execute the generated Go adapter test",
    )

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
    lifecycle_prose = (ROOT / "docs/generated/lifecycles.md").read_text(encoding="utf-8")
    check("| active | promote | active | exploration | exploitation | all | promotion_contract |" in lifecycle_prose, "generated lifecycles: promotion mode_to semantics are missing")
    check("| pending | waive | waived | all | unchanged | soft | human_soft_gate_waiver_contract |" in lifecycle_prose, "generated lifecycles: pending soft-waiver kind is missing")
    check("| failed | waive | waived | all | unchanged | soft | human_soft_gate_waiver_contract |" in lifecycle_prose, "generated lifecycles: failed soft-waiver kind is missing")
    check("Hard gate waiver: **forbidden**." in lifecycle_prose, "generated lifecycles: hard-waiver prohibition is missing")

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
    security_schemes = openapi.get("components", {}).get("securitySchemes", {})
    check(set(security_schemes) == {"HumanSession", "CsrfToken", "AgentBearer"}, "OpenAPI: human cookie/CSRF and agent bearer schemes must be exact")
    check(security_schemes.get("HumanSession", {}).get("in") == "cookie" and security_schemes.get("HumanSession", {}).get("name") == "workplane_session", "OpenAPI: human session must be a named cookie contract")
    check(security_schemes.get("CsrfToken", {}).get("in") == "header" and security_schemes.get("CsrfToken", {}).get("name") == "X-CSRF-Token", "OpenAPI: CSRF must be an explicit header contract")
    check(security_schemes.get("AgentBearer", {}).get("type") == "http" and security_schemes.get("AgentBearer", {}).get("scheme") == "bearer", "OpenAPI: agent authentication must be HTTP bearer")
    login_operation = openapi.get("paths", {}).get("/api/v1/session/login", {}).get("post", {})
    login_response = login_operation.get("responses", {}).get("200", {})
    login_headers = login_response.get("headers", {})
    set_cookie = login_headers.get("Set-Cookie", {})
    check(set(login_headers) == {"Set-Cookie", "X-Request-ID"}, "OpenAPI: login must issue the declared credential and request-id headers")
    check(set_cookie.get("required") is True, "OpenAPI: login Set-Cookie response header must be required")
    check("workplane_session=" in str(set_cookie.get("schema", {}).get("pattern", "")), "OpenAPI: login Set-Cookie must name workplane_session")
    session_schema = openapi.get("components", {}).get("schemas", {}).get("Session", {})
    csrf_schema = session_schema.get("properties", {}).get("csrf_token", {})
    check("csrf_token" in session_schema.get("required", []), "OpenAPI: login session body must return csrf_token")
    check(csrf_schema.get("type") == "string" and csrf_schema.get("minLength", 0) >= 32, "OpenAPI: login CSRF token must be a nontrivial typed response field")
    read_security = [{"HumanSession": []}, {"AgentBearer": []}]
    mutation_security = [{"HumanSession": [], "CsrfToken": []}, {"AgentBearer": []}]
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
            if key == "POST /api/v1/session/login":
                check(operation.get("security") == [], "OpenAPI: login must be the only explicitly unauthenticated operation")
            elif operation.get("x-agent-only") is True:
                check(operation.get("security") == [{"AgentBearer": []}], f"OpenAPI: agent-only read {key} must require bearer authentication")
            elif method in {"post", "put", "patch", "delete"}:
                check(operation.get("security") == mutation_security, f"OpenAPI: protected mutation {key} must require cookie+CSRF or agent bearer")
            else:
                check(operation.get("security") == read_security, f"OpenAPI: protected read {key} must require human session or agent bearer")
            if method in {"post", "put", "patch", "delete"} and key != "POST /api/v1/session/login":
                serialized = json.dumps(operation.get("parameters", []), sort_keys=True)
                check("IdempotencyKey" in serialized, f"OpenAPI: mutation {key} lacks Idempotency-Key")
            if method == "get":
                check("requestBody" not in operation, f"OpenAPI: GET {path} has a mutation body")
    check(None not in operation_ids and len(operation_ids) == len(set(operation_ids)), "OpenAPI: operationId values must be present and unique")
    decision_operation = openapi.get("paths", {}).get("/api/v1/projects/{project_id}/decisions", {}).get("post", {})
    check("ExpectedVersion" in json.dumps(decision_operation.get("parameters", []), sort_keys=True), "OpenAPI: recordDecision lacks required If-Match expected version")
    check("ETag" in decision_operation.get("responses", {}).get("201", {}).get("headers", {}), "OpenAPI: recordDecision response lacks committed version ETag")
    decision_schema = openapi.get("components", {}).get("schemas", {}).get("Decision", {})
    check(decision_schema.get("additionalProperties") is False and "allOf" not in decision_schema, "OpenAPI: Decision must be one closed composable response object")
    schemas = openapi.get("components", {}).get("schemas", {})
    check(openapi.get("x-request-body-max-bytes") == 65_536, "OpenAPI: request body byte boundary must be authoritative")
    expected_string_limits = {
        ("LoginRequest", "email"): (1, 320),
        ("LoginRequest", "password"): (1, 1024),
        ("CreateExplorationProject", "title"): (1, 200),
        ("CreateExplorationProject", "outcome"): (1, 2000),
        ("CreateExplorationProject", "hypothesis"): (1, 2000),
        ("CreateExplorationProject", "falsifier"): (1, 2000),
        ("CreateExplorationProject", "experiment_bound"): (1, 1000),
        ("RecordDecision", "question"): (1, 2000),
        ("RecordDecision", "choice"): (1, 2000),
        ("RecordDecision", "rationale"): (1, 4000),
    }
    for (schema_name, property_name), (minimum, maximum) in expected_string_limits.items():
        property_schema = schemas.get(schema_name, {}).get("properties", {}).get(property_name, {})
        check(
            property_schema.get("minLength") == minimum and property_schema.get("maxLength") == maximum,
            f"OpenAPI: {schema_name}.{property_name} must own min/max length {minimum}/{maximum}",
        )
    expected_array_limits = {
        ("CreateExplorationProject", "decision_criteria"): (1, 32, 1, 500),
        ("RecordDecision", "alternatives"): (0, 32, 0, 1000),
        ("RecordDecision", "evidence"): (0, 64, 0, 2000),
        ("RecordDecision", "consequences"): (0, 32, 0, 2000),
    }
    for (schema_name, property_name), (minimum, maximum, item_minimum, item_maximum) in expected_array_limits.items():
        property_schema = schemas.get(schema_name, {}).get("properties", {}).get(property_name, {})
        item_schema = property_schema.get("items", {})
        check(
            property_schema.get("minItems", 0) == minimum
            and property_schema.get("maxItems") == maximum
            and item_schema.get("minLength", 0) == item_minimum
            and item_schema.get("maxLength") == item_maximum,
            f"OpenAPI: {schema_name}.{property_name} must own item/cardinality boundaries",
        )
    project_request_schema = schemas.get("CreateExplorationProject", {})
    project_boundary = {
        "title": "t" * 200,
        "outcome": "o" * 2000,
        "hypothesis": "h" * 2000,
        "falsifier": "f" * 2000,
        "decision_criteria": ["c" * 500] * 32,
        "experiment_bound": "b" * 1000,
    }
    project_validator = Draft202012Validator(project_request_schema)
    check(not list(project_validator.iter_errors(project_boundary)), "OpenAPI: declared project boundary values must be accepted")
    project_over_boundary = dict(project_boundary, decision_criteria=["criterion"] * 33)
    check(bool(list(project_validator.iter_errors(project_over_boundary))), "OpenAPI: 33 decision criteria must be rejected")
    check(
        schemas.get("RecordDecision", {}).get("properties", {}).get("kind", {}).get("enum") == ["continue"],
        "OpenAPI: the M1 decision request must expose only the implemented continue kind",
    )
    try:
        decision_errors = sorted(
            Draft202012Validator(decision_schema, format_checker=FormatChecker()).iter_errors(load_json(ROOT / "fixtures/decision-response.json")),
            key=lambda item: list(item.path),
        )
        check(not decision_errors, f"OpenAPI: Decision response fixture is invalid: {[error.message for error in decision_errors]}")
    except Exception as error:
        errors.append(f"OpenAPI Decision fixture validation failed: {error}")

    generated_go = (ROOT / "internal/api/generated/server.gen.go").read_text(encoding="utf-8")
    generated_ts = (ROOT / "web/src/api/client.gen.ts").read_text(encoding="utf-8")
    for surface in ("SessionCookie", "CSRFToken", "BearerToken", "ExpectedVersion", "ResponseHeaders", "type Session struct", "RequestBodyMaxBytes", "CreateExplorationProjectDecisionCriteriaMaxItems"):
        check(surface in generated_go, f"generated Go: missing typed security/version surface {surface}")
    check('CSRFToken string `json:"csrf_token"`' in generated_go, "generated Go: login session body drops the CSRF token source")
    check(bool(re.search(r"SetCookie\s+string", generated_go)) and 'Header().Set("Set-Cookie"' in generated_go, "generated Go: login response cannot emit Set-Cookie")
    check(bool(re.search(r"ETag\s+VersionETag", generated_go)) and 'Header().Set("ETag"' in generated_go, "generated Go: versioned response cannot emit ETag")
    for surface in ("HumanMutationSecurity", "AgentBearerSecurity", "X-CSRF-Token", "Authorization", "expectedVersion", "If-Match", "csrf_token", "VersionedResponse", "requestContractSchemas", "validateRequestBody"):
        check(surface in generated_ts, f"generated TypeScript: missing typed security/version surface {surface}")
    check("options: VersionedMutationOptions" in generated_ts, "generated TypeScript: recordDecision does not require versioned mutation options")
    check('response.headers.get("ETag")' in generated_ts and "version: version as VersionETag" in generated_ts, "generated TypeScript: recordDecision drops the committed response version")

    event_schema = load_json(ROOT / "contracts/domain-event.schema.json")
    required_event_fields = {"event_id", "event_type", "schema_version", "organization_id", "aggregate_id", "aggregate_version", "request_id", "actor", "occurred_at", "payload"}
    check(required_event_fields <= set(event_schema.get("required", [])), "event schema: envelope required fields are incomplete")
    check(event_schema.get("additionalProperties") is False, "event schema: envelope must reject unknown fields")
    case_schema = load_json(ROOT / "contracts/acceptance-case.schema.json")
    evidence_schema = load_json(ROOT / "evidence/manifest.schema.json")
    check(case_schema.get("additionalProperties") is False, "case schema: must reject unknown fields")
    try:
        Draft202012Validator.check_schema(event_schema)
        Draft202012Validator.check_schema(case_schema)
        Draft202012Validator.check_schema(evidence_schema)
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
    template_ids = unique_ids(templates, "id", "expected deny templates")
    template_scopes = {template.get("applies_to") for template in templates}
    check(expected_denies.get("coverage") == "every_action_and_openapi_operation", "expected denies: coverage must include actions and operations")
    check(template_scopes == {"all_actions", "all_resource_actions", "human_required_actions"}, "expected denies: coverage templates incomplete")
    check(all(template.get("expected") == "deny" and template.get("rationale") for template in templates), "expected denies: every template needs deny and rationale")
    classification = expected_denies.get("resource_action_classification", {})
    resource_families = classification.get("resource_families", [])
    check(len(resource_families) == len(set(resource_families)), "expected denies: duplicate resource action family")
    check(set(resource_families) <= set(actions_by_family), f"expected denies: unknown resource action families {sorted(set(resource_families) - set(actions_by_family))}")
    resource_actions = {
        action
        for family in resource_families
        for action in actions_by_family.get(family, [])
    } | set(classification.get("resource_actions", []))
    non_resource_actions = set(classification.get("non_resource_actions", []))
    check(not (resource_actions & non_resource_actions), "expected denies: resource and non-resource action classes overlap")
    check(resource_actions | non_resource_actions == set(actions), f"expected denies: resource classification must partition every action missing={sorted(set(actions) - resource_actions - non_resource_actions)} extra={sorted((resource_actions | non_resource_actions) - set(actions))}")

    def required_deny_templates(action: str) -> set[str]:
        scopes = {"all_actions"}
        if action in resource_actions:
            scopes.add("all_resource_actions")
        if action in permissions.get("human_required", []):
            scopes.add("human_required_actions")
        return {template["id"] for template in templates if template["applies_to"] in scopes}

    expanded_denies = load_json(ROOT / "tests/registries/expected-denies.generated.json")
    expanded_action_entries = expanded_denies.get("actions", [])
    expanded_operation_entries = expanded_denies.get("operations", [])
    expanded_actions = {entry.get("action") for entry in expanded_action_entries}
    expanded_operations = {entry.get("operation") for entry in expanded_operation_entries}
    check(len(expanded_actions) == len(expanded_action_entries), "expected denies: duplicate expanded action entry")
    check(len(expanded_operations) == len(expanded_operation_entries), "expected denies: duplicate expanded operation entry")
    check(expanded_actions == set(actions), f"expected denies: expanded action coverage differs: missing={sorted(set(actions) - expanded_actions)} extra={sorted(expanded_actions - set(actions))}")
    check(expanded_operations == set(mapped_operations), f"expected denies: expanded operation coverage differs: missing={sorted(set(mapped_operations) - expanded_operations)} extra={sorted(expanded_operations - set(mapped_operations))}")
    check(set(expanded_denies.get("resource_actions", [])) == resource_actions, "expected denies: generated resource-action classification drift")
    expected_sources = {
        "permissions": file_digest(ROOT / "contracts/permissions.yaml"),
        "operation_permissions": file_digest(ROOT / "contracts/operation-permissions.yaml"),
        "expected_denies": file_digest(ROOT / "contracts/expected-denies.yaml"),
    }
    check(expanded_denies.get("sources") == expected_sources, "expected denies: generated source digests drift")
    for entry in expanded_action_entries:
        action = entry.get("action")
        expected_templates = required_deny_templates(action) if action in set(actions) else set()
        check(set(entry.get("deny_templates", [])) == expected_templates, f"expected denies: action {action} deny template mismatch expected={sorted(expected_templates)} actual={sorted(entry.get('deny_templates', []))}")
        check(set(entry.get("deny_templates", [])) <= template_ids, f"expected denies: action {action} references unknown template")
    for entry in expanded_operation_entries:
        operation = entry.get("operation")
        expected_actions = mapped_operations.get(operation, [])
        expected_templates = {template for action in expected_actions for template in required_deny_templates(action)}
        check(entry.get("actions") == expected_actions, f"expected denies: operation {operation} action list drift")
        check(set(entry.get("deny_templates", [])) == expected_templates, f"expected denies: operation {operation} deny template mismatch expected={sorted(expected_templates)} actual={sorted(entry.get('deny_templates', []))}")
    cross_org = "DENY-CROSS-ORG"
    check(all((cross_org in entry.get("deny_templates", [])) == (entry.get("action") in resource_actions) for entry in expanded_action_entries), "expected denies: cross-org action coverage does not match resource classification")
    check(all((cross_org in entry.get("deny_templates", [])) == any(action in resource_actions for action in entry.get("actions", [])) for entry in expanded_operation_entries), "expected denies: cross-org operation coverage does not match resource classification")

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
    topology = (topology_root / "instances.toml").read_text(encoding="utf-8")
    gate_blocks = re.findall(r"```(?:agent-team-verify-gates|verify-gates)\s*\n(.*?)```", topology, flags=re.DOTALL)
    check(len(gate_blocks) == 1, "topology: canonical verifier gate block must exist exactly once")
    if gate_blocks:
        gate_lines = [line.strip() for line in gate_blocks[0].splitlines() if line.strip()]
        check(gate_lines == ["workplane-smoke :: cd projects/workplane && make bootstrap && make smoke"], "topology: canonical verifier gate must name the exact Workplane smoke command")

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
