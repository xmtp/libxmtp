#!/usr/bin/env python3
"""Collect CI evidence without starting workflows or polling GitHub.

Offline use preserves the input snapshot:
  python3 dev/ci/ci-history-collect.py --input /tmp/ci-history --output /tmp/ci-full
Network reads require --fetch and use a fixed request budget. A benchmark sample
is a frozen JSON file with revisions, source SHAs, changed files, and paired run
IDs. Never replace a failed or slow revision after that file is frozen.
"""

import argparse
import collections
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import threading
import urllib.parse

PRIMARY = {
    "CI",
    "Lint",
    "Test",
    "Docs Quality",
    "Build and Deploy Docs",
    "Cache all Nix Outputs",
}
DIRECT_EVENTS = {"push", "pull_request"}
SHA = re.compile(r"[0-9a-f]{40}\Z")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def read_json(path, default=None):
    return json.loads(path.read_text()) if path.exists() else default


def flatten_runs(value):
    if isinstance(value, dict):
        return value.get("workflow_runs", [])
    if value and isinstance(value[0], dict) and "workflow_runs" in value[0]:
        return [run for page in value for run in page["workflow_runs"]]
    return value or []


class BoundedAPI:
    """Use saved responses first. Stop before the request or rate reserve is spent."""

    def __init__(
        self, repo, output, enabled=False, budget=100, reserve=200, refresh=False
    ):
        self.repo = repo
        self.output = output
        self.enabled = enabled
        self.budget = budget
        self.reserve = reserve
        self.refresh = refresh
        self.requests = 0
        self.remaining = None
        self.reset = None
        self.lock = threading.Lock()
        self.errors = []
        self.blocked = None

    def get(self, endpoint):
        key = hashlib.sha256(endpoint.encode()).hexdigest()
        saved = self.output / "api" / f"{key}.json"
        if (
            saved.exists()
            and not self.refresh
            and (endpoint != "rate_limit" or not self.enabled)
        ):
            return read_json(saved)["body"]
        if self.blocked:
            raise RuntimeError(self.blocked)
        if not self.enabled:
            raise RuntimeError(f"Offline: missing API response for {endpoint}")
        # Check the rate window once. Never wait for a reset or poll a run.
        if self.remaining is None and endpoint != "rate_limit":
            rate = self.get("rate_limit")["resources"]["core"]
            self.remaining, self.reset = rate["remaining"], rate["reset"]
        with self.lock:
            if self.requests >= self.budget:
                raise RuntimeError("Fixed API request budget exhausted")
            if self.remaining is not None and self.remaining <= self.reserve:
                raise RuntimeError(f"API rate reserve reached; reset at {self.reset}")
            self.requests += 1
            if self.remaining is not None:
                self.remaining -= 1
        path = endpoint if endpoint == "rate_limit" else f"repos/{self.repo}/{endpoint}"
        result = subprocess.run(
            ["gh", "api", "--include", path], capture_output=True, text=True
        )
        if result.returncode:
            if (
                re.search(r"HTTP/[^ ]+ (403|429)", result.stdout)
                or "rate limit" in result.stderr.lower()
            ):
                self.blocked = (
                    "GitHub denied the rate/permission window; no further network reads"
                )
            raise RuntimeError(result.stderr.strip() or "GitHub API read failed")
        headers, separator, body = result.stdout.replace("\r\n", "\n").partition("\n\n")
        if not separator:
            raise RuntimeError("GitHub response has no header/body boundary")
        observed = re.search(r"(?im)^x-ratelimit-remaining:\s*(\d+)", headers)
        if observed:
            with self.lock:
                amount = int(observed[1])
                self.remaining = (
                    min(self.remaining, amount)
                    if self.remaining is not None
                    else amount
                )
        data = json.loads(body)
        write_json(saved, {"endpoint": endpoint, "headers": headers, "body": data})
        return data

    def pages(self, endpoint, key, limit=10):
        rows = []
        capped = False
        for page in range(1, limit + 1):
            join = "&" if "?" in endpoint else "?"
            value = self.get(f"{endpoint}{join}per_page=100&page={page}")
            batch = value[key] if key else value
            rows.extend(batch)
            if len(batch) < 100:
                break
            capped = page == limit
        return rows, capped


