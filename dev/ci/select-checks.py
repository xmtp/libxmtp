#!/usr/bin/env python3
"""Select CI checks from a verified ancestor and the tested source tree."""

import argparse
import json
import os
from pathlib import Path
import re
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
    "check_sdk",
    "check_sdk_unit",
    "check_bindings_ios",
    "check_bindings_android",
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
    "test_ios_platform",
    "test_android_platform",
    "test_android_consumers",
    "test_keepalive",
    "test_xdbg",
    "test_sdk_staging",
    "test_bridge_runtime",
    "test_browser_platform",
    "test_swift_seams",
    "docs_quality",
    "docs_site",
    "docs_rust",
    "docs_swift",
    "docs_kotlin",
    "sdk_node",
    "sdk_browser",
    "backend_products",
)


SOURCE_SUITES = ("lint_workspace", "lint_js", "lint_config", "lint_proto")
TEST_SUITES = (
    "test_native_backend",
    "test_validation",
    "test_backend",
    "test_workspace",
    "test_keepalive",
    "test_wasm",
    "test_node",
    "test_agent",
    "test_xdbg",
    "test_browser",
    "test_bindings",
    "test_sdk_staging",
    "test_bridge_runtime",
    "test_browser_platform",
)
# New owners and non-source inputs keep full coverage until their readers are known.
RUST_OWNERS = {
    "crates/xmtp_api",
    "crates/xmtp_api_grpc",
    "crates/xmtp_archive",
    "crates/xmtp_attachments",
    "crates/xmtp_common",
    "crates/xmtp_content_types",
    "crates/xmtp_cryptography",
    "crates/xmtp_db",
    "crates/xmtp_events",
    "crates/xmtp_id",
    "crates/xmtp_logging",
    "crates/xmtp_macro",
    "crates/xmtp_mls",
    "crates/xmtp_mls_common",
    "crates/xmtp_mls_validation",
    "crates/xmtp_proto",
    "crates/xmtp_push_types",
    "apps/chaos",
    "apps/db_tools",
    "apps/error_glossary",
    "apps/keepalive-probe",
    "apps/xmtp_debug",
}


def pure_rust_source(name):
    parts = Path(name).parts
    return (
        len(parts) >= 4
        and "/".join(parts[:2]) in RUST_OWNERS
        and parts[2] in {"src", "tests", "benches", "examples"}
        and name.endswith(".rs")
    )


def suites(selection, names):
    return [name for name in names if selection["checks"][name]]


