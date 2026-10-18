#!/usr/bin/env python3
"""Exercise the compiler wrapper's argument and path contracts."""

import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("apple-native-tool.sh")


class AppleNativeToolTests(unittest.TestCase):
    def invoke(self, mode, arguments, *, build_root=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture = root / "capture.json"
            compiler = root / "compiler"
            compiler.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "pathlib.Path(os.environ['CAPTURE']).write_text("
                "json.dumps({'args': sys.argv[1:], 'rustflags': os.environ['RUSTFLAGS'], "
                "'build_root': os.environ['NIX_BUILD_TOP']}))\n"
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
                env["NIX_BUILD_TOP"] = str(root)
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
            arguments
            + [
                f"--remap-path-prefix={found['build_root']}=/build",
                f"--remap-path-prefix={Path(found['build_root']).resolve()}=/build",
            ],
        )
        self.assertEqual(found["rustflags"], "-C opt-level=2 --cfg caller_option")

    def compile_in_independent_roots(self, mode, source_text, extra_args):
        outputs = []
        with tempfile.TemporaryDirectory() as directory:
            for name in ("first-build", "second-build"):
                root = Path(directory) / name
                source_dir = root / "source"
                source_dir.mkdir(parents=True)
                suffix = "rs" if mode == "rustc" else "c" if mode == "cc" else "cpp"
                source = source_dir / f"probe.{suffix}"
                source.write_text(source_text)
                generated = root / "target" / "generated.rs"
                generated.parent.mkdir()
                generated.write_text("pub fn generated() -> &'static str { file!() }\n")
                compiler = shutil.which(
                    {"rustc": "rustc", "cc": "cc", "cxx": "c++"}[mode]
                )
                self.assertIsNotNone(compiler, "run this test in dev/nix-shell")
                output = root / "probe.o"
                env = dict(
                    os.environ,
                    NIX_BUILD_TOP=str(root),
                    OUT_DIR=str(generated.parent),
                    XMTP_NATIVE_CC=compiler,
                    XMTP_NATIVE_CXX=compiler,
                )
                command = ["bash", str(SCRIPT), mode]
                if mode == "rustc":
                    command.append(compiler)
                else:
                    command += [
                        f'-DENGINESDIR="{root}/install/lib/engines-3"',
                        f'-DMODULESDIR="{root}/install/lib/ossl-modules"',
                    ]
                result = subprocess.run(
                    command + extra_args + [str(source), "-o", str(output)],
                    cwd=source_dir,
                    env=env,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                data = output.read_bytes()
                self.assertFalse(
                    str(root).encode() in data, "logical build root leaked into object"
                )
                self.assertFalse(
                    str(root.resolve()).encode() in data,
                    "physical build root leaked into object",
                )
                outputs.append(data)
        self.assertTrue(
            outputs[0] == outputs[1], "transient roots changed compiler output"
        )
        return outputs[0]

    def test_rust_generated_paths_and_debug_info_are_stable(self):
        data = self.compile_in_independent_roots(
            "rustc",
            'include!(concat!(env!("OUT_DIR"), "/generated.rs"));\n'
            "#[unsafe(no_mangle)] pub fn probe() -> &'static str { generated() }\n",
            [
                "--crate-name=path_probe",
                "--crate-type=lib",
                "--edition=2024",
                "--emit=obj",
                "-g",
            ],
        )
        self.assertIn(b"/build/target/generated.rs", data)

    def test_c_and_cxx_file_and_openssl_paths_are_stable(self):
        for mode in ("cc", "cxx"):
            with self.subTest(mode=mode):
                data = self.compile_in_independent_roots(
                    mode,
                    "const char *source_path = __FILE__;\n"
                    "const char *engine_path = ENGINESDIR;\n"
                    "const char *module_path = MODULESDIR;\n",
                    ["-g", "-c"],
                )
                self.assertIn(b"/build/source/probe.", data)
                self.assertIn(b"/build/install/lib/engines-3", data)
                self.assertIn(b"/build/install/lib/ossl-modules", data)

    def test_missing_build_root_fails_before_compiler_runs(self):
        result, found = self.invoke("rustc", ["--version"], build_root=None)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Nix must provide its build root", result.stderr)
        self.assertIsNone(found)


if __name__ == "__main__":
    unittest.main()
