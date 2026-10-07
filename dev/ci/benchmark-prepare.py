#!/usr/bin/env python3
"""Save actual source and graph proofs. Do not replace missing revisions."""

import argparse
import collections
import datetime
import hashlib
import importlib.util
import json
import pathlib
import re
import subprocess


def history_module():
    spec = importlib.util.spec_from_file_location(
        "history", pathlib.Path(__file__).with_name("ci-history-collect.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args], text=True, stderr=subprocess.PIPE
    ).strip()


def read(path):
    return json.loads(path.read_text())


def sha256(value):
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def write_new(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as stream:
        stream.write(json.dumps(value, sort_keys=True, indent=2) + "\n")


def latest(runs, prs, count=30, base="self-hosted"):
    selected, counts = [], {}
    primary = history_module().PRIMARY | {"CI"}
    for event in ("push", "pull_request"):
        groups = collections.defaultdict(list)
        for row in runs:
            if row.get("event") == event:
                groups[row["head_sha"]].append(row)
        eligible = []
        for sha, rows in groups.items():
            if any(row.get("status") != "completed" for row in rows):
                continue
            if not any(row.get("name") in primary for row in rows):
                continue
            associations = [
                pr for row in rows for pr in row.get("pull_requests", [])
            ] + prs
            proofs = [
                pr
                for pr in associations
                if pr.get("base", {}).get("ref") == base
                and pr.get("head", {}).get("sha") == sha
            ]
            verified = (
                all(row.get("head_branch") == base for row in rows)
                if event == "push"
                else bool(proofs)
            )
            if verified:
                eligible.append(
                    {
                        "cohort": event,
                        "source_sha": sha,
                        "historical_run_ids": sorted(row["id"] for row in rows),
                        "created_at": max(row["created_at"] for row in rows),
                        "pr_metadata": proofs[0] if proofs else None,
                    }
                )
        eligible.sort(
            key=lambda row: (row["created_at"], row["source_sha"]), reverse=True
        )
        counts[event] = len(eligible)
        selected.extend(eligible[:count])
    return selected, counts


def source_proof(root, row):
    sha = row["source_sha"]
    tree = git(root, "rev-parse", f"{sha}^{{tree}}")
    if row["cohort"] == "push":
        base = git(root, "rev-parse", f"{sha}^1")
        evidence = {
            "kind": "saved base push metadata",
            "run_ids": row["historical_run_ids"],
        }
    else:
        pr = row["pr_metadata"]
        base_sha = pr.get("base", {}).get("sha")
        if not re.fullmatch(r"[0-9a-f]{40}", base_sha or ""):
            raise ValueError("Historical PR base SHA is absent")
        base = git(root, "merge-base", sha, base_sha)
        evidence = {
            "kind": "saved PR metadata with exact head",
            "number": pr["number"],
            "base_ref": pr["base"]["ref"],
            "base_sha": base_sha,
            "metadata_sha256": sha256(pr),
        }
    paths = git(root, "diff", "--name-only", "--no-renames", "-z", base, sha).split(
        "\0"
    )
    return {
        "cohort": row["cohort"],
        "source_sha": sha,
        "checkout_sha": sha,
        "source_tree_hash": tree,
        "changed_files": sorted({path for path in paths if path}),
        "change_class": history_module().classify_paths(paths),
        "base_verified": True,
        "base_evidence": evidence,
        "changed_files_evidence": {
            "kind": "local Git object diff",
            "from": base,
            "to": sha,
            "name_status": git(
                root, "diff", "--name-status", "--no-renames", base, sha
            ).splitlines(),
        },
        "snapshot_policy": "Exact head snapshot; historical PR merge checkout is not inferred",
    }


def graph_proof(root, sha):
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Graph SHA must contain 40 hexadecimal characters")
    paths = git(
        root,
        "ls-tree",
        "-r",
        "--name-only",
        sha,
        ".github",
        "dev/ci",
        "dev/js",
        "dev/just",
        "nix",
        "justfile",
        "pnpm-workspace.yaml",
        "crates/xmtp_sdk/dev",
    ).splitlines()
    return {
        "sha": sha,
        "tree": git(root, "rev-parse", f"{sha}^{{tree}}"),
        "files": {
            line.split("\t", 1)[1]: line.split()[2]
            for line in git(root, "ls-tree", "-r", sha, *paths).splitlines()
        },
    }


def prepare(args):
    runs = []
    for name in ("push-runs.json", "pr-runs.json"):
        runs.extend(history_module().flatten_runs(read(args.history / name)))
    runs = list({row["id"]: row for row in runs}.values())
    prs = read(args.history / "prs.json")
    selected, counts = latest(runs, prs, base=args.base)
    blockers = [
        f"Only {counts[event]} completed revisions have verified {event} base; require 30"
        for event in counts
        if counts[event] < 30
    ]
    proofs, missing = [], []
    for row in selected:
        try:
            proofs.append(source_proof(args.repo, row))
        except (ValueError, subprocess.CalledProcessError) as error:
            missing.append(
                {
                    "cohort": row["cohort"],
                    "source_sha": row["source_sha"],
                    "error": str(error),
                }
            )
    if missing:
        blockers.append(
            f"{len(missing)} selected source/base objects are unavailable; no revisions were replaced"
        )
    graphs = {
        arm: graph_proof(args.repo, sha)
        for arm, sha in (("old", args.old), ("candidate", args.candidate))
    }
    owners = read(args.owners) if args.owners else {}
    owner_hash = sha256(owners) if owners else None
    if (
        not owners.get("checks")
        or not owners.get("test_ids")
        or not owners.get("evidence")
    ):
        blockers.append("Current collected check/test owner inventory is absent")
    for number in (4419, 4421, 4424, 4434):
        if not owners.get("migrations", {}).get(str(number)):
            blockers.append(f"SDK owner migration proof for PR {number} is absent")
    if owners.get("retired_conformance_excluded") is not True:
        blockers.append(
            "Retired conformance exclusion is not proved in both owner inventories"
        )
    expectations = read(args.expectations) if args.expectations else {}
    for proof in proofs:
        key = f"{proof['cohort']}:{proof['source_sha']}"
        item = expectations.get(key, {})
        if not all(
            field in item
            for field in ("expected_check_ids", "expected_test_ids", "cache_state")
        ) or item.get("cache_state") not in {"cold", "warm"}:
            blockers.append(f"Fixed workload/cache-state proof absent: {key}")
        else:
            proof.update(
                {
                    field: item[field]
                    for field in (
                        "expected_check_ids",
                        "expected_test_ids",
                        "cache_state",
                    )
                }
            )
    report = {
        "schema_version": 1,
        "prepared_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "history_collected_at": read(args.history / "collection.json").get(
            "collected_at"
        ),
        "status": "BLOCKED" if blockers else "READY_TO_FREEZE",
        "blockers": blockers,
        "eligible_counts": counts,
        "selected": selected,
        "source_proofs": proofs,
        "missing_source_objects": missing,
        "graphs": graphs,
        "owner_inventory_sha256": owner_hash,
        "note": "Saved historical discovery is not a new benchmark-start window. No hosted timing or capacity is inferred.",
    }
    write_new(args.output / "preparation.json", report)
    if args.freeze:
        if blockers:
            raise ValueError(
                "Cannot freeze; preparation.json lists all admission failures"
            )
        if not args.overlay_pairs:
            raise ValueError("Schema 2 freeze requires precomputed exact overlay pairs")
        pairs = read(args.overlay_pairs)
        try:
            sample = history_module().freeze_sample(
                runs,
                prs,
                proofs,
                args.base,
                args.old,
                args.candidate,
                owner_hash,
                schema_version=2,
                overlay_pairs=pairs,
            )
        except TypeError as error:
            raise ValueError(
                "Collector does not yet support schema 2 overlay freeze"
            ) from error
        sample["owner_inventory_verified"] = True
        sample["owner_inventory_evidence"] = owners["evidence"]
        write_new(args.output / "frozen-source-sample.json", sample)
    print(
        json.dumps(
            {
                "status": report["status"],
                "eligible_counts": counts,
                "proved_sources": len(proofs),
                "blockers": len(blockers),
            }
        )
    )
    return bool(blockers)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", type=pathlib.Path, required=True)
    parser.add_argument("--repo", type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument("--base", default="self-hosted")
    parser.add_argument("--old", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--owners", type=pathlib.Path)
    parser.add_argument("--expectations", type=pathlib.Path)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--freeze", action="store_true")
    parser.add_argument("--overlay-pairs", type=pathlib.Path)
    args = parser.parse_args()
    try:
        return int(prepare(args))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"benchmark: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
