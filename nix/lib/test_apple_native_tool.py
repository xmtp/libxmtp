#!/usr/bin/env python3
"""Exercise the compiler wrapper's argument and path contracts."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("apple-native-tool.sh")


class AppleNativeToolTests(unittest.TestCase):
    def invoke(self, mode, arguments, *, build_root="/tmp/nix-build-123"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture = root / "capture.json"
            compiler = root / "compiler"
            compiler.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "pathlib.Path(os.environ['CAPTURE']).write_text("
                "json.dumps({'args': sys.argv[1:], 'rustflags': os.environ['RUSTFLAGS']}))\n"
            )
            compiler.chmod(0o755)
            env = dict(
                os.environ,
                CAPTURE=str(capture),
                XMTP_NATIVE_CC=str(compiler),
                XMTP_NATIVE_CXX=str(compiler),
                RUSTFLAGS="-C opt-level=2 --cfg caller_option",
            )
            env.pop("NIX_BUILD_TOP", None)
            if build_root is not None:
                env["NIX_BUILD_TOP"] = build_root
            command = ["bash", str(SCRIPT), mode]
            if mode == "rustc":
                command.append(str(compiler))
            result = subprocess.run(
                command + arguments,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            found = json.loads(capture.read_text()) if capture.exists() else None
            return result, found

    def test_rust_args_and_caller_flags_survive_runtime_remapping(self):
        arguments = ["--crate-name", "proof", "source with spaces.rs", "--cfg=caller"]
        result, found = self.invoke("rustc", arguments)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            found["args"],
            arguments + ["--remap-path-prefix=/tmp/nix-build-123=/build"],
        )
        self.assertEqual(found["rustflags"], "-C opt-level=2 --cfg caller_option")

    def test_c_and_cxx_remap_only_configured_openssl_paths(self):
        arguments = [
            "-c",
            "source with spaces.c",
            '-DENGINESDIR="/tmp/nix-build-123/install/lib/engines-3"',
            '-DMODULESDIR="/tmp/nix-build-123/install/lib/ossl-modules"',
            '-DOTHER="/tmp/nix-build-123/keep"',
            "--sysroot=/Applications/Xcode.app/Contents/Developer/SDK",
        ]
        for mode in ("cc", "cxx"):
            with self.subTest(mode=mode):
                result, found = self.invoke(mode, arguments)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    found["args"],
                    ["-ffile-prefix-map=/tmp/nix-build-123=/build"]
                    + arguments[:2]
                    + [
                        '-DENGINESDIR="/build/install/lib/engines-3"',
                        '-DMODULESDIR="/build/install/lib/ossl-modules"',
                    ]
                    + arguments[4:],
                )

    def test_missing_build_root_fails_before_compiler_runs(self):
        result, found = self.invoke("rustc", ["--version"], build_root=None)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Nix must provide its build root", result.stderr)
        self.assertIsNone(found)


if __name__ == "__main__":
    unittest.main()
