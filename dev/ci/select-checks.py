#!/usr/bin/env python3
"""Select required checks from the event's Git diff; unknown inputs run all."""

import argparse
import json
import os
from pathlib import Path
import subprocess

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
CHECKS = (
    SOURCE_SUITES
    + TEST_SUITES
    + (
        "lint_ios",
        "lint_android",
        "check_rust",
        "check_types",
        "check_sdk",
        "check_sdk_unit",
        "check_bindings_ios",
        "check_bindings_android",
        "test_ios",
        "test_ios_platform",
        "test_android",
        "test_android_consumers",
        "test_android_platform",
        "test_swift_lifecycle",
        "docs_quality",
        "docs_site",
        "docs_rust",
    )
)
RUST_OWNERS = {
    "xmtp_api",
    "xmtp_api_grpc",
    "xmtp_archive",
    "xmtp_attachments",
    "xmtp_common",
    "xmtp_content_types",
    "xmtp_cryptography",
    "xmtp_db",
    "xmtp_events",
    "xmtp_id",
    "xmtp_logging",
    "xmtp_macro",
    "xmtp_mls",
    "xmtp_mls_common",
    "xmtp_mls_validation",
    "xmtp_proto",
    "xmtp_user_preferences",
    "xmtp-workspace-hack",
}


