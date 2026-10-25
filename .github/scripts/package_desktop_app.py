#!/usr/bin/env python3
"""Create the immutable tar.gz payload consumed by the Shell updater."""

import json
import gzip
import sys
import tarfile
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: package_desktop_app.py BASELINE_DIR OUTPUT_ARCHIVE")

    baseline = Path(sys.argv[1]).resolve()
    archive_path = Path(sys.argv[2]).resolve()
    manifest_path = baseline / "manifest.json"
    if not manifest_path.is_file():
        raise SystemExit(f"missing App manifest: {manifest_path}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 2 or manifest.get("app_id") != "anvil.desktop":
        raise SystemExit("unexpected App manifest schema or app_id")
    for field in ("version", "target", "page_entry", "sidecar_entry"):
        if not isinstance(manifest.get(field), str) or not manifest[field]:
            raise SystemExit(f"invalid App manifest field: {field}")

    for relative in ("manifest.json", manifest["page_entry"], manifest["sidecar_entry"]):
        path = (baseline / relative).resolve()
        if not path.is_relative_to(baseline) or not path.is_file():
            raise SystemExit(f"missing or unsafe package entry: {relative}")

    archive_path.parent.mkdir(parents=True, exist_ok=True)
    def normalize_metadata(info: tarfile.TarInfo) -> tarfile.TarInfo:
        info.mtime = 0
        info.uid = 0
        info.gid = 0
        info.uname = ""
        info.gname = ""
        return info

    with archive_path.open("wb") as raw_file:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw_file, mtime=0, compresslevel=9) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as archive:
                for path in sorted(baseline.rglob("*")):
                    if path.is_symlink():
                        raise SystemExit(f"refusing symbolic link in App package: {path}")
                    if path.is_file():
                        archive.add(
                            path,
                            arcname=path.relative_to(baseline).as_posix(),
                            recursive=False,
                            filter=normalize_metadata,
                        )

    print(f"packaged {manifest['app_id']} {manifest['version']} for {manifest['target']}: {archive_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
