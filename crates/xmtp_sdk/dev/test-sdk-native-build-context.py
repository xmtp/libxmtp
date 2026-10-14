#!/usr/bin/env python3
"""Check native build context: source identity, compiler inputs, and deployment floors."""

import json
import os
import subprocess
from unittest.mock import patch
import unittest

from packaging_test_base import PackagingTestBase, artifacts


# The fixture base replaces this; keep the real function for its own tests.
REAL_PROVENANCE = artifacts.provenance.__wrapped__


class NativeBuildContextTests(PackagingTestBase, unittest.TestCase):
    def test_untracked_input_files_fail_before_the_nix_evaluation(self):
        def git(*args):
            return subprocess.check_output(
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
                cwd=self.root,
                text=True,
            )

        git("init", "--quiet")
        git("add", "--all")
        git("commit", "--quiet", "--no-verify", "--no-gpg-sign", "-m", "fixture")
        added = self.root / "crates/xmtp_sdk/src/added.rs"
        added.parent.mkdir(parents=True, exist_ok=True)
        added.write_text("pub fn added() {}")
        real = subprocess.check_output
        evaluated = {"source": "closure", "generator": "bindgen closure"}

        def nix_or_git(command, **kwargs):
            if command[0] == "nix":
                return json.dumps(evaluated).encode()
            return real(command, **kwargs)

        with (
            patch.object(artifacts.sys, "platform", "linux"),
            patch.object(artifacts.subprocess, "check_output", side_effect=nix_or_git),
        ):
            with self.assertRaisesRegex(
                ValueError, "git add.*crates/xmtp_sdk/src/added.rs"
            ):
                REAL_PROVENANCE()
            git("add", "--all")
            self.assertEqual(REAL_PROVENANCE(), evaluated)
            # Files that are not SDK inputs do not block the evaluation.
            for name in (
                "crates/xmtp_sdk/proptest-regressions/lib.txt",
                "crates/xmtp_sdk/src/lib.rs.orig",
            ):
                (self.root / name).parent.mkdir(parents=True, exist_ok=True)
                (self.root / name).write_text("not an input")
            self.assertEqual(REAL_PROVENANCE(), evaluated)

    def test_windows_identity_names_the_commit_and_uncommitted_inputs(self):
        def git(*args):
            return subprocess.check_output(
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
                cwd=self.root,
                text=True,
            ).strip()

        def identity():
            with patch.object(artifacts.sys, "platform", "win32"):
                value = REAL_PROVENANCE()
            self.assertEqual(value["source"], value["generator"])
            return value["source"]

        source = self.root / "crates/xmtp_sdk/src/lib.rs"
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text("pub fn committed() {}")
        git("init", "--quiet")
        git("add", "--all")
        git("commit", "--quiet", "--no-verify", "--no-gpg-sign", "-m", "fixture")
        clean = f"commit:{git('rev-parse', 'HEAD')}"
        self.assertEqual(identity(), clean)
        # Files outside the SDK inputs, such as a runtime checkout, do not count.
        (self.root / "ubrn").mkdir()
        (self.root / "ubrn/file.txt").write_text("other checkout")
        self.assertEqual(identity(), clean)
        source.write_text("pub fn edited() {}")
        edited = identity()
        self.assertTrue(edited.startswith(clean + "+dirty:"), edited)
        source.write_text("pub fn committed() {}")
        added = self.root / "apps/xmtp_sdk_bindgen/runtime/swift/new.swift"
        added.parent.mkdir(parents=True, exist_ok=True)
        added.write_text("// first")
        untracked = identity()
        self.assertTrue(untracked.startswith(clean + "+dirty:"), untracked)
        self.assertNotEqual(untracked, edited)
        added.write_text("// second")
        self.assertNotIn(identity(), (clean, edited, untracked))

    def test_source_key_follows_sdk_inputs_not_commits(self):
        def git(*args):
            return subprocess.check_output(
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
                cwd=self.root,
                text=True,
            ).strip()

        def commit(message):
            git("add", "--all")
            git("commit", "--quiet", "--no-verify", "--no-gpg-sign", "-m", message)

        (self.root / "Cargo.toml").write_text(
            '[workspace.metadata.xmtp-sdk-fork]\nrev = "' + "f" * 40 + '"\n'
        )
        recipe = self.root / artifacts.NODE_PLATFORM_RECIPE
        recipe.parent.mkdir(parents=True)
        recipe.write_text("windows: build")
        source = self.root / "crates/xmtp_sdk/src/lib.rs"
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text("pub fn one() {}")
        git("init", "--quiet")
        commit("first")
        key = artifacts.source_key("release", "x86_64-pc-windows-msvc")
        self.assertEqual(key["forkRevision"], "f" * 40)
        self.assertNotIn(git("rev-parse", "HEAD"), repr(key))
        # A commit that changes no SDK input keeps the key.
        (self.root / "README.md").write_text("docs")
        commit("docs")
        self.assertEqual(artifacts.source_key("release", "x86_64-pc-windows-msvc"), key)
        self.assertNotEqual(
            artifacts.source_key("release", "aarch64-pc-windows-msvc")["key"],
            key["key"],
        )
        self.assertEqual(artifacts.source_key()["source"], key["source"])
        self.assertNotIn("key", artifacts.source_key())
        # The build recipe and the compiler context are part of the key only.
        recipe.write_text("windows: changed build")
        commit("recipe")
        changed = artifacts.source_key("release", "x86_64-pc-windows-msvc")
        self.assertEqual(changed["source"], key["source"])
        self.assertNotEqual(changed["key"], key["key"])
        with patch.object(artifacts, "build_context", return_value="other compiler"):
            other = artifacts.source_key("release", "x86_64-pc-windows-msvc")
        self.assertNotEqual(other["key"], changed["key"])
        # An uncommitted input has no key; a committed one changes it.
        source.write_text("pub fn two() {}")
        with self.assertRaisesRegex(ValueError, "committed SDK inputs"):
            artifacts.source_key()
        commit("edit")
        edited = artifacts.source_key("release", "x86_64-pc-windows-msvc")
        self.assertNotEqual(edited["source"], key["source"])
        self.assertNotEqual(edited["key"], key["key"])

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