def match_pr_base(runs, prs, base):
    branches = {
        pr["head"]["ref"] for pr in prs if pr.get("base", {}).get("ref") == base
    }
    matched = []
    for run in runs:
        if run["event"] != "pull_request":
            continue
        associations = run.get("pull_requests", [])
        if associations and any(pr["base"]["ref"] == base for pr in associations):
            matched.append(run)
        elif not associations and run["head_branch"] in branches:
            matched.append(run)
    return matched


def select_cohort(runs, event, count):
    event_runs = [run for run in runs if run["event"] == event]
    heads = list(
        dict.fromkeys(
            run["head_sha"]
            for run in event_runs
            if run["name"] in PRIMARY and run["status"] == "completed"
        )
    )[:count]
    return [run for run in event_runs if run["head_sha"] in set(heads)]


def classify_paths(paths):
    """Apply the plan's fixed class order. Unknown inputs use build setup."""
    if not paths:
        return "build-setup"
    build_names = {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "flake.nix",
        "flake.lock",
        "pnpm-lock.yaml",
        "pnpm-workspace.yaml",
        "package.json",
        "justfile",
        "rustfmt.toml",
    }
    build_prefixes = (
        ".cargo/",
        ".github/",
        ".config/",
        "nix/",
        "dev/",
        "sdks/ios/",
        "sdks/android/",
    )
    known_prefixes = build_prefixes + (
        "sdks/",
        "crates/",
        "proto/",
        "apps/xmtp_sdk_bindgen/",
        "apps/backend/",
        "apps/cli/",
        "apps/web-chat/",
        "apps/docs/",
        "docs/",
    )
    if any(
        path not in build_names and not path.startswith(known_prefixes)
        for path in paths
    ):
        return "build-setup"
    if any(
        path.endswith((".just", "/Cargo.toml", "/package.json", ".lock"))
        for path in paths
    ):
        return "build-setup"
    if any(path in build_names or path.startswith(build_prefixes) for path in paths):
        return "build-setup"
    if any(
        path.startswith(("sdks/", "crates/xmtp_sdk/", "apps/xmtp_sdk_bindgen/"))
        for path in paths
    ):
        return "sdk-generator"
    if any(path.startswith(("crates/", "proto/")) for path in paths):
        return "rust-core"
    known = ("apps/backend/", "apps/cli/", "apps/web-chat/", "apps/docs/", "docs/")
    if any(not path.startswith(known) for path in paths):
        return "build-setup"
    if any(path.startswith("apps/backend/") for path in paths):
        return "backend-only"

    def prose(path):
        return path.endswith((".md", ".mdx")) and path.startswith(
            ("docs/", "apps/docs/")
        )

    return "prose-only" if all(prose(path) for path in paths) else "js-only"


def source_map(runs, links):
    """A workflow_run SHA is not its tested source. Require upstream event evidence."""
    by_id = {run["id"]: run for run in runs}
    link_by_id = {int(link["child_run_id"]): link for link in links}
    resolved = {}
    errors = []

    def resolve(run_id, chain=()):
        if run_id in resolved:
            return resolved[run_id]
        if run_id in chain or run_id not in by_id:
            raise ValueError("Missing upstream run or a workflow_run cycle")
        run = by_id[run_id]
        if run["event"] in DIRECT_EVENTS:
            item = {
                "source_sha": run["head_sha"],
                "root_run_id": run_id,
                "root_event": run["event"],
                "verified": True,
                "proof": "Direct run metadata",
            }
        elif run["event"] == "workflow_run":
            link = link_by_id.get(run_id)
            if not link or not link.get("evidence"):
                raise ValueError("Missing saved upstream event evidence")
            parent_id = int(link["parent_run_id"])
            parent = by_id.get(parent_id)
            if not parent or parent["head_sha"] != link.get("parent_head_sha"):
                raise ValueError("Upstream head SHA does not match its run metadata")
            if parent["event"] != link.get("parent_event"):
                raise ValueError("Upstream event does not match its run metadata")
            root = resolve(parent_id, (*chain, run_id))
            item = {**root, "parent_run_id": parent_id, "proof": link["evidence"]}
        else:
            raise ValueError("Event is outside automatic push/PR cost scope")
        resolved[run_id] = item
        return item

    for run in runs:
        if run["event"] in DIRECT_EVENTS | {"workflow_run"}:
            try:
                resolve(run["id"])
            except ValueError as error:
                errors.append({"run_id": run["id"], "reason": str(error)})
    return {str(key): value for key, value in resolved.items()}, errors


