#!/usr/bin/env python3
"""Run the real SDK workload inside the owned API 34 emulator scope."""

import argparse
import difflib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import threading
import subprocess
import sys
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[4]
ANDROID = ROOT / "sdks/android"
APP_ID = "org.xmtp.android.example"
TEST_CLASS = "org.xmtp.android.example.messenger.MessengerPerformanceInstrumentedTest"


def p95(values):
    if len(values) != 30 or any(
        not math.isfinite(value) or value < 0 for value in values
    ):
        raise ValueError("Each query needs 30 finite measured durations")
    return sorted(values)[28]


def validate(report):
    expected = {
        "api": 34,
        "abi": "x86_64",
        "cores": 4,
        "groups": 1000,
        "messages": "100000",
        "heavyMessages": 50000,
        "warmups": 5,
        "measuredRuns": 30,
        "bodyAsciiBytes": 256,
        "visitedTranscripts": 10,
    }
    for key, value in expected.items():
        if report.get(key) != value:
            raise ValueError(f"Invalid {key}: expected {value}, got {report.get(key)}")
    if (
        not report.get("sdkVersion")
        or not isinstance(report.get("seedMs"), int)
        or report["seedMs"] <= 0
    ):
        raise ValueError("Missing SDK version or completed seed duration")
    if not report.get("workloadId"):
        raise ValueError("Missing seeded workload identity")
    if report.get("maxCacheTranscripts") != 3:
        raise ValueError("maxCacheTranscripts must reach the three-transcript bound")
    if report.get("maxTranscriptRows") != 500 or report.get("maxCacheRows") != 1500:
        raise ValueError(
            "The workload must reach maxTranscriptRows and maxCacheRows bounds"
        )
    replay = report.get("startupReplayRows")
    if replay not in (0, 1) or report.get("replayRowsAfterMeasurements") != replay:
        raise ValueError("Seed replay overlapped measurement")
    if report.get("hardware") not in ("ranchu", "goldfish"):
        raise ValueError("The result is not from the fixed emulator")
    if not 3_800_000 <= report.get("memoryKb", 0) <= 4_300_000:
        raise ValueError("The emulator does not have 4 GiB RAM")
    for metric, limit in (("firstMs", 300), ("olderMs", 250), ("listMs", 1000)):
        if p95(report.get(metric, [])) > limit:
            raise ValueError(f"{metric} p95 exceeds {limit} ms")
    for metric, limit in (
        ("heapDeltaBytes", 64 * 1024 * 1024),
        ("maxTranscriptRows", 500),
        ("maxCacheRows", 1500),
        ("maxPageRows", 500),
        ("maxHistoryReadRows", 50),
    ):
        if (
            metric not in report
            or not math.isfinite(report[metric])
            or report[metric] < 0
            or report[metric] > limit
        ):
            raise ValueError(f"{metric} exceeds {limit}")


def command(arguments, **kwargs):
    return subprocess.run(arguments, check=True, text=True, **kwargs)


def result_failures():
    failures = []
    for path in (ANDROID / "example/build/outputs/androidTest-results/connected").rglob(
        "*.xml"
    ):
        tree = ET.parse(path)
        for case in tree.iter("testcase"):
            if case.get("classname") == TEST_CLASS:
                for child in case:
                    if child.tag in ("failure", "error"):
                        failures.append(
                            (child.get("message", "") + " " + (child.text or ""))
                        )
    return failures


