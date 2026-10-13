#!/usr/bin/env python3
"""Prove cutover holds and source identity checks without external writes."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[2]
SHA = "a" * 40
OTHER = "b" * 40


class CutoverTest(unittest.TestCase):
    def command(self, script, values, primary=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "output"
            calls = root / "calls"
            for name, value in (
                ("git", values.get("CHECKOUT_SHA", SHA)),
                ("gh", values.get("HEAD_SHA", SHA)),
            ):
                stub = root / name
                stub.write_text(
                    "#!/bin/sh\nprintf '%s\\n' '"
                    + value
                    + "'\nprintf '%s\\n' '"
                    + name
                    + "' >> '"
                    + str(calls)
                    + "'\n"
                )
                stub.chmod(0o755)
            env = dict(
                os.environ,
                PATH=str(root) + os.pathsep + os.environ["PATH"],
                GITHUB_OUTPUT=str(output),
                GITHUB_EVENT_NAME="push",
                GITHUB_REF="refs/heads/self-hosted",
                GITHUB_SHA=SHA,
                GITHUB_REPOSITORY="fixture/repo",
                XMTP_DEPLOYMENT_HOLD="",
                XMTP_RELEASE_HOLD="",
                ACCEPTED_SOURCE="",
                DRY_RUN="false",
            )
            env.update(values)
            args = ["bash", str(ROOT / "dev" / script)]
            if primary is not None:
                args.append(primary)
            result = subprocess.run(args, env=env, capture_output=True, text=True)
            records = (
                dict(line.split("=", 1) for line in output.read_text().splitlines())
                if output.exists()
                else {}
            )
            return (
                result,
                records,
                calls.read_text().splitlines() if calls.exists() else [],
            )

    def test_hold_defaults_and_separate_operations(self):
        for operation, field in (
            ("deployment", "XMTP_DEPLOYMENT_HOLD"),
            ("release", "XMTP_RELEASE_HOLD"),
        ):
            for value, allowed in (("", "true"), ("false", "true"), ("true", "false")):
                with self.subTest(operation=operation, value=value):
                    result, output, calls = self.command(
                        "cutover-policy", {field: value}, operation
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output["allowed"], allowed)
                    self.assertEqual(calls, [])
        result, output, _ = self.command(
            "cutover-policy", {"XMTP_RELEASE_HOLD": "true"}, "deployment"
        )
        self.assertEqual(output["allowed"], "true")
        result, output, _ = self.command(
            "cutover-policy", {"XMTP_DEPLOYMENT_HOLD": "true"}, "release"
        )
        self.assertEqual(output["allowed"], "true")

    def test_invalid_hold_fails_closed(self):
        for value in ("TRUE", "0", " true", "yes"):
            result, output, _ = self.command(
                "cutover-policy", {"XMTP_DEPLOYMENT_HOLD": value}, "deployment"
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("allowed", output)

    def test_release_preview_can_run_while_held(self):
        result, output, _ = self.command(
            "cutover-policy",
            {"XMTP_RELEASE_HOLD": "true", "DRY_RUN": "true"},
            "release",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output["allowed"], "true")

    def test_current_push_deploys_but_superseded_push_does_not(self):
        result, output, _ = self.command("cutover-source", {})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output["deploy"], "true")
        result, output, _ = self.command("cutover-source", {"HEAD_SHA": OTHER})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output["deploy"], "false")

    def test_dispatch_requires_all_source_identities(self):
        values = {"GITHUB_EVENT_NAME": "workflow_dispatch", "ACCEPTED_SOURCE": SHA}
        result, output, _ = self.command("cutover-source", values)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output["deploy"], "true")
        for change in (
            {"ACCEPTED_SOURCE": "a" * 7},
            {"ACCEPTED_SOURCE": OTHER},
            {"GITHUB_REF": "refs/heads/feature"},
            {"CHECKOUT_SHA": OTHER},
            {"HEAD_SHA": OTHER},
        ):
            with self.subTest(change=change):
                result, output, _ = self.command(
                    "cutover-source", dict(values, **change)
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("deploy", output)

    def test_ci_dispatch_and_other_events_only_build(self):
        for values in (
            {"GITHUB_EVENT_NAME": "workflow_dispatch"},
            {"GITHUB_EVENT_NAME": "pull_request"},
            {"GITHUB_REF": "refs/tags/backend-8.0.0"},
            {"GITHUB_REF": "refs/heads/main"},
        ):
            result, output, calls = self.command("cutover-source", values)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output["deploy"], "false")
            self.assertEqual(calls, [])

    def test_deployment_entry_points_use_the_policy(self):
        required = {
            "deploy-docs.yml": ("deploy", "deploy"),
            "deploy-web-chat.yml": ("deploy", "deploy"),
            "deploy-backend-dev.yml": ("deploy", "deploy"),
            "push-backend.yml": ("publish", "allowed"),
            "push-keepalive-probe.yml": ("build", "allowed"),
        }
        for file, (job, output) in required.items():
            with self.subTest(file=file):
                workflow = yaml.safe_load(
                    (ROOT / ".github/workflows" / file).read_text()
                )
                self.assertEqual(
                    workflow["jobs"]["cutover-policy"]["uses"],
                    "./.github/workflows/check-cutover-policy.yml",
                )
                guarded = workflow["jobs"][job]
                self.assertIn("cutover-policy", guarded["needs"])
                self.assertIn(
                    "needs.cutover-policy.outputs." + output + " == 'true'",
                    guarded["if"],
                )
        docs = yaml.safe_load((ROOT / ".github/workflows/deploy-docs.yml").read_text())
        gate = docs["jobs"]["result"]
        self.assertIn("cutover-policy", gate["needs"])
        expression = gate["steps"][0]["env"]["PASSED"]
        self.assertIn("needs.cutover-policy.result == 'success'", expression)
        self.assertIn(
            "needs.cutover-policy.outputs.deploy != 'true' || needs.deploy.result == 'success'",
            expression,
        )
        self.assertIn("needs.compose.result == 'success'", expression)

    def test_keepalive_writers_are_serial_and_recheck_after_build(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/push-keepalive-probe.yml").read_text()
        )
        self.assertEqual(
            workflow["concurrency"],
            {"group": "keepalive-publication", "cancel-in-progress": False},
        )
        triggers = workflow.get("on", workflow.get(True))
        self.assertEqual(set(triggers["push"]), {"branches"})
        job = workflow["jobs"]["build"]
        self.assertIn("needs.cutover-policy.outputs.deploy == 'true'", job["if"])
        steps = job["steps"]
        build = next(i for i, s in enumerate(steps) if s.get("id") == "push")
        current = next(i for i, s in enumerate(steps) if s.get("id") == "current")
        publish = next(
            i
            for i, s in enumerate(steps)
            if s.get("name") == "Publish current image tags"
        )
        self.assertIs(steps[build]["with"]["push"], False)
        self.assertIs(steps[build]["with"]["load"], True)
        self.assertLess(build, current)
        self.assertLess(current, publish)
        self.assertEqual(steps[publish]["if"], "steps.current.outputs.deploy == 'true'")

    def test_backend_release_routes_and_gates_publication(self):
        push = yaml.safe_load((ROOT / ".github/workflows/push-backend.yml").read_text())
        policy = push["jobs"]["cutover-policy"]["with"]
        self.assertEqual(
            policy["operation"],
            "${{ (inputs.release-build || startsWith(github.ref, 'refs/tags/')) "
            "&& 'release' || 'deployment' }}",
        )
        self.assertEqual(policy["dry-run"], "${{ inputs.dry-run == true }}")
        result, output, _ = self.command(
            "cutover-policy",
            {
                "GITHUB_REF": "refs/tags/backend-8.0.0",
                "XMTP_RELEASE_HOLD": "true",
                "XMTP_DEPLOYMENT_HOLD": "false",
            },
            "release",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output["allowed"], "false")
        release = yaml.safe_load(
            (ROOT / ".github/workflows/release-backend.yml").read_text()
        )["jobs"]
        self.assertEqual(release["cutover-policy"]["with"]["operation"], "release")
        self.assertEqual(release["setup"]["needs"], "cutover-policy")
        self.assertEqual(
            release["setup"]["if"], "needs.cutover-policy.outputs.allowed == 'true'"
        )
        self.assertIn("setup", release["publish"]["needs"])
        self.assertEqual(release["publish"].get("if", "success()"), "success()")


if __name__ == "__main__":
    unittest.main()
