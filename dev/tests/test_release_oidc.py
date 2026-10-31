"""Validate nested release permissions and execute the npm boundary commands."""

import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

import yaml

from test_react_native_workflow import expression
import test_sdk_release_selection as selection

ROOT = Path(__file__).resolve().parents[2]
JS = ("node-sdk", "browser-sdk", "agent-sdk", "react-native-sdk", "cli")
READ_ONLY = {"contents": "read"}


def workflow(name):
    return yaml.safe_load((ROOT / ".github/workflows" / name).read_text())


def evaluate(raw, values):
    raw = re.sub(r"^\$\{\{\s*|\s*\}\}$", "", raw.strip())
    return expression(
        " ".join(raw.split()),
        {key: values[key] for key in sorted(values, key=len, reverse=True)},
    )


def expand(raw, values):
    def replace(match):
        value = evaluate(match[1], values)
        return str(value).lower() if isinstance(value, bool) else str(value)

    return re.sub(r"\$\{\{(.*?)\}\}", replace, raw, flags=re.DOTALL)


class ReleaseOidcTest(unittest.TestCase):
    def leaves(self, name, granted, supplied=None):
        document = workflow(name)
        declared = document[True]["workflow_call"]["inputs"]
        if supplied is not None:
            self.assertLessEqual(set(supplied), set(declared), name)
            for key, spec in declared.items():
                if spec.get("required") and "default" not in spec:
                    self.assertIn(key, supplied, f"{name} requires {key}")

        def bounded(requested, parent, where):
            levels = {"none": 0, "read": 1, "write": 2}
            for scope, value in requested.items():
                self.assertLessEqual(
                    levels[value],
                    levels[parent.get(scope, "none")],
                    f"{where} escalates {scope}",
                )
            return requested

        defaults = bounded(document.get("permissions", granted), granted, name)
        leaves = []
        for job_id, job in document["jobs"].items():
            effective = bounded(
                job.get("permissions", defaults), granted, f"{name}/{job_id}"
            )
            called = job.get("uses", "")
            if called.startswith("./.github/workflows/"):
                leaves.extend(
                    self.leaves(Path(called).name, effective, job.get("with", {}))
                )
            else:
                leaves.append((f"{name}/{job_id}", effective))
        return leaves

    def test_build_and_preparation_calls_validate_with_read_only_permissions(self):
        jobs = workflow("release.yml")["jobs"]
        for sdk in JS:
            with self.subTest(sdk=sdk):
                caller = jobs["build-" + sdk]
                self.assertEqual(caller["permissions"], READ_ONLY)
                leaves = self.leaves(
                    Path(caller["uses"]).name, READ_ONLY, caller["with"]
                )
                self.assertTrue(leaves)
                for name, permissions in leaves:
                    self.assertEqual(permissions, READ_ONLY, name)
        self.leaves("npm-prepare.yml", READ_ONLY)

    def test_real_publisher_is_reachable_only_after_all_six_and_non_dry(self):
        graph = selection.SdkReleaseSelectionTest()
        graph.setUp()
        _, selected = graph.resolve(**{sdk: True for sdk in selection.SDKS})
        publishers = workflow("release.yml")["jobs"]
        child = workflow("npm-publish.yml")["jobs"]["publish"]
        for dry in (False, True):
            for failed in (None, *selection.SDKS):
                for status in (
                    ("failure", "skipped", "cancelled") if failed else ("success",)
                ):
                    with self.subTest(dry=dry, failed=failed, status=status):
                        results = graph.graph(
                            selected | {"dry-run": str(dry).lower()},
                            {"build-" + failed: status} if failed else {},
                        )
                        for sdk in JS:
                            caller = publishers["release-" + sdk]
                            self.assertEqual(
                                caller["uses"], "./.github/workflows/npm-publish.yml"
                            )
                            leaves = self.leaves(
                                "npm-publish.yml", caller["permissions"], caller["with"]
                            )
                            self.assertEqual(
                                [
                                    name
                                    for name, grants in leaves
                                    if grants.get("id-token") == "write"
                                ],
                                ["npm-publish.yml/publish"],
                            )
                            reachable = results["release-" + sdk] == "success"
                            self.assertEqual(reachable, not dry and failed is None)
        for dry in (False, True):
            for prepare in (False, True):
                self.assertEqual(
                    evaluate(
                        child.get("if", "true"),
                        {"inputs.dry-run": dry, "inputs.prepare-only": prepare},
                    ),
                    not dry and not prepare,
                )

    def test_preparation_and_preview_execute_without_real_publication(self):
        steps = workflow("npm-prepare.yml")["jobs"]["prepare"]["steps"]
        for dry, prepare in (
            (False, False),
            (True, False),
            (False, True),
            (True, True),
        ):
            with (
                self.subTest(dry=dry, prepare=prepare),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                (root / "dev").mkdir()
                for helper in ("sdk_version.py", "publish_npm.py"):
                    shutil.copyfile(ROOT / "dev" / helper, root / "dev" / helper)
                package = root / "fixture.tgz"
                with tarfile.open(package, "w:gz") as archive:
                    data = json.dumps(
                        {"name": "@xmtp/node-sdk", "version": "8.0.0-rc5"}
                    ).encode()
                    info = tarfile.TarInfo("package/package.json")
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))
                plan = root / "sdk-release-plan/release-plan.json"
                plan.parent.mkdir()
                plan.write_text(
                    json.dumps(
                        dict(
                            sourceSha="a" * 40,
                            targetVersion="8.0.0",
                            version="8.0.0-rc5",
                            kind="rc",
                            rcNumber=5,
                            timestamp="20261021153000",
                        )
                    )
                )
                for name, script in {
                    "dev/nix-shell": f'#!{sys.executable}\nimport os,sys\ni=sys.argv.index("--command")+1\nos.execvp(sys.argv[i],sys.argv[i:])\n',
                    "pnpm": f'#!{sys.executable}\nimport os,shutil,sys\nfrom pathlib import Path\np=Path(sys.argv[sys.argv.index("--pack-destination")+1]);shutil.copyfile(os.environ["FIXTURE"],p/"package.tgz")\n',
                    "npm": f'#!{sys.executable}\nimport json,os,sys\nwith open(os.environ["CALLS"],"a") as log: log.write(json.dumps(sys.argv[1:])+"\\n")\n',
                }.items():
                    command = root / name
                    command.write_text(script)
                    command.chmod(0o755)
                values = {
                    "inputs." + key: spec.get("default", "")
                    for key, spec in workflow("npm-prepare.yml")[True]["workflow_call"][
                        "inputs"
                    ].items()
                }
                values |= {
                    "inputs.dry-run": dry,
                    "inputs.prepare-only": prepare,
                    "inputs.sdk": "node-sdk",
                    "inputs.version": "8.0.0-rc5",
                    "inputs.package-dir": "package",
                }
                calls = root / "calls"
                envfile, output = root / "env", root / "output"
                env = os.environ | {
                    "PATH": str(root) + os.pathsep + os.environ["PATH"],
                    "RUNNER_TEMP": str(root),
                    "GITHUB_ENV": str(envfile),
                    "GITHUB_OUTPUT": str(output),
                    "FIXTURE": str(package),
                    "CALLS": str(calls),
                }
                for step in steps:
                    if "run" not in step or not evaluate(
                        step.get("if", "true"), values
                    ):
                        continue
                    result = subprocess.run(
                        ["bash", "-euc", expand(step["run"], values)],
                        cwd=root,
                        env=env
                        | {
                            key: expand(value, values)
                            for key, value in step.get("env", {}).items()
                        },
                        capture_output=True,
                        text=True,
                    )
                    if not dry and not prepare:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertIn("all-six", result.stdout)
                        break
                    self.assertEqual(
                        result.returncode, 0, result.stdout + result.stderr
                    )
                    if envfile.exists():
                        env |= dict(
                            line.split("=", 1)
                            for line in envfile.read_text().splitlines()
                        )
                    if step.get("id") and output.exists():
                        values |= {
                            f"steps.{step['id']}.outputs.{key}": value
                            for key, value in (
                                line.split("=", 1)
                                for line in output.read_text().splitlines()
                            )
                        }
                events = (
                    [json.loads(line) for line in calls.read_text().splitlines()]
                    if calls.exists()
                    else []
                )
                self.assertEqual(len(events), int(dry))
                for event in events:
                    self.assertEqual(event[0], "publish")
                    self.assertIn("--dry-run", event)
                retained = root / "checked-npm/package.tgz"
                self.assertEqual(retained.exists(), prepare)
                if prepare:
                    self.assertEqual(retained.read_bytes(), package.read_bytes())
                    receipt = json.loads(
                        (retained.parent / "release-product.json").read_text()
                    )
                    self.assertEqual(receipt["release"], json.loads(plan.read_text()))

    def test_publisher_requires_retained_bytes_before_checkout(self):
        job = workflow("npm-publish.yml")["jobs"]["publish"]
        guard = next(
            step
            for step in job["steps"]
            if step.get("name") == "Require retained checked bytes"
        )
        self.assertLess(
            job["steps"].index(guard),
            next(i for i, step in enumerate(job["steps"]) if "uses" in step),
        )
        for prepared in (False, True):
            result = subprocess.run(
                ["bash", "-euc", guard["run"]],
                env=os.environ | {"PREPARED": str(prepared).lower()},
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode == 0, prepared)
            if not prepared:
                self.assertIn("retained checked package", result.stdout)


if __name__ == "__main__":
    unittest.main()
