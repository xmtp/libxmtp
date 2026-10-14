"""Regression tests for the Nix wrapper and Codex hook. No Nix build required."""

import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

from nix_hook import rewrite

ROOT = Path(__file__).resolve().parents[2]


class NixWrapperTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xmtp agent tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        (self.root / "dev").mkdir()
        (self.root / "nix").mkdir()
        self.wrapper = self.root / "dev/nix-shell"
        shutil.copy2(ROOT / "dev/nix-shell", self.wrapper)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        (self.root / "flake.lock").write_text("initial")
        self.log = self.root / "nix-calls"
        binary = self.root / "bin"
        binary.mkdir()
        # A `nix print-dev-env` that logs the flake reference and prints an
        # environment, like the real one, with a per-entry temporary directory.
        fake = binary / "nix"
        fake.write_text(
            "#!/usr/bin/env bash\nset -eu\n"
            'printf "%s\\n" "${@: -1}" >> "$TEST_NIX_LOG"\n'
            'if [[ -n "${TEST_NIX_FAIL:-}" ]]; then exit 1; fi\n'
            # The GC root: a profile link and its generation link.
            'mkdir -p "$TEST_STORE"; ln -sfn "$TEST_STORE" "$3-1-link"; ln -sfn "$(basename "$3")-1-link" "$3"\n'
            "printf 'export XMTP_TEST_SHELL=%q\\n' \"${@: -1}\"\n"
            "printf '%s\\n' 'export NIX_BUILD_TOP=\"$(mktemp -d)\"'\n"
        )
        fake.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{binary}:{os.environ['PATH']}",
            "TEST_NIX_LOG": str(self.log),
            "XDG_CACHE_HOME": str(self.root / "cache"),
            "TEST_STORE": str(self.root / "store/environment"),
        }
        for name in ("XMTP_NIX_WRAPPER_ID", "NIX_DEVSHELL", "TEST_NIX_FAIL"):
            self.env.pop(name, None)

    def run_wrapper(self, *args):
        return subprocess.run(
            [str(self.wrapper), *args],
            cwd=self.root,
            env=self.env,
            text=True,
            capture_output=True,
        )

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def key(self):
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
            (self.root / "nix" / name).write_text(name)
        for index in range(130):
            (self.root / f"nix/input-{index:03}.nix").write_text(str(index))
        missing = self.root / "nix/removed.nix"
        missing.write_text("removed")
        subprocess.run(["git", "add", "nix/removed.nix"], cwd=self.root, check=True)
        missing.unlink()
        (self.root / "nix/directory.nix").mkdir()
        (self.root / "nix/broken.nix").symlink_to("missing")

        paths = subprocess.check_output(
            [
                "git",
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
                "nix",
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
        fingerprint = hashlib.sha256(lines).hexdigest() + "  -"
        system = subprocess.check_output(["uname", "-sm"], text=True).strip()
        expected = (
            "default-"
            + hashlib.sha256(f"{system}\ndefault\n{fingerprint}\n".encode()).hexdigest()
        )

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
        self.assertEqual(self.key(), expected)
        calls = [int(line) for line in hash_log.read_text().splitlines()]
        # Two file batches, then the fingerprint and the key digests.
        self.assertEqual(len(calls), 4)
        self.assertEqual(calls.count(2), 2)
        self.assertTrue(all(count <= 131 for count in calls))

    @unittest.skipUnless(sys.platform == "darwin", "macOS system tools")
    def test_macos_system_bash_and_xargs(self):
        (self.root / "nix/line\nbreak.nix").write_text("portable")
        expected = self.key()
        self.env["PATH"] = f"{self.root}/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        self.assertEqual(self.key(), expected)

    def test_input_listing_failure_does_not_enter_nix(self):
        fake = self.root / "bin/git"
        fake.write_text("#!/usr/bin/env bash\nexit 13\n")
        fake.chmod(0o755)
        result = self.run_wrapper("printf should-not-run")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(self.calls(), [])

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
        self.assertEqual(self.calls(), [])

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

    def test_environment_and_temporary_directory(self):
        result = self.run_wrapper(
            'printf "%s" "$XMTP_TEST_SHELL"; echo "$NIX_BUILD_TOP" >&2'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, f"{self.root}#default")
        self.assertFalse(Path(result.stderr.strip()).exists())

    def test_source_edits_reuse_the_cached_environment(self):
        for edit in (
            "src/lib.rs",
            "Cargo.lock",
            "Cargo.toml",
            "sdks/node/package.json",
        ):
            path = self.root / edit
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(edit)
            self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 1)

    def test_key_inputs_evaluate_again(self):
        (self.root / "nix/shells").mkdir()
        (self.root / "nix/shells/rust.nix").write_text("{ }")
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        for edit in (
            "nix/shells/rust.nix",
            "flake.lock",
            "package.json",
            "dev/nix-shell",
        ):
            with (self.root / edit).open("a") as file:
                file.write("\n# edit")
            self.assertEqual(self.run_wrapper("true").returncode, 0)
        (self.root / "flake.lock").unlink()
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 6)

    def test_refresh_evaluates_again(self):
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(self.run_wrapper("--refresh", "true").returncode, 0)
        self.assertEqual(len(self.calls()), 2)

    def test_reuse_keeps_the_gc_root_of_an_environment_in_use(self):
        cache = self.root / "cache/xmtp/nix-shell"
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        entry = sorted(cache.iterdir())
        week = time.time() - 8 * 24 * 3600
        for path in entry:
            os.utime(path, (week, week), follow_symlinks=False)
        # A reuse, then another key of the same shell, which cleans up.
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        (self.root / "nix/added.nix").write_text("{ }")
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertTrue(all(path.is_symlink() or path.exists() for path in entry))
        self.assertEqual(len(self.calls()), 2)

    def test_lost_gc_root_evaluates_again(self):
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        # A garbage collection removed the environment the root pointed at.
        os.rmdir(self.env["TEST_STORE"])
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 2)
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 2)

    def test_failed_evaluation_caches_nothing(self):
        self.env["TEST_NIX_FAIL"] = "1"
        self.assertNotEqual(self.run_wrapper("printf no").returncode, 0)
        del self.env["TEST_NIX_FAIL"]
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 2)

    def test_print_env_for_direnv(self):
        result = self.run_wrapper("--print-env", "rust")
        self.assertEqual(result.returncode, 0, result.stderr)
        loaded = subprocess.run(
            [
                "bash",
                "-c",
                result.stdout
                + '\nprintf "%s|%s|%s" "$XMTP_TEST_SHELL" "${NIX_BUILD_TOP-unset}" "$XMTP_NIX_WRAPPER_ID"',
            ],
            env={**self.env, "TMPDIR": "/caller-tmp"},
            text=True,
            capture_output=True,
        )
        self.assertEqual(loaded.returncode, 0, loaded.stderr)
        shell, top, marker = loaded.stdout.split("|")
        self.assertEqual((shell, top), (f"{self.root}#rust", "unset"))
        self.assertTrue(marker.startswith(f"{self.root}:rust:rust-"), marker)
        # The marker lets a nested call reuse the direnv environment.
        nested = self.run_wrapper("--shell", "rust", "true")
        self.assertEqual(nested.returncode, 0)
        self.assertEqual(len(self.calls()), 1)

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

    def test_foreign_marker_is_not_reused(self):
        self.env["XMTP_NIX_WRAPPER_ID"] = "/other:default:old"
        self.env["IN_NIX_SHELL"] = "impure"
        result = self.run_wrapper('printf "%s" "$XMTP_TEST_SHELL"')
        self.assertEqual(result.stdout, f"{self.root}#default")

    def test_shell_files_read_only_key_inputs(self):
        # A shell file that reads a repository file outside nix/ must list it
        # in the wrapper's key, or an edit to it reuses a stale environment.
        text = (ROOT / "dev/nix-shell").read_text()
        listed = text.split("key_paths=(", 1)[1].split(")", 1)[0].split()
        shell_files = [
            *ROOT.glob("nix/shells/*.nix"),
            *(ROOT / "nix" / name for name in ("js.nix", "js-node.nix", "docs.nix")),
            *(
                ROOT / "nix/lib" / name
                for name in (
                    "shell-common.nix",
                    "android-env.nix",
                    "ios-env.nix",
                    "packages/kache.nix",
                    "packages/pnpm.nix",
                )
            ),
        ]
        for path in shell_files:
            for match in re.finditer(
                r"(?<![\w.$/-])(\.\.?(?:/[\w.@+-]+)+)", path.read_text()
            ):
                read = (path.parent / match.group(1)).resolve().relative_to(ROOT)
                with self.subTest(file=path.name, reads=str(read)):
                    self.assertTrue(
                        any(read.is_relative_to(entry) for entry in listed),
                        f"{read} is not in dev/nix-shell key_paths",
                    )

    def show_service_env(self):
        for name in ("DATABASE_URL", "XMTP_BACKEND_URL", "SQLX_OFFLINE"):
            self.env.pop(name, None)
        return 'printf "%s|%s|%s" "${DATABASE_URL-unset}" "${XMTP_BACKEND_URL-unset}" "$SQLX_OFFLINE"'

    def test_loads_worktree_env_without_overriding_the_caller(self):
        # An ad-hoc `cargo test` must reach this worktree's stack, not slot 0's.
        docker = self.root / "dev/docker"
        docker.mkdir()
        shutil.copy2(ROOT / "dev/docker/load-env", docker / "load-env")
        (docker / ".env").write_text(
            "DATABASE_URL='postgres://xmtp:xmtp@127.0.0.1:55532/xmtp_backend'\n"
            "XMTP_BACKEND_URL='http://127.0.0.1:5150'\n"
        )
        command = self.show_service_env()
        self.env["XMTP_BACKEND_URL"] = "http://caller:1"
        result = self.run_wrapper(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout,
            "postgres://xmtp:xmtp@127.0.0.1:55532/xmtp_backend|http://caller:1|true",
        )

    def test_no_worktree_env_and_caller_sqlx_mode(self):
        command = self.show_service_env()
        self.env["SQLX_OFFLINE"] = "false"
        result = self.run_wrapper(command)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "unset|unset|false")

    def test_invalid_arguments(self):
        self.assertEqual(self.run_wrapper().returncode, 2)
        self.assertEqual(self.run_wrapper("--command").returncode, 2)
        self.assertEqual(self.run_wrapper("one", "two").returncode, 2)
        self.assertEqual(self.run_wrapper("--print-env", "rust", "x").returncode, 2)

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
