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

    def test_release_hold_precedes_credentials(self):
        for file in ("check-release-push.yml", "push-release-tag.yml"):
            workflow = yaml.safe_load((ROOT / ".github/workflows" / file).read_text())
            job = next(iter(workflow["jobs"].values()))
            self.assertEqual(job["steps"][0]["name"], "Check release hold")
            script = job["steps"][0]["run"]
            for hold, dry_run, expected in (
                ("", "false", 0),
                ("false", "false", 0),
                ("true", "false", 1),
                ("invalid", "false", 1),
            ):
                result = subprocess.run(
                    ["bash", "-euc", script],
                    env=dict(os.environ, HOLD=hold, DRY_RUN=dry_run),
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, expected, result.stderr)
            self.assertLess(
                0,
                next(
                    i
                    for i, step in enumerate(job["steps"])
                    if "actions/create-github-app-token" in step.get("uses", "")
                ),
            )

    def test_direct_publishers_have_release_boundaries(self):
        root = ROOT / ".github/workflows"
        npm = yaml.safe_load((root / "npm-publish.yml").read_text())
        self.assertIn("vars.XMTP_RELEASE_HOLD", npm["jobs"]["publish"]["if"])
        self.assertEqual(
            npm["jobs"]["check-push-permissions"]["uses"],
            "./.github/workflows/check-release-push.yml",
        )
        for file, job in (
            ("release.yml", "validate"),
            ("release-backend.yml", "setup"),
        ):
            workflow = yaml.safe_load((root / file).read_text())
            self.assertEqual(
                workflow["jobs"]["cutover-policy"]["with"]["operation"], "release"
            )
            self.assertEqual(workflow["jobs"][job]["needs"], "cutover-policy")
            self.assertIn(
                "needs.cutover-policy.outputs.allowed == 'true'",
                workflow["jobs"][job]["if"],
            )
        backend = yaml.safe_load((root / "push-backend.yml").read_text())
        self.assertIn(
            "inputs.release-build",
            backend["jobs"]["cutover-policy"]["with"]["operation"],
        )
        self.assertIn(
            "'release'", backend["jobs"]["cutover-policy"]["with"]["operation"]
        )

    def test_nightly_requires_both_latest_normal_checks(self):
        for conclusions, skip in (
            (("success", "success"), "false"),
            (("failure", "success"), "true"),
            (("success", "cancelled"), "true"),
            (("in_progress", "success"), "true"),
            (("missing", "success"), "true"),
            (("wrong_app", "success"), "true"),
            (("older_success", "success"), "true"),
        ):
            with (
                self.subTest(conclusions=conclusions),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                responses = {}
                for name, state in zip(("Test", "Lint"), conclusions):
                    record = {
                        "id": 2,
                        "app": {"id": 15368},
                        "started_at": "2026-10-12T02:00:00Z",
                        "status": "completed",
                        "conclusion": state,
                    }
                    if state == "in_progress":
                        record["status"] = state
                    if state == "wrong_app":
                        record["app"]["id"] = 1
                        record["conclusion"] = "success"
                    records = [] if state == "missing" else [record]
                    if state == "older_success":
                        record["conclusion"] = "failure"
                        records.insert(
                            0,
                            dict(
                                record,
                                id=1,
                                started_at="2026-10-12T01:00:00Z",
                                conclusion="success",
                            ),
                        )
                    responses[name] = {"check_runs": records}
                stub = root / "gh"
                stub.write_text(
                    "#!/usr/bin/env python3\nimport json,sys\nresponses="
                    + repr(responses)
                    + "\nname='Test' if 'check_name=Test' in sys.argv[2] else 'Lint'\nresult={'workflow_runs':[{'id':2,'event':'push','status':'completed','conclusion':'success'}]} if 'actions/workflows' in sys.argv[2] else responses[name]\nprint(json.dumps(result))\n"
                )
                stub.chmod(0o755)
                output = root / "output"
                result = subprocess.run(
                    ["bash", str(ROOT / "dev/release-ci-gate"), SHA],
                    env=dict(
                        os.environ,
                        PATH=str(root) + os.pathsep + os.environ["PATH"],
                        GITHUB_REPOSITORY="fixture/repo",
                        GITHUB_OUTPUT=str(output),
                    ),
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text().strip(), "skip=" + skip)
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/release-gate-plan.yml").read_text()
        )
        self.assertEqual(
            workflow["jobs"]["gate"]["outputs"]["gate-skip"],
            "${{ steps.ci.outputs.skip }}",
        )

    def test_full_ci_rerun_blocks_old_successful_aggregates(self):
        import json

        for state in ("queued", "in_progress", "failure", "missing"):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                output = root / "output"
                runs = (
                    []
                    if state == "missing"
                    else [
                        {
                            "id": 1,
                            "event": "push",
                            "status": "completed",
                            "conclusion": "success",
                        },
                        {
                            "id": 2,
                            "event": "push",
                            "status": state if state != "failure" else "completed",
                            "conclusion": None if state != "failure" else "failure",
                        },
                    ]
                )
                response = {
                    "workflow_runs": runs,
                    "check_runs": [
                        {
                            "id": 1,
                            "app": {"id": 15368},
                            "status": "completed",
                            "conclusion": "success",
                        }
                    ],
                }
                stub = root / "gh"
                stub.write_text(
                    "#!/bin/sh\ncat <<'RESPONSE'\n"
                    + json.dumps(response)
                    + "\nRESPONSE\n"
                )
                stub.chmod(0o755)
                result = subprocess.run(
                    ["bash", str(ROOT / "dev/release-ci-gate"), SHA],
                    env=dict(
                        os.environ,
                        PATH=str(root) + os.pathsep + os.environ["PATH"],
                        GITHUB_REPOSITORY="fixture/repo",
                        GITHUB_OUTPUT=str(output),
                    ),
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text().strip(), "skip=true")

    def test_nightly_keeps_gate_code_and_publication_source_separate(self):
        workflows = ROOT / ".github/workflows"
        plan = yaml.safe_load((workflows / "release-gate-plan.yml").read_text())
        gate = plan["jobs"]["gate"]
        self.assertEqual(gate["steps"][0]["with"]["ref"], "${{ github.sha }}")
        self.assertEqual(
            plan["jobs"]["pending"]["steps"][0]["with"]["ref"],
            "${{ needs.gate.outputs.sha }}",
        )
        release = yaml.safe_load((workflows / "release.yml").read_text())
        for job in (
            "release-ios",
            "release-android",
            "release-node-sdk",
            "release-browser-sdk",
            "release-agent-sdk",
            "release-cli",
        ):
            self.assertIn("needs.plan.outputs.sha", release["jobs"][job]["with"]["ref"])

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
