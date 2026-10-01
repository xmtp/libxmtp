#!/usr/bin/env python3
"""Run real simulator bridge controls. These are not performance samples."""

import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "hosts"))
import ios
from fixtures import canonical, digest


def main():
    config = json.loads(Path(sys.argv[1]).read_text())
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    receipt = json.loads(Path(config["build_receipt"]).read_text())
    fixture = {"messages": [], "callback_delay_ms": 25}
    (output / "fixture.json").write_bytes(canonical(fixture))
    request = {
        "phase": "probe",
        "target": "swift",
        "side": receipt["side"],
        "package_sha256": receipt["package_sha256"],
        "fixture_sha256": digest(fixture),
        "state_directory": str(output),
    }
    results = []

    def record(name, value):
        results.append({"name": name, **value})
        (output / "controls.json").write_text(json.dumps(results, indent=2))

    def probe(name, **fields):
        start = time.monotonic()
        result = ios.invoke(config, {**request, **fields})
        record(name, {"result": result, "wall_ms": (time.monotonic() - start) * 1000})
        return result

    baseline = probe("fresh-process")
    allocated = probe("touched-allocation", allocate_bytes=64 * 1024 * 1024)
    fresh = probe("fresh-process-after-allocation")
    assert (
        allocated["peak_memory_bytes"]
        > baseline["peak_memory_bytes"] + 32 * 1024 * 1024
    )
    assert (
        allocated["peak_memory_bytes"] > fresh["peak_memory_bytes"] + 32 * 1024 * 1024
    )
    assert allocated["peak_memory_bytes"] >= allocated["allocation_peak_bytes"]
    probe("http-signer", probe="signer")
    outside = probe("transport-delay", transport_delay_ms=300)
    inside = probe("operation-delay", operation_delay_ms=300)
    assert outside["duration_ms"] < 100 and inside["duration_ms"] >= 290

    for name, fields in [
        ("app-error", {"probe": "error"}),
        ("deadline", {"operation_delay_ms": 10000}),
    ]:
        settings = {**config, "timeout_seconds": 3} if name == "deadline" else config
        try:
            ios.invoke(settings, {**request, **fields})
        except (RuntimeError, TimeoutError) as error:
            record(name, {"rejected": str(error)})
        else:
            raise AssertionError(f"{name} was admitted")
        logs = max(
            (output / "ios-operations").iterdir(), key=lambda p: p.stat().st_mtime_ns
        )
        commands = json.loads((logs / "commands.json").read_text())
        assert (
            commands[-1]["argv"][2] == "terminate" and commands[-1]["returncode"] == 0
        )
        probe(name + "-recovery")

    # Change the request digest after staging. The real app must report its own error.
    def changed_digest(argv, **kwargs):
        if argv[2] == "launch":
            container = subprocess.run(
                [
                    argv[0],
                    "simctl",
                    "get_app_container",
                    config["simulator_udid"],
                    receipt["bundle_id"],
                    "data",
                ],
                capture_output=True,
                text=True,
                check=True,
            ).stdout.strip()
            path = Path(container) / argv[-1]
            envelope = json.loads(path.read_text())
            envelope["request_sha256"] = "0" * 64
            path.write_text(json.dumps(envelope))
        return subprocess.run(argv, **kwargs)

    try:
        ios.invoke(config, request, changed_digest)
    except ValueError as error:
        record("app-rejects-changed-digest", {"rejected": str(error)})
    else:
        raise AssertionError("Changed digest was admitted")
    logs = max(
        (output / "ios-operations").iterdir(), key=lambda p: p.stat().st_mtime_ns
    )
    error = json.loads((logs / "response.json").read_text())["error"]
    assert "Original request digest mismatch" in error["message"]
    probe("identity-recovery")
    record(
        "scope",
        {
            "passed": True,
            "performance_samples": False,
            "public_sdk_workload_execution": False,
            "build_receipt": config["build_receipt"],
        },
    )
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
