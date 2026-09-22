"""Test the opt-in cache helper without starting a cache server."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class SccacheEnvTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xmtp sccache tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bash = shutil.which("bash")
        self.binary = self.root / "sccache"
        self.binary.write_text("#!/bin/sh\nexit 99\n")
        self.binary.chmod(0o755)
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("SCCACHE_")
            and key not in ("CARGO_INCREMENTAL", "RUSTC_WRAPPER", "BASH_ENV")
        }
        # No system sccache can be found or contacted by these tests.
        self.env["PATH"] = str(self.root)

    def run_shell(self, command):
        return subprocess.run(
            [self.bash, "-euc", command, "test", str(ROOT / "dev/sccache-env")],
            env=self.env,
            text=True,
            capture_output=True,
        )

    def values(self):
        result = self.run_shell(
            'source "$1" >&2; '
            'printf "%s\\n" "$RUSTC_WRAPPER" "$SCCACHE_DIR" '
            '"$SCCACHE_CACHE_SIZE" "${CARGO_INCREMENTAL-unset}"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result, result.stdout.splitlines()

    def test_defaults_and_wrapper_selection(self):
        self.env["RUSTC_WRAPPER"] = "/previous/wrapper"
        result, values = self.values()
        self.assertEqual(
            values,
            [str(self.binary), f"{self.env['HOME']}/.cache/sccache", "10G", "unset"],
        )
        self.assertIn("requested dir=", result.stderr)
        self.assertIn("running server keeps its cache settings", result.stderr)
        self.assertIn("disable: unset RUSTC_WRAPPER", result.stderr)

    def test_unsets_incremental_override(self):
        for value in ("0", "1", ""):
            with self.subTest(value=value):
                self.env["CARGO_INCREMENTAL"] = value
                _, values = self.values()
                self.assertEqual(values[-1], "unset")

    def test_keeps_explicit_cache_settings(self):
        self.env["SCCACHE_DIR"] = str(self.root / "shared cache")
        self.env["SCCACHE_CACHE_SIZE"] = "64M"
        self.env["SCCACHE_SERVER_UDS"] = str(self.root / "private.sock")
        _, values = self.values()
        self.assertEqual(values[1:3], [self.env["SCCACHE_DIR"], "64M"])
        result = self.run_shell('source "$1" >&2; printf "%s" "$SCCACHE_SERVER_UDS"')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, self.env["SCCACHE_SERVER_UDS"])

    def test_empty_cache_settings_use_defaults(self):
        self.env["SCCACHE_DIR"] = ""
        self.env["SCCACHE_CACHE_SIZE"] = ""
        _, values = self.values()
        self.assertEqual(values[1:3], [f"{self.env['HOME']}/.cache/sccache", "10G"])

    def test_requires_sourcing(self):
        result = subprocess.run(
            [self.bash, str(ROOT / "dev/sccache-env")],
            env=self.env,
            text=True,
            capture_output=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("must be sourced", result.stderr)

    def test_missing_sccache_leaves_environment_unchanged(self):
        self.binary.unlink()
        self.env["CARGO_INCREMENTAL"] = "1"
        self.env["RUSTC_WRAPPER"] = "/previous/wrapper"
        result = self.run_shell(
            'if source "$1"; then exit 98; else status=$?; fi; '
            'printf "%s\\n" "$status" "$RUSTC_WRAPPER" "$CARGO_INCREMENTAL" '
            '"${SCCACHE_DIR-unset}" "${SCCACHE_CACHE_SIZE-unset}"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.splitlines(),
            ["1", "/previous/wrapper", "1", "unset", "unset"],
        )
        self.assertIn("sccache not found", result.stderr)


if __name__ == "__main__":
    unittest.main()
