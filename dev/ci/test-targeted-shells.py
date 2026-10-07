#!/usr/bin/env python3
"""Check CI shell inheritance and execute the JavaScript setup shell calls."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "shell_audit", Path(__file__).with_name("check-targeted-shells.py")
)
audit = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(audit)


class ShellAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / ".github/workflows").mkdir(parents=True)
        (self.root / "dev/js").mkdir(parents=True)
        (self.root / "dev/js/sdk-package").write_text(
            "NIX_DEVSHELL=rust dev/nix-shell 'just sdk generate node'\n"
        )

    def workflow(self, text):
        (self.root / ".github/workflows/test.yml").write_text(text)
        return audit.audit(self.root)

    def test_implicit_and_explicit_default_are_rejected(self):
        for command in (
            "dev/nix-shell 'just check'",
            "dev/nix-shell --shell default 'just check'",
            "NIX_DEVSHELL=default dev/nix-shell 'just check'",
            "just check",
        ):
            with self.subTest(command=command), self.assertRaises(ValueError):
                self.workflow(
                    f'jobs:\n  check:\n    steps:\n      - run: "{command}"\n'
                )

    def test_workflow_job_and_step_inheritance_choose_effective_shell(self):
        result = self.workflow("""env:
  NIX_DEVSHELL: rust
jobs:
  check:
    env:
      NIX_DEVSHELL: js-node
    steps:
      - run: dev/nix-shell 'node --version'
      - env:
          NIX_DEVSHELL: ios
        run: dev/nix-shell 'swift --version'
      - run: dev/nix-shell --shell docs 'node --version'
""")
        self.assertEqual(result, {"test.yml/check": ["docs", "ios", "js-node"]})

    def test_full_prepared_javascript_check_has_pinned_browsers(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/check-types.yml").read_text()
        )
        job = workflow["jobs"]["check"]
        step = next(
            step for step in job["steps"] if step.get("run") == "just check-js-prepared"
        )
        inherited = job.get("env", {}).get(
            "NIX_DEVSHELL", workflow.get("env", {}).get("NIX_DEVSHELL")
        )
        self.assertEqual(
            audit.check_steps(ROOT, [step], inherited, {}, "check-types/check"), {"js"}
        )

    def test_step_environment_does_not_leak_to_later_shell_calls(self):
        with self.assertRaises(ValueError):
            self.workflow("""jobs:
  prepare:
    steps:
      - env:
          NIX_DEVSHELL: rust
        run: dev/nix-shell 'python3.11 --version'
      - run: dev/nix-shell 'python3.11 benchmark.py'
""")

    def test_matrix_shells_cannot_fall_back_to_local_default(self):
        text = """jobs:
  check:
    env:
      NIX_DEVSHELL: ${{ matrix.dev-shell }}
    strategy:
      matrix:
        include:
          - name: native
            dev-shell: rust
          - name: browser
            dev-shell: wasm
    steps:
      - run: just test
"""
        self.assertEqual(self.workflow(text), {"test.yml/check": ["rust", "wasm"]})
        with self.assertRaises(ValueError):
            self.workflow(text.replace("dev-shell: wasm", "dev-shell: default"))

    def test_only_fixed_conditional_shell_values_are_accepted(self):
        self.assertEqual(
            audit.resolve("${{ inputs.prepared-products && 'js' || 'rust' }}", {}),
            {"js", "rust"},
        )
        for expression in (
            "${{ inputs.prepared-products && 'js' || inputs.shell }}",
            "${{ inputs.prepared-products && 'js' || 'default' }}",
            None,
            {},
        ):
            self.assertEqual(audit.resolve(expression, {}), set())

    def test_cartesian_exclude_then_include_preserves_every_row(self):
        job = {
            "strategy": {
                "matrix": {
                    "os": ["linux", "mac"],
                    "shell": ["rust", "wasm"],
                    "exclude": [{"os": "linux", "shell": "wasm"}],
                    "include": [{"os": "linux", "shell": "android"}],
                }
            }
        }
        self.assertEqual(
            audit.matrix_rows(job),
            [
                {"os": "linux", "shell": "rust"},
                {"os": "mac", "shell": "rust"},
                {"os": "mac", "shell": "wasm"},
                {"os": "linux", "shell": "android"},
            ],
        )

    def test_include_merges_nonaxis_fields_and_readds_excluded_rows(self):
        matrix = {
            "os": ["linux", "mac"],
            "exclude": [{"os": "mac"}],
            "include": [
                {"shell": "rust"},
                {"os": "mac", "shell": "ios"},
            ],
        }
        self.assertEqual(
            audit.matrix_rows({"strategy": {"matrix": matrix}}),
            [
                {"os": "linux", "shell": "rust"},
                {"os": "mac", "shell": "ios"},
            ],
        )

    def test_json_include_objects_are_checked_in_both_branches(self):
        expression = '${{ fromJSON(inputs.prepared-products && \'[{"dev-shell":"rust"},{"dev-shell":"js-node"}]\' || \'[{"dev-shell":"wasm"}]\') }}'
        data = {
            "jobs": {
                "check": {
                    "env": {"NIX_DEVSHELL": "${{ matrix.dev-shell }}"},
                    "strategy": {"matrix": {"include": expression}},
                    "steps": [{"run": "just test"}],
                }
            }
        }
        self.assertEqual(
            self.workflow(yaml.safe_dump(data)),
            {"test.yml/check": ["js-node", "rust", "wasm"]},
        )
        data["jobs"]["check"]["strategy"]["matrix"]["include"] = expression.replace(
            '"js-node"', '"default"'
        )
        with self.assertRaisesRegex(ValueError, "no targeted shell"):
            self.workflow(yaml.safe_dump(data))

    def test_unknown_json_rows_cannot_supply_a_shell_proof(self):
        data = {
            "jobs": {
                "check": {
                    "env": {"NIX_DEVSHELL": "${{ matrix.dev-shell }}"},
                    "strategy": {"matrix": {"include": "${{ fromJSON(inputs.rows) }}"}},
                    "steps": [{"run": "just test"}],
                }
            }
        }
        with self.assertRaises(ValueError):
            self.workflow(yaml.safe_dump(data))

    def test_excluding_null_does_not_hide_an_unknown_axis(self):
        data = {
            "jobs": {
                "check": {
                    "env": {"NIX_DEVSHELL": "${{ matrix.dev-shell }}"},
                    "strategy": {
                        "matrix": {
                            "dev-shell": "${{ fromJSON(inputs.shells) }}",
                            "exclude": [{"dev-shell": None}],
                        }
                    },
                    "steps": [{"run": "just test"}],
                }
            }
        }
        with self.assertRaises(ValueError):
            self.workflow(yaml.safe_dump(data))

    def test_object_axis_can_resolve_a_nested_shell_field(self):
        data = {
            "jobs": {
                "check": {
                    "env": {"NIX_DEVSHELL": "${{ matrix.config.shell }}"},
                    "strategy": {
                        "matrix": {"config": [{"shell": "rust"}, {"shell": "wasm"}]}
                    },
                    "steps": [{"run": "just test"}],
                }
            }
        }
        self.assertEqual(
            self.workflow(yaml.safe_dump(data)), {"test.yml/check": ["rust", "wasm"]}
        )

    def test_composite_shell_override_does_not_depend_on_caller(self):
        action = self.root / ".github/actions/setup-js/action.yml"
        action.parent.mkdir(parents=True)
        action.write_text("""runs:
  using: composite
  steps:
    - shell: bash
      run: dev/nix-shell --shell js-node 'pnpm install --frozen-lockfile'