def select(paths, event="pull_request", verified=False, fork=False):
    if event not in ("push", "pull_request", "workflow_dispatch"):
        raise ValueError("Unsupported CI event")
    for name in paths:
        if not isinstance(name, str) or not name or name.startswith("/"):
            raise ValueError("Changed path must be a repository-relative string")
        if ".." in Path(name).parts:
            raise ValueError("Changed path escapes the repository")
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
            "test_workspace",
            "test_wasm",
            "test_backend",
            "test_native_backend",
            "test_validation",
            "test_keepalive",
            "test_xdbg",
            "check_sdk_unit",
            "docs_rust",
        )

    def ios(native=False, public=False):
        enable("lint_ios", "test_ios", "test_swift_seams")
        if native:
            enable("check_bindings_ios", "test_ios_platform")
        if public:
            enable("docs_swift", "docs_site")

    def android(native=False, public=False):
        enable("lint_android", "test_android", "test_android_consumers")
        if native:
            enable(
                "check_bindings_android", "test_android_platform", "test_sdk_staging"
            )
        if public:
            enable("docs_kotlin", "docs_site")

    def core_push():
        # Postmerge checks retain language units and consumers. Native packaging
        # and the docs site need their own inputs.
        rust()
        ios()
        android()
        enable(
            "lint_js",
            "check_types",
            "check_sdk",
            "test_node",
            "test_browser",
            "test_agent",
            "test_bridge_runtime",
            "test_browser_platform",
        )

    # Narrowing requires the same graph and a successful ancestor. Core Rust
    # pushes retain language boundary checks; unrelated prose remains narrow.
    if not verified or event == "workflow_dispatch":
        full()
    else:
        for name in paths:
            # Tree formatting reads these source owners as well as config files.
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
                "sdks/js.just",
                ".config/nextest.toml",
            ) or name.startswith(("nix/", ".cargo/", ".github/", "dev/ci/")):
                full()
            elif name.endswith((".nix", "/Cargo.toml", "/build.rs", "/package.json")):
                full()
            elif name.startswith(
                (
                    "crates/xmtp_sdk/",
                    "crates/xmtp_configuration/",
                    "bindings/",
                    "apps/xmtp_sdk_bindgen/",
                    "proto/",
                    "apps/backend/",
                    "crates/xmtp_api_backend/",
                    "crates/xmtp_attachments_server/",
                    "dev/backend/",
                    "dev/docker/",
                    "dev/js/",
                    "sdks/node/runtime/",
                    "sdks/browser/runtime/",
                    "sdks/agent/runtime/",
                )
            ) or name in (
                "dev/agent-run",
                "dev/worktree-env",
                "dev/nix-shell",
                "dev/up",
            ):
                full()
            elif pure_rust_source(name):
                core_push() if event == "push" else rust()
            elif name.startswith(
                (
                    "crates/",
                    "apps/chaos/",
                    "apps/db_tools/",
                    "apps/error_glossary/",
                    "apps/keepalive-probe/",
                    "apps/xmtp_debug/",
                )
            ):
                full()
            elif name.startswith("sdks/browser/test/platform/"):
                enable(
                    "test_bridge_runtime",
                    "test_browser_platform",
                    "check_sdk",
                    "lint_js",
                    "lint_config",
                )
            elif name == "sdks/ios/script/check-consumer.sh":
                ios()
            elif name == "sdks/android/dev/check-consumers":
                android()
            elif name == "sdks/ios/ios.just" or name.startswith("sdks/ios/dev/"):
                ios(native=True, public=True)
            elif name == "sdks/android/android.just":
                android(native=True, public=True)
            elif name.startswith(("sdks/node/", "apps/cli/")) and name.endswith(
                (".ts", ".tsx", ".js", ".mjs")
            ):
                enable("lint_js", "check_types", "test_node", "test_agent")
                if name.startswith(
                    (
                        "sdks/node/src/",
                        "sdks/node/type-tests/publicSurface",
                        "sdks/node/examples/",
                        "sdks/node/scripts/",
                    )
                ):
                    enable("docs_site")
                elif not name.startswith(
                    (
                        "sdks/node/test/",
                        "sdks/node/type-tests/",
                        "apps/cli/src/",
                        "apps/cli/test/",
                    )
                ) and name not in (
                    "sdks/node/vitest.config.ts",
                    "sdks/node/vitest.setup.ts",
                ):
                    full()
            elif name.startswith("sdks/agent/") and name.endswith(
                (".ts", ".tsx", ".js", ".mjs")
            ):
                enable("lint_js", "check_types", "test_agent")
                # The Agent entry point exports these source owners. Test files
                # and the test-only utility are separate inputs.
                if (
                    name.startswith("sdks/agent/src/")
                    and not name.endswith((".test.ts", ".test.tsx"))
                    and name != "sdks/agent/src/util/test.ts"
                ):
                    enable("docs_site")
                elif not name.startswith(
                    ("sdks/agent/src/", "sdks/agent/test/")
                ) and name not in (
                    "sdks/agent/vitest.config.ts",
                    "sdks/agent/vitest.setup.ts",
                ):
                    full()
            elif name.startswith(("sdks/browser/", "apps/web-chat/")) and name.endswith(
                (".ts", ".tsx", ".js", ".mjs")
            ):
                enable(
                    "lint_js",
                    "check_types",
                    "test_browser",
                    "test_bridge_runtime",
                    "test_browser_platform",
                )
                if name.startswith(
                    (
                        "sdks/browser/src/",
                        "sdks/browser/type-tests/publicSurface",
                        "sdks/browser/examples/",
                    )
                ):
                    enable("docs_site")
                elif not name.startswith(
                    (
                        "sdks/browser/test/",
                        "sdks/browser/type-tests/",
                        "apps/web-chat/src/",
                        "apps/web-chat/test/",
                    )
                ):
                    full()
            elif (
                name.startswith("sdks/ios/")
                and name.endswith(".swift")
                or name == "Package.swift"
            ):
                if name.startswith("sdks/ios/Tests/"):
                    ios()
                    if name == "sdks/ios/Tests/XmtpSdkTests/AppleLifecycleTests.swift":
                        enable("test_ios_platform")
                else:
                    # Sources contain public Swift declarations. No AST claim
                    # narrows them to private implementation files.
                    ios(native=True, public=True)
            elif name.startswith("sdks/android/") and name.endswith(
                (".kt", ".kts", ".java", ".gradle", ".xml")
            ):
                if name.startswith("sdks/android/library/src/test/"):
                    android()
                elif name.startswith("sdks/android/library/src/androidTest/"):
                    android()
                    enable("test_android_platform")
                else:
                    android(native=True, public=True)
            elif name.startswith(("sdks/ios/example/", "sdks/ios/XMTPiOSExample/")):
                ios(native=True, public=True)
            elif name.startswith("sdks/android/example/"):
                android(native=True, public=True)
            elif name.startswith(("docs/specs/", "docs/schemas/")) or (
                name.startswith(("docs/", "apps/docs/"))
                and name.endswith((".toml", ".json", ".yaml", ".yml"))
            ):
                full()
            elif name.startswith("apps/docs/"):
                enable("lint_js", "check_types", "docs_site")
            elif name.startswith("docs/"):
                enable("docs_site")
                if name in (
                    "docs/backend-observability.md",
                    "docs/specs/OPS-backend-operations.md",
                ) or name.startswith("docs/schemas/"):
                    enable("test_workspace", "test_backend")
            elif name.startswith((".agents/", ".vscode/", ".zed/")) or name in (
                "README.md",
                "AGENTS.md",
                "CLAUDE.md",
            ):
                enable("docs_quality")
            elif name in (".oxlintrc.json", ".oxfmtrc.json"):
                enable("lint_js", "check_types")
            else:
                full()

    checks["test_bindings"] = (
        checks["check_bindings_ios"] or checks["check_bindings_android"]
    )
    if checks["check_types"] or checks["check_sdk"] or checks["docs_site"]:
        enable("sdk_node", "sdk_browser")
    if checks["test_node"] or checks["test_agent"]:
        enable("sdk_node")
    if (
        checks["test_browser"]
        or checks["test_bridge_runtime"]
        or checks["test_browser_platform"]
    ):
        enable("sdk_browser")
    if any(
        checks[name]
        for name in (
            "test_workspace",
            "test_wasm",
            "test_node",
            "test_browser",
            "test_agent",
            "check_sdk_unit",
            "test_bridge_runtime",
            "test_browser_platform",
        )
    ):
        enable("backend_products")
    if fork:
        for name in (
            "test_ios",
            "test_ios_platform",
            "test_native_backend",
            "test_swift_seams",
        ):
            checks[name] = False
    checks["source_lint"] = any(checks[name] for name in SOURCE_SUITES)
    checks["tests"] = any(checks[name] for name in TEST_SUITES)
    return {"schema_version": 1, "checks": checks}


