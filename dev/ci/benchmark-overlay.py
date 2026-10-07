#!/usr/bin/env python3
"""Build and verify graph overlays without changing application source bytes.

The generated repository is local. This tool does not push or start workflows.
Run with dev/nix-shell 'python3.11 -B dev/ci/benchmark-overlay.py ...'.
The pinned Python environment includes PyYAML.
"""

import argparse
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

SCHEMA = 1
# This list is code, not caller data. A forged manifest cannot widen it.
BUILD_FILES = {
    "justfile",
    "pnpm-workspace.yaml",
    "apps/backend/backend.just",
    "apps/docs/docs.just",
    "sdks/js.just",
    "sdks/android/android.just",
    "apps/docs/scripts/build-site.mjs",
    "crates/xmtp_sdk/dev/link-runtime-packages",
    "crates/xmtp_sdk/dev/lint-generated",
    "crates/xmtp_sdk/dev/sdk-artifacts.py",
    "dev/js/sdk-package",
    "dev/js/lint-source.mjs",
    "nix/shells/rust.nix",
    "nix/shells/local.nix",
}
SAFE_SCRIPT_KEYS = {
    "lint:source",
    "lint:prepared",
    "typecheck:prepared",
    "check:examples:prepared",
    "build:prepared",
}
SAFE_TASK_KEYS = SAFE_SCRIPT_KEYS - {"lint:source"}
SHA = re.compile(r"[0-9a-f]{40}\Z")


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def run(repo, *args, input=None, env=None):
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        input=input,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    if result.returncode:
        raise ValueError(result.stderr.decode().strip())
    return result.stdout


def git(repo, *args, **kwargs):
    return run(repo, *args, **kwargs).decode().strip()


def tree(repo, sha):
    result = {}
    for record in run(repo, "ls-tree", "-rz", sha).split(b"\0"):
        if record:
            meta, path = record.split(b"\t", 1)
            mode, kind, blob = meta.decode().split()
            result[path.decode()] = {"mode": mode, "kind": kind, "blob": blob}
    return result


def content(repo, revision, path):
    return run(repo, "show", f"{revision}:{path}")


def yaml_module():
    import yaml

    # YAML 1.1 treats GitHub's 'on' key as a boolean. Remove that resolver.
    class Loader(yaml.SafeLoader):
        yaml_implicit_resolvers = {
            key: [
                (tag, pattern)
                for tag, pattern in entries
                if tag != "tag:yaml.org,2002:bool"
            ]
            for key, entries in yaml.SafeLoader.yaml_implicit_resolvers.items()
        }

    Loader.add_implicit_resolver(
        "tag:yaml.org,2002:bool", re.compile(r"^(?:true|false)$"), list("tf")
    )
    Loader.add_constructor(
        "tag:yaml.org,2002:bool", lambda loader, node: node.value == "true"
    )
    return yaml, Loader


def parse_yaml(raw):
    yaml, loader = yaml_module()
    return yaml.load(raw, Loader=loader)


def dump_yaml(value):
    yaml, _ = yaml_module()
    return yaml.safe_dump(value, sort_keys=False, width=110).encode()


def permitted_file(path):
    return (
        (path in BUILD_FILES and path != "pnpm-workspace.yaml")
        or (path.startswith(".github/") and Path(path).suffix in {".yml", ".yaml"})
        or (
            path.startswith("dev/ci/")
            and Path(path).suffix in {".py", ".json", ".trigger"}
            and "/fixtures/" not in path
        )
    )


def validate_boundary(boundary):
    if boundary.get("schema_version") != SCHEMA or set(boundary) != {
        "schema_version",
        "files",
        "package_scripts",
        "workspace_tasks",
    }:
        raise ValueError("Invalid overlay boundary schema")
    for path in boundary["files"]:
        if not permitted_file(path):
            raise ValueError(
                f"Application, runtime, test, or unaudited file in boundary: {path}"
            )
    for path, keys in boundary["package_scripts"].items():
        if (
            Path(path).name != "package.json"
            or path.startswith(("/", "../"))
            or ".." in Path(path).parts
        ):
            raise ValueError("Invalid package script boundary path")
        if not set(keys) <= SAFE_SCRIPT_KEYS:
            raise ValueError(
                "Application test/runtime package scripts cannot enter the boundary"
            )
    if not set(boundary["workspace_tasks"]) <= SAFE_TASK_KEYS:
        raise ValueError("Unaudited workspace task in boundary")


