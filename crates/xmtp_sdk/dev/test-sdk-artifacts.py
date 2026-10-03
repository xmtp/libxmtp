#!/usr/bin/env python3
"""Check artifact reuse, early mismatch rejection, and stale output cleanup."""

import argparse
import importlib.util
import json
import os
import signal
import shutil
from pathlib import Path
import tempfile
from unittest.mock import patch
import unittest

spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)

mobile_spec = importlib.util.spec_from_file_location(
    "mobile", Path(__file__).with_name("mobile-package.py")
)
mobile = importlib.util.module_from_spec(mobile_spec)
mobile_spec.loader.exec_module(mobile)


class ArtifactTests(unittest.TestCase):
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

    def test_archive_index_inputs_change_artifact_cache_context(self):
        names = (
            "RANLIB",
            "RANLIBFLAGS",
            "TARGET_RANLIB",
            "TARGET_RANLIBFLAGS",
            "HOST_RANLIB",
            "HOST_RANLIBFLAGS",
        ) + tuple(
            prefix + target
            for prefix in ("RANLIB_", "RANLIBFLAGS_")
            for triple in mobile.ANDROID.values()
            for target in (triple, triple.replace("-", "_"))
        )
        names += tuple(
            "AARCH64_APPLE_DARWIN_OPENSSL_" + key for key in ("LIB_DIR", "INCLUDE_DIR")
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

    def test_compiler_change_rebuilds_artifacts(self):
        artifacts.build(self.args)
        with patch.object(artifacts, "build_context", return_value="changed-compiler"):
            artifacts.build(self.args)
        self.assertEqual(len(self.calls), 4)

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
        self.args.out.mkdir()
        stale = self.args.out / "old/runtime/Stale.swift"
        stale.parent.mkdir(parents=True)
        stale.write_text("stale")
        before = len(self.calls)
        artifacts.render(self.args)
        self.assertFalse(stale.exists())
        self.assertEqual(len(self.calls), before + 1)
        self.assertEqual(self.calls[-1][1], "generate")

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

    def test_target_render_preserves_unselected_valid_trees(self):
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

    def test_partial_render_failure_preserves_all_previous_targets(self):
        self.render_all_targets()
        before = self.tree_bytes(self.args.out)
        self.args.targets = ("swift",)
        with patch.object(
            artifacts, "run", side_effect=RuntimeError("generation failed")
        ):
            with self.assertRaisesRegex(RuntimeError, "generation failed"):
                artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(self.args.out), before)

    def test_partial_render_rejects_stale_unselected_identity_and_bytes(self):
        mutations = (
            "generator",
            "source",
            "features",
            "profile",
            "target",
            "generated",
            "native",
            "extra",
            "link",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                self.render_all_targets()
                tree = self.args.out / "typescript-napi"
                metadata = tree / "sdk-contract.json"
                record = json.loads(metadata.read_text())
                if mutation == "generator":
                    record["generator"] = "old generator"
                elif mutation in ("source", "features", "profile", "target"):
                    record["artifact"][mutation] = "wrong identity"
                elif mutation == "generated":
                    (tree / "index.ts").write_text("tampered generated bytes")
                elif mutation == "native":
                    # Do not corrupt the selected Swift artifact receipt.
                    record["artifact"]["files"] = {
                        str(self.root / "absent-native"): "bad"
                    }
                elif mutation == "extra":
                    (tree / "unexpected.ts").write_text("unrecorded code")
                else:
                    outside = self.root / "unrecorded-runtime"
                    outside.mkdir(exist_ok=True)
                    (outside / "runtime.ts").write_text("unrecorded linked code")
                    (tree / "linked-runtime").symlink_to(
                        outside, target_is_directory=True
                    )
                metadata.write_text(json.dumps(record))
                self.args.targets = ("swift",)
                artifacts.render(self.args)
                self.assertFalse(tree.exists())
                self.assertTrue((self.args.out / "swift/index.ts").exists())

    def test_partial_render_rejects_malformed_unselected_artifact_files(self):
        languages = (
            "swift",
            "kotlin",
            "typescript-napi",
            "typescript-wasm",
            "typescript-pure",
        )
        for language in languages:
            for malformed in (None, [], ["invalid"], {}, "invalid", False):
                with self.subTest(language=language, files=malformed):
                    self.render_all_targets()
                    tree = self.args.out / language
                    metadata = tree / "sdk-contract.json"
                    record = json.loads(metadata.read_text())
                    record["artifact"]["files"] = malformed
                    metadata.write_text(json.dumps(record))
                    rejected = (
                        {"typescript-wasm", "typescript-pure"}
                        if language in ("typescript-wasm", "typescript-pure")
                        else {language}
                    )
                    self.args.targets = ("node" if language == "swift" else "swift",)
                    selected = set(artifacts.target_languages(self.args.targets[0]))
                    preserved = {
                        path.name: self.tree_bytes(path)
                        for path in self.args.out.iterdir()
                        if path.name not in rejected | selected
                    }
                    calls = len(self.calls)
                    artifacts.render(self.args)
                    for name in rejected:
                        self.assertFalse((self.args.out / name).exists())
                    for name in selected:
                        self.assertTrue((self.args.out / name / "index.ts").is_file())
                    for name, files in preserved.items():
                        self.assertEqual(self.tree_bytes(self.args.out / name), files)
                    self.assertFalse(
                        any(
                            command[0] == "dev/agent-run"
                            for command in self.calls[calls:]
                        )
                    )

    def test_malformed_unselected_metadata_preserves_prior_output_on_render_failure(
        self,
    ):
        self.render_all_targets()
        metadata = self.args.out / "typescript-napi/sdk-contract.json"
        record = json.loads(metadata.read_text())
        record["artifact"]["files"] = None
        metadata.write_text(json.dumps(record))
        before = self.tree_bytes(self.args.out)
        self.args.targets = ("swift",)
        with patch.object(
            artifacts, "run", side_effect=RuntimeError("generation failed")
        ):
            with self.assertRaisesRegex(RuntimeError, "generation failed"):
                artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(self.args.out), before)

    def test_partial_render_preserves_reused_native_generator_provenance(self):
        self.render_all_targets()
        tree = self.args.out / "typescript-napi"
        metadata = tree / "sdk-contract.json"
        record = json.loads(metadata.read_text())
        record["artifact"]["generator"] = "legitimate prior native generator"
        metadata.write_text(json.dumps(record))
        before = self.tree_bytes(tree)
        self.args.targets = ("swift",)
        artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(tree), before)

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

    def test_partial_render_preserves_browser_pair_only_with_matching_contracts(self):
        self.render_all_targets()
        pure = self.args.out / "typescript-pure/sdk-contract.json"
        record = json.loads(pure.read_text())
        record["contract"] = "different browser contract"
        pure.write_text(json.dumps(record))
        self.args.targets = ("swift",)
        artifacts.render(self.args)
        self.assertFalse((self.args.out / "typescript-pure").exists())
        self.assertFalse((self.args.out / "typescript-wasm").exists())
        self.assertTrue((self.args.out / "typescript-napi/index.ts").exists())

    def test_render_generation_failure_preserves_previous_tree(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        before = self.tree_bytes(self.args.out)
        with patch.object(
            artifacts, "run", side_effect=RuntimeError("generation failed")
        ):
            with self.assertRaisesRegex(RuntimeError, "generation failed"):
                artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(self.args.out), before)

    def test_render_promotion_failure_preserves_previous_tree(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        before = self.tree_bytes(self.args.out)
        replace = os.replace

        def fail(source, destination):
            if (
                Path(destination) == self.args.out.resolve()
                and Path(source).name != "previous"
            ):
                raise OSError("promotion failed")
            return replace(source, destination)

        with (
            patch.object(artifacts.os, "replace", side_effect=fail),
            patch.object(
                artifacts.shutil, "move", side_effect=OSError("promotion failed")
            ),
        ):
            with self.assertRaisesRegex(OSError, "promotion failed"):
                artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(self.args.out), before)
        self.assertTrue(self.args.out.is_dir())

    def test_render_interruptions_preserve_previous_tree(self):
        replace = os.replace
        for kind in ("sigint", "exit"):
            for phase in ("before-backup", "after-backup", "after-promotion"):
                with self.subTest(kind=kind, phase=phase):
                    artifacts.build(self.args)
                    artifacts.render(self.args)
                    output = self.args.out.resolve()
                    (output / "previous.bin").write_bytes(b"prior valid output")
                    before = self.tree_bytes(output)
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
                    with patch.object(artifacts.os, "replace", side_effect=interrupt):
                        with self.assertRaises(expected) as error:
                            artifacts.render(self.args)
                    self.assertTrue(fired)
                    if kind == "exit":
                        self.assertEqual(error.exception.code, 71)
                    self.assertEqual(self.tree_bytes(output), before)
                    self.assertEqual(
                        list(output.parent.glob(".sdk-render-stage-*")), []
                    )

    def test_render_interrupted_rollback_preserves_original_failure(self):
        replace = os.replace
        for kind in ("sigint", "exit"):
            for phase in ("before-rollback", "after-rollback"):
                with self.subTest(kind=kind, phase=phase):
                    artifacts.build(self.args)
                    artifacts.render(self.args)
                    output = self.args.out.resolve()
                    (output / "previous.bin").write_bytes(b"prior valid output")
                    before = self.tree_bytes(output)

                    def invoke():
                        artifacts.render(self.args)

                    files = self.tree_bytes
                    prefix = ".sdk-render-stage-*"
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
                            error.exception.__notes__[0].split("preserved at ", 1)[1]
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

    def test_render_first_rename_failure_preserves_previous_tree(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        before = self.tree_bytes(self.args.out)
        with patch.object(
            artifacts.os, "replace", side_effect=OSError("backup failed")
        ):
            with self.assertRaisesRegex(OSError, "backup failed"):
                artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(self.args.out), before)

    def test_render_failed_rollback_preserves_named_backup_across_success(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        before = self.tree_bytes(self.args.out)
        replace = os.replace

        def fail(source, destination):
            if Path(source) == self.args.out.resolve():
                return replace(source, destination)
            raise OSError("promotion or rollback failed")

        with patch.object(artifacts.os, "replace", side_effect=fail):
            with self.assertRaisesRegex(
                OSError, "promotion or rollback failed"
            ) as error:
                artifacts.render(self.args)
        backup = Path(error.exception.__notes__[0].split("preserved at ", 1)[1])
        self.assertTrue(backup.is_absolute())
        self.assertEqual(self.tree_bytes(backup), before)
        artifacts.render(self.args)
        self.assertEqual(self.tree_bytes(backup), before)
        restored = self.root / "manual-recovery"
        os.replace(backup, restored)
        self.assertEqual(self.tree_bytes(restored), before)

    def test_render_fresh_promotion_failure_leaves_no_product(self):
        artifacts.build(self.args)
        with (
            patch.object(
                artifacts.os, "replace", side_effect=OSError("promotion failed")
            ),
            patch.object(
                artifacts.shutil, "move", side_effect=OSError("promotion failed")
            ),
        ):
            with self.assertRaisesRegex(OSError, "promotion failed"):
                artifacts.render(self.args)
        self.assertFalse(self.args.out.exists())

    def test_asset_mismatch_rejected_before_generator_runs(self):
        artifacts.build(self.args)
        (self.args.artifacts / "native/libxmtp_sdk.a").write_text("wrong binary")
        before = len(self.calls)
        with self.assertRaisesRegex(ValueError, "artifact mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), before)
        self.assertFalse(self.args.out.exists())

    def test_mixed_source_rejected_before_generator_runs(self):
        artifacts.build(self.args)
        manifest = self.args.artifacts / "artifacts.json"
        record = json.loads(manifest.read_text())
        record["artifacts"]["native"]["source"] = "different-source"
        manifest.write_text(json.dumps(record))
        before = len(self.calls)
        with self.assertRaisesRegex(ValueError, "source contract mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), before)

    def test_generator_mismatch_rejected_before_generator_runs(self):
        artifacts.build(self.args)
        before = len(self.calls)
        manifest = self.args.artifacts / "artifacts.json"
        record = json.loads(manifest.read_text())
        record["artifacts"]["bindgen"]["generator"] = "wrong generator"
        manifest.write_text(json.dumps(record))
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), before)


if __name__ == "__main__":
    unittest.main()
