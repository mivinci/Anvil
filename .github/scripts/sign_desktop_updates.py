#!/usr/bin/env python3
"""Sign per-target Desktop update manifests for a versioned release."""

import hashlib
import json
import os
import sys
import tarfile
from pathlib import Path
from urllib.parse import quote


TARGETS = [
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: sign_desktop_updates.py DIST_DIR")

    dist = Path(sys.argv[1])
    repository = os.environ["GITHUB_REPOSITORY"]
    release_tag = os.environ["RELEASE_TAG"]
    version = os.environ["RELEASE_VERSION"]
    signing_key_hex = os.environ["SIGNING_KEY"]
    public_key_hex = os.environ["UPDATE_PUBLIC_KEY"]

    try:
        from nacl.signing import SigningKey
    except ImportError as error:
        raise SystemExit("PyNaCl is required to sign Desktop update manifests") from error

    signing_key = SigningKey(bytes.fromhex(signing_key_hex))
    if signing_key.verify_key.encode().hex() != public_key_hex.lower():
        raise SystemExit("Desktop update signing key does not match the configured Desktop update public key")

    manifests_dir = dist / "update-manifests"
    manifests_dir.mkdir(parents=True, exist_ok=True)

    for target in TARGETS:
        package = dist / f"anvil-desktop-{version}-{target}.tar.gz"
        if not package.is_file():
            raise SystemExit(f"missing Desktop package for {target}: {package}")

        with tarfile.open(package, "r:gz") as archive:
            member = archive.getmember("manifest.json")
            if not member.isfile():
                raise SystemExit(f"invalid manifest entry in {package}")
            extracted = archive.extractfile(member)
            if extracted is None:
                raise SystemExit(f"cannot read manifest in {package}")
            app_manifest = json.load(extracted)

        if (
            app_manifest.get("app_id") != "anvil.desktop"
            or app_manifest.get("version") != version
            or app_manifest.get("target") != target
        ):
            raise SystemExit(f"App package manifest mismatch for {target}")

        payload = json.dumps(
            {
                "schema_version": 1,
                "channel": "stable",
                "app_id": "anvil.desktop",
                "version": version,
                "target": target,
                "package_url": f"https://github.com/{repository}/releases/download/{quote(release_tag, safe='')}/{package.name}",
                "package_size": package.stat().st_size,
                "package_sha256": sha256_file(package),
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")

        envelope = {
            "payload": payload.hex(),
            "signature": signing_key.sign(payload).signature.hex(),
        }
        (manifests_dir / f"{target}.json").write_text(
            json.dumps(envelope, separators=(",", ":")) + "\n", encoding="utf-8"
        )

    print(f"signed Desktop {version} update manifests for {len(TARGETS)} targets")
    return 0


if __name__ == "__main__":
    sys.exit(main())
