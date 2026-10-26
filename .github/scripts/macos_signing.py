"""Import an optional, persistent Runtime signing identity into an isolated CI keychain.

Never generates a replacement identity. No private key/password is printed or committed.
"""
import base64
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import tempfile


def run(args):
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"macOS signing setup failed ({args[0]}); check certificate and password")
    return result.stdout


def cleanup(directory):
    keychain = directory / "signing.keychain-db"
    search_list = directory / "search-list.json"
    if search_list.exists():
        run(["security", "list-keychains", "-d", "user", "-s", *json.loads(search_list.read_text())])
    if keychain.exists():
        run(["security", "delete-keychain", str(keychain)])
    shutil.rmtree(directory, ignore_errors=True)


def setup():
    certificate = os.environ.get("ANVIL_MACOS_CERTIFICATE_P12", "")
    password = os.environ.get("ANVIL_MACOS_CERTIFICATE_PASSWORD", "")
    if not certificate:
        if password:
            raise RuntimeError("Certificate password configured without a P12 certificate")
        print("No macOS certificate configured; retaining ad-hoc signing")
        return
    # Record the path before import so the always() step can clean up partial failures.
    directory = Path(tempfile.mkdtemp(prefix="anvil-signing-", dir=os.environ["RUNNER_TEMP"]))
    directory.chmod(0o700)
    with open(os.environ["GITHUB_STATE_FILE"], "w") as state:
        state.write(str(directory))
    keychain = directory / "signing.keychain-db"
    p12 = directory / "identity.p12"
    keychain_password = secrets.token_hex(32)
    try:
        p12.touch(mode=0o600)
        p12.write_bytes(base64.b64decode(certificate, validate=True))
        run(["security", "create-keychain", "-p", keychain_password, str(keychain)])
        run(["security", "set-keychain-settings", "-lut", "21600", str(keychain)])
        run(["security", "unlock-keychain", "-p", keychain_password, str(keychain)])
        run(["security", "import", str(p12), "-k", str(keychain), "-P", password,
             "-T", "/usr/bin/codesign"])
        run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
             "-s", "-k", keychain_password, str(keychain)])
        # Tauri also signs nested binaries. Give it the same identity and make the
        # temporary keychain discoverable without changing the default keychain.
        search_list = re.findall(r'"([^"\n]+)"', run(["security", "list-keychains", "-d", "user"]))
        (directory / "search-list.json").write_text(json.dumps(search_list))
        run(["security", "list-keychains", "-d", "user", "-s", *search_list, str(keychain)])
        identities = set(re.findall(r"\) ([A-Fa-f0-9]{40}) ",
                                    run(["security", "find-identity", "-p", "codesigning", str(keychain)])))
        if len(identities) != 1:
            raise RuntimeError("P12 must contain exactly one code-signing identity")
        with open(os.environ["GITHUB_ENV"], "a") as env:
            identity = identities.pop()
            env.write(f"ANVIL_MACOS_SIGNING_IDENTITY={identity}\n")
            config = json.dumps({"bundle": {"macOS": {"signingIdentity": identity}}})
            env.write(f"ANVIL_MACOS_TAURI_CONFIG={config}\n")
            env.write(f"ANVIL_MACOS_SIGNING_KEYCHAIN={keychain}\n")
        print("Imported macOS Runtime signing identity")
    finally:
        p12.unlink(missing_ok=True)


if __name__ == "__main__":
    if sys.platform != "darwin":
        raise SystemExit("macOS signing setup requires macOS")
    if sys.argv[1:] == ["setup"]:
        setup()
    elif sys.argv[1:] == ["cleanup"]:
        state = Path(os.environ["GITHUB_STATE_FILE"])
        if state.exists():
            cleanup(Path(state.read_text()))
            state.unlink()
    else:
        raise SystemExit("Usage: macos_signing.py setup|cleanup")
