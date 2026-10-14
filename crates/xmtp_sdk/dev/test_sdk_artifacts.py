#!/usr/bin/env python3
"""Check artifact reuse, render input checks, toolchain inputs, and stale output cleanup."""

import argparse
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
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
        self.cargo_environments = []
        self.args = argparse.Namespace(
            artifacts=self.root / "artifacts",
            targets=("node",),
            out=self.root / "generated",
            rust_target="",
            skip_bindgen=False,
            profile="debug",
        )
        # The cargo bindgen unless a test selects the Nix one.
        self.route_patch = patch.object(
            artifacts, "bindgen_route", return_value="cargo"
        )
        self.route_patch.start()
        self.hash_patch = patch.object(
            artifacts, "source_hash", return_value="fixture-source"
        )
        self.hash_patch.start()
        self.render_hash_patch = patch.object(
            artifacts, "render_hash", return_value="fixture-render"
        )
        self.render_hash_patch.start()
        self.context_patch = patch.object(
            artifacts, "build_context", return_value="fixture-compiler"
        )
        self.context_patch.start()
        self.run_patch = patch.object(artifacts, "run", side_effect=self.command)
        self.run_patch.start()
        self.formatter_patch = patch.object(
            artifacts, "formatter", return_value=["--oxfmt", "fixture-oxfmt"]
        )
        self.formatter_patch.start()
        self.runtime_patch = patch.object(
            artifacts,
            "runtime_packages",
            side_effect=lambda names: {
                name: self.root / "store" / name for name in names
            },
        )
        self.runtime_patch.start()

    def tearDown(self):
        self.route_patch.stop()
        self.runtime_patch.stop()
        self.formatter_patch.stop()
        self.run_patch.stop()
        self.render_hash_patch.stop()
        self.hash_patch.stop()
        self.context_patch.stop()
        self.temporary.cleanup()

    def command(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "nix":
            binary = Path(command[command.index("--out-link") + 1]) / "bin"
            binary.mkdir(parents=True, exist_ok=True)
            (binary / "xmtp-sdk-bindgen").write_text("fixture Nix bindgen")
        elif command[0] == "dev/agent-run":
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

    def test_compiler_change_rebuilds_artifacts(self):
        artifacts.build(self.args)
        with patch.object(artifacts, "build_context", return_value="changed-compiler"):
            artifacts.build(self.args)
        self.assertEqual(len(self.calls), 4)

    def test_render_replaces_stale_files_without_building(self):
        artifacts.build(self.args)
        stale = self.args.out / "typescript-napi/runtime/Stale.ts"
        stale.parent.mkdir(parents=True)
        stale.write_text("stale")
        before = len(self.calls)
        artifacts.render(self.args)
        self.assertFalse(stale.exists())
        self.assertEqual(len(self.calls), before + 1)
        self.assertEqual(self.calls[-1][1], "render")

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
                        "source": "closure",
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
            pair = json.loads(
                (root / "out/linux-x64-gnu/sdk-node-platform.json").read_text()
            )
            self.assertEqual(pair["source"], "closure")
            self.assertNotIn("commit", pair)
            # A cargo pair names the commit of its build record instead.
            index = root / "artifacts.json"
            cargo = args[: args.index("--native-provenance")] + [
                "--native-artifacts",
                str(index),
                *args[args.index("--runtime-provenance") :],
            ]
            native_record = {
                "source": "commit:abc123",
                "target": "x86_64-unknown-linux-gnu",
                "features": "",
                "profile": "release",
                "files": {str(library.resolve()): artifacts.digest(library)},
            }
            index.write_text(json.dumps({"artifacts": {"native": native_record}}))
            missing_key = subprocess.run(
                cargo, cwd=artifacts.ROOT, capture_output=True, text=True
            )
            self.assertIn("needs --source-key", missing_key.stderr)
            key = root / "source-key.json"
            key.write_text(
                json.dumps({"source": "tree:x", "key": "k", "forkRevision": revision})
            )
            cargo += ["--source-key", str(key)]
            built = subprocess.run(
                cargo, cwd=artifacts.ROOT, capture_output=True, text=True
            )
            self.assertEqual(built.returncode, 0, built.stderr)
            pair = json.loads(
                (root / "out/linux-x64-gnu/sdk-node-platform.json").read_text()
            )
            self.assertEqual(pair["commit"], "abc123")
            self.assertEqual((pair["source"], pair["sourceKey"]), ("tree:x", "k"))
            index.write_text(
                json.dumps(
                    {"artifacts": {"native": dict(native_record, source="closure")}}
                )
            )
            rejected = subprocess.run(
                cargo, cwd=artifacts.ROOT, capture_output=True, text=True
            )
            self.assertIn("does not name its commit", rejected.stderr)
            index.write_text(
                json.dumps(
                    {
                        "artifacts": {
                            "native": dict(
                                native_record, source="commit:abc123+dirty:fixture"
                            )
                        }
                    }
                )
            )
            rejected = subprocess.run(
                cargo, cwd=artifacts.ROOT, capture_output=True, text=True
            )
            self.assertIn("has uncommitted changes", rejected.stderr)
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
