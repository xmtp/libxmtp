#!/usr/bin/env python3
"""Check the docs workflow concurrency and its result gate."""

import json
import os
from pathlib import Path
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = yaml.safe_load((ROOT / ".github/workflows/deploy-docs.yml").read_text())
JOBS = WORKFLOW["jobs"]


def gate(results, native, deploy):
    step = JOBS["result"]["steps"][0]
    env = {
        "RESULTS": json.dumps({job: {"result": r} for job, r in results.items()}),
        "NATIVE": json.dumps(native),
        "DEPLOY": json.dumps(deploy),
    }
    return subprocess.run(
        ["bash", "-c", step["run"]],
        env=os.environ | env,
        capture_output=True,
        text=True,
    ).returncode


def passing(**changes):
    results = {
        job: "success"
        for job in ("rust", "swift", "kotlin", "site", "compose", "deploy")
    }
    return results | changes


class ConcurrencyTests(unittest.TestCase):
    def test_no_workflow_level_group(self):
        # A dispatch on trunk would share it with the merge run's docs call.
        self.assertNotIn("concurrency", WORKFLOW)

    def test_only_the_pages_deploy_is_serialized_and_never_cancelled(self):
        grouped = {job for job, spec in JOBS.items() if "concurrency" in spec}
        self.assertEqual(grouped, {"deploy"})
        # One fixed group for the one Pages site. A pending deploy waits in
        # the queue and is never replaced, and a running one is never
        # cancelled: a cancelled job fails the caller's suite.
        self.assertEqual(
            JOBS["deploy"]["concurrency"],
            {"group": "pages", "cancel-in-progress": False, "queue": "max"},
        )

    def test_deploy_timeout_exceeds_the_deploy_pages_wait(self):
        # actions/deploy-pages gives up after 10 minutes by default.
        self.assertGreater(JOBS["deploy"].get("timeout-minutes", 0), 10)


class ResultGateTests(unittest.TestCase):
    def test_gate_flags_follow_the_job_conditions(self):
        env = JOBS["result"]["steps"][0]["env"]
        self.assertEqual(env["DEPLOY"], "${{ " + JOBS["deploy"]["if"] + " }}")
        self.assertEqual(env["NATIVE"], "${{ " + JOBS["swift"]["if"] + " }}")
        self.assertEqual(env["NATIVE"], "${{ " + JOBS["kotlin"]["if"] + " }}")
        self.assertEqual(env["RESULTS"], "${{ toJSON(needs) }}")
        self.assertEqual(set(JOBS["result"]["needs"]), set(JOBS) - {"result"})

    def test_merge_run_passes(self):
        self.assertEqual(gate(passing(), native=True, deploy=True), 0)

    def test_dispatch_or_main_push_passes_without_a_deploy(self):
        # ci.yml workflow_dispatch runs and pushes to main build the native
        # references but do not deploy.
        results = passing(deploy="skipped")
        self.assertEqual(gate(results, native=True, deploy=False), 0)

    def test_failed_skipped_or_cancelled_deploy_fails_a_deploying_run(self):
        for result in ("failure", "skipped", "cancelled"):
            results = passing(deploy=result)
            self.assertEqual(gate(results, native=True, deploy=True), 1, result)

    def test_pull_request_needs_no_native_references_or_deploy(self):
        results = passing(swift="skipped", kotlin="skipped", deploy="skipped")
        self.assertEqual(gate(results, native=False, deploy=False), 0)

    def test_every_build_job_must_pass(self):
        for job in ("rust", "site", "compose"):
            for result in ("failure", "cancelled", "skipped"):
                results = passing(**{job: result})
                self.assertEqual(
                    gate(results, native=False, deploy=False), 1, (job, result)
                )

    def test_native_references_must_pass_outside_pull_requests(self):
        for job in ("swift", "kotlin"):
            for result in ("failure", "cancelled", "skipped"):
                results = passing(**{job: result})
                self.assertEqual(
                    gate(results, native=True, deploy=False), 1, (job, result)
                )


if __name__ == "__main__":
    unittest.main()