def boundary_for(repo, old, candidate):
    left, right = tree(repo, old), tree(repo, candidate)
    files, package_scripts, tasks = [], {}, []
    # Include all YAML graph files, not only changed roots: nested workflows
    # and setup actions must come from the same pinned graph.
    files.extend(
        path
        for path in left.keys() | right.keys()
        if path.startswith(".github/") and permitted_file(path)
    )
    for path in sorted(left.keys() | right.keys()):
        if left.get(path) == right.get(path):
            continue
        if permitted_file(path):
            files.append(path)
        elif Path(path).name == "package.json":
            a = (
                json.loads(content(repo, old, path)).get("scripts", {})
                if path in left
                else {}
            )
            b = (
                json.loads(content(repo, candidate, path)).get("scripts", {})
                if path in right
                else {}
            )
            keys = sorted(
                key for key in a.keys() | b.keys() if a.get(key) != b.get(key)
            )
            if not set(keys) <= SAFE_SCRIPT_KEYS:
                raise ValueError(
                    f"Graph changes an application/package script outside the boundary: {path}:{keys}"
                )
            package_scripts[path] = keys
        elif path == "pnpm-workspace.yaml":
            a = parse_yaml(content(repo, old, path)).get("tasks", {})
            b = parse_yaml(content(repo, candidate, path)).get("tasks", {})
            tasks = sorted(
                key for key in a.keys() | b.keys() if a.get(key) != b.get(key)
            )
            if not set(tasks) <= SAFE_TASK_KEYS:
                raise ValueError("Graph changes an unaudited pnpm task")
        elif (
            path.endswith("AGENTS.md")
            or path.startswith("docs/")
            or path == ".gitignore"
            or path == "crates/xmtp_sdk/dev/test-sdk-artifacts.py"
        ):
            # Documentation and the source snapshot's tests remain frozen.
            continue
        else:
            raise ValueError(f"Graph change has no audited overlay boundary: {path}")
    files.append("dev/ci/benchmark-receipt.py")
    result = {
        "schema_version": SCHEMA,
        "files": sorted(set(files)),
        "package_scripts": package_scripts,
        "workspace_tasks": tasks,
    }
    validate_boundary(result)
    return result


def application_payload(repo, revision, boundary):
    validate_boundary(boundary)
    entries = tree(repo, revision)
    payload = {}
    for path, entry in entries.items():
        if path in boundary["files"]:
            continue
        if path in boundary["package_scripts"]:
            value = json.loads(content(repo, revision, path))
            scripts = value.get("scripts", {})
            for key in boundary["package_scripts"][path]:
                scripts.pop(key, None)
            payload[path] = {"mode": entry["mode"], "json": value}
        elif path == "pnpm-workspace.yaml":
            value = parse_yaml(content(repo, revision, path))
            for key in boundary["workspace_tasks"]:
                value.get("tasks", {}).pop(key, None)
            payload[path] = {"mode": entry["mode"], "yaml": value}
        else:
            payload[path] = entry
    return digest(payload)


def put(repo, path, data, env, mode="100644"):
    blob = git(repo, "hash-object", "-w", "--stdin", input=data)
    git(repo, "update-index", "--add", "--cacheinfo", f"{mode},{blob},{path}", env=env)
    return blob


