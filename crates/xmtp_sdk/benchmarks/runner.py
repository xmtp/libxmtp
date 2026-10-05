#!/usr/bin/env python3
"""Run measurements through an installed-package host adapter."""

import argparse
import hashlib
import json
import math
import os
import platform
import signal
import subprocess
import sys
from pathlib import Path

from fixtures import (
    canonical,
    dataset,
    digest,
    expected_observation,
    expected_stream_counts,
)
from packages import inventory

TARGETS = ("swift", "kotlin", "node", "browser")
WORKLOADS = (
    "cold_start",
    "page",
    "stream",
    "callback_immediate",
    "callback_slow",
    "build_clean",
    "build_warm",
)
SAFETY = ("correctness", "deadlock", "use_after_end", "retained_growth")


def percentile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    left = math.floor(position)
    right = math.ceil(position)
    return ordered[left] + (ordered[right] - ordered[left]) * (position - left)


def spread(values):
    return {
        "samples": len(values),
        "p50": percentile(values, 0.5),
        "p95": percentile(values, 0.95),
    }


def write_json(path, value):
    Path(path).write_bytes(canonical(value) + b"\n")


def positive(value):
    return type(value) in (float, int) and math.isfinite(value) and value > 0


def validate_config(config):
    if config.get("schema") != 1 or config.get("target") not in TARGETS:
        raise ValueError("Expected schema 1 and a supported target")
    if type(config.get("samples")) is not int or config["samples"] < 1:
        raise ValueError("Set a positive sample count")
    if not positive(config.get("timeout_seconds")):
        raise ValueError("Set a finite adapter timeout")
    for key in (
        "runner_class",
        "os",
        "hardware",
        "runtime",
        "clean_build_policy",
        "warm_build_policy",
        "memory_scope",
        "cache_policy",
    ):
        if not isinstance(config.get(key), str) or not config[key].strip():
            raise ValueError(f"Missing {key}")
    package = config["package"]
    for key in ("version", "commit", "compiler", "production_flags"):
        if not isinstance(package.get(key), str) or not package[key].strip():
            raise ValueError(f"Missing package {key}")
    if package.get("profile") != "release" or package.get("public_api") is not True:
        raise ValueError("The package must use installed public release APIs")
    if not package.get("command") or not all(
        isinstance(x, str) and x for x in package["command"]
    ):
        raise ValueError("The host adapter command must be an argv array")
    if not isinstance(package["command"], list):
        raise ValueError("Commands must not be shell strings")
    if not package.get("adapter_sources"):
        raise ValueError("Record each adapter source file")


