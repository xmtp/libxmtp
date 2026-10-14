#!/usr/bin/env python3
"""Check the source package helper with controlled producer and stager commands."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
TOOL = r"""
import hashlib, json, os, pathlib, sys
root = pathlib.Path.cwd()
name = pathlib.Path(sys.argv[0]).name
if name == "node" and sys.argv[1].endswith("check-generated-assets.mjs"):
    # The receipt check is real.
    os.execv(os.environ["REAL_NODE"], ["node", *sys.argv[1:]])
target = sys.argv[1].split()[-1] if name == "nix-shell" else sys.argv[2]
trees = ["typescript-napi"] if target == "node" else ["typescript-wasm", "typescript-pure"]
with (root / "calls.jsonl").open("a") as log:
    log.write(json.dumps([name, sys.argv[1:], os.getenv("NIX_DEVSHELL")]) + "\n")
if name == "nix-shell":
    assert sys.argv[1:] == ["just sdk generate " + target]
    assert os.environ["NIX_DEVSHELL"] == "rust"
    if os.getenv("FAIL_PRODUCER"):
        sys.exit(47)
    generation = os.getenv("GENERATION", "fresh")
    for tree in trees:
        folder = root / "target/sdk-generated" / tree
        folder.mkdir(parents=True, exist_ok=True)
        (folder / "index.ts").write_text(generation)
        digest = hashlib.sha256(generation.encode()).hexdigest()
        (folder / "sdk-contract.json").write_text(
            json.dumps({"generation": generation, "files": {"index.ts": digest}})
        )
else:
    assert sys.argv[1:] == ["crates/xmtp_sdk/dev/stage-package.mjs", target, "--public"]
    generated = pathlib.Path(os.getenv("XMTP_SDK_GENERATED_DIR") or root / "target/sdk-generated")
    marker = (generated / trees[0] / "index.ts").read_text()
    product = root / "target/sdk-packages" / target
    product.mkdir(parents=True, exist_ok=True)
    (product / "index.js").write_text(marker)
    (product / "sdk-contract.json").write_text(marker)
"""


class SourcePackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        (self.root / "dev/js").mkdir(parents=True)
        (self.root / "bin").mkdir()
        for name in ("sdk-package", ".setup"):
            shutil.copyfile(ROOT / "dev/js" / name, self.root / "dev/js" / name)
        (self.root / "crates/xmtp_sdk/dev").mkdir(parents=True)
        shutil.copyfile(
            ROOT / "crates/xmtp_sdk/dev/check-generated-assets.mjs",
            self.root / "crates/xmtp_sdk/dev/check-generated-assets.mjs",
        )
        for name in ("dev/nix-shell", "bin/node"):
            path = self.root / name
            path.write_text(f"#!{sys.executable}\n" + TOOL)
            path.chmod(0o755)
        for target in ("node", "browser"):
            (self.root / "sdks" / target).mkdir(parents=True)
            (self.root / "sdks" / target / "package.json").write_text(
                '{"version": "1"}'
            )
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("XMTP_SDK_")
        }
        self.env.update(
            PATH=str(self.root / "bin") + os.pathsep + os.environ["PATH"],
            REAL_NODE=shutil.which("node"),
            NIX_DEVSHELL="js",
        )

    def run_helper(self, target, **env):
        return subprocess.run(
            ["bash", str(self.root / "dev/js/sdk-package"), target],
            cwd=self.root,
            env={**self.env, **env},
            text=True,
            capture_output=True,
        )

    def calls(self):
        path = self.root / "calls.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()]

    def test_source_builds_generate_and_stage_only_changed_inputs(self):
        for target in ("node", "browser"):
            manifest = self.root / "sdks" / target / "package.json"
            for generation, edit in (
                ("fresh", False),
                ("fresh", False),
                ("fresh", True),
                ("fresh", False),
                ("changed", False),
            ):
                if edit:
                    manifest.write_text('{"version": "2"}')
                result = self.run_helper(target, GENERATION=generation)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    (self.root / "sdks" / target / "dist/index.js").read_text(),
                    generation,
                )
        # A repeated run reuses the staged product; a changed package
        # manifest or generated tree stages again.
        stage, reuse = ["nix-shell", "node"], ["nix-shell"]
        self.assertEqual(
            [call[0] for call in self.calls()],
            (stage + reuse + stage + reuse + stage) * 2,
        )

    def test_explicit_generated_input_reuses_without_a_producer(self):
        supplied = self.root / "supplied"
        for tree in ("typescript-napi", "typescript-wasm", "typescript-pure"):
            (supplied / tree).mkdir(parents=True)
            (supplied / tree / "index.ts").write_text("supplied")
            digest = hashlib.sha256(b"supplied").hexdigest()
            (supplied / tree / "sdk-contract.json").write_text(
                json.dumps({"files": {"index.ts": digest}})
            )
        for target in ("node", "browser"):
            tree = supplied / (
                "typescript-napi" if target == "node" else "typescript-wasm"
            )
            for _ in range(2):
                result = self.run_helper(target, XMTP_SDK_GENERATED_DIR=str(supplied))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    (self.root / "sdks" / target / "dist/index.js").read_text(),
                    (tree / "index.ts").read_text(),
                )
            # One byte of a writable generated file, with its receipt unchanged,
            # stages again (and the stager's own receipt check then rejects it).
            (tree / "index.ts").write_text("Supplied")
            self.run_helper(target, XMTP_SDK_GENERATED_DIR=str(supplied))
            (tree / "index.ts").write_text("supplied")
        # No producer runs; the second run of each target reuses the product.
        self.assertEqual([call[0] for call in self.calls()], ["node", "node"] * 2)

    def test_failed_producer_preserves_dist_and_never_stages(self):
        for target in ("node", "browser"):
            dest = self.root / "sdks" / target / "dist"
            dest.mkdir()
            (dest / "index.js").write_text("prior")
            result = self.run_helper(target, FAIL_PRODUCER="1")
            self.assertEqual(result.returncode, 47, result.stderr)
            self.assertEqual((dest / "index.js").read_text(), "prior")
        self.assertEqual([call[0] for call in self.calls()], ["nix-shell", "nix-shell"])


if __name__ == "__main__":
    unittest.main()
