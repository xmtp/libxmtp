#!/usr/bin/env python3
"""Select full Nix warming unless an unchanged workflow proves inputs unchanged."""

import argparse
import ast
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

WORKFLOW = ".github/workflows/fh-cache.yml"
GRAPH = (".github/workflows", ".github/actions", "dev/ci")
EMBEDDED_DOCS = {
    "docs/backend-observability.md",
    "docs/specs/OPS-backend-operations.md",
    "docs/schemas/backend-v1.json",
}
WEB_ROOTS = (
    "sdks/node/",
    "sdks/browser/",
    "sdks/agent/",
    "apps/cli/",
    "apps/docs/",
    "apps/web-chat/",
    "dev/release-tools/src/",
)
WEB_EXTENSIONS = {
    ".js",
    ".mjs",
    ".cjs",
    ".ts",
    ".mts",
    ".cts",
    ".tsx",
    ".jsx",
    ".css",
    ".scss",
    ".svg",
    ".html",
}
SHA = re.compile(r"[0-9a-f]{40}")
# Review the skip paths again before updating this input contract.
AUDITED_INPUT_CONTRACT = (
    "844a3f6137c2e2e5785b19f803be81d053f8ee1990876b615e8dee2b14ddf93d"
)
SELECTOR_PATH = "dev/ci/select-nix-outputs.py"


def valid_path(name):
    if not isinstance(name, str) or not name or "\\" in name or "\x00" in name:
        return False
    path = PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts and str(path) == name


def irrelevant(name):
    if not valid_path(name) or name in EMBEDDED_DOCS:
        return False
    path = PurePosixPath(name)
    if ".config." in path.name:
        return False
    # Rust source and unknown file types can add new embedded input contracts.
    if name.startswith("docs/") and path.suffix in {".md", ".markdown"}:
        return True
    if name in {"README.md", "AGENTS.md", "CLAUDE.md"}:
        return True
    return name.startswith(WEB_ROOTS) and path.suffix in WEB_EXTENSIONS


def select(paths, verified=False, event="push", ref="refs/heads/self-hosted"):
    if not isinstance(paths, list) or any(not valid_path(path) for path in paths):
        raise ValueError("Changed paths must be valid repository paths")
    full = not (
        verified
        and event == "push"
        and ref in {"refs/heads/main", "refs/heads/self-hosted"}
        and all(irrelevant(path) for path in paths)
    )
    return {"schemaVersion": 1, "full": full}


def command(args):
    return subprocess.check_output(args, stderr=subprocess.DEVNULL, timeout=20)


def github(repository, endpoint):
    return json.loads(command(["gh", "api", f"repos/{repository}/{endpoint}"]))


def normalized_selector(data):
    """Keep the selector code in the pin without hashing the pin's own value."""
    text = data.decode()
    nodes = [
        node
        for node in ast.parse(text).body
        if isinstance(node, ast.Assign)
        and len(node.targets) == 1
        and isinstance(node.targets[0], ast.Name)
        and node.targets[0].id == "AUDITED_INPUT_CONTRACT"
    ]
    if (
        len(nodes) != 1
        or not isinstance(nodes[0].value, ast.Constant)
        or not isinstance(nodes[0].value.value, str)
    ):
        raise ValueError("Audited input contract must have one literal assignment")
    node = nodes[0]
    lines = text.splitlines(keepends=True)
    lines[node.lineno - 1 : node.end_lineno] = [
        "AUDITED_INPUT_CONTRACT = '<reviewed-value>'\n"
    ]
    return "".join(lines).encode()


def contract_fingerprint(head):
    records = command(["git", "ls-tree", "-r", head]).splitlines()
    selected = []
    for record in records:
        metadata, name_bytes = record.split(b"\t", 1)
        name = name_bytes.decode()
        if irrelevant(name):
            continue
        if name == SELECTOR_PATH:
            # Only the literal pin value is omitted. Readers and selection logic
            # stay protected, including this normalization code itself.
            source = command(["git", "show", f"{head}:{SELECTOR_PATH}"])
            mode, kind, _ = metadata.split()
            code_hash = hashlib.sha256(normalized_selector(source)).hexdigest()
            record = b" ".join([mode, kind, code_hash.encode()]) + b"\t" + name_bytes
        selected.append(record)
    return hashlib.sha256(b"\n".join(selected) + b"\n").hexdigest()


def input_contract_matches(head):
    return contract_fingerprint(head) == AUDITED_INPUT_CONTRACT


