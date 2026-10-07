"""Check current DocC inputs and checkout isolation without native compilation."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "docs_package", Path(__file__).with_name("docs-package.py")
)
docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docs)


class DocsPackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name).resolve()
        self.root = self.base / "checkout"
        self.root.mkdir()
        self.write(self.root, "Package.swift", "package exact language flags\n")
        self.lock = {
            "version": 3,
            "originHash": "root-origin",
            "pins": [{"identity": "docc", "version": "1.5.0"}],
        }
        self.write(self.root, "Package.resolved", json.dumps(self.lock))
        self.write(
            self.root,
            "sdks/ios/Sources/XmtpSdk/AppleLogSink.swift",
            "handwritten log sink\n",
        )
        self.write(
            self.root,
            "sdks/ios/Sources/XmtpSdk/runtime/StaleRemoved.swift",
            "public struct RemovedPublicDeclaration {}\n",
        )
        self.write(
            self.root,
            "sdks/ios/Sources/XmtpSdk/xmtp_sdk.swift",
            "old generated declaration\n",
        )
        self.write(
            self.root, "sdks/ios/Tests/XmtpSdkTests/Test.swift", "handwritten test\n"
        )
        self.product = self.base / "nix-product"
        self.generated = self.product / "swift"
        self.write(
            self.generated,
            "xmtp_sdk.swift",
            "public struct CurrentPublicDeclaration {}\n",
        )
        self.write(self.generated, "xmtp_sdkFFI.h", "current header")
        self.write(self.generated, "xmtp_sdkFFI.modulemap", "current module map")
        self.write(self.generated, "include/xmtp_sdkFFI.h", "current header")
        self.write(self.generated, "include/module.modulemap", "current module map")
        self.write(
            self.generated, "runtime/Generated.swift", "current generated runtime\n"
        )
        self.write(
            self.product, "XmtpSdkFFI.xcframework/Info.plist", "current framework plist"
        )
        self.write(
            self.product,
            "XmtpSdkFFI.xcframework/macos-arm64/libxmtp_sdk.a",
            "current framework library",
        )
        self.native = self.base / "libxmtp_sdk.dylib"
        self.native.write_text("current native metadata library")
        self.record = {
            "contract": "current-contract",
            "generator": "current-generator",
            "artifact": {
                "source": "current-source",
                "generator": "current-generator",
                "profile": "release",
                "features": "",
                "target": "",
                "files": {str(self.native): docs.digest(self.native)},
            },
            "files": {
                name: digest
                for name, digest in docs.inventory(self.generated).items()
                if not name.startswith("include/")
            },
        }
        self.save_record()
        self.output = self.base / "output/reference/swift"
        self.receipt = self.base / "inputs.json"
        self.tools = self.base / "bin"
        self.tools.mkdir()
        self.swift = self.tools / "swift"
        self.swift.write_text(
            f"#!{sys.executable}\n"
            + r"""
from pathlib import Path
import json, os, sys
view=Path.cwd(); root=Path(os.environ['DOCS_ROOT'])
assert view != root and not view.is_relative_to(root)
assert (view/'Package.swift').read_text() == 'package exact language flags\n'
assert (view/'sdks/ios/Sources/XmtpSdk/AppleLogSink.swift').read_text() == 'handwritten log sink\n'
assert not (view/'sdks/ios/Sources/XmtpSdk/runtime/StaleRemoved.swift').exists()
assert (view/'sdks/ios/Sources/XmtpSdk/runtime/Generated.swift').read_text() == 'current generated runtime\n'
assert (view/'sdks/ios/Tests/XmtpSdkTests/Test.swift').read_text() == 'handwritten test\n'
assert 'CurrentPublicDeclaration' in (view/'sdks/ios/Sources/XmtpSdk/xmtp_sdk.swift').read_text()
assert (view/'sdks/ios/Artifacts/XmtpSdkFFI.xcframework/Info.plist').is_file()
args=sys.argv[1:]; out=Path(args[args.index('--output-path')+1])
assert out.is_absolute()
assert args[args.index('--target')+1] == 'XmtpSdk'
assert args[args.index('--hosting-base-path')+1] == 'reference/swift'
lock=json.loads((view/'Package.resolved').read_text());lock['originHash']='view-origin'
if os.environ.get('PIN_DRIFT') == '1':lock['pins'][0]['version']='unexpected-upgrade'
(view/'Package.resolved').write_text(json.dumps(lock))
(view/'.build').mkdir()
Path(os.environ['VIEW_PATH']).write_text(str(view))
if os.environ.get('ROOT_DRIFT') == '1':
    (root/'sdks/ios/Sources/XmtpSdk/AppleLogSink.swift').write_text('unexpected source change')
