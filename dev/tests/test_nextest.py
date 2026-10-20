#!/usr/bin/env python3
"""Check the rules of the CI workspace test run: its Cargo profile and its
nextest test groups."""

from pathlib import Path
import re
import shutil
import subprocess
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CiTestProfileTests(unittest.TestCase):
    def test_workspace_tests_build_with_overflow_checks(self):
        # crates/xmtp_common/tests/ci_test_profile.rs checks the profile only
        # when the build names it, so this checks that the build does.
        profile = tomllib.loads((ROOT / "Cargo.toml").read_text())["profile"]["ci-test"]
        self.assertIs(profile["overflow-checks"], True)
        self.assertIs(profile["debug-assertions"], True)
        for attribute in ("CARGO_PROFILE", "cargoArtifacts.CARGO_PROFILE"):
            with self.subTest(attribute=attribute):
                value = subprocess.run(
                    [
                        shutil.which("nix"),
                        "eval",
                        "--raw",
                        f".#nextest.x86_64-linux.{attribute}",
                    ],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout
                self.assertEqual(value, "ci-test")


class ToxiproxyGroupTests(unittest.TestCase):
    def test_every_toxiproxy_test_runs_in_the_serial_group(self):
        # The tests that call toxiproxy_test reset one shared proxy server, so
        # the nextest group must name each of them, and only them.
        config = tomllib.loads((ROOT / ".config/nextest.toml").read_text())
        (override,) = [
            o
            for o in config["profile"]["default"]["overrides"]
            if o.get("test-group") == "toxiproxy"
        ]
        self.assertEqual(config["test-groups"]["toxiproxy"]["max-threads"], 1)
        pattern = re.fullmatch(r"test\(/(.*)/\)", override["filter"])[1]
        sources = subprocess.run(
            ["git", "ls-files", "*.rs"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
        callers = set()
        for name in sources:
            text = (ROOT / name).read_text()
            for call in re.finditer(r"(?<![\w])toxiproxy_test\(", text):
                if text[: call.start()].rstrip().endswith("fn"):
                    continue
                functions = re.findall(r"\bfn ([a-z_0-9]+)", text[: call.start()])
                callers.add(functions[-1])
        self.assertGreaterEqual(len(callers), 6)
        for caller in sorted(callers):
            with self.subTest(caller=caller):
                self.assertRegex(f"module::{caller}", pattern)
        named = set(re.findall(r"[a-z_0-9]+", pattern.split("(", 1)[1]))
        self.assertEqual(named, callers)


if __name__ == "__main__":
    unittest.main()
