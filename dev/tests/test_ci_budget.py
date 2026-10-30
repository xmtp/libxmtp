#!/usr/bin/env python3
"""Exercise the reduced PR budget and full scheduled coverage."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

import test_ci_select
from test_ci_suites import evaluate
from test_ci_select import advisory, event, select, selected


ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = {
    "test_example_android",
    "test_example_android_emulator",
    "test_example_ios",
}


class BudgetSelectionTests(unittest.TestCase):
    def test_examples_leave_merges_but_keep_pr_and_full_runs(self):
        self.assertTrue(EXAMPLES.isdisjoint(selected(select(name="push"))))
        for name in ("schedule", "workflow_dispatch"):
            result = select(files=None, name=name, payload={}, outcome="skipped")
            self.assertLessEqual(
                EXAMPLES | {"test_react_native", "test_wasm", "docs_site", "test_ios"},
                selected(result),
            )
            self.assertEqual(advisory(result), set())
            self.assertNotIn("detection_failed", result["reasons"])
        self.assertIn(
            "test_example_ios", selected(select(["apps/example-ios/App.swift"]))
        )
        self.assertLessEqual(
            EXAMPLES - {"test_example_ios"},
            selected(select([], payload=event(labels=["android"]))),
        )

    def test_wasm_samples_core_changes_and_keeps_sensitive_paths_full(self):
        for path in ("crates/xmtp_mls/src/client.rs", "apps/backend/src/main.rs"):
            suites = selected(select([path]))
            self.assertIn("test_wasm_sample", suites)
            self.assertNotIn("test_wasm", suites)
        for path in (
            "crates/xmtp_db/src/encrypted_store/database/wasm/restore.rs",
            "crates/xmtp_sdk/src/client/wasm_storage_tests.rs",
            "crates/xmtp_attachments/src/store/opfs.rs",
            "crates/xmtp_db/tests/opfs.rs",
            "crates/xmtp_mls/tests/attachment_opfs.rs",
            "crates/xmtp_logging/src/layers/web.rs",
            "crates/xmtp_common/Cargo.toml",
            "crates/xmtp_db/Cargo.toml",
            "crates/xmtp_cryptography/Cargo.toml",
            "sdks/browser/test/platform/bridge.worker.mts",
            "Cargo.lock",
            "nix/package/wasm-nextest.nix",
            "webdriver.json",
            "flake.nix",
        ):
            with self.subTest(path=path):
                suites = selected(select([path]))
                self.assertIn("test_wasm", suites)
                self.assertNotIn("test_wasm_sample", suites)
        full = selected(select([], payload=event(labels=["ci-wasm-full"])))
        self.assertIn("test_wasm", full)
        self.assertNotIn("test_wasm_sample", full)
        for name in ("push", "schedule", "workflow_dispatch"):
            self.assertIn("test_wasm", selected(select(name=name, payload={})))

    def test_broad_rn_smoke_needs_a_label(self):
        for path in (
            "apps/backend/src/main.rs",
            "crates/xmtp_sdk/src/lib.rs",
            "flake.nix",
        ):
            result = select([path])
            self.assertNotIn(
                "test_react_native_smoke", advisory(result) | selected(result)
            )
        result = select([], payload=event(labels=["ci-rn-smoke"]))
        self.assertEqual(advisory(result), {"test_react_native_smoke"})
        result = select(
            ["sdks/react-native/src/index.ts"], payload=event(labels=["ci-rn-smoke"])
        )
        self.assertIn("test_react_native", advisory(result))
        self.assertNotIn("test_react_native_smoke", advisory(result))
        self.assertFalse(
            advisory(select([], payload=event(draft=True, labels=["ci-rn-smoke"])))
        )

    def test_backend_only_pr_uses_darwin_smoke_not_full_ios(self):
        suites = selected(select(["apps/backend/src/main.rs"]))
        self.assertIn("test_backend_darwin", suites)
        self.assertNotIn("test_ios", suites)
        for path in (
            "sdks/ios/Sources/Client.swift",
            "crates/xmtp_sdk/src/lib.rs",
            "nix/lib/ios-env.nix",
        ):
            suites = selected(select(["apps/backend/src/main.rs", path]))
            self.assertIn("test_ios", suites)
            self.assertNotIn("test_backend_darwin", suites)

    def test_prose_uses_lint_and_published_site_changes_keep_full_docs(self):
        for path in (
            "docs/nix-setup.md",
            "docs/specs/CONF-backend-configuration.md",
            "AGENTS.md",
        ):
            suites = selected(select([path]))
            self.assertIn("lint_config", suites)
            self.assertNotIn("docs_site", suites)
        for path in (
            "apps/docs/src/content/docs/index.mdx",
            "apps/docs/scripts/check-site.mjs",
            "crates/xmtp_sdk/src/lib.rs",
        ):
            self.assertIn("docs_site", selected(select([path])))


class WasmBudgetTests(unittest.TestCase):
    def test_partition_selection_rotates_and_non_pr_runs_are_full(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/test-wasm.yml").read_text()
        )
        archive = workflow["jobs"]["archive"]
        step = next(s for s in archive["steps"] if s.get("id") == "partitions")
        self.assertEqual(
            archive["outputs"]["partitions"],
            "${{ steps.partitions.outputs.partitions }}",
        )
        self.assertEqual(
            workflow["jobs"]["partition"]["strategy"]["matrix"]["part"],
            "${{ fromJSON(needs.archive.outputs.partitions) }}",
        )
        for scope, event_name, number, expected in [
            ("sample", "pull_request", n, [n % 4 + 1]) for n in range(1, 9)
        ] + [
            (scope, name, 1, [1, 2, 3, 4])
            for scope, name in (
                ("full", "pull_request"),
                ("sample", "push"),
                ("sample", "schedule"),
                ("sample", "workflow_dispatch"),
            )
        ]:
            with (
                self.subTest(scope=scope, event=event_name, number=number),
                tempfile.TemporaryDirectory() as directory,
            ):
                output = Path(directory) / "output"
                env = os.environ | {
                    "SCOPE": scope,
                    "EVENT_NAME": event_name,
                    "RUN_NUMBER": str(number),
                    "GITHUB_OUTPUT": str(output),
                }
                subprocess.run(
                    ["bash", "-euc", step["run"]],
                    env=env,
                    check=True,
                    capture_output=True,
                )
                self.assertEqual(
                    json.loads(output.read_text().split("=", 1)[1]), expected
                )

    def test_full_ci_has_a_daily_trigger(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        self.assertEqual(workflow["on"]["schedule"], [{"cron": "17 9 * * *"}])

    def test_nightly_rn_runs_every_journey(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/test-react-native.yml").read_text()
        )
        consumers = workflow["jobs"]["consumers"]
        expression = consumers["env"]["XMTP_RN_JOURNEYS"]
        for name, expected in (
            ("pull_request", "pr"),
            ("schedule", "full"),
            ("workflow_dispatch", "full"),
        ):
            self.assertEqual(
                evaluate(expression, {"github.event_name": name}), expected
            )

    def test_schedule_runs_without_path_detection_and_alerts_on_failure(self):
        helper = test_ci_select.WorkflowTests()
        run, outputs, _ = helper.run_select(
            {"changes": "null", "all_files_files": "null"}, {}, "schedule"
        )
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertLessEqual(
            EXAMPLES | {"test_react_native", "test_wasm", "test_ios", "docs_site"},
            set(json.loads(outputs["test_suites"])),
        )
        self.assertEqual(json.loads(outputs["advisory_suites"]), [])
        for phase in ("lint", "test"):
            steps = helper.jobs[phase]["steps"]
            notify = next(
                s for s in steps if s.get("uses") == "./.github/actions/slack-notify"
            )
            self.assertEqual(
                notify["if"], "${{ failure() && github.event_name == 'schedule' }}"
            )
            checkout = next(
                s for s in steps if s.get("uses", "").startswith("actions/checkout@")
            )
            self.assertEqual(checkout["if"], notify["if"])
            self.assertLess(steps.index(checkout), steps.index(notify))


if __name__ == "__main__":
    unittest.main()
