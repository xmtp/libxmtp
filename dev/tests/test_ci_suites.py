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
        "alpha": {"title": "Alpha", "path_filters": ["crates/xmtp_sdk/**"]},
        "beta": {"title": "Beta", "path_filters": [{"path_group": "android"}]},
    }
    for name in data["suites"]:
        data["suites"][name]["products"] = []
    data["suites"]["test_android"]["products"] = ["beta", "alpha"]
    data["suites"]["test_ios"]["products"] = ["alpha"]
    return data


def job_runs(job, outputs, results):
    """Evaluate a generated ci.yml job's `if` the way GitHub does.

    `outputs` holds the Select suites outputs as lists; `results` the results
    of the other jobs. Select suites succeeded unless `results` says otherwise.
    """
    results = {"detect-changes": "success"} | results
    condition = job["if"].strip().removeprefix("${{").removesuffix("}}")
    # Without a status function, GitHub runs the job only when every job it
    # needs succeeded.
    if not re.search(r"\b(always|cancelled|failure|success)\(\)", condition):
        if any(results[need] != "success" for need in job.get("needs", [])):
            return False
    # The run itself is not cancelled.
    source = condition.replace("!cancelled()", "True").replace("&&", " and ")
    source = re.sub(
        r"contains\(fromJSON\(needs\.detect-changes\.outputs\.(\w+)\), '(\w+)'\)",
        lambda m: repr(m[2] in outputs.get(m[1], [])),
        source,
    )
    source = re.sub(r"needs\.([\w-]+)\.result", lambda m: repr(results[m[1]]), source)
    return eval(source, {"__builtins__": {}}, {})


