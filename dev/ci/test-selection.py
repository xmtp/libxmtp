#!/usr/bin/env python3
"""Exercise CI input selection and failure gates with independent cases."""

import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
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

    def test_owned_package_manifests_keep_full_checks(self):
        for path in (
            "crates/xmtp_sdk/Cargo.toml",
            "apps/backend/Cargo.toml",
        ):
            with self.subTest(path=path):
                self.assertTrue(all(self.selected(path).values()))

    def test_source_owners_keep_tree_format_readers(self):
        for path in (
            "sdks/ios/Sources/Client.swift",
            "sdks/node/type-tests/publicSurface.ts",
            "sdks/browser/src/Client.ts",
        ):
            with self.subTest(path=path):
                self.assertTrue(self.selected(path)["lint_config"])
        self.assertFalse(self.selected("crates/xmtp_mls/src/lib.rs")["lint_config"])

    def test_unverified_advanced_or_stacked_base_selects_all(self):
        checks = selector.select(["docs/guide.md"], verified=False)["checks"]
        self.assertTrue(all(checks.values()))
        union = selector.select(
            ["crates/xmtp_mls/src/lib.rs", "docs/guide.md"], verified=True
        )["checks"]
        self.assertFalse(union["test_node"])
        self.assertTrue(union["check_sdk_unit"])
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
            self.assertTrue(
                self.selected(path)[
                    "test_node" if path.startswith("sdks/node/") else "test_browser"
                ]
            )
        for path in ("/absolute", "../escape", ""):
            with self.assertRaises(ValueError):
                self.selected(path)

    def test_unknown_toml_nix_and_hakari_select_full_consumers(self):
        for path in (
            ".config/hakari.toml",
            "new-provider/build-inputs.toml",
            "new-provider/build-inputs.nix",
            "sdks/node/build-inputs.nix",
        ):
            with self.subTest(path=path):
                self.assertTrue(all(self.selected(path).values()))

    def test_handwritten_sdk_sources_keep_runtime_without_generated_or_rust_units(self):
        for path in ("sdks/node/src/index.ts", "sdks/browser/src/index.ts"):
            with self.subTest(path=path):
                checks = self.selected(path)
                self.assertFalse(checks["check_sdk"])
                self.assertFalse(checks["check_sdk_unit"])
        for path in ("crates/xmtp_sdk/src/lib.rs", "apps/backend/src/main.rs"):
            self.assertTrue(self.selected(path)["check_sdk_unit"])

    def test_pure_rust_pr_retains_rust_tests_without_language_consumers(self):
        for path in (
            "crates/xmtp_mls/src/lib.rs",
            "crates/xmtp_db/tests/store.rs",
            "apps/keepalive-probe/src/main.rs",
        ):
            with self.subTest(path=path):
                checks = self.selected(path)
                for name in (
                    "lint_workspace",
                    "check_rust",
                    "test_workspace",
                    "test_wasm",
                    "check_sdk_unit",
                    "backend_products",
                    "docs_rust",
                ):
                    self.assertTrue(checks[name], name)
                for name in (
                    "lint_js",
                    "lint_ios",
                    "lint_android",
                    "check_types",
                    "check_sdk",
                    "test_node",
                    "test_browser",
                    "test_agent",
                    "test_bindings",
                    "test_ios",
                    "test_android",
                    "test_sdk_staging",
                    "test_bridge_runtime",
                    "test_browser_platform",
                    "test_swift_seams",
                    "sdk_node",
                    "sdk_browser",
                    "docs_site",
                    "docs_swift",
                    "docs_kotlin",
                ):
                    self.assertFalse(checks[name], name)

    def test_owned_language_sources_select_only_their_runtime_flags(self):
        cases = {
            "sdks/node/src/Client.ts": {"test_node", "test_agent"},
            "sdks/agent/src/Client.ts": {"test_agent"},
            "sdks/browser/src/Client.ts": {"test_browser"},
            "sdks/ios/Sources/Client.swift": {"test_ios", "test_bindings"},
            "sdks/android/library/src/Client.kt": {
                "test_android",
                "test_bindings",
                "test_sdk_staging",
            },
        }
        names = {
            "test_node",
            "test_browser",
            "test_agent",
            "test_ios",
            "test_android",
            "test_bindings",
            "test_sdk_staging",
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                checks = self.selected(path)
                self.assertEqual({name for name in names if checks[name]}, expected)
                self.assertFalse(checks["check_sdk_unit"])
                self.assertFalse(checks["check_sdk"])
                self.assertEqual(
                    checks["check_bindings_ios"], path.startswith("sdks/ios/")
                )
                self.assertEqual(
                    checks["check_bindings_android"], path.startswith("sdks/android/")
                )

    def test_shared_sdk_and_unproved_readers_select_all_languages(self):
        for path in (
            "crates/xmtp_sdk/src/lib.rs",
            "crates/xmtp_configuration/src/lib.rs",
            "apps/xmtp_sdk_bindgen/runtime/ts/index.ts",
            "bindings/shared.rs",
            "crates/xmtp_db/build.rs",
            "crates/xmtp_db/data/embedded.bin",
            "crates/new-owner/src/lib.rs",
            "sdks/node/runtime/custom.js",
            "sdks/node/package.json",
            "docs/specs/PROC-message-processing.md",
            "docs/schemas/runtime.json",
            "apps/docs/settings.yaml",
        ):
            with self.subTest(path=path):
                self.assertTrue(all(self.selected(path).values()))

    def test_active_browser_platform_owners_select_both_linux_routes_and_lint(self):
        for path in (
            "sdks/browser/test/platform/bridge.real.mts",
            "sdks/browser/test/platform/bridge.chromium.html",
            "sdks/browser/test/platform/object-store/object-store.mjs",
            "sdks/browser/test/platform/suite.worker.ts",
            "sdks/browser/test/platform/attachment-lifetime.chromium.ts",
            "sdks/browser/test/platform/storage.layout.chromium.ts",
        ):
            with self.subTest(path=path):
                checks = self.selected(path)
                for name in (
                    "test_bridge_runtime",
                    "test_browser_platform",
                    "check_sdk",
                    "sdk_node",
                    "sdk_browser",
                    "backend_products",
                ):
                    self.assertTrue(checks[name], name)
                for name in (
                    "test_swift_seams",
                    "test_ios",
                    "test_android",
                    "test_node",
                ):
                    self.assertFalse(checks[name], name)
        shared = self.selected("apps/xmtp_sdk_bindgen/runtime-tests/ts/bridge.test.ts")
        self.assertTrue(shared["test_swift_seams"])
        self.assertTrue(shared["test_bridge_runtime"])
        self.assertTrue(shared["test_browser_platform"])

    def test_swift_seam_owners_and_fork_policy(self):
        for path in (
            "sdks/ios/Sources/XmtpSdk/runtime/streams/StreamMethods.swift",
            "Package.swift",
            "sdks/ios/ios.just",
            "sdks/ios/script/check-consumer.sh",
            "sdks/ios/dev/bindings",
            "sdks/ios/Tests/Consumer/main.swift",
            "sdks/ios/Tests/XmtpSdkTests/ReaderTeardownTests.swift",
            "sdks/ios/Tests/XmtpSdkTests/RuntimeFakes.swift",
        ):
            with self.subTest(path=path):
                selected = selector.select([path], verified=True)
                self.assertTrue(selected["checks"]["test_swift_seams"])
                self.assertNotIn(
                    "test_swift_seams", selector.suites(selected, selector.TEST_SUITES)
                )
                fork = selector.select([path], verified=True, fork=True)["checks"]
                self.assertFalse(fork["test_swift_seams"])
                self.assertTrue(fork["check_bindings_ios"])
        self.assertEqual(
            selector.TEST_SUITES[-2:], ("test_bridge_runtime", "test_browser_platform")
        )
        self.assertEqual(len(selector.TEST_SUITES), 14)

    def test_android_consumer_helpers_keep_staging_route(self):
        for path in (
            "sdks/android/android.just",
            "sdks/android/dev/check-consumers",
            "sdks/android/library/build.gradle",
            "sdks/android/library/src/test/negative/ConsumerNegative.kt",
        ):
            with self.subTest(path=path):
                checks = self.selected(path)
                self.assertTrue(checks["test_sdk_staging"])
                self.assertTrue(checks["check_bindings_android"])
                self.assertFalse(checks["test_swift_seams"])

    def test_core_push_is_broad_but_prose_push_remains_narrow(self):
        core = selector.select(
            ["crates/xmtp_mls/src/lib.rs"], event="push", verified=True
        )["checks"]
        self.assertTrue(all(core.values()))
        prose = selector.select(["docs/guide.md"], event="push", verified=True)[
            "checks"
        ]
        self.assertTrue(prose["docs_site"])
        self.assertFalse(prose["test_node"])
        self.assertFalse(prose["test_workspace"])

    def test_cli_emits_exact_selected_suite_arrays_and_empty_arrays(self):
        root = Path(__file__).resolve().parents[2]
        expected_runtime = [
            "test_native_backend",
            "test_validation",
            "test_backend",
            "test_workspace",
            "test_keepalive",
            "test_wasm",
            "test_xdbg",
        ]
        for paths, sources, runtime in (
            (["crates/xmtp_mls/src/lib.rs"], ["lint_workspace"], expected_runtime),
            (["docs/guide.md"], [], []),
        ):
            with self.subTest(paths=paths), tempfile.TemporaryDirectory() as tmp:
                folder = Path(tmp)
                changed = folder / "paths.json"
                changed.write_text(json.dumps(paths))
                result = subprocess.run(
                    [
                        sys.executable,
                        "-B",
                        str(root / "dev/ci/select-checks.py"),
                        "--paths-json",
                        str(changed),
                        "--verified",
                        "--source-suites-output",
                        str(folder / "source.json"),
                        "--test-suites-output",
                        str(folder / "runtime.json"),
                    ],
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                selection = json.loads(result.stdout)
                self.assertEqual(selection["schema_version"], 1)
                self.assertEqual(set(selection), {"schema_version", "checks"})
                self.assertEqual(
                    json.loads((folder / "source.json").read_text()), sources
                )
                self.assertEqual(
                    json.loads((folder / "runtime.json").read_text()), runtime
                )


class WorkflowGateTests(unittest.TestCase):
    root = Path(__file__).resolve().parents[2]

    def test_actual_test_gate_enforces_selected_docs(self):
        source = (self.root / ".github/workflows/ci.yml").read_text()
        test_job = source.split("\n  test:\n", 1)[1]
        mapping = json.loads(re.search(r"--mapping '(\{[^']+\})'", test_job)[1])
        self.assertIn("docs_site", mapping)
        self.assertRegex(test_job, r"needs: \[[^\]]*\bdocs\b")
        selection = selector.select(["docs/guide.md"], verified=True)
        needs = {"detect-changes": {"result": "success"}}
        needs.update(
            {
                job: {"result": "success" if selection["checks"][key] else "skipped"}
                for key, job in mapping.items()
            }
        )
        for result, expected in (
            ("success", 0),
            ("failure", 1),
            ("cancelled", 1),
            ("skipped", 1),
        ):
            with self.subTest(result=result):
                needs[mapping["docs_site"]]["result"] = result
                command = [
                    sys.executable,
                    "-B",
                    str(self.root / "dev/ci/check-gate.py"),
                    "--selection",
                    json.dumps(selection),
                    "--needs",
                    json.dumps(needs),
                    "--mapping",
                    json.dumps(mapping),
                ]
                actual = subprocess.run(command, capture_output=True)
                self.assertEqual(actual.returncode, expected, actual.stderr.decode())
        selection["checks"]["docs_site"] = False
        actual = subprocess.run(
            [
                sys.executable,
                "-B",
                str(self.root / "dev/ci/check-gate.py"),
                "--selection",
                json.dumps(selection),
                "--needs",
                json.dumps(needs),
                "--mapping",
                json.dumps(mapping),
            ],
            capture_output=True,
        )
        self.assertEqual(actual.returncode, 0, actual.stderr.decode())

    def test_selected_rust_sdk_units_use_backend_without_language_products(self):
        source = (self.root / ".github/workflows/ci.yml").read_text()
        job = re.split(
            r"\n  [a-z][a-z-]*:\n",
            source.split("\n  check-sdk-unit:\n", 1)[1],
            maxsplit=1,
        )[0]
        self.assertIn("backend-products", job)
        self.assertNotIn("sdk-node", job)
        self.assertNotIn("sdk-browser", job)
        self.assertIn("check-sdk-unit.yml", job)
        workflow = (self.root / ".github/workflows/check-sdk-unit.yml").read_text()
        self.assertIn("just test crate xmtp_sdk", workflow)
        self.assertNotIn("sdk-product", workflow)
        self.assertNotIn("setup-js", workflow)
        self.assertNotIn("--release", workflow)
        test_job = source.split("\n  test:\n", 1)[1]
        mapping = json.loads(re.search(r"--mapping '(\{[^']+\})'", test_job)[1])
        self.assertEqual(mapping["check_sdk_unit"], "check-sdk-unit")
        selection = selector.select(["crates/xmtp_mls/src/lib.rs"], verified=True)
        needs = {
            "detect-changes": {"result": "success"},
            "check-sdk-unit": {"result": "failure"},
        }
        command = [
            sys.executable,
            "-B",
            str(self.root / "dev/ci/check-gate.py"),
            "--selection",
            json.dumps(selection),
            "--needs",
            json.dumps(needs),
            "--mapping",
            json.dumps({"check_sdk_unit": mapping["check_sdk_unit"]}),
        ]
        actual = subprocess.run(command, capture_output=True)
        self.assertNotEqual(actual.returncode, 0)

    def test_actual_rust_docs_gate_routes_selected_provider(self):
        source = (
            (self.root / ".github/workflows/ci.yml")
            .read_text()
            .split("\n  test:\n", 1)[1]
        )
        maps = [
            json.loads(item) for item in re.findall(r"--mapping '(\{[^']+\})'", source)
        ]
        standalone = next(
            item for item in maps if item.get("docs_rust") == "docs-rust-reference"
        )
        fullsite = next(item for item in maps if item.get("docs_rust") == "docs")
        self.assertIn("checks.docs_site", source)
        for mapping, selected_site in ((standalone, False), (fullsite, True)):
            selection = selector.select(["crates/xmtp_mls/src/lib.rs"], verified=True)
            selection["checks"]["docs_site"] = selected_site
            needs = {
                "detect-changes": {"result": "success"},
                "docs": {"result": "success" if selected_site else "skipped"},
                "docs-rust-reference": {
                    "result": "skipped" if selected_site else "success"
                },
            }
            for status, expected in (
                ("success", 0),
                ("failure", 1),
                ("cancelled", 1),
                ("skipped", 1),
            ):
                needs[mapping["docs_rust"]]["result"] = status
                actual = subprocess.run(
                    [
                        sys.executable,
                        "-B",
                        str(self.root / "dev/ci/check-gate.py"),
                        "--selection",
                        json.dumps(selection),
                        "--needs",
                        json.dumps(needs),
                        "--mapping",
                        json.dumps(mapping),
                    ],
                    capture_output=True,
                )
                self.assertEqual(actual.returncode, expected, actual.stderr.decode())

    def test_quality_gate_and_failure_watcher_follow_actual_workflow(self):
        source = (self.root / ".github/workflows/ci.yml").read_text()
        lint_job = source.split("\n  lint:\n", 1)[1].split("\n  test:\n", 1)[0]
        mapping = json.loads(re.search(r"--mapping '(\{[^']+\})'", lint_job)[1])
        self.assertEqual(mapping["docs_quality"], "docs-quality")
        self.assertRegex(lint_job, r"needs: \[[^\]]*docs-quality")
        workflow_name = re.search(r"^name: (.+)$", source, re.MULTILINE)[1].strip('"')
        listener = (
            self.root / ".github/workflows/flaky-failure-watcher.yml"
        ).read_text()
        watched = re.findall(r'^      - "([^"\n]+)"$', listener, re.MULTILINE)
        self.assertIn(workflow_name, watched)


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


class ShallowHistoryTests(unittest.TestCase):
    """Run the selector CLI against actual shallow Git repositories."""

    tool = Path(__file__).with_name("select-checks.py").resolve()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name)
        self.remote = self.folder / "remote"
        self.remote.mkdir()
        self.git(self.remote, "init", "-q", "--initial-branch=main")
        self.git(self.remote, "config", "user.name", "Fixture")
        self.git(self.remote, "config", "user.email", "fixture@example.test")
        self.git(self.remote, "config", "commit.gpgsign", "false")
        graph = self.remote / "dev/ci/select-checks.py"
        graph.parent.mkdir(parents=True)
        graph.write_bytes(self.tool.read_bytes())
        self.base = self.commit("docs/base.md", "base")
        tools = self.folder / "tools"
        tools.mkdir()
        gh = tools / "gh"
        gh.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            "sha = sys.argv[2].split('/commits/')[1].split('/')[0]\n"
            "valid = sha in json.loads(os.environ['FIXTURE_VERIFIED'])\n"
            "print(json.dumps({'check_runs': [{'id': i, 'name': n, 'app': {'slug': 'github-actions'}, 'status': 'completed', 'conclusion': 'success'} for i, n in enumerate(('Lint','Test'))] if valid else []}))\n"
        )
        gh.chmod(0o755)
        self.env = dict(
            os.environ,
            PATH=str(tools) + os.pathsep + os.environ["PATH"],
            GITHUB_REPOSITORY="fixture/repo",
        )

    def git(self, root, *args):
        return (
            subprocess.check_output(
                ["git", "-C", str(root), *args], stderr=subprocess.DEVNULL
            )
            .decode()
            .strip()
        )

    def commit(self, name, text):
        path = self.remote / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        self.git(self.remote, "add", ".")
        self.git(self.remote, "commit", "-qm", "fixture")
        return self.git(self.remote, "rev-parse", "HEAD")

    def clone(self, depth=2):
        self.checkout = self.folder / "checkout"
        subprocess.run(
            [
                "git",
                "clone",
                "--quiet",
                f"--depth={depth}",
                self.remote.as_uri(),
                str(self.checkout),
            ],
            check=True,
        )
        self.assertEqual(
            self.git(self.checkout, "rev-parse", "--is-shallow-repository"), "true"
        )
        return self.git(self.checkout, "rev-parse", "HEAD")

    def selected(self, event="push", before=None, verified=None):
        payload = {
            "before": self.base if before is None else before,
            "pull_request": {"head": {"repo": {"full_name": "fixture/repo"}}},
        }
        event_file = self.folder / "event.json"
        event_file.write_text(json.dumps(payload))
        env = dict(
            self.env,
            GITHUB_EVENT_PATH=str(event_file),
            FIXTURE_VERIFIED=json.dumps([self.base] if verified is None else verified),
        )
        head = self.git(self.checkout, "rev-parse", "HEAD")
        result = subprocess.run(
            [sys.executable, "-B", str(self.tool), "--event", event],
            cwd=self.checkout,
            env=env,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.git(self.checkout, "rev-parse", "HEAD"), head)
        return json.loads(result.stdout)["checks"], result.stderr

    def assert_prose(self, checks):
        self.assertTrue(checks["docs_site"])
        self.assertTrue(checks["sdk_node"])
        self.assertFalse(checks["test_workspace"])
        self.assertFalse(checks["test_node"])

    def test_normal_push_depth_two_preserves_before_parent(self):
        self.commit("docs/guide.md", "guide")
        self.clone()
        self.assertEqual(self.git(self.checkout, "rev-parse", "HEAD^1"), self.base)
        checks, log = self.selected()
        self.assert_prose(checks)
        self.assertNotIn("CI history fetched", log)

    def test_batched_push_fetches_exact_base_and_bounded_head_history(self):
        for i in range(6):
            self.commit("docs/guide.md", f"guide {i}")
        head = self.clone()
        checks, log = self.selected()
        self.assert_prose(checks)
        self.assertIn(f"exact {self.base} at depth 1", log)
        self.assertIn(f"exact {head} at depth 32", log)

    def synthetic_merge(self, base):
        self.git(self.remote, "checkout", "-qb", "feature", base)
        feature = self.commit("docs/feature.md", "feature")
        self.git(self.remote, "checkout", "-q", "main")
        self.git(self.remote, "merge", "--no-ff", "-qm", "synthetic merge", "feature")
        return feature

    def test_pr_merge_depth_two_preserves_both_actual_parents(self):
        feature = self.synthetic_merge(self.base)
        self.clone()
        self.assertEqual(self.git(self.checkout, "rev-parse", "HEAD^1"), self.base)
        self.assertEqual(self.git(self.checkout, "rev-parse", "HEAD^2"), feature)
        checks, log = self.selected("pull_request")
        self.assert_prose(checks)
        self.assertNotIn("CI history fetched", log)

    def test_pr_depth_one_fetches_actual_merge_parent_and_history(self):
        self.synthetic_merge(self.base)
        head = self.clone(depth=1)
        checks, log = self.selected("pull_request")
        self.assert_prose(checks)
        self.assertIn(f"exact {self.base} at depth 1", log)
        self.assertIn(f"exact {head} at depth 32", log)

    def test_advanced_or_stacked_untested_base_keeps_full_core_checks(self):
        for style in ("advanced", "stacked"):
            with self.subTest(style=style):
                if style == "stacked":
                    self.git(self.remote, "branch", "-D", "feature")
                    self.git(self.remote, "reset", "--hard", self.base)
                    self.git(self.remote, "clean", "-fdq")
                    self.git(self.remote, "checkout", "-qb", "stack-a")
                self.commit("crates/xmtp_mls/src/lib.rs", "untested core")
                if style == "stacked":
                    self.git(self.remote, "branch", "-f", "main", "HEAD")
                    self.git(self.remote, "checkout", "-q", "main")
                self.synthetic_merge(self.base)
                if hasattr(self, "checkout"):
                    shutil.rmtree(self.checkout)
                self.clone()
                checks, _ = self.selected("pull_request", verified=[self.base])
                self.assertTrue(all(checks.values()))

    def test_unfetchable_missing_before_keeps_full_selection(self):
        self.commit("docs/guide.md", "guide")
        self.clone()
        checks, log = self.selected(before="f" * 40, verified=["f" * 40])
        self.assertTrue(all(checks.values()))
        self.assertIn("fetch failed", log)

    def test_missing_origin_history_keeps_full_selection(self):
        for i in range(4):
            self.commit("docs/guide.md", str(i))
        self.clone()
        self.git(
            self.checkout,
            "remote",
            "set-url",
            "origin",
            (self.folder / "missing").as_uri(),
        )
        checks, _ = self.selected()
        self.assertTrue(all(checks.values()))

    def test_present_base_without_shallow_edges_is_not_coverage(self):
        for i in range(4):
            self.commit("docs/guide.md", str(i))
        self.clone(depth=1)
        self.git(self.checkout, "fetch", "--quiet", "--depth=1", "origin", self.base)
        self.git(self.checkout, "cat-file", "-e", f"{self.base}^{{commit}}")
        self.git(
            self.checkout,
            "remote",
            "set-url",
            "origin",
            (self.folder / "missing").as_uri(),
        )
        checks, _ = self.selected()
        self.assertTrue(all(checks.values()))

    def test_nonancestor_remote_commit_is_not_coverage(self):
        self.git(self.remote, "checkout", "--orphan", "unrelated")
        self.git(self.remote, "rm", "-rf", ".")
        graph = self.remote / "dev/ci/select-checks.py"
        graph.parent.mkdir(parents=True, exist_ok=True)
        graph.write_bytes(self.tool.read_bytes())
        unrelated = self.commit("docs/unrelated.md", "unrelated")
        self.git(self.remote, "checkout", "-q", "main")
        self.commit("docs/guide.md", "guide")
        self.clone()
        self.git(self.checkout, "fetch", "--quiet", "--depth=1", "origin", unrelated)
        checks, _ = self.selected(before=unrelated, verified=[unrelated])
        self.assertTrue(all(checks.values()))

    def test_changed_ci_graph_keeps_full_selection(self):
        self.commit("dev/ci/select-checks.py", "changed graph")
        self.clone()
        checks, _ = self.selected()
        self.assertTrue(all(checks.values()))

    def test_pr_head_without_merge_is_not_a_tested_base(self):
        self.commit("docs/guide.md", "guide")
        self.clone()
        checks, _ = self.selected("pull_request")
        self.assertTrue(all(checks.values()))


if __name__ == "__main__":
    unittest.main()
