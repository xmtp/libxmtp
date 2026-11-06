#!/usr/bin/env python3
"""Check the rate limit setting of the shared backend configurations.

Every client of a local or check stack shares one address, so the default
per-caller limits would fail unrelated suites. Those configurations turn rate
limits off. A deployment keeps the default.
"""

from pathlib import Path
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


def rate_limits_enabled(path):
    config = tomllib.loads((ROOT / path).read_text())
    return config.get("rate_limits", {}).get("enabled", True)


class BackendConfigTests(unittest.TestCase):
    def test_local_and_check_stacks_turn_rate_limits_off(self):
        for path in (
            "dev/backend/local.toml",
            "dev/backend/local-s3.toml",
            "dev/tls/backend.toml",
        ):
            with self.subTest(path=path):
                self.assertIs(rate_limits_enabled(path), False)

    def test_the_node_recovery_backend_turns_rate_limits_off(self):
        source = (ROOT / "sdks/node/test/recoveryBackend.ts").read_text()
        self.assertRegex(
            source, re.compile(r"^\[rate_limits\]\nenabled = false$", re.M)
        )

    def test_the_fly_deployment_keeps_the_default(self):
        self.assertIs(rate_limits_enabled("dev/fly/backend/config.toml"), True)


if __name__ == "__main__":
    unittest.main()
