"""Execute release selection and gates using the workflow's actual scripts."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

from test_react_native_workflow import expression, inner
from test_warm_sets import evaluate

ROOT = Path(__file__).resolve().parents[2]
SDKS = (
    "ios",
    "android",
    "node-sdk",
    "browser-sdk",
    "agent-sdk",
    "react-native-sdk",
    "cli",
)


class SdkReleaseSelectionTest(unittest.TestCase):
    def setUp(self):
        self.workflow = yaml.safe_load(
            (ROOT / ".github/workflows/release.yml").read_text()
        )
        self.jobs = self.workflow["jobs"]

    def resolve(self, kind="dev", **inputs):
        dispatch = self.workflow[True]["workflow_dispatch"]["inputs"]
        values = {
            f"inputs.{name}": spec.get("default", "") for name, spec in dispatch.items()
        }
        values |= {f"inputs.{name}": value for name, value in inputs.items()}
        values |= {"inputs.release-type": kind, "github.ref": "refs/heads/main"}
        step = next(
            step
            for step in self.jobs["validate"]["steps"]
            if step.get("id") == "resolve"
        )
        env = {}
        for name, raw in step["env"].items():
            value = expression(inner(raw), values)
            env[name] = str(value).lower() if isinstance(value, bool) else str(value)
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "output"
            result = subprocess.run(
                ["bash", "-euc", step["run"]],
                env=os.environ | env | {"GITHUB_OUTPUT": str(output)},
                capture_output=True,
                text=True,
            )
            outputs = (
                dict(line.split("=", 1) for line in output.read_text().splitlines())
                if output.exists()
                else {}
            )
        return result, outputs

    def graph(self, selection, outcomes=None, cancelled=False):
        outcomes = outcomes or {}
        results = {"validate": "success"}
        values = {
            f"needs.validate.outputs.{name}": value for name, value in selection.items()
        }

        def needs(name):
            dependencies = self.jobs[name].get("needs", [])
            return [dependencies] if isinstance(dependencies, str) else dependencies

        def ancestors(name):
            direct = needs(name)
            return set(direct).union(*(ancestors(parent) for parent in direct))

        def result(name):
            if name not in results:
                dependencies = {parent: result(parent) for parent in needs(name)}
                succeeded = all(
                    result(parent) == "success" for parent in ancestors(name)
                )
                job = self.jobs[name]
                if not evaluate(
                    job.get("if", "true"),
                    needs=dependencies,
                    values=values,
                    cancelled=cancelled,
                    succeeded=succeeded,
                ):
                    results[name] = "skipped"
                elif name in ("sdk-ready", "sdk-complete"):
                    (step,) = job["steps"]
                    # GitHub supplies these two JSON contexts to the real gate.
                    env = dict(
                        os.environ,
                        SDK_SELECTION=json.dumps(selection),
                        JOB_RESULTS=json.dumps(
                            {
                                key: {"result": value}
                                for key, value in dependencies.items()
                            }
                        ),
                        SDK_JOB_PREFIX=step["env"]["SDK_JOB_PREFIX"],
                    )
                    run = subprocess.run(
                        ["bash", "-euc", step["run"]],
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    results[name] = "success" if run.returncode == 0 else "failure"
                else:
                    results[name] = outcomes.get(name, "success")
            return results[name]

        for name in [
            *("build-" + sdk for sdk in SDKS),
            "sdk-ready",
            *("release-" + sdk for sdk in SDKS),
            "sdk-complete",
        ]:
            result(name)
        return results

    def test_each_product_can_be_released_for_every_release_type(self):
        for kind in ("dev", "rc", "final"):
            for sdk in SDKS:
                with self.subTest(kind=kind, sdk=sdk):
                    run, selection = self.resolve(kind, **{sdk: True})
                    self.assertEqual(run.returncode, 0, run.stderr)
                    expected = (
                        {sdk, "node-sdk"} if sdk in ("agent-sdk", "cli") else {sdk}
                    )
                    self.assertEqual(
                        {name for name in SDKS if selection[name] == "true"}, expected
                    )
                    results = self.graph(selection)
                    for phase in ("build", "release"):
                        self.assertEqual(
                            {
                                name
                                for name in SDKS
                                if results[f"{phase}-{name}"] == "success"
                            },
                            expected,
                        )
                    self.assertEqual(results["sdk-ready"], "success")
                    self.assertEqual(results["sdk-complete"], "success")

    def test_explicit_all_products_and_backend_only_runs(self):
        self.assertNotIn("sdks", self.workflow[True]["workflow_dispatch"]["inputs"])
        for kind in ("dev", "rc", "final"):
            with self.subTest(kind=kind):
                run, selection = self.resolve(kind, **dict.fromkeys(SDKS, True))
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertTrue(all(selection[sdk] == "true" for sdk in SDKS))
                self.assertEqual(set(self.graph(selection).values()), {"success"})
                run, selection = self.resolve(kind, backend=True)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(selection["products-selected"], "false")
                self.assertTrue(
                    all(
                        value == "skipped"
                        for name, value in self.graph(selection).items()
                        if name != "validate"
                    )
                )

    def test_a_selected_build_failure_or_skip_blocks_every_publisher(self):
        _, selection = self.resolve(android=True, cli=True, **{"browser-sdk": True})
        for status in ("failure", "skipped", "cancelled"):
            with self.subTest(status=status):
                results = self.graph(selection, {"build-android": status})
                self.assertEqual(results["sdk-ready"], "failure")
                self.assertTrue(
                    all(results[f"release-{sdk}"] == "skipped" for sdk in SDKS)
                )
                self.assertEqual(results["sdk-complete"], "failure")

    def test_failed_node_publication_blocks_dependents_and_completion(self):
        for dependent in ("agent-sdk", "cli"):
            for status in ("failure", "skipped", "cancelled"):
                with self.subTest(dependent=dependent, status=status):
                    _, selection = self.resolve(**{dependent: True})
                    results = self.graph(selection, {"release-node-sdk": status})
                    self.assertEqual(results[f"release-{dependent}"], "skipped")
                    self.assertEqual(results["sdk-complete"], "failure")

    def test_single_sdk_dry_run_and_cancellation_never_publish(self):
        _, selection = self.resolve(android=True, **{"dry-run": True})
        results = self.graph(selection)
        self.assertEqual(results["build-android"], "success")
        self.assertEqual(results["sdk-ready"], "success")
        self.assertEqual(results["sdk-complete"], "skipped")
        self.assertTrue(all(results[f"release-{sdk}"] == "skipped" for sdk in SDKS))
        _, selection = self.resolve(android=True)
        results = self.graph(selection, cancelled=True)
        self.assertTrue(all(results[f"release-{sdk}"] == "skipped" for sdk in SDKS))


if __name__ == "__main__":
    unittest.main()
