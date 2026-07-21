#!/usr/bin/env python3
"""Check local Markdown links without requiring network access."""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys
from urllib.parse import unquote


ROOT = pathlib.Path(__file__).resolve().parents[1]
LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
EXTERNAL = re.compile(r"^(https?|mailto):", re.IGNORECASE)


def tracked_markdown_files() -> list[pathlib.Path]:
    output = subprocess.check_output(["git", "ls-files", "*.md"], cwd=ROOT, text=True)
    return [ROOT / line for line in output.splitlines() if line]


def is_anchor_only(target: str) -> bool:
    return target.startswith("#")


def check_file(path: pathlib.Path) -> list[str]:
    errors: list[str] = []
    text = path.read_text(encoding="utf-8")
    for match in LINK.finditer(text):
        target = match.group(1)
        if EXTERNAL.match(target) or is_anchor_only(target):
            continue
        path_part = target.split("#", 1)[0]
        if not path_part:
            continue
        decoded = unquote(path_part)
        candidate = (path.parent / decoded).resolve()
        try:
            candidate.relative_to(ROOT)
        except ValueError:
            errors.append(f"{path.relative_to(ROOT).as_posix()}: link escapes repository: {target}")
            continue
        if not candidate.exists():
            errors.append(f"{path.relative_to(ROOT).as_posix()}: broken local Markdown link: {target}")
    return errors


def main() -> int:
    errors: list[str] = []
    for path in tracked_markdown_files():
        errors.extend(check_file(path))
    if errors:
        for error in errors:
            print(f"docs link check failed: {error}", file=sys.stderr)
        return 1
    print("validated local Markdown links")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
