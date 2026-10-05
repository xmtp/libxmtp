#!/usr/bin/env python3
"""Run one operation in a fresh Release iOS Simulator app process."""

import json
import math
import signal
import subprocess
import sys
import time
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fixtures import canonical, digest

MEMORY_SCOPE = "ios-app-resident-high-water"
# Termination after a failed or finished operation gets its own short deadline.
CLEANUP_SECONDS = 15


def checked_result(envelope, value):
    for key in ("operation_id", "request_sha256"):
        if value.get(key) != envelope[key]:
            raise ValueError(f"iOS response identity mismatch: {key}")
    if "error" in value:
        raise RuntimeError(f"iOS app failed: {value['error']}")
    result = value["result"]
    peak = result.get("peak_memory_bytes")
    if type(peak) is not int or peak <= 0 or result.get("memory_scope") != MEMORY_SCOPE:
        raise ValueError("iOS app resident high-water mark is absent")
    return result


def invoke(config, request):
    timeout = config["timeout_seconds"]
    if (
        not isinstance(timeout, (int, float))
        or not math.isfinite(timeout)
        or timeout <= 0
    ):
        raise ValueError("Set a finite positive iOS adapter timeout")
    if request["target"] != "swift":
        raise ValueError("iOS adapter requires a Swift request")
    app = Path(config["app_path"])
    root = Path(request["state_directory"])
    root.mkdir(parents=True, exist_ok=True)
    operation = uuid.uuid4().hex
    logs = root / "ios-operations" / operation
    logs.mkdir(parents=True)
    bundle = config["bundle_id"]
    udid = config["simulator_udid"]
    if not udid or udid == "booted":
        raise ValueError("Select one explicit simulator UDID")
    deadline = time.monotonic() + timeout
    commands = []

    def command(*args, cleanup=False):
        argv = [config.get("xcrun", "xcrun"), "simctl", *args]
        remaining = (
            CLEANUP_SECONDS
            if cleanup
            else max(0.01, deadline - time.monotonic())
        )
        result = subprocess.run(
            argv, text=True, capture_output=True, timeout=remaining
        )
        commands.append(
            {
                "argv": argv,
                "returncode": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }
        )
        if result.returncode and not (
            cleanup
            and result.returncode == 3
            and "found nothing to terminate" in result.stderr
        ):
            raise RuntimeError(f"simctl failed: {args}: {result.stderr}")
        return result

    try:
        # Every path, including install/launch failure, ends with termination.
        command("terminate", udid, bundle, cleanup=True)
        if request["phase"] == "setup":
            command("install", udid, str(app))
        container = Path(
            command("get_app_container", udid, bundle, "data").stdout.strip()
        )
        state_key = digest({"state_directory": str(root.resolve())})
        local = container / "Library/Application Support/xmtp-benchmark" / state_key
        local.mkdir(parents=True, exist_ok=True)
        fixture = json.loads((root / "fixture.json").read_text())
        if digest(fixture) != request["fixture_sha256"]:
            raise ValueError("iOS fixture digest mismatch")
        (local / "fixture.json").write_bytes(canonical(fixture))
        (local / "host.json").write_bytes(
            canonical(
                {
                    "backend_url": config["backend_url"],
                    "signer_url": config["signer_url"],
                }
            )
        )
        transport = container / "Library/Caches/xmtp-benchmark" / operation
        transport.mkdir(parents=True)
        envelope = {
            "operation_id": operation,
            "request_json": canonical(request).decode(),
            "request_sha256": digest(request),
            "state_key": state_key,
        }
        incoming = transport / "request.json"
        incoming.write_bytes(canonical(envelope))
        (logs / "request.json").write_bytes(incoming.read_bytes())
        command(
            "launch",
            "--terminate-running-process",
            udid,
            bundle,
            "--benchmark-request",
            str(incoming.relative_to(container)),
        )
        outgoing = transport / "response.json"
        while not outgoing.exists():
            if time.monotonic() >= deadline:
                raise TimeoutError("iOS operation exceeded its deadline")
            time.sleep(0.02)
        raw = outgoing.read_bytes()
        (logs / "response.json").write_bytes(raw)
        return checked_result(envelope, json.loads(raw))
    finally:
        try:
            command("terminate", udid, bundle, cleanup=True)
        finally:
            (logs / "commands.json").write_text(json.dumps(commands, indent=2))


def main():
    def interrupted(signum, frame):
        raise InterruptedError(f"iOS launcher interrupted by signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    config = json.loads(Path(sys.argv[1]).read_text())
    print(json.dumps(invoke(config, json.load(sys.stdin))))


if __name__ == "__main__":
    main()