def select(paths, event="pull_request", fork=False):
    checks = dict.fromkeys(CHECKS, False)
    checks["docs_quality"] = True

    def enable(*names):
        checks.update(dict.fromkeys(names, True))

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

    def ios(platform=False, public=False):
        enable("lint_ios", "test_ios", "test_swift_lifecycle")
        if platform:
            enable("test_ios_platform", "check_bindings_ios")
        if public:
            enable("docs_site")

    def android(platform=False, public=False):
        enable("lint_android", "test_android", "test_android_consumers")
        if platform:
            enable(
                "test_android_platform", "check_bindings_android", "test_sdk_staging"
            )
        if public:
            enable("docs_site")

    if paths is None or event not in ("pull_request", "push"):
        full()
    else:
        for name in paths:
            parts = Path(name).parts
            if not name or name.startswith("/") or ".." in parts:
                full()
                continue
            if name.endswith((".toml", ".nix")):
                enable("lint_config")
            if name.startswith(
                (
                    ".github/",
                    "nix/",
                    "dev/",
                    ".cargo/",
                    "proto/",
                    "bindings/",
                    "apps/backend/",
                    "apps/xmtp_sdk_bindgen/",
                    "crates/xmtp_sdk/",
                    "crates/xmtp_configuration/",
                    "docs/specs/",
                    "docs/schemas/",
                )
            ):
                full()
            elif name in (
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
            ):
                full()
            elif name.endswith(
                (
                    "/Cargo.toml",
                    "/build.rs",
                    "/package.json",
                    ".nix",
                    ".toml",
                    ".lock",
                    ".gradle",
                    ".kts",
                )
            ):
                full()
            elif (
                len(parts) >= 4
                and parts[0] == "crates"
                and parts[1] in RUST_OWNERS
                and parts[2] in ("src", "tests", "benches", "examples")
                and name.endswith(".rs")
            ):
                rust()
                if event == "push":
                    ios()
                    android()
                    enable(
                        "lint_js",
                        "check_types",
                        "check_sdk",
                        "test_node",
                        "test_agent",
                        "test_browser",
                        "test_bridge_runtime",
                        "test_browser_platform",
                    )
            elif name.startswith("sdks/ios/") and name.endswith(".swift"):
                ios(
                    platform=not name.startswith("sdks/ios/Tests/"),
                    public=not name.startswith("sdks/ios/Tests/"),
                )
                if name.endswith("AppleLifecycleTests.swift"):
                    enable("test_ios_platform")
            elif name.startswith("sdks/android/") and name.endswith((".kt", ".java")):
                test = name.startswith("sdks/android/library/src/test/")
                android(
                    platform=not test, public=not test and "/androidTest/" not in name
                )
            elif name.startswith(("sdks/node/", "apps/cli/")) and name.endswith(
                (".ts", ".tsx", ".js", ".mjs")
            ):
                enable(
                    "lint_js", "lint_config", "check_types", "test_node", "test_agent"
                )
                if name.startswith(
                    (
                        "sdks/node/src/",
                        "sdks/node/examples/",
                        "sdks/node/type-tests/publicSurface",
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
            elif name.startswith(("sdks/browser/", "apps/web-chat/")) and name.endswith(
                (".ts", ".tsx", ".js", ".mjs")
            ):
                enable(
                    "lint_js",
                    "lint_config",
                    "check_types",
                    "test_browser",
                    "test_bridge_runtime",
                    "test_browser_platform",
                )
                if name.startswith("sdks/browser/test/platform/"):
                    enable("check_sdk")
                if name.startswith(
                    (
                        "sdks/browser/src/",
                        "sdks/browser/examples/",
                        "sdks/browser/type-tests/publicSurface",
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
            elif name.startswith("sdks/agent/") and name.endswith(
                (".ts", ".tsx", ".js", ".mjs")
            ):
                enable("lint_js", "check_types", "test_agent")
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
            elif name.startswith("apps/docs/"):
                enable("lint_js", "check_types", "docs_site")
            elif name.startswith("docs/") and name.endswith((".md", ".mdx")):
                enable("docs_site")
            elif name.startswith((".agents/", ".vscode/", ".zed/")) or name in (
                "README.md",
                "AGENTS.md",
                "CLAUDE.md",
            ):
                pass
            else:
                full()
    checks["test_bindings"] = (
        checks["check_bindings_ios"] or checks["check_bindings_android"]
    )
    if fork:
        for name in (
            "test_ios",
            "test_ios_platform",
            "test_swift_lifecycle",
            "test_native_backend",
        ):
            checks[name] = False
    checks["source_lint"] = any(checks[name] for name in SOURCE_SUITES)
    checks["tests"] = any(checks[name] for name in TEST_SUITES)
    lint_jobs = {
        "source-lint": checks["source_lint"],
        "docs-quality": checks["docs_quality"],
        "lint-ios": checks["lint_ios"],
        "lint-android": checks["lint_android"],
    }
    test_jobs = {
        "check-rust": checks["check_rust"],
        "check-types": checks["check_types"],
        "check-sdk": checks["check_sdk"],
        "check-sdk-unit": checks["check_sdk_unit"],
        "tests": checks["tests"],
        "docs": checks["docs_site"],
        "test-ios": checks["test_ios"] or checks["test_ios_platform"],
        "test-android": checks["test_android"]
        or checks["test_android_consumers"]
        or checks["test_android_platform"],
        "test-swift-lifecycle": checks["test_swift_lifecycle"],
        "docs-rust-reference": checks["docs_rust"] and not checks["docs_site"],
    }
    return {
        "checks": checks,
        "lint_jobs": ["detect-changes"]
        + [job for job, run in lint_jobs.items() if run],
        "test_jobs": ["detect-changes"]
        + [job for job, run in test_jobs.items() if run],
    }


def changed_paths(event):
    """PR merge parent is the actual base; push uses the event's before SHA."""
    kind = os.environ.get("GITHUB_EVENT_NAME")
    try:
        base = (
            "HEAD^1"
            if kind == "pull_request"
            else event["before"]
            if kind == "push"
            else ""
        )
        if not base or set(base) == {"0"}:
            return None
        if kind == "pull_request":
            parents = subprocess.check_output(
                ["git", "show", "-s", "--format=%P", "HEAD"], text=True
            ).split()
            if len(parents) != 2:
                return None
        data = subprocess.check_output(
            ["git", "diff", "--no-renames", "--name-only", "-z", base, "HEAD"],
            stderr=subprocess.DEVNULL,
        )
        return list(filter(None, data.decode().split("\0")))
    except (KeyError, subprocess.CalledProcessError):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--source-suites-output", required=True)
    parser.add_argument("--test-suites-output", required=True)
    args = parser.parse_args()
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    repository = event.get("pull_request", {}).get("head", {}).get("repo")
    fork = os.environ.get("GITHUB_EVENT_NAME") == "pull_request" and (
        not isinstance(repository, dict)
        or repository.get("full_name") != os.environ["GITHUB_REPOSITORY"]
    )
    result = select(changed_paths(event), os.environ.get("GITHUB_EVENT_NAME"), fork)
    for filename, value in (
        (args.output, result),
        (args.source_suites_output, [n for n in SOURCE_SUITES if result["checks"][n]]),
        (args.test_suites_output, [n for n in TEST_SUITES if result["checks"][n]]),
    ):
        Path(filename).write_text(json.dumps(value, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
