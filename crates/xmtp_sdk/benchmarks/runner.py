#!/usr/bin/env python3
"""Run paired measurements through installed-package host adapters."""

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
from ios_cleanup import terminate_registered
from benchmark_stats import LIMIT, MIN_PAIRS, paired_summary, percentile

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


def write_json(path, value):
    Path(path).write_bytes(canonical(value) + b"\n")


def positive(value):
    return type(value) in (float, int) and math.isfinite(value) and value > 0


def validate_config(config):
    if config.get("schema") != 1 or config.get("target") not in TARGETS:
        raise ValueError("Expected schema 1 and a supported target")
    if config.get("purpose") not in {"release", "harness-control"}:
        raise ValueError("Declare release or harness-control purpose")
    if type(config.get("pairs")) is not int or config["pairs"] < MIN_PAIRS:
        raise ValueError("At least 20 pairs are required")
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
    if config.get("enrichment") != dataset()["enrichment"]:
        raise ValueError("Use the complete fixture enrichment on both packages")
    for side in ("old", "new"):
        package = config[side]
        for key in ("version", "commit", "compiler", "production_flags", "provenance"):
            if not isinstance(package.get(key), str) or not package[key].strip():
                raise ValueError(f"Missing {side} {key}")
        if package.get("profile") != "release" or package.get("public_api") is not True:
            raise ValueError("Both packages must use installed public release APIs")
        if not package.get("command") or not all(
            isinstance(x, str) and x for x in package["command"]
        ):
            raise ValueError("Each host adapter command must be an argv array")
        if not isinstance(package["command"], list):
            raise ValueError("Commands must not be shell strings")
        if not package.get("adapter_sources"):
            raise ValueError("Record each adapter source file")
    if config["old"].get("published") is not True:
        raise ValueError("Use a published old package, not a local development build")
    if config["new"].get("integrated_head") is not True:
        raise ValueError("The new package must identify its integrated source head")


def invoke(config, side, request, output):
    package = config[side]
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
        if request["target"] == "swift":
            terminate_registered(request)
        raise ValueError(
            f"Adapter timeout: {side} {request['phase']} {request.get('workload')}"
        )
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


def validate_mobile(row):
    response = row["response"]
    if row["workload"] == "mobile_lift" and row["side"] == "new":
        mobile = response.get("mobile_lift", {})
        if not all(positive(mobile.get(key)) for key in ("record_ms", "class_ms")):
            raise ValueError("Mobile runs must measure class and record lifts")
        order = ["record", "class"] if row["pair"] % 2 == 0 else ["class", "record"]
        if mobile.get("order") != order:
            raise ValueError("Alternate mobile record/class order")


