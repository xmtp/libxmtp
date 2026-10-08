#!/usr/bin/env python3.11
"""Check artifact reuse, render input checks, toolchain inputs, and stale output cleanup."""

import argparse
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import shutil
import tempfile
from unittest.mock import patch
import unittest

from artifact_compiler_input_tests import CompilerInputTests
from artifact_test_modules import artifacts, mobile


class ArtifactTests(CompilerInputTests, unittest.TestCase):
    def test_android_targets_vendor_openssl_with_inherited_host_libraries(self):
        host = {
            "ANDROID_NDK_HOME": "/fixture/ndk",
            "OPENSSL_NO_VENDOR": "1",
            "OPENSSL_DIR": "/fixture/host/include",
            "OPENSSL_LIB_DIR": "/fixture/host/lib",
        }
        with (
            patch.dict(os.environ, host, clear=True),
            patch.object(Path, "glob", return_value=[Path("/fixture/ndk/toolchain")]),
            patch.object(Path, "is_file", return_value=True),
        ):
            for triple in mobile.ANDROID.values():
                with self.subTest(triple=triple):
                    env = mobile.android_environment(triple)
                    prefix = triple.upper().replace("-", "_") + "_OPENSSL_"
                    self.assertEqual(env[prefix + "NO_VENDOR"], "0")
                    for name in ("OPENSSL_DIR", "OPENSSL_LIB_DIR", "OPENSSL_NO_VENDOR"):
                        self.assertEqual(env[name], host[name])
                    self.assertFalse(
                        prefix + "NO_VENDOR" in os.environ, "Parent environment changed"
                    )

    def test_android_preserves_explicit_target_openssl_policy_and_paths(self):
        triple = "aarch64-linux-android"
        prefix = "AARCH64_LINUX_ANDROID_OPENSSL_"
        for selected in ("NO_VENDOR", "DIR", "LIB_DIR", "INCLUDE_DIR"):
            with (
                self.subTest(selected=selected),
                patch.dict(
                    os.environ,
                    {
                        "ANDROID_NDK_HOME": "/fixture/ndk",
                        "OPENSSL_NO_VENDOR": "1",
                        prefix + selected: "1"
                        if selected == "NO_VENDOR"
                        else "/target/openssl",
                    },
                    clear=True,
                ),
                patch.object(
                    Path, "glob", return_value=[Path("/fixture/ndk/toolchain")]
                ),
                patch.object(Path, "is_file", return_value=True),
            ):
                env = mobile.android_environment(triple)
                self.assertEqual(env[prefix + selected], os.environ[prefix + selected])
                self.assertEqual(env["OPENSSL_NO_VENDOR"], "1")
                if selected != "NO_VENDOR":
                    self.assertEqual(env[prefix + "NO_VENDOR"], "1")

    def test_android_target_openssl_paths_select_external_libraries(self):
        prefix = "AARCH64_LINUX_ANDROID_OPENSSL_"
        for path in ("DIR", "LIB_DIR", "INCLUDE_DIR"):
            for host_policy in (None, "0", "1"):
                for target_policy in (None, "0", "1"):
                    inputs = {
                        "ANDROID_NDK_HOME": "/fixture/ndk",
                        prefix + path: "/caller/openssl",
                    }
                    if host_policy is not None:
                        inputs["OPENSSL_NO_VENDOR"] = host_policy
                    if target_policy is not None:
                        inputs[prefix + "NO_VENDOR"] = target_policy
                    with (
                        self.subTest(path=path, host=host_policy, target=target_policy),
                        patch.dict(os.environ, inputs, clear=True),
                        patch.object(
                            Path, "glob", return_value=[Path("/fixture/ndk/toolchain")]
                        ),
                        patch.object(Path, "is_file", return_value=True),
                    ):
                        env = mobile.android_environment("aarch64-linux-android")
                        self.assertEqual(
                            env[prefix + "NO_VENDOR"],
                            target_policy if target_policy is not None else "1",
                        )
                        self.assertEqual(env[prefix + path], "/caller/openssl")
                        self.assertTrue(
                            all(
                                env.get(name) == value for name, value in inputs.items()
                            ),
                            "Caller input changed",
                        )

    def test_android_target_root_keeps_host_inputs_separate(self):
        host_prefix = "AARCH64_APPLE_DARWIN_OPENSSL_"
        for triple in mobile.ANDROID.values():
            prefix = triple.upper().replace("-", "_") + "_OPENSSL_"
            for host_policy in (None, "0", "1"):
                for target_policy in (None, "0", "1"):
                    for components in (
                        (),
                        ("LIB_DIR",),
                        ("INCLUDE_DIR",),
                        ("LIB_DIR", "INCLUDE_DIR"),
                    ):
                        for qualified_host in (False, True):
                            inputs = {
                                "ANDROID_NDK_HOME": "/fixture/ndk",
                                prefix + "DIR": "/target/root",
                                "OPENSSL_LIB_DIR": "/host/lib",
                                "OPENSSL_INCLUDE_DIR": "/host/include",
                            }
                            inputs.update(
                                {prefix + key: "/caller/" + key for key in components}
                            )
                            if qualified_host:
                                inputs.update(
                                    {
                                        host_prefix + key: "/qualified/" + key
                                        for key in ("LIB_DIR", "INCLUDE_DIR")
                                    }
                                )
                            if host_policy is not None:
                                inputs["OPENSSL_NO_VENDOR"] = host_policy
                            if target_policy is not None:
                                inputs[prefix + "NO_VENDOR"] = target_policy
                            with (
                                self.subTest(
                                    triple=triple,
                                    host=host_policy,
                                    policy=target_policy,
                                    components=components,
                                    qualified_host=qualified_host,
                                ),
                                patch.dict(os.environ, inputs, clear=True),
                                patch.object(
                                    Path,
                                    "glob",
                                    return_value=[Path("/fixture/ndk/toolchain")],
                                ),
                                patch.object(Path, "is_file", return_value=True),
                                patch.object(
                                    mobile.artifacts,
                                    "compiler_host",
                                    return_value="aarch64-apple-darwin",
                                ),
                            ):
                                env = mobile.android_environment(triple)
                                self.assertEqual(
                                    dict(os.environ),
                                    inputs,
                                    "Parent environment changed",
                                )
                                self.assertEqual(
                                    env[prefix + "NO_VENDOR"], target_policy or "1"
                                )
                                for key in ("LIB_DIR", "INCLUDE_DIR"):
                                    self.assertEqual(
                                        env.get(prefix + key), inputs.get(prefix + key)
                                    )
                                    if target_policy == "0":
                                        self.assertEqual(
                                            env["OPENSSL_" + key],
                                            inputs["OPENSSL_" + key],
                                        )
                                    else:
                                        self.assertFalse(
                                            "OPENSSL_" + key in env,
                                            "Host component shadows target root",
                                        )
                                        self.assertEqual(
                                            env[host_prefix + key],
                                            inputs.get(
                                                host_prefix + key,
                                                inputs["OPENSSL_" + key],
                                            ),
                                        )

    def test_android_equal_host_target_does_not_add_root_shadow(self):
        triple = "aarch64-linux-android"
        prefix = "AARCH64_LINUX_ANDROID_OPENSSL_"
        inputs = {
            "ANDROID_NDK_HOME": "/fixture/ndk",
            prefix + "DIR": "/target/root",
            "OPENSSL_LIB_DIR": "/host/lib",
        }
        with (
            patch.dict(os.environ, inputs, clear=True),
            patch.object(Path, "glob", return_value=[Path("/fixture/ndk/toolchain")]),
            patch.object(Path, "is_file", return_value=True),
            patch.object(mobile.artifacts, "compiler_host", return_value=triple),
        ):
            env = mobile.android_environment(triple)
            self.assertFalse(
                prefix + "LIB_DIR" in env, "Host compensation shadows target root"
            )
            self.assertFalse(
                "OPENSSL_LIB_DIR" in env, "Generic path shadows target root"
            )
            self.assertEqual(dict(os.environ), inputs)

    def test_android_archive_index_uses_target_tool_and_keeps_caller_inputs(self):
        host = {
            "ANDROID_NDK_HOME": "/fixture/ndk",
            "RANLIB": "/host/ranlib",
            "RANLIBFLAGS": "host flags",
        }
        for triple in mobile.ANDROID.values():
            target = triple.replace("-", "_")
            for override in (
                None,
                "RANLIB_" + triple,
                "RANLIB_" + target,
                "TARGET_RANLIB",
            ):
                with (
                    self.subTest(triple=triple, override=override),
                    patch.dict(
                        os.environ,
                        host | ({override: "/caller/llvm-ranlib"} if override else {}),
                        clear=True,
                    ),
                    patch.object(
                        Path, "glob", return_value=[Path("/fixture/ndk/toolchain")]
                    ),
                    patch.object(Path, "is_file", return_value=True),
                ):
                    env = mobile.android_environment(triple)
                    self.assertEqual(env["RANLIB"], host["RANLIB"])
                    self.assertEqual(env["RANLIBFLAGS"], host["RANLIBFLAGS"])
                    if override:
                        self.assertEqual(env[override], "/caller/llvm-ranlib")
                        if override != "RANLIB_" + target:
                            self.assertFalse(
                                "RANLIB_" + target in env, "Caller tool was masked"
                            )
                    else:
                        self.assertEqual(
                            Path(env["RANLIB_" + target]).name,
                            "llvm-ranlib.exe"
                            if mobile.sys.platform == "win32"
                            else "llvm-ranlib",
                        )
                    self.assertTrue(
                        dict(os.environ)
                        == host
                        | ({override: "/caller/llvm-ranlib"} if override else {}),
                        "Parent environment changed",
                    )

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.calls = []
        self.cargo_environments = []
        self.args = argparse.Namespace(
            artifacts=self.root / "artifacts",
            targets=("node",),
            out=self.root / "generated",
            features="",
            rust_target="",
            skip_bindgen=False,
            reuse_bindgen=None,
            profile="debug",
            no_format=True,
        )
        self.hash_patch = patch.object(
            artifacts, "source_hash", return_value="fixture-source"
        )
        self.hash_patch.start()
        self.context_patch = patch.object(
            artifacts, "build_context", return_value="fixture-compiler"
        )
        self.context_patch.start()
        self.run_patch = patch.object(artifacts, "run", side_effect=self.command)
        self.run_patch.start()

    def tearDown(self):
        self.run_patch.stop()
        self.hash_patch.stop()
        self.context_patch.stop()
        self.temporary.cleanup()

    def command(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "dev/agent-run":
            self.cargo_environments.append(dict(kwargs["env"]))
            out = Path(kwargs["env"]["CARGO_TARGET_DIR"])
            if "--target" in command:
                out /= command[command.index("--target") + 1]
            out /= "debug"
            out.mkdir(parents=True, exist_ok=True)
            names = (
                ["xmtp-sdk-bindgen"]
                if "xmtp-sdk-bindgen" in command
                else ["xmtp_sdk.wasm"]
                if "--target" in command
                else ["libxmtp_sdk.a", "libxmtp_sdk.dylib", "libxmtp_sdk.so"]
            )
            for name in names:
                (out / name).write_text("fixture artifact")
        else:
            out = Path(command[command.index("--out") + 1])
            out.mkdir(parents=True, exist_ok=True)
            (out / "index.ts").write_text("export const sdkVersion = 'fixture';\n")

    def test_native_build_reuses_identical_artifacts_and_does_not_build_wasm(self):
        artifacts.build(self.args)
        artifacts.build(self.args)
        self.assertEqual(len(self.calls), 2)
        self.assertFalse(
            any("wasm32-unknown-unknown" in command for command in self.calls)
        )
        artifacts.render(self.args)
        self.assertTrue((self.args.out / "typescript-napi/index.ts").is_file())
        self.assertFalse((self.args.out / "typescript-wasm").exists())

    def test_browser_build_and_render_need_no_native_product(self):
        self.args.targets = ("browser",)
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.assertFalse((self.args.artifacts / "native").exists())
        self.assertEqual(len(self.cargo_environments), 3)
        for tree in ("typescript-wasm", "typescript-pure"):
            self.assertTrue((self.args.out / tree / "index.ts").is_file())
            record = json.loads(
                (self.args.out / tree / "sdk-contract.json").read_text()
            )
            self.assertEqual(record["artifact"]["profile"], "debug")
        self.assertFalse((self.args.out / "typescript-napi").exists())

    def test_sequential_host_builds_share_dependencies_and_keep_role_bytes(self):
        artifacts.build(self.args)
        self.assertEqual(
            self.cargo_environments[0]["CARGO_TARGET_DIR"],
            self.cargo_environments[1]["CARGO_TARGET_DIR"],
        )
        record = json.loads((self.args.artifacts / "artifacts.json").read_text())[
            "artifacts"
        ]
        for role in ("native", "bindgen"):
            artifacts.verify(record[role])
            self.assertEqual(record[role]["profile"], "debug")
            self.assertEqual(record[role]["features"], "")
        artifacts.render(self.args)
        self.assertTrue((self.args.out / "typescript-napi/index.ts").is_file())

    def restored_checkout(self):
        first = self.root / "first-checkout"
        inputs = {
            "Cargo.toml": "[workspace]\nmembers=[]\n",
            "crates/fixture/src/lib.rs": "pub fn fixture() {}\n",
            "apps/xmtp_sdk_bindgen/runtime/ts/fixture.ts": "export const marker='first';\n",
            "sdks/node/test.ts": "export const marker='sdk';\n",
        }
        for name, body in inputs.items():
            path = first / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body)
        subprocess.run(["git", "init", "-q", str(first)], check=True)
        self.hash_patch.stop()
        self.args.artifacts = first / "target/sdk-artifacts"
        with patch.object(artifacts, "ROOT", first):
            artifacts.build(self.args)
        second = self.root / "fresh-checkout"
        shutil.copytree(first, second)
        for name in inputs:
            path = second / name
            os.utime(path, (path.stat().st_atime, path.stat().st_mtime + 600))
        shutil.rmtree(first)
        self.args.artifacts = second / "target/sdk-artifacts"
        self.args.out = second / "target/sdk-generated"
        self.calls.clear()
        self.cargo_environments.clear()
        return second

    def test_restored_content_skips_cargo_despite_new_checkout_mtimes_and_sdk_ts(self):
        with patch.object(artifacts, "build_context", return_value="b" * 64):
            checkout = self.restored_checkout()
            (checkout / "sdks/node/test.ts").write_text(
                "export const changed='typescript';\n"
            )
            with patch.object(artifacts, "ROOT", checkout):
                artifacts.build(self.args)
                self.assertEqual(self.cargo_environments, [])
                artifacts.render(self.args)
            record = json.loads((self.args.artifacts / "artifacts.json").read_text())[
                "artifacts"
            ]
            for role in ("native", "bindgen"):
                artifacts.verify(record[role])
                self.assertTrue(
                    all(str(checkout) in path for path in record[role]["files"])
                )
                self.assertEqual(record[role]["profile"], "debug")
                self.assertEqual(record[role]["features"], "")
            self.assertTrue((self.args.out / "typescript-napi/index.ts").is_file())

    def test_restored_cache_rebuilds_changed_rust_content(self):
        with patch.object(artifacts, "build_context", return_value="b" * 64):
            checkout = self.restored_checkout()
            (checkout / "crates/fixture/src/lib.rs").write_text(
                "pub fn changed_rust() {}\n"
            )
            with patch.object(artifacts, "ROOT", checkout):
                artifacts.build(self.args)
            self.assertEqual(len(self.cargo_environments), 2)
            self.assertTrue(all("cargo" in command for command in self.calls))

    def test_generator_only_cache_reuses_library_and_passes_current_source_preflight(
        self,
    ):
        self.args.targets = ("swift", "kotlin", "node")
        product_spec = importlib.util.spec_from_file_location(
            "sdk_products", artifacts.ROOT / "dev/ci/sdk-products.py"
        )
        products = importlib.util.module_from_spec(product_spec)
        product_spec.loader.exec_module(products)
        with patch.object(artifacts, "build_context", return_value="b" * 64):
            checkout = self.restored_checkout()
            native = json.loads((self.args.artifacts / "artifacts.json").read_text())[
                "artifacts"
            ]["native"]
            runtime = checkout / "apps/xmtp_sdk_bindgen/runtime/ts/fixture.ts"
            runtime.write_text("export const changed='generator';\n")
            with patch.object(artifacts, "ROOT", checkout):
                artifacts.build(self.args)
                self.assertEqual(len(self.cargo_environments), 1)
                self.assertIn("xmtp-sdk-bindgen", self.calls[0])
                artifacts.render(self.args)
            with patch.object(products.artifacts, "ROOT", checkout):
                records = products.check_generated(self.args.out, "node")
            actual = records[0]["artifact"]
            self.assertEqual(
                {key: value for key, value in actual.items() if key != "files"},
                {key: value for key, value in native.items() if key != "files"},
            )
            self.assertEqual(
                list(actual["files"].values()), list(native["files"].values())
            )
            self.assertNotEqual(actual["generator"], records[0]["generator"])
            with patch.object(products.artifacts, "ROOT", checkout):
                self.assertEqual(
                    records[0]["generator"], products.artifacts.source_hash(True)
                )

    def test_restored_cache_rejects_current_role_bytes_and_context_tampering(self):
        with patch.object(artifacts, "build_context", return_value="b" * 64):
            checkout = self.restored_checkout()
            index = self.args.artifacts / "artifacts.json"
            record = json.loads(index.read_text())
            library = next(
                (self.args.artifacts / "native").glob("*.dylib"),
                next((self.args.artifacts / "native").glob("*.so"), None),
            )
            before = library.read_bytes()
            library.write_bytes(b"changed restored role bytes")
            with patch.object(artifacts, "ROOT", checkout):
                with self.assertRaisesRegex(ValueError, "artifact mismatch"):
                    artifacts.build(self.args)
            library.write_bytes(before)
            record["artifacts"]["native"]["features"] = "conformance"
            index.write_text(json.dumps(record))
            with patch.object(artifacts, "ROOT", checkout):
                with self.assertRaisesRegex(
                    ValueError, "artifact context mismatch: features"
                ):
                    artifacts.build(self.args)
            self.assertEqual(self.cargo_environments, [])

    def test_windows_build_invokes_cargo_without_the_posix_wrapper(self):
        self.args.skip_bindgen = True
        self.args.rust_target = "x86_64-pc-windows-msvc"

        def windows_cargo(command, **kwargs):
            self.calls.append(command)
            self.cargo_environments.append(dict(kwargs["env"]))
            output = (
                Path(kwargs["env"]["CARGO_TARGET_DIR"])
                / self.args.rust_target
                / "debug"
            )
            output.mkdir(parents=True)
            for name in ("xmtp_sdk.dll", "xmtp_sdk.lib"):
                (output / name).write_text("fixture artifact")

        with (
            patch.dict(os.environ, {"CARGO_BUILD_JOBS": "7"}),
            patch.object(artifacts.sys, "platform", "win32"),
            patch.object(artifacts, "run", side_effect=windows_cargo),
        ):
            artifacts.build(self.args)
        self.assertEqual(self.calls[0][0], "cargo")
        self.assertEqual(self.calls[0].count("--target"), 1)
        self.assertEqual(self.cargo_environments[0]["CARGO_BUILD_JOBS"], "7")
        self.assertEqual(self.cargo_environments[0]["OPENSSL_NO_VENDOR"], "0")
        self.assertEqual(self.cargo_environments[0]["OPENSSL_STATIC"], "1")
        self.assertTrue((self.args.artifacts / "native/xmtp_sdk.dll").is_file())

    def test_cargo_build_preserves_caller_job_count(self):
        with patch.dict(os.environ, {"CARGO_BUILD_JOBS": "7"}):
            artifacts.build(self.args)
        self.assertEqual(len(self.cargo_environments), 2)
        for environment in self.cargo_environments:
            self.assertEqual(environment["CARGO_BUILD_JOBS"], "7")

    def test_cargo_build_leaves_unset_job_count_unset(self):
        with patch.dict(os.environ):
            os.environ.pop("CARGO_BUILD_JOBS", None)
            artifacts.build(self.args)
        self.assertEqual(len(self.cargo_environments), 2)
        for environment in self.cargo_environments:
            self.assertFalse(
                "CARGO_BUILD_JOBS" in environment,
                "Unset Cargo job count must remain unset",
            )

    def test_bindgen_is_reused_across_default_and_conformance_artifacts(self):
        artifacts.build(self.args)
        shared = self.args.artifacts
        self.args.artifacts = self.root / "conformance"
        self.args.features = "conformance"
        self.args.reuse_bindgen = shared
        before = len(self.calls)
        artifacts.build(self.args)
        self.assertEqual(len(self.calls), before + 1)
        self.assertIn("conformance", self.calls[-1])
        self.assertNotIn("xmtp-sdk-bindgen", self.calls[-1])

    def test_render_replaces_stale_files_without_building(self):
        artifacts.build(self.args)
        stale = self.args.out / "typescript-napi/runtime/Stale.ts"
        stale.parent.mkdir(parents=True)
        stale.write_text("stale")
        before = len(self.calls)
        artifacts.render(self.args)
        self.assertFalse(stale.exists())
        self.assertEqual(len(self.calls), before + 1)
        self.assertEqual(self.calls[-1][1], "generate")

    def test_tampered_artifact_rejected_before_render_runs_it(self):
        self.args.targets = ("swift",)
        artifacts.build(self.args)
        for path in ("bindgen/xmtp-sdk-bindgen", "native/libxmtp_sdk.a"):
            with self.subTest(path=path):
                artifact = self.args.artifacts / path
                original = artifact.read_bytes()
                artifact.write_text("altered after build")
                before = len(self.calls)
                with self.assertRaisesRegex(ValueError, "artifact mismatch"):
                    artifacts.render(self.args)
                self.assertEqual(len(self.calls), before)
                self.assertFalse(self.args.out.exists())
                artifact.write_bytes(original)

    def test_stale_node_and_browser_render_rejected_before_generator_runs(self):
        self.args.targets = artifacts.TARGETS
        artifacts.build(self.args)
        for target in ("node", "browser"):
            for changed, message in ((False, "source"), (True, "generator")):
                with (
                    self.subTest(target=target, changed=message),
                    patch.object(
                        artifacts,
                        "source_hash",
                        side_effect=lambda generator=False, changed=changed: (
                            "current" if generator == changed else "fixture-source"
                        ),
                    ),
                ):
                    self.args.targets = (target,)
                    before = len(self.calls)
                    with self.assertRaisesRegex(ValueError, f"{message} mismatch"):
                        artifacts.render(self.args)
                    self.assertEqual(len(self.calls), before)
                    self.assertFalse(self.args.out.exists())

    def tree_bytes(self, root):
        return {
            str(p.relative_to(root)): p.read_bytes()
            for p in root.rglob("*")
            if p.is_file()
        }

    def render_all_targets(self):
        self.args.targets = artifacts.TARGETS
        artifacts.build(self.args)
        artifacts.render(self.args)

    def test_target_render_replaces_selected_and_keeps_unselected_trees(self):
        languages = {
            "swift": {"swift"},
            "kotlin": {"kotlin"},
            "node": {"typescript-napi"},
            "browser": {"typescript-wasm", "typescript-pure"},
        }
        for target, selected in languages.items():
            with self.subTest(target=target):
                self.render_all_targets()
                before = {
                    language: self.tree_bytes(self.args.out / language)
                    for names in languages.values()
                    for language in names
                    if language not in selected
                }
                for language in selected:
                    (self.args.out / language / "stale.txt").write_text("stale")
                calls = len(self.calls)
                self.args.targets = (target,)
                artifacts.render(self.args)
                for language, files in before.items():
                    self.assertEqual(self.tree_bytes(self.args.out / language), files)
                for language in selected:
                    self.assertFalse((self.args.out / language / "stale.txt").exists())
                self.assertFalse(
                    any(command[0] == "dev/agent-run" for command in self.calls[calls:])
                )

    def test_partial_native_render_does_not_need_unselected_wasm_index_entries(self):
        self.render_all_targets()
        before = {
            language: self.tree_bytes(self.args.out / language)
            for language in ("typescript-wasm", "typescript-pure")
        }
        manifest = self.args.artifacts / "artifacts.json"
        record = json.loads(manifest.read_text())
        del record["artifacts"]["wasm"]
        del record["artifacts"]["pure"]
        manifest.write_text(json.dumps(record))
        self.args.targets = ("swift",)
        calls = len(self.calls)
        artifacts.render(self.args)
        for language, files in before.items():
            self.assertEqual(self.tree_bytes(self.args.out / language), files)
        self.assertEqual(len(self.calls), calls + 1)


