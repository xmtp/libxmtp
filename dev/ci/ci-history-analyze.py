#!/usr/bin/env python3
"""Analyze complete CI allocation and a fixed hosted old/candidate sample.

Unknown capacities, missing attempts, unmapped workflow_run events, absent
runtime evidence, and incomplete pairs produce UNVERIFIED acceptance. Historical
lower bounds remain useful evidence. They are never a timing or cost proof.
The input is a new collector snapshot; the legacy history layout is also readable.
"""

import argparse
import collections
import datetime
import importlib.util
import hashlib
import json
import pathlib
import re
import statistics
import subprocess
import sys


def read_json(path, default=None):
    return json.loads(path.read_text()) if path.exists() else default


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def timestamp(value):
    return datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))


def seconds(start, end):
    if not start or not end:
        return None
    value = (timestamp(end) - timestamp(start)).total_seconds()
    return value if value >= 0 else None


def stats(values):
    if not values:
        return {"n": 0, "median": None, "p90": None}
    ordered = sorted(values)
    return {
        "n": len(values),
        "median": round(statistics.median(values), 4),
        "p90": round(ordered[round((len(ordered) - 1) * 0.9)], 4),
    }


def capacity(labels, records):
    """Use an observed/configured count or an explicit Nvcpu allocation label."""
    for label in labels:
        record = records.get(label)
        if isinstance(record, dict):
            count = record.get("cores")
            evidence = record.get("evidence", {})
            if (
                isinstance(count, (int, float))
                and not isinstance(count, bool)
                and count > 0
                and evidence.get("kind")
                in {"runner-observation", "runner-configuration"}
                and evidence.get("source")
            ):
                return count, evidence
        match = re.fullmatch(r"blacksmith-(\d+)vcpu-.*", label)
        if match and int(match[1]) > 0:
            return int(match[1]), {"kind": "explicit-allocation-label", "source": label}
    return None, {"kind": "unknown", "source": list(labels)}


