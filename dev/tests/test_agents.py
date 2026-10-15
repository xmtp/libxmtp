"""Regression tests for the Nix wrapper and Codex hook. No Nix build required."""

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

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "dev/agents"))
from nix_hook import rewrite  # noqa: E402


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
        # A loaded worktree environment exports its slot, which would make
        # dev/docker/load-env regenerate the file.
        for name in (
            "XMTP_NIX_WRAPPER_ID",
            "NIX_DEVSHELL",
            "TEST_NIX_FAIL",
            "NIX_BUILD_TOP",
            "TMP",
            "TEMP",
            "TEMPDIR",
            "XMTP_WORKTREE_SLOT",
            "XMTP_WORKTREE_NAME",
        ):
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
            'printf "%s|%s" "$XMTP_TEST_SHELL" "${NIX_BUILD_TOP-unset}"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        # The per-entry directory is removed before the command runs.
        self.assertEqual(result.stdout, f"{self.root}#default|unset")

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

    def test_environment_runs_in_its_own_bash_and_keeps_host_tools(self):
        # The environment names its stdenv bash (CONFIG_SHELL), sets PATH to
        # its own directories, and appends the caller's PATH at the end, as
        # `nix print-dev-env` does. The caller's `bash` here cannot run it (on
        # macOS it is bash 3.2), and a host tool must still resolve.
        real_bash = shutil.which("bash")
        dev = self.root / "dev-env-bin"
        dev.mkdir()
        (dev / "bash").symlink_to(real_bash)
        host = self.root / "host-bin"
        host.mkdir()
        # A host `bash` that runs scripts but cannot run the environment.
        for name, body in (
            ("bash", f'case "$1" in -c) exit 97 ;; esac\nexec {real_bash} "$@"'),
            ("host-tool", "echo host tool"),
        ):
            (host / name).write_text(f"#!/bin/sh\n{body}\n")
            (host / name).chmod(0o755)
        (self.root / "bin/nix").write_text(
            "#!/usr/bin/env bash\nset -eu\n"
            'mkdir -p "$TEST_STORE"; ln -sfn "$TEST_STORE" "$3"\n'
            f"printf '%s\\n' \"CONFIG_SHELL='{real_bash}'\" "
            "'nix_saved_PATH=\"$PATH\"' "
            f"\"PATH='{dev}'\" "
            "'PATH=\"$PATH${nix_saved_PATH:+:$nix_saved_PATH}\"'\n"
        )
        self.env["PATH"] = f"{host}:{self.env['PATH']}"
        result = self.run_wrapper("host-tool")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "host tool\n")

    def test_commands_keep_the_callers_temporary_directory(self):
        # The environment points TMPDIR at a per-entry directory; here it has
        # already removed it. An outer shell's NIX_BUILD_TOP is not ours to
        # remove.
        caller = self.root / "caller-tmp"
        caller.mkdir()
        self.env.update(TMPDIR=str(caller), NIX_BUILD_TOP=str(caller))
        make_temp = shlex.join(
            [sys.executable, "-c", "import tempfile; print(tempfile.mkstemp()[1])"]
        )
        for name, environment in (
            (
                "removed",
                'export NIX_BUILD_TOP="$(mktemp -d)"; export TMPDIR="$NIX_BUILD_TOP"; rmdir "$TMPDIR"',
            ),
            ("inherited", "export TMPDIR=/nonexistent-entry-dir"),
        ):
            with self.subTest(environment=name):
                (self.root / "bin/nix").write_text(
                    "#!/usr/bin/env bash\nset -eu\n"
                    'mkdir -p "$TEST_STORE"; ln -sfn "$TEST_STORE" "$3"\n'
                    f"printf '%s\\n' {shlex.quote(environment)}\n"
                )
                result = self.run_wrapper("--refresh", make_temp)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(result.stdout.startswith(str(caller)), result.stdout)
                self.assertTrue(caller.is_dir())

    def test_starts_incremental_cleanup_for_its_worktree(self):
        marker = self.root / "cleanup-args"
        stub = self.root / "dev/clean-incremental"
        stub.write_text(
            f'#!/bin/sh\nprintf "%s\\n" "$@" > {shlex.quote(str(marker))}\n'
        )
        stub.chmod(0o755)
        result = self.run_wrapper("true")
        self.assertEqual(result.returncode, 0, result.stderr)
        # The cleanup runs in the background, so the command does not wait.
        deadline = time.monotonic() + 10
        while not marker.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        self.assertEqual(
            marker.read_text().splitlines(), ["--auto", f"{self.root}/target"]
        )

    def test_failed_evaluation_caches_nothing(self):
        self.env["TEST_NIX_FAIL"] = "1"
        self.assertNotEqual(self.run_wrapper("printf no").returncode, 0)
        del self.env["TEST_NIX_FAIL"]
        self.assertEqual(self.run_wrapper("true").returncode, 0)
        self.assertEqual(len(self.calls()), 2)

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


if __name__ == "__main__":
    unittest.main()
