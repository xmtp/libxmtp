#!/usr/bin/env python3
"""Prepare a proposal for controlled CI replay. Do not launch or admit a run.

Run with dev/nix-shell 'python3.11 -B dev/ci/benchmark-controlled.py ...'.
The source mutations are disclosed input-invalidation controls. They do not
reproduce historical functional edits or prove performance acceptance.
"""

import argparse
import collections
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess

RECIPES = {
    "build-setup": (".cargo/config.toml", "#"),
    "sdk-generator": ("crates/xmtp_sdk/src/lib.rs", "//"),
    "rust-core": ("crates/xmtp_mls/src/lib.rs", "//"),
    "backend-only": ("apps/backend/src/lib.rs", "//"),
    "js-only": ("apps/cli/src/index.ts", "//"),
    "prose-only": ("docs/benchmark-controlled-input.md", "markdown"),
}
SHA = re.compile(r"[0-9a-f]{40}\Z")


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def module(filename):
    path = Path(__file__).with_name(filename)
    spec = importlib.util.spec_from_file_location(filename.replace("-", "_"), path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def git(repo, *args, data=None, env=None):
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        input=data,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return result.stdout


def text_git(repo, *args, **kwargs):
    return git(repo, *args, **kwargs).decode().strip()


def tree(repo, revision):
    result = {}
    for record in git(repo, "ls-tree", "-rz", revision).split(b"\0"):
        if record:
            metadata, path = record.split(b"\t", 1)
            mode, kind, oid = metadata.decode().split()
            result[path.decode()] = {"mode": mode, "kind": kind, "blob": oid}
    return result


def owner_file(path):
    return (
        path.startswith(
            (
                "sdks/node/test/",
                "sdks/browser/test/",
                "sdks/agent/test/",
                "sdks/ios/Tests/",
            )
        )
        or path.startswith("sdks/android/")
        and ("/src/test/" in path or "/src/androidTest/" in path)
    )


def dependency_file(path):
    return Path(path).name in {
        "Cargo.toml",
        "Cargo.lock",
        "package.json",
        "pnpm-lock.yaml",
        "flake.lock",
        "rust-toolchain.toml",
        "gradle.lockfile",
        "libs.versions.toml",
    } or path.endswith(".lockfile")


def inventory(entries, predicate):
    return {path: value for path, value in entries.items() if predicate(path)}


def workload_rows(path):
    value = json.loads(path.read_text())
    rows = value.get("revisions", value.get("source_proofs", []))
    if len(rows) != 60:
        raise ValueError("The historical dossier must supply exactly 60 source proofs")
    counts = collections.Counter(row["cohort"] for row in rows)
    if counts != {"push": 30, "pull_request": 30}:
        raise ValueError(
            "The workload must retain exactly 30 push and 30 PR source rows"
        )
    history = module("ci-history-collect.py")
    result = []
    for index, row in enumerate(rows):
        if not SHA.fullmatch(row.get("source_sha", "")):
            raise ValueError("Historical source identity is invalid")
        if row["change_class"] != history.classify_paths(row["changed_files"]):
            raise ValueError(
                "Historical workload class differs from its changed-file proof"
            )
        if (
            not row.get("changed_files_evidence")
            or row.get("base_verified") is not True
        ):
            raise ValueError("Historical source/class/base evidence is absent")
        result.append(
            {
                "ordinal": index,
                "cohort": row["cohort"],
                "historical_source_sha": row["source_sha"],
                "historical_source_tree_hash": row["source_tree_hash"],
                "historical_changed_files": row["changed_files"],
                "historical_changed_files_evidence": row["changed_files_evidence"],
                "change_class": row["change_class"],
                "case_id": f"{row['cohort']}:{row['source_sha']}",
            }
        )
    return result


def matches_branch(config):
    import fnmatch

    if not isinstance(config, dict):
        return True
    if ("tags" in config or "tags-ignore" in config) and not any(
        key in config for key in ("branches", "branches-ignore")
    ):
        return False
    if any(
        fnmatch.fnmatch("self-hosted", pattern)
        for pattern in config.get("branches-ignore", [])
    ):
        return False
    return any(
        fnmatch.fnmatch("self-hosted", pattern)
        for pattern in config.get("branches", ["*"])
    )


def scope_audit(repo, revision):
    overlay = module("benchmark-overlay.py")
    entries = tree(repo, revision)
    workflows = []
    for path, record in sorted(entries.items()):
        if not path.startswith(".github/workflows/") or not path.endswith(
            (".yml", ".yaml")
        ):
            continue
        raw = git(repo, "show", f"{revision}:{path}")
        workflow = overlay.parse_yaml(raw)
        events = workflow.get("on", {})
        if isinstance(events, list):
            events = {event: {} for event in events}
        if not isinstance(events, dict):
            events = {events: {}}
        automatic = []
        if "push" in events and matches_branch(events["push"]):
            automatic.append("push")
        if "pull_request" in events:
            automatic.append("pull_request")
        if "workflow_run" in events:
            automatic.append("workflow_run")
        if not automatic:
            continue
        source = raw.decode()
        findings = []
        checks = {
            "registry-or-publication": r"(?:packages:\s*write|docker/login-action@|build-push-action@|push-to-registry|imagetools|oras\s+push)",
            "deployment": r"(?:deploy-pages@|vercel@|vercel\s+deploy|environment:\s*(?:production|web-chat)|deploy-dev:)",
            "issue-or-message-write": r"(?:issues:\s*write|pull-requests:\s*write|create-issue@|repository-dispatch@)",
            "trusted-ref-guard": r"(?:refs/heads/self-hosted|git/ref/heads/self-hosted|github\.ref\s*==)",
            "warming-or-cache": r"(?:warm-deps|om ci|omnix|cache/save@|cachix-auth-token|sticky)",
            "downstream-default-branch": r"workflow_run:",
        }
        for name, pattern in checks.items():
            if re.search(pattern, source):
                findings.append(name)
        jobs = [
            {
                "id": name,
                "name": job.get("name", name),
                "runs_on": job.get("runs-on"),
                "uses": job.get("uses"),
                "if": job.get("if"),
                "permissions": job.get("permissions", workflow.get("permissions")),
                "secrets": job.get("secrets"),
            }
            for name, job in workflow.get("jobs", {}).items()
        ]
        workflows.append(
            {
                "path": path,
                "graph_blob": record["blob"],
                "name": workflow.get("name"),
                "original_events": events,
                "automatic_events": automatic,
                "findings": findings,
                "jobs": jobs,
                "scope_status": "UNVERIFIED_NEEDS_BUILD_AND_TRUST_AUDIT",
            }
        )
    # Capture local reusable workflows and actions that inherit each root's
    # event, source and trust state. Conditions still need an independent audit.
    dependencies = {}
    pending = [workflow["path"] for workflow in workflows]
    seen = set(pending)
    while pending:
        path = pending.pop()
        raw = git(repo, "show", f"{revision}:{path}")
        value = overlay.parse_yaml(raw)
        references = []
        if isinstance(value, dict):
            for job in value.get("jobs", {}).values():
                if job.get("uses", "").startswith("./.github/"):
                    references.append(job["uses"][2:])
                for step in job.get("steps", []):
                    if step.get("uses", "").startswith("./.github/"):
                        references.append(step["uses"][2:])
            for step in value.get("runs", {}).get("steps", []):
                if step.get("uses", "").startswith("./.github/"):
                    references.append(step["uses"][2:])
        for referenced in references:
            if referenced.startswith(".github/actions/"):
                referenced = referenced.rstrip("/") + "/action.yml"
            if referenced not in entries:
                dependencies[referenced] = {"status": "MISSING_LOCAL_GRAPH_REFERENCE"}
                continue
            if referenced in seen:
                continue
            seen.add(referenced)
            pending.append(referenced)
            linked_raw = git(repo, "show", f"{revision}:{referenced}").decode()
            linked = overlay.parse_yaml(linked_raw)
            dependencies[referenced] = {
                "graph_blob": entries[referenced]["blob"],
                "jobs": list(linked.get("jobs", {})),
                "cache_or_write_markers": [
                    marker
                    for marker in (
                        "cachix-auth-token",
                        "cache/save@",
                        "sticky",
                        "packages: write",
                        "github.ref",
                        "github.event_name",
                    )
                    if marker in linked_raw
                ],
                "status": "TRANSITIVE_CONDITIONS_AND_COSTS_NEED_AUDIT",
            }
    return {
        "graph_sha": revision,
        "graph_tree": text_git(repo, "rev-parse", f"{revision}^{{tree}}"),
        "workflows": workflows,
        "transitive_local_graph": dependencies,
        "automatic_scope_closed": False,
        "note": "Pattern findings are an audit inventory, not proof of complete publication, build, warming or downstream scope",
    }


def contract(base, dossier_digest, class_counts):
    return {
        "schema_version": 1,
        "status": "PROPOSAL_ONLY_NOT_ACCEPTANCE",
        "sampling_revision": {
            "source_policy": "Real controlled snapshots from current app/SDK scope, not historical source heads",
            "baseline_source_sha": base,
            "historical_dossier_sha256": dossier_digest,
            "class_counts": class_counts,
            "historical_order_fixed": True,
            "projection_policy": "Disclosed comment-only input invalidation representatives; original functional edits are not replayed",
            "within_class_representativeness_proved": False,
            "user_policy_revision_approved": False,
        },
        "source_evidence": [
            "Original historical source/file/class proof and actual controlled source commit/tree",
            "Exact actual changed-file diff and class from the two controlled snapshots",
            "Identical current app/test/dependency bytes across old/candidate arms",
            "SDK owner/helper byte inventory and collected current test IDs per source/profile",
            "Actual push before/head and actual PR base/head/merge SHA/tree; source match before execution",
            "Immutable graph boundary, graph/tooling/harness identity, and exact overlay transformation receipts",
        ],
        "selection_evidence": [
            "Actual successful full bootstrap per arm with Lint/Test checks, source/task/test/profile/coverage receipts",
            "Seed checkout is an ancestor of the actual event checkout, with the same arm graph/harness identity",
            "Independent transport manifest verification before any metadata exclusion",
            "Diff verified original application snapshots, including deleted/renamed and advanced/stacked base inputs",
            "Use the actual event and fork/trust state; no fixture --verified or fabricated passed stamp",
            "Production selector fallback is unchanged; failed ancestor proof selects full or fails",
        ],
        "owner_and_coverage_evidence": [
            "All current rules, tests, assertions, recovery cases/deadlines/cycles, features and targets remain merge-blocking",
            "Retired conformance excluded in both graphs; current SDK owners execute where selected",
            "Per-job execution/profile/source evidence, native retry records and current collected/executed test IDs",
            "Fresh native test results and all original coverage/profile bytes; cached test outcomes are not execution",
            "Paired selected-suite/test/rule inventory equality and coverage-line/profile parity",
        ],
        "performance_evidence": [
            "Actual hosted creation-to-gate time including queue/setup/producer waits/transfers/teardown and every retry",
            "All automatic root workflows and real downstream descendants at each event and every attempt",
            "Assigned-job allocation with actual capacity evidence; failures/cancellations are retained",
            "Cold/warm policy declared before outcomes and verified by native input/cache evidence",
            "Bootstrap and cache-preparation costs stay visible in a full campaign ledger; no pro-rated virtual durations",
            "Overall/per-class medians,p90,no-op selections and selected-suite median with the fixed historical weights",
            "Candidate first-attempt failures/test retries do not exceed old; every fixed row retains a successful pair",
            "Controlled results are labelled; current plan targets and next30 live revisions are not silently declared met",
        ],
        "topology": {
            "canonical_source": "Two30-commit source chains from the frozen current baseline, one per historical cohort",
            "arm_seed": "One actual full graph bootstrap seed per arm; unverified first runs use the full production fallback",
            "push": "Replay commit parent is the real same-arm prior successful seed/replay; actual event before must equal it",
            "pull_request": "Reuse the one draft delivery PR into self-hosted. Verify a same-arm seed ancestor in the merge head history, not forge HEAD^1(main) graph equality",
            "merge": "Real github.sha checkout is retained. Merge payload/tree must equal the frozen source plus audited graph; base advance mismatch blocks that row",
            "selector_adapter": "A benchmark-only adapter verifies live seed/check/owner/coverage evidence, then calls retained selection logic with that proven base. Production fallback remains intact",
            "transport_metadata": "Separate authenticated per-row source/binding metadata from immutable graph identity; do not blindly omit dev/ci/benchmark-overlay.json",
            "failure": "Do not replace the row or advance a verified seed from a failed run; keep every attempt and cost",
        },
        "automatic_scope_gate": {
            "closed": False,
            "requirements": [
                "Every original automatic workflow/build/warming lane has a selected, attempted, and costed owner",
                "Trusted build guards must run the same original builds on an audited sandbox destination or fail admission",
                "No registry/release/site deployment or message side effect without explicit authorization",
                "No PR cache-write secret or false push/ref/source trust bridge",
                "workflow_run default-branch readers have real saved parent events and exact graph/source receipts",
                "Manual release schedules are reported separately and are not used to claim push savings",
            ],
        },
    }


def prepare(args):
    for value in (args.base, args.old_graph, args.candidate_graph):
        if not SHA.fullmatch(value):
            raise ValueError("Source and audit graph refs must be full SHAs")
    rows = workload_rows(args.historical)
    dossier_digest = hashlib.sha256(args.historical.read_bytes()).hexdigest()
    counts = {
        cohort: dict(
            collections.Counter(
                row["change_class"] for row in rows if row["cohort"] == cohort
            )
        )
        for cohort in ("push", "pull_request")
    }
    args.output.mkdir(parents=True, exist_ok=False)
    repo = args.output / "sources.git"
    subprocess.run(
        ["git", "clone", "--bare", "--shared", str(args.repo.resolve()), str(repo)],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    baseline = tree(repo, args.base)
    owners = inventory(baseline, owner_file)
    dependencies = inventory(baseline, dependency_file)
    if not owners or not dependencies:
        raise ValueError("Current owner/dependency source inventories are missing")
    env = dict(
        os.environ,
        GIT_WORK_TREE=str(args.output.resolve()),
        GIT_AUTHOR_NAME="Controlled CI source proposal",
        GIT_AUTHOR_EMAIL="ci-source-proposal@invalid.local",
        GIT_COMMITTER_NAME="Controlled CI source proposal",
        GIT_COMMITTER_EMAIL="ci-source-proposal@invalid.local",
    )
    heads = {cohort: args.base for cohort in ("push", "pull_request")}
    snapshots = []
    history = module("ci-history-collect.py")
    for row in rows:
        cohort = row["cohort"]
        parent = heads[cohort]
        env["GIT_INDEX_FILE"] = str(args.output / f"{cohort}.index")
        text_git(repo, "read-tree", parent, env=env)
        path, comment = RECIPES[row["change_class"]]
        parent_entries = tree(repo, parent)
        original = (
            git(repo, "show", f"{parent}:{path}") if path in parent_entries else b""
        )
        if not original and comment != "markdown":
            raise ValueError(f"Controlled input is absent in current source: {path}")
        nonce = digest(
            {
                "dossier": dossier_digest,
                "case_id": row["case_id"],
                "ordinal": row["ordinal"],
            }
        )
        marker = f"controlled-ci-input:{nonce}"
        mutation = (
            f"\n<!-- {marker} -->\n"
            if comment == "markdown"
            else f"\n{comment} {marker}\n"
        )
        raw = original + mutation.encode()
        oid = text_git(repo, "hash-object", "-w", "--stdin", data=raw)
        mode = parent_entries.get(path, {}).get("mode", "100644")
        text_git(
            repo,
            "update-index",
            "--add",
            "--cacheinfo",
            f"{mode},{oid},{path}",
            env=env,
        )
        source_tree = text_git(repo, "write-tree", env=env)
        source = text_git(
            repo,
            "commit-tree",
            source_tree,
            "-p",
            parent,
            data=f"Controlled input {row['case_id']} ({row['change_class']})\n".encode(),
            env=env,
        )
        actual_paths = sorted(
            filter(
                None,
                text_git(
                    repo, "diff", "--name-only", "--no-renames", "-z", parent, source
                ).split("\0"),
            )
        )
        actual_class = history.classify_paths(actual_paths)
        if actual_class != row["change_class"]:
            raise ValueError(
                "Controlled actual file class differs from the fixed historical class"
            )
        entries = tree(repo, source)
        if (
            inventory(entries, owner_file) != owners
            or inventory(entries, dependency_file) != dependencies
        ):
            raise ValueError(
                "Controlled source changed current SDK owner or dependency bytes"
            )
        snapshots.append(
            dict(
                row,
                source_snapshot_sha=source,
                source_snapshot_tree=source_tree,
                source_parent_sha=parent,
                actual_changed_files=actual_paths,
                actual_change_class=actual_class,
                source_mutation={
                    "kind": "append-only-inert-comment",
                    "path": path,
                    "appended_sha256": hashlib.sha256(mutation.encode()).hexdigest(),
                },
                current_owner_source_sha256=digest(owners),
                dependency_source_sha256=digest(dependencies),
                old_source_snapshot_sha=source,
                candidate_source_snapshot_sha=source,
                admission_status="PROPOSAL_ONLY_NO_HOSTED_EVIDENCE",
            )
        )
        heads[cohort] = source
    refs = []
    for cohort, source in heads.items():
        ref = f"refs/heads/codex/controlled-source-{cohort}"
        text_git(repo, "update-ref", ref, source)
        refs.append(ref)
    text_git(
        repo,
        "bundle",
        "create",
        str(args.output / "sources.bundle"),
        *refs,
        f"^{args.base}",
    )
    proposal = contract(args.base, dossier_digest, counts)
    proposal["source_instances"] = snapshots
    proposal["current_sdk_owner_files"] = owners
    proposal["dependency_files"] = dependencies
    proposal["source_proposal_sha256"] = digest(
        {"source_instances": snapshots, "contract": proposal["sampling_revision"]}
    )
    proposal["scope_audits"] = {
        "old": scope_audit(args.repo, args.old_graph),
        "candidate_audit_only_not_frozen": scope_audit(args.repo, args.candidate_graph),
    }
    (args.output / "proposal.json").write_bytes(canonical(proposal) + b"\n")
    print(
        json.dumps(
            {
                "status": proposal["status"],
                "source_instances": len(snapshots),
                "class_counts": counts,
                "current_owner_files": len(owners),
                "sources_bundle": str(args.output / "sources.bundle"),
            }
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--base", required=True)
    parser.add_argument("--historical", type=Path, required=True)
    parser.add_argument("--old-graph", required=True)
    parser.add_argument("--candidate-graph", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        prepare(args)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"controlled replay proposal: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
