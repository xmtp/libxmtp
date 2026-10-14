"""Check the real cache environment helper and CI write policy."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

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


TRUNK_PUSH = {
    "GITHUB_EVENT_NAME": "push",
    "GITHUB_REF_TYPE": "branch",
    "GITHUB_REF": "refs/heads/main",
    "GITHUB_REF_PROTECTED": "true",
}


class CachePolicyTests(unittest.TestCase):
    """dev/kache-ci-policy: the cache mode, the event, and the tokens."""

    def policy(self, **values):
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder) / "outputs"
            output.write_text("")
            env = {
                "PATH": os.environ["PATH"],
                "GITHUB_OUTPUT": str(output),
                "GITHUB_ACTIONS": "true",
                **values,
            }
            result = subprocess.run(
                [shutil.which("bash"), str(ROOT / "dev/kache-ci-policy")],
                env=env,
                capture_output=True,
                text=True,
            )
            return result, output.read_text()

    def test_trusted_mode_runs_only_on_protected_trunk_pushes(self):
        for event, kind, ref, protected, allowed in (
            ("push", "branch", "refs/heads/self-hosted", "true", True),
            ("push", "branch", "refs/heads/main", "true", True),
            ("push", "branch", "refs/heads/self-hosted", "false", False),
            ("push", "branch", "refs/heads/main", "", False),
            ("push", "branch", "refs/heads/codex/experiment", "true", False),
            ("push", "tag", "refs/tags/release", "true", False),
            ("pull_request", "branch", "refs/heads/self-hosted", "true", False),
            ("pull_request_target", "branch", "refs/heads/main", "true", False),
            ("workflow_dispatch", "branch", "refs/heads/main", "true", False),
            ("schedule", "branch", "refs/heads/main", "true", False),
            ("", "", "", "", False),
        ):
            with self.subTest(event=event, kind=kind, ref=ref, protected=protected):
                result, output = self.policy(
                    XMTP_CACHE_MODE="trusted",
                    GITHUB_EVENT_NAME=event,
                    GITHUB_REF_TYPE=kind,
                    GITHUB_REF=ref,
                    GITHUB_REF_PROTECTED=protected,
                    KACHE_CI_BACKEND="s3",
                    KACHE_CI_READONLY="false",
                )
                if allowed:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output, "cache-write=true\n")
                else:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("protected push", result.stderr)
                    self.assertEqual(output, "")
        result, _ = self.policy(
            XMTP_CACHE_MODE="trusted", GITHUB_ACTIONS="", **TRUNK_PUSH
        )
        self.assertNotEqual(result.returncode, 0)

    def test_only_trusted_writer_jobs_upload_to_kache(self):
        for mode, backend, readonly, expected in (
            ("trusted", "s3", "false", "true"),
            ("ci", "s3", "false", "false"),
            ("read", "s3", "false", "false"),
            ("", "s3", "false", "false"),
            ("trusted", "s3", "true", "false"),
            ("trusted", "s3", "", "false"),
            ("trusted", "local", "false", "false"),
            ("trusted", "", "false", "false"),
        ):
            with self.subTest(mode=mode, backend=backend, readonly=readonly):
                result, output = self.policy(
                    XMTP_CACHE_MODE=mode,
                    KACHE_CI_BACKEND=backend,
                    KACHE_CI_READONLY=readonly,
                    **TRUNK_PUSH,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output, f"cache-write={expected}\n")

    def test_each_mode_accepts_only_its_own_token(self):
        for mode, trusted, pr, allowed in (
            ("ci", "false", "true", True),
            ("ci", "true", "true", True),
            ("trusted", "true", "false", True),
            ("read", "false", "false", True),
            ("read", "false", "true", False),
            ("trusted", "true", "true", False),
            ("read", "true", "false", False),
            ("write", "false", "false", False),
        ):
            with self.subTest(mode=mode, trusted=trusted, pr=pr):
                result, output = self.policy(
                    XMTP_CACHE_MODE=mode,
                    XMTP_CACHIX_TOKEN=trusted,
                    XMTP_CACHIX_PR_TOKEN=pr,
                    **TRUNK_PUSH,
                )
                self.assertEqual(result.returncode == 0, allowed, result.stderr)
                if not allowed:
                    self.assertEqual(output, "")


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
            "${{ steps.cache-policy.outputs.cache-write }}",
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


ACTION = ROOT / ".github/actions/setup-nix/action.yml"
SETUP_NIX = "./.github/actions/setup-nix"
SETUP_GRADLE = "gradle/actions/setup-gradle@"
S3_INPUTS = (
    "kache-s3-bucket",
    "kache-s3-region",
    "kache-s3-endpoint",
    "kache-s3-access-key-id",
    "kache-s3-secret-access-key",
)

# Jobs that select the writer environment only for its trusted Cachix token,
# not for Kache. Each runs only on pushes to trunk.
TRUSTED_CACHIX_WRITERS = {"post-merge.yml / build"}

# Jobs that run cargo outside Nix, with their Kache scope. A job leaves this
# list only when it stops running cargo outside Nix, so dropping Kache from a
# cargo job fails here even when its README row goes too.
KACHE_CALLERS = {
    "build-sdk-node-platforms.yml / assemble": "sdk-native-release",
    "check-rust.yml / native": "rust-clippy-native",
    "check-rust.yml / wasm": "rust-clippy-wasm",
    "check-sdk-products.yml / generated": "sdk-products-debug",
    "check-sdk-unit.yml / unit": "sdk-tests-debug",
    "check-types.yml / check": "sdk-products-debug",
    "deploy-docs.yml / site": "sdk-products-debug",
    "deploy-web-chat.yml / deploy": "sdk-products-debug",
    "docs-rust-reference.yml / rust": "rust-reference",
    "manual-sdk-recovery.yml / recovery": "sdk-native-debug",
    "release-agent-sdk.yml / build": "sdk-native-debug",
    "release-browser-sdk.yml / build": "sdk-products-debug",
    "release-cli.yml / build": "sdk-native-debug",
    "test-agent-sdk.yml / test": "sdk-native-debug",
    "test-android.yml / android-stage": "sdk-android-release",
    "test-backend.yml / native": "backend-tests-debug",
    "test-browser-sdk.yml / test": "sdk-products-debug",
    "test-node-sdk.yml / test": "sdk-native-debug",
    "test-sdk-platform.yml / proof": "sdk-browser-conformance",
    "test-sdk.yml / sdk": "sdk-products-tests-conformance",
    "test-sdk.yml / android-stage": "sdk-android-release",
}


def setup_nix_calls():
    """Yield (workflow, job id, job, inputs) for every setup-nix call."""
    for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
        document = yaml.safe_load(path.read_text())
        for job_id, job in (document.get("jobs") or {}).items():
            for step in job.get("steps") or []:
                if step.get("uses") == SETUP_NIX:
                    yield path.name, job_id, job, step.get("with") or {}


def action_step(name):
    steps = yaml.safe_load(ACTION.read_text())["runs"]["steps"]
    return next(step for step in steps if step.get("name") == name)


class SetupNixTests(unittest.TestCase):
    def test_action_has_no_sticky_disk(self):
        # Owner decision 8: no Blacksmith sticky disk. setup-gradle replaced
        # the ~/.gradle disk and its busy-process unmount hook.
        action = yaml.safe_load(ACTION.read_text())
        for name in ("disk-key", "nix-disk", "docker-builder", "gradle-disk"):
            self.assertNotIn(name, action["inputs"])
        text = ACTION.read_text()
        for marker in ("useblacksmith/", "setup-docker-builder", "fuser", ".gradle"):
            self.assertNotIn(marker, text)

    def test_nix_builds_one_derivation_per_cpu_with_all_cores(self):
        for name, field in (
            ("Install Nix (Linux)", "nix_conf"),
            ("Install Nix (macOS)", "extra_nix_config"),
        ):
            lines = action_step(name)["with"][field].splitlines()
            with self.subTest(step=name):
                # Nix applies the last value, so any other line would win.
                for key, value in (("max-jobs", "auto"), ("cores", "0")):
                    self.assertEqual(
                        [
                            line
                            for line in lines
                            if re.match(rf"\s*(extra-)?{key}\s*=", line)
                        ],
                        [f"{key} = {value}"],
                    )

    def test_kache_is_off_unless_a_caller_runs_cargo(self):
        action = yaml.safe_load(ACTION.read_text())
        self.assertEqual(action["inputs"]["kache"]["default"], "false")
        calls = list(setup_nix_calls())
        self.assertGreater(len(calls), 40)
        for workflow, job_id, job, inputs in calls:
            with self.subTest(caller=f"{workflow} / {job_id}"):
                for name in ("disk-key", "nix-disk", "docker-builder", "gradle-disk"):
                    self.assertNotIn(name, inputs)
                environment = repr(job.get("environment") or "")
                if inputs.get("kache") == "true":
                    self.assertTrue(inputs.get("kache-scope"))
                    for name in S3_INPUTS:
                        self.assertIn(name, inputs)
                else:
                    self.assertEqual(
                        [name for name in inputs if name.startswith("kache")], []
                    )
                    if f"{workflow} / {job_id}" in TRUSTED_CACHIX_WRITERS:
                        self.assertEqual(job.get("environment"), "kache-s3-writer")
                        self.assertIn("cachix-auth-token", inputs)
                    else:
                        self.assertNotIn("kache-s3-writer", environment)

    def test_trusted_cachix_writers_run_only_on_trunk_pushes(self):
        for caller in sorted(TRUSTED_CACHIX_WRITERS):
            workflow = caller.split(" / ", 1)[0]
            document = yaml.safe_load(
                (ROOT / ".github/workflows" / workflow).read_text()
            )
            # YAML 1.1 reads the key `on` as true.
            triggers = document.get("on", document.get(True))
            with self.subTest(caller=caller):
                self.assertEqual(
                    triggers, {"push": {"branches": ["main", "self-hosted"]}}
                )

    def test_kache_callers_match_the_cargo_job_list(self):
        callers = {
            f"{workflow} / {job_id}": inputs.get("kache-scope")
            for workflow, job_id, _, inputs in setup_nix_calls()
            if inputs.get("kache") == "true"
        }
        self.assertEqual(callers, KACHE_CALLERS)

    def test_readme_lists_exactly_the_kache_callers(self):
        readme = (ROOT / ".github/actions/setup-nix/README.md").read_text()
        table = readme.split("## Caller scopes\n", 1)[1]
        rows = {
            match.group(1): match.group(2)
            for match in re.finditer(r"^\| `([^`]+ / [^`]+)` \| (.+?) \|", table, re.M)
        }
        callers = {
            f"{workflow} / {job_id}": inputs["kache-scope"]
            for workflow, job_id, _, inputs in setup_nix_calls()
            if inputs.get("kache") == "true"
        }
        self.assertEqual(sorted(rows), sorted(callers))
        for caller, scope in callers.items():
            # `cond && 'a' || 'b'` yields a or b, as dev/ci-suites reads it.
            literals = (
                re.findall(r"(?:&&|\|\|)\s*'([^']+)'", scope)
                if "${{" in scope
                else [scope]
            )
            self.assertTrue(literals)
            for literal in literals:
                with self.subTest(caller=caller, scope=literal):
                    self.assertIn(f"`{literal}`", rows[caller])

    def run_oom_report(self, log, status=0):
        script = action_step("Write kernel OOM report script")["run"]
        body = script.split("<< 'EOF'\n", 1)[1].split("\nEOF\n", 1)[0]
        with tempfile.TemporaryDirectory(prefix="xmtp oom report ") as temp:
            temp = Path(temp)
            (temp / "report.sh").write_text(body + "\n")
            (temp / "dmesg.txt").write_text(log)
            sudo = temp / "sudo"
            sudo.write_text(
                '#!/bin/sh\ntest "$*" = "-n dmesg" || exit 97\n'
                f'cat "{temp}/dmesg.txt"\nexit {status}\n'
            )
            sudo.chmod(0o755)
            return subprocess.run(
                [shutil.which("bash"), str(temp / "report.sh")],
                env={"PATH": f"{temp}:/usr/bin:/bin"},
                capture_output=True,
                text=True,
            )

    def test_oom_report_prints_kernel_records_and_never_fails(self):
        killed = (
            "[  812.1] cargo invoked oom-killer: gfp_mask=0x140cca\n"
            "[  812.2] Out of memory: Killed process 4242 (rustc) total-vm:9000kB\n"
        )
        result = self.run_oom_report("[    0.0] Linux version 6.8\n" + killed)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("::warning title=Kernel OOM::", result.stdout)
        self.assertIn("Killed process 4242 (rustc)", result.stdout)
        self.assertIn("invoked oom-killer", result.stdout)
        self.assertNotIn("Linux version", result.stdout)

        result = self.run_oom_report("[    0.0] Linux version 6.8\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("::warning", result.stdout)
        self.assertIn("No kernel out-of-memory record", result.stdout)

        result = self.run_oom_report("dmesg: read kernel buffer failed\n", status=1)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("::warning title=Kernel log unavailable::", result.stdout)

    def test_oom_report_runs_after_the_job_on_linux(self):
        steps = yaml.safe_load(ACTION.read_text())["runs"]["steps"]
        names = [step.get("name") for step in steps]
        register = action_step("Register kernel OOM report")
        self.assertTrue(register["uses"].startswith("webiny/action-post-run@"))
        self.assertIn("kernel-oom-report.sh", register["with"]["run"])
        self.assertIn("runner.os == 'Linux'", register["if"])
        # Posts run in reverse order: the first registered post runs last.
        self.assertEqual(names.index("Register kernel OOM report"), 1)


# GitHub expression evaluation, enough for the cache-mode wiring: literals,
# context properties, ==, !=, !, &&, || and parentheses. && and || return an
# operand, as GitHub does. Anything else raises, so a new construct in a
# checked field fails loudly instead of being guessed.
EXPRESSION = re.compile(
    r"\s*(?:(?P<string>'(?:[^']|'')*')|(?P<op>==|!=|&&|\|\||!|\(|\))"
    r"|(?P<word>[A-Za-z_][\w-]*(?:\.[A-Za-z_][\w-]*)*))"
)


def truthy(value):
    return value not in (None, False, 0, "")


def number(value):
    if value is None:
        return 0
    if isinstance(value, bool):
        return int(value)
    try:
        return float(value)
    except (TypeError, ValueError):
        return float("nan")


def equal(left, right):
    if isinstance(left, str) and isinstance(right, str):
        return left.lower() == right.lower()
    if type(left) is type(right):
        return left == right
    return number(left) == number(right)


class Context:
    def __init__(self, github, inputs, secrets):
        self.github = github
        # Input name -> function that returns the value, so a caller's
        # `with:` value is evaluated only when the callee reads it.
        self.inputs = inputs
        # None: every repository secret is available.
        self.secrets = secrets

    def lookup(self, path):
        head, _, name = path.partition(".")
        if head == "github":
            return self.github[name]
        if head == "inputs":
            return self.inputs[name]() if name in self.inputs else None
        if head == "secrets":
            if self.secrets is None:
                return f"<{name}>"
            return self.secrets.get(name, "")
        if head == "vars":
            return ""
        if head == "runner" and name == "os":
            return "Linux"
        raise ValueError(f"unsupported context: {path}")

    def evaluate(self, text):
        tokens = []
        position = 0
        while position < len(text.rstrip()):
            match = EXPRESSION.match(text, position)
            if not match:
                raise ValueError(f"cannot parse {text!r} at {position}")
            tokens.append(match)
            position = match.end()
        index = 0

        def peek():
            return tokens[index].group("op") if index < len(tokens) else None

        def primary():
            nonlocal index
            token = tokens[index]
            index += 1
            if token.group("op") == "(":
                value = disjunction()
                assert peek() == ")", text
                index += 1
                return value
            if token.group("op") == "!":
                return not truthy(primary())
            if token.group("string") is not None:
                return token.group("string")[1:-1].replace("''", "'")
            word = token.group("word")
            if word in ("true", "false"):
                return word == "true"
            if word == "null":
                return None
            if word is None:
                raise ValueError(f"unexpected {token.group(0)!r} in {text!r}")
            return self.lookup(word)

        def comparison():
            nonlocal index
            value = primary()
            while peek() in ("==", "!="):
                op = peek()
                index += 1
                result = equal(value, primary())
                value = result if op == "==" else not result
            return value

        def conjunction():
            nonlocal index
            value = comparison()
            while peek() == "&&":
                index += 1
                right = comparison()
                value = right if truthy(value) else value
            return value

        def disjunction():
            nonlocal index
            value = conjunction()
            while peek() == "||":
                index += 1
                right = conjunction()
                value = value if truthy(value) else right
            return value

        value = disjunction()
        assert index == len(tokens), text
        return value

    def value(self, raw):
        """Evaluate a workflow value: a literal, or ${{ }} templates in a string."""
        if not isinstance(raw, str) or "${{" not in raw:
            return raw
        whole = re.fullmatch(r"\s*\$\{\{(.*?)\}\}\s*", raw, re.S)
        if whole:
            return self.evaluate(whole.group(1))

        def text(match):
            result = self.evaluate(match.group(1))
            return (
                str(result).lower() if isinstance(result, bool) else str(result or "")
            )

        return re.sub(r"\$\{\{(.*?)\}\}", text, raw)


def workflow(name):
    document = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
    # YAML 1.1 reads the key `on` as true.
    triggers = document.get("on", document.get(True))
    if isinstance(triggers, str):
        triggers = {triggers: None}
    elif isinstance(triggers, list):
        triggers = dict.fromkeys(triggers)
    return document, triggers


TRUNK = ("main", "self-hosted")


def entry_contexts(triggers):
    """Yield (label, github context, dispatch inputs) for each way a run starts."""

    def branch(event, name):
        return {
            "event_name": event,
            "ref": f"refs/heads/{name}",
            "ref_type": "branch",
            "ref_protected": name in TRUNK,
        }

    for event, config in triggers.items():
        if event == "push":
            for name in (config or {}).get("branches") or TRUNK:
                yield f"push to {name}", branch("push", name), {}
                if name in TRUNK:
                    # A trunk branch that lost its protection is not trusted.
                    context = branch("push", name)
                    context["ref_protected"] = False
                    context["unprotected trunk"] = True
                    yield f"push to unprotected {name}", context, {}
            for name in (config or {}).get("tags") or ():
                yield (
                    f"push of tag {name}",
                    {
                        "event_name": "push",
                        "ref": f"refs/tags/{name}",
                        "ref_type": "tag",
                        "ref_protected": False,
                    },
                    {},
                )
        elif event in ("pull_request", "pull_request_target"):
            context = branch(event, "feature")
            context["ref"] = "refs/pull/1/merge"
            yield event, context, {}
        elif event == "workflow_dispatch":
            inputs = {
                name: (lambda default=spec.get("default"): default)
                for name, spec in ((config or {}).get("inputs") or {}).items()
            }
            for name in ("main", "feature"):
                yield f"dispatch on {name}", branch(event, name), inputs
        elif event in ("schedule", "workflow_run"):
            yield event, branch(event, "main"), {}


def cache_calls():
    """Yield one record per setup-nix call, per entry workflow and event.

    Called workflows get the inputs and secrets their caller passes, so a mode
    that depends on a caller (deploy-docs.yml inside post-merge.yml) is evaluated as
    that caller runs it.
    """

    def walk(name, context, entry, label):
        document, _ = workflow(name)
        for job_id, job in (document.get("jobs") or {}).items():
            uses = job.get("uses", "")
            if uses.startswith("./.github/workflows/"):
                callee = uses.rsplit("/", 1)[1]
                _, callee_triggers = workflow(callee)
                declared = (callee_triggers.get("workflow_call") or {}).get(
                    "inputs"
                ) or {}
                given = job.get("with") or {}
                inputs = {
                    input_name: (
                        (lambda raw=given[input_name]: context.value(raw))
                        if input_name in given
                        else (lambda default=spec.get("default"): default)
                    )
                    for input_name, spec in declared.items()
                }
                passed = job.get("secrets")
                if passed == "inherit":
                    secrets = context.secrets
                elif isinstance(passed, dict):
                    secrets = {key: context.value(raw) for key, raw in passed.items()}
                else:
                    secrets = {}
                yield from walk(
                    callee, Context(context.github, inputs, secrets), entry, label
                )
                continue
            environment = job.get("environment") or ""
            if isinstance(environment, dict):
                environment = environment.get("name") or ""
            steps = job.get("steps") or []
            nix = [
                step.get("with") or {}
                for step in steps
                if step.get("uses") == SETUP_NIX
            ]
            js = [
                step.get("with") or {}
                for step in steps
                if step.get("uses") == "./.github/actions/setup-js"
            ]
            release_tools = any(
                step.get("uses") == "./.github/actions/setup-release-tools"
                for step in steps
            )
            gradle = [
                step.get("with") or {}
                for step in steps
                if step.get("uses", "").startswith(SETUP_GRADLE)
            ]
            # The Gradle cache rule is checked through the job's cache mode.
            assert nix or not gradle, (
                f"{name} / {job_id}: setup-gradle without setup-nix"
            )
            for inputs in nix:
                yield {
                    "where": f"{entry} ({label}): {name} / {job_id}",
                    "entry": entry,
                    "github": context.github,
                    "environment": context.value(environment),
                    "mode": context.value(inputs.get("cache-mode")) or "read",
                    "trusted-token": context.value(inputs.get("cachix-auth-token"))
                    or "",
                    "pr-token": context.value(inputs.get("cachix-pr-auth-token")) or "",
                    "js-modes": [
                        context.value(i.get("cache-mode")) or "read" for i in js
                    ]
                    + (["read"] if release_tools else []),
                    "gradle": [
                        {key: context.value(raw) for key, raw in inputs.items()}
                        for inputs in gradle
                    ],
                }

    for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
        _, triggers = workflow(path.name)
        for label, github, inputs in entry_contexts(triggers):
            yield from walk(path.name, Context(github, inputs, None), path.name, label)


def protected_trunk_push(github):
    return (
        github["event_name"] == "push"
        and github["ref_type"] == "branch"
        and github["ref_protected"]
        and github["ref"] in ("refs/heads/main", "refs/heads/self-hosted")
    )


# Entry workflows whose jobs may read xmtp-pr (mode ci). Every other entry
# (release, publish, deploy, post-merge, schedules, manual tools) reads only xmtp.
# A docs dispatch builds without deploying; post-merge.yml deploys.
CI_ENTRIES = {
    "ci.yml",
    "deploy-docs.yml",
    "lint-deny.yml",
    "test-backend.yml",
    "test-sdk.yml",
}


class CacheModeWiringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.calls = list(cache_calls())

    def test_wiring_is_evaluated_for_every_entry_and_event(self):
        entries = {call["entry"] for call in self.calls}
        self.assertTrue(
            {"ci.yml", "post-merge.yml", "release.yml", "push-backend.yml"} <= entries
        )
        self.assertGreater(len(self.calls), 150)

    def test_writer_environment_and_trusted_mode_go_together_on_trunk_pushes(self):
        for call in self.calls:
            with self.subTest(call["where"]):
                self.assertIn(call["mode"], ("read", "ci", "trusted"))
                trusted = call["mode"] == "trusted"
                self.assertEqual(call["environment"] == "kache-s3-writer", trusted)
                # On a trunk push without branch protection, a job that is
                # trusted outright (post-merge.yml) fails in dev/kache-ci-policy
                # (CachePolicyTests); every other event must not be trusted.
                if trusted and not call["github"].get("unprotected trunk"):
                    self.assertTrue(protected_trunk_push(call["github"]))

    def test_each_job_receives_only_the_token_of_its_mode(self):
        for call in self.calls:
            with self.subTest(call["where"]):
                if call["pr-token"]:
                    self.assertEqual(call["mode"], "ci")
                if call["trusted-token"]:
                    self.assertEqual(call["mode"], "trusted")

    def test_release_deploy_and_warm_runs_never_read_xmtp_pr(self):
        for call in self.calls:
            if call["entry"] not in CI_ENTRIES:
                with self.subTest(call["where"]):
                    self.assertNotEqual(call["mode"], "ci")

    def test_gradle_cache_is_ci_only_and_written_only_on_trunk_pushes(self):
        # The Gradle cache is a CI-only store: only jobs in mode ci restore it,
        # so release, deploy, benchmark, and trusted jobs never read it, and
        # only protected trunk pushes save it; every other run reads only.
        gradle = [call for call in self.calls if call["gradle"]]
        self.assertTrue(gradle)
        for call in gradle:
            with self.subTest(call["where"]):
                self.assertEqual(call["mode"], "ci")
                self.assertEqual(len(call["gradle"]), 1)
                inputs = call["gradle"][0]
                for name in ("cache-disabled", "cache-write-only"):
                    self.assertNotIn(name, inputs)
                self.assertEqual(
                    truthy(inputs.get("cache-read-only")),
                    not protected_trunk_push(call["github"]),
                )

    def test_setup_js_uses_the_setup_nix_mode_of_its_job(self):
        for call in self.calls:
            with self.subTest(call["where"]):
                for mode in call["js-modes"]:
                    self.assertEqual(mode, call["mode"])


def run_steps(action, mode):
    """The steps of a composite action that run on Linux for a mode."""
    context = Context({}, {"cache-mode": lambda: mode}, {})
    steps = yaml.safe_load((ROOT / action).read_text())["runs"]["steps"]
    return [
        step
        for step in steps
        if "if" not in step
        or truthy(
            context.value(
                step["if"] if "${{" in step["if"] else "${{ " + step["if"] + " }}"
            )
        )
    ]


class CacheModeActionTests(unittest.TestCase):
    def test_setup_nix_mode_selects_the_cachix_caches_and_token(self):
        expected = {
            "read": [("xmtp", None, None)],
            "ci": [("xmtp-pr", "xmtp", "${{ inputs.cachix-pr-auth-token }}")],
            "trusted": [("xmtp", None, "${{ inputs.cachix-auth-token }}")],
        }
        for mode, caches in expected.items():
            with self.subTest(mode=mode):
                steps = run_steps(".github/actions/setup-nix/action.yml", mode)
                cachix = [
                    step["with"]
                    for step in steps
                    if step.get("uses", "").startswith("cachix/cachix-action@")
                ]
                self.assertEqual(
                    [
                        (
                            inputs["name"],
                            inputs.get("extraPullNames"),
                            inputs.get("authToken"),
                        )
                        for inputs in cachix
                    ],
                    caches,
                )
                # The policy runs unconditionally, before any cache is set up.
                names = [step.get("name") for step in steps]
                policy = action_step("Cache mode policy")
                self.assertNotIn("if", policy)
                self.assertIn("dev/kache-ci-policy", policy["run"])
                first_cache = min(
                    names.index(step.get("name"))
                    for step in steps
                    if step.get("uses", "").startswith(
                        ("cachix/", "nixbuild/", "kunobi-ninja/", "useblacksmith/")
                    )
                )
                self.assertLess(names.index("Cache mode policy"), first_cache)

    def test_setup_js_mode_selects_the_pnpm_store_action(self):
        expected = {
            "read": [],
            "ci": ["actions/cache/restore"],
            "trusted": ["actions/cache"],
        }
        for mode, actions in expected.items():
            with self.subTest(mode=mode):
                steps = run_steps(".github/actions/setup-js/action.yml", mode)
                self.assertEqual(
                    [
                        step["uses"].split("@")[0]
                        for step in steps
                        if step.get("uses", "").startswith("actions/cache")
                    ],
                    actions,
                )


if __name__ == "__main__":
    unittest.main()
