"""Check which installed-consumer phases XMTP_RN_JOURNEYS selects.

Post-merge and release runs set `full`; pull requests leave it unset. These
cases fail if run.py stops reading the variable or stops requiring the
runtime reload for `full`.
"""

import importlib.util
import os
from pathlib import Path
import sys
import unittest
from unittest import mock

CONSUMERS = Path(__file__).resolve().parents[1] / "test/consumers"
sys.path.insert(0, str(CONSUMERS))
spec = importlib.util.spec_from_file_location("rn_consumers_run", CONSUMERS / "run.py")
run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run)

PR_PHASES = ["foundation", "content", "boundary", "listener", "react"]


class JourneySetTests(unittest.TestCase):
    def phases(self, value):
        environment = {k: v for k, v in os.environ.items() if k != "XMTP_RN_JOURNEYS"}
        if value is not None:
            environment["XMTP_RN_JOURNEYS"] = value
        with mock.patch.dict(os.environ, environment, clear=True):
            return run.journey_set(), run.required_phases("conformance")

    def test_unset_and_empty_select_the_pull_request_set(self):
        for value in (None, ""):
            with self.subTest(value=value):
                self.assertEqual(self.phases(value), ("pr", PR_PHASES))

    def test_pr_selects_the_pull_request_set(self):
        self.assertEqual(self.phases("pr"), ("pr", PR_PHASES))

    def test_full_adds_the_runtime_reload(self):
        self.assertEqual(self.phases("full"), ("full", [*PR_PHASES, "runtime-reload"]))

    def test_an_unknown_value_is_an_error(self):
        with self.assertRaisesRegex(ValueError, "must be pr or full"):
            self.phases("bogus")

    def test_other_modes_do_not_depend_on_the_set(self):
        with mock.patch.dict(os.environ, {"XMTP_RN_JOURNEYS": "full"}):
            self.assertEqual(run.required_phases("foundation"), ["foundation"])
            self.assertEqual(run.required_phases("background"), ["cold-background"])


if __name__ == "__main__":
    unittest.main()
