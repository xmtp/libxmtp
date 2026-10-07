#!/usr/bin/env python3
"""Record real checkout, CPU, task, result, test and coverage evidence per job."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import time
import xml.etree.ElementTree as ET


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def entries(revision):
    result = {}
    raw = subprocess.check_output(["git", "ls-tree", "-rz", revision])
    for record in raw.split(b"\0"):
        if record:
            metadata, path = record.split(b"\t", 1)
            mode, kind, blob = metadata.decode().split()
            result[path.decode()] = {"mode": mode, "kind": kind, "blob": blob}
    return result


def check_source(manifest):
    checkout = git("rev-parse", "HEAD")
    checkout_tree = git("rev-parse", "HEAD^{tree}")
    if os.environ.get("GITHUB_SHA", checkout) != checkout:
        raise ValueError("Actual GITHUB_SHA and checkout differ")
    if (
        git("rev-parse", f"{manifest['frozen_source_sha']}^{{tree}}")
        != manifest["frozen_source_tree_hash"]
    ):
        raise ValueError("Original frozen source object is missing or changed")
    actual = entries("HEAD")
    # The manifest excludes itself from this self-reference-free tree receipt.
    actual.pop("dev/ci/benchmark-overlay.json", None)
    if digest(actual) != manifest["effective_entries_sha256"]:
        raise ValueError("Effective graph/source tree differs from prepared overlay")
    changed = git("diff", "HEAD", "--name-only")
    if changed:
        raise ValueError(f"Tracked source files changed during this job: {changed}")
    return checkout, checkout_tree


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def junit(path):
    document = ET.parse(path).getroot()
    ids, failures = [], []
    for case in document.iter("testcase"):
        identity = (
            f"{case.get('classname', case.get('file', ''))}::{case.get('name', '')}"
        )
        if not case.get("name"):
            raise ValueError("JUnit testcase has no ID")
        if case.find("skipped") is None:
            ids.append(identity)
        if case.find("failure") is not None or case.find("error") is not None:
            failures.append(identity)
    return {
        "test_ids": sorted(set(ids)),
        "failures": failures,
        "case_records": len(ids),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }


def report_files(root):
    # Keep original bytes. Do not create test IDs or retry counts from job status.
    patterns = (
        "target/ci-benchmark/native-reports/**/*.json",
        "target/ci-benchmark/native-reports/**/*.xml",
        "target/ci-benchmark/native-reports/**/*.info",
        "target/ci-benchmark/native-reports/**/*.profraw",
        "target/llvm-cov/lcov.info",
        "target/coverage/lcov.info",
        "target/junit*.xml",
        "target/ci-products/*.manifest.json",
        "target/sdk-artifacts/artifacts.json",
        "target/nextest/**/junit.xml",
        "workspace-tests/junit.xml",
        "workspace-tests/coverage",
        "wasm-tests/coverage",
    )
    return sorted(
        {path for pattern in patterns for path in root.glob(pattern) if path.is_file()}
    )


def public_inputs():
    raw = json.loads(os.environ.get("BENCHMARK_INPUTS") or "{}") or {}
    safe = {
        "kind",
        "target",
        "selection",
        "prepared-products",
        "ref",
        "profile",
        "arch",
        "shard",
        "run_swift",
        "release-build",
        "dry-run",
    }
    return {key: value for key, value in raw.items() if key in safe}


def start(args, manifest, output):
    checkout, checkout_tree = check_source(manifest)
    count = os.cpu_count()
    if not isinstance(count, int) or count <= 0:
        raise ValueError("Actual CPU count is unavailable")
    observed = {"logical_cpu_count": count}
    if hasattr(os, "sched_getaffinity"):
        observed["affinity_cpu_count"] = len(os.sched_getaffinity(0))
    blocked = None
    for workflow, reason in manifest.get("blocked_workflows", {}).items():
        if args.task.startswith(workflow + "-"):
            blocked = f"{workflow}: {reason}"
    task = manifest.get("task_specs", {}).get(args.task)
    if not task:
        raise ValueError("Task/profile identity is absent from the generated graph")
    value = {
        "schema_version": 2,
        "run_id": os.environ.get("GITHUB_RUN_ID"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "task_id": args.task,
        "job_key": os.environ.get("GITHUB_JOB"),
        "runner_name": os.environ.get("RUNNER_NAME"),
        "matrix": json.loads(os.environ.get("BENCHMARK_MATRIX") or "{}"),
        "inputs": public_inputs(),
        "actual_event": os.environ.get("GITHUB_EVENT_NAME"),
        "started_at": utc(),
        "monotonic_start": time.monotonic(),
        "cpu_observation": observed,
        "platform": {"system": platform.system(), "machine": platform.machine()},
        "frozen_source_sha": manifest["frozen_source_sha"],
        "frozen_source_tree_hash": manifest["frozen_source_tree_hash"],
        "checkout_sha": checkout,
        "checkout_tree_hash": checkout_tree,
        "workflow_sha": manifest["workflow_sha"],
        "graph_overlay_sha256": manifest["graph_overlay_sha256"],
        "application_payload_sha256": manifest["application_payload_sha256"],
        "overlay_boundary_sha256": manifest["overlay_boundary_sha256"],
        "frozen_identity_sha256": manifest.get("frozen_identity_sha256"),
        "owner_inventory_sha": manifest.get("expected_owner_inventory_sha256"),
        "task_profile": task,
        "task_profile_sha256": digest(
            {
                "definition": task,
                "inputs": public_inputs(),
                "matrix": json.loads(os.environ.get("BENCHMARK_MATRIX") or "{}"),
            }
        ),
        "blocked_reason": blocked,
        "source_evidence": {
            "kind": "Git checkout/tree and prepared overlay blob inventory",
            "manifest_sha256": hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
        },
    }
    output.mkdir(parents=True, exist_ok=True)
    path = output / "start.json"
    with path.open("x") as stream:
        stream.write(json.dumps(value, sort_keys=True, indent=2) + "\n")
    # Store the authentic event; downstream mapping must name its real parent.
    event = os.environ.get("GITHUB_EVENT_PATH")
    if event:
        shutil.copyfile(event, output / "event.json")
    destination = os.environ.get("GITHUB_OUTPUT")
    if destination:
        with open(destination, "a") as stream:
            stream.write(f"receipt-id={output.name}\n")
    if blocked:
        raise ValueError(blocked)


def finish(args, manifest, output):
    started = json.loads((output / "start.json").read_text())
    reasons = [started["blocked_reason"]] if started.get("blocked_reason") else []
    try:
        checkout, checkout_tree = check_source(manifest)
        if (checkout, checkout_tree) != (
            started["checkout_sha"],
            started["checkout_tree_hash"],
        ):
            raise ValueError("Job checkout changed after start")
    except (ValueError, subprocess.CalledProcessError) as error:
        reasons.append(str(error))
    evidence = []
    ids = set()
    retries = []
    suites = set()
    check_ids = set()
    cache = []
    for path in report_files(Path.cwd()):
        relative = str(path.relative_to(Path.cwd()))
        target = output / "native-reports" / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
        record = {
            "path": relative,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
        if path.suffix == ".xml":
            parsed = junit(path)
            record["junit"] = parsed
            ids.update(parsed["test_ids"])
        elif path.suffix == ".json":
            value = json.loads(path.read_text())
            # Reporters must explicitly provide retry data. JUnit alone does
            # not prove a zero retry count for Vitest or nextest.
            if (
                value.get("schema_version") == 1
                and value.get("kind") == "executed-task-report"
            ):
                if value.get("task_id") != args.task:
                    reasons.append("Native reporter task ID differs")
                    continue
                execution = value.get("execution", {})
                expected_execution = {
                    "run_id": started["run_id"],
                    "run_attempt": started["run_attempt"],
                    "checkout_sha": started["checkout_sha"],
                    "workflow_sha": started["workflow_sha"],
                    "task_profile_sha256": started["task_profile_sha256"],
                }
                if any(
                    str(execution.get(key)) != str(expected)
                    for key, expected in expected_execution.items()
                ):
                    reasons.append(
                        "Native result has no current execution/source/profile receipt"
                    )
                    continue
                begin, end = execution.get("started_at"), execution.get("completed_at")
                try:
                    begin_time = datetime.datetime.fromisoformat(
                        begin.replace("Z", "+00:00")
                    )
                    end_time = datetime.datetime.fromisoformat(
                        end.replace("Z", "+00:00")
                    )
                    job_start = datetime.datetime.fromisoformat(started["started_at"])
                    current = datetime.datetime.now(datetime.timezone.utc)
                    if not job_start <= begin_time <= end_time <= current:
                        raise ValueError("Native execution times are outside this job")
                except (ValueError, TypeError, AttributeError):
                    reasons.append(
                        "Native result does not prove fresh execution in this job"
                    )
                    continue
                count = value.get("test_level_retries")
                if (
                    isinstance(count, int)
                    and not isinstance(count, bool)
                    and count >= 0
                ):
                    retries.append(count)
                ids.update(value.get("executed_test_ids", []))
                suites.update(value.get("selected_test_suites", []))
                check_ids.update(value.get("executed_check_ids", []))
                hits, requests = value.get("cache_hits"), value.get("cache_requests")
                if (
                    isinstance(hits, int)
                    and not isinstance(hits, bool)
                    and isinstance(requests, int)
                    and not isinstance(requests, bool)
                    and 0 <= hits <= requests
                ):
                    cache.append((hits, requests))
        evidence.append(record)
    if not retries:
        reasons.append("Native test-level retry evidence is absent")
    if not check_ids:
        reasons.append("Native executed check/rule inventory is absent")
    if not cache:
        reasons.append("Cache hit/request evidence is absent")
    result = dict(
        started,
        completed_at=utc(),
        elapsed_observation_seconds=time.monotonic() - started["monotonic_start"],
        job_status=os.environ.get("BENCHMARK_JOB_STATUS"),
        selected_checks_complete=os.environ.get("BENCHMARK_JOB_STATUS") == "success"
        and not reasons,
        executed_check_ids=sorted(check_ids),
        executed_test_ids=sorted(ids),
        selected_test_suites=sorted(suites),
        test_level_retries=sum(retries) if retries else None,
        cache_hits=sum(pair[0] for pair in cache) if cache else None,
        cache_requests=sum(pair[1] for pair in cache) if cache else None,
        native_reports=evidence,
        evidence_status="UNVERIFIED" if reasons else "RECORDED",
        unverified_reasons=reasons,
    )
    # Cost and gate times come from real GitHub run/job metadata. This observed
    # span is diagnostic; it excludes checkout and never becomes a gate time.
    result.pop("monotonic_start", None)
    (output / "receipt.json").write_text(
        json.dumps(result, sort_keys=True, indent=2) + "\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("start", "finish"))
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--task", required=True)
    args = parser.parse_args()
    try:
        manifest = json.loads(args.manifest.read_text())
        context = {
            "matrix": json.loads(os.environ.get("BENCHMARK_MATRIX") or "{}"),
            "inputs": public_inputs(),
        }
        output = Path("target/ci-benchmark") / f"{args.task}-{digest(context)[:12]}"
        if args.command == "start":
            start(args, manifest, output)
        else:
            finish(args, manifest, output)
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"benchmark receipt: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
