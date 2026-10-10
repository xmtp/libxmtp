"""Check the real cache environment helper and CI write policy."""

import os
from pathlib import Path
import re
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
            [str(self.binary), "unset", "50GiB", "CI,XMTP_TEST_LOGGING", "0", "1"],
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
            KACHE_MAX_SIZE="10GiB",
            CARGO_INCREMENTAL="1",
        )
        self.assertEqual(self.values()[:3], ["/official/kache", "0", "10GiB"])
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
            for event, kind, ref, protected, expected in (
                ("push", "branch", "refs/heads/self-hosted", "true", "true"),
                ("push", "branch", "refs/heads/main", "true", "true"),
                ("push", "branch", "refs/heads/self-hosted", "false", "false"),
                ("push", "branch", "refs/heads/main", "", "false"),
                ("push", "branch", "refs/heads/codex/experiment", "true", "false"),
                ("push", "tag", "refs/tags/release", "true", "false"),
                ("pull_request", "branch", "refs/heads/self-hosted", "true", "false"),
                ("pull_request_target", "branch", "refs/heads/main", "true", "false"),
                ("workflow_dispatch", "branch", "refs/heads/main", "true", "false"),
                ("schedule", "branch", "refs/heads/main", "true", "false"),
                ("", "", "", "", "false"),
            ):
                with self.subTest(event=event, kind=kind, ref=ref):
                    output.write_text("")
                    env = dict(
                        os.environ,
                        GITHUB_ACTIONS="true",
                        GITHUB_EVENT_NAME=event,
                        GITHUB_REF_TYPE=kind,
                        GITHUB_REF=ref,
                        GITHUB_REF_PROTECTED=protected,
                        KACHE_CI_BACKEND="s3",
                        KACHE_CI_READONLY="false",
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

    def test_readonly_and_local_jobs_cannot_write_on_protected_pushes(self):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "outputs"
            for backend, readonly, actions in (
                ("s3", "true", "true"),
                ("s3", "", "true"),
                ("local", "false", "true"),
                ("", "false", "true"),
                ("s3", "false", "false"),
            ):
                with self.subTest(backend=backend, readonly=readonly, actions=actions):
                    output.write_text("")
                    result = subprocess.run(
                        [shutil.which("bash"), str(ROOT / "dev/kache-ci-policy")],
                        env={
                            "PATH": os.environ["PATH"],
                            "GITHUB_OUTPUT": str(output),
                            "GITHUB_ACTIONS": actions,
                            "GITHUB_EVENT_NAME": "push",
                            "GITHUB_REF_TYPE": "branch",
                            "GITHUB_REF": "refs/heads/main",
                            "GITHUB_REF_PROTECTED": "true",
                            "KACHE_CI_BACKEND": backend,
                            "KACHE_CI_READONLY": readonly,
                        },
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output.read_text(), "cache-write=false\n")


class KacheBackendTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xmtp kache backend ")
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / "outputs"
        self.summary = Path(self.temp.name) / "summary"
        self.env = {
            "PATH": os.environ["PATH"],
            "GITHUB_OUTPUT": str(self.output),
            "GITHUB_STEP_SUMMARY": str(self.summary),
            "KACHE_CI_BUCKET": "xmtp-libxmtp-kache-test-us-east-2",
            "KACHE_CI_SCOPE": "bindings-check-aarch64-apple-darwin",
            "RUNNER_OS": "macOS",
            "RUNNER_ARCH": "ARM64",
        }

    def run_config(self):
        self.output.write_text("")
        self.summary.write_text("")
        result = subprocess.run(
            [shutil.which("bash"), str(ROOT / "dev/kache-ci-config")],
            env=self.env,
            capture_output=True,
            text=True,
        )
        output = dict(
            line.split("=", 1) for line in self.output.read_text().splitlines()
        )
        return result, output

    def credentials(self):
        self.env.update(
            KACHE_CI_ACCESS_KEY="test-access-key-never-print",
            KACHE_CI_SECRET_KEY="test-secret-key-never-print",
        )

    def test_absent_credentials_select_local_storage_with_or_without_variables(self):
        for metadata in (True, False):
            with self.subTest(metadata=metadata):
                if not metadata:
                    for key in (
                        "KACHE_CI_BUCKET",
                        "KACHE_CI_SCOPE",
                        "RUNNER_OS",
                        "RUNNER_ARCH",
                    ):
                        self.env.pop(key, None)
                result, output = self.run_config()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    output,
                    {"backend": "local", "region": "us-east-2", "manifest-key": ""},
                )
                self.assertIn("local only", self.summary.read_text())

    def test_complete_credentials_select_s3_and_scope_by_variant_os_and_architecture(
        self,
    ):
        self.credentials()
        for scope, platform, arch, region in (
            ("rust-clippy-native", "Linux", "X64", ""),
            ("rust-clippy-native", "Linux", "ARM64", "us-east-2"),
            ("bindings-check-aarch64-apple-darwin", "macOS", "ARM64", "us-west-2"),
        ):
            with self.subTest(scope=scope, platform=platform, arch=arch):
                self.env.update(
                    KACHE_CI_SCOPE=scope,
                    RUNNER_OS=platform,
                    RUNNER_ARCH=arch,
                    KACHE_CI_REGION=region,
                )
                result, output = self.run_config()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    output,
                    {
                        "backend": "s3",
                        "region": region or "us-east-2",
                        "manifest-key": f"{scope}-{platform}-{arch}",
                    },
                )
                emitted = (
                    result.stdout
                    + result.stderr
                    + self.output.read_text()
                    + self.summary.read_text()
                )
                self.assertNotIn(self.env["KACHE_CI_ACCESS_KEY"], emitted)
                self.assertNotIn(self.env["KACHE_CI_SECRET_KEY"], emitted)

    def test_partial_credentials_fail_before_any_backend_output(self):
        for field in ("KACHE_CI_ACCESS_KEY", "KACHE_CI_SECRET_KEY"):
            with self.subTest(field=field):
                self.env.pop("KACHE_CI_ACCESS_KEY", None)
                self.env.pop("KACHE_CI_SECRET_KEY", None)
                self.env[field] = "test-credential-never-print"
                result, output = self.run_config()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("both credential fields", result.stderr)
                self.assertNotIn(self.env[field], result.stdout + result.stderr)
                self.assertEqual(output, {})

    def test_credentials_require_bucket_and_safe_scope_metadata(self):
        self.credentials()
        original = dict(self.env)
        for field, value in (
            ("KACHE_CI_BUCKET", ""),
            ("KACHE_CI_SCOPE", ""),
            ("KACHE_CI_SCOPE", "scope\nbackend=local"),
            ("KACHE_CI_REGION", "us-east-2\nbackend=local"),
            ("KACHE_CI_ENDPOINT", "http://example.com"),
            ("KACHE_CI_ENDPOINT", "https://example.com\nbackend=local"),
            ("RUNNER_OS", ""),
            ("RUNNER_ARCH", ""),
            ("KACHE_CI_READONLY", "invalid"),
        ):
            with self.subTest(field=field, value=value):
                self.env = dict(original)
                self.env[field] = value
                result, output = self.run_config()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(output, {})
                emitted = result.stdout + result.stderr + self.summary.read_text()
                self.assertNotIn(self.env["KACHE_CI_ACCESS_KEY"], emitted)
                self.assertNotIn(self.env["KACHE_CI_SECRET_KEY"], emitted)


