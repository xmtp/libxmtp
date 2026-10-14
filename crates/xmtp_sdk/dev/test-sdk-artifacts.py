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

    def test_compiler_change_rebuilds_artifacts(self):
        artifacts.build(self.args)
        with patch.object(artifacts, "build_context", return_value="changed-compiler"):
            artifacts.build(self.args)
        self.assertEqual(len(self.calls), 4)

    def test_nix_bindgen_replaces_the_cargo_build(self):
        self.route_patch.stop()
        with patch.object(artifacts, "bindgen_route", return_value="nix"):
            artifacts.build(self.args)
            artifacts.render(self.args)
        self.route_patch.start()
        self.assertEqual(self.calls[0][0], "dev/agent-run")
        self.assertNotIn("xmtp-sdk-bindgen", self.calls[0])
        self.assertEqual(self.calls[1][-1], ".#xmtp-sdk-bindgen")
        self.assertEqual(
            self.calls[2][0],
            str(self.args.artifacts.resolve() / "bindgen-nix/bin/xmtp-sdk-bindgen"),
        )

    def test_browser_only_builds_skip_the_host_library_and_share_a_wasm_target(self):
        self.args.targets = ("browser",)
        artifacts.build(self.args)
        cargo = [command for command in self.calls if command[0] == "dev/agent-run"]
        self.assertEqual(
            sorted(command[command.index("-p") + 1] for command in cargo),
            ["xmtp-sdk-bindgen", "xmtp_sdk", "xmtp_sdk"],
        )
        self.assertTrue(all("--target" in command for command in cargo[1:]))
        wasm = {env["CARGO_TARGET_DIR"] for env in self.cargo_environments[1:]}
        self.assertEqual(wasm, {str(self.args.artifacts.resolve() / "build/wasm")})
        artifacts.render(self.args)
        self.assertTrue((self.args.out / "typescript-pure/index.ts").is_file())

    def test_bindgen_route_follows_the_bindgen_closure(self):
        self.route_patch.stop()
        self.addCleanup(self.route_patch.start)
        root = self.root / "repo"
        (root / "apps/xmtp_sdk_bindgen").mkdir(parents=True)
        (root / "crates/xmtp_mls").mkdir(parents=True)
        for name in ("apps/xmtp_sdk_bindgen/lib.rs", "crates/xmtp_mls/lib.rs"):
            (root / name).write_text("fn main() {}")

        def git(*args):
            subprocess.run(
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
                cwd=root,
                check=True,
                capture_output=True,
            )

        git("init", "-q")
        git("add", "-A")
        git("commit", "-q", "--no-verify", "--no-gpg-sign", "-m", "trunk")
        git("update-ref", "refs/remotes/origin/self-hosted", "HEAD")
        environment = {
            k: v for k, v in os.environ.items() if k not in ("CI", "XMTP_SDK_BINDGEN")
        }
        with (
            patch.object(artifacts, "ROOT", root),
            patch.object(
                artifacts, "closure_paths", return_value=["apps/xmtp_sdk_bindgen"]
            ),
            patch.dict(os.environ, environment, clear=True),
        ):
            self.assertEqual(artifacts.bindgen_route(), "nix")
            # An edit outside the bindgen's crates keeps the Nix bindgen.
            (root / "crates/xmtp_mls/lib.rs").write_text("// edit")
            self.assertEqual(artifacts.bindgen_route(), "nix")
            for edit in (
                "apps/xmtp_sdk_bindgen/lib.rs",
                "apps/xmtp_sdk_bindgen/new.rs",
                "Cargo.lock",
            ):
                with self.subTest(edit=edit):
                    (root / edit).write_text("// edit")
                    self.assertEqual(artifacts.bindgen_route(), "cargo")
                    git("checkout", "-q", "--", ".")
                    git("clean", "-qfd", "apps", "Cargo.lock")
            self.assertEqual(artifacts.bindgen_route(), "nix")
            for name, value, route in (
                ("CI", "true", "cargo"),
                ("XMTP_SDK_BINDGEN", "cargo", "cargo"),
            ):
                with self.subTest(variable=name), patch.dict(os.environ, {name: value}):
                    self.assertEqual(artifacts.bindgen_route(), route)
            git("update-ref", "-d", "refs/remotes/origin/self-hosted")
            self.assertEqual(artifacts.bindgen_route(), "cargo")
            with patch.dict(os.environ, {"XMTP_SDK_BINDGEN": "nix"}):
                self.assertEqual(artifacts.bindgen_route(), "nix")

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

    def test_only_typescript_renders_get_the_formatter(self):
        self.render_all_targets()
        renders = {
            command[command.index("--target") + 1]: command
            for command in self.calls
            if command[1:2] == ["render"]
        }
        for target, command in renders.items():
            with self.subTest(target=target):
                self.assertEqual(
                    "--oxfmt" in command, target not in ("swift", "kotlin")
                )

    def test_typescript_trees_link_their_runtimes(self):
        self.render_all_targets()
        for language, names in (
            ("typescript-napi", ["core", "node"]),
            ("typescript-wasm", ["core", "wasm"]),
            ("typescript-pure", ["core", "wasm"]),
            ("swift", []),
            ("kotlin", []),
        ):
            with self.subTest(language=language):
                packages = self.args.out / language / "node_modules/@ubjs"
                links = sorted(packages.iterdir()) if packages.exists() else []
                self.assertEqual([link.name for link in links], names)
                for link in links:
                    self.assertEqual(link.readlink(), self.root / "store" / link.name)

    def test_formatter_is_the_shell_oxfmt_or_the_flake_nixpkgs_oxfmt(self):
        self.formatter_patch.stop()
        try:
            with (
                patch.dict(os.environ, {"XMTP_SDK_OXFMT": "/shell/oxfmt"}),
                patch.object(artifacts.subprocess, "check_output") as probe,
            ):
                self.assertEqual(artifacts.formatter(), [])
                probe.assert_not_called()
            environment = {
                key: value
                for key, value in os.environ.items()
                if key != "XMTP_SDK_OXFMT"
            }
            with (
                patch.dict(os.environ, environment, clear=True),
                patch.object(
                    artifacts.subprocess,
                    "check_output",
                    return_value="/nix/store/fixture-oxfmt\n",
                ),
            ):
                self.assertEqual(
                    artifacts.formatter(),
                    ["--oxfmt", "/nix/store/fixture-oxfmt/bin/oxfmt"],
                )
        finally:
            self.formatter_patch.start()

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