def ci_jobs(manifest):
    return yaml.safe_load(SUITES.render_ci(manifest))["jobs"]


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
            lambda d: with_groups(d)["products"]["alpha"].pop("title"),
            lambda d: with_groups(d)["products"]["alpha"].update(title="Test"),
            # The tier, input, and runner keys.
            lambda d: d["suites"]["test_xdbg"].update(advisory_filters=[]),
            lambda d: d["suites"]["test_xdbg"].update(advisory_filters=["!docs/**"]),
            lambda d: d["suites"]["test_xdbg"].update(
                advisory_filters=[{"path_group": "missing"}]
            ),
            lambda d: d["suites"]["test_xdbg"].update(with_={}),
            lambda d: d["suites"]["test_xdbg"].update({"with": ["scope"]}),
            lambda d: d["suites"]["test_xdbg"].update({"with": {"scope": None}}),
            lambda d: d["suites"]["test_xdbg"].update({"with": {"scope": ["a"]}}),
            lambda d: d["suites"]["test_xdbg"].update({"with": {"bad name": "a"}}),
            lambda d: d["suites"]["test_xdbg"].update(runners=["ubuntu-24.04"]),
            lambda d: d["suites"]["test_xdbg"].update(
                runners={"runner": "ubuntu-24.04"}
            ),
            lambda d: d["suites"]["test_xdbg"].update(
                runners={"runner": {"pr": "ubuntu-24.04"}}
            ),
            lambda d: d["suites"]["test_xdbg"].update(
                runners={"runner": {"pr": "ubuntu-latest", "merge": "ubuntu-24.04"}}
            ),
            lambda d: d["suites"]["test_xdbg"].update(
                runners={"runner": {"pr": "ubuntu-24.04", "merge": "ubuntu-24.04"}},
                **{"with": {"runner": "x"}},
            ),
            lambda d: d.pop("runners"),
            lambda d: d["runners"].pop("gates"),
            lambda d: d["runners"].update(
                select={"pr": "my-runner", "merge": "ubuntu-24.04"}
            ),
            # Test suites do not run on drafts; the draft Test gate says so.
            lambda d: d["suites"]["test_xdbg"].update(
                run_on=["draft_pr_push", "ready_pr_push"]
            ),
            # Suite keys are job ids next to the generated ones.
            lambda d: d["suites"].update(test=d["suites"].pop("test_xdbg")),
            lambda d: d["suites"].update(products_x=d["suites"].pop("test_xdbg")),
            lambda d: d["suites"].update(
                test_ios_advisory=d["suites"].pop("test_xdbg")
            ),
            lambda d: d["suites"].update({"Test-X": d["suites"].pop("test_xdbg")}),
        ]:
            with self.assertRaises(SUITES.DeclarationError):
                self.compile(change)

    def test_titles_are_required_short_unique_and_not_reserved(self):
        def title(value):
            return lambda d: d["suites"]["test_xdbg"].update(title=value)

        self.compile(title("A" * 30))
        # Titles repeat across phases (the lint and test jobs of one SDK).
        self.compile(title("Protobuf"))
        for change in [
            lambda d: d["suites"]["test_xdbg"].pop("title"),
            title(None),
            title(""),
            title("A" * 31),
            title(" Backend"),
            title("Backend "),
            title("Backend / tests"),
            title("${{ github.actor }}"),
            title("Lint"),
            title("test"),
            title("Test (not run on drafts)"),
            title("Lint (draft)"),
            title("Select suites"),
            title("Backend (advisory)"),
            # Unique in the phase, ignoring case.
            title("rust workspace"),
            title("Prebuild JS SDKs (Nix)"),
        ]:
            with (
                self.subTest(change=change),
                self.assertRaises(SUITES.DeclarationError),
            ):
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

    def test_workflow_files_select_in_the_strongest_tier(self):
        # An always-advisory suite stays advisory when its own workflow or a
        # local action it uses changes; a suite that can block, blocks.
        def tiers(path, **suite):
            manifest = self.compile(lambda d: d["suites"]["test_xdbg"].update(suite))
            selector = SUITES.load_selector()
            payload = {
                "repository": {"full_name": "r"},
                "pull_request": {"head": {"repo": {"full_name": "r"}}, "labels": []},
            }
            with mock.patch.multiple(
                selector, MANIFEST=manifest, SUITES=manifest["suites"]
            ):
                result = selector.select_suites(
                    [path], "pull_request", payload, "success", 1
                )
            return (
                "test_xdbg" in result["suites"]["test"],
                "test_xdbg" in result["advisory"],
            )

        workflow = ".github/workflows/test-xdbg.yml"
        self.assertEqual(
            tiers(workflow, pr_filters=[], advisory_filters=["docs/**"]), (False, True)
        )
        self.assertEqual(
            tiers(workflow, pr_filters=["apps/**"], advisory_filters=["docs/**"]),
            (True, False),
        )
        self.assertEqual(tiers(workflow, pr_filters=[]), (True, False))
        self.assertEqual(
            tiers("docs/guide.md", pr_filters=[], advisory_filters=["docs/**"]),
            (False, True),
        )

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

    def test_secrets_and_permissions_follow_the_declaration(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["docs_site"]["permissions"]["pages"] = "write"
        jobs = ci_jobs(manifest)
        for name, suite in declaration()["suites"].items():
            callers = [name] + (
                [f"{name}_advisory"] if "advisory_filters" in suite else []
            )
            for caller in callers:
                with self.subTest(caller=caller):
                    job = jobs[caller]
                    if suite["secrets"] == "none":
                        self.assertNotIn("secrets", job)
                    elif suite["secrets"] == "kache":
                        self.assertEqual(set(job["secrets"]), set(SUITES.KACHE_SECRETS))
                    elif suite["secrets"] == "cachix-pr":
                        self.assertEqual(
                            job["secrets"],
                            {
                                "CACHIX_PR_AUTH_TOKEN": "${{ secrets.CACHIX_PR_AUTH_TOKEN }}"
                            },
                        )
                    else:
                        self.assertEqual(job["secrets"], "inherit")
        self.assertEqual(jobs["docs_site"]["permissions"]["pages"], "write")
        self.assertEqual(jobs["test_ios"]["permissions"], {"contents": "read"})

    def test_callers_are_named_by_title_and_gates_keep_the_required_names(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_xdbg"]["title"] = "Observability"
        jobs = ci_jobs(manifest)
        self.assertEqual(jobs["test_xdbg"]["name"], "Observability")
        self.assertEqual(jobs["detect-changes"]["name"], "Select suites")
        self.assertEqual(jobs["products_sdk_js"]["name"], "Prebuild JS SDKs (Nix)")
        # Branch protection requires Lint and Test; drafts get other names so
        # that a draft run cannot satisfy them.
        for gate, draft in (
            ("lint", "Lint (draft)"),
            ("test", "Test (not run on drafts)"),
        ):
            name = jobs[gate]["name"]
            self.assertEqual(
                name,
                "${{ github.event_name == 'pull_request' && "
                "toJSON(github.event.pull_request.draft) == 'true' && "
                f"'{draft}' || '{gate.capitalize()}' }}}}",
            )
            self.assertEqual(jobs[gate]["if"], "always()")
        # Only the gates are named Lint or Test, and they never skip.
        for job_id, job in jobs.items():
            if job_id not in ("detect-changes", "lint", "test"):
                self.assertNotIn(job.get("name"), SUITES.RESERVED_NAMES)

    def test_ci_yml_has_only_select_products_callers_and_two_gates(self):
        jobs = ci_jobs(self.manifest)
        callers = {job for job, spec in jobs.items() if "uses" in spec}
        expected = set(self.manifest["suites"]) | {
            f"{n}_advisory"
            for n, s in self.manifest["suites"].items()
            if SUITES.has_advisory_tier(s)
        }
        self.assertEqual(callers, expected)
        self.assertEqual(
            set(jobs) - callers,
            {"detect-changes", "lint", "test"}
            | {f"products_{g}" for g in self.manifest["products"]},
        )
        for caller in callers:
            name = caller.removesuffix("_advisory")
            self.assertEqual(
                jobs[caller]["uses"],
                f"./.github/workflows/{self.manifest['suites'][name]['workflow']}",
            )


class TierRenderTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT)
    jobs = ci_jobs(manifest)

    def test_an_advisory_suite_has_a_second_caller_that_no_gate_needs(self):
        blocking, advisory = (
            self.jobs["test_example_ios"],
            self.jobs["test_example_ios_advisory"],
        )
        self.assertEqual(advisory["name"], "iOS example app (advisory)")
        self.assertEqual(advisory["uses"], blocking["uses"])
        for outputs, runs_blocking, runs_advisory in [
            ({"test_suites": ["test_example_ios"]}, True, False),
            ({"advisory_suites": ["test_example_ios"]}, False, True),
            ({"test_suites": ["test_ios"], "advisory_suites": []}, False, False),
        ]:
            with self.subTest(outputs=outputs):
                self.assertEqual(job_runs(blocking, outputs, {}), runs_blocking)
                self.assertEqual(job_runs(advisory, outputs, {}), runs_advisory)
        for gate in ("lint", "test"):
            self.assertFalse(
                [
                    need
                    for need in self.jobs[gate]["needs"]
                    if need.endswith("_advisory")
                ]
            )
        self.assertIn("test_example_ios", self.jobs["test"]["needs"])
        # A suite without advisory_filters has one caller.
        self.assertNotIn("test_ios_advisory", self.jobs)

    def test_detection_failure_skips_every_caller(self):
        failed = {"detect-changes": "failure", "products_sdk_android": "skipped"}
        everything = {"test_suites": list(self.manifest["suites"])}
        for job in ("test_ios", "test_android", "test_example_android_advisory"):
            with self.subTest(job=job):
                self.assertFalse(job_runs(self.jobs[job], everything, failed))


class InputRenderTests(unittest.TestCase):
    def manifest(self, **suite):
        data = declaration()
        data["suites"]["test_xdbg"].update(suite)
        return SUITES.compile_declaration(ROOT, data)

    def test_with_inputs_and_runners_reach_the_caller(self):
        manifest = self.manifest(
            **{
                "with": {"scope": "android-smoke", "debug": False, "shards": 2},
                "runners": {
                    "runner": {
                        "pr": "blacksmith-8vcpu-ubuntu-2404",
                        "merge": "ubuntu-24.04",
                    },
                    "big-runner": {
                        "pr": "blacksmith-16vcpu-ubuntu-2404",
                        "merge": "blacksmith-16vcpu-ubuntu-2404",
                    },
                },
            }
        )
        given = ci_jobs(manifest)["test_xdbg"]["with"]
        self.assertEqual(
            given,
            {
                "scope": "android-smoke",
                "debug": False,
                "shards": 2,
                "runner": "${{ github.event_name == 'push' && 'ubuntu-24.04' "
                "|| 'blacksmith-8vcpu-ubuntu-2404' }}",
                "big-runner": "blacksmith-16vcpu-ubuntu-2404",
            },
        )
        # The expression picks the merge label only on a push to main.
        for event, label in [
            ("push", "ubuntu-24.04"),
            ("pull_request", "blacksmith-8vcpu-ubuntu-2404"),
            ("workflow_dispatch", "blacksmith-8vcpu-ubuntu-2404"),
        ]:
            with self.subTest(event=event):
                source = (
                    given["runner"]
                    .removeprefix("${{")
                    .removesuffix("}}")
                    .replace("github.event_name", repr(event))
                    .replace("&&", " and ")
                    .replace("||", " or ")
                )
                self.assertEqual(eval(source, {"__builtins__": {}}, {}), label)

    def test_suites_without_inputs_pass_none(self):
        jobs = ci_jobs(SUITES.compile_declaration(ROOT))
        for name, suite in declaration()["suites"].items():
            if "with" not in suite and "runners" not in suite:
                self.assertNotIn("with", jobs[name], name)

    def test_strings_that_yaml_would_retype_stay_strings(self):
        manifest = self.manifest(
            **{"with": {"a": "true", "b": "1", "c": "x: y", "d": "#"}}
        )
        self.assertEqual(
            ci_jobs(manifest)["test_xdbg"]["with"],
            {"a": "true", "b": "1", "c": "x: y", "d": "#"},
        )


class ProductRenderTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT, with_groups(declaration()))
    jobs = ci_jobs(manifest)

    def test_suites_need_only_their_groups_and_run_unless_one_failed(self):
        self.assertEqual(
            self.jobs["test_ios"]["needs"], ["detect-changes", "products_alpha"]
        )
        self.assertEqual(
            self.jobs["test_android"]["needs"],
            ["detect-changes", "products_alpha", "products_beta"],
        )
        self.assertEqual(self.jobs["test_xdbg"]["needs"], ["detect-changes"])
        outputs = {"test_suites": ["test_ios", "test_android"], "products": []}
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
                self.assertEqual(job_runs(self.jobs["test_ios"], outputs, results), ios)
                self.assertEqual(
                    job_runs(self.jobs["test_android"], outputs, results), android
                )
        unselected = {"test_suites": ["test_android"], "products": []}
        results = {"products_alpha": "success", "products_beta": "success"}
        self.assertFalse(job_runs(self.jobs["test_ios"], unselected, results))
        self.assertTrue(
            job_runs(self.jobs["products_alpha"], {"products": ["alpha"]}, {})
        )
        self.assertFalse(
            job_runs(self.jobs["products_beta"], {"products": ["alpha"]}, {})
        )
        self.assertEqual(self.jobs["products_alpha"]["name"], "Alpha")


