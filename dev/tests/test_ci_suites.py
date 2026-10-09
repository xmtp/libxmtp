#!/usr/bin/env python3
"""Check the CI suite declaration compiler."""

import copy
import importlib.machinery
import importlib.util
from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader("ci_suites", str(ROOT / "dev/ci-suites"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
SUITES = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(SUITES)


def declaration():
    return yaml.safe_load((ROOT / ".github/ci-suites.yml").read_text())


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

    def test_dev_dependencies_count_only_for_roots(self):
        # xmtp_mls has a dev-dependency on the backend; xdbg only builds xmtp_mls.
        self.assertIn("apps/backend", self.graph.closure(["xmtp_mls"]))
        self.assertNotIn("apps/backend", self.graph.closure(["xdbg"]))

    def test_default_members_and_unknown_packages(self):
        self.assertIn("apps/backend", self.graph.closure(["@default-members"]))
        with self.assertRaises(SUITES.DeclarationError):
            self.graph.closure(["no_such_package"])


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

    def test_expression_scopes_use_their_result_literals(self):
        value = "${{ inputs.target == 'browser' && 'conformance' || 'debug' }}"
        self.assertEqual(SUITES.kache_scopes(value), {"conformance", "debug"})
        self.assertEqual(SUITES.kache_scopes("check-${{ matrix.target }}"), {"check-*"})

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
            lambda d: d.update(extra=[]),
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

    def test_every_suite_key_is_required(self):
        for key in sorted(SUITES.REQUIRED_SUITE_KEYS):
            with self.subTest(key=key):
                with self.assertRaisesRegex(SUITES.DeclarationError, key):
                    self.compile(lambda d: d["suites"]["test_xdbg"].pop(key))

    def test_declared_values_and_workflow_files(self):
        suite = SUITES.compile_declaration(ROOT)["suites"]["test_backend"]
        self.assertEqual(suite["run_on"], ["ready_pr_push", "merge"])
        self.assertFalse(suite["disable_on_forks"])
        self.assertEqual(suite["permissions"], {"contents": "read"})
        self.assertIsNone(suite["covered_by"])
        rust = SUITES.compile_declaration(ROOT)["suites"]["docs_rust"]
        self.assertEqual(rust["covered_by"], "docs_site")
        self.assertIn(".github/workflows/test-backend.yml", suite["workflow_files"])
        self.assertIn(".github/actions/setup-nix/**", suite["workflow_files"])


class RenderTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT)

    def jobs(self, phase):
        return yaml.safe_load(SUITES.render_workflow(self.manifest, phase))["jobs"]

    def test_each_phase_gets_only_its_suites(self):
        lint, test = self.jobs("lint"), self.jobs("test")
        self.assertIn("lint_workspace", lint)
        self.assertNotIn("test_ios", lint)
        self.assertIn("test_ios", test)
        self.assertEqual(set(test["required"]["needs"]), set(test) - {"required"})

    def test_secrets_and_permissions_follow_the_declaration(self):
        jobs = self.jobs("test")
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

    def test_suite_workflows_must_not_take_inputs(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["workflow"] = "check-release-push.yml"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test_xdbg: check-release-push.yml must not take inputs", errors)


if __name__ == "__main__":
    unittest.main()