class NodePlatformReceiptTests(unittest.TestCase):
    def test_final_library_and_addon_bytes_must_match_pair_receipts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            library = root / "libxmtp_sdk.so"
            addon = root / "uniffi-runtime-napi.linux-x64-gnu.node"
            library.write_bytes(b"built SDK library")
            addon.write_bytes(b"built pinned runtime")
            native = root / "native-provenance.json"
            runtime = root / "runtime-provenance.json"
            revision = re.search(
                r'\[workspace\.metadata\.xmtp-sdk-fork\][^\[]*rev\s*=\s*"([0-9a-f]{40})"',
                (artifacts.ROOT / "Cargo.toml").read_text(),
            ).group(1)
            native.write_text(
                json.dumps(
                    {
                        "schema": 1,
                        "source": artifacts.source_hash(),
                        "generator": artifacts.source_hash(True),
                        "target": "x86_64-unknown-linux-gnu",
                        "features": "",
                        "profile": "release",
                        "files": {library.name: artifacts.digest(library)},
                    }
                )
            )
            runtime.write_text(
                json.dumps(
                    {
                        "schema": 1,
                        "target": "x86_64-unknown-linux-gnu",
                        "addon": addon.name,
                        "revision": revision,
                        "files": {addon.name: artifacts.digest(addon)},
                    }
                )
            )
            args = [
                "python3.11",
                str(Path(__file__).with_name("record-node-platform.py")),
                "linux-x64-gnu",
                "--library",
                str(library),
                "--addon",
                str(addon),
                "--native-provenance",
                str(native),
                "--runtime-provenance",
                str(runtime),
                "--out",
                str(root / "out"),
            ]

            def record():
                return subprocess.run(
                    args, cwd=artifacts.ROOT, capture_output=True, text=True
                )

            self.assertEqual(record().returncode, 0)
            library.write_bytes(b"replaced SDK library")
            rejected = record()
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("native library bytes mismatch", rejected.stderr)
            library.write_bytes(b"built SDK library")
            addon.write_bytes(b"replaced runtime addon")
            rejected = record()
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("runtime addon bytes mismatch", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