class ExplainTests(unittest.TestCase):
    rules = SUITES.load_selector().Rules(SUITES.compile_declaration(ROOT))

    def test_explain_names_the_tier_of_each_suite(self):
        for path, expected in [
            (
                "apps/example-ios/ExampleApp/View.swift",
                "blocking: lint_config, test_example_ios",
            ),
            (
                "sdks/android/library/src/main/java/Client.kt",
                "blocking: lint_config, test_android; advisory: "
                "test_example_android, test_example_android_emulator",
            ),
            ("flake.nix", "shared (every suite)"),
            ("new-language/source.xyz", "unknown (every suite)"),
            ("AGENTS.md", "blocking: docs_quality (neutral)"),
        ]:
            with self.subTest(path=path):
                self.assertEqual(SUITES.explain(self.rules, path), expected)


class ValidateTests(unittest.TestCase):
    manifest = SUITES.compile_declaration(ROOT)

    def test_unknown_files_and_stale_patterns_fail(self):
        files = SUITES.tracked_files(ROOT) + ["new-language/source.xyz"]
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["paths"].append(
            "crates/xmtp_user_preferences/**"
        )
        errors = "\n".join(SUITES.validate(manifest, files=files))
        self.assertIn("new-language/source.xyz", errors)
        self.assertIn("test_dev: crates/xmtp_user_preferences/**", errors)

    def test_kache_secrets_need_workflow_declarations(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["secrets"] = "kache"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test-dev.yml must declare workflow_call secrets", errors)

    def test_pr_cache_secrets_need_workflow_declarations(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["secrets"] = "cachix-pr"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test-dev.yml must declare workflow_call secrets", errors)

    def test_caller_inputs_must_match_the_workflow(self):
        def errors(**suite):
            manifest = copy.deepcopy(self.manifest)
            manifest["suites"]["docs_site"].update(suite)
            return "\n".join(SUITES.validate(manifest))

        # deploy-docs.yml takes an optional boolean input `deploy`.
        self.assertEqual(errors(**{"with": {"deploy": False}}), "")
        self.assertIn(
            "deploy-docs.yml has no workflow_call input scope",
            errors(**{"with": {"scope": "full"}}),
        )
        self.assertIn(
            "with.deploy must be a boolean", errors(**{"with": {"deploy": "no"}})
        )
        self.assertEqual(errors(**{"with": {"deploy": "${{ inputs.x }}"}}), "")
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["workflow"] = "docs-rust-reference.yml"
        self.assertEqual([e for e in SUITES.validate(manifest) if "input" in e], [])

    def test_required_inputs_must_be_passed(self):
        # release-node-sdk.yml requires inputs that no suite passes.
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["workflow"] = "release-node-sdk.yml"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("release-node-sdk.yml requires inputs", errors)

    def test_runner_inputs_must_be_string_inputs_that_set_runs_on(self):
        manifest = copy.deepcopy(self.manifest)
        suite = manifest["suites"]["docs_site"]
        label = {"pr": "ubuntu-24.04", "merge": "ubuntu-24.04"}
        suite["runners"] = {"deploy": label, "runner": label}
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("deploy-docs.yml input deploy must have type string", errors)
        self.assertIn("no job of deploy-docs.yml runs on inputs.deploy", errors)
        self.assertIn("deploy-docs.yml has no workflow_call input runner", errors)

    def test_a_runner_input_that_sets_runs_on_is_valid(self):
        document = {
            "on": {
                "workflow_call": {
                    "inputs": {"runner": {"type": "string", "default": "ubuntu-24.04"}}
                }
            },
            "jobs": {"a": {"runs-on": "${{ inputs.runner }}"}},
        }
        suite = copy.deepcopy(self.manifest["suites"]["test_dev"])
        suite["runners"] = {"runner": {"pr": "ubuntu-24.04", "merge": "ubuntu-24.04"}}
        self.assertEqual(
            SUITES.validate_inputs("x", suite, document, document["on"]), []
        )
        document["jobs"]["a"]["runs-on"] = "${{ inputs.runner-large }}"
        self.assertEqual(
            SUITES.validate_inputs("x", suite, document, document["on"]),
            [f"x: no job of {suite['workflow']} runs on inputs.runner"],
        )

    def test_dead_brace_alternatives_are_stale(self):
        self.assertEqual(
            SUITES.brace_variants("a/{b,c}/{d,}.x"),
            ["a/b/{d,}.x", "a/c/{d,}.x", "a/{b,c}/d.x", "a/{b,c}/.x"],
        )
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["paths"].append("{docs,no-such-dir}/**")
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn("test_dev: no-such-dir/**", errors)
        self.assertNotIn("test_dev: docs/**", errors)

    def test_called_jobs_must_fit_the_suite_permissions(self):
        # GitHub checks a called job's permissions even when its `if` skips it.
        manifest = copy.deepcopy(self.manifest)
        manifest["suites"]["test_dev"]["workflow"] = "npm-publish.yml"
        errors = "\n".join(SUITES.validate(manifest))
        self.assertIn(
            "test_dev: npm-publish.yml job publish needs id-token: write", errors
        )
        manifest["suites"]["test_dev"]["permissions"] = {
            "contents": "read",
            "id-token": "write",
        }
        errors = "\n".join(SUITES.validate(manifest))
        self.assertNotIn("job publish needs", errors)


class IosLifecycleSplitTests(unittest.TestCase):
    """The iOS unit and lifecycle CI steps split the Swift tests between them."""

    def test_lifecycle_filter_and_skip_name_the_same_tests(self):
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
        self.assertEqual(skipped[-2], "--skip")
        self.assertEqual(selected[1], skipped[-1])


if __name__ == "__main__":
    unittest.main()
