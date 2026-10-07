#!/usr/bin/env python3.11
"""Check native build context: source fingerprint, compiler inputs, and deployment floors."""

import json
import os
import subprocess
from unittest.mock import patch
import unittest

from packaging_test_base import PackagingTestBase, artifacts


class NativeBuildContextTests(PackagingTestBase, unittest.TestCase):
    def test_git_and_filtered_source_have_same_fingerprint(self):
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        expected = (artifacts.source_hash(), artifacts.source_hash(True))
        (self.root / ".git").rename(self.root / ".git-hidden")
        actual = (artifacts.source_hash(), artifacts.source_hash(True))
        self.assertEqual(actual, expected)

    def test_unqualified_compiler_inputs_change_native_cache_admission(self):
        names = (
            "CC",
            "CXX",
            "AR",
            "RANLIB",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "PERL",
        ) + tuple(
            prefix + "OPENSSL_" + name
            for prefix in ("", "AARCH64_LINUX_ANDROID_")
            for name in (
                "DIR",
                "LIB_DIR",
                "INCLUDE_DIR",
                "NO_VENDOR",
                "STATIC",
                "LIBS",
                "CONFIG_DIR",
                "SRC_PERL",
                "RUST_USE_NASM",
            )
        )
        environment = {
            key: value for key, value in os.environ.items() if key not in names
        }
        self.patches[2].stop()
        try:
            with patch.object(
                artifacts.subprocess, "check_output", return_value=b"fixture rustc"
            ):
                for name in names:
                    with (
                        self.subTest(input=name),
                        patch.dict(os.environ, environment, clear=True),
                    ):
                        self.args.artifacts = self.root / name
                        baseline = artifacts.build_context()
                        artifacts.build(self.args)
                        before = json.loads(
                            (self.args.artifacts / "artifacts.json").read_text()
                        )
                        calls = len(self.calls)
                        os.environ[name] = "changed compiler input"
                        self.assertNotEqual(artifacts.build_context(), baseline)
                        artifacts.build(self.args)
                        after = json.loads(
                            (self.args.artifacts / "artifacts.json").read_text()
                        )
                        self.assertNotEqual(
                            before["artifacts"]["native"]["key"],
                            after["artifacts"]["native"]["key"],
                        )
                        self.assertEqual(len(self.calls), calls + 2)
        finally:
            self.patches[2].start()

    def test_apple_native_build_pins_supported_deployment_floors(self):
        with (
            patch.object(artifacts.sys, "platform", "darwin"),
            patch.dict(
                os.environ,
                {
                    "MACOSX_DEPLOYMENT_TARGET": "14.0",
                    "IPHONEOS_DEPLOYMENT_TARGET": "17",
                },
            ),
        ):
            artifacts.build(self.args)
            self.assertEqual(os.environ["MACOSX_DEPLOYMENT_TARGET"], "11.0")
            self.assertEqual(os.environ["IPHONEOS_DEPLOYMENT_TARGET"], "14")

    def test_cargo_compiler_inputs_change_native_cache_admission(self):
        names = ("RUSTC", "CARGO_BUILD_RUSTC")
        environment = {
            key: value for key, value in os.environ.items() if key not in names
        }
        compiler = self.root / "fixture-rustc"
        compiler.write_text("#!/bin/sh\nprintf 'v1\\n'\n")
        compiler.chmod(0o755)
        self.patches[2].stop()
        try:
            for name in names:
                with (
                    self.subTest(input=name),
                    patch.dict(os.environ, environment, clear=True),
                ):
                    self.args.artifacts = self.root / name
                    artifacts.build(self.args)
                    before = json.loads(
                        (self.args.artifacts / "artifacts.json").read_text()
                    )
                    calls = len(self.calls)
                    os.environ[name] = str(compiler)
                    artifacts.build(self.args)
                    after = json.loads(
                        (self.args.artifacts / "artifacts.json").read_text()
                    )
                    self.assertNotEqual(
                        before["artifacts"]["native"]["key"],
                        after["artifacts"]["native"]["key"],
                    )
                    self.assertEqual(len(self.calls), calls + 2)
                    calls = len(self.calls)
                    artifacts.build(self.args)
                    self.assertEqual(len(self.calls), calls)
        finally:
            self.patches[2].start()

    def test_selected_compiler_version_changes_rebuild_without_path_change(self):
        environment = {
            key: value
            for key, value in os.environ.items()
            if key not in ("RUSTC", "CARGO_BUILD_RUSTC")
        }
        compiler = self.root / "fixture-rustc"
        compiler.write_text("#!/bin/sh\nprintf 'fixture rustc version one\\n'\n")
        compiler.chmod(0o755)
        self.patches[2].stop()
        try:
            for variable in ("RUSTC", "CARGO_BUILD_RUSTC"):
                with (
                    self.subTest(input=variable),
                    patch.dict(os.environ, environment, clear=True),
                ):
                    os.environ[variable] = str(compiler)
                    self.args.artifacts = self.root / variable
                    artifacts.build(self.args)
                    calls = len(self.calls)
                    compiler.write_text("#!/bin/sh\nprintf 'v2\\n'\n")
                    artifacts.build(self.args)
                    self.assertEqual(len(self.calls), calls + 2)
                    compiler.write_text("#!/bin/sh\nprintf 'v1\\n'\n")
        finally:
            self.patches[2].start()

    def test_selected_compiler_bytes_change_rebuild_with_same_version(self):
        compiler = self.root / "fixture-rustc"
        compiler.write_text(
            "#!/bin/sh\nprintf 'same version\\n'\n# original compiler\n"
        )
        compiler.chmod(0o755)
        self.patches[2].stop()
        try:
            with patch.dict(os.environ, {"RUSTC": str(compiler)}):
                artifacts.build(self.args)
                calls = len(self.calls)
                compiler.write_text(
                    "#!/bin/sh\nprintf 'same version\\n'\n# changed compiler\n"
                )
                artifacts.build(self.args)
                self.assertEqual(len(self.calls), calls + 2)
                calls = len(self.calls)
                artifacts.build(self.args)
                self.assertEqual(len(self.calls), calls)
        finally:
            self.patches[2].start()

    def test_target_ranlib_path_version_and_bytes_change_cache_context(self):
        tool = self.root / "llvm-ranlib"
        tool.write_text("#!/bin/sh\nprintf 'LLVM ranlib one\\n'\n")
        tool.chmod(0o755)
        self.patches[2].stop()
        try:
            with patch.dict(os.environ, {"RANLIB_aarch64_linux_android": str(tool)}):
                baseline = artifacts.build_context()
                tool.write_text("#!/bin/sh\nprintf 'LLVM ranlib two\\n'\n")
                self.assertNotEqual(artifacts.build_context(), baseline)
                with (
                    patch.object(artifacts.subprocess, "run") as version_probe,
                    patch.object(
                        artifacts.subprocess,
                        "check_output",
                        return_value=b"fixed rustc",
                    ),
                ):
                    version_probe.return_value = subprocess.CompletedProcess(
                        [], 0, b"version one", b""
                    )
                    stable_bytes = artifacts.build_context()
                    version_probe.return_value = subprocess.CompletedProcess(
                        [], 0, b"version two", b""
                    )
                    self.assertNotEqual(artifacts.build_context(), stable_bytes)
                version = artifacts.build_context()
                tool.write_text(
                    "#!/bin/sh\nprintf 'LLVM ranlib two\\n'\n# changed bytes\n"
                )
                self.assertNotEqual(artifacts.build_context(), version)
                before = artifacts.build_context()
                os.environ["RANLIB_aarch64_linux_android"] = str(
                    self.root / "other-ranlib"
                )
                self.assertNotEqual(artifacts.build_context(), before)
        finally:
            self.patches[2].start()

    def test_rustc_override_has_precedence_over_cargo_build_rustc(self):
        self.patches[2].stop()
        try:
            with (
                patch.dict(
                    os.environ,
                    {"RUSTC": "selected-rustc", "CARGO_BUILD_RUSTC": "other-rustc"},
                ),
                patch.object(
                    artifacts.subprocess,
                    "check_output",
                    return_value=b"selected compiler",
                ) as probe,
            ):
                artifacts.build_context()
                probe.assert_called_once_with(["selected-rustc", "-vV"], cwd=self.root)
        finally:
            self.patches[2].start()


if __name__ == "__main__":
    unittest.main()
