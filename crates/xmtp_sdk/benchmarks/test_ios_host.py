"""Command doubles prove iOS transport rules, not installed SDK behavior."""

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HOSTS = Path(__file__).parent / "hosts"
sys.path.insert(0, str(HOSTS))
import driver
import ios
from fixtures import canonical, digest
from ios_identity import tree_identity


spec = importlib.util.spec_from_file_location("ios_prepare", HOSTS / "ios/prepare.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class IOSHostTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.app = self.root / "XmtpBenchmark.app"
        self.app.mkdir()
        (self.app / "binary").write_bytes(b"command-double-not-a-real-SDK")
        self.container = self.root / "container"
        self.container.mkdir()
        self.state = self.root / "state"
        self.state.mkdir()
        fixture = {"messages": []}
        (self.state / "fixture.json").write_bytes(canonical(fixture))
        self.receipt = {
            "side": "new",
            "package_sha256": "package",
            "app_build_id": "build",
            "simulator_udid": "explicit-udid",
            "bundle_id": "org.xmtp.benchmark.new",
            "app_path": str(self.app),
            "app_sha256": tree_identity(self.app),
        }
        receipt = self.root / "receipt.json"
        receipt.write_text(json.dumps(self.receipt))
        self.config = {
            "timeout_seconds": 1,
            "simulator_udid": "explicit-udid",
            "build_receipt": str(receipt),
            "backend_url": "http://localhost:1",
            "signer_url": "http://localhost:2",
        }
        self.request = {
            "target": "swift",
            "phase": "setup",
            "side": "new",
            "state_directory": str(self.state),
            "package_sha256": "package",
            "fixture_sha256": digest(fixture),
        }
        self.active = False
        self.mode = "ok"
        self.envelopes = []

    def call(self, argv, **kwargs):
        action = argv[2]
        result = ""
        if action == "terminate":
            self.active = False
        elif action == "get_app_container":
            result = str(self.app if argv[-1] == "app" else self.container)
        elif action == "launch":
            self.active = True
            source = self.container / argv[-1]
            envelope = json.loads(source.read_text())
            self.envelopes.append(envelope)
            response = {
                key: envelope[key]
                for key in (
                    "operation_id",
                    "request_sha256",
                    "side",
                    "package_sha256",
                    "app_build_id",
                )
            }
            response["result"] = {
                "ready": True,
                "peak_memory_bytes": 123456,
                "memory_scope": ios.MEMORY_SCOPE,
            }
            if self.mode == "timeout":
                return subprocess.CompletedProcess(argv, 0, "", "")
            if self.mode in response:
                response[self.mode] = "stale-or-wrong"
            elif self.mode == "error":
                response["error"] = {"message": "app failure"}
            elif self.mode == "missing-memory":
                del response["result"]["peak_memory_bytes"]
            (source.parent / "response.json").write_text(
                "{" if self.mode == "malformed" else json.dumps(response)
            )
        return subprocess.CompletedProcess(argv, 0, result, "")

    def test_original_request_and_stable_local_state(self):
        original = copy.deepcopy(self.request)
        for _ in range(2):
            self.assertTrue(ios.invoke(self.config, self.request, self.call)["ready"])
            self.assertFalse(self.active)
        a, b = self.envelopes
        self.assertEqual(json.loads(a["request_json"]), original)
        self.assertEqual(a["request_sha256"], digest(original))
        self.assertEqual(self.request, original)
        self.assertEqual(a["state_key"], b["state_key"])
        self.assertNotEqual(a["operation_id"], b["operation_id"])
        staged = (
            self.container
            / "Library/Application Support/xmtp-benchmark"
            / a["state_key"]
            / "fixture.json"
        )
        self.assertTrue(staged.exists())

    def test_reject_stale_or_wrong_response_and_terminate(self):
        for mode in (
            "operation_id",
            "request_sha256",
            "side",
            "package_sha256",
            "app_build_id",
            "error",
            "malformed",
            "missing-memory",
        ):
            with self.subTest(mode=mode):
                self.mode = mode
                with self.assertRaises((ValueError, RuntimeError)):
                    ios.invoke(self.config, self.request, self.call)
                self.assertFalse(self.active)
        self.mode = "ok"
        self.assertTrue(ios.invoke(self.config, self.request, self.call)["ready"])

    def test_timeout_terminates_and_next_operation_works(self):
        self.mode = "timeout"
        self.config["timeout_seconds"] = 0.03
        with self.assertRaises(TimeoutError):
            ios.invoke(self.config, self.request, self.call)
        self.assertFalse(self.active)
        self.mode = "ok"
        self.config["timeout_seconds"] = 1
        self.assertTrue(ios.invoke(self.config, self.request, self.call)["ready"])
        self.assertNotEqual(
            self.envelopes[0]["operation_id"], self.envelopes[1]["operation_id"]
        )

    def test_app_bytes_and_package_are_checked_before_launch(self):
        self.request["package_sha256"] = "another-package"
        with self.assertRaisesRegex(ValueError, "package_sha256"):
            ios.invoke(self.config, self.request, self.call)
        self.request["package_sha256"] = "package"
        (self.app / "binary").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "app bytes"):
            ios.invoke(self.config, self.request, self.call)
        self.assertEqual(self.envelopes, [])

    def test_driver_uses_app_memory_even_when_launcher_is_larger(self):
        response = {"peak_memory_bytes": 123456, "memory_scope": ios.MEMORY_SCOPE}
        with patch.object(
            driver.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0, json.dumps(response), ""),
        ):
            actual = driver.command(
                ["double"], {"target": "swift", "phase": "measure"}, self.root / "log"
            )
        self.assertEqual(actual["peak_memory_bytes"], 123456)

    def test_driver_rejects_missing_invalid_or_wrong_memory(self):
        for response in (
            {},
            {"peak_memory_bytes": 0},
            {"peak_memory_bytes": True},
            {"peak_memory_bytes": 123456, "memory_scope": "launcher-rss"},
        ):
            with (
                self.subTest(response=response),
                patch.object(
                    driver.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess(
                        [], 0, json.dumps(response), ""
                    ),
                ),
            ):
                with self.assertRaises(ValueError):
                    driver.command(
                        ["double"],
                        {"target": "swift", "phase": "measure"},
                        self.root / "log",
                    )

    def test_cleanup_failure_rejects_a_valid_result(self):
        terminations = 0

        def failing_cleanup(argv, **kwargs):
            nonlocal terminations
            if argv[2] == "terminate":
                terminations += 1
                if terminations == 2:
                    return subprocess.CompletedProcess(
                        argv, 42, "", "termination failed"
                    )
            return self.call(argv, **kwargs)

        with self.assertRaisesRegex(RuntimeError, "simctl failed"):
            ios.invoke(self.config, self.request, failing_cleanup)
        self.assertTrue(self.active)
        self.assertEqual(
            len(list(self.state.glob("ios-operations/*/commands.json"))), 1
        )

    def test_prepared_source_drift_fails_before_compilation(self):
        package = self.root / "source-double"
        package.mkdir()
        (package / "Package.swift").write_text("fixture package manifest")
        (package / "native.a").write_text("fixture native bytes")
        output = self.root / "prepared"
        prepare.prepare(
            {
                "side": "new",
                "package_root": str(package),
                "assets": {"public": ["Package.swift"], "native": ["native.a"]},
            },
            output,
        )
        (output / "SwiftSupport.swift").write_text("changed source")
        with patch.object(
            prepare.subprocess,
            "run",
            side_effect=AssertionError("Compiler ran after source drift"),
        ) as compiler:
            with self.assertRaisesRegex(ValueError, "Prepared app source changed"):
                prepare.build(output, "explicit-udid", self.root / "derived")
            compiler.assert_not_called()

    def test_frozen_dependencies_reject_changed_pins_and_bytes(self):
        installed = self.root / "package"
        frozen = installed / "dependencies"
        resolved = self.root / "resolved"
        for base in [frozen, resolved]:
            (base / "checkouts/a").mkdir(parents=True)
            (base / "checkouts/a/source.swift").write_text("original source")
            (base / "Package.resolved").write_text(
                json.dumps({"pins": [{"revision": "exact"}]})
            )
        receipt = {"package_root": str(installed), "dependency_root": "dependencies"}
        lock = resolved / "Package.resolved"
        self.assertEqual(
            prepare.verify_dependencies(receipt, resolved, lock),
            [{"revision": "exact"}],
        )
        (resolved / "checkouts/a/source.swift").write_text("changed source")
        with self.assertRaisesRegex(ValueError, "dependency bytes"):
            prepare.verify_dependencies(receipt, resolved, lock)
        (resolved / "checkouts/a/source.swift").write_text("original source")
        lock.write_text(json.dumps({"pins": [{"revision": "different"}]}))
        with self.assertRaisesRegex(ValueError, "dependency pins"):
            prepare.verify_dependencies(receipt, resolved, lock)
        with self.assertRaisesRegex(ValueError, "Freeze resolved"):
            prepare.verify_dependencies(
                {**receipt, "dependency_root": None}, resolved, lock
            )


if __name__ == "__main__":
    unittest.main()
