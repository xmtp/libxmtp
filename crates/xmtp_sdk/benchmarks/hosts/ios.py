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
from ios_identity import tree_identity
import ios_cleanup

MEMORY_SCOPE = "ios-app-resident-high-water"


def checked_result(envelope, value):
    for key in (
        "operation_id",
        "request_sha256",
        "side",
        "package_sha256",
        "app_build_id",
    ):
        if value.get(key) != envelope[key]:
            raise ValueError(f"iOS response identity mismatch: {key}")
    if "error" in value:
        raise RuntimeError(f"iOS app failed: {value['error']}")
    result = value["result"]
    peak = result.get("peak_memory_bytes")
    if type(peak) is not int or peak <= 0 or result.get("memory_scope") != MEMORY_SCOPE:
        raise ValueError("iOS app resident high-water mark is absent")
    return result


def invoke(config, request, call=None):
    timeout = config["timeout_seconds"]
    if (
        not isinstance(timeout, (int, float))
        or not math.isfinite(timeout)
        or timeout <= 0
    ):
        raise ValueError("Set a finite positive iOS adapter timeout")
    receipt = json.loads(Path(config["build_receipt"]).read_text())
    for key in ("side", "package_sha256"):
        if request[key] != receipt[key]:
            raise ValueError(f"iOS app does not match request {key}")
    if request["target"] != "swift":
        raise ValueError("iOS adapter requires a Swift request")
    app = Path(receipt["app_path"])
    if tree_identity(app) != receipt["app_sha256"]:
        raise ValueError("iOS app bytes differ from the build receipt")
    root = Path(request["state_directory"])
    root.mkdir(parents=True, exist_ok=True)
    operation = uuid.uuid4().hex
    logs = root / "ios-operations" / operation
    logs.mkdir(parents=True)
    bundle = receipt["bundle_id"]
    udid = config["simulator_udid"]
    if not udid or udid == "booted":
        raise ValueError("Select one explicit simulator UDID for both sides")
    if receipt.get("simulator_udid") != udid:
        raise ValueError("Use the simulator recorded by the app build")
    deadline = time.monotonic() + timeout
    commands = []

    def command(*args, cleanup=False):
        argv = [config.get("xcrun", "xcrun"), "simctl", *args]
        remaining = (
            ios_cleanup.CLEANUP_SECONDS
            if cleanup
            else max(0.01, deadline - time.monotonic())
        )
        result = (call or subprocess.run)(
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
        ios_cleanup.register(
            request,
            [config.get("xcrun", "xcrun"), "simctl", "terminate", udid, bundle],
            logs,
        )
        # Every path, including install/launch failure, ends with termination.
        command("terminate", udid, bundle, cleanup=True)
        if request["phase"] in {"setup", "probe"}:
            command("install", udid, str(app))
        installed = Path(
            command("get_app_container", udid, bundle, "app").stdout.strip()
        )
        if tree_identity(installed) != receipt["app_sha256"]:
            raise ValueError("Installed iOS app bytes differ from the build receipt")
        container = Path(
            command("get_app_container", udid, bundle, "data").stdout.strip()
        )
        state_key = digest(
            {"state_directory": str(root.resolve()), "side": request["side"]}
        )
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
            "side": receipt["side"],
            "package_sha256": receipt["package_sha256"],
            "app_build_id": receipt["app_build_id"],
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
        ios_cleanup.clear(request)


def main():
    def interrupted(signum, frame):
        raise InterruptedError(f"iOS launcher interrupted by signal {signum}")

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    config = json.loads(Path(sys.argv[1]).read_text())
    print(json.dumps(invoke(config, json.load(sys.stdin))))


if __name__ == "__main__":
    main()
