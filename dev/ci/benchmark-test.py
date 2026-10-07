#!/usr/bin/env python3
"""Exercise real overlay commits and reject source or graph forgery."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(
    os.environ.get(
        "BENCHMARK_OVERLAY_SCRIPT", Path(__file__).with_name("benchmark-overlay.py")
    )
)
spec = importlib.util.spec_from_file_location("overlay", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class OverlayTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.repo = self.root / "source"
        self.repo.mkdir()
        self.git("init")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@invalid.local")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "core.hooksPath", "/dev/null")
        self.write("crates/application/src/lib.rs", "pub fn source() -> u32 { 7 }\n")
        self.write("sdks/node/src/runtime.ts", "export const runtime = 7;\n")
        self.write(
            "package.json",
            json.dumps(
                {
                    "scripts": {"test": "vitest run"},
                    "dependencies": {"runtime": "1.0.0"},
                }
            ),
        )
        self.write("pnpm-workspace.yaml", "tasks:\n  test:\n    dependsOn: []\n")
        self.write(
            ".github/workflows/test.yml",
            "name: Test\non:\n  push:\n    branches: [self-hosted]\n  pull_request:\njobs:\n  unit:\n    runs-on: ubuntu-latest\n    strategy:\n      fail-fast: false\n    steps:\n      - uses: actions/checkout@v6\n      - run: python3 -c 'print(7)'\n",
        )
        self.write(
            ".github/workflows/test-sdk.yml",
            "name: SDK\non:\n  workflow_call:\njobs:\n  sdk:\n    runs-on: ubuntu-latest\n    steps:\n      - run: old-retired-facade\n  swift:\n    runs-on: ubuntu-latest\n    steps:\n      - run: old-retired-swift\n  android-stage:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v6\n      - run: real-android-stage\n",
        )
        self.source = self.commit("source")
        self.old = self.source
        self.write("dev/ci/check-gate.py", "print('real candidate helper')\n")
        self.write(
            "package.json",
            json.dumps(
                {
                    "scripts": {"test": "vitest run", "lint:prepared": "oxlint ."},
                    "dependencies": {"runtime": "999.0.0"},
                }
            ),
        )
        self.write(
            "pnpm-workspace.yaml",
            "tasks:\n  test:\n    dependsOn: []\n  lint:prepared:\n    dependsOn: []\n",
        )
        self.candidate = self.commit("graph")
        self.output = self.root / "export"
        result = subprocess.run(
            [
                sys.executable,
                "-B",
                str(SCRIPT),
                "materialize",
                "--repo",
                str(self.repo),
                "--source",
                self.source,
                "--old",
                self.old,
                "--candidate",
                self.candidate,
                "--output",
                str(self.output),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.bare = self.output / "replay.git"
        self.manifest = json.loads((self.output / "candidate.json").read_text())

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args, repo=None, input=None, env=None):
        return subprocess.check_output(
            ["git", "-C", str(repo or self.repo), *args],
            text=True,
            input=input,
            env=env,
            stderr=subprocess.PIPE,
        ).strip()

    def write(self, name, value):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def altered_commit(self, path, text):
        env = dict(
            os.environ,
            GIT_INDEX_FILE=str(self.root / "mutation.index"),
            GIT_WORK_TREE=str(self.root),
            GIT_AUTHOR_NAME="Fixture",
            GIT_AUTHOR_EMAIL="fixture@invalid.local",
            GIT_COMMITTER_NAME="Fixture",
            GIT_COMMITTER_EMAIL="fixture@invalid.local",
        )
        self.git("read-tree", self.manifest["checkout_sha"], repo=self.bare, env=env)
        blob = self.git("hash-object", "-w", "--stdin", repo=self.bare, input=text)
        self.git(
            "update-index",
            "--add",
            "--cacheinfo",
            f"100644,{blob},{path}",
            repo=self.bare,
            env=env,
        )
        tree = self.git("write-tree", repo=self.bare, env=env)
        commit = self.git(
            "commit-tree",
            tree,
            "-p",
            self.manifest["checkout_sha"],
            repo=self.bare,
            input="mutation\n",
            env=env,
        )
        return commit, tree

    def test_actual_pair_keeps_runtime_and_dependency_bytes(self):
        self.assertTrue(module.verify(self.bare, self.manifest))
        old = json.loads((self.output / "old.json").read_text())
        self.assertEqual(
            old["application_payload_sha256"],
            self.manifest["application_payload_sha256"],
        )
        package = json.loads(
            self.git(
                "show", f"{self.manifest['checkout_sha']}:package.json", repo=self.bare
            )
        )
        self.assertEqual(package["dependencies"], {"runtime": "1.0.0"})
        self.assertEqual(package["scripts"]["test"], "vitest run")
        self.assertEqual(package["scripts"]["lint:prepared"], "oxlint .")
        self.assertEqual(
            self.git(
                "show",
                f"{self.manifest['checkout_sha']}:sdks/node/src/runtime.ts",
                repo=self.bare,
            ),
            "export const runtime = 7;",
        )

    def test_application_mutation_fails_even_with_new_checkout_tree(self):
        commit, tree = self.altered_commit(
            "crates/application/src/lib.rs", "pub fn source() -> u32 { 8 }\n"
        )
        self.manifest.update(checkout_sha=commit, checkout_tree_hash=tree)
        with self.assertRaisesRegex(ValueError, "application/test/runtime"):
            module.verify(self.bare, self.manifest)

    def test_caller_cannot_hide_runtime_in_boundary(self):
        self.manifest["overlay_boundary"]["files"].append("sdks/node/src/runtime.ts")
        self.manifest["overlay_boundary_sha256"] = module.digest(
            self.manifest["overlay_boundary"]
        )
        with self.assertRaisesRegex(ValueError, "Application, runtime, test"):
            module.validate_boundary(self.manifest["overlay_boundary"])

    def test_test_script_cannot_enter_boundary(self):
        self.manifest["overlay_boundary"]["package_scripts"]["package.json"].append(
            "test"
        )
        with self.assertRaisesRegex(ValueError, "test/runtime"):
            module.validate_boundary(self.manifest["overlay_boundary"])

    def test_missing_graph_file_receipt_fails(self):
        del self.manifest["overlay_files"]["dev/ci/check-gate.py"]
        self.manifest["graph_overlay_sha256"] = module.digest(
            self.manifest["overlay_files"]
        )
        with self.assertRaises(ValueError):
            module.verify(self.bare, self.manifest)

    def test_retired_jobs_removed_and_android_kept(self):
        raw = self.git(
            "show",
            f"{self.manifest['checkout_sha']}:.github/workflows/test-sdk.yml",
            repo=self.bare,
        )
        jobs = module.parse_yaml(raw)["jobs"]
        self.assertEqual(set(jobs), {"android-stage"})
        workflow = module.parse_yaml(
            self.git(
                "show",
                f"{self.manifest['checkout_sha']}:.github/workflows/test.yml",
                repo=self.bare,
            )
        )
        self.assertIs(workflow["jobs"]["unit"]["strategy"]["fail-fast"], False)
        self.assertIn("on", workflow)
        self.assertEqual(
            workflow["jobs"]["unit"]["steps"][0]["with"]["ref"], "${{ github.sha }}"
        )

    def test_reused_callers_have_distinct_receipt_and_profile_ids(self):
        checkout = self.root / "caller-checkout"
        subprocess.run(
            ["git", "clone", "--quiet", str(self.bare), str(checkout)], check=True
        )
        self.git("checkout", "--quiet", self.manifest["checkout_sha"], repo=checkout)
        markers, profiles = [], []
        for kind in ("node", "browser"):
            destination = self.root / f"{kind}-output"
            env = dict(
                os.environ,
                GITHUB_SHA=self.manifest["checkout_sha"],
                GITHUB_OUTPUT=str(destination),
                BENCHMARK_INPUTS=json.dumps({"kind": kind}),
            )
            result = subprocess.run(
                [
                    sys.executable,
                    "-B",
                    "dev/ci/benchmark-receipt.py",
                    "start",
                    "--manifest",
                    "dev/ci/benchmark-overlay.json",
                    "--task",
                    "test-unit",
                ],
                cwd=checkout,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            marker = destination.read_text().strip().split("=", 1)[1]
            markers.append(marker)
            receipt = json.loads(
                (checkout / "target/ci-benchmark" / marker / "start.json").read_text()
            )
            self.assertEqual(receipt["inputs"], {"kind": kind})
            profiles.append(receipt["task_profile_sha256"])
        self.assertNotEqual(markers[0], markers[1])
        self.assertNotEqual(profiles[0], profiles[1])

    def test_runtime_records_real_checkout_cpu_and_missing_reporters(self):
        checkout = self.root / "checkout"
        subprocess.run(
            ["git", "clone", "--quiet", str(self.bare), str(checkout)], check=True
        )
        self.git("checkout", "--quiet", self.manifest["checkout_sha"], repo=checkout)
        env = dict(
            os.environ,
            GITHUB_SHA=self.manifest["checkout_sha"],
            GITHUB_EVENT_NAME="push",
            GITHUB_RUN_ID="100",
            GITHUB_RUN_ATTEMPT="1",
        )
        command = [sys.executable, "-B", "dev/ci/benchmark-receipt.py"]
        started = subprocess.run(
            command
            + [
                "start",
                "--manifest",
                "dev/ci/benchmark-overlay.json",
                "--task",
                "test-unit",
            ],
            cwd=checkout,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(started.returncode, 0, started.stderr)
        ended = subprocess.run(
            command
            + [
                "finish",
                "--manifest",
                "dev/ci/benchmark-overlay.json",
                "--task",
                "test-unit",
            ],
            cwd=checkout,
            env=dict(env, BENCHMARK_JOB_STATUS="success"),
            capture_output=True,
            text=True,
        )
        self.assertEqual(ended.returncode, 0, ended.stderr)
        receipt = json.loads(
            next((checkout / "target/ci-benchmark").glob("*/receipt.json")).read_text()
        )
        self.assertEqual(receipt["checkout_sha"], self.manifest["checkout_sha"])
        self.assertEqual(receipt["frozen_source_sha"], self.source)
        self.assertGreater(receipt["cpu_observation"]["logical_cpu_count"], 0)
        self.assertEqual(receipt["evidence_status"], "UNVERIFIED")
        self.assertIsNone(receipt["test_level_retries"])
        self.assertFalse(receipt["selected_checks_complete"])


if __name__ == "__main__":
    unittest.main()