if os.environ.get('SWIFT_FAIL') == '1':
    raise SystemExit(47)
out.mkdir(parents=True)
(out/'index.html').write_text('baseUrl = "/reference/swift/"; CurrentPublicDeclaration')
(out/'documentation/xmtpsdk').mkdir(parents=True)
(out/'documentation/xmtpsdk/index.html').write_text('CurrentPublicDeclaration')
"""
        )
        self.swift.chmod(0o755)
        self.environment = patch.dict(
            os.environ,
            {
                "PATH": f"{self.tools}:{os.environ['PATH']}",
                "DOCS_ROOT": str(self.root),
                "VIEW_PATH": str(self.base / "view-path"),
            },
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.identity = patch.object(
            docs,
            "source_identity",
            return_value=("current-source", "current-generator"),
        )
        self.identity.start()
        self.addCleanup(self.identity.stop)

    def write(self, root, name, content):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def save_record(self):
        (self.generated / "sdk-contract.json").write_text(json.dumps(self.record))

    def test_current_package_view_preserves_checkout_and_public_doc_path(self):
        before = docs.checkout_inputs(self.root)
        docs.generate(self.root, self.product, self.output, self.receipt)
        self.assertEqual(before, docs.checkout_inputs(self.root))
        self.assertEqual(
            json.loads((self.root / "Package.resolved").read_text()), self.lock
        )
        self.assertIn(
            "CurrentPublicDeclaration",
            (self.output / "documentation/xmtpsdk/index.html").read_text(),
        )
        inputs = json.loads(self.receipt.read_text())
        self.assertEqual(inputs["profile"], "release")
        self.assertEqual(inputs["contract"], "current-contract")
        self.assertNotEqual(
            inputs["checkoutInputs"]["Package.resolved"],
            inputs["resolvedViewInputs"]["Package.resolved"],
        )
        self.assertFalse(Path((self.base / "view-path").read_text()).exists())

    def test_stale_source_or_profile_cannot_run_docc(self):
        for key, value in (("source", "stale"), ("profile", "debug")):
            original = self.record["artifact"][key]
            self.record["artifact"][key] = value
            self.save_record()
            with self.subTest(key=key), self.assertRaises(ValueError):
                docs.generate(self.root, self.product, self.output, self.receipt)
            self.record["artifact"][key] = original
        self.assertFalse((self.base / "view-path").exists())

    def test_current_renderer_can_preserve_native_compile_generator(self):
        self.record["artifact"]["generator"] = "earlier-native-generator"
        self.save_record()
        docs.generate(self.root, self.product, self.output, self.receipt)
        inputs = json.loads(self.receipt.read_text())
        self.assertEqual(inputs["generator"], "current-generator")
        self.assertEqual(inputs["nativeGenerator"], "earlier-native-generator")

    def test_stale_render_generator_cannot_run_docc(self):
        self.record["generator"] = "earlier-render-generator"
        self.save_record()
        with self.assertRaisesRegex(ValueError, "current source"):
            docs.generate(self.root, self.product, self.output, self.receipt)
        self.assertFalse((self.base / "view-path").exists())

    def test_removed_runtime_declaration_is_absent_only_from_current_view(self):
        before = docs.checkout_inputs(self.root)
        docs.generate(self.root, self.product, self.output, self.receipt)
        inputs = json.loads(self.receipt.read_text())
        stale = "sdks/ios/Sources/XmtpSdk/runtime/StaleRemoved.swift"
        self.assertIn(stale, inputs["checkoutInputs"])
        self.assertNotIn(stale, inputs["resolvedViewInputs"])
        self.assertEqual(before, docs.checkout_inputs(self.root))
        prefix = "sdks/ios/Sources/XmtpSdk/"
        runtime = {
            name.removeprefix(prefix): checksum
            for name, checksum in inputs["resolvedViewInputs"].items()
            if name.startswith(prefix + "runtime/")
        }
        self.assertEqual(
            runtime,
            {
                name: checksum
                for name, checksum in self.record["files"].items()
                if name.startswith("runtime/")
            },
        )

    def test_changed_generated_bytes_or_extra_header_cannot_run_docc(self):
        path = self.generated / "xmtp_sdk.swift"
        path.write_text("stale declaration")
        with self.assertRaisesRegex(ValueError, "receipt"):
            docs.prepare_view(self.root, self.product, self.base)
        path.write_text("public struct CurrentPublicDeclaration {}\n")
        self.write(self.generated, "include/unexpected.h", "extra header")
        with self.assertRaisesRegex(ValueError, "wrapper header"):
            docs.prepare_view(self.root, self.product, self.base)

    def test_root_input_drift_during_docc_is_rejected(self):
        with patch.dict(os.environ, {"ROOT_DRIFT": "1"}):
            with self.assertRaisesRegex(ValueError, "changed checkout inputs"):
                docs.generate(self.root, self.product, self.output, self.receipt)
        self.assertFalse(self.receipt.exists())

    def test_root_input_drift_before_docc_is_rejected(self):
        original = docs.prepare_view

        def drifting_view(root, product, view):
            result = original(root, product, view)
            (root / "Package.swift").write_text("unexpected input change")
            return result

        with patch.object(docs, "prepare_view", side_effect=drifting_view):
            with self.assertRaisesRegex(ValueError, "during preparation"):
                docs.generate(self.root, self.product, self.output, self.receipt)
        self.assertFalse((self.base / "view-path").exists())

    def test_docc_failure_is_preserved_and_view_removed(self):
        with patch.dict(os.environ, {"SWIFT_FAIL": "1"}):
            with self.assertRaises(subprocess.CalledProcessError):
                docs.generate(self.root, self.product, self.output, self.receipt)
        self.assertFalse(self.receipt.exists())
        self.assertFalse(Path((self.base / "view-path").read_text()).exists())

    def test_view_dependency_pin_change_is_rejected(self):
        with patch.dict(os.environ, {"PIN_DRIFT": "1"}):
            with self.assertRaisesRegex(ValueError, "dependency pins"):
                docs.generate(self.root, self.product, self.output, self.receipt)
        self.assertFalse(self.receipt.exists())

    def test_legacy_bindings_route_changes_tracked_checkout_inputs(self):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        source = Path(__file__).resolve().parents[1] / "dev/bindings"
        destination = self.root / "sdks/ios/dev/bindings"
        destination.parent.mkdir(parents=True)
        destination.write_bytes(source.read_bytes())
        builder = self.tools / "nix"
        builder.write_text(
            f"#!{sys.executable}\n"
            + "from pathlib import Path\nimport os, sys\n"
            + "args=sys.argv[1:]; output=Path(args[args.index('--out-link')+1])\n"
            + "output.parent.mkdir(parents=True,exist_ok=True)\n"
            + "output.symlink_to(os.environ['DOCS_PRODUCT'])\n"
        )
        builder.chmod(0o755)
        before = docs.checkout_inputs(self.root)
        with patch.dict(os.environ, {"DOCS_PRODUCT": str(self.product)}):
            subprocess.run(
                ["bash", str(destination)],
                cwd=self.root,
                check=True,
                stdout=subprocess.DEVNULL,
            )
        after = docs.checkout_inputs(self.root)
        self.assertNotEqual(before, after)
        self.assertNotEqual(
            before["sdks/ios/Sources/XmtpSdk/xmtp_sdk.swift"],
            after["sdks/ios/Sources/XmtpSdk/xmtp_sdk.swift"],
        )
        self.assertFalse(
            (self.root / "sdks/ios/Sources/XmtpSdk/runtime/StaleRemoved.swift").exists()
        )


if __name__ == "__main__":
    unittest.main()
