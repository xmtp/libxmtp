#!/usr/bin/env python3
"""Exercise CI input selection and failure gates with independent cases."""

import importlib.util
from pathlib import Path
import unittest


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


selector = module("selector", "select-checks.py")
gate = module("gate", "check-gate.py")


class SelectionTests(unittest.TestCase):
    def selected(self, path):
        return selector.select([path], verified=True)["checks"]

    def test_generator_selects_both_public_families_and_native_checks(self):
        checks = self.selected("apps/xmtp_sdk_bindgen/templates/bridge/index.ts")
        for name in (
            "sdk_node",
            "sdk_browser",
            "test_node",
            "test_browser",
            "test_agent",
            "test_bindings",
            "check_types",
            "check_rust",
        ):
            self.assertTrue(checks[name], name)

    def test_shared_inputs_select_every_build_family(self):
        for path in (
            "Cargo.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "nix/lib/sdk-sources.nix",
            ".github/actions/setup-nix/action.yml",
            "pnpm-workspace.yaml",
            ".config/nextest.toml",
        ):
            with self.subTest(path=path):
                checks = self.selected(path)
                self.assertTrue(all(checks.values()))

    def test_core_and_service_changes_select_runtime_checks(self):
        for path in (
            "crates/xmtp_db/src/lib.rs",
            "proto/message.proto",
            "apps/backend/migrations/new.sql",
            "dev/docker/compose.yml",
        ):
            with self.subTest(path=path):
                checks = self.selected(path)
                for name in (
                    "test_workspace",
                    "test_backend",
                    "test_native_backend",
                    "test_node",
                    "test_browser",
                    "test_agent",
                    "backend_products",
                ):
                    self.assertTrue(checks[name], name)

    def test_prose_reuses_products_without_selecting_runtime_tests(self):
        checks = self.selected("docs/self-hosted/client-guide.md")
        for name in ("docs_quality", "docs_site", "sdk_node", "sdk_browser"):
            self.assertTrue(checks[name], name)
        for name in ("test_node", "test_browser", "test_workspace"):
            self.assertFalse(checks[name], name)

    def test_owned_package_manifests_keep_config_checks(self):
        for path in (
            "crates/xmtp_sdk/Cargo.toml",
            "apps/backend/Cargo.toml",
            "sdks/ios/Sources/Client.swift",
            "sdks/node/type-tests/publicSurface.ts",
        ):
            with self.subTest(path=path):
                self.assertTrue(self.selected(path)["lint_config"])

    def test_unverified_advanced_or_stacked_base_selects_all(self):
        checks = selector.select(["docs/guide.md"], verified=False)["checks"]
        self.assertTrue(all(checks.values()))
        union = selector.select(
            ["crates/xmtp_mls/src/lib.rs", "docs/guide.md"], verified=True
        )["checks"]
        self.assertTrue(union["test_node"])
        self.assertTrue(union["test_workspace"])

    def test_unknown_input_and_native_fork_policy(self):
        checks = self.selected("new-provider/source.custom")
        self.assertTrue(all(checks.values()))
        fork = selector.select([], fork=True)["checks"]
        self.assertFalse(fork["test_ios"])
        self.assertFalse(fork["test_native_backend"])
        self.assertTrue(fork["test_node"])

    def test_changed_deleted_and_renamed_paths_use_same_rules(self):
        for path in (
            "sdks/node/test/deleted.test.ts",
            "sdks/browser/test/renamed.test.ts",
        ):
            self.assertTrue(self.selected(path)["test_browser"])
        for path in ("/absolute", "../escape", ""):
            with self.assertRaises(ValueError):
                self.selected(path)


class GateTests(unittest.TestCase):
    def setUp(self):
        self.selection = {"schema_version": 1, "checks": {"test_node": True}}
        self.mapping = {"test_node": "node"}
        self.needs = {
            "detect-changes": {"result": "success"},
            "node": {"result": "success"},
        }

    def test_valid_selected_and_deliberately_unselected(self):
        gate.check_gate(self.selection, self.needs, self.mapping)
        self.selection["checks"]["test_node"] = False
        self.needs["node"]["result"] = "skipped"
        gate.check_gate(self.selection, self.needs, self.mapping)

    def test_failed_or_cancelled_detector_never_passes(self):
        self.selection["checks"]["test_node"] = False
        self.needs["node"]["result"] = "skipped"
        for result in ("failure", "cancelled", "skipped"):
            self.needs["detect-changes"]["result"] = result
            with self.assertRaises(ValueError):
                gate.check_gate(self.selection, self.needs, self.mapping)

    def test_selected_failure_cancel_skip_or_missing_result_fails(self):
        for result in ("failure", "cancelled", "skipped", None):
            self.needs["node"]["result"] = result
            with self.assertRaises(ValueError):
                gate.check_gate(self.selection, self.needs, self.mapping)

    def test_malformed_selection_and_unknown_status_fail(self):
        for value in (None, "false", 0):
            self.selection["checks"]["test_node"] = value
            with self.assertRaises(ValueError):
                gate.check_gate(self.selection, self.needs, self.mapping)
        self.selection["schema_version"] = 0
        with self.assertRaises(ValueError):
            gate.check_gate(self.selection, self.needs, self.mapping)


if __name__ == "__main__":
    unittest.main()
