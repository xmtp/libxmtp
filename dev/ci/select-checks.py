#!/usr/bin/env python3
"""Select CI checks from a verified ancestor and the tested source tree."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

CHECKS = (
    "lint_workspace",
    "lint_js",
    "lint_config",
    "lint_proto",
    "lint_ios",
    "lint_android",
    "check_rust",
    "check_types",
    "test_workspace",
    "test_wasm",
    "test_node",
    "test_browser",
    "test_agent",
    "test_backend",
    "test_native_backend",
    "test_validation",
    "test_bindings",
    "test_ios",
    "test_android",
    "test_keepalive",
    "test_xdbg",
    "test_sdk_staging",
    "docs_quality",
    "docs_site",
    "docs_rust",
    "docs_swift",
    "docs_kotlin",
    "sdk_node",
    "sdk_browser",
    "backend_products",
)


def select(paths, event="pull_request", verified=False, fork=False):
    if event not in ("push", "pull_request", "workflow_dispatch"):
        raise ValueError("Unsupported CI event")
    checks = dict.fromkeys(CHECKS, False)
    checks["docs_quality"] = True

    def enable(*names):
        for name in names:
            checks[name] = True

    def full():
        checks.update(dict.fromkeys(CHECKS, True))

    def rust():
        enable(
            "lint_workspace",
            "check_rust",
            "lint_js",
            "check_types",
            "test_workspace",
            "test_wasm",
            "test_node",
            "test_browser",
            "test_agent",
            "test_backend",
            "test_native_backend",
            "test_validation",
            "test_keepalive",
            "test_xdbg",
            "test_sdk_staging",
            "docs_site",
            "docs_rust",
        )
        if event != "pull_request":
            enable("test_ios", "test_android", "docs_swift", "docs_kotlin")

    if not verified or event == "workflow_dispatch":
        full()
    else:
        for name in paths:
            if not isinstance(name, str) or not name or name.startswith("/"):
                raise ValueError("Changed path must be a repository-relative string")
            if ".." in Path(name).parts:
                raise ValueError("Changed path escapes the repository")
            if name.endswith((".toml", ".nix")) or name.startswith(
                ("sdks/ios/Sources/", "sdks/node/", "sdks/browser/")
            ):
                enable("lint_config")
            if name in (
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                "flake.nix",
                "flake.lock",
                "package.json",
                "pnpm-lock.yaml",
                "pnpm-workspace.yaml",
                "justfile",
            ) or name.startswith(("nix/", ".cargo/", ".github/", "dev/ci/")):
                full()
            elif name.startswith(("crates/", "bindings/", "proto/")):
                rust()
                if name.startswith("proto/"):
                    enable("lint_proto")
                if name.startswith(("crates/xmtp_sdk/", "bindings/")):
                    enable("test_bindings", "test_ios", "test_android", "lint_android")
            elif name.startswith("apps/xmtp_sdk_bindgen/"):
                rust()
                enable("test_bindings", "test_ios", "test_android", "lint_android")
            elif name.startswith("apps/backend/"):
                enable(
                    "lint_workspace",
                    "check_rust",
                    "test_workspace",
                    "test_backend",
                    "test_native_backend",
                    "test_node",
                    "test_browser",
                    "test_agent",
                    "test_bindings",
                    "test_sdk_staging",
                )
                if event != "pull_request":
                    enable("test_ios", "test_android")
            elif name.startswith(
                ("dev/backend/", "dev/docker/", "dev/js/")
            ) or name in (
                "dev/agent-run",
                "dev/worktree-env",
                "dev/nix-shell",
                "dev/up",
            ):
                full()
            elif name.startswith(("sdks/node/", "sdks/agent/", "apps/cli/")):
                enable(
                    "lint_js", "check_types", "test_node", "test_browser", "test_agent"
                )
            elif name.startswith(("sdks/browser/", "apps/web-chat/")):
                enable("lint_js", "check_types", "test_browser")
            elif name == "sdks/js.just":
                full()
            elif name.startswith("sdks/ios/") or name == "Package.swift":
                enable("lint_ios", "test_ios", "test_bindings", "docs_swift")
            elif name.startswith("sdks/android/"):
                enable(
                    "lint_android",
                    "test_android",
                    "test_bindings",
                    "test_sdk_staging",
                    "docs_kotlin",
                )
            elif name.startswith("apps/docs/"):
                enable("lint_js", "check_types", "docs_site")
            elif name.startswith("docs/"):
                enable("docs_site")
                if name in (
                    "docs/backend-observability.md",
                    "docs/specs/OPS-backend-operations.md",
                ) or name.startswith("docs/schemas/"):
                    enable("test_workspace", "test_backend")
            elif name.startswith("apps/keepalive-probe/"):
                enable(
                    "lint_workspace", "check_rust", "test_workspace", "test_keepalive"
                )
            elif name.startswith(
                ("apps/xmtp_debug/", "apps/db_tools/", "apps/error_glossary/")
            ):
                enable("lint_workspace", "check_rust", "test_workspace", "test_xdbg")
            elif (
                name.startswith("dev/release-tools/")
                or name.startswith((".vscode/", ".zed/"))
                or name in (".oxlintrc.json", ".oxfmtrc.json")
            ):
                enable("lint_js", "check_types")
            elif name == ".config/nextest.toml":
                full()
            elif name.endswith((".toml", ".nix")):
                enable("lint_config")
            elif name.startswith(".agents/") or name in (
                "README.md",
                "AGENTS.md",
                "CLAUDE.md",
            ):
                enable("docs_quality")
            else:
                full()

    # Every selected consumer gets its products. Full type checks read both SDKs.
    if checks["check_types"] or checks["docs_site"]:
        enable("sdk_node", "sdk_browser")
    if checks["test_node"] or checks["test_agent"]:
        enable("sdk_node")
    if checks["test_browser"]:
        enable("sdk_browser")
    if any(
        checks[name]
        for name in (
            "test_workspace",
            "test_wasm",
            "test_node",
            "test_browser",
            "test_agent",
        )
    ):
        enable("backend_products")
    if fork:
        # Native acceptance currently excludes fork PRs. Keep this explicit.
        for name in ("test_ios", "test_native_backend"):
            checks[name] = False
    return {"schema_version": 1, "checks": checks}


def git(*args):
    return subprocess.check_output(["git", *args]).decode().strip()


def graph_matches(base):
    files = (
        subprocess.check_output(
            ["git", "ls-files", "-z", ".github/workflows", ".github/actions", "dev/ci"]
        )
        .decode()
        .split("\0")
    )
    for name in filter(None, files):
        try:
            old = subprocess.check_output(
                ["git", "show", f"{base}:{name}"], stderr=subprocess.DEVNULL
            )
        except subprocess.CalledProcessError:
            return False
        if old != Path(name).read_bytes():
            return False
    return True


def base_verified(base, repository):
    if not graph_matches(base):
        return False
    try:
        response = subprocess.check_output(
            [
                "gh",
                "api",
                f"repos/{repository}/commits/{base}/check-runs?per_page=100",
            ]
        )
        records = json.loads(response)["check_runs"]
    except (subprocess.CalledProcessError, KeyError, json.JSONDecodeError):
        return False
    for name in ("Lint", "Test"):
        candidates = [
            r
            for r in records
            if r["name"] == name and r.get("app", {}).get("slug") == "github-actions"
        ]
        if not candidates:
            return False
        latest = max(candidates, key=lambda r: r["id"])
        if latest.get("status") != "completed" or latest.get("conclusion") != "success":
            return False
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--paths-json", help="Offline fixture input; no GitHub reads")
    parser.add_argument(
        "--verified", action="store_true", help="Fixture input has a verified base"
    )
    parser.add_argument(
        "--event", default=os.environ.get("GITHUB_EVENT_NAME", "pull_request")
    )
    parser.add_argument("--fork", action="store_true")
    parser.add_argument("--output")
    args = parser.parse_args()
    if args.paths_json:
        paths = json.loads(Path(args.paths_json).read_text())
        if not isinstance(paths, list):
            raise ValueError("Changed paths must be an array")
        verified = args.verified
    else:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        head = git("rev-parse", "HEAD")
        if args.event == "pull_request":
            base = git("rev-parse", "HEAD^1")
            args.fork = (
                event["pull_request"]["head"]["repo"]["full_name"]
                != os.environ["GITHUB_REPOSITORY"]
            )
        elif args.event == "push":
            base = event.get("before", "")
        else:
            base = ""
        verified = False
        paths = []
        if base and set(base) != {"0"}:
            try:
                subprocess.run(
                    ["git", "merge-base", "--is-ancestor", base, head],
                    check=True,
                    capture_output=True,
                )
                verified = base_verified(base, os.environ["GITHUB_REPOSITORY"])
                if verified:
                    paths = (
                        subprocess.check_output(
                            ["git", "diff", "--name-only", "-z", base, head]
                        )
                        .decode()
                        .split("\0")
                    )
                    paths = list(filter(None, paths))
            except subprocess.CalledProcessError:
                verified = False
    result = select(paths, args.event, verified, args.fork)
    encoded = json.dumps(result, separators=(",", ":"))
    if args.output:
        Path(args.output).write_text(encoded + "\n")
    print(encoded)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        print(f"CI selection failed: {error}", file=sys.stderr)
        sys.exit(1)
