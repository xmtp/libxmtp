#!/usr/bin/env python3
"""Branch-only Nix runner experiment: group, measure, and report warm members."""

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
# Keep expensive product families separate; combine shells and products whose
# closures overlap. Fail on an unassigned member so new warm outputs cannot vanish.
GROUPS = {
    "x86_64-linux": {
        "core-shells": ("shell-default", "shell-rust", "shell-wasm"),
        "js-shells": ("shell-js", "shell-js-node", "shell-docs"),
        "android-shells": (
            "shell-android",
            "shell-android-ci",
            "shell-android-ci-emulator",
        ),
        "react-native-js": (
            "shell-react-native",
            "react-native-js-tools",
            "react-native-js-tools-deps",
            "ubjs-react-native",
            "xmtp-sdk-generated-react-native",
            "xmtp-sdk-react-native-js",
        ),
        "js-sdks-and-fixtures": (
            "xmtp-sdk-generated-node",
            "xmtp-sdk-generated-browser",
            "xmtp-sdk-wasm-fixture-panic",
            "xmtp-sdk-pure-fixture-codec",
            "xmtp-legacy-migration-fixture-host",
        ),
        "android-sdk": (
            "android-sdk-libs",
            "android-sdk-libs-fast",
            "xmtp-sdk-libs",
            "xmtp-sdk-generated-kotlin",
        ),
        "android-example": ("example-android-apk",),
        "backend": ("xmtp-backend", "backend-image-x86_64-unknown-linux-musl"),
        "node-gnu": ("xmtp-sdk-node-linux-x64-gnu", "xmtp-sdk-node-linux-arm64-gnu"),
        "node-musl": ("xmtp-sdk-node-linux-x64-musl", "xmtp-sdk-node-linux-arm64-musl"),
    },
    "aarch64-darwin": {
        "core-shells": ("shell-default", "shell-ios", "shell-rust", "shell-wasm"),
        "js-shells": ("shell-js", "shell-js-node", "shell-docs"),
        "android": ("shell-android", "android-sdk-libs-fast"),
        "js-sdks": ("xmtp-sdk-generated-node", "xmtp-sdk-generated-browser"),
        "services": ("services-test", "backend-image-aarch64-unknown-linux-musl"),
        # Fast packages a subset of release's slices; share their compilation.
        "apple": (
            "ios-xcframeworks-fast",
            "ios-xcframeworks",
            "xmtp-sdk-node-darwin-arm64",
        ),
    },
}


def matrix(path):
    rows = []
    for system, sets in json.loads(Path(path).read_text()).items():
        seen = set()
        available = {
            name: dict(
                member=name,
                set=group,
                drv=drv,
                target=f".#ciSets.{system}.{group}.passthru.members.{name}",
            )
            for group, members in sets.items()
            for name, drv in members.items()
        }
        assigned = [name for names in GROUPS[system].values() for name in names]
        if len(assigned) != len(set(assigned)) or set(assigned) != set(available):
            raise SystemExit(
                f"Grouping does not match {system} warm members: "
                f"{set(assigned) ^ set(available)}"
            )
        for group, names in GROUPS[system].items():
            members = []
            for name in names:
                member = available[name]
                drv = member["drv"]
                # Shell aliases and shared dev/release members need one build.
                if drv in seen:
                    continue
                seen.add(drv)
                members.append(member)
            if members:
                rows.append(
                    dict(
                        system=system,
                        runner=RUNNERS[system],
                        group=group,
                        members=members,
                    )
                )
    if not 10 <= len(rows) <= 20:
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
    members = json.loads(os.environ["SPIKE_MEMBERS"])
    targets = [member["target"] for member in members]
    started = time.monotonic()
    metrics = dict(targets=targets, before=resources(), samples=0)
    save(output / "metrics.json", metrics)
    # Record whether this member's source changed. Shell/tool-only members can
    # remain unchanged: their task really is to fetch/build the shell inputs.
    expression = (
        "sets: { "
        + " ".join(
            f"{json.dumps(m['member'])} = sets.{json.dumps(m['set'])}.passthru.members."
            f"{json.dumps(m['member'])}.drvPath;"
            for m in members
        )
        + " }"
    )
    drvs = json.loads(
        subprocess.check_output(
            [
                "nix",
                "eval",
                "--json",
                f".#ciSets.{os.environ['SPIKE_SYSTEM']}",
                "--apply",
                expression,
            ],
            text=True,
        )
    )
    metrics.update(
        drvs=drvs,
        source_changed={m["member"]: drvs[m["member"]] != m["drv"] for m in members},
    )
    command = [
        "nix",
        "build",
        "--keep-going",
        "--no-link",
        "--print-out-paths",
        *targets,
    ]
    print("Building", targets, "source changed:", metrics["source_changed"], flush=True)
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
        group=f"{os.environ['SPIKE_SYSTEM']} / {os.environ['SPIKE_GROUP']}",
        status=os.environ["SPIKE_JOB_STATUS"],
        targets=[m["target"] for m in json.loads(os.environ["SPIKE_MEMBERS"])],
    )
    save(path, metrics)
    with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as summary:
        summary.write("```json\n" + json.dumps(metrics, indent=2) + "\n```\n")


def summary(path):
    print("One fresh runner per logical dev/release group; no cache writes.\n")
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
        "| Group | Result | Seconds | Source changes | Min free disk GiB | Min available RAM GiB |"
    )
    print("| --- | --- | ---: | --- | ---: | ---: |")
    for file in sorted(Path(path).glob("*/metrics.json")):
        row = json.loads(file.read_text())
        changed = row.get("source_changed", {})
        row["source_changes"] = f"{sum(changed.values())}/{len(changed)}"
        values = [
            row.get(key, "—")
            for key in (
                "group",
                "status",
                "elapsed_seconds",
                "source_changes",
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
