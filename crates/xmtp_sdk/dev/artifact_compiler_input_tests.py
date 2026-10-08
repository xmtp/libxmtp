"""Compiler input cases that use the ArtifactTests fixture."""

import json
import os
from pathlib import Path
from unittest.mock import patch

from artifact_test_modules import artifacts, mobile


class CompilerInputTests:
    def test_compiler_host_uses_artifact_compiler_selection(self):
        for inputs, expected in (
            ({}, "rustc"),
            ({"CARGO_BUILD_RUSTC": "/build/rustc"}, "/build/rustc"),
            (
                {"RUSTC": "/selected/rustc", "CARGO_BUILD_RUSTC": "/build/rustc"},
                "/selected/rustc",
            ),
        ):
            with (
                self.subTest(inputs=tuple(inputs)),
                patch.dict(os.environ, inputs, clear=True),
                patch.object(
                    artifacts.subprocess,
                    "check_output",
                    return_value=b"rustc fixture\nhost: aarch64-apple-darwin\n",
                ) as query,
            ):
                self.assertEqual(artifacts.compiler_host(), "aarch64-apple-darwin")
                query.assert_called_once_with([expected, "-vV"], cwd=artifacts.ROOT)
        with patch.object(
            artifacts.subprocess, "check_output", return_value=b"rustc fixture\n"
        ):
            with self.assertRaisesRegex(ValueError, "no host triple"):
                artifacts.compiler_host()

    def test_archive_index_inputs_change_artifact_cache_context(self):
        names = (
            "RANLIB",
            "RANLIBFLAGS",
            "TARGET_RANLIB",
            "TARGET_RANLIBFLAGS",
            "HOST_RANLIB",
            "HOST_RANLIBFLAGS",
            "VERGEN_GIT_SHA",
            "CI",
            "XMTP_TEST_LOGGING",
            "CARGO_INCREMENTAL",
            "KACHE_ADAPTIVE_INCREMENTAL",
            "KACHE_PRESERVE_INCREMENTAL",
            "KACHE_KEY_ENV_VARS",
            "KACHE_BASE_DIR",
            "KACHE_CACHE_EXECUTABLES",
            "KACHE_BUILD_SCRIPT_CACHE",
            "KACHE_CACHE_CC_LINKS",
            "CARGO_BUILD_RUSTFLAGS",
            "HOST_CFLAGS",
            "TARGET_CFLAGS",
            "HOST_CXXFLAGS",
            "TARGET_CXXFLAGS",
            "HOST_CC",
            "TARGET_CC",
            "HOST_CXX",
            "TARGET_CXX",
            "HOST_AR",
            "TARGET_AR",
            "HOST_ARFLAGS",
            "TARGET_ARFLAGS",
            "CC_SHELL_ESCAPED_FLAGS",
            "CRATE_CC_NO_DEFAULTS",
        ) + tuple(
            prefix + target
            for prefix in ("RANLIB_", "RANLIBFLAGS_")
            for triple in mobile.ANDROID.values()
            for target in (triple, triple.replace("-", "_"))
        )
        names += tuple(
            "AARCH64_APPLE_DARWIN_OPENSSL_" + key for key in ("LIB_DIR", "INCLUDE_DIR")
        )
        names += tuple(
            variable + "_" + target
            for variable in ("CC", "CXX", "CFLAGS", "CXXFLAGS", "AR", "ARFLAGS")
            for target in ("aarch64-apple-darwin", "aarch64_apple_darwin")
        )
        self.context_patch.stop()
        try:
            with patch.object(
                artifacts.subprocess, "check_output", return_value=b"fixture rustc"
            ):
                for name in names:
                    with (
                        self.subTest(input=name),
                        patch.dict(os.environ, {}, clear=True),
                    ):
                        before = artifacts.build_context()
                        os.environ[name] = "caller-selected input"
                        self.assertNotEqual(artifacts.build_context(), before)
        finally:
            self.context_patch.start()

    def test_kache_additional_keyed_environment_changes_sdk_context(self):
        self.context_patch.stop()
        try:
            with (
                patch.object(
                    artifacts.subprocess, "check_output", return_value=b"fixture rustc"
                ),
                patch.dict(
                    os.environ,
                    {
                        "KACHE_KEY_ENV_VARS": "CI, SDK_CALLER_ABI",
                        "SDK_CALLER_ABI": "first",
                    },
                    clear=True,
                ),
            ):
                before = artifacts.build_context()
                os.environ["SDK_CALLER_ABI"] = "second"
                self.assertNotEqual(artifacts.build_context(), before)
        finally:
            self.context_patch.start()

    def test_restored_legacy_cargo_config_invalidates_all_roles(self):
        self.args.targets = artifacts.TARGETS
        with patch.object(artifacts, "build_context", return_value="b" * 64):
            checkout = self.restored_checkout()
            config = checkout / ".cargo/config"
            config.parent.mkdir()
            config.write_text("[build]\nrustflags=['--cfg', 'new_checkout_flag']\n")
            with patch.object(artifacts, "ROOT", checkout):
                artifacts.build(self.args)
            self.assertEqual(len(self.cargo_environments), 4)

    def test_native_wrapper_context_gets_fresh_owned_objects(self):
        self.context_patch.stop()
        original = self.command
        values = []

        def sticky_native(command, **kwargs):
            original(command, **kwargs)
            if (
                command[0] == "dev/agent-run"
                and "xmtp_sdk" in command
                and "--target" not in command
            ):
                target = Path(kwargs["env"]["CARGO_TARGET_DIR"])
                marker = target / "untracked-native-object"
                if not marker.exists():
                    marker.write_text(kwargs["env"]["NIX_CFLAGS_COMPILE"])
                for library in (target / "debug").glob("libxmtp_sdk.*"):
                    library.write_text(marker.read_text())
                values.append(marker.read_text())

        with patch.object(artifacts, "run", side_effect=sticky_native):
            with patch.dict(os.environ, {"NIX_CFLAGS_COMPILE": "first-wrapper-flags"}):
                artifacts.build(self.args)
            with patch.dict(os.environ, {"NIX_CFLAGS_COMPILE": "second-wrapper-flags"}):
                artifacts.build(self.args)
        self.assertEqual(values, ["first-wrapper-flags", "second-wrapper-flags"])
        self.assertNotEqual(
            self.cargo_environments[0]["CARGO_TARGET_DIR"],
            self.cargo_environments[2]["CARGO_TARGET_DIR"],
        )
        self.assertEqual(
            self.cargo_environments[0]["CARGO_TARGET_DIR"],
            self.cargo_environments[1]["CARGO_TARGET_DIR"],
        )

    def test_native_alias_bytes_and_unresolved_tools_invalidate_raw_reuse(self):
        self.context_patch.stop()
        compiler = self.root / "compiler"
        compiler.write_text("#!/bin/sh\nexit 0\n")
        compiler.chmod(0o755)
        with patch.dict(os.environ, {"HOST_CC": str(compiler)}):
            before = artifacts.build_context()
            compiler.write_text("#!/bin/sh\nexit 1\n")
            self.assertNotEqual(artifacts.build_context(), before)
            self.assertTrue(artifacts.native_tools_supported())
        with patch.dict(os.environ, {"TARGET_CC": str(self.root / "missing-compiler")}):
            artifacts.build(self.args)
            first = self.cargo_environments[0]["CARGO_TARGET_DIR"]
            artifacts.build(self.args)
            self.assertEqual(len(self.cargo_environments), 4)
            self.assertNotEqual(first, self.cargo_environments[2]["CARGO_TARGET_DIR"])

    def test_receipt_records_target_build_and_config_flag_routes(self):
        with patch.dict(
            os.environ,
            {
                "CARGO_BUILD_RUSTFLAGS": "--cfg build_probe",
                "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS": "--cfg target_probe",
            },
        ):
            artifacts.build(self.args)
        receipt = json.loads((self.args.artifacts / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        self.assertEqual(
            receipt["compilerFlags"]["CARGO_BUILD_RUSTFLAGS"], ["--cfg", "build_probe"]
        )
        self.assertEqual(
            receipt["compilerFlags"]["CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS"],
            ["--cfg", "target_probe"],
        )
        self.assertIn("cargo-target:cfg(all())", receipt["compilerFlags"])
        self.assertTrue(receipt["debugProfileValid"])

    def test_config_injected_compiler_lookup_disables_raw_reuse(self):
        root = self.root / "compiler-config"
        (root / ".cargo").mkdir(parents=True)
        for body in (
            '[build]\nrustc-wrapper="/unproved/wrapper"\n',
            '[env]\nHOST_CC="/unproved/compiler"\n',
            '[env]\nCARGO_BUILD_RUSTC_WRAPPER="/unproved/wrapper"\n',
        ):
            (root / ".cargo/config.toml").write_text(body)
            with patch.object(artifacts, "ROOT", root):
                self.assertFalse(artifacts.native_tools_supported())

    def test_encoded_receipt_keeps_whitespace_inside_one_compiler_argument(self):
        encoded = '--cfg\x1fprobe="not -Copt-level=3"'
        with patch.dict(os.environ, {"CARGO_ENCODED_RUSTFLAGS": encoded}):
            artifacts.inputs.require_debug_profile(artifacts.ROOT)
            artifacts.build(self.args)
        receipt = json.loads((self.args.artifacts / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        self.assertEqual(
            receipt["compilerFlags"]["CARGO_ENCODED_RUSTFLAGS"],
            ["--cfg", 'probe="not -Copt-level=3"'],
        )
        self.assertTrue(receipt["debugProfileValid"])

    def test_new_literal_data_and_direct_environment_change_raw_keys(self):
        checkout = self.restored_checkout()
        source = checkout / "crates/fixture/src/lib.rs"
        data = source.with_name("read-data.dat")
        source.write_text(
            'pub const DATA:&str=include_str!("read-data.dat");\npub const ENV:Option<&str>=option_env!("SDK_FIXTURE_EMBEDDED_INPUT");\n'
        )
        data.write_text("first embedded bytes")
        self.context_patch.stop()
        with patch.object(artifacts, "ROOT", checkout):
            with patch.dict(os.environ, {"SDK_FIXTURE_EMBEDDED_INPUT": "first-env"}):
                artifacts.build(self.args)
                self.cargo_environments.clear()
                data.write_text("second embedded bytes")
                artifacts.build(self.args)
                self.assertEqual(len(self.cargo_environments), 2)
                self.cargo_environments.clear()
            with patch.dict(os.environ, {"SDK_FIXTURE_EMBEDDED_INPUT": "second-env"}):
                artifacts.build(self.args)
                self.assertEqual(len(self.cargo_environments), 2)

    def test_cargo_configuration_precedence_ancestors_and_home_are_hashed(self):
        config = self.root / "project/.cargo"
        config.mkdir(parents=True)
        root = config.parent
        legacy = config / "config"
        modern = config / "config.toml"
        legacy.write_text('[build]\nrustflags=["--cfg", "legacy"]\n')
        modern.write_text('[build]\nrustflags=["-Cdebug-assertions=false"]\n')
        home = self.root / "cargo-home"
        home.mkdir()
        (home / "config.toml").write_text('[build]\nrustflags=["--cfg", "home"]\n')
        ancestor = self.root / ".cargo"
        ancestor.mkdir()
        (ancestor / "config.toml").write_text(
            '[build]\nrustflags=["--cfg", "ancestor"]\n'
        )
        with patch.dict(os.environ, {"CARGO_HOME": str(home)}):
            hashes, effective = artifacts.inputs.cargo_config(root)
            self.assertIn("workspace:.cargo/config", hashes)
            self.assertNotIn("workspace:.cargo/config.toml", hashes)
            self.assertEqual(
                effective["build"]["rustflags"],
                ["--cfg", "home", "--cfg", "ancestor", "--cfg", "legacy"],
            )
            self.assertTrue(artifacts.inputs.debug_profile_valid(root))
            (home / "config.toml").write_text(
                '[build]\nrustflags=["--cfg", "changed_home"]\n'
            )
            self.assertNotEqual(artifacts.inputs.cargo_config(root)[0], hashes)

    def test_unproved_dynamic_include_uses_fresh_outputs_each_build(self):
        checkout = self.restored_checkout()
        (checkout / "crates/fixture/src/lib.rs").write_text(
            'const DATA:&str=include_str!(concat!(env!("SDK_DYNAMIC_INPUT"),"/data"));\n'
        )
        with patch.object(artifacts, "ROOT", checkout):
            artifacts.build(self.args)
            first = self.cargo_environments[-1]["CARGO_TARGET_DIR"]
            self.cargo_environments.clear()
            artifacts.build(self.args)
            self.assertEqual(len(self.cargo_environments), 2)
            self.assertNotEqual(first, self.cargo_environments[-1]["CARGO_TARGET_DIR"])

    def test_compiler_change_rebuilds_artifacts(self):
        artifacts.build(self.args)
        with patch.object(artifacts, "build_context", return_value="changed-compiler"):
            artifacts.build(self.args)
        self.assertEqual(len(self.calls), 4)