class KacheWorkflowWiringTests(unittest.TestCase):
    def fields_for_use(self, relative_path, action, section="with"):
        lines = (ROOT / relative_path).read_text().splitlines()
        indices = [
            index
            for index, line in enumerate(lines)
            if line.strip().startswith(f"uses: {action}")
        ]
        self.assertEqual(len(indices), 1)
        index = indices[0]
        level = len(lines[index]) - len(lines[index].lstrip())
        container = next(
            before
            for before in range(index - 1, -1, -1)
            if lines[before].strip()
            and len(lines[before]) - len(lines[before].lstrip()) < level
        )
        container_level = len(lines[container]) - len(lines[container].lstrip())
        end = next(
            (
                after
                for after in range(index + 1, len(lines))
                if lines[after].strip()
                and len(lines[after]) - len(lines[after].lstrip()) <= container_level
            ),
            len(lines),
        )
        mappings = [
            position
            for position in range(container + 1, end)
            if lines[position].strip() == f"{section}:"
            and len(lines[position]) - len(lines[position].lstrip()) == level
        ]
        self.assertEqual(len(mappings), 1)
        start = mappings[0] + 1
        end = next(
            (
                position
                for position in range(start, end)
                if lines[position].strip()
                and len(lines[position]) - len(lines[position].lstrip()) <= level
            ),
            end,
        )
        children = [
            line
            for line in lines[start:end]
            if len(line) - len(line.lstrip()) == level + 2
        ]
        return {
            key: value.strip().strip('"')
            for key, value in re.findall(
                r"^\s+([\w-]+):[ \t]*(.*)$",
                "\n".join(children),
                re.MULTILINE,
            )
        }

    def test_action_keeps_local_jobs_off_github_and_remote_storage(self):
        fields = self.fields_for_use(
            ".github/actions/setup-nix/action.yml", "kunobi-ninja/kache-action@"
        )
        self.assertEqual(fields["github-cache"].strip("'"), "false")
        self.assertEqual(fields["s3-prefix"], "libxmtp/trusted")
        self.assertEqual(
            fields["save-cache"],
            "${{ steps.compiler-cache-policy.outputs.cache-write }}",
        )
        for key in (
            "s3-bucket",
            "s3-endpoint",
            "s3-access-key-id",
            "s3-secret-access-key",
        ):
            self.assertEqual(
                fields[key],
                "${{ steps.compiler-cache-backend.outputs.backend == 's3' && inputs.kache-"
                + key
                + " || '' }}",
            )
        self.assertEqual(
            fields["s3-region"], "${{ steps.compiler-cache-backend.outputs.region }}"
        )
        for key in ("manifest-key", "namespace"):
            self.assertEqual(
                fields[key], "${{ steps.compiler-cache-backend.outputs.manifest-key }}"
            )

    def test_linux_pilot_receives_reader_secrets_from_its_caller(self):
        # dev/ci-suites generates this caller from the `secrets: kache` suite setting.
        fields = self.fields_for_use(
            ".github/workflows/test-generated.yml",
            "./.github/workflows/check-rust.yml",
            "secrets",
        )
        for secret in ("KACHE_S3_ACCESS_KEY_ID", "KACHE_S3_SECRET_ACCESS_KEY"):
            self.assertEqual(fields.get(secret), "${{ secrets." + secret + " }}")


if __name__ == "__main__":
    unittest.main()