def invoke(config, request, output):
    package = config["package"]
    env = dict(os.environ, NODE_ENV="production")
    process = subprocess.Popen(
        package["command"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(
            json.dumps(request), timeout=config["timeout_seconds"]
        )
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        stdout, stderr = process.communicate()
        output.with_suffix(".stdout").write_text(stdout)
        output.with_suffix(".stderr").write_text(stderr)
        raise ValueError(f"Adapter timeout: {request['phase']} {request.get('workload')}")
    output.with_suffix(".stdout").write_text(stdout)
    output.with_suffix(".stderr").write_text(stderr)
    if process.returncode:
        raise ValueError(f"Adapter exit {process.returncode}: {output}")
    result = json.loads(stdout)
    if result.get("request_sha256") != digest(request):
        raise ValueError("Adapter response does not identify this request")
    return result


def validate_measurement(row, fixture, target):
    response = row["response"]
    if response.get("observation") != expected_observation(fixture, row["workload"]):
        raise ValueError("Incorrect or incomplete observed public values")
    if target == "browser" and row["workload"] == "stream":
        primary, events = expected_stream_counts(fixture)
        if (response.get("streamed_primary"), response.get("streamed_events")) != (
            primary,
            events,
        ):
            raise ValueError("Incorrect Browser stream event counts")
    if set(response.get("safety", {})) != set(SAFETY):
        raise ValueError("Missing independent correctness or lifetime outcome")
    if any(
        value is not None and type(value) is not bool
        for value in response["safety"].values()
    ):
        raise ValueError(
            "Safety outcomes must be booleans or null for unmeasured outcomes"
        )
    for key in ("duration_ms", "peak_memory_bytes"):
        if not positive(response.get(key)):
            raise ValueError(f"Invalid {key}")
    if row["workload"] == "stream" and not positive(response.get("streamed_events")):
        raise ValueError("Invalid streamed_events")
    if (
        row["workload"] == "callback_slow"
        and response["duration_ms"] < fixture["callback_delay_ms"]
    ):
        raise ValueError("Slow callback returned before the controlled barrier delay")
    if target == "browser":
        tasks = response.get("long_tasks_ms")
        if not isinstance(tasks, list) or any(
            not positive(v) or v <= 50 for v in tasks
        ):
            raise ValueError("Record every browser task strictly above 50 ms")


def summarize(ledger):
    config = ledger["config"]
    validate_config(config)
    fixture = dataset(config["target"])
    if ledger.get("fixture_sha256") != digest(fixture):
        raise ValueError("Dataset hash mismatch")
    rows = ledger["samples"]
    expected = [(w, i) for w in WORKLOADS for i in range(config["samples"])]
    if [(r["workload"], r["sample"]) for r in rows] != expected:
        raise ValueError("Missing, duplicate, or out-of-order measurements")
    for row in rows:
        validate_measurement(row, fixture, config["target"])
    safety_failures = []
    for row in rows:
        flags = row["response"]["safety"]
        if flags["correctness"] is False or any(flags[k] is True for k in SAFETY[1:]):
            safety_failures.append(
                {
                    "workload": row["workload"],
                    "sample": row["sample"],
                    "outcomes": flags,
                }
            )
    results = []
    for workload in WORKLOADS:
        samples = [r["response"] for r in rows if r["workload"] == workload]
        metrics = ["duration_ms", "peak_memory_bytes"]
        if workload == "stream":
            for sample in samples:
                sample["messages_per_second"] = (
                    sample["streamed_events"] * 1000 / sample["duration_ms"]
                )
            metrics.append("messages_per_second")
        for metric in metrics:
            results.append(
                {
                    "workload": workload,
                    "metric": metric,
                    **spread([sample[metric] for sample in samples]),
                }
            )
    package = ledger["package"]
    for metric in ("raw_bytes", "compressed_bytes"):
        if not positive(package[metric]):
            raise ValueError("Invalid package size")
        results.append(
            {"workload": "complete_package", "metric": metric, "value": package[metric]}
        )
    report = {
        "schema": 2,
        "target": config["target"],
        "results": results,
        "safety_failures": safety_failures,
        "unmeasured_safety": sorted(
            {
                key
                for row in rows
                for key, value in row["response"]["safety"].items()
                if value is None
            }
        ),
    }
    if config["target"] == "browser":
        report["browser_long_tasks"] = [
            {
                "workload": w,
                "count_p50": percentile(
                    [
                        len(r["response"]["long_tasks_ms"])
                        for r in rows
                        if r["workload"] == w
                    ],
                    0.5,
                ),
                "durations_ms": [
                    d
                    for r in rows
                    if r["workload"] == w
                    for d in r["response"]["long_tasks_ms"]
                ],
            }
            for w in WORKLOADS
        ]
    return report


def markdown(report):
    lines = [
        "# Benchmark report",
        "",
        f"Target: {report['target']}.",
        "",
        "| Workload | Metric | p50 | p95 |",
        "| --- | --- | ---: | ---: |",
    ]
    for row in report["results"]:
        if "value" in row:
            lines.append(
                f"| {row['workload']} | {row['metric']} | {row['value']} | {row['value']} |"
            )
        else:
            lines.append(
                f"| {row['workload']} | {row['metric']} | {row['p50']:.4f} | {row['p95']:.4f} |"
            )
    lines += [
        "",
        "Raw samples, the package inventory, and environment metadata are in the JSON files.",
        "",
    ]
    return "\n".join(lines)


def run(config_path, output, target=None):
    config = json.loads(Path(config_path).read_text())
    validate_config(config)
    if target and target != config["target"]:
        raise ValueError("Target entry point does not match the configuration")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = dataset(config["target"])
    write_json(output / "fixture.json", fixture)
    package = config["package"]
    inventory_value = inventory(package["root"], package["assets"], config["target"])
    sources = {
        str(Path(p).resolve()): hashlib.sha256(Path(p).read_bytes()).hexdigest()
        for p in package["adapter_sources"]
    }
    ledger = {
        "schema": 2,
        "config": config,
        "fixture_sha256": digest(fixture),
        "package": inventory_value,
        "adapter_sources": sources,
        "samples": [],
        "runner": {"python": sys.version, "platform": platform.platform()},
    }
    write_json(output / "ledger.json", ledger)
    state = output / "state"
    base = {
        "target": config["target"],
        "fixture_sha256": digest(fixture),
        "package_root": str(Path(package["root"]).resolve()),
        "package_sha256": inventory_value["sha256"],
        "state_directory": str(state),
    }
    try:
        request = {**base, "phase": "setup", "fixture": str(output / "fixture.json")}
        ledger["setup"] = invoke(config, request, output / "setup")
        if ledger["setup"].get("ready") is not True:
            raise ValueError("Adapter fixture setup is not ready")
        for workload in WORKLOADS:
            for sample in range(config["samples"]):
                request = {
                    **base,
                    "phase": "reset",
                    "sample": sample,
                    "workload": workload,
                    "callback_delay_ms": fixture["callback_delay_ms"],
                }
                name = f"{workload}-{sample:03d}"
                reset = invoke(config, request, output / f"{name}-reset")
                if reset.get("ready") is not True:
                    raise ValueError("Adapter reset is not ready")
                request["phase"] = "measure"
                response = invoke(config, request, output / name)
                row = {"workload": workload, "sample": sample, "response": response}
                ledger["samples"].append(row)
                write_json(output / "ledger.json", ledger)
                validate_measurement(row, fixture, config["target"])
        report = summarize(ledger)
        write_json(output / "report.json", report)
        (output / "report.md").write_text(markdown(report))
        return 1 if report["safety_failures"] else 0
    except Exception as error:
        write_json(output / "error.json", {"error": str(error)})
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    for target in ("run", *TARGETS):
        command = sub.add_parser(target)
        command.add_argument("config")
        command.add_argument("output")
    command = sub.add_parser("analyze")
    command.add_argument("ledger")
    command.add_argument("output")
    command = sub.add_parser("fixture")
    command.add_argument("output")
    command.add_argument("--target", choices=TARGETS)
    args = parser.parse_args()
    if args.action == "fixture":
        write_json(args.output, dataset(args.target))
        return 0
    if args.action == "analyze":
        report = summarize(json.loads(Path(args.ledger).read_text()))
        write_json(args.output, report)
        return 1 if report["safety_failures"] else 0
    return run(args.config, args.output, None if args.action == "run" else args.action)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, KeyError, OSError) as error:
        print(f"Benchmark failed: {error}", file=sys.stderr)
        sys.exit(2)
