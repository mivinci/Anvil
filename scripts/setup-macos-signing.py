"""Create a persistent local development signing certificate; upload only on request.

Private files live outside the checkout with user-only permissions. This script never
reads existing account credentials, changes trust settings, or replaces an identity.
"""
import base64
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys

DIRECTORY = Path.home() / "Library/Application Support/Anvil Development Signing"


def run(args, data=None):
    result = subprocess.run(args, input=data, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed; no credential output has been printed")
    return result.stdout.decode().strip()


def setup():
    os.umask(0o077)
    DIRECTORY.mkdir(mode=0o700, parents=True, exist_ok=True)
    config = DIRECTORY / "identity.json"
    if config.exists():
        print("Existing development signing identity retained")
        return
    p12 = DIRECTORY / "identity.p12"
    password = DIRECTORY / "p12-password.txt"
    pem = DIRECTORY / "certificate.pem"
    key = DIRECTORY / "private-key.pem"
    if not p12.exists():
        if password.exists() or pem.exists() or key.exists():
            raise RuntimeError("Incomplete certificate generation; inspect the private signing directory")
        password.write_text(secrets.token_urlsafe(32))
        run(["openssl", "req", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "3650", "-nodes",
             "-subj", "/CN=Anvil Development/O=Q2Q", "-addext", "basicConstraints=critical,CA:FALSE",
             "-addext", "keyUsage=critical,digitalSignature", "-addext", "extendedKeyUsage=codeSigning",
             "-keyout", str(key), "-out", str(pem)])
        try:
            run(["openssl", "pkcs12", "-export", "-inkey", str(key), "-in", str(pem), "-out", str(p12),
                 "-name", "Anvil Development", "-passout", "file:" + str(password),
                 "-keypbe", "PBE-SHA1-3DES", "-certpbe", "PBE-SHA1-3DES", "-macalg", "sha1"])
        finally:
            key.unlink(missing_ok=True)
    if not password.exists() or not pem.exists():
        raise RuntimeError("Incomplete certificate backup; refusing to replace the existing identity")
    fingerprint = run(["openssl", "x509", "-in", str(pem), "-noout", "-fingerprint", "-sha1"])
    identity = fingerprint.split("=", 1)[1].replace(":", "")
    if not re.fullmatch(r"[A-Fa-f0-9]{40}", identity):
        raise RuntimeError("Invalid certificate fingerprint")
    keychain = run(["security", "default-keychain", "-d", "user"]).strip('"')
    run(["security", "import", str(p12), "-k", keychain,
         "-P", password.read_text(), "-T", "/usr/bin/codesign"])
    config.write_text(json.dumps({"identity": identity, "keychain": keychain}) + "\n")
    print("Created Anvil Development identity and local signing configuration")


def upload():
    # Repository selection must be explicit; no automatic publishing or workflow run.
    repository = run(["git", "remote", "get-url", "origin"])
    if repository not in ("https://github.com/mivinci/Anvil.git", "git@github.com:mivinci/Anvil.git"):
        raise RuntimeError("Unexpected origin; refusing to upload signing secrets")
    for name, content in (
        ("ANVIL_MACOS_CERTIFICATE_P12", base64.b64encode((DIRECTORY / "identity.p12").read_bytes())),
        ("ANVIL_MACOS_CERTIFICATE_PASSWORD", (DIRECTORY / "p12-password.txt").read_bytes()),
    ):
        run(["gh", "secret", "set", name, "--repo", "mivinci/Anvil"], content)
    print("Configured the two macOS signing repository secrets; no release started")


if __name__ == "__main__":
    if sys.platform != "darwin":
        raise SystemExit("Requires macOS")
    if sys.argv[1:] == ["setup"]:
        setup()
    elif sys.argv[1:] == ["upload"]:
        upload()
    else:
        raise SystemExit("Usage: python3 scripts/setup-macos-signing.py setup|upload")
