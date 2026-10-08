#!/usr/bin/env python3
"""Check retained selection boundaries and the actual workflow result gates."""

import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import yaml

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "selection", Path(__file__).with_name("select-checks.py")
)
selection = importlib.util.module_from_spec(spec)
spec.loader.exec_module(selection)


def workflow(name):
    value = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
    if True in value:
        value["on"] = value.pop(True)
    return value


def passed(expression, checks=None, results=None, inputs=None, event="pull_request"):
    text = expression.removeprefix("${{").removesuffix("}}").strip()
    text = re.sub(
        r"fromJSON\(needs.detect-changes.outputs.selection\).checks.([a-z_]+)",
        lambda m: "checks.get(" + repr(m[1]) + ")",
        text,
    )
    text = re.sub(
        r"needs.([a-z_-]+).result", lambda m: "results.get(" + repr(m[1]) + ")", text
    )
    text = re.sub(
        r"inputs.([a-z_-]+)", lambda m: "inputs.get(" + repr(m[1]) + ")", text
    )
    text = text.replace("github.event_name", "event").replace("github.ref", "ref")
    text = text.replace("&&", " and ").replace("||", " or ")
    text = re.sub(r"!(?!=)", " not ", text)
    return bool(
        eval(
            text,
            {"__builtins__": {}},
            {
                "checks": checks or {},
                "results": results or {},
                "inputs": inputs or {},
                "event": event,
                "ref": "refs/heads/self-hosted",
            },
        )
    )


