#!/usr/bin/env python3
"""Check CI suite selection, result gates, and decision summaries."""

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
ALL = set(SELECTOR.SUITES)


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


def select(
    files=("docs/guide.md",),
    name="pull_request",
    payload=None,
    outcome="success",
    total=1,
    error=None,
    deleted=(),
):
    return SELECTOR.select_suites(
        None if files is None else list(files),
        name,
        event() if payload is None else payload,
        outcome,
        total,
        error,
        deleted,
    )


def selected(result):
    return set(result["suites"]["lint"] + result["suites"]["test"])


def eligible(kind, fork=False):
    return {
        n
        for n, s in SELECTOR.SUITES.items()
        if kind in s["run_on"] and not (fork and s["disable_on_forks"])
    }


class SelectionTests(unittest.TestCase):
    def test_docs_select_docs_suites_only(self):
        result = select()
        self.assertEqual(result["reasons"], [])
        self.assertEqual(result["suites"]["lint"], ["docs_quality"])
        self.assertEqual(result["suites"]["test"], ["docs_site"])

    def test_rust_closures_narrow_selection(self):
        client = selected(select(["crates/xmtp_mls/src/client.rs"]))
        self.assertIn("test_workspace", client)
        self.assertTrue(
            client.isdisjoint(
                {"test_backend", "test_validation", "test_keepalive", "test_node"}
            )
        )
        backend_doc = selected(select(["docs/backend-observability.md"]))
        self.assertIn("test_backend", backend_doc)
        proto = selected(select(["crates/xmtp_proto/src/lib.rs"]))
        self.assertIn("test_validation", proto)

    def test_neutral_markdown_selects_only_markdown_suites(self):
        result = select(["crates/xmtp_mls/AGENTS.md"])
        self.assertEqual(result["reasons"], [])
        self.assertEqual(selected(result), {"docs_quality"})
        self.assertEqual(selected(select(["img/tech-stack.png"])), set())

    def test_neutral_files_reach_repository_wide_globs(self):
        # treefmt formats shell scripts everywhere, including neutral directories.
        result = select([".murmur/provisioning.sh"])
        self.assertEqual(selected(result), {"lint_config"})

    def test_deleted_paths_run_the_declaration_check(self):
        self.assertNotIn("lint_config", selected(select()))
        result = select(["docs/guide.md"], deleted=["docs/guide.md"])
        self.assertIn("lint_config", selected(result))
        self.assertEqual(result["deleted_paths"], ["docs/guide.md"])

    def test_deleted_fork_is_a_fork(self):
        payload = event(fork=True)
        payload["pull_request"]["head"]["repo"] = None
        result = select(["proto/a.proto"], payload=payload)
        self.assertTrue(result["fork"])
        self.assertNotIn("test_ios", selected(result))

    def test_shared_input_selects_every_suite_and_records_paths(self):
        result = select(["proto/mls/a.proto", "crates/xmtp_mls/src/client.rs"])
        self.assertEqual(result["reasons"], ["shared_input"])
        self.assertEqual(result["shared_paths"], ["proto/mls/a.proto"])
        self.assertEqual(selected(result), eligible("ready_pr_push") - {"docs_rust"})

    def test_unknown_paths_select_every_suite_and_are_named(self):
        result = select(["new-language/source.xyz"])
        self.assertEqual(result["reasons"], ["unknown_paths"])
        self.assertEqual(result["unknown_paths"], ["new-language/source.xyz"])
        self.assertIn("test_wasm", selected(result))

    def test_full_reasons_accumulate(self):
        result = select(["proto/a.proto", "new.xyz"], total=3001)
        self.assertEqual(
            result["reasons"], ["large_pr", "shared_input", "unknown_paths"]
        )

    def test_exact_api_limit_is_path_selected_and_over_limit_is_full(self):
        self.assertEqual(select(total=3000)["reasons"], [])
        result = select(total=3001)
        self.assertEqual(result["reasons"], ["large_pr"])
        self.assertIn("lint_proto", selected(result))

    def test_rename_expansion_does_not_select_full(self):
        paths = [f"docs/renamed-{n}.md" for n in range(4000)]
        result = select(paths, total=2000)
        self.assertEqual(result["reasons"], [])
        self.assertEqual(result["suites"]["test"], ["docs_site"])

    def test_bad_metadata_selects_full(self):
        for total in [None, "13", True, -1]:
            with self.subTest(total=total):
                result = select(total=total)
                self.assertEqual(result["reasons"], ["detection_failed"])
                self.assertIn("check_rust", selected(result))

    def test_detector_failure_and_missing_files_select_full(self):
        for kwargs in [{"outcome": "failure"}, {"files": None}]:
            with self.subTest(kwargs=str(kwargs)):
                result = select(**kwargs)
                self.assertEqual(result["reasons"], ["detection_failed"])
                self.assertIn("test_android", selected(result))

    def test_bad_detector_input_is_unusable(self):
        good = {"changes": '["all_files"]', "all_files_files": '["docs/a.md"]'}
        self.assertEqual(SELECTOR.read_files(json.dumps(good)), ["docs/a.md"])
        for key, value in [
            ("changes", ""),
            ("changes", "{}"),
            ("changes", "[]"),
            ("all_files_files", ""),
            ("all_files_files", "null"),
            ("all_files_files", "[1]"),
        ]:
            with self.subTest(key=key, value=value):
                data = json.dumps(good | {key: value})
                self.assertIsNone(SELECTOR.read_files(data))
        self.assertIsNone(SELECTOR.read_files("not json"))

    def test_empty_diff_is_valid(self):
        data = {"changes": "[]", "all_files_files": "[]"}
        self.assertEqual(SELECTOR.read_files(json.dumps(data)), [])
        result = select([], total=0)
        self.assertEqual(result["reasons"], [])
        self.assertEqual(selected(result), set())

    def test_draft_pushes_run_only_draft_suites(self):
        result = select(["proto/a.proto"], payload=event(draft=True))
        self.assertEqual(result["event_kind"], "draft_pr_push")
        self.assertEqual(selected(result), eligible("draft_pr_push"))
        self.assertIn("lint_workspace", selected(result))
        self.assertEqual(result["suites"]["test"], [])

    def test_forks_skip_disabled_suites(self):
        result = select(["proto/a.proto"], payload=event(fork=True))
        self.assertNotIn("test_ios", selected(result))
        self.assertIn("test_android", selected(result))
        self.assertIn("test_ios", result["policy_excluded"])

    def test_manual_run_selects_every_suite(self):
        result = select(None, name="workflow_dispatch", payload={}, outcome="skipped")
        self.assertEqual(result["reasons"], ["explicit_request"])
        self.assertEqual(selected(result), ALL - {"docs_rust"})

    def test_merge_runs_every_merge_suite_without_path_matches(self):
        result = select(["docs/guide.md"], name="push", payload={})
        self.assertEqual(result["reasons"], ["merge"])
        self.assertEqual(selected(result), eligible("merge") - {"docs_rust"})

    def test_covered_suite_is_dropped_when_its_cover_runs(self):
        alone = selected(select(["docs/error_glossary.md"]))
        self.assertIn("docs_site", alone)
        self.assertNotIn("docs_rust", alone)
        rust = selected(select(["crates/xmtp_mls/src/client.rs"]))
        self.assertIn("docs_rust", rust)
        both = selected(select(["crates/xmtp_mls/src/client.rs", "apps/docs/a.ts"]))
        self.assertIn("docs_site", both)
        self.assertNotIn("docs_rust", both)

    def test_summary_shows_causes_paths_and_suites_as_text(self):
        # Shared paths are listed in the summary.
        path = 'proto/<img src=x>/quote"\nCI_PATH_DATA.proto'
        text = SELECTOR.summary(select([path]))
        for value in [
            "shared_input",
            "Shared paths",
            "Selected test suites",
            "test_ios",
        ]:
            self.assertIn(value, text)
        self.assertIn("&lt;img src=x&gt;", text)
        self.assertIn("\\nCI_PATH_DATA.proto", text)
        self.assertNotIn("<img src=x>", text)


