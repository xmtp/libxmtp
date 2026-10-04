#!/usr/bin/env python3
"""Keep the retention ledger and each unswitched source guard effective."""

import importlib.util
import os
from unittest.mock import patch
from pathlib import Path
import shutil
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
        spec = importlib.util.spec_from_file_location(
            "cutover_inventory", ROOT / "dev/sdk/inventory.py"
        )
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        module.ROOT = self.root
        module.SWIFT = self.root / "sdks/ios/Sources/XMTPiOS"
        module.KOTLIN = (
            self.root / "sdks/android/library/src/main/java/org/xmtp/android/library"
        )
        module.TS_ROOTS = {
            sdk: self.root / f"sdks/{sdk.lower()}/src" for sdk in ("Node", "Browser")
        }
        module.OUT = self.root / "docs/self-hosted/sdk-api-manifest.md"
        module.MOBILE_TEST_MAP = self.root / "dev/sdk/binding-test-map.tsv"
        return module

    def test_switched_inventory_keeps_ledger_and_unswitched_source_checks(self):
        for source in ("docs/self-hosted/sdk-api-manifest.md",):
            self.copy(source)
        for folder, version in (("node", "6.0.0"), ("browser", "7.0.0")):
            directory = self.root / f"sdks/{folder}"
            directory.mkdir(parents=True)
            (directory / "package.json").write_text(
                '{"version":"' + version + '","scripts":{"build":"legacy"}}'
            )
            (directory / "src").mkdir()
            (directory / "src/index.ts").write_text("export const fixtureRoot = 1;\n")
        android = self.root / "sdks/android/library"
        android.mkdir(parents=True)
        (android / "build.gradle").write_text("legacy")
        (android.parent / "gradle.properties").write_text("version=7.0.0\n")
        (self.root / "Package.swift").write_text(
            'name: "XmtpSdk"; path: "sdks/ios/Sources/XmtpSdk"'
        )
        public = self.root / "sdks/ios/Sources/XmtpSdk/public.swift"
        public.parent.mkdir(parents=True)
        public.write_text(
            "public final class SDKClient {\n public static func create() {}\n public static func build() {}\n public func end() {}\n}\npublic struct Timestamp {\n public let ns: Int64\n}\n"
        )
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
        source_only = module.build(source_only=True)
        self.assertEqual(module.source_only_manifest(built), source_only)
        public.rename(public.with_suffix(".missing"))
        self.assertEqual(module.build(source_only=True), source_only)
        with self.assertRaisesRegex(
            ValueError, "missing or empty current public projection"
        ):
            module.build()
        public.with_suffix(".missing").rename(public)
        module.OUT.write_text(
            built.replace("`Client.create`", "`Client.omittedCreate`", 1)
        )
        with self.assertRaisesRegex(ValueError, "pinned pre-switch ledger changed"):
            module.build()
        with self.assertRaisesRegex(ValueError, "pinned pre-switch ledger changed"):
            module.build(source_only=True)
        module.OUT.write_text(built)
        sibling = self.root / "sdks/node/src/index.ts"
        sibling.write_text(
            sibling.read_text() + "\nexport const unapprovedSiblingExport = true;\n"
        )
        self.assertNotEqual(module.build(), built)
        self.assertNotEqual(module.build(source_only=True), source_only)
        self.assertIn(module.ledger_section(ledger, "Swift"), built)

    def test_browser_inventory_requires_own_main_and_pure_roots(self):
        module = self.inventory()
        generated = self.root / "own-generated"
        worker = generated / "typescript-wasm/index.ts"
        pure = generated / "typescript-pure/index.ts"
        worker.parent.mkdir(parents=True)
        pure.parent.mkdir(parents=True)
        worker.write_text("export { Client, Message, Timestamp };\n")
        pure.write_text("export { Timestamp, generateInboxId, initPureWasm };\n")
        with patch.dict(os.environ, {"XMTP_SDK_GENERATED_DIR": str(generated)}):
            rows = module.switched_source_rows({"Browser"})
            self.assertEqual(len(rows), 2)
            self.assertIn("/pure root export names | 3 |", rows[1])
            pure.unlink()
            with self.assertRaisesRegex(ValueError, "missing current pure projection"):
                module.switched_source_rows({"Browser"})
            pure.write_text("export { Timestamp };\n")
            with self.assertRaisesRegex(
                ValueError, "pure root misses retained exports"
            ):
                module.switched_source_rows({"Browser"})

    def test_switch_detection_keeps_each_sdk_independent(self):
        for source in (
            "sdks/node/package.json",
            "sdks/browser/package.json",
            "sdks/android/gradle.properties",
            "sdks/android/library/build.gradle",
        ):
            self.copy(source)
        for folder in ("node", "browser"):
            (self.root / f"sdks/{folder}/package.json").write_text(
                '{"version":"8.0.0","scripts":{"build":"legacy"}}'
            )
        (self.root / "sdks/android/gradle.properties").write_text("version=7.0.0\n")
        (self.root / "sdks/android/library/build.gradle").write_text("legacy")
        (self.root / "Package.swift").write_text('name: "XMTPiOS"')
        module = self.inventory()
        self.assertEqual(module.switched_sdks(self.root), set())
        (self.root / "Package.swift").write_text(
            'name: "XmtpSdk"; path: "sdks/ios/Sources/XmtpSdk"'
        )
        self.assertEqual(module.switched_sdks(self.root), {"Swift"})
        (self.root / "sdks/node/package.json").write_text(
            '{"version":"8.0.0","scripts":{"build":"bash ../../dev/js/sdk-package node"}}'
        )
        self.assertEqual(module.switched_sdks(self.root), {"Swift", "Node"})
        (self.root / "sdks/android/gradle.properties").write_text("version=8.0.0\n")
        self.assertEqual(module.switched_sdks(self.root), {"Swift", "Node"})
        (self.root / "sdks/android/library/build.gradle").write_text(
            "XMTP_SDK_GENERATED_DIR"
        )
        self.assertEqual(module.switched_sdks(self.root), {"Swift", "Node", "Kotlin"})
        (self.root / "sdks/browser/package.json").write_text(
            '{"version":"8.0.0","scripts":{"build":"bash ../../dev/js/sdk-package node"}}'
        )
        self.assertEqual(module.switched_sdks(self.root), {"Swift", "Node", "Kotlin"})
        (self.root / "sdks/browser/package.json").write_text(
            '{"version":"8.0.0","scripts":{"build":"bash ../../dev/js/sdk-package browser"}}'
        )
        self.assertEqual(
            module.switched_sdks(self.root), {"Swift", "Node", "Kotlin", "Browser"}
        )


if __name__ == "__main__":
    unittest.main()