def collector_module():
    path = pathlib.Path(__file__).with_name("ci-history-collect.py")
    spec = importlib.util.spec_from_file_location("ci_history_collect", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_attempt(root, run, attempt):
    record = read_json(root / "attempts" / f"{run['id']}-{attempt}.json")
    if record is not None:
        return record
    # Legacy files contain only the latest attempt. Do not invent earlier data.
    if attempt == run.get("run_attempt", 1):
        jobs = read_json(root / f"jobs-{run['id']}.json")
        if jobs is not None and all(
            job.get("run_attempt", attempt) == attempt for job in jobs
        ):
            return {
                "run_id": run["id"],
                "attempt": attempt,
                "run": run,
                "jobs": jobs,
                "metadata_complete": True,
            }
    return {
        "run_id": run["id"],
        "attempt": attempt,
        "run": None,
        "jobs": None,
        "metadata_complete": False,
    }


def attempt_metrics(record, fallback_run, capacities, snapshot_time, seen_jobs):
    run = record.get("run") or {}
    reasons = []
    known = 0
    unknown = 0
    gates = []
    starts = []
    ends = []
    details = []
    if not record.get("metadata_complete"):
        reasons.append("Missing run-attempt or job metadata")
    if run.get("status", "completed") != "completed" or run.get("conclusion") is None:
        reasons.append("Attempt is not terminal")
    for job in record.get("jobs") or []:
        # Requested labels are present on some jobs canceled before assignment.
        # They do not prove that a CPU was allocated to the job.
        assigned = bool(job.get("runner_id") or job.get("runner_name"))
        if not assigned and "runner_id" not in job and "runner_name" not in job:
            reasons.append(
                f"Missing runner assignment metadata for job {job.get('id')}"
            )
        if not assigned and job.get("conclusion") == "success":
            reasons.append(
                f"Successful job has no recorded runner assignment: {job.get('id')}"
            )
        start = job.get("started_at")
        end = job.get("completed_at")
        if job.get("name") in ("Lint", "Test") and end:
            gates.append(
                {
                    "name": job["name"],
                    "completed_at": end,
                    "conclusion": job.get("conclusion"),
                    "run_id": record["run_id"],
                    "attempt": record["attempt"],
                }
            )
        if start and assigned:
            starts.append(start)
        if end and assigned:
            ends.append(end)
        if not assigned:
            continue
        elapsed = seconds(start, end)
        if elapsed is None:
            # A stale active snapshot does not prove allocation until today's time.
            # Use no invented end time; terminal acceptance requires complete times.
            reasons.append(
                f"Missing or invalid completed allocation for job {job.get('id')}"
            )
        if elapsed is None:
            continue
        job_id = job.get("id")
        if job_id is None:
            # Never merge two ID-less jobs. Preserve time but block verified cost.
            reasons.append("Job has no global ID")
            key = (record["run_id"], record["attempt"], len(details))
        else:
            key = job_id
        owner = seen_jobs.get(key)
        if owner:
            if owner[0] != record["run_id"]:
                reasons.append(f"Job ID {job_id} occurs in different workflows")
            continue
        seen_jobs[key] = (record["run_id"], record["attempt"])
        cores, evidence = capacity(job.get("labels", []), capacities)
        if cores is None:
            unknown += elapsed / 60
            reasons.append(f"Unknown allocated CPU capacity for job {job_id}")
        else:
            known += elapsed / 60 * cores
        step_records = []
        for step in job.get("steps", []):
            duration = seconds(step.get("started_at"), step.get("completed_at"))
            if duration is not None:
                step_records.append(
                    {
                        "name": step["name"],
                        "seconds": duration,
                        "conclusion": step.get("conclusion"),
                    }
                )
        details.append(
            {
                "job_id": job_id,
                "name": job["name"],
                "seconds": elapsed,
                "conclusion": job.get("conclusion"),
                "cores": cores,
                "capacity_evidence": evidence,
                "steps": step_records,
                "url": job.get("html_url"),
            }
        )
    return {
        "run_id": record["run_id"],
        "attempt": record["attempt"],
        "created_at": run.get("created_at", fallback_run["created_at"]),
        "conclusion": run.get("conclusion"),
        "known_core_minutes": known,
        "unknown_capacity_runner_minutes": unknown,
        "cost_status": "VERIFIED" if not reasons else "UNVERIFIED",
        "unverified_reasons": sorted(set(reasons)),
        "gates": gates,
        "jobs": details,
        "first_job_started_at": min(starts) if starts else None,
        "last_job_completed_at": max(ends) if ends else None,
    }


def run_metrics(root, runs, capacities, collection):
    attempts = []
    rows = []
    seen = {}
    snapshot = collection.get("collected_at")
    for run in sorted(
        runs, key=lambda value: (value["id"], value.get("run_attempt", 1))
    ):
        samples = []
        for number in range(1, run.get("run_attempt", 1) + 1):
            record = load_attempt(root, run, number)
            samples.append(attempt_metrics(record, run, capacities, snapshot, seen))
        attempts.extend(samples)
        gate_records = [gate for sample in samples for gate in sample["gates"]]
        starts = [
            sample["first_job_started_at"]
            for sample in samples
            if sample["first_job_started_at"]
        ]
        ends = [
            sample["last_job_completed_at"]
            for sample in samples
            if sample["last_job_completed_at"]
        ]
        gates = {}
        for name in ("Lint", "Test"):
            observed_gates = [gate for gate in gate_records if gate["name"] == name]
            current_gate = (
                max(observed_gates, key=lambda gate: gate["completed_at"])
                if observed_gates
                else None
            )
            # A later failed/canceled gate supersedes an earlier success.
            if current_gate and current_gate["conclusion"] == "success":
                end = current_gate["completed_at"]
                elapsed = seconds(run["created_at"], end)
                gates[name] = {
                    "completed_at": end,
                    "first_creation_to_success_minutes": elapsed / 60
                    if elapsed is not None
                    else None,
                }
        rows.append(
            {
                "id": run["id"],
                "event": run["event"],
                "name": run["name"],
                "head_sha": run["head_sha"],
                "head_branch": run.get("head_branch"),
                "workflow_id": run.get("workflow_id"),
                "created_at": run["created_at"],
                "conclusion": run.get("conclusion"),
                "url": run.get("html_url"),
                "gates": gates,
                "known_core_minutes": sum(
                    sample["known_core_minutes"] for sample in samples
                ),
                "unknown_capacity_runner_minutes": sum(
                    sample["unknown_capacity_runner_minutes"] for sample in samples
                ),
                "cost_status": "VERIFIED"
                if all(sample["cost_status"] == "VERIFIED" for sample in samples)
                else "UNVERIFIED",
                "attempt_count": len(samples),
                "canceled_attempts": sum(
                    sample["conclusion"] == "cancelled" for sample in samples
                ),
                "first_attempt_failed": samples[0]["conclusion"]
                in ("failure", "timed_out", "action_required", "startup_failure"),
                "first_job_started_at": min(starts) if starts else None,
                "last_job_completed_at": max(ends) if ends else None,
                "unverified_reasons": sorted(
                    {
                        reason
                        for sample in samples
                        for reason in sample["unverified_reasons"]
                    }
                ),
            }
        )
    return rows, attempts


def cohort_metrics(runs, origins, collection):
    groups = collections.defaultdict(list)
    unassigned = []
    for run in runs:
        origin = origins.get(str(run["id"]))
        if not origin or not origin.get("verified"):
            unassigned.append(run["id"])
        else:
            groups[(origin["root_event"], origin["source_sha"])].append(run)
    rows = []
    scope_ok = (
        collection.get("downstream_discovered") is True
        and not any(collection.get("discovery_caps", {}).values())
        and not collection.get("unmapped_downstream_run_ids")
        and not collection.get("errors")
    )
    for (event, sha), samples in groups.items():
        cost_ok = scope_ok and all(
            sample["cost_status"] == "VERIFIED" for sample in samples
        )
        rows.append(
            {
                "event": event,
                "source_sha": sha,
                "run_ids": [sample["id"] for sample in samples],
                "known_core_minutes": sum(
                    sample["known_core_minutes"] for sample in samples
                ),
                "unknown_capacity_runner_minutes": sum(
                    sample["unknown_capacity_runner_minutes"] for sample in samples
                ),
                "cost_status": "VERIFIED" if cost_ok else "UNVERIFIED",
                "active": any(sample["conclusion"] is None for sample in samples),
                "canceled": any(sample["canceled_attempts"] for sample in samples),
            }
        )
    return rows, unassigned


def runtime_record(evidence, run_id, attempt):
    return evidence.get(f"{run_id}:{attempt}", evidence.get(f"{run_id}-{attempt}", {}))


def verify_overlay(revision, arm, frozen_id, inventory, options, cache):
    """Recompute the audited graph/application boundary from real Git objects."""
    side = revision.get(arm, {})
    reasons = []
    filename = side.get("overlay_manifest")
    repo = options.get("repo")
    verifier = options.get("verifier")
    if not filename or repo is None or verifier is None:
        return [
            "Schema2 requires an overlay manifest, Git repository, and audited verifier"
        ], None
    path = pathlib.Path(filename)
    if not path.is_absolute():
        path = options["manifest_base"] / path
    try:
        raw = path.read_bytes()
        manifest = json.loads(raw)
        expected = {
            "schema_version": 2,
            "arm": arm,
            "frozen_source_sha": revision.get("source_sha"),
            "frozen_source_tree_hash": revision.get("source_tree_hash"),
            "workflow_sha": side.get("workflow_sha"),
            "checkout_sha": side.get("checkout_sha"),
            "checkout_tree_hash": side.get("checkout_tree_hash"),
            "graph_overlay_sha256": side.get("graph_overlay_sha256"),
            "application_payload_sha256": revision.get("application_payload_sha256"),
            "overlay_boundary_sha256": revision.get("overlay_boundary_sha256"),
            "frozen_identity_sha256": frozen_id,
            "expected_owner_inventory_sha256": inventory,
        }
        for key, value in expected.items():
            if manifest.get(key) != value:
                reasons.append(f"Overlay manifest differs from fixed identity: {key}")
        verifier_hash = hashlib.sha256(pathlib.Path(verifier).read_bytes()).hexdigest()
        cache_key = (
            str(path.resolve()),
            hashlib.sha256(raw).hexdigest(),
            str(pathlib.Path(repo).resolve()),
            verifier_hash,
        )
        if cache_key not in cache:
            result = subprocess.run(
                [
                    sys.executable,
                    str(verifier),
                    "verify",
                    "--repo",
                    str(repo),
                    "--manifest",
                    str(path),
                ],
                capture_output=True,
                text=True,
            )
            if result.returncode:
                cache[cache_key] = (
                    None,
                    "Audited overlay verifier rejected graph/application bytes",
                )
            else:
                value = json.loads(result.stdout)
                cache[cache_key] = (value, None)
        verified, error = cache[cache_key]
        if error:
            reasons.append(error)
        elif not isinstance(verified, dict) or verified.get("status") != "VERIFIED":
            reasons.append("Audited overlay verifier did not verify the graph")
        else:
            output_fields = {
                "source_sha": revision.get("source_sha"),
                "checkout_sha": side.get("checkout_sha"),
                "application_payload_sha256": revision.get(
                    "application_payload_sha256"
                ),
                "overlay_boundary_sha256": revision.get("overlay_boundary_sha256"),
                "graph_overlay_sha256": side.get("graph_overlay_sha256"),
                "frozen_identity_sha256": frozen_id,
            }
            if any(verified.get(key) != value for key, value in output_fields.items()):
                reasons.append(
                    "Verified overlay result differs from fixed original/checkout identities"
                )
        proof = {
            "manifest_sha256": hashlib.sha256(raw).hexdigest(),
            "verifier_sha256": verifier_hash,
            "result": verified,
            "status": "VERIFIED" if not reasons else "UNVERIFIED",
        }
        return reasons, proof
    except (OSError, ValueError, TypeError, KeyError) as error:
        return [f"Overlay verification evidence is missing or invalid: {error}"], None


def arm_metrics(
    revision,
    arm,
    rows_by_id,
    attempts,
    origins,
    evidence,
    collection,
    inventory,
    frozen_id,
    schema_version=1,
    overlay_options=None,
    overlay_cache=None,
):
    side = revision.get(arm, {})
    seeds = set(side.get("run_ids", []))
    trees = {
        (
            rows_by_id[run_id]["event"],
            rows_by_id[run_id]["head_sha"],
            rows_by_id[run_id]["head_branch"],
        )
        for run_id in seeds
        if run_id in rows_by_id
    }
    # A binding cannot omit an automatic workflow at the same event tree.
    seeds |= {
        run_id
        for run_id, row in rows_by_id.items()
        if (row["event"], row["head_sha"], row["head_branch"]) in trees
        and row["event"] in {"push", "pull_request"}
    }
    roots = {
        origins[str(run_id)]["root_run_id"]
        for run_id in seeds
        if str(run_id) in origins
    }
    run_ids = seeds | {
        int(run_id)
        for run_id, origin in origins.items()
        if origin["root_run_id"] in roots
    }
    samples = [rows_by_id[run_id] for run_id in run_ids if run_id in rows_by_id]
    reasons = []
    overlay_proof = None
    if schema_version == 2:
        failures, overlay_proof = verify_overlay(
            revision,
            arm,
            frozen_id,
            inventory,
            overlay_options or {},
            overlay_cache if overlay_cache is not None else {},
        )
        reasons.extend(failures)
    if any(
        str(run_id) not in origins
        or origins[str(run_id)].get("verified") is not True
        or origins[str(run_id)].get("root_event") not in {"push", "pull_request"}
        for run_id in seeds
    ):
        reasons.append("Frozen workflow is outside verified automatic push/PR scope")
    if seeds - rows_by_id.keys():
        reasons.append("Missing a frozen workflow run")
    if not seeds or side.get("scope_closed") is not True:
        reasons.append("Automatic workflow scope is not closed and recorded")
    if collection.get("downstream_discovered") is not True or any(
        collection.get("discovery_caps", {}).values()
    ):
        reasons.append("Downstream discovery is absent or capped")
    if collection.get("unmapped_downstream_run_ids"):
        reasons.append("Downstream runs have no verified source mapping")
    if collection.get("errors"):
        reasons.append("Collection has missing or failed reads")
    if any(sample["cost_status"] != "VERIFIED" for sample in samples):
        reasons.append("Allocation has missing attempts, times, IDs, or CPU capacities")
    arm_attempts = [sample for sample in attempts if sample["run_id"] in run_ids]
    retries = 0
    suites = set()
    cache_hits = 0
    cache_requests = 0
    checks = set()
    test_ids = set()
    for sample in arm_attempts:
        observed = runtime_record(evidence, sample["run_id"], sample["attempt"])
        if not observed:
            reasons.append(
                "Missing per-attempt hosted source, suite, and retry evidence"
            )
            continue
        if schema_version == 2:
            identity = {
                "schema_version": 2,
                "frozen_source_sha": revision.get("source_sha"),
                "frozen_source_tree_hash": revision.get("source_tree_hash"),
                "checkout_sha": side.get("checkout_sha"),
                "checkout_tree_hash": side.get("checkout_tree_hash"),
                "graph_overlay_sha256": side.get("graph_overlay_sha256"),
                "application_payload_sha256": revision.get(
                    "application_payload_sha256"
                ),
                "overlay_boundary_sha256": revision.get("overlay_boundary_sha256"),
            }
            if any(observed.get(key) != value for key, value in identity.items()):
                reasons.append(
                    "Hosted schema2 original/actual overlay identities do not match"
                )
            if "source_sha" in observed or "source_tree_hash" in observed:
                reasons.append(
                    "Schema2 receipt contains ambiguous legacy source aliases"
                )
        elif (
            observed.get("source_sha") != revision.get("source_sha")
            or observed.get("checkout_sha")
            != revision.get("checkout_sha", revision.get("source_sha"))
            or observed.get("source_tree_hash") != revision.get("source_tree_hash")
        ):
            reasons.append("Hosted checkout does not prove the frozen source snapshot")
        if observed.get("frozen_identity_sha256") != frozen_id:
            reasons.append("Hosted evidence does not name the immutable frozen sample")
        if not observed.get("source_evidence"):
            reasons.append("Hosted source evidence has no saved provenance record")
        expected_workflow = side.get("workflow_versions", {}).get(
            str(rows_by_id[sample["run_id"]]["workflow_id"]), side.get("workflow_sha")
        )
        if observed.get("workflow_sha") != expected_workflow:
            reasons.append("Hosted workflow identity differs from the pinned graph")
        if observed.get("owner_inventory_sha") != inventory:
            reasons.append(
                "Post-migration check owners differ from the frozen inventory"
            )
        retry_count = observed.get("test_level_retries")
        if (
            not isinstance(retry_count, int)
            or isinstance(retry_count, bool)
            or retry_count < 0
        ):
            reasons.append("Test-level retry count is missing or invalid")
        else:
            retries += retry_count
        selected = observed.get("selected_test_suites")
        if not isinstance(selected, list) or not all(
            isinstance(value, str) for value in selected
        ):
            reasons.append("Selected test suites are not recorded")
        else:
            suites.update(selected)
        if (
            sample["conclusion"] == "success"
            and observed.get("selected_checks_complete") is not True
        ):
            reasons.append("Selected check completion is not proved")
        for field, target in (
            ("executed_check_ids", checks),
            ("executed_test_ids", test_ids),
        ):
            values = observed.get(field)
            if not isinstance(values, list) or not all(
                isinstance(value, str) for value in values
            ):
                reasons.append(f"Executed obligation inventory is absent: {field}")
            else:
                target.update(values)
        hits, requests = observed.get("cache_hits"), observed.get("cache_requests")
        if (
            not isinstance(hits, int)
            or isinstance(hits, bool)
            or not isinstance(requests, int)
            or isinstance(requests, bool)
            or hits < 0
            or requests < hits
        ):
            reasons.append("Cache hit accounting is missing or invalid")
        else:
            cache_hits += hits
            cache_requests += requests
    if test_ids and not suites:
        reasons.append("Executed tests have no selected suite identity")
    if (
        "expected_check_ids" not in revision
        or set(revision["expected_check_ids"]) != checks
    ):
        reasons.append("Executed check IDs do not equal the fixed required inventory")
    if (
        "expected_test_ids" not in revision
        or set(revision["expected_test_ids"]) != test_ids
    ):
        reasons.append("Executed test IDs do not equal the fixed required inventory")
    if side.get("cache_state") not in {"cold", "warm"}:
        reasons.append("Cold/warm cache state is not fixed")
    required = side.get("required_workflows")
    if not required:
        reasons.append("Selected required workflow inventory is missing")
    for name in required or []:
        executions = [
            sample
            for sample in samples
            if sample["name"] == name and sample["id"] in seeds
        ]
        latest = (
            max(executions, key=lambda sample: (sample["created_at"], sample["id"]))
            if executions
            else None
        )
        if latest is None or latest["conclusion"] != "success":
            reasons.append(
                f"Required workflow latest execution is not successful: {name}"
            )
    gate_times = {}
    for name in ("Lint", "Test"):
        successful = [
            sample
            for sample in samples
            if name in sample["gates"] and sample["id"] in seeds
        ]
        if not successful:
            reasons.append(f"No successful required {name} gate")
            gate_times[name] = None
            continue
        workflow_ids = {sample["workflow_id"] for sample in successful}
        names = {sample["name"] for sample in successful}
        # Include previous canceled/failed executions of the same required gate.
        started = [
            sample["created_at"]
            for sample in samples
            if sample["id"] in seeds
            and (
                (
                    sample["workflow_id"] is not None
                    and sample["workflow_id"] in workflow_ids
                )
                or sample["name"] in names
            )
        ]
        executions = [
            sample
            for sample in samples
            if sample["id"] in seeds
            and (
                (
                    sample["workflow_id"] is not None
                    and sample["workflow_id"] in workflow_ids
                )
                or sample["name"] in names
            )
        ]
        latest = max(
            executions, key=lambda sample: (sample["created_at"], sample["id"])
        )
        if name not in latest["gates"]:
            reasons.append(f"Latest required {name} gate is not successful")
            gate_times[name] = None
            continue
        finish = latest["gates"][name]["completed_at"]
        elapsed = seconds(min(started), finish) if started else None
        gate_times[name] = elapsed / 60 if elapsed is not None else None
        if elapsed is None:
            reasons.append(f"Invalid elapsed time to successful {name} gate")
    return {
        "run_ids": sorted(run_ids),
        "overlay_verification": overlay_proof,
        "actual_checkout_sha": side.get("checkout_sha")
        if schema_version == 2
        else None,
        "actual_checkout_tree_hash": side.get("checkout_tree_hash")
        if schema_version == 2
        else None,
        "status": "VERIFIED" if not reasons else "UNVERIFIED",
        "unverified_reasons": sorted(set(reasons)),
        "gate_minutes": gate_times,
        "known_core_minutes": sum(sample["known_core_minutes"] for sample in samples),
        "unknown_capacity_runner_minutes": sum(
            sample["unknown_capacity_runner_minutes"] for sample in samples
        ),
        "attempts": len(arm_attempts),
        "additional_attempt_count": len(arm_attempts) - len(samples),
        "workflow_retry_rate": sum(sample["attempt_count"] > 1 for sample in samples)
        / len(samples)
        if samples
        else None,
        "canceled_attempts": sum(sample["canceled_attempts"] for sample in samples),
        "first_attempt_failures": sum(
            sample["first_attempt_failed"] for sample in samples
        ),
        "test_level_retries": retries,
        "selected_test_suites": sorted(suites),
        "executed_check_ids": sorted(checks),
        "executed_test_ids": sorted(test_ids),
        "first_attempt_failure_rate": sum(
            sample["first_attempt_failed"] for sample in samples
        )
        / len(samples)
        if samples
        else None,
        "cache_hits": cache_hits,
        "cache_requests": cache_requests,
        "cache_hit_rate": cache_hits / cache_requests if cache_requests else None,
        "cache_state": side.get("cache_state"),
    }


def paired_analysis(
    sample,
    rows,
    attempts,
    origins,
    evidence,
    collection,
    required=None,
    overlay_options=None,
):
    module = collector_module()
    reasons = module.validate_sample(sample, required)
    try:
        version = module.sample_schema(sample)
    except ValueError:
        version = 1
    overlay_cache = {}
    if sample.get("owner_inventory_verified") is not True or not sample.get(
        "owner_inventory_evidence"
    ):
        reasons.append("Post-migration owner inventory is not verified")
    by_id = {row["id"]: row for row in rows}
    pairs = []
    used = {}
    for revision in sample.get("revisions", []):
        arms = {
            arm: arm_metrics(
                revision,
                arm,
                by_id,
                attempts,
                origins,
                evidence,
                collection,
                sample.get("owner_inventory_sha"),
                sample.get("frozen_identity_sha256"),
                schema_version=version,
                overlay_options=overlay_options,
                overlay_cache=overlay_cache,
            )
            for arm in ("old", "candidate")
        }
        for arm, result in arms.items():
            for run_id in result["run_ids"]:
                key = (revision.get("cohort"), revision.get("source_sha"), arm)
                if run_id in used and used[run_id] != key:
                    reasons.append(
                        "A workflow run is counted in more than one sample arm"
                    )
                used[run_id] = key
        if not revision.get("source_tree_hash"):
            reasons.append("Frozen source tree identity is missing")
        pair_reasons = []
        if any(value["status"] != "VERIFIED" for value in arms.values()):
            pair_reasons.append("Old/candidate pair is incomplete or unverified")
        if (
            arms["old"]["executed_check_ids"] != arms["candidate"]["executed_check_ids"]
            or arms["old"]["executed_test_ids"]
            != arms["candidate"]["executed_test_ids"]
        ):
            pair_reasons.append("Executed obligations differ between paired graphs")
        if (
            arms["old"]["selected_test_suites"]
            != arms["candidate"]["selected_test_suites"]
        ):
            pair_reasons.append(
                "Selected suite inventory differs between paired graphs"
            )
        if arms["candidate"]["cache_state"] != arms["old"]["cache_state"]:
            pair_reasons.append("Cold/warm cache states are not paired")
        pairs.append(
            {
                "cohort": revision.get("cohort"),
                "source_sha": revision.get("source_sha"),
                "change_class": revision.get("change_class"),
                "checkout_sha": revision.get(
                    "checkout_sha", revision.get("source_sha")
                ),
                "workflow_versions": {
                    arm: revision.get(arm, {}).get("workflow_sha")
                    for arm in ("old", "candidate")
                },
                **arms,
                "status": "VERIFIED" if not pair_reasons else "UNVERIFIED",
                "unverified_reasons": pair_reasons,
            }
        )
    for cohort in ("push", "pull_request"):
        members = [pair for pair in pairs if pair["cohort"] == cohort]
        for metric in ("first_attempt_failures", "test_level_retries"):
            if sum(pair["candidate"][metric] for pair in members) > sum(
                pair["old"][metric] for pair in members
            ):
                reasons.append(
                    f"Candidate {metric} exceed old count across fixed {cohort} sample"
                )
    if any(pair["status"] != "VERIFIED" for pair in pairs):
        reasons.append(
            "Every frozen revision must have a complete successful stable pair"
        )
    groups = {}
    # Failed pairs stay in the fixed groups. No outcome can change class weights.
    for cohort in ("push", "pull_request"):
        selected = [pair for pair in pairs if pair["cohort"] == cohort]
        categories = {
            "overall": selected,
            "selected-test": [
                pair for pair in selected if pair["old"]["selected_test_suites"]
            ],
            "no-op-test": [
                pair for pair in selected if not pair["old"]["selected_test_suites"]
            ],
        }
        for state in ("cold", "warm"):
            categories[f"cache-{state}"] = [
                pair for pair in selected if pair["old"]["cache_state"] == state
            ]
        for name in {pair["change_class"] for pair in selected}:
            categories[name] = [
                pair for pair in selected if pair["change_class"] == name
            ]
        for name, members in categories.items():
            groups[f"{cohort}:{name}"] = {
                "fixed_revision_count": len(members),
                "verified_pairs": sum(pair["status"] == "VERIFIED" for pair in members),
            }
            for arm in ("old", "candidate"):
                groups[f"{cohort}:{name}"][arm] = {
                    "lint_minutes": stats(
                        [
                            pair[arm]["gate_minutes"]["Lint"]
                            for pair in members
                            if pair[arm]["gate_minutes"]["Lint"] is not None
                        ]
                    ),
                    "test_minutes": stats(
                        [
                            pair[arm]["gate_minutes"]["Test"]
                            for pair in members
                            if pair[arm]["gate_minutes"]["Test"] is not None
                        ]
                    ),
                    "allocated_core_minutes": stats(
                        [pair[arm]["known_core_minutes"] for pair in members]
                    ),
                    "canceled_attempts": sum(
                        pair[arm]["canceled_attempts"] for pair in members
                    ),
                    "first_attempt_failures": sum(
                        pair[arm]["first_attempt_failures"] for pair in members
                    ),
                    "test_level_retries": sum(
                        pair[arm]["test_level_retries"] for pair in members
                    ),
                }
    targets = {
        "P1": "UNVERIFIED",
        "P2": "UNVERIFIED",
        "P3": "UNVERIFIED",
        "P12": "UNVERIFIED",
    }
    if not reasons:
        targets["P12"] = "PASS"
    # Diagnostic observations do not qualify population median targets, at any size.
    return {
        "status": "VERIFIED" if not reasons else "UNVERIFIED",
        "report_scope": "predeclared_diagnostic_controls",
        "median_target_qualification": "UNVERIFIED",
        "targets": targets,
        "frozen_identity_sha256": sample.get("frozen_identity_sha256"),
        "owner_inventory_sha": sample.get("owner_inventory_sha"),
        "unverified_reasons": sorted(set(reasons)),
        "pairs": pairs,
        "groups": groups,
        "fixed_class_weights": {
            cohort: dict(
                collections.Counter(
                    pair["change_class"] for pair in pairs if pair["cohort"] == cohort
                )
            )
            for cohort in ("push", "pull_request")
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--input", type=pathlib.Path, default=pathlib.Path("/tmp/ci-history")
    )
    parser.add_argument(
        "--output", type=pathlib.Path, help="New report directory; defaults to input"
    )
    parser.add_argument("--output-prefix", default="")
    parser.add_argument("--since")
    parser.add_argument("--sample", type=pathlib.Path)
    parser.add_argument("--bindings", type=pathlib.Path)
    parser.add_argument(
        "--require-acceptance",
        "--require-complete-diagnostics",
        action="store_true",
        help="Exit nonzero if declared diagnostic evidence or stability guards fail; does not qualify median targets",
    )
    parser.add_argument(
        "--required-revisions",
        type=int,
        default=None,
        help="Legacy argument accepted for compatibility; no size requirement or median qualification",
    )
    parser.add_argument(
        "--cores-json",
        type=pathlib.Path,
        help="Capacity records with observed/configured provenance; numeric guesses are not verified",
    )
    parser.add_argument(
        "--overlay-repo",
        type=pathlib.Path,
        help="Git object repository for audited schema2 verification",
    )
    parser.add_argument(
        "--overlay-verifier",
        type=pathlib.Path,
        default=pathlib.Path(__file__).with_name("benchmark-overlay.py"),
    )
    args = parser.parse_args()
    root = args.input
    output = args.output or root
    runs = read_json(root / "runs.json", read_json(root / "selected-runs.json", []))
    # Duplicate discovery pages must not double-count a run or job.
    by_id = {run["id"]: run for run in runs}
    runs = [
        run
        for run in by_id.values()
        if not args.since or run["created_at"] >= args.since
    ]
    collection = read_json(root / "collection.json", {})
    capacities = (
        read_json(args.cores_json, {})
        if args.cores_json
        else read_json(root / "runner-capacities.json", {})
    )
    rows, attempts = run_metrics(root, runs, capacities, collection)
    origins = read_json(root / "run-source-map.json", {})
    if not origins:
        origins = {
            str(run["id"]): {
                "source_sha": run["head_sha"],
                "root_run_id": run["id"],
                "root_event": run["event"],
                "verified": True,
            }
            for run in runs
            if run["event"] in {"push", "pull_request"}
        }
    heads, unassigned = cohort_metrics(rows, origins, collection)
    summary = {
        "schema_version": 2,
        "units": "Allocated runner job duration * verified CPU capacity. Not CPU use or billing. Unknown capacity is excluded from the known lower bound.",
        "run_count": len(rows),
        "attempt_count": len(attempts),
        "unassigned_run_ids": unassigned,
        "known_core_minutes_total": sum(row["known_core_minutes"] for row in rows),
        "unknown_capacity_runner_minutes_total": sum(
            row["unknown_capacity_runner_minutes"] for row in rows
        ),
        "cost_status": "VERIFIED"
        if heads
        and all(head["cost_status"] == "VERIFIED" for head in heads)
        and not unassigned
        else "UNVERIFIED",
        "canceled_attempt_known_core_minutes": sum(
            row["known_core_minutes"]
            for row in attempts
            if row["conclusion"] == "cancelled"
        ),
        "heads": heads,
        "capacity_records": capacities,
    }
    sample = (
        read_json(args.sample)
        if args.sample
        else read_json(root / "frozen-sample.json")
    )
    paired = None
    if sample:
        if args.bindings:
            sample = collector_module().bind_sample(sample, read_json(args.bindings))
        evidence = read_json(root / "runtime-evidence.json", {})
        paired = paired_analysis(
            sample,
            rows,
            attempts,
            origins,
            evidence,
            collection,
            args.required_revisions,
            overlay_options={
                "repo": args.overlay_repo,
                "verifier": args.overlay_verifier,
                "manifest_base": args.sample.parent if args.sample else root,
            },
        )
        write_json(output / f"{args.output_prefix}paired-analysis.json", paired)
        summary["paired_status"] = paired["status"]
        summary["targets"] = paired["targets"]
    for name, value in (
        ("summary", summary),
        ("run-metrics", rows),
        ("attempt-metrics", attempts),
        ("head-metrics", heads),
    ):
        write_json(output / f"{args.output_prefix}{name}.json", value)
    print(
        json.dumps(
            {
                key: value
                for key, value in summary.items()
                if key not in {"heads", "capacity_records"}
            },
            indent=2,
        )
    )
    if args.require_acceptance and (
        paired is None
        or paired["status"] != "VERIFIED"
        or paired["targets"]["P12"] != "PASS"
    ):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
