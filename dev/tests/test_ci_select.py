#!/usr/bin/env python3
"""Check CI suite selection, result gates, and decision summaries."""

import copy
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

    def test_backend_change_runs_backend_and_client_suites_only(self):
        # A backend change runs the client suites that talk to the backend.
        # The platform suites add wrapper code; the merge run covers them.
        clients = {"test_workspace", "test_wasm", "test_node", "check_sdk_unit"}
        platforms = {
            "test_android",
            "test_example_android",
            "test_ios",
            "test_browser",
            "test_agent",
            "test_sdk_platform",
        }
        for path in (
            "apps/backend/src/main.rs",
            "nix/package/backend.nix",
            "dev/docker/up",
            "dev/docker/toxiproxy/config.json",
            "dev/backend/local-s3.toml",
        ):
            with self.subTest(path=path):
                suites = selected(select([path]))
                self.assertTrue(clients | {"test_backend", "test_xdbg"} <= suites)
                self.assertTrue(suites.isdisjoint(platforms))

    def test_android_paths_select_the_app_and_sdk_suites(self):
        # path: (suites it must select, suites it must not select)
        app_only = {"test_android", "check_sdk", "check_sdk_unit"}
        table = {
            "apps/example-android/app/src/main/java/Screen.kt": (
                {"test_example_android"},
                app_only,
            ),
            "apps/example-android/fixtures/metadata_backend.py": (
                {"test_example_android"},
                app_only,
            ),
            "nix/package/example-android-apk.nix": (
                {"test_example_android"},
                app_only,
            ),
            "sdks/android/library/src/main/java/Client.kt": (
                {"test_example_android", "test_android"},
                set(),
            ),
            "sdks/android/gradle/verification-metadata.xml": (
                {"test_example_android", "test_android"},
                {"check_rust", "test_workspace", "test_node", "test_ios"},
            ),
            "crates/xmtp_sdk/src/client.rs": (
                {"test_example_android", "test_android"},
                set(),
            ),
        }
        for path, (included, excluded) in table.items():
            with self.subTest(path=path):
                result = select([path])
                self.assertEqual(result["unknown_paths"], [])
                self.assertLessEqual(included, selected(result))
                self.assertTrue(excluded.isdisjoint(selected(result)))

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

    def test_summary_shows_paths_as_text(self):
        # A changed path cannot add HTML or Markdown lines to the step summary.
        path = "proto/<img src=x>\n## CI_PATH_DATA.proto"
        text = SELECTOR.summary(select([path]))
        self.assertNotIn("<img", text)
        lines = text.splitlines()
        self.assertEqual([x for x in lines if x.startswith("## CI_PATH")], [])
        self.assertIn("&lt;img src=x&gt;\\n## CI_PATH_DATA.proto", text)


def with_groups():
    """Two groups: test_android waits for both, test_example_android for one."""
    manifest = copy.deepcopy(SELECTOR.MANIFEST)
    manifest["products"] = {
        "alpha": {"cargo_roots": [], "crates": [], "paths": ["crates/xmtp_sdk/**"]},
        "beta": {
            "cargo_roots": [],
            "crates": [],
            "paths": ["sdks/android/**", "!sdks/android/**/*.md"],
        },
    }
    for name, suite in manifest["suites"].items():
        suite["products"] = []
    manifest["suites"]["test_android"]["products"] = ["alpha", "beta"]
    manifest["suites"]["test_example_android"]["products"] = ["beta"]
    return mock.patch.multiple(SELECTOR, MANIFEST=manifest, SUITES=manifest["suites"])


class ProductSelectionTests(unittest.TestCase):
    kotlin = "sdks/android/library/src/main/java/Client.kt"
    sdk = "crates/xmtp_sdk/src/client.rs"

    def test_groups_need_a_waiting_suite_and_a_matching_path(self):
        with with_groups():
            for files, expected in [
                ([self.kotlin], ["beta"]),
                # One suite that waits for two groups.
                ([self.kotlin, self.sdk], ["alpha", "beta"]),
                # test_android runs, but no path matches beta's filters.
                ([self.sdk], ["alpha"]),
                ([self.sdk.replace("client.rs", "AGENTS.md")], []),
                (["sdks/android/README.md"], []),
                # No selected suite waits for a group.
                (["crates/xmtp_mls/src/client.rs"], []),
                (["apps/example-android/app/build.gradle"], []),
            ]:
                with self.subTest(files=files):
                    result = select(files)
                    self.assertEqual(result["products"], expected)


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


class MetadataTests(unittest.TestCase):
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
        self.assertEqual(json.loads(values["products"]), [])
        # The path is not in the checkout, so it counts as deleted.
        lint = json.loads(values["lint_suites"])
        self.assertEqual(set(lint), {"docs_quality", "lint_config"})
        self.assertIn("docs_site", text)
        outputs = self.workflow["jobs"]["detect-changes"]["outputs"]
        self.assertEqual(set(outputs), {"lint_suites", "test_suites", "products"})

    def gate(self, script, env):
        return subprocess.run(
            ["bash", "-c", script],
            env=os.environ | env,
            capture_output=True,
            text=True,
        ).returncode

    def test_suite_gate_requires_every_selected_suite(self):
        step = self.generated["test"]["jobs"]["required"]["steps"][0]
        script = step["run"]
        static = {"PRODUCTS": "[]", "WAITS": step["env"]["WAITS"]}
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
            env = static | {
                "SELECTED": '["docs_site"]',
                "RESULTS": json.dumps(results),
            }
            self.assertEqual(self.gate(script, env), expected, state)
        env = static | {"SELECTED": "[]", "RESULTS": json.dumps(results)}
        self.assertEqual(self.gate(script, env), 0)

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
