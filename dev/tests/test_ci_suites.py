#!/usr/bin/env python3
"""Check the CI suite declaration compiler."""

import copy
import importlib.machinery
import importlib.util
from pathlib import Path
import shlex
import subprocess
import unittest

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
            lambda d: d["suites"]["test_xdbg"].update(path_filters="x"),
            lambda d: d["suites"]["test_xdbg"].update(
                path_filters=[{"path_group": "missing"}]
            ),
            lambda d: d["suites"]["test_xdbg"].update(path_filters=[{"crate": "xdbg"}]),
            lambda d: d["suites"]["test_xdbg"].update(
                path_filters=[{"cargo_package": "xdbg", "path_group": "swift"}]
            ),
            lambda d: (
                d["path_groups"].update(loop=[{"path_group": "loop"}])
                or d["suites"]["test_xdbg"].update(
                    path_filters=[{"path_group": "loop"}]
                )
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

    def test_secret_modes_are_validated(self):
        self.compile(lambda d: d["suites"]["lint_proto"].update(secrets="none"))
        for value in ("all", "None", "", None):
            with self.subTest(value=value):
                with self.assertRaisesRegex(
                    SUITES.DeclarationError, "inherit, kache, none"
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
        jobs = yaml.safe_load(SUITES.render_workflow(manifest, "test"))["jobs"]
        self.assertEqual(jobs["test_ios"]["secrets"], "inherit")
        self.assertEqual(set(jobs["check_rust"]["secrets"]), set(SUITES.KACHE_SECRETS))
        self.assertEqual(jobs["docs_site"]["permissions"]["pages"], "write")
        self.assertEqual(jobs["test_ios"]["permissions"], {"contents": "read"})


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


if __name__ == "__main__":
    unittest.main()
