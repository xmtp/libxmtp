#!/usr/bin/env python3
"""Check the source package helper with controlled producer and stager commands."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOL = r"""
import json, os, pathlib, sys
root = pathlib.Path.cwd()
name = pathlib.Path(sys.argv[0]).name
target = sys.argv[1].split()[-1] if name == "nix-shell" else sys.argv[2]
with (root / "calls.jsonl").open("a") as log:
    log.write(json.dumps([name, sys.argv[1:], os.getenv("NIX_DEVSHELL")]) + "\n")
if name in ("nix-shell", "generate"):
    if name == "nix-shell":
        assert sys.argv[1:] == ["just sdk generate " + target]
        assert os.environ["NIX_DEVSHELL"] == "rust"
    else:
        assert sys.argv[1:] == ["--targets", target]
    if os.getenv("FAIL_PRODUCER"):
        sys.exit(47)
    (root / ("generated-" + target)).write_text(name)
else:
    assert sys.argv[1:] == ["crates/xmtp_sdk/dev/stage-package.mjs", target, "--public"]
    marker = root / ("generated-" + target)
    product = root / "target/sdk-packages" / target
    product.mkdir(parents=True, exist_ok=True)
    (product / "index.js").write_text(marker.read_text())
"""


class SourcePackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        (self.root / "dev/js").mkdir(parents=True)
        (self.root / "bin").mkdir()
        shutil.copyfile(ROOT / "dev/js/sdk-package", self.root / "dev/js/sdk-package")
        (self.root / "crates/xmtp_sdk/dev").mkdir(parents=True)
        for name in ("dev/nix-shell", "bin/node", "crates/xmtp_sdk/dev/generate"):
            path = self.root / name
            path.write_text(f"#!{sys.executable}\n" + TOOL)
            path.chmod(0o755)
        for target in ("node", "browser"):
            (self.root / "sdks" / target).mkdir(parents=True)
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("XMTP_SDK_")
        }
        self.env.update(
            PATH=str(self.root / "bin") + os.pathsep + os.environ["PATH"],
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

    def test_fresh_and_repeated_source_builds_generate_before_staging(self):
        for target in ("node", "browser"):
            for _ in range(2):
                result = self.run_helper(target)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    (self.root / "sdks" / target / "dist/index.js").read_text(),
                    "nix-shell",
                )
        self.assertEqual([call[0] for call in self.calls()], ["nix-shell", "node"] * 4)

    def test_nix_route_generates_without_a_rust_shell(self):
        for target in ("node", "browser"):
            result = self.run_helper(target, XMTP_SDK_BUILD="nix")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                (self.root / "sdks" / target / "dist/index.js").read_text(), "generate"
            )
        self.assertEqual([call[0] for call in self.calls()], ["generate", "node"] * 2)

    def test_supplied_generated_input_is_refused(self):
        for target in ("node", "browser"):
            result = self.run_helper(target, XMTP_SDK_GENERATED_DIR=str(self.root))
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("unset XMTP_SDK_GENERATED_DIR", result.stderr)
        self.assertFalse((self.root / "calls.jsonl").exists())

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