class SelectionTests(unittest.TestCase):
    def test_pure_rust_pr_keeps_rust_and_omits_hosts(self):
        checks = selection.select(["crates/xmtp_mls/src/lib.rs"])["checks"]
        expected = {
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
            "docs_quality",
        }
        self.assertEqual({name for name in selection.CHECKS if checks[name]}, expected)

    def test_core_push_keeps_units_and_consumers_without_platform_packages(self):
        checks = selection.select(["crates/xmtp_mls/src/lib.rs"], "push")["checks"]
        for name in (
            "test_ios",
            "test_swift_seams",
            "test_android",
            "test_android_consumers",
            "test_node",
            "test_browser",
            "test_agent",
            "check_sdk",
        ):
            self.assertTrue(checks[name], name)
        for name in (
            "test_ios_platform",
            "test_android_platform",
            "test_sdk_staging",
            "docs_site",
        ):
            self.assertFalse(checks[name], name)

    def test_build_dependency_generator_service_and_unknown_inputs_run_all(self):
        for path in (
            "Cargo.lock",
            ".config/hakari.toml",
            "nix/new.nix",
            "apps/new/Cargo.toml",
            "apps/backend/src/main.rs",
            "apps/xmtp_sdk_bindgen/src/lib.rs",
            "crates/xmtp_sdk/src/lib.rs",
            "crates/new/src/lib.rs",
            ".github/workflows/test.yml",
            "dev/new-helper",
            "unknown",
        ):
            with self.subTest(path=path):
                self.assertTrue(
                    all(
                        selection.select([path])["checks"][name]
                        for name in selection.CHECKS
                    )
                )

    def test_docs_and_native_input_boundaries(self):
        self.assertTrue(selection.select(["docs/guide.md"])["checks"]["docs_site"])
        self.assertFalse(
            selection.select(["sdks/ios/Tests/XmtpSdkTests/ReaderTeardownTests.swift"])[
                "checks"
            ]["docs_site"]
        )
        for path in (
            "sdks/ios/Sources/XmtpSdk/Client.swift",
            "sdks/android/library/src/main/Client.kt",
            "sdks/node/src/index.ts",
            "sdks/browser/src/index.ts",
        ):
            self.assertTrue(selection.select([path])["checks"]["docs_site"], path)
        c = selection.select(["sdks/android/library/src/test/ReaderTest.kt"])["checks"]
        self.assertTrue(c["test_android_consumers"])
        self.assertFalse(c["test_android_platform"])
        c = selection.select(
            ["sdks/android/library/src/androidTest/AndroidPackageTest.kt"]
        )["checks"]
        self.assertTrue(c["test_android_platform"])

    def test_manual_missing_diff_and_fork_policy(self):
        self.assertTrue(
            all(selection.select(None)["checks"][n] for n in selection.CHECKS)
        )
        self.assertTrue(
            all(
                selection.select([], "workflow_dispatch")["checks"][n]
                for n in selection.CHECKS
            )
        )
        c = selection.select(None, fork=True)["checks"]
        for name in (
            "test_native_backend",
            "test_ios",
            "test_ios_platform",
            "test_swift_seams",
        ):
            self.assertFalse(c[name])
        self.assertTrue(c["test_android_platform"])

    def test_browser_platform_keeps_explicit_generated_type_checks(self):
        for event in ("pull_request", "push"):
            for filename in (
                "suite.worker.ts",
                "attachment-lifetime.chromium.ts",
                "storage.layout.chromium.ts",
            ):
                with self.subTest(event=event, filename=filename):
                    checks = selection.select(
                        ["sdks/browser/test/platform/" + filename], event
                    )["checks"]
                    self.assertTrue(checks["check_sdk"])
                    self.assertTrue(checks["test_browser_platform"])
                    self.assertFalse(checks["docs_site"])

    def test_public_source_rename_keeps_old_path_for_push_and_pr(self):
        with tempfile.TemporaryDirectory() as directory:
            subprocess.run(["git", "init", "-q", directory], check=True)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", directory, *args], text=True
                ).strip()

            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Fixture")
            old = "sdks/browser/src/codec.ts"
            new = "sdks/browser/test/codec.ts"
            source = Path(directory) / old
            source.parent.mkdir(parents=True)
            source.write_text("export const codec = 1;\n")
            git("add", ".")
            git("commit", "-qm", "public source")
            base = git("rev-parse", "HEAD")
            (Path(directory) / new).parent.mkdir(parents=True)
            git("mv", old, new)
            git("commit", "-qm", "move public source to test")
            feature = git("rev-parse", "HEAD")
            self.assertEqual(
                git("diff", "--name-status", "--find-renames", base, feature),
                "R100\t" + old + "\t" + new,
            )
            original = Path.cwd()
            try:
                os.chdir(directory)
                for event in ("push", "pull_request"):
                    if event == "pull_request":
                        git("checkout", "-qb", "base-side", base)
                        (Path(directory) / "base-change").write_text("base update")
                        git("add", ".")
                        git("commit", "-qm", "base update")
                        git("merge", "--no-ff", "-qm", "PR merge", feature)
                    with (
                        self.subTest(event=event),
                        patch.dict(os.environ, {"GITHUB_EVENT_NAME": event}),
                    ):
                        paths = selection.changed_paths({"before": base})
                        self.assertEqual(set(paths), {old, new})
                        checks = selection.select(paths, event)["checks"]
                        self.assertTrue(checks["docs_site"])
                        self.assertTrue(checks["test_browser"])
            finally:
                os.chdir(original)

    def test_event_git_diff_and_unavailable_base(self):
        with tempfile.TemporaryDirectory() as directory:
            subprocess.run(["git", "init", "-q", directory], check=True)

            def git(*args):
                return subprocess.check_output(
                    ["git", "-C", directory, *args], text=True
                ).strip()

            git("config", "user.email", "fixture@example.invalid")
            git("config", "user.name", "Fixture")
            (Path(directory) / "base").write_text("base")
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            (Path(directory) / "changed").write_text("head")
            git("add", ".")
            git("commit", "-qm", "head")
            original = Path.cwd()
            try:
                os.chdir(directory)
                with patch.dict(os.environ, {"GITHUB_EVENT_NAME": "push"}):
                    self.assertEqual(
                        selection.changed_paths({"before": base}), ["changed"]
                    )
                    self.assertIsNone(selection.changed_paths({"before": "0" * 40}))
                    self.assertIsNone(selection.changed_paths({"before": "f" * 40}))
                with patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request"}):
                    self.assertIsNone(selection.changed_paths({}))
                    feature = git("rev-parse", "HEAD")
                    git("checkout", "-qb", "base-side", base)
                    (Path(directory) / "base-change").write_text("base update")
                    git("add", ".")
                    git("commit", "-qm", "base update")
                    git("merge", "--no-ff", "-qm", "PR merge", feature)
                    self.assertEqual(selection.changed_paths({}), ["changed"])
            finally:
                os.chdir(original)

    def test_static_routers_and_matrix_results_reject_skipped_missing_and_failure(self):
        for filename, names in (
            ("test.yml", selection.TEST_SUITES),
            ("lint.yml", selection.SOURCE_SUITES),
        ):
            top = workflow(filename)
            caller = top["on"]["workflow_call"]["inputs"]
            self.assertEqual(json.loads(caller["suites"]["default"]), list(names))
            matrix = top["jobs"]["suites"]
            self.assertIs(matrix["strategy"]["fail-fast"], True)
            self.assertEqual(
                matrix["strategy"]["matrix"]["suite"], "${{ fromJSON(inputs.suites) }}"
            )
            router = workflow(matrix["uses"].split("/")[-1])
            result = router["jobs"]["result"]["steps"][0]["env"]["PASSED"]
            for name in names:
                self.assertIn(name, router["jobs"])
                self.assertTrue(
                    passed(result, results={name: "success"}, inputs={"suite": name})
                )
                for status in ("failure", "cancelled", "skipped", None):
                    self.assertFalse(
                        passed(result, results={name: status}, inputs={"suite": name})
                    )
            self.assertFalse(passed(result, inputs={"suite": "unknown"}))
            result = top["jobs"]["results"]["steps"][0]["env"]["PASSED"]
            for status in ("failure", "cancelled", "skipped", None):
                self.assertFalse(passed(result, results={"suites": status}))

    def test_required_and_mobile_gates_use_selected_success(self):
        top = workflow("ci.yml")
        checks = selection.select(None)["checks"]
        for name in ("lint", "test"):
            gate = top["jobs"][name]
            expression = gate["steps"][0]["env"]["PASSED"]
            results = dict.fromkeys(gate["needs"], "success")
            self.assertTrue(passed(expression, checks, results))
            for job in gate["needs"]:
                for status in ("failure", "cancelled", "skipped", None):
                    bad = {**results, job: status}
                    # The full-site route owns Rust references; its standalone job is skipped.
                    if job == "docs-rust-reference":
                        continue
                    self.assertFalse(
                        passed(expression, checks, bad), (name, job, status)
                    )
        expression = workflow("test-android.yml")["jobs"]["results"]["steps"][0]["env"][
            "PASSED"
        ]
        inputs = {"run-unit": True, "run-consumers": True, "run-platform": True}
        defaults = workflow("test-android.yml")["on"]["workflow_call"]["inputs"]
        for name in inputs:
            self.assertIs(defaults[name]["default"], True)
        results = dict.fromkeys(
            ("unit-tests", "min-sdk-smoke", "integration-tests"), "success"
        )
        self.assertTrue(passed(expression, results=results, inputs=inputs))
        for job in results:
            for status in ("failure", "cancelled", "skipped", None):
                self.assertFalse(
                    passed(expression, results={**results, job: status}, inputs=inputs)
                )

    def test_command_jobs_select_small_shells(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            value = workflow(path.name)
            for name, job in value.get("jobs", {}).items():
                for step in job.get("steps", []):
                    command = step.get("run", "")
                    if not re.search(r"(?:dev/nix-shell|\bjust\s)", command):
                        continue
                    if job.get("runs-on") == "windows-latest":
                        continue
                    shell = {
                        **value.get("env", {}),
                        **job.get("env", {}),
                        **step.get("env", {}),
                    }.get("NIX_DEVSHELL")
                    explicit = re.search(
                        r"(?:--shell\s+|NIX_DEVSHELL=)(rust|js-node|js|ios|android|wasm|docs)",
                        command,
                    )
                    self.assertTrue(shell or explicit, (path.name, name, command))
                    self.assertNotEqual(shell, "default", (path.name, name))

    def test_recovery_and_windows_stay_manual(self):
        self.assertEqual(
            set(workflow("manual-sdk-recovery.yml")["on"]), {"workflow_dispatch"}
        )
        self.assertNotIn("recovery", workflow("test-node-sdk.yml")["jobs"])
        self.assertEqual(
            workflow("test-sdk.yml")["jobs"]["windows-load"]["if"],
            "github.event_name == 'workflow_dispatch'",
        )


if __name__ == "__main__":
    unittest.main()
