#!/usr/bin/env python3
"""Copy Tauri installer files from Cargo's bundle directory into dist."""

import re
import shutil
import sys
from pathlib import Path


ALLOWED_SUFFIXES = {".appimage", ".deb", ".dmg", ".exe", ".msi", ".rpm"}


def main() -> int:
    if len(sys.argv) != 4:
        raise SystemExit("usage: collect_desktop_installers.py REPOSITORY_ROOT DIST_DIR VERSION")

    root = Path(sys.argv[1]).resolve()
    dist = Path(sys.argv[2]).resolve()
    version = sys.argv[3]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise SystemExit(f"invalid release version: {version}")
    version_pattern = re.compile(rf"(?<!\d){re.escape(version)}(?!\d)")
    bundle = root / "target/release/bundle"
    installers = [
        path
        for path in bundle.rglob("*")
        if path.is_file()
        and path.suffix.lower() in ALLOWED_SUFFIXES
        and version_pattern.search(path.name)
    ] if bundle.is_dir() else []
    if not installers:
        raise SystemExit(f"no Tauri {version} installer artifacts found under {bundle}")

    dist.mkdir(parents=True, exist_ok=True)
    for installer in installers:
        shutil.copy2(installer, dist / installer.name)
    print(f"collected {len(installers)} Tauri installer artifact(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
