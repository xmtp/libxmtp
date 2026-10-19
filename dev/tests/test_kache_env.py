"""Check the Kache settings file, the CI wiring, and the CI write policy."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
KACHE_CI = ROOT / ".github/actions/setup-nix/kache-ci"
KACHE_TOML = ROOT / "nix/lib/kache.toml"


# Each key of nix/lib/kache.toml and the variable that Kache documents for it.
# Kache reports a host key as overridden only by that key's own variable, so a
# key it does not know (a typo) is never reported as overridden.
KACHE_KEY_VARIABLES = {
    "cache.local_store": ("KACHE_CACHE_DIR", "store"),
    "cache.local_max_size": ("KACHE_MAX_SIZE", "1GiB"),
    "cache.cache_executables": ("KACHE_CACHE_EXECUTABLES", "0"),
    "cache.cache_cc_links": ("KACHE_CACHE_CC_LINKS", "1"),
    "cache.auto_clean_unused_units_days": ("KACHE_AUTO_CLEAN_UNUSED_UNITS_DAYS", "9"),
    "cache.auto_recover_targets": ("KACHE_AUTO_RECOVER_TARGETS", "0"),
}
RUST_SHELLS = ("rust", "local", "android")


def toml_keys(table, prefix=""):
    for key, value in table.items():
        if isinstance(value, dict):
            yield from toml_keys(value, f"{prefix}{key}.")
        else:
            yield f"{prefix}{key}"


class KacheConfigTests(unittest.TestCase):
    """The real Kache reads nix/lib/kache.toml, in an empty home."""

    def setUp(self):
        self.kache = shutil.which("kache")
        self.assertIsNotNone(self.kache, "run inside dev/nix-shell")
        temp = tempfile.TemporaryDirectory(prefix="xmtp kache config ")
        self.addCleanup(temp.cleanup)
        self.home = Path(temp.name)

    def host_config(self, **variables):
        """Kache's `Host config` doctor check, with no other config file."""
        result = subprocess.run(
            [self.kache, "--json", "doctor"],
            cwd=self.home,
            env={
                "PATH": os.environ["PATH"],
                "HOME": str(self.home),
                "XDG_CONFIG_HOME": str(self.home / "config"),
                "XDG_CACHE_HOME": str(self.home / "cache"),
                "KACHE_HOST_CONFIG": str(KACHE_TOML),
                **variables,
            },
            capture_output=True,
            text=True,
        )
        checks = json.loads(result.stdout)["checks"]
        check = next(check for check in checks if check["label"] == "Host config")
        self.assertTrue(check["pass"], check["detail"])
        prefix = f"{KACHE_TOML}: "
        self.assertTrue(check["detail"].startswith(prefix), check["detail"])
        return sorted(check["detail"].removeprefix(prefix).split(", "))

    def test_kache_reads_every_key_of_the_file(self):
        keys = sorted(toml_keys(tomllib.loads(KACHE_TOML.read_text())))
        self.assertEqual(keys, sorted(KACHE_KEY_VARIABLES))
        self.assertEqual(self.host_config(), keys)

    def test_kache_knows_every_key_of_the_file(self):
        variables = {
            variable: str(self.home / value) if key == "cache.local_store" else value
            for key, (variable, value) in KACHE_KEY_VARIABLES.items()
        }
        self.assertEqual(
            self.host_config(**variables),
            sorted(
                f"{key} (overridden by {variable})"
                for key, (variable, _) in KACHE_KEY_VARIABLES.items()
            ),
        )

    def test_rust_shells_pin_the_settings_that_keep_builds_correct(self):
        # Variables win over every Kache file, so a personal config cannot
        # turn these off. dev/tests run inside the default (local) shell.
        shell = os.environ.get("XMTP_DEV_SHELL")
        if shell not in RUST_SHELLS:
            self.skipTest(f"not in a Rust dev shell ({shell})")
        self.assertEqual(os.environ.get("KACHE_KEY_ENV_VARS"), "CI,XMTP_TEST_LOGGING")
        self.assertEqual(os.environ.get("KACHE_BUILD_SCRIPT_CACHE"), "0")
        self.assertEqual(
            Path(os.environ["KACHE_HOST_CONFIG"]).read_text(), KACHE_TOML.read_text()
        )
        self.assertEqual(Path(os.environ["RUSTC_WRAPPER"]).name, "kache")


