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
    def test_no_workflow_level_group(self):
        # A dispatch on trunk would share it with the merge run's docs call.
        self.assertNotIn("concurrency", WORKFLOW)

    def test_no_docs_job_is_serialized(self):
        self.assertEqual(
            [job for job, spec in JOBS.items() if "concurrency" in spec], []
        )

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

    def test_deploy_timeout_exceeds_the_deploy_pages_wait(self):
        # actions/deploy-pages gives up after 10 minutes by default.
        self.assertGreater(POST_MERGE["deploy"].get("timeout-minutes", 0), 10)


class ResultGateTests(unittest.TestCase):
    def test_gate_flags_follow_the_job_conditions(self):
        env = JOBS["result"]["steps"][0]["env"]
        # A JSON boolean also when a manual dispatch leaves the input unset.
        self.assertEqual(env["NATIVE"], "${{ inputs.deploy == true }}")
        self.assertEqual(env["RESULTS"], "${{ toJSON(needs) }}")
        self.assertEqual(set(JOBS["result"]["needs"]), set(JOBS) - {"result"})

    def test_warm_run_passes(self):
        self.assertEqual(gate(passing(), native=True), 0)

    def test_merge_run_needs_no_native_references(self):
        results = passing(swift="skipped", kotlin="skipped")
        self.assertEqual(gate(results, native=False), 0)

    def test_every_build_job_must_pass(self):
        for job in ("rust", "site", "compose"):
            for result in ("failure", "cancelled", "skipped"):
                results = passing(**{job: result})
                self.assertEqual(gate(results, native=False), 1, (job, result))

    def test_native_references_must_pass_when_deploying(self):
        for job in ("swift", "kotlin"):
            for result in ("failure", "cancelled", "skipped"):
                results = passing(**{job: result})
                self.assertEqual(gate(results, native=True), 1, (job, result))


if __name__ == "__main__":
    unittest.main()
