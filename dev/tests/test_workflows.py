"""Repository-wide workflow rules: result gates and action pins."""

import json
import os
from pathlib import Path
import re
import subprocess
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
GITHUB = ROOT / ".github"
GATE_JOBS = ("result", "results", "required")


def tracked(*patterns):
    listing = subprocess.run(
        ["git", "ls-files", *patterns],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [ROOT / path for path in listing.split()]


def workflows():
    for path in tracked(".github/workflows/*.yml"):
        yield path, yaml.safe_load(path.read_text())


def expression_value(expression, results):
    """The value a gate step reads for one `${{ ... }}` expression."""
    suites = [job for job in results if not job.startswith("products_")]
    products = [job.removeprefix("products_") for job in results if job not in suites]
    known = {
        "toJSON(needs)": json.dumps({job: {"result": r} for job, r in results.items()}),
        "inputs.suites": json.dumps(suites),
        "inputs.products": json.dumps(products),
        # Deploy runs need every job, so the gate is checked at its strictest.
        "inputs.deploy == true": "true",
    }
    if expression in known:
        return known[expression]
    if re.fullmatch(r"needs\.[\w-]+\.result == 'success'( && .*)?", expression):
        python = re.sub(
            r"needs\.([\w-]+)\.result", lambda m: repr(results[m[1]]), expression
        ).replace("&&", " and ")
        return "true" if eval(python, {"__builtins__": {}}, {}) else "false"
    raise AssertionError(f"unknown gate expression {expression!r}; extend this test")


def run_gate(step, results):
    env = dict(os.environ)
    for name, value in (step.get("env") or {}).items():
        value = str(value)
        match = re.fullmatch(r"\$\{\{\s*(.*?)\s*\}\}", value)
        env[name] = expression_value(match[1], results) if match else value
    return subprocess.run(
        ["bash", "-c", step["run"]], env=env, capture_output=True, text=True
    ).returncode


class GateTests(unittest.TestCase):
    def test_every_result_gate_needs_and_checks_every_job(self):
        """A gate job runs always, needs every other job, and fails on each one."""
        gates = 0
        for path, workflow in workflows():
            jobs = workflow.get("jobs") or {}
            for name in set(jobs) & set(GATE_JOBS):
                gates += 1
                gate = jobs[name]
                with self.subTest(workflow=path.name, gate=name):
                    self.assertEqual(gate.get("if"), "always()")
                    needs = gate["needs"]
                    self.assertEqual(set(needs), set(jobs) - {name})
                    (step,) = gate["steps"]
                    passing = {job: "success" for job in needs}
                    self.assertEqual(run_gate(step, passing), 0)
                    for job in needs:
                        for result in ("failure", "skipped", "cancelled"):
                            self.assertNotEqual(
                                run_gate(step, passing | {job: result}),
                                0,
                                f"{job} {result} passes the gate",
                            )
        # Guards against a renamed gate that this test no longer finds.
        self.assertGreaterEqual(gates, 5)


class ActionPinTests(unittest.TestCase):
    def test_every_external_action_is_pinned_by_commit(self):
        """A tag can move; a 40-hex commit cannot."""
        unpinned = []
        for path in tracked(".github/*.yml", ".github/**/*.yml", ".github/**/*.yaml"):
            for number, line in enumerate(path.read_text().splitlines(), 1):
                match = re.match(r"\s*(?:-\s*)?uses:\s*([^\s#]+)", line)
                if not match or match[1].startswith("./"):
                    continue
                if not re.fullmatch(r"[^@\s]+@[0-9a-f]{40}", match[1]):
                    unpinned.append(f"{path.relative_to(ROOT)}:{number}: {match[1]}")
        self.assertEqual(unpinned, [])


if __name__ == "__main__":
    unittest.main()
