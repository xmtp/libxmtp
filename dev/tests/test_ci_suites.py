#!/usr/bin/env python3
"""Check the CI suite declaration compiler."""

import copy
import importlib.machinery
import importlib.util
from pathlib import Path
import re
import shlex
import subprocess
import unittest
from unittest import mock

import yaml

ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader("ci_suites", str(ROOT / "dev/ci-suites"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
SUITES = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(SUITES)


def declaration():
    return yaml.safe_load((ROOT / ".github/ci-suites.yml").read_text())


def with_groups(data):
    """Two product groups: test_android waits for both, test_ios for one."""
    data["products"] = {
        "alpha": {"path_filters": ["crates/xmtp_sdk/**"]},
        "beta": {"path_filters": [{"path_group": "android"}]},
    }
    for name in data["suites"]:
        data["suites"][name]["products"] = []
    data["suites"]["test_android"]["products"] = ["beta", "alpha"]
    data["suites"]["test_ios"]["products"] = ["alpha"]
    return data


def job_runs(job, inputs, results):
    """Evaluate a generated job's `if` the way GitHub does."""
    condition = job["if"].strip().removeprefix("${{").removesuffix("}}")
    # Without a status function, GitHub runs the job only when every job it
    # needs succeeded.
    if not re.search(r"\b(always|cancelled|failure|success)\(\)", condition):
        if any(results[need] != "success" for need in job.get("needs", [])):
            return False
    # The run itself is not cancelled.
    source = condition.replace("!cancelled()", "True").replace("&&", " and ")
    source = re.sub(
        r"contains\(fromJSON\(inputs\.(\w+)\), '(\w+)'\)",
        lambda m: repr(m[2] in inputs[m[1]]),
        source,
    )
    source = re.sub(r"needs\.(\w+)\.result", lambda m: repr(results[m[1]]), source)
    return eval(source, {"__builtins__": {}}, {})


class CargoGraphTests(unittest.TestCase):
    graph = SUITES.CargoGraph(ROOT)

    def test_backend_closure_excludes_client_crates_and_follows_patches(self):
        crates = set(self.graph.closure(["xmtp_backend"]))
        self.assertIn("crates/xmtp_proto", crates)
        # xmtp-workspace-hack resolves through [patch.crates-io].
        self.assertIn("crates/xmtp-workspace-hack", crates)
        self.assertTrue(
            crates.isdisjoint({"crates/xmtp_mls", "crates/xmtp_db", "crates/xmtp_sdk"})
        )


class CompileTests(unittest.TestCase):
    def compile(self, change):
        data = declaration()
        change(data)
        return SUITES.compile_declaration(ROOT, data)

    def test_kache_scopes_must_match_the_workflows(self):
        def change(data):
            data["suites"]["check_rust"]["kache"] = ["rust-clippy-native"]

        with self.assertRaisesRegex(SUITES.DeclarationError, "rust-clippy-wasm"):
            self.compile(change)

    def test_invalid_declarations_are_rejected(self):
        for change in [
            lambda d: d["suites"]["test_xdbg"].update(paths=["x"]),
            lambda d: d["suites"]["test_xdbg"].update(pr_filters="x"),
            lambda d: d["suites"]["test_xdbg"].pop("pr_filters"),
            lambda d: d["suites"]["test_xdbg"].update(path_filters=[]),
            *[
                lambda d, value=value: d["suites"]["test_xdbg"].update(
                    pr_filters=[{"pr_label": value}]
                )
                for value in (None, True, 1, [], ["iOS"], "", " ")
            ],
            lambda d: d["suites"]["test_xdbg"].update(
                pr_filters=[{"pr_label": "iOS", "path_group": "swift"}]
            ),
            lambda d: d["path_groups"].update(swift=[{"pr_label": "iOS"}]),
            lambda d: d["products"]["sdk_js"].update(
                path_filters=[{"pr_label": "iOS"}]
            ),
            lambda d: d["suites"]["test_xdbg"].update(
                pr_filters=[{"path_group": "missing"}]
            ),
            lambda d: d["suites"]["test_xdbg"].update(pr_filters=[{"crate": "xdbg"}]),
            lambda d: d["suites"]["test_xdbg"].update(
                pr_filters=[{"cargo_package": "xdbg", "path_group": "swift"}]
            ),
            lambda d: (
                d["path_groups"].update(loop=[{"path_group": "loop"}])
                or d["suites"]["test_xdbg"].update(pr_filters=[{"path_group": "loop"}])
            ),
            lambda d: d["suites"]["test_xdbg"].update(run_on=["nightly"]),
            lambda d: d["suites"]["test_xdbg"].update(run_on=[]),
            lambda d: d["suites"]["test_xdbg"].update(secrets="all"),
            lambda d: d["suites"]["test_xdbg"].update(covered_by="missing"),
            lambda d: d["suites"]["test_xdbg"].update(disable_on_forks="no"),
            lambda d: d["suites"]["test_xdbg"].update(permissions={}),
            lambda d: d["suites"]["test_xdbg"].update(permissions={"pages": "admin"}),
            lambda d: d["suites"]["test_xdbg"].update(covered_by=["docs_site"]),
            lambda d: d["suites"]["test_xdbg"].update(covered_by=None),
            lambda d: d.pop("neutral"),
            lambda d: d.update(on_deleted_path=[]),
            lambda d: d.update(on_deleted_path=["missing"]),
            lambda d: d["suites"]["docs_site"].update(covered_by="docs_rust"),
            lambda d: d["suites"]["test_xdbg"].update(covered_by="test_xdbg"),
            lambda d: d.update(extra=[]),
            lambda d: with_groups(d)["suites"]["test_ios"].update(products=["gamma"]),
            lambda d: with_groups(d)["suites"]["lint_proto"].update(products=["alpha"]),
            lambda d: with_groups(d)["products"].update(
                gamma={"path_filters": ["docs/**"]}
            ),
            lambda d: with_groups(d)["products"].update(
                {"Bad-Name": {"path_filters": ["a"]}}
            ),
            lambda d: with_groups(d)["products"].update(alpha={"path_filters": []}),
            lambda d: with_groups(d)["products"].update(alpha=["crates/xmtp_sdk/**"]),
            lambda d: d["suites"]["test_xdbg"].pop("products"),
        ]:
            with self.assertRaises(SUITES.DeclarationError):
                self.compile(change)

    def test_filters_expand_groups_packages_and_globs(self):
        groups = {
            "inner": ["a/**", {"cargo_package": "xmtp_id"}],
            "outer": [{"path_group": "inner"}, "!a/skip/**"],
        }
        filters = [{"path_group": "outer"}, {"cargo_package": ["x", "y"]}, "b.md"]
        packages, globs = SUITES.expand_filters(filters, groups, "test")
        self.assertEqual(packages, ["xmtp_id", "x", "y"])
        self.assertEqual(globs, ["a/**", "!a/skip/**", "b.md"])
        self.compile(lambda d: d["suites"]["test_xdbg"].update(pr_filters=[]))

    def test_compiled_labels_and_paths_select_independently(self):
        manifest = self.compile(
            lambda d: d["suites"]["test_xdbg"].update(
                pr_filters=["docs/**", "!docs/private/**", {"pr_label": "observe"}]
            )
        )
        selector = SUITES.load_selector()
        for path, labels, expected in [
            ("docs/guide.md", [], True),
            ("docs/private/guide.md", [], False),
            ("docs/private/guide.md", ["observe"], True),
            ("README.md", ["observe"], True),
            ("README.md", [], False),
            ("README.md", ["Observe"], False),
        ]:
            with self.subTest(path=path, labels=labels):
                payload = {
                    "repository": {"full_name": "r"},
                    "pull_request": {
                        "head": {"repo": {"full_name": "r"}},
                        "labels": [{"name": label} for label in labels],
                    },
                }
                with mock.patch.multiple(
                    selector, MANIFEST=manifest, SUITES=manifest["suites"]
                ):
                    result = selector.select_suites(
                        [path], "pull_request", payload, "success", 1
                    )
                self.assertEqual("test_xdbg" in result["suites"]["test"], expected)

    def test_react_native_runs_for_shared_mobile_and_backend_inputs(self):
        manifest = SUITES.compile_declaration(ROOT)
        selector = SUITES.load_selector()
        payload = {
            "repository": {"full_name": "r"},
            "pull_request": {
                "head": {"repo": {"full_name": "r"}},
                "labels": [],
            },
        }
        for path in [
            "nix/lib/ios-env.nix",
            "nix/lib/android-env.nix",
            "nix/lib/android-emulator-start.py",
            "nix/lib/android-sync-clock.sh",
            "nix/android-packages.nix",
            "nix/shells/android.nix",
            "nix/services/stack.nix",
        ]:
            with (
                self.subTest(path=path),
                mock.patch.multiple(
                    selector, MANIFEST=manifest, SUITES=manifest["suites"]
                ),
            ):
                result = selector.select_suites(
                    [path], "pull_request", payload, "success", 1
                )
                self.assertIn("test_react_native", result["suites"]["test"])

    def test_secret_modes_are_validated(self):
        self.compile(lambda d: d["suites"]["lint_proto"].update(secrets="none"))
        for value in ("all", "None", "", None):
            with self.subTest(value=value):
                with self.assertRaisesRegex(
                    SUITES.DeclarationError, "inherit, kache, none, cachix-pr"
                ):
                    self.compile(
                        lambda d: d["suites"]["lint_proto"].update(secrets=value)
                    )


class RenderTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT)

    def jobs(self, phase):
        return yaml.safe_load(SUITES.render_workflow(self.manifest, phase))["jobs"]

    def test_secrets_and_permissions_follow_the_declaration(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["docs_site"]["permissions"]["pages"] = "write"
        jobs = {}
        for phase in SUITES.GENERATED_WORKFLOWS:
            jobs.update(yaml.safe_load(SUITES.render_workflow(manifest, phase))["jobs"])
        for name, suite in declaration()["suites"].items():
            with self.subTest(suite=name):
                job = jobs[name]
                if suite["secrets"] == "none":
                    self.assertNotIn("secrets", job)
                elif suite["secrets"] == "kache":
                    self.assertEqual(set(job["secrets"]), set(SUITES.KACHE_SECRETS))
                elif suite["secrets"] == "cachix-pr":
                    self.assertEqual(
                        job["secrets"],
                        {"CACHIX_PR_AUTH_TOKEN": "${{ secrets.CACHIX_PR_AUTH_TOKEN }}"},
                    )
                else:
                    self.assertEqual(job["secrets"], "inherit")
        self.assertEqual(jobs["docs_site"]["permissions"]["pages"], "write")
        self.assertEqual(jobs["test_ios"]["permissions"], {"contents": "read"})
        for phase, name in (("lint", "lint_example_ios"), ("test", "test_example_ios")):
            with self.subTest(suite=name):
                phase_jobs = yaml.safe_load(SUITES.render_workflow(manifest, phase))[
                    "jobs"
                ]
                self.assertEqual(
                    phase_jobs[name].get("secrets"),
                    {"CACHIX_PR_AUTH_TOKEN": "${{ secrets.CACHIX_PR_AUTH_TOKEN }}"},
                )


class ProductRenderTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT, with_groups(declaration()))
    workflow = yaml.safe_load(SUITES.render_workflow(manifest, "test"))
    jobs = workflow["jobs"]

    def test_suites_need_only_their_groups_and_run_unless_one_failed(self):
        self.assertEqual(self.jobs["test_ios"]["needs"], ["products_alpha"])
        self.assertEqual(
            self.jobs["test_android"]["needs"], ["products_alpha", "products_beta"]
        )
        self.assertNotIn("needs", self.jobs["test_xdbg"])
        inputs = {"suites": ["test_ios", "test_android"], "products": []}
        for alpha, beta, ios, android in [
            ("success", "success", True, True),
            # A group that did not run: the suite builds the products itself.
            ("skipped", "skipped", True, True),
            ("success", "skipped", True, True),
            ("failure", "success", False, False),
            ("skipped", "failure", True, False),
        ]:
            with self.subTest(alpha=alpha, beta=beta):
                results = {"products_alpha": alpha, "products_beta": beta}
                self.assertEqual(job_runs(self.jobs["test_ios"], inputs, results), ios)
                self.assertEqual(
                    job_runs(self.jobs["test_android"], inputs, results), android
                )
        unselected = {"suites": ["test_android"], "products": []}
        results = {"products_alpha": "success", "products_beta": "success"}
        self.assertFalse(job_runs(self.jobs["test_ios"], unselected, results))
        self.assertTrue(
            job_runs(self.jobs["products_alpha"], {"products": ["alpha"]}, {})
        )
        self.assertFalse(
            job_runs(self.jobs["products_beta"], {"products": ["alpha"]}, {})
        )


class ValidateTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT)

    def test_repository_declaration_is_valid(self):
        self.assertEqual(SUITES.validate(self.manifest), [])

    def test_unknown_files_and_stale_patterns_fail(self):
        files = SUITES.tracked_files(ROOT) + ["new-language/source.xyz"]
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["paths"].append(
            "crates/xmtp_user_preferences/**"
        )
        errors = "\n".join(SUITES.validate(manifest, files=files))
        self.assertIn("new-language/source.xyz", errors)
        self.assertIn("test_xdbg: crates/xmtp_user_preferences/**", errors)

    def test_kache_secrets_need_workflow_declarations(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["secrets"] = "kache"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test-xdbg.yml must declare workflow_call secrets", errors)

    def test_pr_cache_secrets_need_workflow_declarations(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["secrets"] = "cachix-pr"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test-xdbg.yml must declare workflow_call secrets", errors)

    def test_suite_permissions_must_fit_the_caller_grant(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["lint_proto"]["permissions"] = {
            "contents": "read",
            "pages": "write",
        }
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("lint_proto: needs pages: write", errors)
        self.assertNotIn("docs_site", errors)

    def test_dead_brace_alternatives_are_stale(self):
        self.assertEqual(
            SUITES.brace_variants("a/{b,c}/{d,}.x"),
            ["a/b/{d,}.x", "a/c/{d,}.x", "a/{b,c}/d.x", "a/{b,c}/.x"],
        )
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["paths"].append("{docs,no-such-dir}/**")
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test_xdbg: no-such-dir/**", errors)
        self.assertNotIn("test_xdbg: docs/**", errors)

    def test_called_jobs_must_fit_the_suite_permissions(self):
        # GitHub checks a called job's permissions even when its `if` skips it.
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["workflow"] = "npm-publish.yml"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn(
            "test_xdbg: npm-publish.yml job publish needs id-token: write", errors
        )
        manifest["suites"]["test_xdbg"]["permissions"] = {
            "contents": "read",
            "id-token": "write",
        }
        errors = "\n".join(SUITES.validate(manifest))
        self.assertNotIn("job publish needs", errors)


class IosLifecycleSplitTests(unittest.TestCase):
    """The iOS unit and lifecycle CI jobs split the Swift tests between them."""

    def test_lifecycle_checks_run_once_across_ci_jobs(self):
        # `test-lifecycle` selects these tests and `test skip-lifecycle` skips
        # them. The two must name the same tests, or a proof runs twice or never.
        def swift_test_args(*recipe):
            result = subprocess.run(
                ["just", "--dry-run", "ios", *recipe],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=True,
            )
            command = next(
                line
                for line in (result.stdout + result.stderr).splitlines()
                if "swift test" in line
            )
            return shlex.split(command.split("swift test", 1)[1])

        selected = swift_test_args("test-lifecycle")
        skipped = swift_test_args("test", "skip-lifecycle")
        self.assertEqual(selected[0], "--filter")
        self.assertEqual(
            selected[1], r"XmtpSdkTests\.(ReaderTeardownTests|ListenerGateTests)"
        )
        self.assertEqual(skipped[-2], "--skip")
        self.assertEqual(selected[1], skipped[-1])
        self.assertNotIn("--skip", swift_test_args("test"))
        self.assertEqual(
            swift_test_args(
                "test", "all", "--filter", "XmtpSdkTests.MessageHistoryPageTests"
            ),
            ["--parallel", "-q", "--filter", "XmtpSdkTests.MessageHistoryPageTests"],
        )
        for test_case in ("ReaderTeardownTests", "ListenerGateTests"):
            self.assertRegex(f"XmtpSdkTests.{test_case}/testA", selected[1])
        rejected = subprocess.run(
            ["just", "--dry-run", "ios", "test", "skip"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(rejected.returncode, 0)


class ExampleIOSWorkflowTests(unittest.TestCase):
    def workflow(self):
        return yaml.safe_load(
            (ROOT / ".github/workflows/test-example-ios.yml").read_text()
        )

    def steps(self):
        jobs = self.workflow()["jobs"]
        self.assertEqual(
            set(jobs), {"tests"}, "one job must share the binding preparation"
        )
        return jobs["tests"]["steps"]

    def test_one_job_shares_checkout_and_toolchain(self):
        steps = self.steps()
        for action in (
            "actions/checkout@",
            "./.github/actions/setup-nix",
            "maxim-lobanov/setup-xcode@",
        ):
            with self.subTest(action=action):
                self.assertEqual(
                    sum(step.get("uses", "").startswith(action) for step in steps), 1
                )

    def test_every_app_gate_remains_required(self):
        for command in (
            "just example-ios build",
            "CONFIGURATION=Push",
            "just example-ios lint",
            "just example-ios test'",
            "just backend ci just example-ios test-ui",
            "just backend ci nix run .#example-ios -- --headless",
            "CODE_SIGN_ENTITLEMENTS",
            "dev/check-apple-toolchain",
        ):
            with self.subTest(command=command):
                matching = [
                    step for step in self.steps() if command in step.get("run", "")
                ]
                self.assertTrue(matching, command)
                self.assertTrue(
                    all(
                        "if" not in step and not step.get("continue-on-error", False)
                        for step in matching
                    )
                )

    def test_failures_keep_results_and_separate_service_logs(self):
        steps = self.steps()
        results = next(
            (
                step
                for step in steps
                if step.get("with", {}).get("name") == "example-ios-journeys-results"
            ),
            None,
        )
        self.assertIsNotNone(results, "failed journeys must retain their xcresult")
        self.assertEqual(results["if"], "failure()")
        self.assertEqual(
            results["with"]["path"], "apps/example-ios/.build/DerivedData/Logs/Test"
        )
        self.assertTrue(results["with"]["include-hidden-files"])
        self.assertEqual(results["with"]["retention-days"], 7)
        phases = [step for step in steps if "just backend ci" in step.get("run", "")]
        self.assertEqual(
            {step["env"]["XMTP_SERVICES_LOGS"] for step in phases},
            {
                "${{ runner.temp }}/backend-ci-logs/journeys",
                "${{ runner.temp }}/backend-ci-logs/launch",
            },
        )
        logs = next(
            (
                step
                for step in steps
                if step.get("with", {}).get("name") == "example-ios-backend-logs"
            ),
            None,
        )
        self.assertIsNotNone(logs, "all service phases must retain logs")
        self.assertEqual(logs["if"], "always()")
        self.assertEqual(logs["with"]["path"], "${{ runner.temp }}/backend-ci-logs")

    def test_only_pr_cache_credentials_reach_app_job(self):
        workflow = self.workflow()
        self.assertEqual(
            set(workflow[True]["workflow_call"]["secrets"]), {"CACHIX_PR_AUTH_TOKEN"}
        )
        setup = next(
            step
            for step in self.steps()
            if step.get("uses") == "./.github/actions/setup-nix"
        )
        self.assertEqual(setup["with"]["cache-mode"], "ci")
        self.assertEqual(
            setup["with"]["cachix-pr-auth-token"], "${{ secrets.CACHIX_PR_AUTH_TOKEN }}"
        )
        for suite in ("test_example_ios", "lint_example_ios"):
            self.assertEqual(declaration()["suites"][suite]["secrets"], "cachix-pr")


if __name__ == "__main__":
    unittest.main()
