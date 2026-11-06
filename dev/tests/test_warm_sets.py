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

from test_react_native_workflow import expression

ROOT = Path(__file__).resolve().parents[2]
JOBS = yaml.safe_load((ROOT / ".github/workflows/post-merge.yml").read_text())["jobs"]
NIX = shutil.which("nix")
# The jobs that warm and check the non-RN sets and publish the docs.
NON_RN_JOBS = ("build", "closure", "docs-swift", "docs-kotlin", "docs", "deploy")
# The RN jobs that cache; rn-alert checks the RN sets and reports.
RN_CHAIN = ("rn-components", "rn-package", "rn-consumers")
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
# RN test outputs that no post-merge job warms. Only full-scope RN pull
# request runs build these apps, and warming them would add a paid 16 vCPU
# Linux or 12x macOS app build to every merge.
COLD_TEST_OUTPUTS = {
    "react-native-test-consumer-android",
    "react-native-test-consumer-ios",
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
# A `curl` for narinfo queries: 200 for the store hashes in FAKE_PRESENT.
FAKE_CURL = """#!{python}
import os, sys

digest = sys.argv[-1].rsplit("/", 1)[1].removesuffix(".narinfo")
sys.stdout.write("200" if digest in os.environ.get("FAKE_PRESENT", "").split() else "404")
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


def run_steps(steps, sets, env=None, closure=(), present=()):
    """Run the `run` text of post-merge steps with the fake nix, cachix, and
    curl.

    Returns the completed process and the calls that the fakes recorded.
    """
    with tempfile.TemporaryDirectory(prefix="warm-sets-") as temp:
        temp = Path(temp)
        binary = temp / "bin"
        binary.mkdir()
        for name, text in (
            ("nix", FAKE_NIX),
            ("cachix", FAKE_CACHIX),
            ("curl", FAKE_CURL),
        ):
            (binary / name).write_text(text)
            (binary / name).chmod(0o755)
        (temp / "sets.json").write_text(json.dumps(stub_sets(sets)))
        (temp / "runner").mkdir()
        log = temp / "log"
        script = "\n".join(s["run"] for s in steps)
        # A step's env, as GitHub sets it; the tests give the values.
        result = subprocess.run(
            ["bash", "-eo", "pipefail", "-c", script],
            cwd=ROOT,
            env=os.environ
            | {
                "PATH": f"{binary}{os.pathsep}{os.environ['PATH']}",
                "REAL_NIX": NIX,
                "FAKE_LOG": str(log),
                "FAKE_SETS": str(temp / "sets.json"),
                "FAKE_CLOSURE": " ".join(closure),
                "FAKE_PRESENT": " ".join(present),
                "RUNNER_TEMP": str(temp / "runner"),
                "TMPDIR": str(temp),
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


def step(job, name, document=None):
    (found,) = [s for s in (document or JOBS)[job]["steps"] if s.get("name") == name]
    return found


def warm_closure_steps():
    """The steps that run dev/ci/warm-closure, by job."""
    return {
        job: s
        for job, spec in JOBS.items()
        for s in spec.get("steps", [])
        if "dev/ci/warm-closure" in s.get("run", "")
    }


def evaluate(
    condition,
    needs=None,
    values=None,
    cancelled=False,
    succeeded=True,
    failed=False,
):
    """Whether a job or step `if` is true.

    GitHub adds `success()` to an `if` that calls no status function. For a
    job, `success()` is true only when every job that it waits for, directly
    or through other jobs, succeeded: "a failure or skip applies to all jobs
    in the dependency chain from the point of failure or skip onwards"
    (docs.github.com, workflow syntax, `jobs.<job_id>.needs`); for a step,
    when no earlier step failed (expressions, status check functions).
    """
    text = condition.strip()
    match = re.fullmatch(r"\$\{\{\s*(.*?)\s*\}\}", text, re.DOTALL)
    if match:
        text = match[1]
    implicit = not STATUS_FUNCTION.search(text)
    text = (
        text.replace("success()", str(succeeded and not cancelled).lower())
        .replace("failure()", str(failed).lower())
        .replace("cancelled()", str(cancelled).lower())
        .replace("always()", "true")
    )
    needs = needs or {}
    text = re.sub(
        r"contains\(needs\.\*\.result, '(\w+)'\)",
        lambda m: str(m[1] in needs.values()).lower(),
        text,
    )
    text = re.sub(r"needs\.([\w-]+)\.result", lambda m: repr(needs[m[1]]), text)
    runs = bool(expression(text, values or {}))
    return runs and (succeeded and not cancelled if implicit else True)


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


def direct_needs(job):
    needs = JOBS[job].get("needs", [])
    return [needs] if isinstance(needs, str) else needs


def run_graph(outcomes, finished=None):
    """The result of each job when the jobs in `outcomes` end that way and
    every other job that runs succeeds.

    With `finished`, someone cancels the run once those jobs have finished:
    every other job starts after the cancellation and sees `cancelled()` true.
    """
    results = {}

    def result(job):
        if job not in results:
            ancestors = needed(job)
            for name in ancestors:
                result(name)
            cancelled = finished is not None and job not in finished
            runs = evaluate(
                JOBS[job].get("if", "${{ true }}"),
                needs={name: results[name] for name in direct_needs(job)},
                cancelled=cancelled,
                succeeded=all(results[name] == "success" for name in ancestors),
                failed=any(results[name] == "failure" for name in ancestors),
            )
            results[job] = outcomes.get(job, "success") if runs else "skipped"
        return results[job]

    for job in JOBS:
        result(job)
    return results


def system_of(label):
    """The Nix system of a runner label, or None for one without warm sets."""
    if "macos" in label:
        return "aarch64-darwin"
    if "ubuntu" in label:
        return "aarch64-linux" if "arm" in label else "x86_64-linux"
    return None


def blocking_shells(root=ROOT):
    """The dev shells that blocking CI suites enter, by the system of the
    runner.

    It reads the workflow of each suite with `pr_filters`, the local
    workflows that it calls, and the local composite actions that they use:
    each `NIX_DEVSHELL` and `--shell`. A runner label comes from the suite's
    `runners`, a `with:` value, a workflow input default, the job matrix, or a
    literal. `dev/nix-shell` without a shell enters `default`, which is never
    an RN shell.
    """
    github = root / ".github"
    suites = yaml.safe_load((github / "ci-suites.yml").read_text())["suites"]
    found = {}

    def shells(node):
        names = set()
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "NIX_DEVSHELL":
                    names.add(value)
                names |= shells(value)
        elif isinstance(node, list):
            for value in node:
                names |= shells(value)
        elif isinstance(node, str):
            names.update(re.findall(r"--shell[ =]([\w-]+)", node))
            names.update(re.findall(r"NIX_DEVSHELL=([\w-]+)", node))
        return names

    def action(path):
        """The shells of a local composite action and the actions it uses."""
        spec = yaml.safe_load(next(path.glob("action.y*ml")).read_text())
        names = set()
        for s in spec["runs"].get("steps", []):
            names |= shells({k: v for k, v in s.items() if k != "uses"})
            names |= uses(s)
        return names

    def uses(s):
        target = s.get("uses", "")
        return (
            action(root / target) if target.startswith("./.github/actions/") else set()
        )

    def labels(value, inputs, matrix):
        match = re.fullmatch(r"\$\{\{(.*)\}\}", value.strip(), re.DOTALL)
        if not match:
            return {value}
        body = match[1]
        found = {v for v in re.findall(r"'([^']+)'", body) if system_of(v)}
        for name in re.findall(r"inputs\.([\w-]+)", body):
            found |= inputs[name]
        for name in re.findall(r"matrix\.([\w-]+)", body):
            include = matrix.get("include") or []
            found |= {str(row[name]) for row in include if name in row}
            found |= {str(v) for v in matrix.get(name, [])}
        if not found:
            raise AssertionError(f"no runner label in {value}")
        return found

    def workflow(path, inputs):
        document = yaml.safe_load(path.read_text())
        declared = (document.get(True) or document.get("on") or {}).get(
            "workflow_call"
        ) or {}
        for name, spec in (declared.get("inputs") or {}).items():
            if name not in inputs and "default" in spec:
                inputs = inputs | {name: {str(spec["default"])}}
        top = shells(document.get("env") or {})
        for job in document["jobs"].values():
            matrix = (job.get("strategy") or {}).get("matrix") or {}
            if job.get("uses", "").startswith("./.github/workflows/"):
                called = {
                    name: labels(str(value), inputs, matrix)
                    for name, value in (job.get("with") or {}).items()
                    if isinstance(value, str)
                }
                workflow(root / job["uses"], called)
                continue
            names = top | shells(job.get("env") or {})
            for s in job.get("steps", []):
                names |= shells({k: v for k, v in s.items() if k != "uses"}) | uses(s)
            for label in labels(str(job["runs-on"]), inputs, matrix):
                if system_of(label):
                    found.setdefault(system_of(label), set()).update(names)

    for suite in suites.values():
        if suite["pr_filters"]:
            inputs = {
                name: set(pair.values())
                for name, pair in (suite.get("runners") or {}).items()
            }
            workflow(github / "workflows" / suite["workflow"], inputs)
    return found


def missing_shells(sets, shells):
    """The (system, shell) pairs that a blocking suite enters but that no dev
    set of that system holds. A system without warm sets has none."""
    return {
        (system, name)
        for system, names in shells.items()
        if system in sets
        for name in names
        if f"shell-{name}" not in sets[system]["dev"]
    }


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
                for job in (*RN_CHAIN, "rn-alert"):
                    self.assertEqual(results[job], "success", job)

    def test_a_cancelled_run_starts_no_later_job(self):
        # A cancelled run must not start the RN cache jobs, which push with
        # the trusted token and move the RN pins. A job that runs at the
        # cancellation ends as cancelled.
        for finished, outcomes in (
            ({"build"}, {"build": "cancelled"}),
            ({"build"}, {}),
            ({"build", "rn-components"}, {}),
            ({"build", "rn-components", "rn-package"}, {}),
            ({"build", "rn-components", "rn-package", "rn-consumers"}, {}),
        ):
            with self.subTest(finished=sorted(finished), outcomes=outcomes):
                results = run_graph(outcomes, finished=finished)
                for name in set(JOBS) - finished:
                    self.assertEqual(results[name], "skipped", name)

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

    def test_rn_alert_checks_the_pushed_sets_and_alerts_on_any_rn_failure(self):
        check = step("rn-alert", "Dry-run the pushed RN sets in an empty store")
        notify = step("rn-alert", "Notify Slack")
        for broken, check_fails, checks, posts in (
            (None, False, True, False),
            # The check runs after its own earlier steps; a failed check posts.
            (None, True, True, True),
            # One rn-consumers leg failed: the other leg's set is checked.
            ({"rn-consumers": "failure"}, False, True, True),
            ({"rn-consumers": "cancelled"}, False, True, True),
            # A job that hits its timeout ends as cancelled.
            ({"rn-components": "cancelled"}, False, False, True),
            ({"rn-package": "failure"}, False, False, True),
        ):
            with self.subTest(broken=broken, check_fails=check_fails):
                results = run_graph(broken or {})
                self.assertEqual(results["rn-alert"], "success")
                needs = {name: results[name] for name in direct_needs("rn-alert")}
                self.assertIs(evaluate(check["if"], needs=needs), checks)
                outcome = (
                    "failure" if check_fails else ("success" if checks else "skipped")
                )
                self.assertIs(
                    evaluate(
                        notify["if"],
                        needs=needs,
                        values={"steps.check.outcome": outcome},
                        succeeded=not check_fails,
                        failed=check_fails,
                    ),
                    posts,
                )
        # A cancelled run checks and alerts on nothing.
        finished = set(RN_CHAIN) | {"build"}
        self.assertEqual(run_graph({}, finished=finished)["rn-alert"], "skipped")

    def test_rn_cache_alert_ignores_non_rn_failures(self):
        check = step("rn-alert", "Dry-run the pushed RN sets in an empty store")
        notify = step("rn-alert", "Notify Slack")
        for broken in (*NON_RN_JOBS, "pnpm-store"):
            for result in ("failure", "cancelled"):
                with self.subTest(job=broken, result=result):
                    results = run_graph({broken: result})
                    self.assertEqual(results["rn-alert"], "success")
                    needs = {name: results[name] for name in direct_needs("rn-alert")}
                    self.assertTrue(evaluate(check["if"], needs=needs))
                    self.assertFalse(
                        evaluate(
                            notify["if"],
                            needs=needs,
                            values={"steps.check.outcome": "success"},
                        )
                    )


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
        # hold them: the JavaScript layer on Linux, and the RN shells that
        # blocking suites enter on each system.
        cls.blocking_rn = {
            system: {
                f"shell-{name}" for name in cls.shells.get(system, ()) if is_rn(name)
            }
            for system in cls.sets
        }
        cls.blocking_rn["x86_64-linux"] |= RN_JS_LAYER
        # Each stub member path, as (system, set, member).
        stub = stub_sets(cls.sets)
        cls.paths = {
            member["outPath"]: (system, name, member_name)
            for system, system_sets in stub.items()
            for name, spec in system_sets.items()
            for member_name, member in spec["passthru"]["members"].items()
        }
        # aarch64-linux builds no RN output, so it has no rn set.
        cls.rn_systems = {system for system, sets in cls.sets.items() if "rn" in sets}
        cls.rn_set_digests = {
            system: Path(stub[system]["rn"]["outPath"]).name[:32]
            for system in cls.rn_systems
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
        self.assertIn("react-native", self.shells["x86_64-linux"])
        self.assertIn("ios", self.shells["aarch64-darwin"])
        self.assertEqual(missing_shells(self.sets, self.shells), set())

    def test_a_blocking_macos_entry_into_an_rn_shell_needs_it_in_darwin_dev(self):
        # A blocking suite on a macOS runner that enters the RN shell through
        # a composite action of a called workflow.
        with tempfile.TemporaryDirectory(prefix="warm-sets-shells-") as temp:
            root = Path(temp)
            (root / ".github/workflows").mkdir(parents=True)
            (root / ".github/actions/enter").mkdir(parents=True)
            (root / ".github/ci-suites.yml").write_text(
                yaml.safe_dump(
                    {
                        "suites": {
                            "fixture": {
                                "workflow": "fixture.yml",
                                "pr_filters": ["sdks/**"],
                                "runners": {
                                    "runner": {
                                        "pr": "warp-macos-15-arm64-6x",
                                        "merge": "macos-15",
                                    }
                                },
                            },
                            "advisory": {
                                "workflow": "advisory.yml",
                                "pr_filters": [],
                            },
                        }
                    }
                )
            )
            (root / ".github/workflows/fixture.yml").write_text(
                yaml.safe_dump(
                    {
                        "on": {
                            "workflow_call": {"inputs": {"runner": {"type": "string"}}}
                        },
                        "jobs": {
                            "call": {
                                "uses": "./.github/workflows/called.yml",
                                "with": {"runner": "${{ inputs.runner }}"},
                            }
                        },
                    }
                )
            )
            (root / ".github/workflows/called.yml").write_text(
                yaml.safe_dump(
                    {
                        "on": {
                            "workflow_call": {"inputs": {"runner": {"type": "string"}}}
                        },
                        "jobs": {
                            "test": {
                                "runs-on": "${{ inputs.runner }}",
                                "steps": [{"uses": "./.github/actions/enter"}],
                            }
                        },
                    }
                )
            )
            (root / ".github/workflows/advisory.yml").write_text(
                yaml.safe_dump(
                    {
                        "jobs": {
                            "test": {
                                "runs-on": "ubuntu-latest",
                                "env": {"NIX_DEVSHELL": "react-native-local"},
                                "steps": [{"run": "true"}],
                            }
                        }
                    }
                )
            )
            (root / ".github/actions/enter/action.yml").write_text(
                yaml.safe_dump(
                    {
                        "runs": {
                            "using": "composite",
                            "steps": [
                                {
                                    "shell": "bash",
                                    "run": "dev/nix-shell --shell react-native 'true'",
                                }
                            ],
                        }
                    }
                )
            )
            shells = blocking_shells(root)
        self.assertEqual(shells, {"aarch64-darwin": {"react-native"}})
        self.assertEqual(
            missing_shells(self.sets, shells), {("aarch64-darwin", "react-native")}
        )

    def test_build_has_a_leg_for_each_system(self):
        legs = JOBS["build"]["strategy"]["matrix"]["include"]
        self.assertEqual({leg["system"] for leg in legs}, set(self.sets))

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

    def test_an_rn_job_builds_pushes_and_pins_the_rn_set_on_each_leg(self):
        job = JOBS["rn-consumers"]
        self.assertEqual(job["env"]["SYSTEM"], "${{ matrix.system }}")
        legs = job["strategy"]["matrix"]["include"]
        self.assertEqual({leg["system"] for leg in legs}, self.rn_systems)
        names = ("Build the RN warm set", "Push and pin the RN warm set")
        self.assertEqual(
            step("rn-consumers", names[1])["env"]["CACHIX_AUTH_TOKEN"],
            "${{ secrets.CACHIX_AUTH_TOKEN }}",
        )
        for leg in legs:
            system = leg["system"]
            with self.subTest(system=system):
                values = {f"matrix.{key}": value for key, value in leg.items()}
                values["runner.os"] = "macOS" if "darwin" in system else "Linux"
                steps = [
                    s
                    for s in job["steps"]
                    if evaluate(s.get("if", "${{ true }}"), values=values)
                ]
                for name in names:
                    (found,) = [s for s in steps if s.get("name") == name]
                    self.assertNotIn("continue-on-error", found)
                result, calls = run_steps(
                    [s for s in steps if s.get("name") in names],
                    self.sets,
                    env={"SYSTEM": system, "RUNNER_OS": values["runner.os"]},
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

    def test_every_rn_test_output_is_warmed_or_known_cold(self):
        script = "\n".join(
            s.get("run", "")
            for job, spec in JOBS.items()
            if job.startswith("rn-")
            for s in spec.get("steps", [])
        )
        for system, outputs in self.outputs.items():
            for name in sorted(n for n in outputs if "-test-" in n):
                with self.subTest(system=system, output=name):
                    warmed = re.search(rf"\.#{re.escape(name)}(?![\w-])", script)
                    self.assertIs(bool(warmed), name not in COLD_TEST_OUTPUTS)

    def test_the_closure_check_reads_only_rn_outputs_that_blocking_suites_build(self):
        self.assertEqual(set(warm_closure_steps()), {"closure", "rn-alert"})
        result, calls = run_steps([warm_closure_steps()["closure"]], self.sets)
        self.assertEqual(result.returncode, 0, result.stderr)
        (dry_run,) = [c["dry-run"] for c in calls if "dry-run" in c]
        # Only the set members that reach the dry run count as checked.
        checked = self.checked(dry_run)
        for system, system_sets in self.sets.items():
            with self.subTest(system=system):
                # An empty set (aarch64-linux release) has nothing to check.
                self.assertEqual(
                    {name for s, name in checked if s == system},
                    {name for name, names in system_sets.items() if names} - {"rn"},
                )
                reached = {
                    self.paths[p][2]
                    for p in dry_run
                    if p in self.paths and self.paths[p][0] == system
                }
                self.assertLessEqual(
                    {m for m in reached if is_rn(m)}, self.blocking_rn[system]
                )

    def test_rn_alert_checks_the_rn_set_of_each_system_that_xmtp_holds(self):
        check = step("rn-alert", "Dry-run the pushed RN sets in an empty store")
        self.assertEqual(
            check["env"]["RN_CONSUMERS"], "${{ needs.rn-consumers.result }}"
        )
        both = self.rn_systems
        for consumers, present, returncode, checked in (
            ("success", both, 0, both),
            # One leg failed before its push: the other system is checked.
            ("failure", {"x86_64-linux"}, 0, {"x86_64-linux"}),
            ("failure", {"aarch64-darwin"}, 0, {"aarch64-darwin"}),
            ("failure", set(), 0, set()),
            # Every leg succeeded, so a missing set is an error.
            ("success", {"x86_64-linux"}, 1, set()),
        ):
            with self.subTest(consumers=consumers, present=sorted(present)):
                result, calls = run_steps(
                    [check],
                    self.sets,
                    env={"RN_CONSUMERS": consumers},
                    present=[self.rn_set_digests[s] for s in present],
                )
                self.assertEqual(
                    result.returncode, returncode, result.stdout + result.stderr
                )
                dry_runs = [c["dry-run"] for c in calls if "dry-run" in c]
                self.assertEqual(
                    self.checked(dry_runs[0]) if dry_runs else set(),
                    {(system, "rn") for system in checked},
                )

    def test_rn_isolation_fails_when_another_set_builds_an_rn_member(self):
        nix_eval = yaml.safe_load((ROOT / ".github/workflows/nix-eval.yml").read_text())
        (check,) = [
            s
            for s in nix_eval["jobs"]["ci-sets"]["steps"]
            if "dev/ci/rn-isolation" in s.get("run", "")
        ]
        stub = stub_sets(self.sets)
        result, calls = run_steps([check], self.sets)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        (evaluated,) = [c["result"] for c in calls if "eval" in c]
        # It also evaluates every set's drvPath.
        self.assertEqual(
            {system: set(value["drvs"]) for system, value in evaluated.items()},
            {system: set(sets) for system, sets in self.sets.items()},
        )
        for system in self.rn_systems:
            with self.subTest(system=system):
                members = stub[system]["rn"]["passthru"]["members"]
                drv = next(iter(members.values()))["drvPath"]
                result, _ = run_steps([check], self.sets, closure=[drv])
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(drv, result.stdout)

    def test_the_rn_set_holds_every_other_rn_output(self):
        for system in self.rn_systems:
            sets = self.sets[system]
            with self.subTest(system=system):
                rn = set(sets["rn"])
                blocking = self.blocking_rn[system]
                # Nothing else hides in it from the closure check.
                self.assertTrue(all(is_rn(name) for name in rn), rn)
                self.assertEqual(rn & blocking, set())
                # Every RN output stays warm and pinned. The RN stages take
                # their members from the sets. The test outputs are not set
                # members; test_every_rn_test_output_is_warmed_or_known_cold
                # covers them.
                outputs = {
                    name for name in self.outputs[system] if "-test-" not in name
                }
                self.assertEqual(outputs - blocking, rn)
                self.assertLessEqual(blocking & outputs, set(sets["dev"]))


if __name__ == "__main__":
    unittest.main()