""")
        result = self.workflow("""jobs:
  check:
    env:
      NIX_DEVSHELL: android
    steps:
      - uses: ./.github/actions/setup-js
""")
        self.assertEqual(result, {"test.yml/check": ["js-node"]})
        action.write_text(action.read_text().replace(" --shell js-node", ""))
        result = audit.audit(self.root)
        self.assertEqual(result, {"test.yml/check": ["android"]})

    def test_sdk_helper_cannot_restore_the_forced_local_shell(self):
        (self.root / "dev/js/sdk-package").write_text(
            "NIX_DEVSHELL=default dev/nix-shell 'just sdk generate node'\n"
        )
        with self.assertRaises(ValueError):
            audit.audit(self.root)


class SetupJavaScriptTests(unittest.TestCase):
    def test_real_action_commands_always_choose_js_node_for_pnpm(self):
        action = yaml.safe_load(
            (ROOT / ".github/actions/setup-js/action.yml").read_text()
        )
        commands = [
            step["run"]
            for step in action["runs"]["steps"]
            if "dev/nix-shell" in step.get("run", "")
        ]
        self.assertEqual(len(commands), 2)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "dev").mkdir()
            stub = root / "dev/nix-shell"
            stub.write_text(
                f"#!{sys.executable}\n"
                + r"""
import json, os, sys
args = sys.argv[1:]
shell = os.environ.get("NIX_DEVSHELL", "default")
if args[:1] == ["--shell"]:
    shell = args[1]
    args = args[2:]
with open("calls.jsonl", "a") as log:
    log.write(json.dumps({"shell": shell, "command": args, "caller": os.environ.get("NIX_DEVSHELL")}) + "\n")
if args == ["pnpm store path --silent"]:
    print("/fixture/pnpm-store")
elif args == ["command -v pnpm"]:
    print("/fixture/bin/pnpm")
else:
    assert args == ["pnpm install --frozen-lockfile --prefer-offline"]
"""
            )
            stub.chmod(0o755)
            for caller in ("rust", "android", "js", "ios", "default"):
                with self.subTest(caller=caller):
                    log = root / "calls.jsonl"
                    log.unlink(missing_ok=True)
                    environment = dict(
                        os.environ,
                        NIX_DEVSHELL=caller,
                        GITHUB_OUTPUT=str(root / "output"),
                        GITHUB_PATH=str(root / "path"),
                    )
                    for command in commands:
                        subprocess.run(
                            ["bash", "-euc", command],
                            cwd=root,
                            env=environment,
                            check=True,
                        )
                    calls = [json.loads(line) for line in log.read_text().splitlines()]
                    self.assertEqual([call["shell"] for call in calls], ["js-node"] * 3)
                    self.assertEqual([call["caller"] for call in calls], [caller] * 3)
                    self.assertEqual(
                        calls[-1]["command"],
                        ["pnpm install --frozen-lockfile --prefer-offline"],
                    )


if __name__ == "__main__":
    unittest.main()
