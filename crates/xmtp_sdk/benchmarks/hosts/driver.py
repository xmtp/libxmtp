#!/usr/bin/env python3
"""Wrap a real host executable with the run protocol and build timing."""

import json
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fixtures import digest, expected_observation
from processes import execute


def command(argv, request, log):
    if request["target"] == "swift":
        run = subprocess.run(
            argv, input=json.dumps(request), capture_output=True, text=True
        )
        code, stdout, stderr, memory = run.returncode, run.stdout, run.stderr, 0
    else:
        code, stdout, stderr, _, memory = execute(argv, json.dumps(request))
    log.with_suffix(".stderr").write_text(stderr)
    log.with_suffix(".stdout").write_text(stdout)
    if code:
        raise RuntimeError(f"Host exited {code}: {log}")
    response = json.loads(stdout)
    if request["phase"] == "measure":
        if request["target"] == "swift":
            peak = response.get("peak_memory_bytes")
            if (
                type(peak) is not int
                or peak <= 0
                or response.get("memory_scope") != "ios-app-resident-high-water"
            ):
                raise ValueError(
                    "Swift measurement requires the app resident high-water mark"
                )
        elif request["target"] != "kotlin":
            response["peak_memory_bytes"] = max(
                memory, response.get("peak_memory_bytes", 0)
            )
    return response


def record_observation(response, workload, log):
    # Only the page workload returns content. Other workloads report completion
    # and, for stream, delivered-ID counts. No observation is made up for them.
    if workload != "page":
        if "observed_messages" in response:
            raise ValueError("Only page measurements return observed public values")
        if response.get("completed") is not True:
            raise ValueError("The host did not complete its public operation")
        if workload == "stream" and not all(
            type(response.get(key)) is int
            for key in ("streamed_primary", "streamed_events")
        ):
            raise ValueError("Stream measurements require delivered-ID counts")
        return
    if "observed_messages" not in response:
        raise ValueError("Page measurements require observed public values")
    values = response.pop("observed_messages")
    response["observation"] = {
        "count": len(values),
        "semantic_sha256": digest(values),
    }
    (log.with_suffix(".observations.json")).write_text(json.dumps(values))


def main():
    config = json.loads(Path(sys.argv[1]).read_text())
    request = json.load(sys.stdin)
    root = Path(request["state_directory"])
    root.mkdir(parents=True, exist_ok=True)
    log = (
        root
        / f"host-{request['phase']}-{request.get('workload', 'setup')}-{request.get('sample', 0)}"
    )
    if request["phase"] == "setup":
        (root / "fixture.json").write_bytes(Path(request["fixture"]).read_bytes())
    fixture = json.loads((root / "fixture.json").read_text())
    if digest(fixture) != request["fixture_sha256"]:
        raise ValueError("Host fixture hash mismatch")
    workload = request.get("workload", "")
    if workload.startswith("build_"):
        cache = Path(config["build_cache"]).resolve()
        # Clean only this explicitly declared cache inside the benchmark state.
        cache.relative_to(root.resolve())
        if cache == root.resolve():
            raise ValueError(
                "Build cache must not be the complete benchmark state directory"
            )
        if request["phase"] == "reset":
            if workload == "build_clean" and cache.exists():
                shutil.rmtree(cache)
            if workload == "build_warm":
                warm = subprocess.run(
                    config["build_command"], capture_output=True, text=True, check=True
                )
                log.with_suffix(".warm.stdout").write_text(warm.stdout)
                log.with_suffix(".warm.stderr").write_text(warm.stderr)
            response = {"ready": True}
        else:
            code, stdout, stderr, duration, memory = execute(config["build_command"])
            log.with_suffix(".build.stdout").write_text(stdout)
            log.with_suffix(".build.stderr").write_text(stderr)
            if code:
                raise RuntimeError(f"Build failed with exit {code}")
            response = {
                "duration_ms": duration,
                "completed": True,
                "peak_memory_bytes": memory,
            }
    else:
        response = command(config["host_command"], request, log)
    response["request_sha256"] = digest(request)
    if request["phase"] == "measure":
        if not workload.startswith("build_") and response.get("source") != {
            "fixture_sha256": request["fixture_sha256"],
            "package_sha256": request["package_sha256"],
        }:
            raise ValueError("The host did not identify its fixture and package")
        record_observation(response, workload, log)
        if request["target"] == "browser" and workload.startswith("build_"):
            response["long_tasks_ms"] = []
        # These flags cover this timed operation only. The separate callback
        # matrix must establish retained-work and lifetime behavior across cycles.
        safety = response.setdefault("safety", {})
        # Exact page observations establish content correctness only.
        safety.setdefault(
            "correctness",
            response["observation"] == expected_observation(fixture)
            if workload == "page"
            else None,
        )
        for outcome in ("deadlock", "use_after_end", "retained_growth"):
            safety.setdefault(outcome, None)
    print(json.dumps(response))


if __name__ == "__main__":
    main()
