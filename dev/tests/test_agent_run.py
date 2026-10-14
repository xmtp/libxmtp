"""Check command routing without running a build or starting services."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class AgentRunTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="agent runner ")
        self.addCleanup(self.temp.cleanup)
        self.bin = Path(self.temp.name)
        self.env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}"}
        for name in ("CI", "XMTP_RTK", "XMTP_RTK_NEXTEST"):
            self.env.pop(name, None)
        cargo = self.bin / "cargo"
        cargo.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "print(json.dumps(sys.argv[1:]))\n"
            "sys.exit(int(os.environ.get('TEST_EXIT', '0')))\n"
        )
        cargo.chmod(0o755)
        self.rtk = self.bin / "rtk"
        self.rtk.write_text(
            "#!/usr/bin/env bash\n"
            "echo FILTERED >&2\n"
            'if [[ "${TEST_BLANK:-0}" == 1 ]]; then exit 2; fi\n'
            'exec "$@"\n'
        )
        self.rtk.chmod(0o755)

    def run_command(self, *args):
        return subprocess.run(
            ["bash", str(ROOT / "dev/agent-run"), *args],
            env=self.env,
            capture_output=True,
            text=True,
        )

    def test_default_is_raw(self):
        result = self.run_command("cargo", "check", "--locked")
        self.assertEqual(result.returncode, 0)
        self.assertNotIn("FILTERED", result.stderr)

    def test_filter_preserves_arguments_and_failure(self):
        self.env.update(XMTP_RTK="1", TEST_EXIT="37")
        args = ["clippy", "--locked", "--features", "one two", "--", "-Dwarnings"]
        result = self.run_command("cargo", *args)
        self.assertEqual(json.loads(result.stdout), args)
        self.assertEqual(result.returncode, 37)
        self.assertIn("FILTERED", result.stderr)
        self.assertIn("XMTP_RTK=0", result.stderr)

    def test_ci_and_explicit_bypass(self):
        for settings in ({"XMTP_RTK": "1", "CI": "true"}, {"XMTP_RTK": "0"}):
            with self.subTest(settings=settings):
                self.env.pop("CI", None)
                self.env.update(settings)
                result = self.run_command("cargo", "test")
                self.assertEqual(result.returncode, 0)
                self.assertNotIn("FILTERED", result.stderr)

    def test_structured_and_inspection_modes_are_raw(self):
        self.env["XMTP_RTK"] = "1"
        cases = [
            ["metadata", "--format-version", "1"],
            ["llvm-cov", "nextest", "--no-fail-fast", "--no-report"],
            ["check", "--message-format=json"],
            ["check", "--message-format", "json"],
            ["test", "--", "--format=json"],
            ["test", "--", "--nocapture"],
            ["test", "--list"],
            ["build", "--help"],
        ]
        for args in cases:
            with self.subTest(args=args):
                result = self.run_command("cargo", *args)
                self.assertEqual(json.loads(result.stdout), args)
                self.assertNotIn("FILTERED", result.stderr)

    def test_nextest_requires_separate_opt_in(self):
        self.env["XMTP_RTK"] = "1"
        self.assertNotIn("FILTERED", self.run_command("cargo", "nextest", "run").stderr)
        self.env["XMTP_RTK_NEXTEST"] = "1"
        result = self.run_command("cargo", "nextest", "run", "-E", "test(a) | test(b)")
        self.assertIn("FILTERED", result.stderr)
        self.assertEqual(json.loads(result.stdout)[-1], "test(a) | test(b)")
        self.assertNotIn(
            "FILTERED", self.run_command("cargo", "nextest", "list").stderr
        )

    def test_empty_failure_has_recovery_message(self):
        self.env.update(XMTP_RTK="1", XMTP_RTK_NEXTEST="1", TEST_BLANK="1")
        result = self.run_command("cargo", "nextest", "run")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        self.assertIn("XMTP_RTK=0", result.stderr)

    def test_missing_rtk_runs_once_without_filter(self):
        # A closed PATH makes this independent of a host RTK installation.
        self.rtk.unlink()
        cargo = self.bin / "cargo"
        cargo.write_text("#!/bin/sh\necho raw\nexit 23\n")
        self.env.update(XMTP_RTK="1", PATH=str(self.bin))
        result = subprocess.run(
            ["/bin/bash", str(ROOT / "dev/agent-run"), "cargo", "check"],
            env=self.env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 23)
        self.assertEqual(result.stdout, "raw\n")

    def test_no_command_is_an_error(self):
        self.assertEqual(self.run_command().returncode, 2)
