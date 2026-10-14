#!/usr/bin/env python3
"""Check config provenance and the mobile staging from Nix libraries."""

import json
import os
import plistlib
import signal
from pathlib import Path
import subprocess
import shutil
import sys
import zipfile
from unittest.mock import Mock, patch
import unittest

from packaging_test_base import (
    AndroidDependencyInputs,
    PackagingTestBase,
    artifacts,
    mobile,
)


class PackagingTests(
    PackagingTestBase,
    AndroidDependencyInputs,
    unittest.TestCase,
):
    def native_libraries(self):
        """Nix-shaped JNI libraries with receipts that match the binding."""
        for abi, triple in mobile.ANDROID.items():
            folder = self.root / "mobile" / abi
            (folder / "lib").mkdir(parents=True, exist_ok=True)
            (folder / "lib/libxmtp_sdk.so").write_text(f"fixture library {abi}")
            (folder / "native-provenance.json").write_text(
                json.dumps(
                    {
                        "schema": 1,
                        "source": artifacts.source_hash(),
                        "target": triple,
                        "profile": "release",
                        "features": "",
                    }
                )
            )

    def ios_release(self, triples=None):
        """A Nix-shaped `.#ios-xcframeworks` output."""
        release = self.root / "mobile/ios/XmtpSdkFFI"
        framework = release / "XmtpSdkFFI.xcframework"
        framework.mkdir(parents=True, exist_ok=True)
        libraries = []
        for triple in mobile.IOS if triples is None else triples:
            platform, variant, arch = mobile.apple_slice(triple)
            library = {"SupportedPlatform": platform, "SupportedArchitectures": [arch]}
            if variant:
                library["SupportedPlatformVariant"] = variant
            libraries.append(library)
        with (framework / "Info.plist").open("wb") as plist:
            plistlib.dump({"AvailableLibraries": libraries}, plist)
        (release / "Sources/XmtpSdk").mkdir(parents=True, exist_ok=True)
        (release / "Sources/XmtpSdk/xmtp_sdk.swift").write_text("fixture binding")

    def prepare_mobile_stage(self, target):
        if target == "ios":
            self.ios_release()
        else:
            self.args.targets = ("kotlin",)
            artifacts.build(self.args)
            artifacts.render(self.args)
            self.native_libraries()
            self.seed_android_dependency_inputs()
        output = self.root / "products" / target
        if output.exists():
            shutil.rmtree(output)
        output.mkdir(parents=True)
        (output / "previous.txt").write_bytes(b"prior valid product")
        return output

    def mobile_tool(self, command, **kwargs):
        self.assertEqual(
            command[:5],
            [
                str(self.sdk_root / "gradlew"),
                "-p",
                str(self.sdk_root),
                ":library:assembleRelease",
                "--no-daemon",
            ],
        )
        output = self.sdk_root / "library/build/outputs/aar"
        output.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(output / "library-release.aar", "w") as archive:
            archive.writestr("classes.jar", b"fixture classes")
            for abi in mobile.ANDROID:
                archive.writestr(f"jni/{abi}/libxmtp_sdk.so", b"fixture native")

    def assemble_mobile(self, target, tool=None, sdk_root=None):
        if not isinstance(tool, Mock):
            tool = Mock(side_effect=tool or self.mobile_tool)
        with (
            patch.object(mobile, "ROOT", self.root),
            patch.object(mobile, "run", tool),
            patch.object(
                sys,
                "argv",
                [
                    "mobile-package.py",
                    "stage",
                    target,
                    "--generated",
                    str(self.args.out),
                    "--libraries",
                    str(self.root / "mobile"),
                    "--out",
                    str(self.root / "products"),
                    *(["--sdk-root", str(sdk_root)] if sdk_root else []),
                ],
            ),
        ):
            mobile.main()

    def product_files(self, output):
        return {
            str(path.relative_to(output)): path.read_bytes()
            for path in output.rglob("*")
            if path.is_file()
        }

    def test_ios_stage_copies_the_release_framework_with_every_listed_slice(self):
        output = self.prepare_mobile_stage("ios")
        before = self.product_files(output)
        self.ios_release(mobile.IOS[:-1])
        with self.assertRaisesRegex(ValueError, "slices"):
            self.assemble_mobile("ios")
        self.assertEqual(self.product_files(output), before)
        self.ios_release()
        self.assemble_mobile("ios")
        self.assertIn(".macOS(.v11)", (output / "Package.swift").read_text())
        self.assertTrue((output / "Sources/XmtpSdk/xmtp_sdk.swift").is_file())
        contract = json.loads((output / "sdk-contract.json").read_text())
        self.assertEqual(contract["slices"], list(mobile.IOS))

    def test_android_stage_admits_only_matching_libraries(self):
        output = self.prepare_mobile_stage("android")
        receipt = (
            self.root / "mobile" / next(iter(mobile.ANDROID)) / "native-provenance.json"
        )
        original = receipt.read_text()
        for field, wrong in (
            ("source", "wrong source"),
            ("features", "conformance"),
            ("profile", "debug"),
            ("target", "wrong-target"),
        ):
            with self.subTest(field=field):
                receipt.write_text(json.dumps({**json.loads(original), field: wrong}))
                tool = Mock(side_effect=self.mobile_tool)
                with self.assertRaisesRegex(
                    ValueError, "mobile binding contract mismatch"
                ):
                    self.assemble_mobile("android", tool)
                tool.assert_not_called()
                self.assertTrue((output / "previous.txt").exists())
        receipt.write_text(original)
        self.assemble_mobile("android")
        contract = json.loads((output / "sdk-contract.json").read_text())
        self.assertEqual(set(contract["native"]), set(mobile.ANDROID.values()))
        with zipfile.ZipFile(output / "xmtp-sdk.aar") as archive:
            self.assertIn("classes.jar", archive.namelist())

    def test_android_stage_rejects_a_stale_binding(self):
        for path, content, message in (
            (
                "apps/xmtp_sdk_bindgen/template.txt",
                "new template",
                "binding generator mismatch",
            ),
            (
                "crates/xmtp_sdk/uniffi.toml",
                "changed configuration",
                "binding source mismatch",
            ),
        ):
            with self.subTest(changed=path):
                self.prepare_mobile_stage("android")
                original = (self.root / path).read_text()
                (self.root / path).write_text(content)
                tool = Mock(side_effect=self.mobile_tool)
                with self.assertRaisesRegex(ValueError, message):
                    self.assemble_mobile("android", tool)
                tool.assert_not_called()
                (self.root / path).write_text(original)

    def test_android_late_tool_failure_preserves_prior_and_cleans_fresh_stage(self):
        output = self.prepare_mobile_stage("android")
        before = self.product_files(output)

        def fail(command, **kwargs):
            self.mobile_tool(command, **kwargs)
            raise subprocess.CalledProcessError(1, command)

        def missing_abi(command, **kwargs):
            self.mobile_tool(command, **kwargs)
            archive = self.sdk_root / "library/build/outputs/aar/library-release.aar"
            with zipfile.ZipFile(archive, "w") as broken:
                broken.writestr("classes.jar", b"fixture classes")

        for tool, error in (
            (fail, subprocess.CalledProcessError),
            (missing_abi, ValueError),
        ):
            with self.subTest(tool=tool.__name__):
                with self.assertRaises(error):
                    self.assemble_mobile("android", tool)
                self.assertEqual(self.product_files(output), before)
                self.assertEqual(list(output.parent.glob(".sdk-mobile-stage-*")), [])
        self.assemble_mobile("android")
        self.assertFalse((output / "previous.txt").exists())
        contract = json.loads((output / "sdk-contract.json").read_text())
        for name, expected in contract["assets"].items():
            self.assertEqual(artifacts.digest(output / name), expected)

    def test_mobile_interruptions_preserve_prior_bytes(self):
        replace = os.replace
        for target in ("ios", "android"):
            for kind in ("sigint", "exit"):
                for phase in ("before-backup", "after-backup", "after-promotion"):
                    with self.subTest(target=target, kind=kind, phase=phase):
                        output = self.prepare_mobile_stage(target).resolve()
                        before = self.product_files(output)
                        fired = False

                        def interrupt(source, destination):
                            nonlocal fired
                            backup = Path(source) == output
                            promotion = Path(source).name == "product"
                            selected = (backup and phase != "after-promotion") or (
                                promotion and phase == "after-promotion"
                            )
                            if not fired and selected and phase == "before-backup":
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)
                            replace(source, destination)
                            if not fired and selected:
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)

                        expected = KeyboardInterrupt if kind == "sigint" else SystemExit
                        with patch.object(mobile.os, "replace", side_effect=interrupt):
                            with self.assertRaises(expected) as error:
                                self.assemble_mobile(target)
                        self.assertTrue(fired)
                        if kind == "exit":
                            self.assertEqual(error.exception.code, 71)
                        self.assertEqual(self.product_files(output), before)
                        self.assertEqual(
                            list(output.parent.glob(".sdk-mobile-stage-*")), []
                        )

    def test_mobile_interrupted_rollback_preserves_original_failure(self):
        replace = os.replace
        for target in ("ios", "android"):
            for kind in ("sigint", "exit"):
                for phase in ("before-rollback", "after-rollback"):
                    with self.subTest(target=target, kind=kind, phase=phase):
                        output = self.prepare_mobile_stage(target).resolve()
                        before = self.product_files(output)

                        def invoke():
                            self.assemble_mobile(target)

                        files = self.product_files
                        prefix = ".sdk-mobile-stage-*"
                        failure = OSError("promotion failed")
                        fired = False

                        def interrupt(source, destination):
                            nonlocal fired
                            if Path(source).name == "product":
                                raise failure
                            rollback = Path(source).name == "previous"
                            if rollback and phase == "before-rollback":
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)
                            replace(source, destination)
                            if rollback and phase == "after-rollback":
                                fired = True
                                if kind == "sigint":
                                    os.kill(os.getpid(), signal.SIGINT)
                                raise SystemExit(71)

                        with patch.object(os, "replace", side_effect=interrupt):
                            with self.assertRaises(BaseException) as error:
                                invoke()
                        self.assertTrue(fired)
                        self.assertIs(error.exception, failure)
                        stages = list(output.parent.glob(prefix))
                        if phase == "before-rollback":
                            self.assertFalse(output.exists())
                            backup = Path(
                                error.exception.__notes__[0].split("preserved at ", 1)[
                                    1
                                ]
                            )
                            self.assertTrue(backup.is_absolute())
                            self.assertEqual(files(backup), before)
                            invoke()
                            self.assertEqual(files(backup), before)
                            recovery = self.root / "manual-recovery"
                            if recovery.exists():
                                shutil.rmtree(recovery)
                            os.replace(backup, recovery)
                            self.assertEqual(files(recovery), before)
                            shutil.rmtree(stages[0])
                        else:
                            self.assertEqual(files(output), before)
                            self.assertEqual(stages, [])

    def test_mobile_promotion_failures_restore_or_preserve_prior_bytes(self):
        replace = os.replace
        for target in ("ios", "android"):
            for failures in ((1,), (2,), (2, 3)):
                with self.subTest(target=target, failures=failures):
                    output = self.prepare_mobile_stage(target)
                    before = self.product_files(output)
                    calls = []

                    def fail(source, destination):
                        calls.append((Path(source), Path(destination)))
                        if len(calls) in failures:
                            raise OSError("fixture rename failure")
                        replace(source, destination)

                    with patch.object(mobile.os, "replace", side_effect=fail):
                        with self.assertRaisesRegex(
                            OSError, "fixture rename failure|previous product preserved"
                        ) as error:
                            self.assemble_mobile(target)
                    if failures == (2, 3):
                        self.assertFalse(output.exists())
                        stages = list(output.parent.glob(".sdk-mobile-stage-*"))
                        self.assertEqual(len(stages), 1)
                        self.assertEqual(
                            error.exception.__notes__[0],
                            f"previous product preserved at {(stages[0] / 'previous').resolve()}",
                        )
                        self.assertEqual(
                            self.product_files(stages[0] / "previous"), before
                        )
                        shutil.rmtree(stages[0])
                    else:
                        self.assertEqual(self.product_files(output), before)
                        self.assertEqual(
                            list(output.parent.glob(".sdk-mobile-stage-*")), []
                        )
                        shutil.rmtree(output)

    def test_source_identity_change_invalidates_build_and_recorded_source(self):
        path = self.root / "crates/xmtp_id/src/scw_verifier/chain_urls_default.json"
        path.parent.mkdir(parents=True)
        path.write_text("original embedded data")
        artifacts.build(self.args)
        artifacts.render(self.args)
        old_record = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        before_calls = len(self.calls)
        path.write_text("changed embedded data only")
        self.assertNotEqual(artifacts.source_hash(), old_record["artifact"]["source"])
        artifacts.build(self.args)
        self.assertEqual(len(self.calls), before_calls + 2)
        artifacts.render(self.args)
        new_record = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        self.assertEqual(new_record["artifact"]["source"], artifacts.source_hash())

    def test_render_rejects_a_stale_generator(self):
        self.args.targets = ("swift", "kotlin")
        artifacts.build(self.args)
        artifacts.render(self.args)
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text("new template")
        with self.assertRaisesRegex(ValueError, "generator mismatch"):
            artifacts.render(self.args)

    def test_configuration_source_invalidates_cached_generator(self):
        path = self.root / "crates/xmtp_configuration/src/lib.rs"
        path.parent.mkdir(parents=True)
        path.write_text('pub const WASM_VFS_DIRECTORY: &str = "original";')
        artifacts.build(self.args)
        artifacts.render(self.args)
        before = artifacts.source_hash(True)
        path.write_text('pub const WASM_VFS_DIRECTORY: &str = "changed";')
        self.assertNotEqual(artifacts.source_hash(True), before)
        artifacts.build(self.args)
        artifacts.render(self.args)


if __name__ == "__main__":
    unittest.main()
