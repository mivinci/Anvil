#!/usr/bin/env python3
"""Opt-in real Minecraft server acceptance in an isolated temporary data root."""
import argparse
import json
import os
import pathlib
import platform
import queue
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request


VERSION_MANIFEST = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"


def call(binary, root, *command, timeout=600):
    result = subprocess.run(
        [binary, "--data-dir", str(root), "--json", *command],
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert not result.stderr, result.stderr
    envelope = json.loads(result.stdout)
    assert envelope["schema_version"] == 1 and envelope["ok"], envelope
    return envelope["data"]


def current_release():
    request = urllib.request.Request(
        VERSION_MANIFEST, headers={"User-Agent": "Anvil-live-server-smoke/1"}
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        manifest = json.load(response)
    version = manifest["latest"]["release"]
    assert isinstance(version, str) and version
    return version


def free_local_port():
    with socket.socket() as server_socket:
        server_socket.bind(("127.0.0.1", 0))
        return server_socket.getsockname()[1]


def drain_stdout(stream, messages):
    try:
        for line in iter(stream.readline, b""):
            try:
                messages.put(json.loads(line))
            except Exception as error:  # Preserve malformed output for a useful failure.
                messages.put(AssertionError(f"invalid JSON event {line!r}: {error}"))
    finally:
        messages.put(None)


def drain_stderr(stream):
    for line in iter(stream.readline, b""):
        sys.stderr.write(line.decode("utf-8", errors="replace"))
        sys.stderr.flush()


def run_server(binary, root, name, interrupt=False):
    args = [
        binary,
        "--data-dir",
        str(root),
        "--json-events",
        "server",
        "start",
        name,
        "--accept-eula",
    ]
    process = subprocess.Popen(
        args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    messages = queue.Queue()
    stdout_thread = threading.Thread(
        target=drain_stdout, args=(process.stdout, messages), daemon=True
    )
    stderr_thread = threading.Thread(
        target=drain_stderr, args=(process.stderr,), daemon=True
    )
    stdout_thread.start()
    stderr_thread.start()

    events = []
    action_sent = False
    deadline = time.monotonic() + 600
    try:
        while time.monotonic() < deadline:
            try:
                event = messages.get(timeout=1)
            except queue.Empty:
                if process.poll() is not None:
                    continue
                continue
            if event is None:
                break
            if isinstance(event, BaseException):
                raise event
            assert event.get("schema_version") == 1 and event.get("event") == "state", event
            events.append(event)
            state = event["data"].get("state")
            print(f"{name}: {state}", flush=True)
            if state == "ready" and not action_sent:
                if interrupt:
                    assert os.name == "posix", "SIGINT acceptance currently requires POSIX"
                    process.send_signal(signal.SIGINT)
                else:
                    process.stdin.write(b"stop\n")
                    process.stdin.flush()
                action_sent = True

        if not action_sent:
            raise AssertionError(f"server did not reach Ready; events: {events}")
        return_code = process.wait(timeout=60)
        stdout_thread.join(timeout=10)
        stderr_thread.join(timeout=10)
        assert return_code == 0, f"Anvil exited with {return_code}; events: {events}"
        states = [event["data"]["state"] for event in events]
        required = ["preparing", "starting", "running", "ready"]
        if interrupt:
            required.append("stopping")
        required.append("exited")
        assert states == required, f"unexpected lifecycle states: {states}"
        session = events[-1]["data"]["session"]
        assert session["state"] == "exited" and session["exit_code"] == 0, session
        assert session["stop_requested"], session
        return session
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)
        if process.stdin:
            process.stdin.close()
        stdout_thread.join(timeout=10)
        stderr_thread.join(timeout=10)


def run_version(binary, root, version, check_interrupt):
    name = "smoke-1-21-1" if version == "1.21.1" else "smoke-current"
    print(f"Installing Minecraft server {version}", flush=True)
    call(binary, root, "server", "create", name, "--version", version)

    game = root / "instances" / f"server-{name}" / "game"
    game.mkdir(parents=True, exist_ok=True)
    port = free_local_port()
    properties = game / "server.properties"
    properties.write_text(
        f"server-ip=127.0.0.1\nserver-port={port}\nonline-mode=true\n",
        encoding="utf-8",
    )

    call(binary, root, "server", "list")
    run_server(binary, root, name)
    eula = (game / "eula.txt").read_text(encoding="utf-8")
    assert "eula=true" in eula.lower(), eula
    saved_properties = properties.read_text(encoding="utf-8").lower()
    assert "server-ip=127.0.0.1" in saved_properties, saved_properties
    assert "online-mode=true" in saved_properties, saved_properties
    assert (game / "world" / "level.dat").is_file(), "world was not saved"

    if check_interrupt:
        print("Checking Ctrl-C graceful shutdown", flush=True)
        session = run_server(binary, root, name, interrupt=True)
        assert session["stop_requested"]
        assert (game / "world" / "level.dat").is_file(), "world disappeared after Ctrl-C"
    print(f"PASS server {version}: Ready, saved world, clean exit", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--versions", nargs="+", default=["1.21.1"])
    parser.add_argument("--include-current", action="store_true")
    parser.add_argument("--check-interrupt", action="store_true")
    args = parser.parse_args()
    binary = str(pathlib.Path(args.binary).resolve())
    print(f"Runner: {platform.system()} {platform.machine()}", flush=True)
    versions = list(args.versions)
    if args.include_current:
        release = current_release()
        print(f"Current Mojang release: {release}", flush=True)
        if release not in versions:
            versions.append(release)
    if args.check_interrupt and os.name != "posix":
        raise SystemExit("--check-interrupt is currently supported only on POSIX")

    with tempfile.TemporaryDirectory(prefix="anvil-server-smoke-") as directory:
        root = pathlib.Path(directory)
        for index, version in enumerate(versions):
            run_version(binary, root, version, args.check_interrupt and index == 0)
    print("PASS temporary server data removed", flush=True)


if __name__ == "__main__":
    main()
