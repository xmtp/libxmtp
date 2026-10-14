#!/usr/bin/env python3
"""Check the CI suite declaration compiler."""

import copy
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
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

    def test_product_groups_compile_like_suite_filters(self):
        manifest = self.compile(with_groups)
        self.assertEqual(list(manifest["products"]), ["alpha", "beta"])
        self.assertIn("sdks/android/**", manifest["products"]["beta"]["paths"])
        self.assertIn("!docs/**", manifest["products"]["beta"]["paths"])
        # Declaration order of the groups, not the suite's order.
        self.assertEqual(
            manifest["suites"]["test_android"]["products"], ["alpha", "beta"]
        )
        self.assertEqual(manifest["suites"]["test_xdbg"]["products"], [])

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

    def test_suites_without_secrets_get_no_secrets_key(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["secrets"] = "none"
        jobs = yaml.safe_load(SUITES.render_workflow(manifest, "test"))["jobs"]
        self.assertNotIn("secrets", jobs["test_xdbg"])
        self.assertEqual(jobs["test_ios"]["secrets"], "inherit")

    def test_secrets_and_permissions_follow_the_declaration(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["docs_site"]["permissions"]["pages"] = "write"
        jobs = yaml.safe_load(SUITES.render_workflow(manifest, "test"))["jobs"]
        self.assertEqual(jobs["test_ios"]["secrets"], "inherit")
        self.assertEqual(set(jobs["check_rust"]["secrets"]), set(SUITES.KACHE_SECRETS))
        self.assertEqual(jobs["docs_site"]["permissions"]["pages"], "write")
        self.assertEqual(jobs["test_ios"]["permissions"], {"contents": "read"})


class ProductRenderTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT, with_groups(declaration()))
    workflow = yaml.safe_load(SUITES.render_workflow(manifest, "test"))
    jobs = workflow["jobs"]

    def test_each_group_gets_one_products_job(self):
        products = {n: j for n, j in self.jobs.items() if n.startswith("products_")}
        self.assertEqual(set(products), {"products_alpha", "products_beta"})
        steps = json.dumps(products["products_beta"]["steps"])
        self.assertIn("nix build --keep-going", steps)
        self.assertIn(".#ciSets.x86_64-linux.beta", steps)
        self.assertNotIn(".#ciSets.x86_64-linux.alpha", steps)
        # YAML 1.1 reads the `on` key as True.
        inputs = self.workflow[True]["workflow_call"]["inputs"]
        self.assertTrue(inputs["products"]["required"])
        lint = yaml.safe_load(SUITES.render_workflow(self.manifest, "lint"))
        self.assertNotIn("products", lint[True]["workflow_call"]["inputs"])

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

    def gate(self, selected, products, results):
        gate = self.jobs["required"]
        self.assertTrue(
            {"products_alpha", "products_beta", "test_android"} <= set(gate["needs"])
        )
        step = gate["steps"][0]
        env = {k: v for k, v in step["env"].items() if "${{" not in v}
        env |= {
            "SELECTED": json.dumps(selected),
            "PRODUCTS": json.dumps(products),
            "RESULTS": json.dumps({k: {"result": v} for k, v in results.items()}),
        }
        run = subprocess.run(
            ["bash", "-c", step["run"]],
            env=os.environ | env,
            capture_output=True,
            text=True,
        )
        return run.returncode, run.stdout

    def test_gate_names_a_failed_products_job_once(self):
        passed = {
            "products_alpha": "success",
            "products_beta": "skipped",
            "test_android": "success",
            "test_ios": "success",
            "test_xdbg": "success",
        }
        selected = ["test_android", "test_ios", "test_xdbg"]
        self.assertEqual(self.gate(selected, ["alpha"], passed)[0], 0)
        broken = passed | {
            "products_alpha": "failure",
            "test_android": "skipped",
            "test_ios": "skipped",
        }
        code, output = self.gate(selected, ["alpha"], broken)
        self.assertEqual(code, 1)
        self.assertEqual(output.count("products_alpha: failure"), 1)
        self.assertNotIn("test_android", output)
        self.assertNotIn("test_ios", output)
        for change in [
            {"test_xdbg": "failure"},
            # A selected group whose job did not run.
            {"products_alpha": "skipped"},
            {"test_ios": "cancelled"},
        ]:
            with self.subTest(change=change):
                code, output = self.gate(selected, ["alpha"], passed | change)
                self.assertEqual(code, 1)
                self.assertIn(next(iter(change)), output)


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

    def test_ci_passes_the_products_input_only_when_groups_exist(self):
        groups = SUITES.compile_declaration(ROOT, with_groups(declaration()))
        none = copy.deepcopy(groups)
        none["products"] = {}
        # ci.yml either passes the input or not, so exactly one manifest fits it.
        errors = ["\n".join(SUITES.validate(m)) for m in (groups, none)]
        wrong = [e for e in errors if "ci.yml must pass" in e]
        self.assertEqual(len(wrong), 1)
        self.assertIn("${{ needs.detect-changes.outputs.products }}", wrong[0])

    def test_product_group_patterns_must_match_files(self):
        manifest = SUITES.compile_declaration(ROOT, with_groups(declaration()))
        manifest["products"]["alpha"]["paths"].append("no-such-dir/**")
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("products.alpha: no-such-dir/**", errors)

    def test_suite_workflows_must_not_require_inputs(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["workflow"] = "check-release-push.yml"
        errors = "\n".join(SUITES.validate(manifest))
        # `ref` is required; the optional `dry-run` keeps its default.
        self.assertIn(
            "test_xdbg: check-release-push.yml must not require inputs ['ref']", errors
        )

    def test_suite_workflows_may_take_optional_inputs(self):
        # post-merge.yml sets deploy-docs.yml's optional `deploy` input; the merge
        # run calls it without inputs.
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["workflow"] = "deploy-docs.yml"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertNotIn("inputs", errors)

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


if __name__ == "__main__":
    unittest.main()