def graph_matches(base, head):
    # Git compares added and deleted paths too. A deleted selector cannot be ignored.
    return not command(["git", "diff", "--name-only", "-z", base, head, "--", *GRAPH])


def latest_base_success(base, repository, branch):
    workflow = github(repository, "actions/workflows/fh-cache.yml")
    workflow_id = workflow["id"]
    if type(workflow_id) is not int or workflow_id <= 0 or workflow["path"] != WORKFLOW:
        return False
    records = github(
        repository,
        f"actions/workflows/{workflow_id}/runs?head_sha={base}&event=push&per_page=100",
    )["workflow_runs"]
    if not isinstance(records, list) or not records:
        return False
    # A newer failed, cancelled, pending, or rerun attempt invalidates an old success.
    if any(
        not isinstance(record, dict)
        or type(record.get("id")) is not int
        or record["id"] <= 0
        for record in records
    ):
        return False
    latest = max(records, key=lambda record: record["id"])
    return (
        latest.get("workflow_id") == workflow_id
        and latest.get("path") == WORKFLOW
        and latest.get("head_sha") == base
        and latest.get("head_branch") == branch
        and latest.get("event") == "push"
        and latest.get("status") == "completed"
        and latest.get("conclusion") == "success"
        and type(latest.get("run_attempt")) is int
        and latest["run_attempt"] > 0
    )


def changed_inputs(event, ref, event_file, repository, workflow_ref, expected_sha):
    if event != "push" or ref not in {"refs/heads/main", "refs/heads/self-hosted"}:
        return [], False, "Manual and tag runs validate all outputs"
    try:
        if workflow_ref != f"{repository}/{WORKFLOW}@{ref}":
            return [], False, "Workflow identity is not the branch workflow"
        payload = json.loads(Path(event_file).read_text())
        if not isinstance(payload, dict):
            return [], False, "Push event is malformed"
        base = payload.get("before", "")
        head = command(["git", "rev-parse", "HEAD"]).decode().strip()
        if not SHA.fullmatch(base) or base == "0" * 40 or head != expected_sha:
            return [], False, "Push ancestor or checkout identity is missing"
        if payload.get("ref") != ref or payload.get("after") != head:
            return [], False, "Push event differs from the checkout"
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", base, head],
            check=True,
            capture_output=True,
            timeout=20,
        )
        if not input_contract_matches(head):
            return (
                [],
                False,
                "Build input contract changed; review skip paths before updating its pin",
            )
        if not graph_matches(base, head):
            return [], False, "CI graph differs from the ancestor"
        if not latest_base_success(base, repository, ref.removeprefix("refs/heads/")):
            return [], False, "Ancestor has no current successful warming run"
        paths = list(
            filter(
                None,
                command(["git", "diff", "--name-only", "-z", base, head])
                .decode()
                .split("\0"),
            )
        )
        return paths, True, "Same workflow verified the unchanged ancestor graph"
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        return [], False, "Ancestor proof is unavailable; validate all outputs"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--paths-json", help="Offline path fixture; no GitHub reads")
    parser.add_argument(
        "--verified",
        action="store_true",
        help="Offline fixture has a verified ancestor",
    )
    parser.add_argument("--output")
    parser.add_argument("--github-output")
    args = parser.parse_args()
    event = os.environ.get("GITHUB_EVENT_NAME", "workflow_dispatch")
    ref = os.environ.get("GITHUB_REF", "")
    if args.paths_json:
        paths = json.loads(Path(args.paths_json).read_text())
        verified = args.verified
        reason = "Offline path fixture"
    else:
        paths, verified, reason = changed_inputs(
            event,
            ref,
            os.environ.get("GITHUB_EVENT_PATH", ""),
            os.environ.get("GITHUB_REPOSITORY", ""),
            os.environ.get("GITHUB_WORKFLOW_REF", ""),
            os.environ.get("GITHUB_SHA", ""),
        )
    result = select(paths, verified, event, ref)
    result.update({"verified": verified, "reason": reason, "paths": paths})
    encoded = json.dumps(result, separators=(",", ":"))
    if args.output:
        Path(args.output).write_text(encoded + "\n")
    if args.github_output:
        with Path(args.github_output).open("a") as output:
            output.write(f"full={str(result['full']).lower()}\n")
    print(encoded)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, TypeError) as error:
        print(f"Nix output selection failed: {error}", file=sys.stderr)
        sys.exit(1)