class MatchTests(unittest.TestCase):
    def test_glob_semantics(self):
        for pattern, path, expected in [
            ("**/AGENTS.md", "AGENTS.md", True),
            ("**/AGENTS.md", "crates/a/AGENTS.md", True),
            ("crates/*.rs", "crates/a/b.rs", False),
            ("crates/**", "crates/a/b.rs", True),
            ("nix/package/backend{,-ci}.nix", "nix/package/backend-ci.nix", True),
            ("nix/package/backend{,-ci}.nix", "nix/package/backend-cix.nix", False),
            ("sdks/{node,browser}/**/*.{ts,tsx}", "sdks/node/src/a/b.tsx", True),
            ("a?.md", "ab.md", True),
            ("a?.md", "a/.md", False),
            ("docs/a+b.md", "docs/aab.md", False),
            ("docs/**", "docs/a\nb.md", True),
        ]:
            with self.subTest(pattern=pattern, path=path):
                self.assertEqual(SELECTOR.glob_match(pattern, path), expected)

    def rules(self):
        return SELECTOR.Rules(
            {
                "shared": ["proto/**"],
                "neutral": ["**/AGENTS.md"],
                "suites": {
                    "rust": {
                        "crates": ["crates/a/**"],
                        "workflow_files": [".github/workflows/rust.yml"],
                        "paths": ["Cargo.lock", "!crates/a/fixtures/**"],
                    },
                    "markdown": {
                        "crates": [],
                        "workflow_files": [],
                        "paths": ["**/*.md"],
                    },
                },
            }
        )

    def test_suites_match_their_inputs_and_exclusions(self):
        rules = self.rules()
        self.assertEqual(rules.suites_for("crates/a/src/lib.rs"), ["rust"])
        self.assertEqual(rules.suites_for(".github/workflows/rust.yml"), ["rust"])
        self.assertEqual(rules.suites_for("crates/a/fixtures/x.bin"), [])
        self.assertTrue(rules.known("crates/a/fixtures/x.bin"))

    def test_neutral_files_reach_only_markdown_patterns(self):
        rules = self.rules()
        self.assertEqual(rules.suites_for("crates/a/AGENTS.md"), ["markdown"])
        self.assertEqual(rules.suites_for("crates/a/README.md"), ["rust", "markdown"])

    def test_shared_and_unknown(self):
        rules = self.rules()
        self.assertTrue(rules.shared("proto/a.proto"))
        self.assertFalse(rules.shared("proto/AGENTS.md"))
        self.assertFalse(rules.known("new/file.xyz"))


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


