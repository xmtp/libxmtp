"""SDK publishers are internal to the all-six release orchestrator."""

import os
from pathlib import Path
import subprocess
import unittest

import yaml

from test_react_native_workflow import expression, inner

ROOT = Path(__file__).resolve().parents[2]


class ReleaseEntrypointTest(unittest.TestCase):
    def guards(self):
        for file, job in (
            ("check-release-push", "verify"),
            ("npm-publish", "publish"),
            ("push-release-tag", "push"),
        ):
            workflow = yaml.safe_load(
                (ROOT / f".github/workflows/{file}.yml").read_text()
            )
            steps = workflow["jobs"][job]["steps"]
            guard = next(
                (
                    step
                    for step in steps
                    if step.get("name") == "Require the all-SDK release orchestrator"
                ),
                None,
            )
            self.assertIsNotNone(guard, f"{file} admits a standalone publisher")
            self.assertEqual(
                guard["env"]["CALLER_WORKFLOW_REF"], "${{ github.workflow_ref }}"
            )
            self.assertLess(
                steps.index(guard),
                next(i for i, step in enumerate(steps) if "uses" in step),
            )
            yield file, guard

    def test_real_publication_requires_the_barrier_owning_workflow(self):
        for file, guard in self.guards():
            for workflow, allowed in (
                ("xmtp/libxmtp/.github/workflows/release.yml@refs/heads/main", True),
                (
                    "xmtp/libxmtp/.github/workflows/release.yml@refs/heads/release/8.0.0",
                    True,
                ),
                (
                    "xmtp/libxmtp/.github/workflows/release-node-sdk.yml@refs/heads/main",
                    False,
                ),
                (
                    "xmtp/libxmtp/.github/workflows/standalone.yml@refs/heads/main",
                    False,
                ),
                ("other/repo/.github/workflows/release.yml@refs/heads/main", False),
                ("", False),
            ):
                with self.subTest(file=file, workflow=workflow):
                    result = subprocess.run(
                        ["bash", "-euc", guard["run"]],
                        env=dict(
                            os.environ,
                            GITHUB_REPOSITORY="xmtp/libxmtp",
                            CALLER_WORKFLOW_REF=workflow,
                        ),
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode == 0, allowed, result.stderr)

    def test_dry_builds_need_no_publication_origin_but_real_publishers_do(self):
        for file, guard in self.guards():
            for dry, prepare in (
                (False, False),
                (True, False),
                (True, True),
                (False, True),
            ):
                with self.subTest(file=file, dry=dry, prepare=prepare):
                    actual = "if" not in guard or expression(
                        inner(guard["if"]),
                        {"inputs.dry-run": dry, "inputs.prepare-only": prepare},
                    )
                    self.assertEqual(
                        actual,
                        file == "push-release-tag"
                        or (not dry and (file != "npm-publish" or not prepare)),
                    )


if __name__ == "__main__":
    unittest.main()
