"""Exercise real component admission and npm tarball assembly with small bytes.

These fixtures prove packaging/provenance guards, not native code execution.
"""

import importlib.util
from io import BytesIO
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import tarfile
import unittest

spec = importlib.util.spec_from_file_location(
    "rn_package", Path(__file__).with_name("package.py")
)
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="rn-package-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.identity = {
            "source": "sdk-source",
            "generator": "render-source",
            "profile": "release",
            "features": "",
            "forkRevision": package.artifacts.fork_revision(),
            "renderContract": "render-assets",
            "runtimeContract": "runtime-assets",
            "javascriptToolchain": "typescript-7.0.2-es2020-esnext-define-v1",
            "productContract": "js-and-native-adapters",
        }
        self.js = self.root / "js"
        for name, content in {
            "src/specs/NativeXmtpReactNative.ts": "// fixture private native spec\n",
            "LICENSE": (package.ROOT / "LICENSE").read_text(),
            "dist/entry.js": "export const value = 1;\n",
            "dist/entry.d.ts": "export declare const value: number;\n",
            "dist/runtime/core/package.json": '{"name":"@ubjs/core","version":"0.0.0","module":"index.js","types":"index.d.ts"}',
            "THIRD_PARTY_LICENSES/ubrn-LICENSE": "fixture upstream license\n",
            "dist/runtime/core/index.js": "export {};\n",
            "dist/runtime/core/index.d.ts": "export {};\n",
            "cpp/generated/xmtp_sdk_ffi.hpp": "// matched ABI declarations\n",
            "cpp/generated/xmtp_sdk.cpp": "// generated source\n",
            "cpp/generated/xmtp_sdk.hpp": "// generated header\n",
            "cpp/runtime/Runtime.h": "// pinned runtime\n",
            "react-native.config.cjs": "module.exports = {};\n",
            "XmtpReactNative.podspec": "# fixture\n",
            "package.json": json.dumps(
                {
                    "name": package.NAME,
                    "version": "8.0.0",
                    "codegenConfig": {
                        "name": "XmtpReactNativeSpec",
                        "type": "modules",
                        "jsSrcsDir": "src/specs",
                        "android": {"javaPackageName": "org.xmtp.reactnative"},
                        "ios": {
                            "modulesProvider": {"XmtpReactNative": "XmtpReactNative"}
                        },
                    },
                    "exports": {
                        ".": {"types": "./dist/entry.d.ts", "import": "./dist/entry.js"}
                    },
                }
            ),
        }.items():
            path = self.js / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        for name in [
            "cpp/XmtpNative.cpp",
            "cpp/XmtpNative.h",
            "cpp/XmtpRuntime.cpp",
            "cpp/XmtpRuntime.h",
            "ios/XmtpReactNative.h",
            "ios/XmtpReactNative.mm",
            "android/build.gradle",
            "android/CMakeLists.txt",
            "android/XmtpJni.cpp",
            "android/src/main/AndroidManifest.xml",
            "android/src/main/java/org/xmtp/reactnative/XmtpReactNativeModule.java",
            "android/src/main/java/org/xmtp/reactnative/XmtpReactNativePackage.java",
        ]:
            path = self.js / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture adapter source\n")
        self.identity["productContract"] = hashlib.sha256(
            json.dumps(package.inventory(self.js), separators=(",", ":")).encode()
        ).hexdigest()
        package.record(self.js, "react-native-js", self.identity)
        self.components = {}
        for platform, mapping in (("android", package.ANDROID), ("ios", package.IOS)):
            libraries = self.root / f"{platform}-libraries"
            toolchains = {
                target: {
                    "build": "fixture-compiler",
                    "rust": "fixture-rust",
                    "apple": "fixture-apple" if platform == "ios" else None,
                }
                for target in mapping.values()
            }
            toolchain_file = self.root / f"{platform}-toolchains.json"
            toolchain_file.write_text(json.dumps(toolchains))
            for name, target in mapping.items():
                library = libraries / name
                (library / "lib").mkdir(parents=True)
                (
                    library
                    / "lib"
                    / ("libxmtp_sdk.so" if platform == "android" else "libxmtp_sdk.a")
                ).write_bytes(f"native bytes for {target}".encode())
                (library / "native-provenance.json").write_text(
                    json.dumps(
                        {
                            "schema": 1,
                            "source": self.identity["source"],
                            "target": target,
                            "profile": "release",
                            "features": "",
                            "toolchain": toolchains[target],
                        }
                    )
                )
            output = self.root / platform
            package.component(
                SimpleNamespace(
                    platform=platform,
                    js=self.js,
                    libraries=libraries,
                    out=output,
                    fast=False,
                    toolchain_identity=toolchain_file,
                )
            )
            self.components[platform] = output

    def assemble(self, version=None):
        output = self.root / "assembled"
        package.assemble(
            SimpleNamespace(
                js=self.js,
                android=self.components["android"],
                ios=self.components["ios"],
                out=output,
                version=version,
            )
        )
        return output

    def test_real_tarball_contains_every_target_and_release_version(self):
        output = self.assemble("8.0.1-rc.1")
        receipt = json.loads((output / "sdk-contract.json").read_text())
        tarball = output / receipt["tarball"]
        self.assertEqual(receipt["sha256"], package.digest(tarball))
        self.assertEqual(
            package.digest(output / "package/LICENSE"),
            package.digest(package.ROOT / "LICENSE"),
        )
        verified = package.verify_package(tarball, "8.0.1-rc.1", "sdk-source")
        for platform, mapping in (("android", package.ANDROID), ("ios", package.IOS)):
            expected = (
                {
                    "aarch64-linux-android",
                    "armv7-linux-androideabi",
                    "x86_64-linux-android",
                    "i686-linux-android",
                }
                if platform == "android"
                else {"aarch64-apple-ios", "aarch64-apple-ios-sim"}
            )
            self.assertEqual(set(verified["components"][platform]["native"]), expected)
        with self.assertRaisesRegex(ValueError, "differs from"):
            package.verify_package(tarball, "8.0.0")
        with self.assertRaisesRegex(ValueError, "source identity mismatch"):
            package.verify_package(tarball, source="other-release")
        # A second spelling must not overwrite the same admitted asset after
        # extraction, even when the alias carries identical bytes.
        alias = self.root / "aliased-package.tgz"
        with (
            tarfile.open(tarball, "r:gz") as source,
            tarfile.open(alias, "w:gz") as archive,
        ):
            content = source.extractfile("package/dist/entry.js").read()
            for member in source.getmembers():
                archive.addfile(
                    member, source.extractfile(member) if member.isfile() else None
                )
            member = tarfile.TarInfo("package/./dist/entry.js")
            member.size = len(content)
            archive.addfile(member, BytesIO(content))
        with self.assertRaisesRegex(ValueError, "unsafe RN tarball member"):
            package.verify_package(alias, "8.0.1-rc.1", "sdk-source")
        with package.unpack(tarball) as clean:
            (clean / "src/specs/NativeXmtpReactNative.ts").unlink()
            contract_path = clean / "sdk-contract.json"
            contract = json.loads(contract_path.read_text())
            contract["assets"].pop("src/specs/NativeXmtpReactNative.ts")
            contract_path.write_text(json.dumps(contract))
            with self.assertRaisesRegex(
                ValueError, "missing required asset: src/specs/NativeXmtpReactNative.ts"
            ):
                package.verify_package(clean, "8.0.1-rc.1", "sdk-source")

        # A self-consistent file inventory cannot waive the required XMTP notice.
        (output / "package/LICENSE").unlink()
        contract_path = output / "package/sdk-contract.json"
        contract = json.loads(contract_path.read_text())
        contract["assets"].pop("LICENSE")
        contract_path.write_text(json.dumps(contract))
        with self.assertRaisesRegex(ValueError, "missing required asset: LICENSE"):
            package.verify_package(output, "8.0.1-rc.1", "sdk-source")

    def test_corrupt_runtime_bytes_are_rejected(self):
        output = self.assemble()
        (output / "package/cpp/runtime/Runtime.h").write_text("wrong fork runtime\n")
        with self.assertRaisesRegex(
            ValueError, "asset mismatch.*cpp/runtime/Runtime.h"
        ):
            package.verify_package(output)

    def test_complete_receipt_cannot_hide_missing_abi(self):
        android = self.components["android"]
        missing = package.binary_path("android", "x86")
        (android / missing).unlink()
        receipt = json.loads((android / "sdk-contract.json").read_text())
        receipt["assets"].pop(missing.as_posix())
        (android / "sdk-contract.json").write_text(json.dumps(receipt))
        with self.assertRaisesRegex(ValueError, "unexpected native files"):
            self.assemble()
        self.assertFalse((self.root / "assembled").exists())

    def test_wrong_native_toolchain_is_rejected_before_staging(self):
        library = self.root / "android-libraries/arm64-v8a/native-provenance.json"
        receipt = json.loads(library.read_text())
        receipt["toolchain"]["rust"] = "other-rust-toolchain"
        library.write_text(json.dumps(receipt))
        output = self.root / "wrong-toolchain"
        with self.assertRaisesRegex(ValueError, "native toolchain mismatch"):
            package.component(
                SimpleNamespace(
                    platform="android",
                    js=self.js,
                    libraries=self.root / "android-libraries",
                    out=output,
                    fast=False,
                    toolchain_identity=self.root / "android-toolchains.json",
                )
            )
        self.assertFalse(output.exists())

    def test_mismatched_runtime_components_do_not_replace_previous_product(self):
        output = self.assemble()
        original = (output / "sdk-contract.json").read_bytes()
        receipt_path = self.components["ios"] / "sdk-contract.json"
        receipt = json.loads(receipt_path.read_text())
        receipt["identity"]["runtimeContract"] = "different-runtime"
        receipt_path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(ValueError, "identity mismatch: runtimeContract"):
            self.assemble()
        self.assertEqual((output / "sdk-contract.json").read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
