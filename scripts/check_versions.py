#!/usr/bin/env python3
"""Validate product version consistency across release surfaces."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import plistlib
import re
import subprocess
import sys
import tomllib
from typing import Iterable


ROOT = pathlib.Path(__file__).resolve().parents[1]
SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
PRODUCTION_TAG = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")


def fail(message: str) -> None:
    raise SystemExit(message)


def rel(path: pathlib.Path) -> str:
    return path.relative_to(ROOT).as_posix()


def read_text(path: pathlib.Path) -> str:
    return path.read_text(encoding="utf-8")


def load_toml(path: pathlib.Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def workspace_version() -> str:
    cargo = load_toml(ROOT / "Cargo.toml")
    try:
        version = cargo["workspace"]["package"]["version"]
    except KeyError as error:
        fail("Cargo.toml is missing [workspace.package].version")
    if not SEMVER.match(version):
        fail(f"workspace version is not stable semver MAJOR.MINOR.PATCH: {version}")
    return version


def workspace_members() -> Iterable[pathlib.Path]:
    cargo = load_toml(ROOT / "Cargo.toml")
    members = cargo.get("workspace", {}).get("members", [])
    for member in members:
        yield ROOT / member / "Cargo.toml"


def check_cargo(version: str, errors: list[str]) -> None:
    for manifest in workspace_members():
        if not manifest.exists():
            errors.append(f"workspace member manifest is missing: {rel(manifest)}")
            continue
        package = load_toml(manifest).get("package", {})
        package_version = package.get("version")
        if isinstance(package_version, dict):
            if package_version.get("workspace") is not True:
                errors.append(f"{rel(manifest)} has unsupported package.version table")
        elif package_version != version:
            errors.append(
                f"{rel(manifest)} package.version {package_version!r} does not match workspace {version}"
            )


def check_tauri(version: str, errors: list[str]) -> None:
    config_path = ROOT / "src-tauri" / "tauri.conf.json"
    config = json.loads(read_text(config_path))
    if config.get("version") != version:
        errors.append(
            f"{rel(config_path)} version {config.get('version')!r} does not match workspace {version}"
        )


def parse_xcode_values(project_text: str, key: str) -> list[str]:
    return re.findall(rf"(?m)^\s*{re.escape(key)} = ([^;]+);", project_text)


def clean_xcode_value(value: str) -> str:
    return value.strip().strip('"')


def check_ios(version: str, errors: list[str]) -> None:
    plist_path = ROOT / "ios" / "KE8YGWLogger" / "KE8YGWLogger" / "Resources" / "Info.plist"
    with plist_path.open("rb") as handle:
        plist = plistlib.load(handle)
    if plist.get("CFBundleShortVersionString") != "$(MARKETING_VERSION)":
        errors.append(f"{rel(plist_path)} must read CFBundleShortVersionString from MARKETING_VERSION")
    if plist.get("CFBundleVersion") != "$(CURRENT_PROJECT_VERSION)":
        errors.append(f"{rel(plist_path)} must read CFBundleVersion from CURRENT_PROJECT_VERSION")

    project_path = ROOT / "ios" / "KE8YGWLogger" / "KE8YGWLogger.xcodeproj" / "project.pbxproj"
    project_text = read_text(project_path)
    marketing_versions = {clean_xcode_value(value) for value in parse_xcode_values(project_text, "MARKETING_VERSION")}
    if marketing_versions != {version}:
        errors.append(
            f"{rel(project_path)} MARKETING_VERSION values {sorted(marketing_versions)} do not equal {version}"
        )

    build_versions = {clean_xcode_value(value) for value in parse_xcode_values(project_text, "CURRENT_PROJECT_VERSION")}
    if not build_versions:
        errors.append(f"{rel(project_path)} is missing CURRENT_PROJECT_VERSION")
    elif len(build_versions) != 1:
        errors.append(f"{rel(project_path)} has inconsistent CURRENT_PROJECT_VERSION values: {sorted(build_versions)}")
    else:
        build = next(iter(build_versions))
        if not re.match(r"^[0-9]+$", build):
            errors.append(f"{rel(project_path)} CURRENT_PROJECT_VERSION must be a numeric App Store build number")


def check_openapi(version: str, errors: list[str]) -> None:
    for path in (ROOT / "openapi" / "api-v1.yaml", ROOT / "openapi" / "api-v1-baseline.json"):
        spec = json.loads(read_text(path))
        info = spec.get("info", {})
        if info.get("version") != "1.0.0":
            errors.append(f"{rel(path)} info.version must remain 1.0.0 for the /api/v1 contract")
        if info.get("x-product-version") != version:
            errors.append(
                f"{rel(path)} info.x-product-version {info.get('x-product-version')!r} does not match workspace {version}"
            )


def check_release_workflow(version: str, errors: list[str]) -> None:
    workflow_path = ROOT / ".github" / "workflows" / "release.yml"
    workflow = read_text(workflow_path)
    required_fragments = [
        "python scripts/check_versions.py --release-tag",
        "ke8ygw-logger-${{ needs.validate-production-tag.outputs.version }}-",
        "pattern: ke8ygw-logger-*",
        "git merge-base --is-ancestor",
        '"v$version" != "$tag"',
    ]
    for fragment in required_fragments:
        if fragment not in workflow:
            errors.append(f"{rel(workflow_path)} is missing release/version artifact check fragment: {fragment}")

    ci_path = ROOT / ".github" / "workflows" / "ci.yml"
    ci = read_text(ci_path)
    for fragment in (
        "python scripts/check_versions.py",
        "internal-dev-${version}-${GITHUB_SHA}-${GITHUB_RUN_NUMBER}",
        "beta-main-${version}-${GITHUB_SHA}-${GITHUB_RUN_NUMBER}",
    ):
        if fragment not in ci:
            errors.append(f"{rel(ci_path)} is missing channel artifact/version fragment: {fragment}")


def check_release_tag(version: str, release_tag: str | None, errors: list[str]) -> None:
    tag = release_tag or os.environ.get("GITHUB_REF_NAME")
    ref_type = os.environ.get("GITHUB_REF_TYPE")
    if not tag or (release_tag is None and ref_type != "tag"):
        return

    if not PRODUCTION_TAG.match(tag):
        errors.append(f"production release tag must match vMAJOR.MINOR.PATCH: {tag}")
        return
    if tag != f"v{version}":
        errors.append(f"production tag {tag} does not match workspace version {version}")


def check_existing_tags(errors: list[str]) -> None:
    try:
        output = subprocess.check_output(
            ["git", "tag", "--list", "v*"],
            cwd=ROOT,
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError):
        return
    for tag in output.splitlines():
        if tag and not PRODUCTION_TAG.match(tag):
            errors.append(f"existing v-prefixed tag is not a production semver tag: {tag}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--print-version", action="store_true", help="print the canonical product version and exit")
    parser.add_argument("--release-tag", help="validate a production release tag against the workspace version")
    args = parser.parse_args()

    version = workspace_version()
    if args.print_version:
        print(version)
        return 0

    errors: list[str] = []
    check_cargo(version, errors)
    check_tauri(version, errors)
    check_ios(version, errors)
    check_openapi(version, errors)
    check_release_workflow(version, errors)
    check_release_tag(version, args.release_tag, errors)
    check_existing_tags(errors)

    if errors:
        for error in errors:
            print(f"version check failed: {error}", file=sys.stderr)
        return 1

    print(f"validated product version {version} across Cargo, Tauri, iOS, API metadata, artifacts, and tags")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