def git(*args):
    return subprocess.check_output(["git", *args]).decode().strip()


def fetch_commit(commit, depth):
    """Fetch one exact commit and a bounded amount of its history."""
    if not re.fullmatch(r"[0-9a-fA-F]{40}", commit):
        return False
    try:
        subprocess.run(
            ["git", "fetch", "--no-tags", f"--depth={depth}", "origin", commit],
            check=True,
            capture_output=True,
            timeout=30,
            env=dict(os.environ, GIT_TERMINAL_PROMPT="0"),
        )
        print(f"CI history fetched exact {commit} at depth {depth}", file=sys.stderr)
        return True
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
        print("CI history fetch failed; select all checks", file=sys.stderr)
        return False


def ensure_ancestor(base, head):
    """Require a real ancestry proof; missing shallow edges are not proof."""
    if not all(re.fullmatch(r"[0-9a-fA-F]{40}", value) for value in (base, head)):
        return False
    present = (
        subprocess.run(
            ["git", "cat-file", "-e", f"{base}^{{commit}}"], capture_output=True
        ).returncode
        == 0
    )
    if not present and not fetch_commit(base, 1):
        return False

    def ancestor():
        return (
            subprocess.run(
                ["git", "merge-base", "--is-ancestor", base, head],
                capture_output=True,
            ).returncode
            == 0
        )

    if ancestor():
        return True
    if git("rev-parse", "--is-shallow-repository") != "true":
        return False
    for depth in (32, 128):
        if not fetch_commit(head, depth):
            return False
        if ancestor():
            return True
    print("CI ancestry is not proved; select all checks", file=sys.stderr)
    return False


def merge_parent(head):
    # Read the actual commit object even when a shallow boundary hides its
    # parents from revision walking. A PR checkout must be a real merge.
    parents = [
        line.split()[1]
        for line in git("cat-file", "-p", head).split("\n\n", 1)[0].splitlines()
        if line.startswith("parent ")
    ]
    return parents[0] if len(parents) == 2 else ""


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
    parser.add_argument("--source-suites-output")
    parser.add_argument("--test-suites-output")
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
            base = merge_parent(head)
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
                verified = ensure_ancestor(base, head) and base_verified(
                    base, os.environ["GITHUB_REPOSITORY"]
                )
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
    for output, names in (
        (args.source_suites_output, SOURCE_SUITES),
        (args.test_suites_output, TEST_SUITES),
    ):
        if output:
            Path(output).write_text(
                json.dumps(suites(result, names), separators=(",", ":")) + "\n"
            )
    print(encoded)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        print(f"CI selection failed: {error}", file=sys.stderr)
        sys.exit(1)