def adapted_workflow(raw, path, arm, manifest_path):
    workflow = parse_yaml(raw)
    retired = []
    if path == ".github/workflows/test-sdk.yml":
        for key in ("sdk", "swift"):
            if key in workflow.get("jobs", {}):
                retired.append(key)
                del workflow["jobs"][key]
    # The adapter remains on a real push/PR event. Reusable jobs use this same
    # effective commit. GITHUB_SHA is therefore the real tested overlay SHA.
    roots = workflow.get("on", {})
    if isinstance(roots, list):
        roots = {name: {} for name in roots}
    if isinstance(roots, dict):
        for event in ("push", "pull_request"):
            if event not in roots:
                continue
            config = roots[event] or {}
            if event == "push":
                branches = config.get("branches", ["*"])
                tags_only = (
                    ("tags" in config or "tags-ignore" in config)
                    and "branches" not in config
                    and "branches-ignore" not in config
                )
                ignored = any(
                    fnmatch.fnmatch("self-hosted", item)
                    for item in config.get("branches-ignore", [])
                )
                if (
                    not tags_only
                    and not ignored
                    and any(fnmatch.fnmatch("self-hosted", item) for item in branches)
                ):
                    config["branches"] = [f"codex/ci-benchmark-{arm}-*"]
                else:
                    # Tag/release paths cannot be enabled by a benchmark branch.
                    config["branches"] = ["__benchmark_no_automatic_branch__"]
                config.pop("branches-ignore", None)
                config.pop("tags", None)
                config.pop("tags-ignore", None)
            roots[event] = config
        workflow["on"] = roots
    if "concurrency" in workflow:
        original_concurrency = workflow["concurrency"]
        if isinstance(original_concurrency, str):
            workflow["concurrency"] = f"benchmark-{arm}-{original_concurrency}"
        else:
            workflow["concurrency"] = dict(original_concurrency)
            workflow["concurrency"]["group"] = (
                f"benchmark-{arm}-{original_concurrency['group']}"
            )
    for key, job in workflow.get("jobs", {}).items():
        if "steps" not in job:
            continue
        steps = job["steps"]
        checkout = [
            index
            for index, step in enumerate(steps)
            if "actions/checkout@" in step.get("uses", "")
        ]
        if not checkout:
            # API-only jobs still get an honest source and CPU observation.
            steps.insert(0, {"uses": "actions/checkout@v6"})
            checkout = [0]
        for index in checkout:
            steps[index].setdefault("with", {})["ref"] = "${{ github.sha }}"
            steps[index]["with"]["fetch-depth"] = 0
        name = f"{Path(path).stem}-{key}"
        start = {
            "name": "Record benchmark source and CPU",
            "id": "benchmark-source",
            "run": f"python3 -B dev/ci/benchmark-receipt.py start --manifest {manifest_path} --task {name}",
            "env": {
                "BENCHMARK_MATRIX": "${{ toJSON(matrix) }}",
                "BENCHMARK_INPUTS": "${{ toJSON(inputs) }}",
            },
        }
        steps.insert(checkout[0] + 1, start)
        steps.extend(
            [
                {
                    "name": "Record benchmark result and retained reports",
                    "if": "always()",
                    "run": f"python3 -B dev/ci/benchmark-receipt.py finish --manifest {manifest_path} --task {name}",
                    "env": {
                        "BENCHMARK_JOB_STATUS": "${{ job.status }}",
                        "BENCHMARK_MATRIX": "${{ toJSON(matrix) }}",
                        "BENCHMARK_INPUTS": "${{ toJSON(inputs) }}",
                    },
                },
                {
                    "name": "Retain benchmark evidence",
                    "if": "always()",
                    "uses": "actions/upload-artifact@v4",
                    "with": {
                        "name": f"benchmark-{name}-${{{{ steps.benchmark-source.outputs.receipt-id || github.job }}}}",
                        "path": "target/ci-benchmark/",
                        "if-no-files-found": "error",
                        "retention-days": 30,
                    },
                },
            ]
        )
    return dump_yaml(workflow), retired


