#!/usr/bin/env python3
"""Check artifact reuse, early mismatch rejection, and stale output cleanup."""

import argparse
import importlib.util
import json
import os
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

    def tree_bytes(self, root):
        return {
            str(p.relative_to(root)): p.read_bytes()
            for p in root.rglob("*")
            if p.is_file()
        }

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
                OSError, "previous product preserved at"
            ) as error:
                artifacts.render(self.args)
        backup = Path(str(error.exception).split("preserved at ", 1)[1])
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
