"""Repository-wide workflow rules: result gates, image caches, and action pins."""

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
# The Lint and Test gates of ci.yml: they need detection and only some jobs.
CI_GATES = ("lint", "test")


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
    """The value a gate step reads for one `${{ ... }}` expression.

    Every needed job counts as selected, so a gate is checked at its strictest.
    """
    suites = [
        job
        for job in results
        if not job.startswith("products_") and job != "detect-changes"
    ]
    products = [
        job.removeprefix("products_") for job in results if job.startswith("products_")
    ]
    known = {
        "toJSON(needs)": json.dumps({job: {"result": r} for job, r in results.items()}),
        "needs.detect-changes.result": results.get("detect-changes", ""),
        "needs.detect-changes.outputs.lint_suites": json.dumps(suites),
        "needs.detect-changes.outputs.test_suites": json.dumps(suites),
        "needs.detect-changes.outputs.products": json.dumps(products),
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


def ci_gate_needs(jobs, phase):
    """Detection, the products jobs for Test, and the blocking callers of the phase."""
    callers = [
        job
        for job, spec in jobs.items()
        if f"needs.detect-changes.outputs.{phase}_suites)" in str(spec.get("if", ""))
    ]
    products = (
        [job for job in jobs if job.startswith("products_")] if phase == "test" else []
    )
    return {"detect-changes", *products, *callers}


class GateTests(unittest.TestCase):
    def test_every_result_gate_needs_and_checks_every_job(self):
        """A gate job runs always, needs the jobs it checks, and fails on each one.

        An inner gate needs every other job of its workflow. A ci.yml gate needs
        detection and the blocking callers of its phase, never an advisory one.
        """
        gates = 0
        for path, workflow in workflows():
            jobs = workflow.get("jobs") or {}
            names = set(jobs) & set(CI_GATES if path.name == "ci.yml" else GATE_JOBS)
            for name in names:
                gates += 1
                gate = jobs[name]
                with self.subTest(workflow=path.name, gate=name):
                    self.assertEqual(gate.get("if"), "always()")
                    needs = gate["needs"]
                    if path.name == "ci.yml":
                        self.assertEqual(set(needs), ci_gate_needs(jobs, name))
                    else:
                        self.assertEqual(set(needs), set(jobs) - {name})
                    (step,) = (step for step in gate["steps"] if "run" in step)
                    passing = {job: "success" for job in needs}
                    self.assertEqual(run_gate(step, passing), 0)
                    for job in needs:
                        for result in ("failure", "skipped", "cancelled"):
                            self.assertNotEqual(
                                run_gate(step, passing | {job: result}),
                                0,
                                f"{job} {result} passes the gate",
                            )
        # Guards against a renamed gate that this test no longer finds: the
        # Lint and Test gates of ci.yml at least.
        self.assertGreaterEqual(gates, 2)


class PagesDeployTests(unittest.TestCase):
    def test_only_the_pages_deploy_is_serialized_and_never_cancelled(self):
        jobs = yaml.safe_load((ROOT / ".github/workflows/post-merge.yml").read_text())[
            "jobs"
        ]
        grouped = {job for job, spec in jobs.items() if "concurrency" in spec}
        self.assertEqual(grouped, {"deploy"})
        # One fixed group for the one Pages site. A pending deploy waits in
        # the queue and is never replaced, and a running one is never
        # cancelled: a cancelled job makes the post-merge run cancelled.
        self.assertEqual(
            jobs["deploy"]["concurrency"],
            {"group": "pages", "cancel-in-progress": False, "queue": "max"},
        )
        # actions/deploy-pages gives up after 10 minutes by default.
        self.assertGreater(jobs["deploy"].get("timeout-minutes", 0), 10)


IMAGE_ACTIONS = ("docker/build-push-action@", "docker/bake-action@")
IMAGE_COMMAND = re.compile(r"\bdocker\s+(?:buildx\s+)?(?:build|bake)\b")


def steps(node):
    """Every step under `node`, at any depth: a mapping with `uses` or `run`."""
    if isinstance(node, dict):
        if isinstance(node.get("uses"), str) or isinstance(node.get("run"), str):
            yield node
        for value in node.values():
            yield from steps(value)
    elif isinstance(node, list):
        for value in node:
            yield from steps(value)


def image_build_settings(step):
    """The text that configures the image build a step runs, or None."""
    if str(step.get("uses", "")).startswith(IMAGE_ACTIONS):
        # build-push-action takes `cache-to`; bake-action takes `set` lines
        # such as `*.cache-to=type=gha`.
        return json.dumps(step.get("with") or {})
    run = step.get("run")
    if isinstance(run, str) and IMAGE_COMMAND.search(run):
        return run
    return None


class ImageCacheTests(unittest.TestCase):
    def test_no_image_build_exports_a_layer_cache(self):
        """Each image's builder copies the whole tree before `cargo build`.

        Every build that a source change starts misses that layer, which holds
        the target directory. Exporting it to the GitHub Actions cache gains
        nothing and evicts other caches from the repository's quota. The rule
        covers the build-push and bake actions and `docker build`,
        `docker buildx build`, and `docker buildx bake` commands.
        """
        builds = []
        for path in tracked(".github/*.yml", ".github/**/*.yml", ".github/**/*.yaml"):
            for step in steps(yaml.safe_load(path.read_text())):
                settings = image_build_settings(step)
                if settings is None:
                    continue
                builds.append(path.name)
                with self.subTest(workflow=path.name, step=step.get("name")):
                    self.assertNotIn("cache-to", settings)
        # Guards against a walk that finds nothing: the xdbg and
        # keepalive-probe images at least.
        self.assertGreaterEqual(len(builds), 2)


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


TOOL_FETCH = re.compile(
    r"\bnpx\b|\bpnpm\s+dlx\b|\bnpm\s+(?:install|i)\b[^\n]*\s(?:-g|--global)\b"
    r"|\bchoco\s+install\b|\|\s*(?:ba)?sh\b"
)


class ToolFetchTests(unittest.TestCase):
    def test_no_step_fetches_a_tool_outside_a_lockfile(self):
        """A step gets its tools from Nix, the pnpm lockfile, or a pinned hash.

        `npx`, `pnpm dlx`, global npm installs, Chocolatey, and piping a
        download into a shell resolve a version at run time.
        """
        fetches = []
        for path in tracked(".github/*.yml", ".github/**/*.yml", ".github/**/*.yaml"):
            for step in steps(yaml.safe_load(path.read_text())):
                uses, run = str(step.get("uses", "")), str(step.get("run", ""))
                if "ghaction-chocolatey" in uses or TOOL_FETCH.search(run):
                    fetches.append(f"{path.relative_to(ROOT)}: {step.get('name')}")
        self.assertEqual(fetches, [])


if __name__ == "__main__":
    unittest.main()
