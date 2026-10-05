#!/usr/bin/env python3
"""Launch a release APK and read measurements from its Android process."""

import hashlib
import json
import subprocess
import sys
from pathlib import Path


def main():
    config = json.loads(Path(sys.argv[1]).read_text())
    request = json.load(sys.stdin)
    adb = [config["adb"], "-s", config["device_serial"]]
    package = "org.xmtp.benchmark"
    destination = f"/sdcard/Android/data/{package}/files/benchmark-input"
    root = Path(request["state_directory"])
    request["state_key"] = (
        "bench-" + hashlib.sha256(str(root).encode()).hexdigest()[:24]
    )

    def call(*args):
        return subprocess.run([*adb, *args], text=True, capture_output=True, check=True)

    if request["phase"] == "setup":
        call("install", "-r", config["apk"])
    call("shell", "mkdir", "-p", destination)
    # A shell-created directory must permit the release app to write its result.
    call("shell", "chmod", "0777", destination)
    local = root / "android-request.json"
    local.write_text(json.dumps(request))
    host = root / "android-host.json"
    host.write_text(
        json.dumps(
            {"backend_url": config["backend_url"], "signer_url": config["signer_url"]}
        )
    )
    for source, name in [
        (local, "request.json"),
        (host, "host.json"),
        (root / "fixture.json", "fixture.json"),
    ]:
        call("push", str(source), f"{destination}/{name}")
    call("shell", "rm", "-f", f"{destination}/response.json")
    run = call(
        "shell", "am", "instrument", "-w", f"{package}/org.xmtp.benchmark.Benchmark"
    )
    (root / "android-instrumentation.log").write_text(run.stdout + run.stderr)
    if (
        "benchmark=complete" not in run.stdout
        or "INSTRUMENTATION_CODE: 0" not in run.stdout
    ):
        raise RuntimeError(f"Android benchmark failed: {run.stdout} {run.stderr}")
    response = call("shell", "cat", f"{destination}/response.json")
    value = json.loads(response.stdout)
    if request["phase"] == "measure" and value.get("peak_memory_bytes", 0) <= 0:
        raise ValueError("Android process PSS was not sampled")
    print(json.dumps(value))


if __name__ == "__main__":
    main()