def load_links(source):
    links = read_json(source / "workflow-run-links.json", [])
    for path in (
        sorted((source / "events").glob("*.json"))
        if (source / "events").exists()
        else []
    ):
        event = read_json(path)
        parent = event.get("workflow_run")
        if parent:
            links.append(
                {
                    "child_run_id": int(path.stem),
                    "parent_run_id": parent["id"],
                    "parent_head_sha": parent["head_sha"],
                    "parent_event": parent["event"],
                    "evidence": {
                        "path": str(path),
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    },
                }
            )
    return links


def sample_schema(sample):
    version = sample.get("schema_version", 1)
    if type(version) is not int or version not in (1, 2):
        raise ValueError("Unsupported fixed sample schema")
    return version


def overlay_bindings_identity(sample):
    """Bind real checkout identities outside the self-binding overlay Git tree."""
    value = {
        "schema_version": 2,
        "frozen_identity_sha256": sample.get("frozen_identity_sha256"),
        "revisions": [],
    }
    for row in sample.get("revisions", []):
        value["revisions"].append(
            {
                "cohort": row.get("cohort"),
                "source_sha": row.get("source_sha"),
                "overlays": {
                    arm: {
                        key: row.get(arm, {}).get(key)
                        for key in (
                            "checkout_sha",
                            "checkout_tree_hash",
                            "workflow_sha",
                            "graph_overlay_sha256",
                            "application_payload_sha256",
                        )
                    }
                    for arm in ("old", "candidate")
                },
                "overlay_boundary_sha256": row.get("overlay_boundary_sha256"),
            }
        )
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def frozen_identity(sample):
    """Hash only identities fixed before outcomes. Run bindings are separate."""
    fixed = {
        "frozen_at": sample.get("frozen_at"),
        "owner_inventory_sha": sample.get("owner_inventory_sha"),
        "revisions": [],
    }
    version = sample_schema(sample)
    if version == 2:
        fixed["schema_version"] = 2
    for row in sample.get("revisions", []):
        item = {
            key: row.get(key)
            for key in (
                "cohort",
                "source_sha",
                "checkout_sha",
                "source_tree_hash",
                "changed_files",
                "change_class",
                "base_verified",
                "base_evidence",
                "changed_files_evidence",
                "expected_check_ids",
                "expected_test_ids",
            )
        }
        item["workflows"] = {
            arm: {
                key: row.get(arm, {}).get(key)
                for key in ("workflow_sha", "workflow_versions", "cache_state")
            }
            for arm in ("old", "candidate")
        }
        if version == 2:
            item["application_payload_sha256"] = row.get("application_payload_sha256")
            item["overlay_boundary_sha256"] = row.get("overlay_boundary_sha256")
            for arm in ("old", "candidate"):
                item["workflows"][arm].update(
                    {
                        key: row.get(arm, {}).get(key)
                        for key in (
                            "graph_overlay_sha256",
                            "application_payload_sha256",
                        )
                    }
                )
            # Actual checkout SHA/tree depend on the embedded frozen identity.
            # They are bound separately after materialization to avoid a cycle.
        fixed["revisions"].append(item)
    return hashlib.sha256(
        json.dumps(fixed, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def bind_sample(sample, bindings):
    result = json.loads(json.dumps(sample))
    if bindings.get("frozen_identity_sha256") != sample.get("frozen_identity_sha256"):
        raise ValueError("Run bindings do not name the fixed sample identity")
    version = sample_schema(sample)
    allowed = {"run_ids", "run_window", "required_workflows", "scope_closed"}
    actual_fields = {"checkout_sha", "checkout_tree_hash", "overlay_manifest"}
    if version == 2:
        allowed |= actual_fields
    for row in result.get("revisions", []):
        key = f"{row['cohort']}:{row['source_sha']}"
        for arm in ("old", "candidate"):
            data = bindings.get("revisions", {}).get(key, {}).get(arm, {})
            if set(data) - allowed:
                raise ValueError(
                    "Run bindings cannot change source, class, graph, or cache state"
                )
            if version == 2:
                for field in ("checkout_sha", "checkout_tree_hash"):
                    existing = row.get(arm, {}).get(field)
                    if (
                        field in data
                        and existing is not None
                        and data[field] != existing
                    ):
                        raise ValueError(
                            "Actual overlay checkout identity cannot be replaced"
                        )
            row.setdefault(arm, {}).update(data)
    if version == 2:
        actual_identity = overlay_bindings_identity(result)
        expected = bindings.get(
            "overlay_bindings_sha256", sample.get("overlay_bindings_sha256")
        )
        if expected != actual_identity:
            raise ValueError(
                "Outer overlay bindings do not match actual checkout identities"
            )
        if sample.get("overlay_bindings_sha256") not in (None, actual_identity):
            raise ValueError("Existing actual overlay binding cannot be replaced")
        result["overlay_bindings_sha256"] = actual_identity
    return result


def freeze_sample(
    runs,
    prs,
    proofs,
    base,
    old_sha,
    candidate_sha,
    inventory,
    count=None,
    schema_version=1,
    overlay_pairs=None,
):
    """Freeze latest complete heads before candidate outcomes, or fail without a file."""
    if type(schema_version) is not int or schema_version not in (1, 2):
        raise ValueError("Unsupported fixed sample schema")
    if not SHA.fullmatch(old_sha or "") or not SHA.fullmatch(candidate_sha or ""):
        raise ValueError("Both graph identities must be full commit SHAs")
    revisions = []
    proof_by_key = {(row["cohort"], row["source_sha"]): row for row in proofs}
    for event in ("push", "pull_request"):
        groups = collections.defaultdict(list)
        for run in runs:
            if run["event"] == event:
                groups[run["head_sha"]].append(run)
        ordered = sorted(
            groups.items(),
            key=lambda item: max(run["created_at"] for run in item[1]),
            reverse=True,
        )
        eligible = []
        for sha, executions in ordered:
            if any(run.get("status") != "completed" for run in executions):
                continue
            if not any(run["name"] in PRIMARY for run in executions):
                continue
            if event == "push":
                verified = all(run["head_branch"] == base for run in executions)
            else:
                verified = any(
                    pr.get("base", {}).get("ref") == base
                    and pr.get("head", {}).get("sha") == sha
                    for pr in prs
                )
                verified |= any(
                    association["base"]["ref"] == base
                    and association["head"]["sha"] == sha
                    for run in executions
                    for association in run.get("pull_requests", [])
                )
            if not verified:
                continue
            eligible.append((sha, executions))
        # Select before outcomes. Missing proof must not replace a declared control.
        for sha, executions in eligible[:count]:
            proof = proof_by_key.get((event, sha))
            if (
                not proof
                or not proof.get("changed_files_evidence")
                or not proof.get("source_tree_hash")
            ):
                raise ValueError(
                    f"Frozen revision lacks changed-file/source proof: {event}:{sha}"
                )
            row = {
                **proof,
                "cohort": event,
                "source_sha": sha,
                "base_verified": True,
                "base_evidence": proof.get("base_evidence")
                or {
                    "kind": "GitHub run/PR metadata",
                    "run_ids": [run["id"] for run in executions],
                },
                "change_class": classify_paths(proof["changed_files"]),
                "historical_run_ids": [run["id"] for run in executions],
            }
            row["old"] = {
                "workflow_sha": old_sha,
                "cache_state": proof.get("cache_state"),
            }
            row["candidate"] = {
                "workflow_sha": candidate_sha,
                "cache_state": proof.get("cache_state"),
            }
            if schema_version == 2:
                key = f"{event}:{sha}"
                pair = (overlay_pairs or {}).get(key)
                if (
                    not pair
                    or pair.get("source_sha") != sha
                    or pair.get("source_tree_hash") != row.get("source_tree_hash")
                ):
                    raise ValueError(
                        f"Frozen source has no matching graph overlay pair: {key}"
                    )
                for field in ("application_payload_sha256", "overlay_boundary_sha256"):
                    if not SHA256.fullmatch(pair.get(field, "")):
                        raise ValueError(
                            f"Graph overlay pair lacks a fixed digest: {field}"
                        )
                    row[field] = pair[field]
                for arm, graph in (("old", old_sha), ("candidate", candidate_sha)):
                    identity = pair.get(arm, {})
                    if (
                        identity.get("workflow_sha") != graph
                        or identity.get("application_payload_sha256")
                        != row["application_payload_sha256"]
                    ):
                        raise ValueError(
                            "Graph pair differs from pinned graph/application identity"
                        )
                    digest = identity.get("graph_overlay_sha256", "")
                    if not SHA256.fullmatch(digest):
                        raise ValueError("Graph overlay digest is missing")
                    row[arm].update(
                        {
                            "graph_overlay_sha256": digest,
                            "application_payload_sha256": row[
                                "application_payload_sha256"
                            ],
                        }
                    )
                # Do not bind an initial checkout rendered with a missing F.
                # Materialize again with F, then bind the resulting checkout once.
            revisions.append(row)
    if not revisions:
        raise ValueError("No complete verified source revisions are available")
    result = {
        "schema_version": schema_version,
        "frozen_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "owner_inventory_sha": inventory,
        "owner_inventory_verified": False,
        "revisions": revisions,
    }
    result["frozen_identity_sha256"] = frozen_identity(result)
    return result


def validate_sample(sample, required=None):
    """Check predeclared controls. The legacy size argument does not qualify medians."""
    errors = []
    try:
        version = sample_schema(sample)
    except ValueError as error:
        return [str(error)]
    if sample.get("frozen_identity_sha256") != frozen_identity(sample):
        errors.append("Frozen source/class/workflow identity is missing or changed")
    if version == 2 and sample.get(
        "overlay_bindings_sha256"
    ) != overlay_bindings_identity(sample):
        errors.append("Actual overlay checkout bindings are missing or changed")
    revisions = sample.get("revisions", [])
    keys = [(row.get("cohort"), row.get("source_sha")) for row in revisions]
    if len(keys) != len(set(keys)):
        errors.append("Frozen sample has duplicate revisions")
    if not revisions:
        errors.append("No predeclared source controls are present")
    if any(row.get("cohort") not in DIRECT_EVENTS for row in revisions):
        errors.append("Control cohort must be push or pull_request")
    if not sample.get("frozen_at") or not sample.get("owner_inventory_sha"):
        errors.append(
            "Frozen time and post-migration owner inventory identity are required"
        )
    for row in revisions:
        if not SHA.fullmatch(row.get("source_sha", "")):
            errors.append("Invalid frozen source SHA")
        if row.get("base_verified") is not True:
            errors.append(f"Unverified frozen base: {row.get('source_sha')}")
        if "changed_files" not in row or not row.get("changed_files_evidence"):
            errors.append(
                f"Missing frozen changed-file evidence: {row.get('source_sha')}"
            )
        elif row.get("change_class") != classify_paths(row["changed_files"]):
            errors.append(
                f"Changed-file class does not match fixed rules: {row.get('source_sha')}"
            )
        if version == 2:
            for field in ("application_payload_sha256", "overlay_boundary_sha256"):
                if not SHA256.fullmatch(row.get(field, "")):
                    errors.append(f"Frozen overlay digest is missing: {field}")
            if not SHA.fullmatch(row.get("source_tree_hash", "")):
                errors.append("Frozen original Git tree identity is missing")
        for arm in ("old", "candidate"):
            side = row.get(arm, {})
            if version == 2:
                if not SHA.fullmatch(side.get("checkout_sha", "")) or not SHA.fullmatch(
                    side.get("checkout_tree_hash", "")
                ):
                    errors.append(f"Actual {arm} overlay checkout SHA/tree is missing")
                if not SHA256.fullmatch(side.get("graph_overlay_sha256", "")):
                    errors.append(f"Actual {arm} graph overlay digest is missing")
                if side.get("application_payload_sha256") != row.get(
                    "application_payload_sha256"
                ):
                    errors.append(
                        "Original/old/candidate application payload hashes differ"
                    )
                if (
                    not isinstance(side.get("overlay_manifest"), str)
                    or not side["overlay_manifest"]
                ):
                    errors.append(f"Verified {arm} overlay manifest is missing")
            if not side.get("run_ids") or not SHA.fullmatch(
                side.get("workflow_sha", "")
            ):
                errors.append(
                    f"Missing {arm} run IDs or workflow identity: {row.get('source_sha')}"
                )
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="xmtp/libxmtp")
    parser.add_argument("--base", default="self-hosted")
    parser.add_argument(
        "--input", type=pathlib.Path, default=pathlib.Path("/tmp/ci-history")
    )
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--push-count", type=int, default=30)
    parser.add_argument("--pr-count", type=int, default=30)
    parser.add_argument("--sample", type=pathlib.Path, help="Frozen paired sample JSON")
    parser.add_argument(
        "--bindings",
        type=pathlib.Path,
        help="Later run IDs bound to the immutable sample",
    )
    parser.add_argument("--freeze-sample", action="store_true")
    parser.add_argument("--revision-proofs", type=pathlib.Path)
    parser.add_argument("--old-workflow-sha")
    parser.add_argument("--candidate-workflow-sha")
    parser.add_argument("--owner-inventory-sha")
    parser.add_argument("--sample-schema", type=int, choices=(1, 2), default=1)
    parser.add_argument(
        "--overlay-pairs",
        type=pathlib.Path,
        help="Fixed graph pair identities for a schema2 source freeze",
    )
    parser.add_argument("--discovery-start")
    parser.add_argument("--discovery-end")
    parser.add_argument(
        "--fetch", action="store_true", help="Allow bounded read-only API requests"
    )
    parser.add_argument("--request-budget", type=int, default=100)
    parser.add_argument("--rate-reserve", type=int, default=200)
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument(
        "--resample",
        action="store_true",
        help="Choose a new cohort instead of preserving saved selected run IDs",
    )
    parser.add_argument("--job-page-limit", type=int, default=20)
    args = parser.parse_args()
    if args.output.resolve() == args.input.resolve():
        parser.error("Use a new output directory to preserve the input snapshot")
    if (
        min(args.push_count, args.pr_count, args.request_budget, args.job_page_limit)
        < 1
    ):
        parser.error("Counts and request budget must be positive")
    args.output.mkdir(parents=True, exist_ok=True)
    api = BoundedAPI(
        args.repo,
        args.output,
        args.fetch,
        args.request_budget,
        args.rate_reserve,
        args.refresh,
    )
    errors = []
    input_collection = read_json(args.input / "collection.json", {})
    caps = dict(input_collection.get("discovery_caps", {}))
    all_runs = []
    for name in (
        "push-runs.json",
        "pr-runs.json",
        "downstream-runs.json",
        "runs.json",
        "selected-runs.json",
    ):
        all_runs.extend(flatten_runs(read_json(args.input / name, [])))
    prs = read_json(args.input / "prs.json", [])
    downstream_discovered = (
        args.input / "downstream-runs.json"
    ).exists() or input_collection.get("downstream_discovered", False)
    sample = read_json(args.sample) if args.sample else None
    if args.bindings:
        if not sample:
            parser.error("--bindings requires --sample")
        sample = bind_sample(sample, read_json(args.bindings))
    window = sample.get("discovery_window", {}) if sample else {}
    start = args.discovery_start or window.get("start")
    end = args.discovery_end or window.get("end")
    if bool(start) != bool(end):
        parser.error("Provide both discovery window endpoints")
    date_query = (
        "&created=" + urllib.parse.quote(f"{start}..{end}", safe="") if start else ""
    )
    if args.fetch:
        try:
            prs, pr_metadata_capped = api.pages(
                f"pulls?state=all&base={args.base}", None, 2
            )
            write_json(args.output / "prs.json", prs)
            # PR lookup affects membership proof, not automatic cost enumeration.
            input_collection["pr_metadata_capped"] = pr_metadata_capped
        except RuntimeError as error:
            errors.append({"phase": "pr_metadata", "error": str(error)})
        for event, query in (
            ("push", f"actions/runs?event=push&branch={args.base}"),
            ("pull_request", "actions/runs?event=pull_request"),
            ("workflow_run", "actions/runs?event=workflow_run"),
        ):
            try:
                if sample and event == "push":
                    query = "actions/runs?event=push"
                fetched, caps[event] = api.pages(query + date_query, "workflow_runs")
                all_runs.extend(fetched)
                write_json(args.output / f"{event}-discovery.json", fetched)
                if event == "workflow_run":
                    downstream_discovered = True
            except RuntimeError as error:
                errors.append(
                    {"phase": "discovery", "event": event, "error": str(error)}
                )
    # One run ID is one workflow execution. Reusable jobs stay within that run.
    by_id = {}
    for run in all_runs:
        old = by_id.get(run["id"])
        if old is None or run.get("run_attempt", 1) >= old.get("run_attempt", 1):
            by_id[run["id"]] = run
    runs = sorted(by_id.values(), key=lambda run: run["created_at"], reverse=True)
    if args.freeze_sample:
        if not all(
            (
                args.revision_proofs,
                args.old_workflow_sha,
                args.candidate_workflow_sha,
                args.owner_inventory_sha,
            )
        ):
            parser.error(
                "Freeze requires revision proofs, both graph SHAs, and owner inventory SHA"
            )
        frozen_path = args.output / "frozen-source-sample.json"
        if frozen_path.exists():
            parser.error("The fixed source sample already exists; do not replace it")
        try:
            sample = freeze_sample(
                runs,
                prs,
                read_json(args.revision_proofs),
                args.base,
                args.old_workflow_sha,
                args.candidate_workflow_sha,
                args.owner_inventory_sha,
                schema_version=args.sample_schema,
                overlay_pairs=read_json(args.overlay_pairs)
                if args.overlay_pairs
                else None,
            )
        except ValueError as error:
            parser.error(str(error))
        write_json(frozen_path, sample)
    sample_errors = validate_sample(sample) if sample else []
    if sample:
        seed_ids = {
            run_id
            for row in sample.get("revisions", [])
            for arm in ("old", "candidate")
            for run_id in row.get(arm, {}).get("run_ids", [])
        }
        for run_id in seed_ids - by_id.keys():
            if args.fetch:
                try:
                    run = api.get(f"actions/runs/{run_id}")
                    by_id[run_id] = run
                    runs.append(run)
                except RuntimeError as error:
                    errors.append({"run_id": run_id, "error": str(error)})
        selected_ids = set(seed_ids)
        # Include every automatic direct workflow for each arm's event tree/window.
        for revision in sample.get("revisions", []):
            for arm in ("old", "candidate"):
                side = revision.get(arm, {})
                seeds = [
                    by_id[run_id]
                    for run_id in side.get("run_ids", [])
                    if run_id in by_id
                ]
                event_trees = {
                    (run["event"], run["head_sha"], run["head_branch"]) for run in seeds
                }
                for run in runs:
                    tree = (run["event"], run["head_sha"], run["head_branch"])
                    if tree in event_trees and run["event"] in DIRECT_EVENTS:
                        selected_ids.add(run["id"])
        write_json(args.output / "frozen-sample.json", sample)
    else:
        existing = read_json(
            args.input / "runs.json", read_json(args.input / "selected-runs.json", [])
        )
        if existing and not args.resample and not args.fetch:
            direct = [
                run
                for run in existing
                if run["event"] in DIRECT_EVENTS | {"workflow_run"}
            ]
        else:
            direct = select_cohort(runs, "push", args.push_count)
            direct += select_cohort(
                match_pr_base(runs, prs, args.base), "pull_request", args.pr_count
            )
        selected_ids = {run["id"] for run in direct}
    links = load_links(args.input)
    # A descendant in the time window can have an upstream run outside it.
    # Resolve that explicit parent ID rather than guess from default-branch SHA.
    if args.fetch:
        parent_ids = {int(link["parent_run_id"]) for link in links} - by_id.keys()
        for parent_id in sorted(parent_ids):
            try:
                parent = api.get(f"actions/runs/{parent_id}")
                by_id[parent_id] = parent
                runs.append(parent)
            except RuntimeError as error:
                errors.append({"parent_run_id": parent_id, "error": str(error)})
    origins, link_errors = source_map(runs, links)
    selected_roots = {
        origins[str(run_id)]["root_run_id"]
        for run_id in selected_ids
        if str(run_id) in origins
    }
    for run_id, origin in origins.items():
        if origin["root_run_id"] in selected_roots:
            selected_ids.add(int(run_id))
    selected = [run for run in runs if run["id"] in selected_ids]
    # Unmapped downstream events are retained separately. Do not silently call them free.
    unmapped = [
        run
        for run in runs
        if run["event"] == "workflow_run" and str(run["id"]) not in origins
    ]
    selected += [run for run in unmapped if run["id"] not in selected_ids]
    write_json(args.output / "runs.json", selected)
    write_json(args.output / "run-source-map.json", origins)
    write_json(args.output / "workflow-run-links.json", links)

    attempt_records = []
    for run in selected:
        for attempt in range(1, run.get("run_attempt", 1) + 1):
            key = f"{run['id']}-{attempt}"
            saved = read_json(args.input / "attempts" / f"{key}.json")
            attempt_run = saved.get("run") if saved else None
            jobs = saved.get("jobs") if saved else None
            if jobs is None and attempt == run.get("run_attempt", 1):
                legacy = read_json(args.input / f"jobs-{run['id']}.json")
                if legacy is not None and all(
                    job.get("run_attempt", attempt) == attempt for job in legacy
                ):
                    jobs, attempt_run = legacy, run
            fresh_attempted = args.fetch and (
                args.refresh
                or jobs is None
                or attempt_run is None
                or (saved is not None and saved.get("metadata_complete") is not True)
            )
            fresh_complete = False
            if fresh_attempted:
                try:
                    attempt_run = api.get(
                        f"actions/runs/{run['id']}/attempts/{attempt}"
                    )
                    jobs, capped = api.pages(
                        f"actions/runs/{run['id']}/attempts/{attempt}/jobs",
                        "jobs",
                        args.job_page_limit,
                    )
                    if capped:
                        errors.append(
                            {
                                "run_id": run["id"],
                                "attempt": attempt,
                                "error": "Job pagination cap reached",
                            }
                        )
                        jobs = None
                    else:
                        fresh_complete = True
                except RuntimeError as error:
                    errors.append(
                        {"run_id": run["id"], "attempt": attempt, "error": str(error)}
                    )
            complete = (
                jobs is not None
                and attempt_run is not None
                and (
                    fresh_complete
                    if fresh_attempted
                    else not saved or saved.get("metadata_complete") is True
                )
            )
            record = {
                "run_id": run["id"],
                "attempt": attempt,
                "run": attempt_run,
                "jobs": jobs,
                "metadata_complete": complete,
            }
            write_json(args.output / "attempts" / f"{key}.json", record)
            attempt_records.append(
                {"run_id": run["id"], "attempt": attempt, "metadata_complete": complete}
            )
    if sample and sample_schema(sample) == 2:
        for row in sample.get("revisions", []):
            for arm in ("old", "candidate"):
                filename = row.get(arm, {}).get("overlay_manifest")
                if not filename:
                    continue
                path = pathlib.Path(filename)
                if not path.is_absolute():
                    base_dir = args.sample.parent if args.sample else args.input
                    path = base_dir / path
                if path.exists():
                    name = f"{row['cohort']}-{row['source_sha']}-{arm}.json"
                    target = args.output / "overlays" / name
                    write_json(target, read_json(path))
                    row[arm]["overlay_manifest"] = str(target.resolve())
                else:
                    errors.append(
                        {
                            "overlay_manifest": str(path),
                            "error": "Missing overlay manifest",
                        }
                    )
        write_json(args.output / "frozen-sample.json", sample)
    for name in ("runner-capacities.json", "runtime-evidence.json"):
        value = read_json(args.input / name)
        if value is not None:
            write_json(args.output / name, value)
    metadata = {
        "schema_version": 2,
        "repo": args.repo,
        "base": args.base,
        "collected_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "source_snapshot_collected_at": input_collection.get("collected_at"),
        "existing_cohort_preserved": bool(
            not args.sample and not args.resample and not args.fetch
        ),
        "scope": "All direct push/PR runs and verified workflow_run descendants; every attempt; run IDs counted once",
        "selected_run_count": len(selected),
        "attempts": attempt_records,
        "downstream_discovered": downstream_discovered,
        "unmapped_downstream_run_ids": [run["id"] for run in unmapped],
        "discovery_caps": caps,
        "pr_metadata_capped": input_collection.get("pr_metadata_capped", False),
        "discovery_window": {"start": start, "end": end},
        "run_query_record_cap": 1000,
        "request_count": api.requests,
        "request_budget": args.request_budget,
        "rate_remaining": api.remaining,
        "rate_reset": api.reset,
        "rate_reserve": args.rate_reserve,
        "frozen_sample_errors": sample_errors,
        "link_errors": link_errors,
        "errors": errors,
        "cost_status": "UNVERIFIED"
        if (
            errors
            or sample_errors
            or unmapped
            or not downstream_discovered
            or any(caps.values())
            or not all(row["metadata_complete"] for row in attempt_records)
        )
        else "METADATA_COMPLETE",
        "limits": [
            "No workflow was started and no run was polled.",
            "Child source identity requires saved upstream event evidence; child head_sha alone is insufficient.",
            "A capped discovery query or unknown runner capacity cannot prove full allocated cost.",
            "Saved metadata is a snapshot, not a complete billing record.",
            "Frozen sample identities, owner inventory, test retries and successful pairs are checked by the analyzer.",
        ],
    }
    write_json(args.output / "collection.json", metadata)
    print(
        json.dumps(
            {
                "runs": len(selected),
                "attempts": len(attempt_records),
                "cost_status": metadata["cost_status"],
                "requests": api.requests,
                "error_count": len(errors),
                "error_examples": errors[:5],
                "sample_errors": sample_errors,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
