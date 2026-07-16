#!/usr/bin/env python3
import json
import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
CURRENT = ROOT / "openapi" / "api-v1.yaml"
BASELINE = ROOT / "openapi" / "api-v1-baseline.json"


def load(path):
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError as error:
        raise SystemExit(f"{path}: invalid OpenAPI JSON/YAML subset: {error}") from error


def methods(paths, path):
    return {
        name: value
        for name, value in paths[path].items()
        if name.lower() in {"get", "post", "put", "patch", "delete", "head", "options", "query"}
    }


def schema_type(schema):
    value = schema.get("type")
    return tuple(value) if isinstance(value, list) else value


def compare_schema(name, old, new, errors):
    if not isinstance(old, dict) or not isinstance(new, dict):
        return

    old_type = schema_type(old)
    new_type = schema_type(new)
    if old_type != new_type:
        errors.append(f"changed field type at {name}: {old_type!r} -> {new_type!r}")

    old_enum = set(old.get("enum", []))
    new_enum = set(new.get("enum", []))
    if old_enum and not old_enum.issubset(new_enum):
        errors.append(f"narrowed enum at {name}: removed {sorted(old_enum - new_enum)}")

    old_required = set(old.get("required", []))
    new_required = set(new.get("required", []))
    added_required = new_required - old_required
    if added_required:
        errors.append(f"newly required fields at {name}: {sorted(added_required)}")

    old_props = old.get("properties", {})
    new_props = new.get("properties", {})
    for prop, old_prop in old_props.items():
        if prop not in new_props:
            errors.append(f"removed response/schema field at {name}.{prop}")
            continue
        compare_schema(f"{name}.{prop}", old_prop, new_props[prop], errors)

    if old.get("nullable") is True and new.get("nullable") is False:
        errors.append(f"narrowed nullability at {name}")
    if isinstance(old_type, tuple) and "null" in old_type:
        if not (isinstance(new_type, tuple) and "null" in new_type):
            errors.append(f"removed null from union at {name}")


def main():
    current = load(CURRENT)
    baseline = load(BASELINE)
    errors = []

    old_paths = baseline.get("paths", {})
    new_paths = current.get("paths", {})
    for path in sorted(old_paths):
        if path not in new_paths:
            errors.append(f"removed path: {path}")
            continue
        old_methods = methods(old_paths, path)
        new_methods = methods(new_paths, path)
        for method, old_op in old_methods.items():
            if method not in new_methods:
                errors.append(f"removed method: {method.upper()} {path}")
                continue
            new_op = new_methods[method]

            old_responses = set(old_op.get("responses", {}))
            new_responses = set(new_op.get("responses", {}))
            removed_statuses = old_responses - new_responses
            if removed_statuses:
                errors.append(
                    f"changed status codes for {method.upper()} {path}: removed {sorted(removed_statuses)}"
                )

            if old_op.get("security") != new_op.get("security"):
                errors.append(f"changed authentication requirements for {method.upper()} {path}")

            old_body = old_op.get("requestBody", {})
            new_body = new_op.get("requestBody", {})
            if not old_body.get("required", False) and new_body.get("required", False):
                errors.append(f"newly required request body for {method.upper()} {path}")

    old_schemas = baseline.get("components", {}).get("schemas", {})
    new_schemas = current.get("components", {}).get("schemas", {})
    for name, old_schema in old_schemas.items():
        if name not in new_schemas:
            errors.append(f"removed schema: {name}")
            continue
        compare_schema(f"components.schemas.{name}", old_schema, new_schemas[name], errors)

    old_error_codes = set(
        old_schemas.get("ErrorResponse", {})
        .get("properties", {})
        .get("code", {})
        .get("enum", [])
    )
    new_error_codes = set(
        new_schemas.get("ErrorResponse", {})
        .get("properties", {})
        .get("code", {})
        .get("enum", [])
    )
    if not old_error_codes.issubset(new_error_codes):
        errors.append(f"removed error codes: {sorted(old_error_codes - new_error_codes)}")

    if errors:
        for error in errors:
            print(f"contract break: {error}", file=sys.stderr)
        return 1

    print(f"validated {CURRENT.relative_to(ROOT)} against {BASELINE.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
