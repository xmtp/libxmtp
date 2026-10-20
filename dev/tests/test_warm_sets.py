#!/usr/bin/env python3
"""Check that an RN break on trunk holds back only the RN jobs of post-merge.yml.

The RN device chain is advisory, so an RN break can reach trunk. post-merge.yml
must still build, push, pin, and check the other warm sets of nix/ci-sets.nix
and deploy the docs; only the RN jobs fail and alert.
"""

import json
from pathlib import Path
import re
import shutil
import subprocess
import unittest

import yaml

from test_react_native_workflow import status_expression

ROOT = Path(__file__).resolve().parents[2]
JOBS = yaml.safe_load((ROOT / ".github/workflows/post-merge.yml").read_text())["jobs"]
# The jobs that warm and check the non-RN sets and publish the docs.
NON_RN_JOBS = ("build", "closure", "docs-swift", "docs-kotlin", "docs", "deploy")
# The RN JavaScript layer: the SDK's JavaScript product and its build inputs.
# The JavaScript suites and the docs read it (`sdk_js`), and blocking pull
# request suites build it, so the dev set keeps it.
RN_JS_LAYER = {
    "react-native-js-tools",
    "react-native-js-tools-deps",
    "ubjs-react-native",
    "xmtp-sdk-generated-react-native",
    "xmtp-sdk-react-native-js",
}
# A stand-in for one set: what the closure step reads of each set.
STUB_SET = '{ drvPath = "/nix/store/x.drv"; passthru.members = { }; }'


def nix_json(*args):
    return json.loads(
        subprocess.run(
            [shutil.which("nix"), "eval", "--json", *args],
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


def closure_selection(sets):
    """The sets that the closure step checks, by system, given the set names.

    The step evaluates each installable with an `--apply` function; this
    applies the same functions to stand-in sets with the given names.
    """
    script = next(
        step["run"]
        for step in JOBS["closure"]["steps"]
        if step.get("name", "").startswith("Dry-run the set members")
    )
    evaluations = re.findall(
        r"nix eval [^\n]*?(\.#\S+)[\s\\]*--apply '([^']*)'", script
    )
    # Every evaluation of the step goes through this selection.
    assert len(evaluations) == script.count("nix eval"), script
    selected = {}
    for installable, apply in evaluations:
        path = installable.removeprefix(".#").split(".")
        assert path[0] == "ciSets", installable
        stub = {
            system: {name: None for name in names} for system, names in sets.items()
        }
        for key in path[1:]:
            stub = stub[key]
        value = nix_json("--expr", f"({apply}) ({nix_value(stub)})")
        if len(path) == 2:
            value = {path[1]: value}
        for system, chosen in value.items():
            selected.setdefault(system, set()).update(chosen)
    return selected


def nix_value(stub):
    if stub is None:
        return STUB_SET
    return "{ " + " ".join(f'"{k}" = {nix_value(v)};' for k, v in stub.items()) + " }"


class PostMergeOrderTests(unittest.TestCase):
    def test_the_non_rn_jobs_wait_for_no_rn_job(self):
        rn_jobs = {job for job in JOBS if job.startswith("rn-")}
        self.assertTrue({"rn-components", "rn-package", "rn-consumers"} <= rn_jobs)
        for job in NON_RN_JOBS:
            with self.subTest(job=job):
                self.assertEqual(needed(job) & rn_jobs, set())

    def test_the_rn_chain_starts_after_build_and_also_when_build_fails(self):
        # `build` compiles the native SDK outputs that the RN components share
        # with the Android and iOS release outputs; the RN chain substitutes
        # them instead of compiling them a second time at the same time.
        components = JOBS["rn-components"]
        self.assertEqual(components["needs"], "build")
        for result, cancelled, runs in (
            ("success", False, True),
            ("failure", False, True),
            ("cancelled", True, False),
        ):
            with self.subTest(build=result, run_cancelled=cancelled):
                self.assertIs(
                    status_expression(components["if"], {"build": result}, cancelled),
                    runs,
                )

    def test_build_warms_only_the_dev_and_release_sets(self):
        script = "\n".join(step.get("run", "") for step in JOBS["build"]["steps"])
        self.assertEqual(
            set(re.findall(r"\.#[\w.$-]+", script)),
            {".#ciSets.$SYSTEM.dev", ".#ciSets.$SYSTEM.release", ".#ciSets.$SYSTEM"},
        )
        self.assertNotIn("react-native", script)


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

    def test_the_closure_check_reads_no_rn_output_but_the_javascript_layer(self):
        selected = closure_selection(self.sets)
        for system, names in self.sets.items():
            with self.subTest(system=system):
                # The check still covers every set but the RN one.
                self.assertEqual(selected[system], set(names) - {"rn"})
                for name in selected[system]:
                    with self.subTest(set=name):
                        rn = {m for m in self.sets[system][name] if is_rn(m)}
                        self.assertLessEqual(rn, RN_JS_LAYER)

    def test_the_rn_set_holds_every_other_rn_output(self):
        for system, sets in self.sets.items():
            with self.subTest(system=system):
                rn = set(sets["rn"])
                # Nothing else hides in it from the closure check.
                self.assertTrue(all(is_rn(name) for name in rn), rn)
                self.assertEqual(rn & RN_JS_LAYER, set())
                # Every RN output stays warm and pinned. The RN stages take
                # their members from the sets. The cache jobs warm the test
                # outputs without a set.
                outputs = {
                    name for name in self.outputs[system] if "-test-" not in name
                }
                self.assertEqual(outputs - RN_JS_LAYER, rn)
                self.assertLessEqual(RN_JS_LAYER & outputs, set(sets["dev"]))


if __name__ == "__main__":
    unittest.main()