def summarize(ledger):
    config = ledger["config"]
    validate_config(config)
    fixture = dataset(config["target"])
    if ledger.get("fixture_sha256") != digest(fixture):
        raise ValueError("Dataset hash mismatch")
    rows = ledger["samples"]
    expected = [
        (w, i, s)
        for w in WORKLOADS
        for i in range(config["pairs"])
        for s in (("old", "new") if i % 2 == 0 else ("new", "old"))
    ]
    actual = [(r["workload"], r["pair"], r["side"]) for r in rows]
    if actual != expected:
        raise ValueError("Missing, duplicate, or out-of-order measurement pairs")
    for row in rows:
        validate_measurement(row, fixture, config["target"])
    results = []
    safety_failures = []
    for row in rows:
        flags = row["response"]["safety"]
        if flags["correctness"] is False or any(flags[k] is True for k in SAFETY[1:]):
            safety_failures.append(
                {
                    "workload": row["workload"],
                    "pair": row["pair"],
                    "side": row["side"],
                    "outcomes": flags,
                }
            )
    for workload in WORKLOADS:
        samples = {
            side: [
                r["response"]
                for r in rows
                if r["workload"] == workload and r["side"] == side
            ]
            for side in ("old", "new")
        }
        metrics = [
            ("duration_ms", "build" if workload.startswith("build_") else "latency"),
            ("peak_memory_bytes", "memory"),
        ]
        if workload == "stream":
            for values in samples.values():
                for sample in values:
                    sample["messages_per_second"] = 10000 * 1000 / sample["duration_ms"]
            metrics.append(("messages_per_second", "throughput"))
        for metric, kind in metrics:
            value = paired_summary(
                [r[metric] for r in samples["old"]],
                [r[metric] for r in samples["new"]],
                kind,
            )
            results.append({"workload": workload, "metric": metric, **value})
    if config["target"] in {"swift", "kotlin"}:
        mobile = ledger.get("mobile_samples", [])
        if [(r["workload"], r["pair"], r["side"]) for r in mobile] != [
            ("mobile_lift", i, "new") for i in range(config["pairs"])
        ]:
            raise ValueError("Missing or out-of-order mobile conversion samples")
        for row in mobile:
            validate_mobile(row)
        results.append(
            {
                "workload": "mobile_class_over_record",
                "metric": "duration_ms",
                **paired_summary(
                    [r["response"]["mobile_lift"]["record_ms"] for r in mobile],
                    [r["response"]["mobile_lift"]["class_ms"] for r in mobile],
                    "mobile",
                ),
            }
        )
    for metric in ("raw_bytes", "compressed_bytes"):
        old, new = (ledger["packages"][side][metric] for side in ("old", "new"))
        if not positive(old) or not positive(new):
            raise ValueError("Invalid package size")
        results.append(
            {
                "workload": "complete_package",
                "metric": metric,
                "ratio": new / old,
                "old": old,
                "new": new,
                "decision": "FAIL" if new / old > LIMIT else "RECORDED",
            }
        )
    failed = safety_failures or any(r["decision"] == "FAIL" for r in results)
    unmeasured = sorted(
        {
            key
            for row in rows
            for key, value in row["response"]["safety"].items()
            if value is None
        }
    )
    report = {
        "schema": 1,
        "target": config["target"],
        "purpose": config["purpose"],
        "performance_decision": "FAIL" if failed else "PASS",
        "release_gate": "PENDING",
        "results": results,
        "safety_failures": safety_failures,
        "safety_decision": "FAIL"
        if safety_failures
        else "PENDING"
        if unmeasured
        else "PASS",
        "unmeasured_safety": unmeasured,
        "eager_snapshot_gate": "PENDING",
        "pending": [
            "Independent callback matrix and installed-package proof review",
            "Eager reaction snapshot completeness needs a deterministic public boundary",
            "Review matched package hashes and integrated source provenance",
        ],
        "method": {
            "resamples": 10000,
            "seed": 731,
            "interval": "percentile 95%",
            "statistic": "new median / old median",
            "sampling_unit": "old/new pair",
        },
    }
    if config["purpose"] == "harness-control":
        report["pending"].append("Synthetic controls are not release measurements")
    if config["target"] == "browser":
        report["browser_long_tasks"] = [
            {
                "workload": w,
                "side": s,
                "count_p50": percentile(
                    [
                        len(r["response"]["long_tasks_ms"])
                        for r in rows
                        if r["workload"] == w and r["side"] == s
                    ],
                    0.5,
                ),
                "durations_ms": [
                    d
                    for r in rows
                    if r["workload"] == w and r["side"] == s
                    for d in r["response"]["long_tasks_ms"]
                ],
            }
            for w in WORKLOADS
            for s in ("old", "new")
        ]
    return report


