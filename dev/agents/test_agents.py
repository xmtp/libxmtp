"""Regression tests for the Nix wrapper and Codex hook. No Nix build required."""

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

from nix_hook import rewrite

ROOT = Path(__file__).resolve().parents[2]


class NixWrapperTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xmtp agent tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "dev").mkdir()
        self.wrapper = self.root / "dev/nix-shell"
        shutil.copy2(ROOT / "dev/nix-shell", self.wrapper)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        (self.root / "flake.lock").write_text("initial")
        self.log = self.root / "nix-calls"
        binary = self.root / "bin"
        binary.mkdir()
        fake = binary / "nix"
        fake.write_text(
            "#!/usr/bin/env bash\nset -eu\n"
            'printf "%s\\n" "$2" >> "$TEST_NIX_LOG"\n'
            'while [[ "$1" != --command ]]; do shift; done\nshift\nexec "$@"\n'
        )
        fake.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{binary}:{os.environ['PATH']}",
            "TEST_NIX_LOG": str(self.log),
        }
        self.env.pop("XMTP_NIX_WRAPPER_ID", None)
        self.env.pop("NIX_DEVSHELL", None)

    def run_wrapper(self, *args):
        return subprocess.run(
            [str(self.wrapper), *args],
            cwd=self.root,
            env=self.env,
            text=True,
            capture_output=True,
        )

    def calls(self):
        return self.log.read_text().splitlines()

    def test_exact_arguments_and_cwd(self):
        result = self.run_wrapper(
            "--command",
            "bash",
            "-c",
            'printf "%s\\n" "$PWD" "$1" "$2"',
            "test",
            "a ' b $x",
            "",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, f"{self.root.resolve()}\na ' b $x\n\n")

    def test_legacy_pipeline_and_exit(self):
        result = self.run_wrapper("printf '%s' 'a b' | tr ' ' _; exit 37")
        self.assertEqual(result.stdout, "a_b")
        self.assertEqual(result.returncode, 37)

    def test_nested_reuse(self):
        result = self.run_wrapper(shlex.join([str(self.wrapper), "printf nested"]))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "nested")
        self.assertEqual(len(self.calls()), 1)

    def test_changed_shell_reenters(self):
        result = self.run_wrapper(
            shlex.join([str(self.wrapper), "--shell", "rust", "true"])
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [call.rsplit("#", 1)[1] for call in self.calls()], ["default", "rust"]
        )

    def test_changed_input_reenters(self):
        command = "printf changed > flake.lock; " + shlex.join(
            [str(self.wrapper), "true"]
        )
        result = self.run_wrapper(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 2)

    def test_foreign_marker_is_not_reused(self):
        self.env["XMTP_NIX_WRAPPER_ID"] = "/other:default:old"
        self.env["IN_NIX_SHELL"] = "impure"
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 1)

    def test_invalid_arguments(self):
        self.assertEqual(self.run_wrapper().returncode, 2)
        self.assertEqual(self.run_wrapper("--command").returncode, 2)
        self.assertEqual(self.run_wrapper("one", "two").returncode, 2)

    def test_hook_preserves_permissions_and_input(self):
        payload = {
            "tool_name": "Bash",
            "cwd": str(self.root),
            "tool_input": {"command": "printf '%s' 'a b'; exit 37", "timeout_ms": 1234},
        }
        response = rewrite(payload, self.root)
        output = response["hookSpecificOutput"]
        self.assertEqual(output["hookEventName"], "PreToolUse")
        self.assertEqual(output["permissionDecision"], "allow")
        self.assertNotIn("decision", output)
        self.assertEqual(output["updatedInput"]["timeout_ms"], 1234)
        result = subprocess.run(
            ["bash", "-c", output["updatedInput"]["command"]],
            cwd=self.root,
            env=self.env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertEqual(result.stdout, "a b")

    def test_hook_scope(self):
        self.assertEqual(rewrite({"tool_name": "Read"}, self.root), {})
        self.assertEqual(
            rewrite(
                {"tool_name": "Bash", "cwd": "/", "tool_input": {"command": "true"}},
                self.root,
            ),
            {},
        )
        self.assertEqual(
            rewrite({"tool_name": "Bash", "tool_input": {}}, self.root), {}
        )

    def test_hook_does_not_enable_errexit(self):
        response = rewrite(
            {
                "tool_name": "Bash",
                "cwd": str(self.root),
                "tool_input": {"command": "false; printf continued"},
            },
            self.root,
        )
        command = response["hookSpecificOutput"]["updatedInput"]["command"]
        result = subprocess.run(
            ["bash", "-c", command],
            cwd=self.root,
            env=self.env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "continued")

    def test_hook_json_contract(self):
        result = subprocess.run(
            [shutil.which("python3"), str(ROOT / "dev/agents/nix_hook.py")],
            input=json.dumps(
                {
                    "tool_name": "Bash",
                    "cwd": str(ROOT),
                    "tool_input": {"command": "true"},
                }
            ),
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertIn("updatedInput", json.loads(result.stdout)["hookSpecificOutput"])

    def test_hook_compact_default_and_explicit_bypass(self):
        for value, expected in ((None, "1"), ("0", "0"), ("", "")):
            with self.subTest(value=value):
                self.env.pop("XMTP_RTK", None)
                if value is not None:
                    self.env["XMTP_RTK"] = value
                response = rewrite(
                    {
                        "tool_name": "Bash",
                        "cwd": str(self.root),
                        "tool_input": {"command": 'printf "%s" "$XMTP_RTK"'},
                    },
                    self.root,
                )
                command = response["hookSpecificOutput"]["updatedInput"]["command"]
                result = subprocess.run(
                    ["bash", "-c", command],
                    cwd=self.root,
                    env=self.env,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, expected)


if __name__ == "__main__":
    unittest.main()
