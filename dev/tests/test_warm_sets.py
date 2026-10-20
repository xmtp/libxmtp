#!/usr/bin/env python3
"""Check that an RN break on trunk holds back only the RN jobs of post-merge.yml.

The RN device chain is advisory, so an RN break can reach trunk. post-merge.yml
must still build, push, pin, and check the other warm sets of nix/ci-sets.nix
and deploy the docs; only the RN jobs fail and alert. The RN jobs warm, pin,
and check the RN set themselves.
"""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

import yaml

from test_react_native_workflow import expression, inner, status_expression

ROOT = Path(__file__).resolve().parents[2]
JOBS = yaml.safe_load((ROOT / ".github/workflows/post-merge.yml").read_text())["jobs"]
NIX = shutil.which("nix")
# The jobs that warm and check the non-RN sets and publish the docs.
NON_RN_JOBS = ("build", "closure", "docs-swift", "docs-kotlin", "docs", "deploy")
# The RN jobs that cache and test; rn-alert reports on them.
RN_CHAIN = ("rn-components", "rn-package", "rn-consumers", "rn-smoke")
STATUS_FUNCTION = re.compile(r"\b(?:success|failure|cancelled|always)\(\)")
# The RN JavaScript layer: the SDK's JavaScript product and its build inputs.
# The JavaScript suites and the docs read it through the Linux product group
# `sdk_js`, and blocking Linux suites build it, so the Linux dev set keeps it.
# No blocking suite builds it on Darwin.
RN_JS_LAYER = {
    "react-native-js-tools",
    "react-native-js-tools-deps",
    "ubjs-react-native",
    "xmtp-sdk-generated-react-native",
    "xmtp-sdk-react-native-js",
}

# A `nix` for the post-merge steps. `eval` applies the step's function to stub
# sets (FAKE_SETS) through the real nix; `build`, `build --dry-run`, and
# `derivation show` print stand-ins. Each call that the tests check goes to
# FAKE_LOG.
FAKE_NIX = """#!{python}
import hashlib, json, os, subprocess, sys

args = sys.argv[1:]


def record(entry):
    with open(os.environ["FAKE_LOG"], "a") as log:
        log.write(json.dumps(entry) + "\\n")


def store_path(name):
    digest = hashlib.sha256(name.encode()).hexdigest()[:32].replace("e", "g")
    return f"/nix/store/{{digest}}-{{name}}"


if args[0] == "eval":
    installable = next(a for a in args if a.startswith(".#"))
    apply = args[args.index("--apply") + 1]
    path = installable[2:].split(".")
    assert path[0] == "ciSets", installable
    value = "(builtins.fromJSON (builtins.readFile " + os.environ["FAKE_SETS"] + "))"
    for key in path[1:]:
        value = f'{{value}}."{{key}}"'
    out = subprocess.run(
        [os.environ["REAL_NIX"], "eval", "--impure", "--json", "--expr", f"({{apply}}) {{value}}"],
        check=True, capture_output=True, text=True,
    ).stdout
    record({{"eval": installable, "result": json.loads(out)}})
    sys.stdout.write(out)
elif args[0] == "derivation":
    drvs = [a for a in args if a.startswith("/nix/store/")]
    drvs += os.environ.get("FAKE_CLOSURE", "").split()
    entries = {{drv: {{"env": {{}}}} for drv in drvs}}
    entries[store_path("deps.drv")] = {{"env": {{"CRANE_BUILD_DEPS_ONLY": "1"}}}}
    print(json.dumps(entries))
elif args[0] == "build" and "--dry-run" in args:
    paths = [a for a in args if a.startswith("/nix/store/")]
    record({{"dry-run": paths}})
    print(f"these {{len(paths)}} paths will be fetched (1.0 MiB download, 2.0 MiB unpacked):")
    for number in range(len(paths)):
        print("  " + store_path(f"fetched-{{number}}"))
elif args[0] == "build":
    targets = [a for a in args if a.startswith(".#")]
    record({{"build": targets}})
    for target in targets:
        if target.startswith(".#ciSets."):
            print(store_path("ci-set-" + target.split(".")[-1]))
        else:
            print(store_path(target[2:]))
else:
    sys.exit(f"fake nix: unexpected {{args}}")
""".format(python=sys.executable)
FAKE_CACHIX = """#!{python}
import json, os, sys

with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({{"cachix": sys.argv[1:]}}) + "\\n")
""".format(python=sys.executable)


