#!/usr/bin/env python3
"""Check retained selection boundaries and the actual workflow result gates."""

import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
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


def passed(
    expression,
    checks=None,
    results=None,
    inputs=None,
    event="pull_request",
    plan=None,
    fork=False,
):
    text = expression.removeprefix("${{").removesuffix("}}").strip()
    text = re.sub(
        r"fromJSON\(needs.detect-changes.outputs.selection\).checks.([a-z_]+)",
        lambda m: "checks.get(" + repr(m[1]) + ")",
        text,
    )
    text = re.sub(
        r"contains\(fromJSON\(needs.detect-changes.outputs.(lint_jobs|test_jobs)\), '([^']+)'\)",
        lambda m: repr(m[2]) + " in plan.get(" + repr(m[1]) + ", [])",
        text,
    )
    text = text.replace("needs[inputs.suite].result", "selected_result")
    text = re.sub(
        r"needs.([a-z_-]+).result", lambda m: "results.get(" + repr(m[1]) + ")", text
    )
    text = re.sub(
        r"inputs.([a-z_-]+)", lambda m: "inputs.get(" + repr(m[1]) + ")", text
    )
    text = text.replace("github.event_name", "event").replace("github.ref", "ref")
    text = text.replace("github.event.pull_request.head.repo.full_name", "head_repo")
    text = text.replace("github.repository", "repository")
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
                "plan": plan or {},
                "head_repo": "fork/libxmtp" if fork else "xmtp/libxmtp",
                "repository": "xmtp/libxmtp",
                "selected_result": (results or {}).get((inputs or {}).get("suite")),
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
            "test_swift_lifecycle",
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
            "test_swift_lifecycle",
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

    def test_missing_pr_repository_metadata_uses_fork_policy(self):
        for head in ({"repo": None}, {}, {"repo": "unavailable"}, {"repo": {}}):
            with self.subTest(head=head), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                event = root / "event.json"
                event.write_text(json.dumps({"pull_request": {"head": head}}))
                output = root / "selection.json"
                result = subprocess.run(
                    [
                        sys.executable,
                        "-B",
                        str(ROOT / "dev/ci/select-checks.py"),
                        "--output",
                        str(output),
                        "--source-suites-output",
                        str(root / "source.json"),
                        "--test-suites-output",
                        str(root / "tests.json"),
                    ],
                    cwd=root,
                    env={
                        **os.environ,
                        "GITHUB_EVENT_NAME": "pull_request",
                        "GITHUB_EVENT_PATH": str(event),
                        "GITHUB_REPOSITORY": "xmtp/libxmtp",
                    },
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    json.loads(output.read_text()), selection.select(None, fork=True)
                )

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
            self.assertEqual(set(router["jobs"]) - {"result"}, set(names))
            self.assertEqual(set(router["jobs"]["result"]["needs"]), set(names))
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
        plan = selection.select(None)
        for name in ("lint", "test"):
            gate = top["jobs"][name]
            step = gate["steps"][0]
            key = name + "_jobs"
            required = plan[key]
            self.assertEqual(
                step["env"]["REQUIRED_JOBS"],
                "${{ needs.detect-changes.outputs." + key + " }}",
            )
            self.assertEqual(step["env"]["JOB_RESULTS"], "${{ toJSON(needs) }}")

            def run_gate(results, required_json=None):
                return (
                    subprocess.run(
                        ["bash", "-euc", step["run"]],
                        env={
                            **os.environ,
                            "REQUIRED_JOBS": json.dumps(required)
                            if required_json is None
                            else required_json,
                            "JOB_RESULTS": json.dumps(
                                {
                                    job: {"result": status}
                                    for job, status in results.items()
                                }
                            ),
                        },
                        capture_output=True,
                        text=True,
                    ).returncode
                    == 0
                )

            results = {
                job: "success" if job in required else "skipped"
                for job in gate["needs"]
            }
            self.assertTrue(run_gate(results))
            for job in required:
                for status in ("failure", "cancelled", "skipped", None):
                    self.assertFalse(
                        run_gate({**results, job: status}), (name, job, status)
                    )
                missing = dict(results)
                del missing[job]
                self.assertFalse(run_gate(missing), (name, job, "missing"))
            self.assertFalse(run_gate(results, ""))
            self.assertFalse(run_gate(results, '["unknown"]'))

            # Rust-only changes use the standalone reference; the full site owns it otherwise.
            rust = selection.select(["crates/xmtp_mls/src/lib.rs"])
            required = rust[key]
            results = {
                job: "success" if job in required else "skipped"
                for job in gate["needs"]
            }
            self.assertTrue(run_gate(results))
            if name == "test":
                self.assertIn("docs-rust-reference", required)
                self.assertNotIn("docs", required)
                self.assertIn("docs", plan[key])
                self.assertNotIn("docs-rust-reference", plan[key])

    def test_selected_lists_agree_with_caller_scheduling(self):
        top = workflow("ci.yml")
        for paths, event, fork in (
            (None, "workflow_dispatch", False),
            (None, "pull_request", True),
            ([], "pull_request", False),
            (["crates/xmtp_mls/src/lib.rs"], "pull_request", False),
            (["crates/xmtp_mls/src/lib.rs"], "push", False),
            (["sdks/ios/Sources/XmtpSdk/Client.swift"], "pull_request", False),
            (
                ["sdks/android/library/src/androidTest/AndroidPackageTest.kt"],
                "pull_request",
                False,
            ),
            (["docs/guide.md"], "pull_request", False),
        ):
            plan = selection.select(paths, event, fork)
            for phase in ("lint", "test"):
                key = phase + "_jobs"
                required = plan[key]
                self.assertEqual(required.count("detect-changes"), 1)
                self.assertTrue(set(required).issubset(top["jobs"][phase]["needs"]))
                self.assertEqual(
                    top["jobs"]["detect-changes"]["outputs"][key],
                    "${{ toJSON(fromJSON(steps.select.outputs.selection)."
                    + key
                    + ") }}",
                )
                for job in top["jobs"][phase]["needs"]:
                    if job == "detect-changes":
                        continue
                    expression = top["jobs"][job]["if"]
                    self.assertIn("outputs." + key, expression)
                    self.assertEqual(
                        passed(expression, plan=plan, event=event, fork=fork),
                        job in required,
                        (paths, event, fork, job),
                    )

    def test_mobile_gates_use_selected_success(self):
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

    def test_backend_publisher_keeps_release_paths_without_pr_duplicates(self):
        publisher = workflow("push-backend.yml")
        self.assertEqual(set(publisher["on"]), {"push", "workflow_call"})
        self.assertEqual(
            publisher["on"]["push"],
            {"branches": ["main", "self-hosted"], "tags": ["**"]},
        )
        matrix = publisher["jobs"]["publish"]["strategy"]["matrix"]["include"]
        self.assertEqual({row["arch"] for row in matrix}, {"amd64", "arm64"})
        release = workflow("release-backend.yml")["jobs"]["build"]
        self.assertEqual(release["uses"], "./.github/workflows/push-backend.yml")
        self.assertIs(release["with"]["release-build"], True)
        manifest = publisher["jobs"]["manifest"]
        self.assertIn("publish", manifest["needs"])
        command = next(
            step["run"]
            for step in manifest["steps"]
            if step.get("name") == "Publish commit manifest"
        )
        self.assertIn('"$AMD64_IMAGE" "$ARM64_IMAGE"', command.splitlines()[0])
        expression = workflow("test-backend.yml")["jobs"]["backend-image"]["strategy"][
            "matrix"
        ]["arch"]
        expression = expression.removeprefix("${{ fromJSON(").removesuffix(") }}")
        expression = expression.replace("github.event_name", "event")
        expression = expression.replace("&&", " and ").replace("||", " or ")
        for event, expected in (
            ("pull_request", ["amd64"]),
            ("push", ["amd64"]),
            ("workflow_dispatch", ["amd64", "arm64"]),
        ):
            arches = eval(expression, {"__builtins__": {}}, {"event": event})
            self.assertEqual(json.loads(arches), expected)


if __name__ == "__main__":
    unittest.main()
