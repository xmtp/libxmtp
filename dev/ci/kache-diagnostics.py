#!/usr/bin/env python3
"""Save compiler cache counts without raw events or environment values."""

import argparse
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess


RESULTS = ("local_hit", "remote_hit", "miss", "error", "passthrough")


def collect(wrapper, runtime, ledger):
    wrapper = wrapper.resolve()
    version = subprocess.run(
        [str(wrapper), "--version"], capture_output=True, text=True, timeout=10
    )
    counts = Counter({name: 0 for name in (*RESULTS, "other")})
    elapsed = Counter({name: 0 for name in (*RESULTS, "other")})
    invalid = 0
    events = runtime / "events.jsonl"
    if events.is_file():
        with events.open() as stream:
            for line in stream:
                try:
                    event = json.loads(line)
                    if not isinstance(event, dict):
                        raise ValueError("Invalid cache event")
                except ValueError:
                    invalid += 1
                    continue
                result = event.get("result")
                result = result if result in RESULTS else "other"
                counts[result] += 1
                duration = event.get("elapsed_ms")
                if (
                    isinstance(duration, (int, float))
                    and math.isfinite(duration)
                    and duration >= 0
                ):
                    elapsed[result] += duration
    roles = []
    if ledger.is_file():
        index = json.loads(ledger.read_text())
        for entry in index.get("execution", []):
            role, action = entry.get("role"), entry.get("action")
            if role not in {"native", "bindgen", "wasm", "pure"} or action not in {
                "build",
                "reuse",
            }:
                continue
            record = {"role": role, "action": action}
            seconds = index.get("artifacts", {}).get(role, {}).get("seconds")
            if (
                action == "build"
                and isinstance(seconds, (int, float))
                and math.isfinite(seconds)
                and seconds >= 0
            ):
                record["buildSeconds"] = seconds
            roles.append(record)
    return {
        "schema": 1,
        "wrapper": {
            "path": str(wrapper),
            "immutableNixPath": str(wrapper).startswith("/nix/store/"),
            "sha256": hashlib.sha256(wrapper.read_bytes()).hexdigest(),
        },
        "version": version.stdout.strip().splitlines()[0]
        if version.returncode == 0 and version.stdout.strip()
        else None,
        "versionSucceeded": version.returncode == 0,
        "eventLogPresent": events.is_file(),
        "resultCounts": dict(counts),
        "eventElapsedMs": dict(elapsed),
        "invalidEventLines": invalid,
        "rawRoleExecution": roles,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = collect(
        Path(os.environ["RUSTC_WRAPPER"]),
        Path(os.environ["KACHE_RUNTIME_DIR"]),
        Path("target/sdk-artifacts/artifacts.json"),
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