def overlay(args):
    for value in (args.source, args.old, args.candidate):
        if not SHA.fullmatch(value):
            raise ValueError("Source and graph identities must be full SHAs")
    boundary = boundary_for(args.repo, args.old, args.candidate)
    args.output.mkdir(parents=True, exist_ok=False)
    destination = args.output / "replay.git"
    subprocess.run(
        [
            "git",
            "clone",
            "--bare",
            "--shared",
            str(args.repo.resolve()),
            str(destination),
        ],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    source_tree = git(destination, "rev-parse", f"{args.source}^{{tree}}")
    original_payload = application_payload(destination, args.source, boundary)
    manifests = []
    for arm, graph_sha in (("old", args.old), ("candidate", args.candidate)):
        env = dict(
            os.environ,
            GIT_WORK_TREE=str(args.output.resolve()),
            GIT_INDEX_FILE=str(args.output / f"{arm}.index"),
            GIT_AUTHOR_NAME="CI benchmark",
            GIT_AUTHOR_EMAIL="ci-benchmark@invalid.local",
            GIT_COMMITTER_NAME="CI benchmark",
            GIT_COMMITTER_EMAIL="ci-benchmark@invalid.local",
        )
        git(destination, "read-tree", args.source, env=env)
        graph_entries = tree(destination, graph_sha)
        modified = {}
        retired = []
        task_specs = {}
        blocked_workflows = {}
        for path in boundary["files"]:
            if path == "dev/ci/benchmark-receipt.py":
                raw = Path(__file__).with_name("benchmark-receipt.py").read_bytes()
                mode = "100644"
            elif path not in graph_entries:
                git(destination, "update-index", "--force-remove", path, env=env)
                modified[path] = None
                continue
            else:
                raw = content(destination, graph_sha, path)
                mode = graph_entries[path]["mode"]
            if path.startswith(".github/workflows/"):
                original_workflow = parse_yaml(raw)
                if re.search(
                    rb"(?:packages: write|issues: write|deploy-pages@|docker/login-action@|vercel@|repository-dispatch@)",
                    raw,
                ):
                    blocked_workflows[Path(path).stem] = (
                        "Automatic write/deployment scope needs an audited isolated build adapter"
                    )
                for task_id, task in original_workflow.get("jobs", {}).items():
                    if "steps" in task:
                        task_specs[f"{Path(path).stem}-{task_id}"] = task
                raw, removed = adapted_workflow(
                    raw, path, arm, "dev/ci/benchmark-overlay.json"
                )
                retired.extend(f"{path}:{key}" for key in removed)
            modified[path] = {
                "blob": put(destination, path, raw, env, mode),
                "mode": mode,
                "graph_blob": graph_entries.get(path, {}).get("blob"),
            }
        for path, keys in boundary["package_scripts"].items():
            frozen = json.loads(content(destination, args.source, path))
            graph = json.loads(content(destination, graph_sha, path))
            scripts = frozen.setdefault("scripts", {})
            for key in keys:
                if key in graph.get("scripts", {}):
                    scripts[key] = graph["scripts"][key]
                else:
                    scripts.pop(key, None)
            modified[path] = {
                "blob": put(
                    destination,
                    path,
                    json.dumps(frozen, indent=2).encode() + b"\n",
                    env,
                ),
                "script_keys": keys,
            }
        if boundary["workspace_tasks"]:
            frozen = parse_yaml(
                content(destination, args.source, "pnpm-workspace.yaml")
            )
            graph = parse_yaml(content(destination, graph_sha, "pnpm-workspace.yaml"))
            for key in boundary["workspace_tasks"]:
                if key in graph.get("tasks", {}):
                    frozen.setdefault("tasks", {})[key] = graph["tasks"][key]
                else:
                    frozen.get("tasks", {}).pop(key, None)
            modified["pnpm-workspace.yaml"] = {
                "blob": put(destination, "pnpm-workspace.yaml", dump_yaml(frozen), env),
                "task_keys": boundary["workspace_tasks"],
            }
        effective_tree = git(destination, "write-tree", env=env)
        actual_payload = application_payload(destination, effective_tree, boundary)
        if actual_payload != original_payload:
            raise ValueError(
                "Graph overlay changed application source or runtime bytes"
            )
        manifest = {
            "schema_version": 2,
            "arm": arm,
            "frozen_source_sha": args.source,
            "frozen_source_tree_hash": source_tree,
            "workflow_sha": graph_sha,
            "checkout_tree_hash": effective_tree,
            "effective_entries_sha256": digest(tree(destination, effective_tree)),
            "task_specs": task_specs,
            "blocked_workflows": blocked_workflows,
            "application_payload_sha256": actual_payload,
            "original_application_payload_sha256": original_payload,
            "overlay_boundary": boundary,
            "overlay_boundary_sha256": digest(boundary),
            "comparison_graphs": {"old": args.old, "candidate": args.candidate},
            "graph_overlay_sha256": digest(modified),
            "overlay_files": modified,
            "retired_jobs_removed": retired,
            "expected_owner_inventory_sha256": args.owners_sha,
            "frozen_identity_sha256": args.frozen_identity,
            "admission_status": "UNVERIFIED",
            "event_policy": "real overlay push/PR; no GITHUB_SHA bridge",
            "unverified_reasons": [
                "Owner/test/profile inventories need hosted evidence",
                "Generated graph adaptations need audit",
                "Workflow-run default-branch descendants and live deployment scope need exact mapping",
                "Fixed changed-path selection must be checked against actual overlay diff before acceptance",
            ],
        }
        # Manifest cannot contain its own commit hash. Bind the real commit
        # outside its tree, and store the tree before adding this graph-only file.
        put(
            destination,
            "dev/ci/benchmark-overlay.json",
            canonical(manifest) + b"\n",
            env,
        )
        final_tree = git(destination, "write-tree", env=env)
        commit = git(
            destination,
            "commit-tree",
            final_tree,
            "-p",
            args.source,
            input=f"Benchmark {arm}: frozen {args.source}\n".encode(),
            env=env,
        )
        branch = f"codex/ci-benchmark-{arm}-{args.source[:12]}"
        git(destination, "update-ref", f"refs/heads/{branch}", commit)
        manifest["checkout_sha"] = commit
        manifest["checkout_tree_hash"] = final_tree
        manifest["branch"] = branch
        manifests.append(manifest)
        (args.output / f"{arm}.json").write_bytes(canonical(manifest) + b"\n")
    for manifest in manifests:
        verify(destination, manifest)
    git(
        destination,
        "bundle",
        "create",
        str(args.output / "replay.bundle"),
        *[f"refs/heads/{manifest['branch']}" for manifest in manifests],
    )
    (args.output / "pair.json").write_bytes(
        canonical(
            {
                "schema_version": 2,
                "source_sha": args.source,
                "source_tree_hash": source_tree,
                "application_payload_sha256": original_payload,
                "overlay_boundary_sha256": digest(boundary),
                "old": manifests[0],
                "candidate": manifests[1],
            }
        )
        + b"\n"
    )
    print(
        json.dumps(
            {
                "status": "LOCAL_OVERLAYS_CREATED_ACCEPTANCE_UNVERIFIED",
                "branches": [item["branch"] for item in manifests],
                "application_payload_sha256": original_payload,
            }
        )
    )


def verify(repo, manifest):
    boundary = manifest["overlay_boundary"]
    validate_boundary(boundary)
    graphs = manifest["comparison_graphs"]
    if boundary_for(repo, graphs["old"], graphs["candidate"]) != boundary:
        raise ValueError("Boundary does not equal the pinned graph comparison")
    if graphs.get(manifest["arm"]) != manifest["workflow_sha"]:
        raise ValueError("Arm does not name its pinned graph")
    if digest(boundary) != manifest["overlay_boundary_sha256"]:
        raise ValueError("Overlay boundary digest changed")
    if digest(manifest["overlay_files"]) != manifest["graph_overlay_sha256"]:
        raise ValueError("Overlay file digest changed")
    frozen = manifest["frozen_source_sha"]
    effective = manifest["checkout_sha"]
    if (
        git(repo, "rev-parse", f"{frozen}^{{tree}}")
        != manifest["frozen_source_tree_hash"]
    ):
        raise ValueError("Frozen source tree changed")
    if (
        git(repo, "rev-parse", f"{effective}^{{tree}}")
        != manifest["checkout_tree_hash"]
    ):
        raise ValueError("Effective checkout tree changed")
    # The in-tree receipt manifest is graph-only, and must bind the exact same
    # immutable original, boundary and generated graph blob inventory.
    embedded = json.loads(content(repo, effective, "dev/ci/benchmark-overlay.json"))
    for key in (
        "frozen_source_sha",
        "frozen_source_tree_hash",
        "workflow_sha",
        "graph_overlay_sha256",
        "overlay_boundary",
        "overlay_boundary_sha256",
        "overlay_files",
        "application_payload_sha256",
        "comparison_graphs",
        "frozen_identity_sha256",
        "expected_owner_inventory_sha256",
    ):
        if embedded[key] != manifest[key]:
            raise ValueError(f"Embedded overlay identity changed: {key}")
    entries = tree(repo, effective)
    graph_entries = tree(repo, manifest["workflow_sha"])
    task_specs, blocked_workflows = {}, {}
    for path in graph_entries:
        if path.startswith(".github/workflows/") and path in boundary["files"]:
            raw = content(repo, manifest["workflow_sha"], path)
            workflow = parse_yaml(raw)
            for task_id, task in workflow.get("jobs", {}).items():
                if "steps" in task:
                    task_specs[f"{Path(path).stem}-{task_id}"] = task
            if re.search(
                rb"(?:packages: write|issues: write|deploy-pages@|docker/login-action@|vercel@|repository-dispatch@)",
                raw,
            ):
                blocked_workflows[Path(path).stem] = (
                    "Automatic write/deployment scope needs an audited isolated build adapter"
                )
    if (
        manifest.get("task_specs") != task_specs
        or manifest.get("blocked_workflows") != blocked_workflows
    ):
        raise ValueError(
            "Task/profile or automatic write guards differ from the pinned graph"
        )
    if (
        embedded.get("task_specs") != task_specs
        or embedded.get("blocked_workflows") != blocked_workflows
    ):
        raise ValueError("Embedded task/profile or automatic write guards changed")
    expected_paths = set(boundary["files"]) | set(boundary["package_scripts"])
    if boundary["workspace_tasks"]:
        expected_paths.add("pnpm-workspace.yaml")
    if set(manifest["overlay_files"]) != expected_paths:
        raise ValueError("Overlay receipt omits or adds graph files")
    for path in boundary["files"]:
        record = manifest["overlay_files"][path]
        if path == "dev/ci/benchmark-receipt.py":
            raw = Path(__file__).with_name("benchmark-receipt.py").read_bytes()
        elif path not in graph_entries:
            if record is not None:
                raise ValueError("Graph file is absent in the pinned graph")
            continue
        else:
            if record.get("graph_blob") != graph_entries[path]["blob"]:
                raise ValueError("Graph file provenance differs from pinned graph")
            raw = content(repo, manifest["workflow_sha"], path)
            if path.startswith(".github/workflows/"):
                raw, _ = adapted_workflow(
                    raw, path, manifest["arm"], "dev/ci/benchmark-overlay.json"
                )
        if git(repo, "hash-object", "--stdin", input=raw) != record["blob"]:
            raise ValueError(
                f"Graph adaptation is not the audited transformation: {path}"
            )
    for path, keys in boundary["package_scripts"].items():
        frozen_json = json.loads(content(repo, frozen, path))
        graph_json = json.loads(content(repo, manifest["workflow_sha"], path))
        for key in keys:
            if key in graph_json.get("scripts", {}):
                frozen_json.setdefault("scripts", {})[key] = graph_json["scripts"][key]
            else:
                frozen_json.get("scripts", {}).pop(key, None)
        raw = json.dumps(frozen_json, indent=2).encode() + b"\n"
        if (
            git(repo, "hash-object", "--stdin", input=raw)
            != manifest["overlay_files"][path]["blob"]
        ):
            raise ValueError(
                "Package CI script adaptation differs from the pinned graph"
            )
    if boundary["workspace_tasks"]:
        frozen_yaml = parse_yaml(content(repo, frozen, "pnpm-workspace.yaml"))
        graph_yaml = parse_yaml(
            content(repo, manifest["workflow_sha"], "pnpm-workspace.yaml")
        )
        for key in boundary["workspace_tasks"]:
            if key in graph_yaml.get("tasks", {}):
                frozen_yaml.setdefault("tasks", {})[key] = graph_yaml["tasks"][key]
            else:
                frozen_yaml.get("tasks", {}).pop(key, None)
        if (
            git(repo, "hash-object", "--stdin", input=dump_yaml(frozen_yaml))
            != manifest["overlay_files"]["pnpm-workspace.yaml"]["blob"]
        ):
            raise ValueError("Workspace task adaptation differs from pinned graph")
    for path, record in manifest["overlay_files"].items():
        if record is None:
            if path in entries:
                raise ValueError(f"Deleted graph file returned: {path}")
        elif entries.get(path, {}).get("blob") != record["blob"]:
            raise ValueError(f"Overlay file bytes changed: {path}")
    # Only this self-binding graph manifest is excluded in addition to the
    # hard boundary. Its payload is checked above, not ignored caller data.
    check_boundary = dict(
        boundary, files=boundary["files"] + ["dev/ci/benchmark-overlay.json"]
    )
    a = application_payload(repo, frozen, check_boundary)
    b = application_payload(repo, effective, check_boundary)
    if a != b or a != manifest["application_payload_sha256"]:
        raise ValueError(
            "Effective checkout changes frozen application/test/runtime source"
        )
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("materialize")
    build.add_argument("--repo", type=Path, default=Path.cwd())
    build.add_argument("--source", required=True)
    build.add_argument("--old", required=True)
    build.add_argument("--candidate", required=True)
    build.add_argument("--output", type=Path, required=True)
    build.add_argument("--owners-sha")
    build.add_argument("--frozen-identity")
    check = sub.add_parser("verify")
    check.add_argument("--repo", type=Path, required=True)
    check.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "materialize":
            overlay(args)
        else:
            manifest = json.loads(args.manifest.read_text())
            verify(args.repo, manifest)
            print(
                json.dumps(
                    {
                        "status": "VERIFIED",
                        "source_sha": manifest["frozen_source_sha"],
                        "checkout_sha": manifest["checkout_sha"],
                        "application_payload_sha256": manifest[
                            "application_payload_sha256"
                        ],
                        "overlay_boundary_sha256": manifest["overlay_boundary_sha256"],
                        "graph_overlay_sha256": manifest["graph_overlay_sha256"],
                        "frozen_identity_sha256": manifest.get(
                            "frozen_identity_sha256"
                        ),
                    }
                )
            )
    except (ValueError, OSError, KeyError) as error:
        parser.exit(1, f"benchmark overlay: {error}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