def markdown(report):
    lines = [
        "# Release benchmark report",
        "",
        f"Target: {report['target']}.",
        f"Performance checks: {report['performance_decision']}. Release gate: PENDING.",
        f"Purpose: {report['purpose']}.",
        "",
        "| Workload | Metric | Ratio | 95% interval | Result |",
        "| --- | --- | ---: | --- | --- |",
    ]
    for row in report["results"]:
        ci = row.get("ratio_ci95")
        interval = f"{ci[0]:.4f}–{ci[1]:.4f}" if ci else "direct size ratio"
        lines.append(
            f"| {row['workload']} | {row['metric']} | {row['ratio']:.4f} | {interval} | {row['decision']} |"
        )
    lines += [
        "",
        "p50, p95, all raw pairs, package inventories, and environment metadata are in the JSON files.",
        "",
        "Pending:",
        "",
        *[f"- {item}." for item in report["pending"]],
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
    packages = {
        side: inventory(config[side]["root"], config[side]["assets"], config["target"])
        for side in ("old", "new")
    }
    sources = {
        side: {
            str(Path(p).resolve()): hashlib.sha256(Path(p).read_bytes()).hexdigest()
            for p in config[side]["adapter_sources"]
        }
        for side in ("old", "new")
    }
    ledger = {
        "schema": 1,
        "config": config,
        "fixture_sha256": digest(fixture),
        "packages": packages,
        "adapter_sources": sources,
        "samples": [],
        "mobile_samples": [],
        "runner": {"python": sys.version, "platform": platform.platform()},
        "setup": {},
    }
    write_json(output / "ledger.json", ledger)
    try:
        for side in ("old", "new"):
            request = {
                "phase": "setup",
                "target": config["target"],
                "side": side,
                "fixture": str(output / "fixture.json"),
                "fixture_sha256": digest(fixture),
                "package_root": str(Path(config[side]["root"]).resolve()),
                "package_sha256": packages[side]["sha256"],
                "state_directory": str(output / side),
            }
            ledger["setup"][side] = invoke(
                config, side, request, output / f"setup-{side}"
            )
            if ledger["setup"][side].get("ready") is not True:
                raise ValueError("Adapter fixture setup is not ready")
        for workload in WORKLOADS:
            for pair in range(config["pairs"]):
                for side in ("old", "new") if pair % 2 == 0 else ("new", "old"):
                    request = {
                        "phase": "reset",
                        "target": config["target"],
                        "side": side,
                        "pair": pair,
                        "workload": workload,
                        "fixture_sha256": digest(fixture),
                        "package_sha256": packages[side]["sha256"],
                        "package_root": str(Path(config[side]["root"]).resolve()),
                        "state_directory": str(output / side),
                        "callback_delay_ms": fixture["callback_delay_ms"],
                    }
                    name = f"{workload}-{pair:03d}-{side}"
                    reset = invoke(config, side, request, output / f"{name}-reset")
                    if reset.get("ready") is not True:
                        raise ValueError("Adapter reset is not ready")
                    request["phase"] = "measure"
                    response = invoke(config, side, request, output / name)
                    row = {
                        "workload": workload,
                        "pair": pair,
                        "side": side,
                        "response": response,
                    }
                    ledger["samples"].append(row)
                    write_json(output / "ledger.json", ledger)
                    validate_measurement(row, fixture, config["target"])
        if config["target"] in {"swift", "kotlin"}:
            for pair in range(config["pairs"]):
                request = {
                    "phase": "measure",
                    "target": config["target"],
                    "side": "new",
                    "pair": pair,
                    "workload": "mobile_lift",
                    "fixture_sha256": digest(fixture),
                    "package_sha256": packages["new"]["sha256"],
                    "package_root": str(Path(config["new"]["root"]).resolve()),
                    "state_directory": str(output / "new"),
                }
                response = invoke(
                    config, "new", request, output / f"mobile-lift-{pair:03d}"
                )
                row = {
                    "workload": "mobile_lift",
                    "pair": pair,
                    "side": "new",
                    "response": response,
                }
                ledger["mobile_samples"].append(row)
                write_json(output / "ledger.json", ledger)
                validate_mobile(row)
        for side in ("old", "new"):
            if packages[side] != inventory(
                config[side]["root"], config[side]["assets"], config["target"]
            ):
                raise ValueError("Installed package changed during measurements")
            if sources[side] != {
                str(Path(p).resolve()): hashlib.sha256(Path(p).read_bytes()).hexdigest()
                for p in config[side]["adapter_sources"]
            }:
                raise ValueError("Adapter source changed during measurements")
        report = summarize(ledger)
        write_json(output / "report.json", report)
        (output / "report.md").write_text(markdown(report))
        return 1 if report["performance_decision"] == "FAIL" else 0
    except Exception as error:
        write_json(
            output / "error.json", {"release_gate": "PENDING", "error": str(error)}
        )
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
        return 1 if report["performance_decision"] == "FAIL" else 0
    return run(args.config, args.output, None if args.action == "run" else args.action)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, KeyError, OSError) as error:
        print(f"Benchmark failed: {error}", file=sys.stderr)
        sys.exit(2)
