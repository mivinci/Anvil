#!/usr/bin/env python3
"""Sign per-target Shell release-page manifests for a Desktop release."""

import json
import os
import re
import sys
from pathlib import Path
from urllib.parse import quote


TARGET_INSTALLERS = {
    "x86_64-unknown-linux-gnu": ["Anvil_{version}_amd64.AppImage", "Anvil_{version}_amd64.deb"],
    "x86_64-apple-darwin": ["Anvil_{version}_x64.dmg"],
    "aarch64-apple-darwin": ["Anvil_{version}_aarch64.dmg"],
    "x86_64-pc-windows-msvc": [
        "Anvil_{version}_x64_en-US.msi",
        "Anvil_{version}_x64-setup.exe",
    ],
}


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: sign_shell_updates.py DIST_DIR")

    dist = Path(sys.argv[1])
    repository = os.environ["GITHUB_REPOSITORY"]
    release_tag = os.environ["RELEASE_TAG"]
    version = os.environ["RELEASE_VERSION"]
    signing_key_hex = os.environ["SIGNING_KEY"]
    public_key_hex = os.environ["UPDATE_PUBLIC_KEY"]

    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise SystemExit(f"invalid Shell version: {version}")
    expected_tag = f"desktop/v{version}"
    if release_tag != expected_tag:
        raise SystemExit(f"release tag {release_tag!r} does not match Shell version {version}")

    try:
        from nacl.signing import SigningKey
    except ImportError as error:
        raise SystemExit("PyNaCl is required to sign Shell update manifests") from error

    signing_key = SigningKey(bytes.fromhex(signing_key_hex))
    if signing_key.verify_key.encode().hex() != public_key_hex.lower():
        raise SystemExit("Shell update signing key does not match the configured Desktop update public key")

    release_url = f"https://github.com/{repository}/releases/tag/{quote(release_tag, safe='/')}"
    manifests_dir = dist / "shell-update-manifests"
    manifests_dir.mkdir(parents=True, exist_ok=True)

    for target, installer_names in TARGET_INSTALLERS.items():
        if not any((dist / name.format(version=version)).is_file() for name in installer_names):
            raise SystemExit(f"missing Shell installer for {target}")
        payload = json.dumps(
            {
                "schema_version": 1,
                "channel": "stable",
                "product_id": "anvil-shell",
                "version": version,
                "target": target,
                "release_url": release_url,
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

    print(f"signed Shell {version} release manifests for {len(TARGET_INSTALLERS)} targets")
    return 0


if __name__ == "__main__":
    sys.exit(main())
