#!/usr/bin/env python3
"""Keep cargo-zigbuild's Zig protocol while remapping compiler inputs."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("gnu-native-zig.sh")
REMAP = SCRIPT.with_name("apple-native-tool.sh")
CC_SCRIPT = SCRIPT.with_name("gnu-native-cc.sh")


class GnuNativeZigTests(unittest.TestCase):
    def invoke(self, command, arguments, *, build_root=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture = root / "capture.json"
            for role in ("zig", "cc", "cxx"):
                compiler = root / role
                compiler.write_text(
                    f"#!{sys.executable}\n"
                    "import json, os, pathlib, sys\n"
                    "pathlib.Path(os.environ['CAPTURE']).write_text("
                    "json.dumps([pathlib.Path(__file__).name, *sys.argv[1:]]))\n"
                )
                compiler.chmod(0o755)
            env = dict(
                os.environ,
                CAPTURE=str(capture),
                XMTP_NATIVE_ZIG=str(root / "zig"),
                XMTP_NATIVE_CC=str(root / "cc"),
                XMTP_NATIVE_CXX=str(root / "cxx"),
                XMTP_NATIVE_REMAP_TOOL=str(REMAP),
            )
            env.pop("NIX_BUILD_TOP", None)
            if build_root:
                env["NIX_BUILD_TOP"] = str(root)
            args = [argument.replace("BUILD_ROOT", str(root)) for argument in arguments]
            result = subprocess.run(
                ["bash", str(SCRIPT), command, *args],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            found = json.loads(capture.read_text()) if capture.exists() else None
            return result, found, str(root)

    def test_compiler_frontends_keep_target_args_and_remap_openssl_paths(self):
        for command in ("cc", "c++"):
            with self.subTest(command=command):
                result, found, root = self.invoke(
                    command,
                    [
                        "-target",
                        "x86_64-linux-gnu.2.28",
                        "-c",
                        "source with spaces.c",
                        '-DENGINESDIR="BUILD_ROOT/install/lib/engines-3"',
                        '-DMODULESDIR="BUILD_ROOT/install/lib/ossl-modules"',
                    ],
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    found,
                    [
                        "cc" if command == "cc" else "cxx",
                        f"-ffile-prefix-map={root}=/build",
                        f"-ffile-prefix-map={Path(root).resolve()}=/build",
                        "-target",
                        "x86_64-linux-gnu.2.28",
                        "-c",
                        "source with spaces.c",
                        '-DENGINESDIR="/build/install/lib/engines-3"',
                        '-DMODULESDIR="/build/install/lib/ossl-modules"',
                    ],
                )

    def test_non_compiler_commands_pass_through_without_build_root(self):
        for command, args in (
            ("version", []),
            ("env", []),
            ("ar", ["crs", "archive with spaces.a", "input.o"]),
            ("ranlib", ["archive.a"]),
        ):
            with self.subTest(command=command):
                result, found, _ = self.invoke(command, args, build_root=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(found, ["zig", command, *args])

    def test_compile_without_build_root_fails_before_compiler_runs(self):
        result, found, _ = self.invoke("cc", ["-c", "input.c"], build_root=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Nix must provide its build root", result.stderr)
        self.assertIsNone(found)


class GnuNativeCcTests(unittest.TestCase):
    def test_store_entry_point_forwards_to_selected_target_linker(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture = root / "capture.json"
            # Model cargo-zigbuild's generated linker, including a space in
            # its path. The stable entry point must not reimplement its flags.
            linker = root / "generated linker"
            linker.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "pathlib.Path(os.environ['CAPTURE']).write_text(json.dumps(sys.argv[1:]))\n"
            )
            linker.chmod(0o755)
            arguments = ["-c", "source with spaces.c", "--caller-option"]
            result = subprocess.run(
                ["bash", str(CC_SCRIPT), *arguments],
                env=dict(
                    os.environ,
                    CAPTURE=str(capture),
                    XMTP_NATIVE_LINKER_ENV="CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
                    CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=str(linker),
                ),
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(capture.read_text()), arguments)

    def test_missing_target_linker_fails_closed(self):
        env = dict(os.environ, XMTP_NATIVE_LINKER_ENV="XMTP_TEST_MISSING_LINKER")
        env.pop("XMTP_TEST_MISSING_LINKER", None)
        result = subprocess.run(
            ["bash", str(CC_SCRIPT), "--version"],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
