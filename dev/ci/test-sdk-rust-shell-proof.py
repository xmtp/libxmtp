#!/usr/bin/env python3.11
"""Check proof guards. Actual SDK compilation is covered by the Linux workflow."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location(
    "sdk_rust_shell_proof", Path(__file__).with_name("sdk-rust-shell-proof.py")
)
proof = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(proof)


class ProofTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.fake = self.root / "real-nix"
        self.fake.mkdir()
        executable = self.fake / "nix"
        executable.write_text(
            f"#!{sys.executable}\n"
            "import os,sys\n"
            "args=sys.argv[1:]\n"
            "if '--command' in args:\n"
            " index=args.index('--command')+1\n"
            f" env={{**os.environ,'PATH':{str(self.fake)!r}+':/usr/bin:/bin'}}\n"
            " os.execvpe(args[index],args[index:],env)\n"
            "print('REAL_NIX_REACHED')\n"
        )
        executable.chmod(0o755)
        self.directory = self.root / "guard"
        self.binary = proof.create_guard(self.directory, executable)
        self.environment = {
            **os.environ,
            "PATH": str(self.binary) + os.pathsep + os.environ["PATH"],
            "BASH_ENV": str(self.directory / "bash-env"),
        }

    def command(self, arguments):
        return subprocess.run(
            [str(self.binary / "nix"), *arguments],
            env=self.environment,
            capture_output=True,
            text=True,
        )

    def test_default_local_and_implicit_shells_stop_before_real_nix(self):
        for selector in (".#default", ".#local", "."):
            with self.subTest(selector=selector):
                result = self.command(["develop", selector])
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("SDK_RUST_SHELL_PROOF_BLOCKED", result.stderr)
                self.assertNotIn("REAL_NIX_REACHED", result.stdout)

    def test_targeted_shells_and_non_shell_commands_reach_real_nix(self):
        for arguments in (
            ["develop", ".#rust"],
            ["develop", ".#js"],
            ["develop", ".#js-node"],
            ["build", ".#ubjs-runtime-node"],
        ):
            with self.subTest(arguments=arguments):
                result = self.command(arguments)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("REAL_NIX_REACHED", result.stdout)

    def test_child_path_reset_keeps_guard_before_real_nix(self):
        result = self.command(
            [
                "develop",
                ".#rust",
                "--command",
                "bash",
                "-euc",
                "nix develop .#default",
            ]
        )
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("SDK_RUST_SHELL_PROOF_BLOCKED shell=default", result.stderr)
        calls = [
            json.loads(line)
            for line in (self.directory / "nix-calls.jsonl").read_text().splitlines()
        ]
        self.assertEqual([item["shell"] for item in calls], ["rust", "default"])

    def test_call_log_omits_arguments_and_environment_credentials(self):
        result = self.command(
            [
                "--option",
                "access-tokens",
                "github.com=private-value",
                "eval",
                ".#package",
            ]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        log = (self.directory / "nix-calls.jsonl").read_text()
        self.assertNotIn("private-value", log)
        self.assertEqual(json.loads(log)["command"], "eval")

    def test_cold_clear_removes_only_owned_raw_cache(self):
        cache = self.root / "checkout/target/sdk-artifacts"
        cache.mkdir(parents=True)
        (cache / "artifacts.json").write_text("old receipt")
        registry = self.root / "checkout/cargo/registry/source"
        registry.parent.mkdir(parents=True)
        registry.write_text("keep dependency")
        record = proof.clear_raw(self.root / "checkout")
        self.assertTrue(record["absentAfterClear"])
        self.assertFalse(cache.exists())
        self.assertEqual(registry.read_text(), "keep dependency")

    def test_cold_clear_rejects_escape_without_deleting_external_cache(self):
        checkout = self.root / "checkout"
        checkout.mkdir()
        external = self.root / "external"
        external.mkdir()
        (external / "sdk-artifacts").mkdir()
        (checkout / "target").symlink_to(external, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "owned directory"):
            proof.clear_raw(checkout)
        self.assertTrue((external / "sdk-artifacts").exists())

    def test_cold_evidence_rejects_reused_role(self):
        for target, roles in proof.ROLES.items():
            index = {
                "artifacts": {role: {} for role in roles},
                "execution": [
                    {"role": role, "action": "build"} for role in sorted(roles)
                ],
            }
            proof.check_roles(index, target)
            index["execution"][0]["action"] = "reuse"
            with self.assertRaisesRegex(ValueError, "every raw SDK role to BUILD"):
                proof.check_roles(index, target)


if __name__ == "__main__":
    unittest.main()
