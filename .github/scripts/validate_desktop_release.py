#!/usr/bin/env python3
"""Fail a Desktop release when tag and checked-in product versions differ."""

import json
import re
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: validate_desktop_release.py RELEASE_TAG")

    tag = sys.argv[1]
    match = re.fullmatch(r"desktop(?:-test)?/v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)", tag)
    if not match:
        raise SystemExit(f"invalid Desktop release tag: {tag}")
    tag_version = match.group(1)

    root = Path(__file__).resolve().parents[2]
    tauri = json.loads((root / "apps/shell/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    manifests = sorted((root / "apps/desktop/manifests").glob("*.json"))

    versions = {"Tauri Desktop": tauri.get("version")}
    for manifest_path in manifests:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        versions[str(manifest_path.relative_to(root))] = manifest.get("version")

    errors = [f"{name}={version}" for name, version in versions.items() if version != tag_version]
    if errors:
        raise SystemExit(f"release tag version {tag_version} does not match: {', '.join(errors)}")
    if not manifests:
        raise SystemExit("no platform App manifests found")

    print(f"validated Desktop release {tag_version} across Tauri and {len(manifests)} App manifests")
    return 0


if __name__ == "__main__":
    sys.exit(main())
