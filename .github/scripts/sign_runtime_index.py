#!/usr/bin/env python3
"""Build and Ed25519-sign the runtime update index for a CLI release.

Reads the bare per-target binaries from the dist directory, pins each with
its SHA-256 digest and size, and writes dist/runtime-index.json as a signed
envelope compatible with the serve self-updater and the Shell's App update
manifest format:

    {"payload": "<hex>", "signature": "<hex>"}

Requires the SIGNING_KEY (hex seed), RELEASE_VERSION and API_VERSION
environment variables.
"""

import hashlib
import json
import os
import sys

TARGETS = [
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
]


def main() -> int:
    dist = sys.argv[1] if len(sys.argv) > 1 else "dist"
    version = os.environ["RELEASE_VERSION"]
    api_version = os.environ["API_VERSION"]
    seed = bytes.fromhex(os.environ["SIGNING_KEY"])

    try:
        from nacl.signing import SigningKey
    except ImportError:
        print("pynacl is required to sign the runtime index", file=sys.stderr)
        return 1

    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        print("GITHUB_REPOSITORY is not set", file=sys.stderr)
        return 1
    tag = f"cli/v{version}"

    targets = {}
    for target in TARGETS:
        suffix = ".exe" if target.endswith("windows-msvc") else ""
        name = f"anvil-{version}-{target}{suffix}"
        path = os.path.join(dist, name)
        if not os.path.isfile(path):
            print(f"missing bare binary for {target}: {path}", file=sys.stderr)
            return 1
        digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
        targets[target] = {
            "binary": name,
            "sha256": digest,
            "size": os.path.getsize(path),
            "url": f"https://github.com/{repo}/releases/download/{tag}/{name}",
        }

    payload = json.dumps(
        {
            "schema_version": 1,
            "product": "anvil",
            "version": version,
            "http_api_version": api_version,
            "targets": targets,
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    signature = SigningKey(seed).sign(payload).signature
    envelope = {
        "payload": payload.hex(),
        "signature": signature.hex(),
    }
    with open(os.path.join(dist, "runtime-index.json"), "w", encoding="utf-8") as file:
        json.dump(envelope, file, separators=(",", ":"))
        file.write("\n")
    print(f"signed runtime index for anvil {version} (HTTP API {api_version})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
