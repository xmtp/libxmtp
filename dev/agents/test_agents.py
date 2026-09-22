"""Regression tests for the Nix wrapper and Codex hook. No Nix build required."""

import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
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

    def fingerprint(self):
        result = self.run_wrapper('printf "%s" "$XMTP_NIX_WRAPPER_ID"')
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.removeprefix(f"{self.root}:default:")

    def test_batched_fingerprint_matches_per_file_hashes(self):
        for name in [
            "space name.nix",
            "quote'file.nix",
            "line\nbreak.nix",
            "back\\slash.nix",
            "tab\tfile.nix",
            "雪.nix",
            "-option.nix",
        ]:
            (self.root / name).write_text(name)
        for index in range(130):
            (self.root / f"input-{index:03}.nix").write_text(str(index))
        missing = self.root / "removed.nix"
        missing.write_text("removed")
        subprocess.run(["git", "add", "removed.nix"], cwd=self.root, check=True)
        missing.unlink()
        (self.root / "directory.nix").mkdir()
        (self.root / "broken.nix").symlink_to("missing")

        paths = subprocess.check_output(
            [
                "git",
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
                "*.nix",
                "flake.lock",
                "dev/nix-shell",
            ],
            cwd=self.root,
        ).split(b"\0")
        hasher = shutil.which("shasum")
        lines = b"".join(
            subprocess.check_output(
                [hasher, "-a", "256", "--", os.fsdecode(path)], cwd=self.root
            )
            for path in paths
            if path and (self.root / os.fsdecode(path)).is_file()
        )
        expected = hashlib.sha256(lines).hexdigest() + "  -"

        hash_log = self.root / "hash-calls"
        fake = self.root / "bin/shasum"
        fake.write_text(
            '#!/usr/bin/env bash\nprintf "%s\\n" "$#" >> '
            + shlex.quote(str(hash_log))
            + "\nexec "
            + shlex.quote(hasher)
            + ' "$@"\n'
        )
        fake.chmod(0o755)
        self.assertEqual(self.fingerprint(), expected)
        calls = [int(line) for line in hash_log.read_text().splitlines()]
        self.assertEqual(len(calls), 3)  # Two file batches and the final digest.
        self.assertEqual(calls.count(2), 1)
        self.assertTrue(all(count <= 131 for count in calls))

    def test_empty_input_list_hashes_empty_output(self):
        fake = self.root / "bin/git"
        fake.write_text("#!/usr/bin/env bash\nexit 0\n")
        fake.chmod(0o755)
        self.assertEqual(self.fingerprint(), hashlib.sha256(b"").hexdigest() + "  -")

    @unittest.skipUnless(sys.platform == "darwin", "macOS system tools")
    def test_macos_system_bash_and_xargs(self):
        (self.root / "line\nbreak.nix").write_text("portable")
        expected = self.fingerprint()
        self.env["PATH"] = f"{self.root}/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        self.assertEqual(self.fingerprint(), expected)

    def test_input_listing_failure_does_not_enter_nix(self):
        fake = self.root / "bin/git"
        fake.write_text("#!/usr/bin/env bash\nexit 13\n")
        fake.chmod(0o755)
        result = self.run_wrapper("printf should-not-run")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.log.exists())

    def test_file_hash_failure_does_not_enter_nix(self):
        hasher = shutil.which("shasum")
        fake = self.root / "bin/shasum"
        fake.write_text(
            '#!/usr/bin/env bash\nif [ "$#" -gt 2 ]; then exit 17; fi\nexec '
            + shlex.quote(hasher)
            + ' "$@"\n'
        )
        fake.chmod(0o755)
        result = self.run_wrapper("printf should-not-run")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.log.exists())

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

    def test_added_input_reenters(self):
        command = "printf new > added.nix; " + shlex.join([str(self.wrapper), "true"])
        result = self.run_wrapper(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 2)

    def test_removed_input_reenters(self):
        command = "rm flake.lock; " + shlex.join([str(self.wrapper), "true"])
        result = self.run_wrapper(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 2)

    def test_unrelated_file_keeps_environment(self):
        command = "printf note > notes.txt; " + shlex.join([str(self.wrapper), "true"])
        result = self.run_wrapper(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 1)

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