TRUNK_PUSH = {
    "GITHUB_EVENT_NAME": "push",
    "GITHUB_REF_TYPE": "branch",
    "GITHUB_REF": "refs/heads/main",
    "GITHUB_REF_PROTECTED": "true",
}


class CachePolicyTests(unittest.TestCase):
    """`kache-ci policy`: the cache mode, the event, and the tokens."""

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
                [shutil.which("bash"), str(KACHE_CI), "policy"],
                env=env,
                capture_output=True,
                text=True,
            )
            return result, output.read_text()

    def test_trusted_mode_runs_only_on_protected_trunk_pushes(self):
        for event, kind, ref, protected, allowed in (
            ("push", "branch", "refs/heads/self-hosted", "true", False),
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
            ("ci", "true", "true", False),
            ("ci", "true", "false", False),
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
            [shutil.which("bash"), str(KACHE_CI), "config"],
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
            ".github/workflows/ci.yml",
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
# not for Kache. CacheModeWiringTests checks when each one is trusted.
TRUSTED_CACHIX_WRITERS = {
    "post-merge.yml / build",
    "post-merge.yml / rn-components",
    "post-merge.yml / rn-package",
    "post-merge.yml / rn-consumers",
    "push-backend.yml / publish",
}

# Jobs that run cargo outside Nix, with their Kache scope. A job leaves this
# list only when it stops running cargo outside Nix, so dropping Kache from a
# cargo job fails here even when its README row goes too.
KACHE_CALLERS = {
    "check-rust.yml / native": "rust-clippy-native",
    "check-rust.yml / wasm": "rust-clippy-wasm",
    "check-sdk-unit.yml / unit": "sdk-tests-debug",
    "docs-rust-reference.yml / rust": "rust-reference",
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
    def test_darwin_installer_admits_only_the_pinned_rn_apple_host_inputs(self):
        nix = shutil.which("nix")
        developer = subprocess.check_output(
            [nix, "eval", "--raw", ".#lib.reactNativeXcode.developerDir"],
            cwd=ROOT,
            text=True,
        ).strip()
        expected = str(Path(developer).parent)
        with tempfile.TemporaryDirectory(prefix="xmtp nix host deps ") as folder:
            folder = Path(folder)
            system = folder / "system"
            system.mkdir()
            (system / "nix.conf").write_text(
                "allowed-impure-host-deps = /xmtp-test-inherited-host-dependency\n"
            )
            config = folder / "user.conf"
            config.write_text("")
            env = os.environ | {
                "NIX_CONF_DIR": str(system),
                "NIX_USER_CONF_FILES": str(config),
                "NIX_CONFIG": "",
            }

            def paths():
                return set(
                    subprocess.check_output(
                        [
                            nix,
                            "--extra-experimental-features",
                            "nix-command",
                            "config",
                            "show",
                            "allowed-impure-host-deps",
                        ],
                        env=env,
                        text=True,
                    ).split()
                )

            defaults = paths()
            self.assertIn("/xmtp-test-inherited-host-dependency", defaults)
            config.write_text(
                action_step("Install Nix (macOS)")["with"]["extra_nix_config"]
            )
            self.assertEqual(
                paths(),
                defaults
                | {
                    expected,
                    "/usr/bin/xcrun",
                    "/usr/bin/ditto",
                    "/usr/bin/env",
                    "/usr/bin/touch",
                    "/Library/Preferences/com.apple.dt.Xcode.plist",
                    "/usr/share/firmlinks",
                    "/Library/Developer/PrivateFrameworks/CoreSimulator.framework",
                    "/Library/Developer/PrivateFrameworks/DVTDownloads.framework",
                    "/Library/Developer/PrivateFrameworks/ROCKit.framework",
                    "/Library/Developer/PrivateFrameworks/CoreDevice.framework",
                    "/Library/Developer/PrivateFrameworks/CoreDeviceUtilities.framework",
                    "/Library/Apple/System/Library/PrivateFrameworks/MobileDevice.framework",
                    "/Library/Apple/System/Library/PrivateFrameworks/Mercury.framework",
                },
            )

    def test_action_installs_the_nix_kache_version(self):
        # Two Kache versions in one job share the local cache directory and
        # the S3 manifests, so the action installs the shell's version.
        package = (ROOT / "nix/lib/packages/kache.nix").read_text()
        version = re.search(r'^\s*version = "([^"]+)";', package, re.MULTILINE)
        self.assertEqual(
            action_step("Setup Kache")["with"]["version"], f"v{version[1]}"
        )

    def test_cache_mode_policy_runs_before_every_cache_step(self):
        steps = yaml.safe_load(ACTION.read_text())["runs"]["steps"]
        names = [step.get("name") for step in steps]
        policy = action_step("Cache mode policy")
        self.assertNotIn("if", policy)
        self.assertIn('"$GITHUB_ACTION_PATH/kache-ci" policy', policy["run"])
        caches = [
            index
            for index, step in enumerate(steps)
            if step.get("uses", "").startswith(
                ("cachix/", "nixbuild/", "kunobi-ninja/")
            )
        ]
        self.assertTrue(caches)
        self.assertLess(names.index("Cache mode policy"), min(caches))

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
                        self.assertIn("kache-s3-writer", environment)
                        self.assertIn("cachix-auth-token", inputs)
                    else:
                        self.assertNotIn("kache-s3-writer", environment)

    def run_kache_step(self, name):
        """Run a setup-nix step; return its $GITHUB_ENV and $GITHUB_PATH lines."""
        with tempfile.TemporaryDirectory(prefix="xmtp setup-nix ") as temp:
            temp = Path(temp)
            nix = temp / "nix"
            nix.write_text("#!/bin/sh\necho /nix/store/fake-kache\n")
            nix.chmod(0o755)
            for file in ("env", "path"):
                (temp / file).write_text("")
            result = subprocess.run(
                [
                    shutil.which("bash"),
                    "-euo",
                    "pipefail",
                    "-c",
                    action_step(name)["run"],
                ],
                env={
                    "PATH": f"{temp}:/usr/bin:/bin",
                    "GITHUB_ENV": str(temp / "env"),
                    "GITHUB_PATH": str(temp / "path"),
                    "KACHE_CACHE_DIR": str(temp / "store"),
                    "KACHE_RUNTIME_DIR": str(temp / "runtime"),
                },
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            return (
                (temp / "env").read_text().splitlines(),
                (temp / "path").read_text().splitlines(),
            )

    def test_kache_jobs_compile_without_incremental_state(self):
        step = action_step("Configure Kache compiler environment")
        self.assertEqual(step["if"], "${{ inputs.kache == 'true' }}")
        env, path = self.run_kache_step("Configure Kache compiler environment")
        for line in (
            "CARGO_INCREMENTAL=0",
            "KACHE_ADAPTIVE_INCREMENTAL=0",
            "KACHE_PRESERVE_INCREMENTAL=0",
        ):
            self.assertIn(line, env)
        # kache-action's KACHE_CONFIG holds the S3 remote; the dev shells set
        # KACHE_HOST_CONFIG. Neither may be replaced here.
        written = {line.split("=", 1)[0] for line in env}
        self.assertFalse(written & {"KACHE_CONFIG", "KACHE_HOST_CONFIG"})
        self.assertEqual(path, ["/nix/store/fake-kache/bin"])

    def test_other_jobs_turn_kache_off(self):
        step = action_step("Disable outer compiler cache")
        self.assertEqual(step["if"], "${{ inputs.kache != 'true' }}")
        env, _ = self.run_kache_step("Disable outer compiler cache")
        self.assertIn("KACHE_DISABLED=1", env)

    def test_kache_callers_match_the_cargo_job_list(self):
        callers = {
            f"{workflow} / {job_id}": inputs.get("kache-scope")
            for workflow, job_id, _, inputs in setup_nix_calls()
            if inputs.get("kache") == "true"
        }
        self.assertEqual(callers, KACHE_CALLERS)

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


# GitHub expressions in the checked fields use literals, context properties,
# ==, !=, !, &&, || and parentheses. Each becomes the Python operator with the
# same result (&& and || return an operand, as `and` and `or` do). A function
# call or an unknown context fails in `lookup`, so a new construct fails loudly
# instead of being guessed. Where Python groups differently (chained
# comparisons, a negated operand of a comparison), evaluation raises. Python
# compares strings case-sensitively; every compared value is lowercase.
TOKEN = re.compile(
    r"\s*(?:('(?:[^']|'')*')|(==|!=|&&|\|\||!|\(|\))|([A-Za-z_][\w.-]*))"
)
PYTHON = {
    "&&": "and",
    "||": "or",
    "!": "not",
    "true": "True",
    "false": "False",
    "null": "None",
}


def truthy(value):
    return value not in (None, False, 0, "")


def reject_python_precedence(ops, text):
    """Raise for the forms where Python and GitHub group differently.

    `ops` holds each token's operator, or None for an operand. Python chains
    `a == b == c` and binds `not` looser than `==`; GitHub reads
    `(a == b) == c` and `(!a) == b`. No workflow needs either form.
    """

    def operand_end(index):
        # The index after the operand that starts at `index`.
        if ops[index] == "!":
            return operand_end(index + 1)
        if ops[index] == "(":
            depth = 0
            for end in range(index, len(ops)):
                depth += {"(": 1, ")": -1}.get(ops[end], 0)
                if depth == 0:
                    return end + 1
        return index + 1

    compared = [False]
    for index, op in enumerate(ops):
        if op == "!":
            end = operand_end(index + 1)
            if end < len(ops) and ops[end] in ("==", "!="):
                raise ValueError(f"negated operand of a comparison: {text!r}")
        elif op == "(":
            compared.append(False)
        elif op == ")":
            compared.pop()
        elif op in ("&&", "||"):
            compared[-1] = False
        elif op in ("==", "!="):
            if compared[-1]:
                raise ValueError(f"chained comparison: {text!r}")
            compared[-1] = True


class Context:
    def __init__(self, github, inputs, secrets, matrix=None):
        self.github = github
        # One `strategy.matrix.include` entry, or None outside a matrix leg.
        self.matrix = matrix
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
        if head == "matrix" and self.matrix is not None and name in self.matrix:
            return self.matrix[name]
        if head == "runner" and name == "os":
            return "Linux"
        raise ValueError(f"unsupported context: {path}")

    def evaluate(self, text):
        source = []
        ops = []
        position = 0
        text = text.strip()
        while position < len(text):
            match = TOKEN.match(text, position)
            if not match:
                raise ValueError(f"cannot parse {text!r} at {position}")
            string, op, word = match.groups()
            if string is not None:
                source.append(repr(string[1:-1].replace("''", "'")))
                ops.append(None)
            elif op:
                source.append(PYTHON.get(op, op))
                ops.append(op)
            elif word in PYTHON:
                source.append(PYTHON[word])
                ops.append(None)
            else:
                source.append(f"_({word!r})")
                ops.append(None)
            position = match.end()
        reject_python_precedence(ops, text)
        return eval(" ".join(source), {"__builtins__": {}}, {"_": self.lookup})

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


TRUNK = ("main",)


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
            # Each `include` entry of a matrix is one leg. Another matrix form
            # leaves `matrix` unset, so a checked field that reads it fails.
            matrix = (job.get("strategy") or {}).get("matrix")
            legs = matrix.get("include") if isinstance(matrix, dict) else None
            for leg in legs or [None]:
                yield from job_calls(
                    Context(context.github, context.inputs, context.secrets, leg),
                    f"{entry} ({label}): {name} / {job_id}"
                    + ("" if leg is None else f" {leg}"),
                    entry,
                    environment,
                    nix,
                    js,
                    release_tools,
                    gradle,
                )

    def job_calls(context, where, entry, environment, nix, js, release_tools, gradle):
        for inputs in nix:
            yield {
                "where": where,
                "entry": entry,
                "github": context.github,
                "environment": context.value(environment),
                "mode": context.value(inputs.get("cache-mode")) or "read",
                "trusted-token": context.value(inputs.get("cachix-auth-token")) or "",
                "pr-token": context.value(inputs.get("cachix-pr-auth-token")) or "",
                "js-modes": [context.value(i.get("cache-mode")) or "read" for i in js]
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
        and github["ref"] == "refs/heads/main"
    )


# Entry workflows whose jobs may read xmtp-pr (mode ci). Every other entry
# (release, publish, deploy, post-merge, schedules, manual tools) reads only xmtp.
# A docs dispatch builds without deploying; post-merge.yml deploys.
CI_ENTRIES = {
    "ci.yml",
    "deploy-docs.yml",
    "lint-deny.yml",
    "test-backend.yml",
    "test-react-native.yml",
}


def gradle_cache_is_ci_only(call):
    """Only mode ci restores the Gradle cache; only protected trunk pushes save it."""
    return all(
        call["mode"] == "ci"
        and len(call["gradle"]) == 1
        and not {"cache-disabled", "cache-write-only"} & set(inputs)
        and truthy(inputs.get("cache-read-only"))
        != protected_trunk_push(call["github"])
        for inputs in call["gradle"]
    )


# Each rule holds for every setup-nix call of every entry workflow and event.
WIRING_RULES = (
    (
        "the mode is read, ci, or trusted",
        lambda c: c["mode"] in ("read", "ci", "trusted"),
    ),
    (
        "the writer environment goes with mode trusted",
        lambda c: (c["environment"] == "kache-s3-writer") == (c["mode"] == "trusted"),
    ),
    # On a trunk push without branch protection, a job that is trusted
    # outright (post-merge.yml) fails in `kache-ci policy` (CachePolicyTests).
    (
        "mode trusted runs only on a protected trunk push",
        lambda c: (
            c["mode"] != "trusted"
            or c["github"].get("unprotected trunk")
            or protected_trunk_push(c["github"])
        ),
    ),
    (
        "only mode ci gets the xmtp-pr token",
        lambda c: not c["pr-token"] or c["mode"] == "ci",
    ),
    (
        "only mode trusted gets the trusted Cachix token",
        lambda c: not c["trusted-token"] or c["mode"] == "trusted",
    ),
    (
        "release, deploy, and post-merge runs never read xmtp-pr",
        lambda c: c["entry"] in CI_ENTRIES or c["mode"] != "ci",
    ),
    (
        "setup-js uses the setup-nix mode of its job, or saves the pnpm store "
        "in a post-merge job with mode read, no writer environment, and no token",
        lambda c: all(
            mode == c["mode"]
            or (
                mode == "trusted"
                and c["entry"] == "post-merge.yml"
                and c["mode"] == "read"
                and not c["environment"]
                and not c["trusted-token"]
                and not c["pr-token"]
            )
            for mode in c["js-modes"]
        ),
    ),
    # Package install scripts run arbitrary code.
    (
        "a job with the trusted Cachix token installs no packages",
        lambda c: not c["trusted-token"] or not c["js-modes"],
    ),
    (
        "the Gradle cache is CI-only and written only on trunk pushes",
        gradle_cache_is_ci_only,
    ),
)


class CacheModeWiringTests(unittest.TestCase):
    def test_every_call_follows_the_wiring_rules(self):
        calls = list(cache_calls())
        # The walk reaches the trusted, release, and Gradle jobs.
        self.assertTrue(
            {"ci.yml", "post-merge.yml", "release.yml"} <= {c["entry"] for c in calls}
        )
        self.assertTrue(any(c["gradle"] for c in calls))
        broken = [
            f"{call['where']}: {rule}"
            for call in calls
            for rule, holds in WIRING_RULES
            if not holds(call)
        ]
        if broken:
            self.fail("\n".join(broken))


if __name__ == "__main__":
    unittest.main()
