#!/usr/bin/env python3
"""Keep the retention ledger and each unswitched source guard effective."""

import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import sys
import unittest

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]


class CutoverGates(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()

    def tearDown(self):
        self.temporary.cleanup()

    def copy(self, source):
        target = self.root / source
        target.parent.mkdir(parents=True, exist_ok=True)
        if (ROOT / source).is_dir():
            shutil.copytree(ROOT / source, target)
        else:
            shutil.copy(ROOT / source, target)

    def inventory(self):
        spec = importlib.util.spec_from_file_location("cutover_inventory", ROOT / "dev/sdk/inventory.py")
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        module.ROOT = self.root
        module.SWIFT = self.root / "sdks/ios/Sources/XMTPiOS"
        module.KOTLIN = self.root / "sdks/android/library/src/main/java/org/xmtp/android/library"
        module.TS_ROOTS = {sdk: self.root / f"sdks/{sdk.lower()}/src" for sdk in ("Node", "Browser")}
        module.OUT = self.root / "docs/self-hosted/sdk-api-manifest.md"
        module.MOBILE_TEST_MAP = self.root / "dev/sdk/binding-test-map.tsv"
        return module

    def test_switched_inventory_keeps_ledger_and_unswitched_source_checks(self):
        for source in ("docs/self-hosted/sdk-api-manifest.md",):
            self.copy(source)
        for folder, version in (("node", "6.0.0"), ("browser", "7.0.0")):
            directory = self.root / f"sdks/{folder}"
            directory.mkdir(parents=True)
            (directory / "package.json").write_text('{"version":"' + version + '","scripts":{"build":"legacy"}}')
            (directory / "src").mkdir()
            (directory / "src/index.ts").write_text("export const fixtureRoot = 1;\n")
        android = self.root / "sdks/android/library"
        android.mkdir(parents=True)
        (android / "build.gradle").write_text("legacy")
        (android.parent / "gradle.properties").write_text("version=7.0.0\n")
        (self.root / "Package.swift").write_text('name: "XmtpSdk"; path: "sdks/ios/Sources/XmtpSdk"')
        public = self.root / "sdks/ios/Sources/XmtpSdk/public.swift"
        public.parent.mkdir(parents=True)
        public.write_text("public final class SDKClient {\n public static func create() {}\n public static func build() {}\n public func end() {}\n}\npublic struct Timestamp {\n public let ns: Int64\n}\n")
        module = self.inventory()
        module.kotlin_inventory = lambda: []
        module.mobile_test_map = lambda: []
        ledger = module.OUT.read_text()
        built = module.build()
        self.assertIn("Switched SDK source inventory", built)
        self.assertIn("Swift source declarations", built)
        self.assertIn("| Swift | 7132 |", built)
        module.OUT.write_text(built)
        self.assertEqual(module.build(), built)
        public.rename(public.with_suffix(".missing"))
        with self.assertRaisesRegex(ValueError, "missing or empty current public projection"):
            module.build()
        public.with_suffix(".missing").rename(public)
        module.OUT.write_text(built.replace("`Client.create`", "`Client.omittedCreate`", 1))
        with self.assertRaisesRegex(ValueError, "pinned pre-switch ledger changed"):
            module.build()
        module.OUT.write_text(built)
        sibling = self.root / "sdks/node/src/index.ts"
        sibling.write_text(sibling.read_text() + "\nexport const unapprovedSiblingExport = true;\n")
        self.assertNotEqual(module.build(), built)
        self.assertIn(module.ledger_section(ledger, "Swift"), built)

    def test_isolation_admits_only_the_switched_sdk(self):
        for source in ("sdks/node/package.json", "sdks/browser/package.json", "sdks/android/gradle.properties", "sdks/android/library/build.gradle", "dev/sdk/switches.py", "crates/xmtp_sdk/dev/check-isolation", "crates/xmtp_sdk/dev/isolation-pins.tsv"):
            self.copy(source)
        for folder, version in (("node", "6.0.0"), ("browser", "7.0.0")):
            (self.root / f"sdks/{folder}/package.json").write_text('{"version":"' + version + '","scripts":{"build":"legacy"}}')
        (self.root / "sdks/android/gradle.properties").write_text("version=7.0.0\n")
        (self.root / "sdks/android/library/build.gradle").write_text("legacy")
        (self.root / "Package.swift").write_text('name: "XMTPiOS"')
        facade = self.root / "crates/xmtp_sdk/src/lib.rs"
        facade.parent.mkdir(parents=True)
        facade.write_text("old facade\n")
        sibling = self.root / "sdks/browser/src/guard.ts"
        sibling.parent.mkdir(parents=True)
        sibling.write_text("export const sibling = 1;\n")
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "-c", "user.email=fixture@example.test", "-c", "user.name=fixture", "commit", "-qm", "base"], cwd=self.root, check=True)
        subprocess.run(["git", "update-ref", "refs/remotes/origin/fixture", "HEAD"], cwd=self.root, check=True)
        facade.write_text("new facade\n")
        (self.root / "Package.swift").write_text('name: "XmtpSdk"; path: "sdks/ios/Sources/XmtpSdk"')
        switched = self.root / "sdks/ios/Sources/XmtpSdk/new.swift"
        switched.parent.mkdir(parents=True)
        switched.write_text("public struct Product {}\n")
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(["git", "-c", "user.email=fixture@example.test", "-c", "user.name=fixture", "commit", "-qm", "switch"], cwd=self.root, check=True)
        command = ["bash", "crates/xmtp_sdk/dev/check-isolation", "fixture"]
        result = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        sibling.write_text("export const sibling = 2;\n")
        result = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("sdks/browser/src/guard.ts", result.stderr)
        sibling.write_text("export const sibling = 1;\n")
        node_manifest = self.root / "sdks/node/package.json"
        node_manifest.write_text('{"version":"8.0.0","scripts":{"build":"bash ../../dev/js/sdk-package node"}}')
        agent_manifest = self.root / "sdks/agent/package.json"
        agent_manifest.parent.mkdir(parents=True)
        agent_manifest.write_text('{"name":"@xmtp/agent-sdk","version":"8.0.0","dependencies":{"@xmtp/node-sdk":"workspace:*"}}')
        agent = self.root / "sdks/agent/src/guard.ts"
        agent.parent.mkdir(parents=True)
        agent.write_text("export const agent = 1;\n")
        subprocess.run(["git", "add", "sdks/agent"], cwd=self.root, check=True)
        result = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        node_manifest.write_text('{"version":"6.0.0","scripts":{"build":"legacy"}}')
        result = subprocess.run(command, cwd=self.root, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("sdks/agent/src/guard.ts", result.stderr)


if __name__ == "__main__":
    unittest.main()