def nix_json(*args):
    return json.loads(
        subprocess.run(
            [NIX, "eval", "--json", *args],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    )


def members(output):
    """The member names of each set of a flake output, by system."""
    return nix_json(
        f".#{output}",
        "--apply",
        "builtins.mapAttrs (_: builtins.mapAttrs (_: set: builtins.attrNames set.passthru.members))",
    )


def is_rn(name):
    return "react-native" in name


def store_path(name):
    digest = hashlib.sha256(name.encode()).hexdigest()[:32].replace("e", "g")
    return f"/nix/store/{digest}-{name}"


def stub_sets(sets):
    """Stand-ins for `ciSets` with the real set and member names."""
    return {
        system: {
            name: {
                "drvPath": store_path(f"ci-set-{system}-{name}.drv"),
                "outPath": store_path(f"ci-set-{system}-{name}"),
                "passthru": {
                    "members": {
                        # One path for each set, so each path names its set.
                        member: {
                            "type": "derivation",
                            "drvPath": store_path(f"{system}-{name}-{member}.drv"),
                            "outPath": store_path(f"{system}-{name}-{member}"),
                        }
                        for member in names
                    }
                },
            }
            for name, names in system_sets.items()
        }
        for system, system_sets in sets.items()
    }


def run_steps(steps, sets, env=None, closure=()):
    """Run the `run` text of post-merge steps with the fake nix and cachix.

    Returns the completed process and the calls that the fakes recorded.
    """
    with tempfile.TemporaryDirectory(prefix="warm-sets-") as temp:
        temp = Path(temp)
        binary = temp / "bin"
        binary.mkdir()
        for name, text in (("nix", FAKE_NIX), ("cachix", FAKE_CACHIX)):
            (binary / name).write_text(text)
            (binary / name).chmod(0o755)
        (temp / "sets.json").write_text(json.dumps(stub_sets(sets)))
        (temp / "runner").mkdir()
        log = temp / "log"
        result = subprocess.run(
            ["bash", "-eo", "pipefail", "-c", "\n".join(s["run"] for s in steps)],
            cwd=ROOT,
            env=os.environ
            | {
                "PATH": f"{binary}{os.pathsep}{os.environ['PATH']}",
                "REAL_NIX": NIX,
                "FAKE_LOG": str(log),
                "FAKE_SETS": str(temp / "sets.json"),
                "FAKE_CLOSURE": " ".join(closure),
                "RUNNER_TEMP": str(temp / "runner"),
            }
            | (env or {}),
            capture_output=True,
            text=True,
        )
        calls = (
            [json.loads(line) for line in log.read_text().splitlines()]
            if log.exists()
            else []
        )
        return result, calls


def step(job, name):
    (found,) = [s for s in JOBS[job]["steps"] if s.get("name") == name]
    return found


def warm_closure_steps():
    """The steps that run dev/ci/warm-closure, by job."""
    return {
        job: s
        for job, spec in JOBS.items()
        for s in spec.get("steps", [])
        if "dev/ci/warm-closure" in s.get("run", "")
    }


def needed(job):
    """Every job that `job` waits for, directly or through other jobs."""
    found = set()
    pending = [job]
    while pending:
        needs = JOBS[pending.pop()].get("needs", [])
        for name in [needs] if isinstance(needs, str) else needs:
            if name not in found:
                found.add(name)
                pending.append(name)
    return found


def run_graph(outcomes):
    """The result of each job when the jobs in `outcomes` end that way and
    every other job that runs succeeds.

    GitHub adds `success()` to a job `if` that calls no status function, and
    `success()` is true only when every job that the job waits for, directly
    or through other jobs, succeeded: "a failure or skip applies to all jobs
    in the dependency chain from the point of failure or skip onwards"
    (docs.github.com, workflow syntax, `jobs.<job_id>.needs`; expressions,
    status check functions).
    """
    results = {}

    def result(job):
        if job not in results:
            needs = JOBS[job].get("needs", [])
            needs = [needs] if isinstance(needs, str) else needs
            ancestors = needed(job)
            for name in ancestors:
                result(name)
            succeeded = all(results[name] == "success" for name in ancestors)
            failed = any(results[name] == "failure" for name in ancestors)
            condition = JOBS[job].get("if", "${{ true }}")
            runs = succeeded or bool(STATUS_FUNCTION.search(condition))
            condition = (
                condition.replace("success()", str(succeeded).lower())
                .replace("failure()", str(failed).lower())
                .replace("always()", "true")
            )
            direct = {name: results[name] for name in needs}
            runs = runs and status_expression(condition, direct, cancelled=False)
            results[job] = outcomes.get(job, "success") if runs else "skipped"
        return results[job]

    for job in JOBS:
        result(job)
    return results


def blocking_shells():
    """The dev shells that blocking CI suites enter: each `NIX_DEVSHELL` and
    `--shell` of the workflow of a suite with `pr_filters`."""
    suites = yaml.safe_load((ROOT / ".github/ci-suites.yml").read_text())["suites"]
    found = set()

    def visit(node):
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "NIX_DEVSHELL":
                    found.add(value)
                visit(value)
        elif isinstance(node, list):
            for value in node:
                visit(value)
        elif isinstance(node, str):
            found.update(re.findall(r"--shell[ =]([\w-]+)", node))

    for suite in suites.values():
        if suite["pr_filters"]:
            path = ROOT / ".github/workflows" / suite["workflow"]
            visit(yaml.safe_load(path.read_text()))
    return found


class PostMergeOrderTests(unittest.TestCase):
    def test_the_non_rn_jobs_wait_for_no_rn_job(self):
        rn_jobs = {job for job in JOBS if job.startswith("rn-")}
        self.assertTrue(set(RN_CHAIN) <= rn_jobs)
        for job in NON_RN_JOBS:
            with self.subTest(job=job):
                self.assertEqual(needed(job) & rn_jobs, set())

    def test_an_rn_failure_holds_back_only_the_rn_jobs(self):
        for job in RN_CHAIN:
            for result in ("failure", "cancelled"):
                with self.subTest(job=job, result=result):
                    results = run_graph({job: result})
                    for other in NON_RN_JOBS:
                        self.assertEqual(results[other], "success", other)
                    for later in RN_CHAIN[RN_CHAIN.index(job) + 1 :]:
                        self.assertEqual(results[later], "skipped", later)
                    # A job that hits its timeout ends as cancelled.
                    self.assertEqual(results["rn-alert"], "success")

    def test_the_rn_chain_runs_after_build_and_also_when_build_fails(self):
        # `build` compiles the native SDK outputs that the RN components share
        # with the Android and iOS release outputs; the RN chain substitutes
        # them instead of compiling them a second time at the same time.
        self.assertIn("build", needed("rn-components"))
        for build in ("success", "failure", "cancelled"):
            with self.subTest(build=build):
                results = run_graph({"build": build})
                for job in RN_CHAIN:
                    self.assertEqual(results[job], "success", job)
                self.assertEqual(results["rn-alert"], "skipped")
                # An RN failure still alerts while `build` is broken.
                results = run_graph({"build": build, "rn-package": "failure"})
                self.assertEqual(results["rn-alert"], "success")

    def test_build_runs_no_rn_step(self):
        steps = JOBS["build"]["steps"]
        script = "\n".join(s.get("run", "") for s in steps)
        self.assertEqual(
            set(re.findall(r"\.#[\w.$-]+", script)),
            {".#ciSets.$SYSTEM.dev", ".#ciSets.$SYSTEM.release", ".#ciSets.$SYSTEM"},
        )
        self.assertNotIn("react-native", script)
        for s in steps:
            self.assertNotIn("react-native", s.get("uses", ""))


class WarmSetTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sets = members("ciSets")
        # The RN package outputs and dev shells of each system.
        names = "builtins.mapAttrs (_: builtins.attrNames)"
        packages = nix_json(".#packages", "--apply", names)
        shells = nix_json(".#devShells", "--apply", names)
        cls.outputs = {
            system: {
                name
                for name in packages[system] + [f"shell-{s}" for s in shells[system]]
                if is_rn(name)
            }
            for system in cls.sets
        }
        cls.shells = blocking_shells()
        # The RN outputs that blocking suites build, so the non-RN sets may
        # hold them: on Linux the JavaScript layer and the RN shells that
        # blocking suites enter. Nothing on Darwin.
        cls.blocking_rn = {system: set() for system in cls.sets}
        cls.blocking_rn["x86_64-linux"] = RN_JS_LAYER | {
            f"shell-{name}" for name in cls.shells if is_rn(name)
        }
        # Each stub member path, as (system, set, member).
        stub = stub_sets(cls.sets)
        cls.paths = {
            member["outPath"]: (system, name, member_name)
            for system, system_sets in stub.items()
            for name, spec in system_sets.items()
            for member_name, member in spec["passthru"]["members"].items()
        }

    def checked(self, paths):
        """The (system, set) pairs whose members all reach the dry run."""
        reached = {self.paths[p] for p in paths if p in self.paths}
        return {
            (system, name)
            for system, system_sets in self.sets.items()
            for name, names in system_sets.items()
            if names and {(system, name, m) for m in names} <= reached
        }

    def test_blocking_suites_enter_warm_shells(self):
        self.assertIn("react-native", self.shells)
        warm = set(self.sets["x86_64-linux"]["dev"]) | set(
            self.sets["aarch64-darwin"]["dev"]
        )
        for name in self.shells:
            with self.subTest(shell=name):
                self.assertIn(f"shell-{name}", warm)
                if is_rn(name):
                    # check_react_native runs on Linux.
                    self.assertIn(f"shell-{name}", self.sets["x86_64-linux"]["dev"])

    def test_build_pins_only_the_dev_and_release_sets(self):
        for system in self.sets:
            with self.subTest(system=system):
                result, calls = run_steps(
                    [step("build", "Push and pin the warm sets")],
                    self.sets,
                    env={"SYSTEM": system},
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                (evaluated,) = [c["result"] for c in calls if "eval" in c]
                self.assertEqual(set(evaluated), {"dev", "release"})
                pins = [c["cachix"] for c in calls if c.get("cachix", [""])[0] == "pin"]
                self.assertEqual(
                    pins,
                    [
                        [
                            "pin",
                            "xmtp",
                            f"{name}-{system}",
                            evaluated[name],
                            "--keep-revisions",
                            "3",
                        ]
                        for name in ("dev", "release")
                    ],
                )

    def test_an_rn_job_builds_pushes_and_pins_the_rn_set(self):
        job = JOBS["rn-consumers"]
        legs = job["strategy"]["matrix"]["include"]
        self.assertEqual({leg["system"] for leg in legs}, set(self.sets))
        steps = [
            step("rn-consumers", "Build the RN warm set"),
            step("rn-consumers", "Push and pin the RN warm set"),
        ]
        self.assertEqual(
            steps[1]["env"]["CACHIX_AUTH_TOKEN"], "${{ secrets.CACHIX_AUTH_TOKEN }}"
        )
        for leg in legs:
            system = leg["system"]
            with self.subTest(system=system):
                result, calls = run_steps(
                    steps,
                    self.sets,
                    env={
                        "SYSTEM": system,
                        "RUNNER_OS": "macOS" if "darwin" in system else "Linux",
                    },
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                (built,) = [c["build"] for c in calls if "build" in c]
                self.assertIn(f".#ciSets.{system}.rn", built)
                path = store_path("ci-set-rn")
                cachix = [c["cachix"] for c in calls if "cachix" in c]
                self.assertIn(["push", "xmtp", path], cachix)
                self.assertIn(
                    ["pin", "xmtp", f"rn-{system}", path, "--keep-revisions", "3"],
                    cachix,
                )

    def test_the_closure_check_reads_only_rn_outputs_that_blocking_suites_build(self):
        self.assertEqual(set(warm_closure_steps()), {"closure", "rn-smoke"})
        result, calls = run_steps([warm_closure_steps()["closure"]], self.sets)
        self.assertEqual(result.returncode, 0, result.stderr)
        (dry_run,) = [c["dry-run"] for c in calls if "dry-run" in c]
        # Only the set members that reach the dry run count as checked.
        checked = self.checked(dry_run)
        for system, system_sets in self.sets.items():
            with self.subTest(system=system):
                self.assertEqual(
                    {name for s, name in checked if s == system},
                    set(system_sets) - {"rn"},
                )
                reached = {
                    m
                    for p in dry_run
                    if p in self.paths
                    for s, _, m in [self.paths[p]]
                    if s == system
                }
                self.assertLessEqual(
                    {m for m in reached if is_rn(m)}, self.blocking_rn[system]
                )
        # The warm-up evaluation selects no set that the check leaves out.
        (warm_up,) = [
            c["result"] for c in calls if c.get("eval") == ".#ciSets.x86_64-linux"
        ]
        self.assertLessEqual(set(warm_up), set(self.sets["x86_64-linux"]) - {"rn"})

    def test_the_closure_check_fails_when_a_checked_set_builds_an_rn_member(self):
        rn_member = stub_sets(self.sets)["aarch64-darwin"]["rn"]["passthru"]["members"]
        drv = next(iter(rn_member.values()))["drvPath"]
        result, _ = run_steps(
            [warm_closure_steps()["closure"]], self.sets, closure=[drv]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(drv, result.stdout)

    def test_the_rn_set_is_checked_after_the_rn_jobs_push_it(self):
        check = warm_closure_steps()["rn-smoke"]
        self.assertIn("rn-consumers", needed("rn-smoke"))
        # Only the Android leg runs it: a free runner. It runs after a
        # failed smoke too.
        condition = inner(check["if"]).replace("cancelled()", "false")
        self.assertTrue(expression(condition, {"matrix.platform": "android"}))
        self.assertFalse(expression(condition, {"matrix.platform": "ios"}))
        result, calls = run_steps([check], self.sets)
        self.assertEqual(result.returncode, 0, result.stderr)
        (dry_run,) = [c["dry-run"] for c in calls if "dry-run" in c]
        self.assertEqual(
            self.checked(dry_run), {(system, "rn") for system in self.sets}
        )

    def test_the_rn_set_holds_every_other_rn_output(self):
        for system, sets in self.sets.items():
            with self.subTest(system=system):
                rn = set(sets["rn"])
                blocking = self.blocking_rn[system]
                # Nothing else hides in it from the closure check.
                self.assertTrue(all(is_rn(name) for name in rn), rn)
                self.assertEqual(rn & blocking, set())
                # Every RN output stays warm and pinned. The RN stages take
                # their members from the sets. The cache jobs warm the test
                # outputs without a set.
                outputs = {
                    name for name in self.outputs[system] if "-test-" not in name
                }
                self.assertEqual(outputs - blocking, rn)
                self.assertLessEqual(blocking & outputs, set(sets["dev"]))


if __name__ == "__main__":
    unittest.main()