def load(path):
    return yaml.safe_load((ROOT / path).read_text())


class WorkflowTests(unittest.TestCase):
    workflow = load(".github/workflows/ci.yml")
    generated = {
        phase: load(f".github/workflows/{phase}-generated.yml")
        for phase in ("lint", "test")
    }

    def run_select(self, data):
        step = next(
            s
            for s in self.workflow["jobs"]["detect-changes"]["steps"]
            if s.get("id") == "select"
        )
        script = re.sub(
            r"\$\{\{ toJSON\(steps.paths.outputs.(\w+)\) \}\}",
            lambda m: json.dumps(data[m[1]]),
            step["run"],
        )
        # The selected workflow command must run from the checkout root.
        script = script.replace(
            "python3 dev/ci-select", f'python3 "{ROOT / "dev/ci-select"}"'
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
            return values, (temp / "summary").read_text()

    def test_workflow_passes_json_as_data(self):
        path = 'docs/quote"\nCI_PATH_DATA\n$(touch injected)\n<script>.md'
        data = {
            "changes": json.dumps(["all_files"]),
            "all_files_files": json.dumps([path]),
        }
        values, text = self.run_select(data)
        self.assertEqual(json.loads(values["test_suites"]), ["docs_site"])
        # The path is not in the checkout, so it counts as deleted.
        lint = json.loads(values["lint_suites"])
        self.assertEqual(set(lint), {"docs_quality", "lint_config"})
        self.assertIn("docs_site", text)
        outputs = self.workflow["jobs"]["detect-changes"]["outputs"]
        self.assertEqual(set(outputs), {"lint_suites", "test_suites"})

    def test_generated_workflows_have_one_job_per_suite(self):
        for phase, workflow in self.generated.items():
            names = {n for n, s in SELECTOR.SUITES.items() if s["phase"] == phase}
            jobs = workflow["jobs"]
            self.assertEqual(set(jobs) - {"required"}, names, phase)
            self.assertEqual(set(jobs["required"]["needs"]), names, phase)
            for name in names:
                self.assertIn(f"'{name}'", jobs[name]["if"])
            call = self.workflow["jobs"][f"{phase}-suites"]
            self.assertEqual(call["uses"], f"./.github/workflows/{phase}-generated.yml")
            self.assertIn(f"outputs.{phase}_suites", call["with"]["suites"])

    def gate(self, script, env):
        return subprocess.run(
            ["bash", "-c", script],
            env=os.environ | env,
            capture_output=True,
            text=True,
        ).returncode

    def test_suite_gate_requires_every_selected_suite(self):
        script = self.generated["test"]["jobs"]["required"]["steps"][0]["run"]
        results = {
            "docs_site": {"result": "success"},
            "test_ios": {"result": "skipped"},
        }
        for state, expected in [
            ("success", 0),
            ("failure", 1),
            ("skipped", 1),
            ("cancelled", 1),
        ]:
            results["docs_site"]["result"] = state
            env = {"SELECTED": '["docs_site"]', "RESULTS": json.dumps(results)}
            self.assertEqual(self.gate(script, env), expected, state)
        env = {"SELECTED": "[]", "RESULTS": json.dumps(results)}
        self.assertEqual(self.gate(script, env), 0)

    def test_only_pull_request_runs_cancel_older_runs(self):
        cancel = self.workflow["concurrency"]["cancel-in-progress"]
        self.assertEqual(cancel, "${{ github.event_name == 'pull_request' }}")

    def test_required_checks_need_detection_and_their_phase(self):
        for job, call in [("lint", "lint-suites"), ("test", "test-suites")]:
            gate = self.workflow["jobs"][job]
            self.assertEqual(gate["needs"], ["detect-changes", call])
            script = gate["steps"][0]["run"]
            for detected, suites, expected in [
                ("success", "success", 0),
                ("failure", "skipped", 1),
                ("success", "failure", 1),
                ("success", "skipped", 1),
            ]:
                env = {"DETECTED": detected, "SUITES": suites}
                result = self.gate(script, env)
                self.assertEqual(result, expected, (job, detected, suites))


if __name__ == "__main__":
    unittest.main()
