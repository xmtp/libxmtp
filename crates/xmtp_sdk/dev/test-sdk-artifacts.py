#!/usr/bin/env python3
"""Check artifact reuse, early mismatch rejection, and stale output cleanup."""

import argparse
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import unittest

spec = importlib.util.spec_from_file_location(
    "artifacts", Path(__file__).with_name("sdk-artifacts.py")
)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.calls = []
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
            out = Path(kwargs["env"]["CARGO_TARGET_DIR"]) / "debug"
            out.mkdir(parents=True, exist_ok=True)
            names = (
                ["xmtp-sdk-bindgen"]
                if "xmtp-sdk-bindgen" in command
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
        with patch.object(artifacts, "source_hash", return_value="wrong generator"):
            with self.assertRaisesRegex(ValueError, "generator contract mismatch"):
                artifacts.render(self.args)
        self.assertEqual(len(self.calls), before)


if __name__ == "__main__":
    unittest.main()
