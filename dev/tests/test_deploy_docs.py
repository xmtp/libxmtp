#!/usr/bin/env python3
"""Check the docs workflow, its result gate, and the post-merge Pages deploy."""

import json
import os
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = yaml.safe_load((ROOT / ".github/workflows/deploy-docs.yml").read_text())
JOBS = WORKFLOW["jobs"]
POST_MERGE = yaml.safe_load((ROOT / ".github/workflows/post-merge.yml").read_text())[
    "jobs"
]


def gate(results, native):
    step = JOBS["result"]["steps"][0]
    env = {
        "RESULTS": json.dumps({job: {"result": r} for job, r in results.items()}),
        "NATIVE": json.dumps(native),
    }
    return subprocess.run(
        ["bash", "-c", step["run"]],
        env=os.environ | env,
        capture_output=True,
        text=True,
    ).returncode


def passing(**changes):
    results = {job: "success" for job in ("rust", "swift", "kotlin", "site", "compose")}
    return results | changes


class ConcurrencyTests(unittest.TestCase):
    def test_only_the_pages_deploy_is_serialized_and_never_cancelled(self):
        grouped = {job for job, spec in POST_MERGE.items() if "concurrency" in spec}
        self.assertEqual(grouped, {"deploy"})
        # One fixed group for the one Pages site. A pending deploy waits in
        # the queue and is never replaced, and a running one is never
        # cancelled: a cancelled job makes the post-merge run cancelled.
        self.assertEqual(
            POST_MERGE["deploy"]["concurrency"],
            {"group": "pages", "cancel-in-progress": False, "queue": "max"},
        )
        # actions/deploy-pages gives up after 10 minutes by default.
        self.assertGreater(POST_MERGE["deploy"].get("timeout-minutes", 0), 10)


class ResultGateTests(unittest.TestCase):
    """dev/tests/test_workflows.py checks the gate at its strictest."""

    def test_merge_run_needs_no_native_references(self):
        results = passing(swift="skipped", kotlin="skipped")
        self.assertEqual(gate(results, native=False), 0)


if __name__ == "__main__":
    unittest.main()
