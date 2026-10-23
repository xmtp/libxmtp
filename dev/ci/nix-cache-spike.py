#!/usr/bin/env python3
"""Branch-only Nix runner experiment: enumerate, measure, and report warm members."""

import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

RUNNERS = {"x86_64-linux": "ubuntu-24.04", "aarch64-darwin": "macos-15"}
GIB = 1024**3


def matrix(path):
    rows = []
    for system, sets in json.loads(Path(path).read_text()).items():
        seen = set()
        for group, members in sets.items():
            for member, drv in members.items():
                # Shell aliases and any shared dev/release member need one build.
                if drv in seen:
                    continue
                seen.add(drv)
                rows.append(
                    dict(
                        system=system,
                        runner=RUNNERS[system],
                        member=member,
                        drv=drv,
                        target=f".#ciSets.{system}.{group}.passthru.members.{member}",
                    )
                )
    if not rows or len(rows) > 256:
        raise SystemExit(f"Unexpected matrix size: {len(rows)}")
    print("targets=" + json.dumps({"include": rows}, separators=(",", ":")))


def resources():
    result = {"cpu_count": os.cpu_count(), "machine": platform.machine()}
    result["disk_free_gib"] = {
        path: round(shutil.disk_usage(path).free / GIB, 3)
        for path in ("/", "/nix/store", os.environ["RUNNER_TEMP"])
        if Path(path).exists()
    }
    if platform.system() == "Linux":
        mem = dict(
            line.split(":", 1)
            for line in Path("/proc/meminfo").read_text().splitlines()
        )
        for key in ("MemTotal", "MemAvailable", "SwapFree"):
            result[key + "_gib"] = round(int(mem[key].split()[0]) * 1024 / GIB, 3)
    else:
        result["MemTotal_gib"] = round(
            int(subprocess.check_output(["sysctl", "-n", "hw.memsize"])) / GIB, 3
        )
        for command in (["memory_pressure", "-Q"], ["sysctl", "vm.swapusage"]):
            info = subprocess.run(command, capture_output=True, text=True, check=False)
            result[command[0]] = info.stdout.strip()
    return result


def directory():
    path = Path(os.environ["RUNNER_TEMP"]) / "nix-cache-spike"
    path.mkdir(parents=True, exist_ok=True)
    return path


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def build():
    output = directory()
    target = os.environ["SPIKE_TARGET"]
    started = time.monotonic()
    metrics = dict(target=target, before=resources(), samples=0)
    save(output / "metrics.json", metrics)
    # Record whether this member's source changed. Shell/tool-only members can
    # remain unchanged: their task really is to fetch/build the shell inputs.
    drv = subprocess.check_output(
        ["nix", "eval", "--raw", target + ".drvPath"], text=True
    )
    metrics.update(drv=drv, source_changed=drv != os.environ["SPIKE_ORIGINAL_DRV"])
    command = ["nix", "build", "--no-link", "--print-out-paths", target]
    print("Building", target, "source changed:", metrics["source_changed"], flush=True)
    # Nix's default output shows fetch/build decisions, without full build logs.
    with (output / "build.log").open("w") as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        while True:
            sample = resources()
            metrics["samples"] += 1
            metrics["elapsed_seconds"] = round(time.monotonic() - started, 1)
            metrics["after"] = sample
            free_disk = min(sample["disk_free_gib"].values())
            metrics["min_disk_free_gib"] = min(
                metrics.get("min_disk_free_gib", free_disk), free_disk
            )
            if "MemAvailable_gib" in sample:
                available = sample["MemAvailable_gib"]
                metrics["min_mem_available_gib"] = min(
                    metrics.get("min_mem_available_gib", available), available
                )
            save(output / "metrics.json", metrics)
            try:
                code = process.wait(timeout=5)
                break
            except subprocess.TimeoutExpired:
                pass
    metrics.update(exit_code=code, elapsed_seconds=round(time.monotonic() - started, 1))
    save(output / "metrics.json", metrics)
    # Show enough to diagnose a failure, while keeping the full output as an artifact.
    print("\n".join((output / "build.log").read_text().splitlines()[-60:]))
    print(json.dumps(metrics, indent=2))
    raise SystemExit(code)


def report():
    output = directory()
    path = output / "metrics.json"
    metrics = json.loads(path.read_text()) if path.exists() else {}
    metrics.update(
        target=os.environ["SPIKE_TARGET"], status=os.environ["SPIKE_JOB_STATUS"]
    )
    save(path, metrics)
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as summary:
        summary.write("```json\n" + json.dumps(metrics, indent=2) + "\n```\n")


def summary(path):
    print("One fresh runner per distinct dev/release member; no cache writes.\n")
    print(
        "Run-specific source comment forces project rebuilds; dependencies may substitute."
    )
    print(
        "Shell/tool-only rows may only substitute. Times include target evaluation and realization."
    )
    print(
        "Disk and Linux available RAM are sampled every 5 s; they are approximate low-water marks.\n"
    )
    print(
        "| Member | Result | Seconds | Source changed | Min free disk GiB | Min available RAM GiB |"
    )
    print("| --- | --- | ---: | --- | ---: | ---: |")
    for file in sorted(Path(path).glob("*/metrics.json")):
        row = json.loads(file.read_text())
        values = [
            row.get(key, "—")
            for key in (
                "target",
                "status",
                "elapsed_seconds",
                "source_changed",
                "min_disk_free_gib",
                "min_mem_available_gib",
            )
        ]
        print("| " + " | ".join(map(str, values)) + " |")


if __name__ == "__main__":
    action = sys.argv[1]
    if action == "matrix":
        matrix(sys.argv[2])
    elif action == "snapshot":
        save(directory() / "runner.json", resources())
    elif action == "build":
        build()
    elif action == "report":
        report()
    elif action == "summary":
        summary(sys.argv[2])
    else:
        raise SystemExit(f"Unknown action: {action}")