def execute(output, label, backend):
    serial = os.environ["ANDROID_SERIAL"]
    invocation = [
        str(ANDROID / "gradlew"),
        "-p",
        str(ANDROID),
        ":example:connectedDebugAndroidTest",
        "-Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true",
        "--dependency-verification=strict",
        "--no-daemon",
        f"-Pandroid.testInstrumentationRunnerArguments.class={TEST_CLASS}",
        "-Pandroid.testInstrumentationRunnerArguments.messengerPerformance=true",
        f"-Pandroid.testInstrumentationRunnerArguments.performanceBackendUrl={backend}",
    ]
    subprocess.run(
        [
            "adb",
            "-s",
            serial,
            "shell",
            "run-as",
            APP_ID,
            "rm",
            "-f",
            "files/messenger-performance/result.json",
        ],
        check=False,
        capture_output=True,
    )
    # Keep live seed progress visible while Gradle waits for instrumentation.
    with (output / f"{label}-logcat.txt").open("w") as device_log:
        logcat = subprocess.Popen(
            [
                "adb",
                "-s",
                serial,
                "logcat",
                "-T",
                "1",
                "-v",
                "time",
                "System.out:I",
                "*:S",
            ],
            text=True,
            stdout=subprocess.PIPE,
        )

        def record_progress():
            for line in logcat.stdout:
                device_log.write(line)
                device_log.flush()
                if "MESSENGER_PERFORMANCE_SEED" in line:
                    print(line.rstrip(), flush=True)

        reader = threading.Thread(target=record_progress, daemon=True)
        reader.start()
        try:
            with (output / f"{label}.log").open("w") as log:
                run = subprocess.run(
                    invocation, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT
                )
        finally:
            logcat.terminate()
            logcat.wait(timeout=10)
            reader.join(timeout=10)
    connected = ANDROID / "example/build/outputs/androidTest-results/connected"
    if connected.exists():
        shutil.copytree(connected, output / f"{label}-connected", dirs_exist_ok=True)
    for name in ("seed-progress", "readiness-progress"):
        progress = subprocess.run(
            [
                "adb",
                "-s",
                serial,
                "shell",
                "run-as",
                APP_ID,
                "cat",
                f"files/messenger-performance/{name}.jsonl",
            ],
            text=True,
            capture_output=True,
        )
        (output / f"{label}-{name}.jsonl").write_text(
            progress.stdout if progress.returncode == 0 else ""
        )
    read = subprocess.run(
        [
            "adb",
            "-s",
            serial,
            "shell",
            "run-as",
            APP_ID,
            "cat",
            "files/messenger-performance/result.json",
        ],
        text=True,
        capture_output=True,
    )
    report = json.loads(read.stdout) if read.returncode == 0 else {}
    (output / f"{label}.json").write_text(json.dumps(report, indent=2) + "\n")
    return run.returncode, report, result_failures()


def run(output, backend, red_control):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("Run this proof on a Linux x86_64 host")
    if not os.environ.get("ANDROID_SERIAL", "").startswith("emulator-"):
        raise ValueError("Start this command in the owned run-test-emulator scope")
    flags = os.environ.get("NIX_ANDROID_EMULATOR_FLAGS", "")
    if not re.search(r"(?:^|\s)-cores\s+4(?:\s|$)", flags) or not re.search(
        r"(?:^|\s)-memory\s+4096(?:\s|$)", flags
    ):
        raise ValueError("Set emulator flags -cores 4 -memory 4096")
    output.mkdir(parents=True, exist_ok=True)
    environment = {
        "host": platform.uname()._asdict(),
        "emulatorFlags": flags,
        "commit": command(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True
        ).stdout.strip(),
        "emulatorVersion": command(
            ["emulator", "-version"], capture_output=True
        ).stdout,
        "cpu": Path("/proc/cpuinfo").read_text(),
        "hostMemory": Path("/proc/meminfo").read_text(),
    }
    (output / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
    code, report, failures = execute(output, "green", backend)
    if code or failures:
        raise ValueError(
            "The device performance test failed. Read green.log and the test XML."
        )
    validate(report)
    if red_control:
        cache_red_control(output, backend, report)
    print(f"Performance proof saved to {output}")


def remove_published_cache_eviction(original):
    prefix, marker, cache = original.partition("class SDKTranscriptCache<")
    if not marker:
        raise ValueError("Cannot identify production SDKTranscriptCache")
    weakened, count = re.subn(
        r"while\s*\(entries\.size\s*>\s*3\)\s*entries\.remove\(entries\.keys\.first\(\)\)",
        "// The performance red control removes production cache eviction.",
        cache,
    )
    if count != 1:
        raise ValueError("Cannot identify the production cache eviction statement")
    return prefix + marker + weakened


def cache_red_control(output, backend, report):
    source = (
        ANDROID
        / "example/src/main/java/org/xmtp/android/example/messenger/SDKHistoryPages.kt"
    )
    original = source.read_text()
    weakened = remove_published_cache_eviction(original)
    (output / "red-cache.patch").write_text(
        "".join(
            difflib.unified_diff(
                original.splitlines(keepends=True),
                weakened.splitlines(keepends=True),
                fromfile=str(source),
                tofile=str(source),
            )
        )
    )
    try:
        source.write_text(weakened)
        code, red, failures = execute(output, "red-cache", backend)
        if (
            not code
            or red.get("maxCacheRows", 0) <= 1500
            or red.get("workloadId") != report["workloadId"]
            or not any(
                "Transcript cache trimming was removed" in value for value in failures
            )
        ):
            raise ValueError(
                "The broken production cache did not fail the required device assertion"
            )
    finally:
        source.write_text(original)
    code, restored, failures = execute(output, "restored", backend)
    if code or failures:
        raise ValueError("The restored performance test failed")
    validate(restored)
    if restored["workloadId"] != report["workloadId"]:
        raise ValueError("The restored run did not reuse the same dataset")


def main():
    def interrupted(number, frame):
        raise SystemExit(128 + number)

    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--backend", required=True)
    parser.add_argument("--red-control", action="store_true")
    arguments = parser.parse_args()
    try:
        run(arguments.output.resolve(), arguments.backend, arguments.red_control)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Performance proof failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
