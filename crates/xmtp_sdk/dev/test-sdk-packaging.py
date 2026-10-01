#!/usr/bin/env python3
"""Check config provenance, mobile features, and Android target tools."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import subprocess
from unittest.mock import patch
import unittest


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


artifacts = load("artifacts", "sdk-artifacts.py")
receipt = load("receipt", "record-generated.py")
mobile = load("mobile", "mobile-package.py")


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "crates/xmtp_sdk").mkdir(parents=True)
        (self.root / "apps/xmtp_sdk_bindgen").mkdir(parents=True)
        (self.root / "Cargo.toml").write_text("fixture manifest")
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text(
            "fixture template"
        )
        self.config = self.root / "crates/xmtp_sdk/uniffi.toml"
        self.config.write_text("fixture configuration")
        self.calls = []
        self.patches = [
            patch.object(artifacts, "ROOT", self.root),
            patch.object(receipt.artifacts, "ROOT", self.root),
            patch.object(artifacts, "build_context", return_value="fixture compiler"),
            patch.object(artifacts, "run", side_effect=self.command),
        ]
        for item in self.patches:
            item.start()
        self.args = argparse.Namespace(
            artifacts=self.root / "compiled",
            targets=("swift",),
            out=self.root / "generated",
            features="",
            rust_target="",
            skip_bindgen=False,
            reuse_bindgen=None,
            profile="release",
            no_format=True,
        )

    def tearDown(self):
        for item in reversed(self.patches):
            item.stop()
        self.temporary.cleanup()

    def test_bridge_fixture_builds_use_rust_shell(self):
        source = Path(__file__).with_name("run-bridge-conformance").read_text()
        names = ("prepare-bridge-panic-fixture", "prepare-pure-codec-fixture")
        commands = [
            line
            for line in source.splitlines()
            if any("bash crates/xmtp_sdk/dev/" + name in line for name in names)
        ]
        self.assertEqual(len(commands), 2)
        wrapper = self.root / "dev/nix-shell"
        wrapper.parent.mkdir(parents=True)
        wrapper.write_text(
            '#!/usr/bin/env bash\nset -euo pipefail\nexport XMTP_DEV_SHELL="$NIX_DEVSHELL"\nexec bash -euc "$1"\n'
        )
        wrapper.chmod(0o755)
        trace = self.root / "fixture-shells.txt"
        for name in names:
            fixture = self.root / "crates/xmtp_sdk/dev" / name
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(
                'printf "%s:%s\\n" "'
                + name
                + '" "$XMTP_DEV_SHELL" >> "$SDK_SHELL_TRACE"\n'
            )
        for command in commands:
            subprocess.run(
                ["bash", "-euc", command],
                cwd=self.root,
                check=True,
                env=dict(
                    os.environ,
                    XMTP_DEV_SHELL="js",
                    NIX_DEVSHELL="js",
                    SDK_SHELL_TRACE=str(trace),
                ),
            )
        self.assertEqual(
            trace.read_text().splitlines(), [name + ":rust" for name in names]
        )

    def command(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "dev/agent-run":
            folder = Path(kwargs["env"]["CARGO_TARGET_DIR"]) / (
                "debug" if "xmtp-sdk-bindgen" in command else "release"
            )
            folder.mkdir(parents=True, exist_ok=True)
            for name in (
                "libxmtp_sdk.a",
                "libxmtp_sdk.dylib",
                "libxmtp_sdk.so",
                "xmtp-sdk-bindgen",
            ):
                (folder / name).write_text("fixture binary")
        else:
            folder = Path(command[command.index("--out") + 1])
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "xmtp_sdk.swift").write_text("fixture binding")

    def native_receipts(self, target="ios"):
        native = json.loads((self.args.artifacts / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        native["features"] = ""
        for triple in mobile.IOS if target == "ios" else tuple(mobile.ANDROID.values()):
            folder = self.root / "mobile" / triple
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "artifacts.json").write_text(
                json.dumps({"artifacts": {"native": dict(native, target=triple)}})
            )

    def test_live_compile_inputs_invalidate_build_and_recorded_source(self):
        for name in artifacts.COMPILE_INPUTS:
            with self.subTest(input=name):
                path = self.root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("original embedded data")
                artifacts.build(self.args)
                artifacts.render(self.args)
                before = artifacts.source_hash()
                binaries = {
                    role: Path(next(iter(item["files"])))
                    for role, item in json.loads(
                        (self.args.artifacts / "artifacts.json").read_text()
                    )["artifacts"].items()
                }
                receipt.record(self.args.out, binaries)
                old_record = json.loads(
                    (self.args.out / "swift/sdk-contract.json").read_text()
                )
                before_calls = len(self.calls)
                path.write_text("changed embedded data only")
                self.assertNotEqual(artifacts.source_hash(), before)
                with self.assertRaisesRegex(ValueError, "source contract mismatch"):
                    artifacts.render(self.args)
                self.assertEqual(len(self.calls), before_calls)
                receipt.record(self.args.out, binaries)
                new_record = json.loads(
                    (self.args.out / "swift/sdk-contract.json").read_text()
                )
                self.assertNotEqual(
                    new_record["artifact"]["source"], old_record["artifact"]["source"]
                )
                artifacts.build(self.args)
                self.assertEqual(len(self.calls), before_calls + 2)
                path.write_text("original embedded data")
                artifacts.build(self.args)
                artifacts.render(self.args)
                self.assertEqual(artifacts.source_hash(), before)
                print(
                    "Embedded input changed source/build admission and restored:", name
                )

    def test_swapped_mobile_targets_rejected_before_assembly(self):
        for target in ("android", "ios"):
            self.args.targets = ("kotlin",) if target == "android" else ("swift",)
            artifacts.build(self.args)
            artifacts.render(self.args)
            self.native_receipts(target)
            triples = (
                tuple(mobile.ANDROID.values()) if target == "android" else mobile.IOS
            )
            for triple in triples:
                wrong = triples[0] if triple != triples[0] else triples[-1]
                folder = self.root / "mobile" / triple / "artifacts.json"
                original = folder.read_text()
                folder.write_text(
                    (self.root / "mobile" / wrong / "artifacts.json").read_text()
                )
                with self.subTest(platform=target, expected=triple, wrong=wrong):
                    with (
                        patch.object(
                            mobile.sys,
                            "argv",
                            [
                                "mobile-package.py",
                                "stage",
                                target,
                                "--generated",
                                str(self.args.out),
                                "--artifacts",
                                str(self.root / "mobile"),
                                "--out",
                                str(self.root / "products"),
                            ],
                        ),
                        patch.object(mobile, "run") as assembly,
                    ):
                        with self.assertRaisesRegex(
                            ValueError, "mobile binding contract mismatch"
                        ):
                            mobile.main()
                        assembly.assert_not_called()
                    self.assertFalse((self.root / "products").exists())
                folder.write_text(original)
                mobile.preflight(self.args.out, self.root / "mobile", target)
                print(
                    "Swapped target rejected before assembly; restored:", target, triple
                )

    def test_git_and_filtered_source_have_same_fingerprint(self):
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        expected = (artifacts.source_hash(), artifacts.source_hash(True))
        (self.root / ".git").rename(self.root / ".git-hidden")
        self.assertEqual(
            (artifacts.source_hash(), artifacts.source_hash(True)), expected
        )

    def test_unqualified_compiler_inputs_change_native_cache_admission(self):
        names = ("CC", "CXX", "AR", "CFLAGS", "CXXFLAGS", "LDFLAGS")
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

    def test_generator_only_change_reuses_verified_native_provenance(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        manifest = self.args.artifacts / "artifacts.json"
        native = json.loads(manifest.read_text())["artifacts"]["native"]
        source = artifacts.source_hash()
        (self.root / "apps/xmtp_sdk_bindgen/template.txt").write_text(
            "changed generator only"
        )
        self.assertEqual(artifacts.source_hash(), source)
        calls = len(self.calls)
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), calls)
        artifacts.build(self.args)
        self.assertEqual(len(self.calls), calls + 1)
        self.assertIn("xmtp-sdk-bindgen", self.calls[-1])
        self.assertEqual(
            json.loads(manifest.read_text())["artifacts"]["native"], native
        )
        artifacts.render(self.args)
        binding = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        self.assertEqual(binding["artifact"], native)
        self.assertEqual(binding["generator"], artifacts.source_hash(True))
        library = Path(next(iter(native["files"])))
        original = library.read_bytes()
        library.write_bytes(original + b"tampered native")
        calls = len(self.calls)
        with self.assertRaisesRegex(ValueError, "artifact mismatch"):
            artifacts.render(self.args)
        self.assertEqual(len(self.calls), calls)
        library.write_bytes(original)
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
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        artifacts.build(self.args)
        artifacts.render(self.args)

    def test_config_only_change_invalidates_cli_and_recorded_contract(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.native_receipts()
        original = self.config.read_text()
        before = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        binaries = {
            role: Path(next(iter(item["files"])))
            for role, item in json.loads(
                (self.args.artifacts / "artifacts.json").read_text()
            )["artifacts"].items()
        }
        receipt.record(self.args.out, binaries)
        recorded_before = json.loads(
            (self.args.out / "swift/sdk-contract.json").read_text()
        )
        self.config.write_text("changed configuration only")
        with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
            artifacts.render(self.args)
        receipt.record(self.args.out, binaries)
        recorded_after = json.loads(
            (self.args.out / "swift/sdk-contract.json").read_text()
        )
        print(
            "Config-only contract before:",
            recorded_before["contract"],
            "after:",
            recorded_after["contract"],
        )
        self.assertNotEqual(recorded_before["contract"], recorded_after["contract"])
        self.assertNotEqual(before["generator"], recorded_after["generator"])
        with self.assertRaisesRegex(ValueError, "mobile binding contract mismatch"):
            mobile.preflight(self.args.out, self.root / "mobile", "ios")
        self.config.write_text(original)
        artifacts.render(self.args)
        mobile.preflight(self.args.out, self.root / "mobile", "ios")

    def test_both_receipt_producers_pass_mobile_preflight(self):
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.native_receipts()
        mobile.preflight(self.args.out, self.root / "mobile", "ios")
        binaries = {
            role: Path(next(iter(item["files"])))
            for role, item in json.loads(
                (self.args.artifacts / "artifacts.json").read_text()
            )["artifacts"].items()
        }
        receipt.record(self.args.out, binaries)
        mobile.preflight(self.args.out, self.root / "mobile", "ios")
        recorded = json.loads((self.args.out / "swift/sdk-contract.json").read_text())
        self.assertEqual(recorded["artifact"]["source"], artifacts.source_hash())

    def test_conformance_bindings_rejected_before_mobile_assembly(self):
        self.args.features = "conformance"
        artifacts.build(self.args)
        artifacts.render(self.args)
        self.native_receipts()
        with self.assertRaisesRegex(ValueError, "mobile binding feature mismatch"):
            mobile.preflight(self.args.out, self.root / "mobile", "ios")
        self.assertFalse((self.root / "products").exists())
        self.args.features = ""
        artifacts.build(self.args)
        artifacts.render(self.args)
        mobile.preflight(self.args.out, self.root / "mobile", "ios")

    def test_android_build_selects_all_four_ndk_target_tools(self):
        tools = self.root / "ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
        tools.mkdir(parents=True)
        (tools / "llvm-ar").write_text("archiver")
        for triple in mobile.ANDROID.values():
            target = (
                "armv7a-linux-androideabi"
                if triple == "armv7-linux-androideabi"
                else triple
            )
            for suffix in ("23-clang", "23-clang++"):
                (tools / (target + suffix)).write_text("compiler")
        calls = []
        with (
            patch.dict(os.environ, {"ANDROID_NDK_HOME": str(self.root / "ndk")}),
            patch.object(mobile.sys, "platform", "linux"),
            patch.object(
                mobile.sys,
                "argv",
                [
                    "mobile-package.py",
                    "build",
                    "android",
                    "--artifacts",
                    str(self.root / "android"),
                ],
            ),
            patch.object(
                mobile,
                "run",
                side_effect=lambda command, **kw: calls.append((command, kw)),
            ),
        ):
            mobile.main()
        self.assertEqual(len(calls), 4)
        self.assertEqual(
            {command[command.index("--rust-target") + 1] for command, _ in calls},
            set(mobile.ANDROID.values()),
        )
        for command, kwargs in calls:
            triple = command[command.index("--rust-target") + 1]
            target = triple.replace("-", "_")
            env = kwargs["env"]
            linker = env["CARGO_TARGET_" + target.upper() + "_LINKER"]
            self.assertEqual(linker, env["CC_" + target])
            self.assertTrue(linker.endswith("23-clang"))
            self.assertTrue(env["CXX_" + target].endswith("23-clang++"))
            self.assertEqual(env["AR_" + target], str(tools / "llvm-ar"))
            self.assertEqual(command[-1], str(self.root / "android" / triple))


if __name__ == "__main__":
    unittest.main()
