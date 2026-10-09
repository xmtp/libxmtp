#!/usr/bin/env python3
"""Check CI selection, result-gate inputs, and decision summaries."""

import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock

import yaml


ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader("ci_select", str(ROOT / "dev/ci-select"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
SELECTOR = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(SELECTOR)


def event(draft=False, fork=False):
    return {
        "repository": {"full_name": "xmtp/libxmtp"},
        "pull_request": {
            "number": 123,
            "draft": draft,
            "changed_files": 1,
            "head": {
                "sha": "event-head",
                "repo": {
                    "full_name": "fork/libxmtp" if fork else "xmtp/libxmtp",
                },
            },
        },
    }


def outputs(checks=(), paths=("docs/guide.md",), shared=(), known=None):
    known = paths if known is None else known
    matched = list(checks)
    matched += [
        key
        for key, values in {
            "all_files": paths,
            "known_files": known,
            "shared": shared,
        }.items()
        if values
    ]
    return {
        "changes": json.dumps(matched),
        "all_files_files": json.dumps(paths),
        "known_files_files": json.dumps(known),
        "shared_files": json.dumps(shared),
    }


class SelectionTests(unittest.TestCase):
    def select(
        self,
        data=None,
        name="pull_request",
        payload=None,
        outcome="success",
        total=1,
        error=None,
    ):
        return SELECTOR.select_checks(
            outputs(["docs_site"]) if data is None else data,
            name,
            event() if payload is None else payload,
            outcome,
            total,
            error,
        )

    def test_docs_select_only_docs_and_quality(self):
        result = self.select()
        self.assertEqual(result["reasons"], [])
        self.assertEqual(
            result["plan"]["lint_jobs"], ["detect-changes", "docs-quality"]
        )
        self.assertEqual(result["plan"]["test_jobs"], ["detect-changes", "docs"])
        self.assertEqual(result["plan"]["test_suites"], [])

    def test_shared_input_selects_full_validation_and_records_paths(self):
        paths = ["apps/backend/src/error.rs", "dev/docker/up"]
        result = self.select(outputs(paths=paths, shared=paths))
        self.assertEqual(result["reasons"], ["shared_input"])
        self.assertEqual(result["shared_paths"], paths)
        self.assertTrue(result["plan"]["checks"]["test_ios_platform"])
        self.assertTrue(result["plan"]["checks"]["test_node"])

    def test_unknown_paths_select_full_and_are_named(self):
        result = self.select(outputs(paths=["new-language/source.xyz"], known=[]))
        self.assertEqual(result["reasons"], ["unknown_paths"])
        self.assertEqual(result["unknown_paths"], ["new-language/source.xyz"])
        self.assertTrue(result["plan"]["checks"]["test_wasm"])

    def test_full_reasons_accumulate(self):
        result = self.select(
            outputs(
                paths=["Cargo.toml", "new.xyz"],
                shared=["Cargo.toml"],
                known=["Cargo.toml"],
            ),
            total=3001,
        )
        self.assertEqual(
            result["reasons"], ["large_pr", "shared_input", "unknown_paths"]
        )

    def test_exact_api_limit_is_path_selected_and_over_limit_is_full(self):
        self.assertEqual(self.select(total=3000)["reasons"], [])
        result = self.select(total=3001)
        self.assertEqual(result["reasons"], ["large_pr"])
        self.assertTrue(result["plan"]["checks"]["lint_proto"])

    def test_rename_expansion_does_not_select_full(self):
        paths = [f"docs/renamed-{n}.md" for n in range(4000)]
        result = self.select(outputs(["docs_site"], paths=paths), total=2000)
        self.assertEqual(result["reasons"], [])
        self.assertEqual(result["plan"]["test_jobs"], ["detect-changes", "docs"])

    def test_bad_metadata_selects_full(self):
        for total in [None, "13", True, -1]:
            with self.subTest(total=total):
                result = self.select(total=total)
                self.assertEqual(result["reasons"], ["detection_failed"])
                self.assertTrue(result["plan"]["checks"]["check_rust"])

    def test_detector_failure_selects_full(self):
        result = self.select(outputs(["docs_site"]), outcome="failure")
        self.assertEqual(result["reasons"], ["detection_failed"])
        self.assertTrue(result["plan"]["checks"]["test_android"])

    def test_bad_detector_outputs_select_full(self):
        for key, value in [
            ("changes", ""),
            ("changes", "{}"),
            ("changes", "[]"),
            ("changes", '["unregistered_check"]'),
            ("all_files_files", ""),
            ("known_files_files", "null"),
            ("shared_files", '["outside-the-diff"]'),
        ]:
            with self.subTest(key=key, value=value):
                data = outputs(["docs_site"])
                data[key] = value
                result = self.select(data)
                self.assertEqual(result["reasons"], ["detection_failed"])
                self.assertTrue(result["plan"]["checks"]["test_workspace"])

    def test_unknown_outputs_are_not_confused_with_an_empty_diff(self):
        data = outputs(paths=[])
        self.assertEqual(self.select(data, total=0)["reasons"], [])
        del data["changes"]
        self.assertEqual(self.select(data, total=0)["reasons"], ["detection_failed"])

    def test_draft_and_fork_masks_apply_after_full_selection(self):
        result = self.select(
            outputs(paths=["Cargo.toml"], shared=["Cargo.toml"]),
            payload=event(draft=True),
        )
        self.assertEqual(result["reasons"], ["shared_input"])
        active = {
            k
            for k, v in result["plan"]["checks"].items()
            if v and k in SELECTOR.CHECK_CATALOG
        }
        self.assertEqual(active, SELECTOR.DRAFT_CHECKS)
        self.assertEqual(result["plan"]["test_jobs"], ["detect-changes"])
        result = self.select(
            outputs(paths=["Cargo.toml"], shared=["Cargo.toml"]),
            payload=event(fork=True),
        )
        for flag in SELECTOR.FORK_EXCLUSIONS:
            self.assertFalse(result["plan"]["checks"][flag])
        self.assertNotIn("test-ios", result["plan"]["test_jobs"])
        self.assertTrue(result["plan"]["checks"]["test_android"])

    def test_manual_validation_does_not_report_a_skipped_detector_as_failed(self):
        result = self.select(
            {}, name="workflow_dispatch", payload={}, outcome="skipped"
        )
        self.assertEqual(result["reasons"], ["explicit_request"])
        self.assertTrue(result["plan"]["checks"]["test_workspace"])

    def test_post_merge_rust_adds_language_checks_only_on_push(self):
        data = outputs(
            ["lint_workspace", "check_rust", "test_workspace"],
            paths=["crates/xmtp_mls/src/client.rs"],
        )
        self.assertFalse(self.select(data)["plan"]["checks"]["test_node"])
        result = self.select(data, name="push", payload={})
        for check in SELECTOR.POST_MERGE_RUST:
            self.assertTrue(result["plan"]["checks"][check], check)
        self.assertFalse(result["plan"]["checks"]["check_bindings_ios"])

    def test_owner_jobs_and_suites_are_unique_and_cover_selected_checks(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        targets = {
            "source-lint": yaml.safe_load(
                (ROOT / ".github/workflows/lint-target.yml").read_text()
            )["jobs"],
            "tests": yaml.safe_load(
                (ROOT / ".github/workflows/test-target.yml").read_text()
            )["jobs"],
        }
        result = self.select(outputs(paths=["Cargo.toml"], shared=["Cargo.toml"]))
        plan = result["plan"]
        for key in ["lint_jobs", "test_jobs", "source_suites", "test_suites"]:
            self.assertEqual(len(plan[key]), len(set(plan[key])), key)
        self.assertEqual(plan["test_suites"].count("test_bindings"), 1)
        self.assertIn("docs", plan["test_jobs"])
        self.assertNotIn("docs-rust-reference", plan["test_jobs"])
        rust = self.select(outputs(["docs_rust"]))["plan"]
        self.assertIn("docs-rust-reference", rust["test_jobs"])
        for name, check in SELECTOR.CHECK_CATALOG.items():
            self.assertIn(check.job, workflow["jobs"], name)
            if check.suite:
                self.assertIn(check.suite, targets[check.job], name)
            if check.flag:
                continue
            with self.subTest(name=name):
                single = SELECTOR.build_plan({name})
                self.assertIn(check.job, single[check.phase + "_jobs"])
                self.assertTrue(single["checks"][name])

    def test_lifecycle_filter_sets_the_platform_input_and_obeys_fork_policy(self):
        data = outputs(["test_ios_platform_lifecycle"])
        result = self.select(data)
        self.assertTrue(result["plan"]["checks"]["test_ios_platform"])
        self.assertFalse(result["plan"]["checks"]["test_ios"])
        self.assertIn("test-ios", result["plan"]["test_jobs"])
        self.assertNotIn(
            "test-ios", self.select(data, payload=event(fork=True))["plan"]["test_jobs"]
        )

    def test_summary_shows_causes_paths_and_required_jobs_as_text(self):
        path = 'apps/backend/<img src=x>/quote"\nCI_PATH_DATA.rs'
        result = self.select(outputs(paths=[path], shared=[path]))
        text = SELECTOR.summary(result)
        for value in [
            "shared_input",
            "Shared paths",
            "Matched filters",
            "Required test jobs",
            "test-ios",
        ]:
            self.assertIn(value, text)
        self.assertIn("&lt;img src=x&gt;", text)
        self.assertIn("\\nCI_PATH_DATA.rs", text)
        self.assertNotIn("<img src=x>", text)


class MetadataTests(unittest.TestCase):
    def test_event_total_does_not_require_an_api_call(self):
        with mock.patch.object(
            SELECTOR, "urlopen", side_effect=AssertionError("Unexpected API call")
        ):
            self.assertEqual(SELECTOR.pr_changed_files(event(), {}), (1, None))

    def test_api_total_is_checked_against_the_event_head(self):
        payload = event()
        del payload["pull_request"]["changed_files"]
        env = {
            "GITHUB_API_URL": "https://api.github.com",
            "GITHUB_REPOSITORY": "xmtp/libxmtp",
            "GITHUB_TOKEN": "test-token",
        }
        for head, total, expected in [
            ("event-head", 3001, (3001, None)),
            ("new-head", 1, (None, "The PR head changed after this event.")),
        ]:
            with self.subTest(head=head):
                response = mock.MagicMock()
                response.__enter__.return_value.read.return_value = json.dumps(
                    {"head": {"sha": head}, "changed_files": total}
                )
                with mock.patch.object(
                    SELECTOR, "urlopen", return_value=response
                ) as fetch:
                    self.assertEqual(SELECTOR.pr_changed_files(payload, env), expected)
                self.assertEqual(fetch.call_args.kwargs["timeout"], 20)


class WorkflowTests(unittest.TestCase):
    def test_filter_and_catalog_names_agree(self):
        filters = yaml.safe_load((ROOT / ".github/ci-paths.yml").read_text())
        self.assertEqual(
            set(filters) - SELECTOR.METADATA_FILTERS,
            set(SELECTOR.CHECK_CATALOG) - {"docs_quality"},
        )

    def test_workflow_passes_json_as_data_and_gates_use_the_plan(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        step = next(
            s
            for s in workflow["jobs"]["detect-changes"]["steps"]
            if s.get("id") == "select"
        )
        path = 'docs/quote"\nCI_PATH_DATA\n$(touch injected)\n<script>.md'
        data = outputs(["docs_site"], paths=[path])
        script = re.sub(
            r"\$\{\{ toJSON\(steps.paths.outputs.(\w+)\) \}\}",
            lambda m: json.dumps(data[m[1]]),
            step["run"],
        )
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            (temp / "event.json").write_text(json.dumps(event()))
            env = os.environ | {
                "GITHUB_EVENT_PATH": str(temp / "event.json"),
                "GITHUB_EVENT_NAME": "pull_request",
                "PATHS_OUTCOME": "success",
                "GITHUB_OUTPUT": str(temp / "outputs"),
                "GITHUB_STEP_SUMMARY": str(temp / "summary"),
            }
            # The selected workflow command must run from the checkout root.
            script = script.replace(
                "python3 dev/ci-select", f'python3 "{ROOT / "dev/ci-select"}"'
            )
            run = subprocess.run(
                ["bash", "-c", script],
                cwd=temp,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertFalse((temp / "injected").exists())
            values = dict(
                line.split("=", 1)
                for line in (temp / "outputs").read_text().splitlines()
            )
            plan = json.loads(values["selection"])
            self.assertEqual(plan["test_jobs"], ["detect-changes", "docs"])
            self.assertIn("docs_site", (temp / "summary").read_text())
            results = {name: {"result": "success"} for name in plan["test_jobs"]}
            gate = workflow["jobs"]["test"]["steps"][0]["run"]
            for state, expected in [
                ("success", 0),
                ("failure", 1),
                ("skipped", 1),
                ("cancelled", 1),
            ]:
                results["docs"]["result"] = state
                check = subprocess.run(
                    ["bash", "-c", gate],
                    env=os.environ
                    | {
                        "REQUIRED_JOBS": json.dumps(plan["test_jobs"]),
                        "JOB_RESULTS": json.dumps(results),
                    },
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(check.returncode, expected, state)


if __name__ == "__main__":
    unittest.main()
