#!/usr/bin/env python3
"""Live provider/CLI acceptance; opt-in CI, uses an isolated disposable data root."""
import argparse
import json
import pathlib
import subprocess
import tempfile
import os
import signal
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--versions", nargs="+", default=["21", "17", "21.0.8+9"])
    parser.add_argument("--check-interrupt", action="store_true")
    args = parser.parse_args()
    binary = str(pathlib.Path(args.binary).resolve())
    with tempfile.TemporaryDirectory(prefix="anvil-java-smoke-") as directory:
        root = pathlib.Path(directory)

        def call(*command, code=0):
            result = subprocess.run(
                [binary, "--data-dir", str(root), "--json", *command],
                capture_output=True, text=True, timeout=600,
            )
            assert result.returncode == code, result.stdout + result.stderr
            assert not result.stderr, result.stderr
            data = json.loads(result.stdout)
            assert data["schema_version"] == 1
            assert data["ok"] == (code == 0), data
            return data["data"] if code == 0 else data["error"]

        assert call("java", "list")["runtimes"] == []
        if args.check_interrupt:
            assert os.name == "posix", "SIGINT acceptance currently requires POSIX"
            process = subprocess.Popen(
                [binary, "--data-dir", str(root), "--json", "java", "install", "--version", args.versions[0]],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            try:
                deadline = time.monotonic() + 180
                while True:
                    committed = False
                    for journal in (root / "cache" / "java").glob("*.q2q-http.resume"):
                        try:
                            committed |= any(json.loads(journal.read_text())["done"])
                        except (FileNotFoundError, json.JSONDecodeError):
                            pass
                    if committed:
                        process.send_signal(signal.SIGINT)
                        break
                    assert process.poll() is None, "download completed before interruption could be checked"
                    assert time.monotonic() < deadline, "timed out waiting for a durable range"
                    time.sleep(0.01)
                stdout, stderr = process.communicate(timeout=60)
                assert process.returncode == 130, stdout + stderr
                assert not stderr, stderr
                assert json.loads(stdout)["error"]["code"] == "cancelled"
                assert call("java", "list")["runtimes"] == []
                print("PASS SIGINT after durable range; next install resumes", flush=True)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=30)
        installed = []
        for version in args.versions:
            result = call("java", "install", "--version", version)
            runtime = result["runtime"]
            assert not result["already_installed"], result
            assert runtime["major"] == int(version.split(".")[0])
            if "+" in version:
                assert runtime["version"] == version
            java = pathlib.Path(runtime["executable"])
            assert java.is_file() and java.is_relative_to(root)
            probe = subprocess.run([str(java), "-version"], capture_output=True, text=True, timeout=30)
            assert probe.returncode == 0, probe.stderr
            repeated = call("java", "install", "--version", version)
            assert repeated["already_installed"]
            assert repeated["runtime"]["id"] == runtime["id"]
            installed.append(runtime["id"])
            print(f"PASS install/repeat/probe: {runtime['id']}", flush=True)
        assert {r["id"] for r in call("java", "list")["runtimes"]} == set(installed)
        for runtime_id in installed:
            assert call("java", "uninstall", runtime_id)["removed"]["id"] == runtime_id
        assert call("java", "list")["runtimes"] == []
        assert any((root / "cache" / "java").glob("*.tar.gz")) or any((root / "cache" / "java").glob("*.zip"))
        assert call("java", "install", "--version", "bad", code=2)["code"] == "invalid_argument"
        print("PASS list/uninstall/cache/JSON/errors", flush=True)


if __name__ == "__main__":
    main()
