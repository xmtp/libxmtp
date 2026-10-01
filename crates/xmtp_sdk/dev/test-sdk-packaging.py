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

    def native_receipts(self):
        native = json.loads((self.args.artifacts / "artifacts.json").read_text())[
            "artifacts"
        ]["native"]
        native["features"] = ""
        for triple in mobile.IOS:
            folder = self.root / "mobile" / triple
            folder.mkdir(parents=True, exist_ok=True)
            (folder / "artifacts.json").write_text(
                json.dumps({"artifacts": {"native": native}})
            )

    def test_git_and_filtered_source_have_same_fingerprint(self):
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        expected = (artifacts.source_hash(), artifacts.source_hash(True))
        (self.root / ".git").rename(self.root / ".git-hidden")
        self.assertEqual(
            (artifacts.source_hash(), artifacts.source_hash(True)), expected
        )

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
