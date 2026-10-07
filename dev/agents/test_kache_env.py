"""Check the real cache environment helper and CI write policy."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class KacheEnvTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xmtp kache env ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bash = shutil.which("bash")
        self.binary = self.root / "kache"
        self.binary.write_text("#!/bin/sh\nexit 99\n")
        self.binary.chmod(0o755)
        self.env = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith(("KACHE_", "XMTP_KACHE"))
            and key not in ("CI", "CARGO_INCREMENTAL", "RUSTC_WRAPPER", "BASH_ENV")
        }
        self.env["PATH"] = str(self.root)

    def shell(self, script):
        return subprocess.run(
            [self.bash, "-euc", script, "test", str(ROOT / "dev/kache-env")],
            env=self.env,
            capture_output=True,
            text=True,
        )

    def values(self):
        result = self.shell(
            'source "$1"; printf "%s\\n" "${RUSTC_WRAPPER-unset}" '
            '"${CARGO_INCREMENTAL-unset}" "${KACHE_MAX_SIZE-unset}" '
            '"${KACHE_KEY_ENV_VARS-unset}" "${KACHE_BUILD_SCRIPT_CACHE-unset}" '
            '"${KACHE_CACHE_EXECUTABLES-unset}"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.splitlines()

    def test_local_defaults_select_cache_and_keep_incremental_profile(self):
        values = self.values()
        self.assertEqual(
            values,
            [str(self.binary), "unset", "10GiB", "CI,XMTP_TEST_LOGGING", "0", "1"],
        )
        for value in ("0", "1"):
            with self.subTest(value=value):
                self.env["CARGO_INCREMENTAL"] = value
                self.assertEqual(self.values()[1], value)

    def test_ci_opt_in_preserves_action_paths_and_sets_trial_policy(self):
        self.env.update(
            CI="true",
            XMTP_KACHE="1",
            RUSTC_WRAPPER="/official/kache",
            KACHE_CACHE_DIR="/private/store",
            KACHE_RUNTIME_DIR="/private/runtime",
            CARGO_INCREMENTAL="1",
        )
        self.assertEqual(self.values()[:2], ["/official/kache", "0"])
        result = self.shell(
            'source "$1"; printf "%s\\n" "$KACHE_CACHE_DIR" "$KACHE_RUNTIME_DIR" '
            '"$KACHE_ADAPTIVE_INCREMENTAL" "$KACHE_PRESERVE_INCREMENTAL"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.splitlines(), ["/private/store", "/private/runtime", "0", "0"]
        )

    def test_ci_requires_explicit_cache_opt_in(self):
        self.env["CI"] = "true"
        self.assertEqual(self.values(), ["unset"] * 6)

    def test_opt_out_removes_only_our_selected_wrapper(self):
        result = self.shell(
            'source "$1"; XMTP_KACHE=0; source "$1"; printf "%s" "${RUSTC_WRAPPER-unset}"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "unset")
        self.env.update(XMTP_KACHE="0", RUSTC_WRAPPER="/custom/instrumentation")
        self.assertEqual(self.values()[0], "/custom/instrumentation")

    def test_extra_keyed_inputs_and_store_limit_are_preserved(self):
        self.env.update(KACHE_KEY_ENV_VARS="CUSTOM_*", KACHE_MAX_SIZE="64MiB")
        self.assertEqual(self.values()[2:4], ["64MiB", "CUSTOM_*,CI,XMTP_TEST_LOGGING"])
        result = self.shell(
            'source "$1"; source "$1"; printf "%s" "$KACHE_KEY_ENV_VARS"'
        )
        self.assertEqual(result.stdout, "CUSTOM_*,CI,XMTP_TEST_LOGGING")

    def test_missing_binary_fails_without_changing_flags(self):
        self.binary.unlink()
        self.env.update(CARGO_INCREMENTAL="1", RUSTC_WRAPPER="/previous/wrapper")
        result = self.shell(
            'if source "$1"; then exit 98; fi; printf "%s\\n" "$RUSTC_WRAPPER" "$CARGO_INCREMENTAL"'
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines(), ["/previous/wrapper", "1"])
        self.assertIn("kache not found", result.stderr)


class DarwinWrapperTests(unittest.TestCase):
    def test_linked_outputs_pass_through_and_library_compiles_use_cache(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            wrapper = folder / "xmtp-kache"
            shutil.copy2(ROOT / "dev/kache-darwin-wrapper", wrapper)
            for name, marker in (("rustc", "direct"), ("kache", "cache")):
                executable = folder / name
                executable.write_text(
                    '#!/bin/sh\nprintf "' + marker + '\\n"\nprintf "%s\\n" "$@"\n'
                )
                executable.chmod(0o755)
            for flags, direct in (
                (["--crate-type", "bin"], True),
                (["--crate-type=cdylib"], True),
                (["--crate-type=dylib"], True),
                (["--crate-type=proc-macro"], True),
                (["--crate-type=staticlib"], True),
                (["--crate-type=lib,cdylib"], True),
                (["--crate-type=rlib", "--test"], True),
                (["--crate-type=rlib"], False),
                (["--crate-type", "lib"], False),
                (["-vV"], True),
            ):
                with self.subTest(flags=flags):
                    argv = [
                        str(folder / "rustc"),
                        *flags,
                        "source file.rs",
                        "-Cdebug-assertions=yes",
                    ]
                    result = subprocess.run(
                        [shutil.which("bash"), str(wrapper), *argv],
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    lines = result.stdout.splitlines()
                    self.assertEqual(lines[0], "direct" if direct else "cache")
                    self.assertEqual(lines[1:], argv[1:] if direct else argv)


class KacheWritePolicyTests(unittest.TestCase):
    def test_real_policy_allows_only_trusted_branch_pushes(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "outputs"
            for event, kind, ref, expected in (
                ("push", "branch", "refs/heads/self-hosted", "true"),
                ("push", "branch", "refs/heads/main", "true"),
                ("push", "branch", "refs/heads/codex/experiment", "false"),
                ("push", "tag", "refs/tags/release", "false"),
                ("pull_request", "branch", "refs/heads/self-hosted", "false"),
                ("pull_request_target", "branch", "refs/heads/main", "false"),
                ("workflow_dispatch", "branch", "refs/heads/main", "false"),
                ("", "", "", "false"),
            ):
                with self.subTest(event=event, kind=kind, ref=ref):
                    output.write_text("")
                    env = dict(
                        os.environ,
                        GITHUB_ACTIONS="true",
                        GITHUB_EVENT_NAME=event,
                        GITHUB_REF_TYPE=kind,
                        GITHUB_REF=ref,
                        GITHUB_OUTPUT=str(output),
                    )
                    result = subprocess.run(
                        [shutil.which("bash"), str(ROOT / "dev/kache-ci-policy")],
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output.read_text(), f"cache-write={expected}\n")


if __name__ == "__main__":
    unittest.main()
