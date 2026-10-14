#!/usr/bin/env python3
"""Check the actual emulator selector before any AVD or download runs."""

import os
from pathlib import Path
import subprocess
import unittest


class EmulatorPlatformTests(unittest.TestCase):
    def select(self, api=None, supported="1"):
        env = dict(
            os.environ,
            ANDROID_DEFAULT_EMULATOR_API="34",
            ANDROID_API23_SUPPORTED=supported,
        )
        env.pop("NIX_ANDROID_EMULATOR_API", None)
        if api is not None:
            env["NIX_ANDROID_EMULATOR_API"] = api
        return subprocess.run(
            [
                "bash",
                "-ec",
                'source "$1"; printf "%s" "$ANDROID_EMULATOR_API"',
                "selector",
                str(Path(__file__).with_name("android-emulator-platform.sh")),
            ],
            env=env,
            capture_output=True,
            text=True,
        )

    def test_default_and_named_minimum_selection(self):
        self.assertEqual(self.select().stdout, "34")
        selected = self.select("23")
        self.assertEqual(selected.returncode, 0, selected.stderr)
        self.assertEqual(selected.stdout, "23")

    def test_invalid_or_unavailable_minimum_fails_before_avd_creation(self):
        for api, supported in [("23", "0"), ("24", "1"), ("23;false", "1")]:
            rejected = self.select(api, supported)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertEqual(rejected.stdout, "")
            self.assertIn("emulator", rejected.stderr)


if __name__ == "__main__":
    unittest.main()
