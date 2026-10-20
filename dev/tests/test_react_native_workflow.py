"""Exercise RN product handoffs through the production composite command."""

import fnmatch
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
ACTION = yaml.safe_load(
    (ROOT / ".github/actions/react-native-product/action.yml").read_text()
)
COMMAND = next(
    step["run"]
    for step in ACTION["runs"]["steps"]
    if step.get("name") == "Build and exchange the declared products"
)
XCODE_CHECK = next(
    step
    for step in ACTION["runs"]["steps"]
    if step.get("name") == "Verify the exact RN Apple toolchain"
)
WORKFLOWS = ROOT / ".github/workflows"
PACKAGE_ROOT = "/nix/store/" + "p" * 32 + "-package"


def workflow(name):
    return yaml.safe_load((WORKFLOWS / name).read_text())


def expression(raw, values):
    """Evaluate a GitHub expression made of ==, !=, &&, ||, and literals."""
    for name, value in values.items():
        raw = raw.replace(name, repr(value))
    raw = raw.replace("&&", " and ").replace("||", " or ").replace("!", " not ")
    raw = raw.replace(" not =", " !=")
    return eval(raw, {"__builtins__": {}}, {"true": True, "false": False})


def inner(raw):
    match = re.fullmatch(r"\$\{\{\s*(.*?)\s*\}\}", raw.strip(), re.DOTALL)
    assert match, raw
    return match[1]


def download_package(directory, archive):
    """Apply the downloader's single-artifact extraction contract to its inputs."""
    values = {
        "inputs.stage": "consumers",
        "inputs.artifact-prefix": "same-run",
        "inputs.exchange": "true",
    }
    artifacts = {"same-run-package-closure": archive}
    for step in ACTION["runs"]["steps"]:
        if not step.get("uses", "").startswith("actions/download-artifact@"):
            continue
        if not expression(step["if"], values):
            continue
        inputs = {
            key: re.sub(
                r"\$\{\{\s*(.*?)\s*\}\}",
                lambda match: expression(match[1], values),
                raw,
            )
            for key, raw in step["with"].items()
        }
        pattern = inputs.get("name", inputs.get("pattern", "*"))
        selected = {
            name: path
            for name, path in artifacts.items()
            if fnmatch.fnmatchcase(name, pattern)
        }
        assert selected, pattern
        for name, path in selected.items():
            # v8 extracts a single match directly into `path`, even with pattern.
            destination = directory / inputs["path"]
            if len(selected) > 1:
                destination /= name
            shutil.copytree(path, destination)


FAKE_NIX = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with open(os.environ["EVENTS"], "a") as output:
    output.write(json.dumps(["nix", args, os.environ["RN_PACKAGE_VERSION"]]) + "\\n")
if args[0] == "build":
    if "--json" in args:
        print(json.dumps([{"outputs": {"out": "/nix/store/consumer-bundles"}}]))
elif "sdkProvenance.source" in " ".join(args):
    print("source-identity")
else:
    print("/nix/store/developer")
"""

# The exchange script behind dev/nix-shell: records each call, writes the
# export manifest, and prints the declared roots of each stage.
FAKE_EXCHANGE = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
args = args[args.index("nix/react-native-exchange.py") + 1:]
with open(os.environ["EVENTS"], "a") as output:
    output.write(json.dumps(["exchange", args]) + "\\n")
kind = args[args.index("--kind") + 1]
stage = args[args.index("--stage") + 1]
name = "xmtp-sdk-react-native-" + {
    "test": "test-package",
    "test-android": "test-package-android",
}.get(kind, "package")
roots = {name: os.environ["PACKAGE"]} if stage == "package" else {"component": "/nix/store/c"}
if args[0] == "roots":
    print(json.dumps(roots))
    sys.exit(0)
directory = Path(args[args.index("--directory") + 1])
if args[0] == "export":
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "manifest.json").write_text(json.dumps({"roots": roots}))
else:
    assert (directory / "manifest.json").is_file(), directory
"""


class ReactNativeProductTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        binary = self.directory / "bin"
        binary.mkdir()
        (self.directory / "dev").mkdir()
        sdk = self.directory / "sdks/react-native"
        sdk.mkdir(parents=True)
        (sdk / "package.json").write_text('{"version":"8.0.0"}')
        self.store = self.directory / "package"
        self.store.mkdir()
        self.env = os.environ | {
            "PATH": str(binary) + os.pathsep + os.environ["PATH"],
            "RN_SYSTEM": "x86_64-linux",
            "RN_SOURCE_SHA": "a" * 40,
            "RN_PACKAGE_VERSION": "",
            "RN_PACKAGE_KIND": "release",
            "RN_CONSUMERS": "all",
            "RN_EXCHANGE": "true",
            "RN_SUBSTITUTE_ONLY": "false",
            "RN_ARTIFACT_PREFIX": "same-run",
            "RN_CACHE_MODE": "ci",
            "RUNNER_OS": "Linux",
            "GITHUB_ENV": str(self.directory / "output"),
            "RUNNER_TEMP": str(self.directory),
            "EVENTS": str(self.directory / "events"),
            "PACKAGE": str(self.store),
        }
        (binary / "git").write_text(
            '#!/usr/bin/env bash\nprintf "%s\\n" "$RN_SOURCE_SHA"\n'
        )
        (binary / "nix").write_text(FAKE_NIX)
        (self.directory / "dev/nix-shell").write_text(FAKE_EXCHANGE)
        for path in (*binary.iterdir(), self.directory / "dev/nix-shell"):
            path.chmod(0o755)

    def run_stage(self, stage, **env):
        result = subprocess.run(
            ["/bin/bash", "-euc", COMMAND],
            cwd=self.directory,
            env=self.env | env | {"RN_STAGE": stage},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def events(self):
        return [
            json.loads(line)
            for line in Path(self.env["EVENTS"]).read_text().splitlines()
        ]

    def exchanges(self):
        return [
            args
            for command, args, *_ in self.events()
            if command == "exchange" and args[0] != "roots"
        ]

    def builds(self):
        return [
            event[1]
            for event in self.events()
            if event[0] == "nix" and event[1][0] == "build"
        ]

    def output(self):
        return Path(self.env["GITHUB_ENV"]).read_text()

    def test_fork_and_release_handoffs_preserve_package_identity(self):
        for version in ("", "8.0.1-pre.20260102030405.nightly.abcdef0"):
            with self.subTest(version=version):
                for path in ("events", "output", "target"):
                    shutil.rmtree(self.directory / path, ignore_errors=True)
                    (self.directory / path).unlink(missing_ok=True)
                env = {
                    "RN_PACKAGE_VERSION": version,
                    "RN_CACHE_MODE": "read" if version else "ci",
                }
                for system in ("x86_64-linux", "aarch64-darwin"):
                    path = (
                        self.directory
                        / f"target/rn-exchange/same-run-components-{system}"
                    )
                    path.mkdir(parents=True)
                    (path / "manifest.json").write_text("{}")
                self.run_stage("package", **env)
                archive = self.directory / "uploaded-package"
                shutil.rmtree(archive, ignore_errors=True)
                shutil.copytree(self.directory / "target/rn-exchange/package", archive)
                shutil.rmtree(self.directory / "target/rn-exchange")
                download_package(self.directory, archive)
                self.run_stage("consumers", **env)
                exchanges = self.exchanges()
                self.assertEqual(
                    [args[0] for args in exchanges],
                    ["import", "import", "export", "import"],
                )
                self.assertEqual(
                    [args[args.index("--system") + 1] for args in exchanges[:2]],
                    ["x86_64-linux", "aarch64-darwin"],
                )
                for args in exchanges[2:]:
                    self.assertNotIn("", args)
                    self.assertEqual(args[args.index("--stage") + 1], "package")
                    self.assertEqual(
                        args[args.index("--version") + 1]
                        if "--version" in args
                        else "",
                        version,
                    )
                builds = self.builds()
                self.assertEqual(len(builds), 2)
                self.assertTrue(
                    all(("--impure" in args) == bool(version) for args in builds)
                )
                if not version:
                    self.assertIn(".#react-native-consumer-android", builds[1])
                output = self.output()
                self.assertIn(f"XMTP_RN_PACKAGE_DIR={self.store}\n", output)
                self.assertIn(
                    f"XMTP_RN_EXPECTED_VERSION={version or '8.0.0'}\n", output
                )
                self.assertIn(
                    "XMTP_RN_CONSUMER_BUNDLES=/nix/store/consumer-bundles\n", output
                )

    def test_token_runs_substitute_the_test_package_instead_of_exchanging(self):
        for kind, consumers, stages, package, app in (
            (
                "test",
                "all",
                "testStages",
                "xmtp-sdk-react-native-test-package",
                ".#react-native-test-consumer-android",
            ),
            # The Android smoke: the Android-only package and the bare app.
            (
                "test-android",
                "bare",
                "androidTestStages",
                "xmtp-sdk-react-native-test-package-android",
                ".#react-native-test-consumer-android-bare",
            ),
        ):
            with self.subTest(kind=kind):
                for path in ("events", "output"):
                    (self.directory / path).unlink(missing_ok=True)
                env = {"RN_EXCHANGE": "false", "RN_PACKAGE_KIND": kind}
                self.run_stage("components", **env)
                self.run_stage("package", **env)
                self.run_stage("consumers", RN_CONSUMERS=consumers, **env)
                self.assertEqual(self.exchanges(), [])
                self.assertFalse((self.directory / "target/rn-exchange").exists())
                components, built, package_fetch, apps = self.builds()
                prefix = f".#legacyPackages.x86_64-linux.reactNative.{stages}"
                self.assertIn(f"{prefix}.components", components)
                self.assertIn(f"{prefix}.package", built)
                self.assertIn(f".#packages.x86_64-linux.{package}", package_fetch)
                self.assertEqual(apps[-1], app)
                output = self.output()
                self.assertIn(f"XMTP_RN_PACKAGE_DIR={self.store}\n", output)
                self.assertIn("XMTP_RN_EXPECTED_VERSION=8.0.0-partial\n", output)
                # The pushes read the declared roots, not an export manifest.
                roots = re.findall(r"^XMTP_RN_ROOTS=(.*)$", output, re.MULTILINE)
                self.assertEqual(
                    [json.loads(Path(path).read_text()) for path in roots],
                    [{"component": "/nix/store/c"}, {package: str(self.store)}],
                )

    def test_android_only_package_imports_only_linux_components(self):
        path = self.directory / "target/rn-exchange/same-run-components-x86_64-linux"
        path.mkdir(parents=True)
        (path / "manifest.json").write_text("{}")
        self.run_stage("package", RN_PACKAGE_KIND="test-android")
        imports = [args for args in self.exchanges() if args[0] == "import"]
        self.assertEqual(
            [args[args.index("--system") + 1] for args in imports], ["x86_64-linux"]
        )

    def test_exchange_runs_take_the_roots_from_the_checked_manifest(self):
        self.run_stage("components")
        self.assertEqual([args[0] for args in self.exchanges()], ["export"])
        self.assertNotIn(
            "roots",
            [args[0] for command, args, *_ in self.events() if command == "exchange"],
        )
        (path,) = re.findall(r"^XMTP_RN_ROOTS=(.*)$", self.output(), re.MULTILINE)
        self.assertEqual(
            json.loads(Path(path).read_text()), {"component": "/nix/store/c"}
        )

    def test_substitute_only_allows_no_builder(self):
        self.run_stage("consumers", RN_EXCHANGE="false", RN_SUBSTITUTE_ONLY="true")
        builds = self.builds()
        self.assertEqual(len(builds), 2)
        for args in builds:
            self.assertEqual(args[args.index("--max-jobs") + 1], "0")
            self.assertIn("always-allow-substitutes", args)

    def test_release_and_bare_selections_that_do_not_exist_fail(self):
        for env in (
            {"RN_PACKAGE_KIND": "test", "RN_PACKAGE_VERSION": "8.0.1"},
            {"RN_PACKAGE_KIND": "release", "RN_CONSUMERS": "bare"},
            # The bare app comes only from the Android-only package, which has
            # no other app and no Darwin stage.
            {"RN_PACKAGE_KIND": "test", "RN_CONSUMERS": "bare"},
            {"RN_PACKAGE_KIND": "test-android", "RN_CONSUMERS": "all"},
            {"RN_PACKAGE_KIND": "test-android", "RN_SYSTEM": "aarch64-darwin"},
        ):
            with self.subTest(env=env):
                result = subprocess.run(
                    ["/bin/bash", "-euc", COMMAND],
                    cwd=self.directory,
                    env=self.env
                    | {"RN_EXCHANGE": "false", "RN_STAGE": "consumers"}
                    | env,
                    capture_output=True,
                    text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(
                    any(args[-1].startswith(".#react-native") for args in self.builds())
                    if Path(self.env["EVENTS"]).exists()
                    else False
                )

    def test_wrong_checkout_is_rejected_before_building(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            git = directory / "git"
            git.write_text('#!/usr/bin/env bash\nprintf "%040d\\n" 0\n')
            git.chmod(0o755)
            nix = directory / "nix"
            nix.write_text('#!/usr/bin/env bash\ntouch "$BUILD_REACHED"\n')
            nix.chmod(0o755)
            (directory / "dev").mkdir()
            wrapper = directory / "dev/nix-shell"
            wrapper.write_text("#!/usr/bin/env bash\nexit 0\n")
            wrapper.chmod(0o755)
            result = subprocess.run(
                ["/bin/bash", "-euc", COMMAND],
                cwd=directory,
                env=self.env
                | {
                    "PATH": str(directory) + os.pathsep + os.environ["PATH"],
                    "RN_CACHE_MODE": "read",
                    "RN_STAGE": "components",
                    "BUILD_REACHED": str(directory / "built"),
                },
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((directory / "built").exists())


def product_calls(document):
    for job_id, job in document["jobs"].items():
        for step in job.get("steps") or []:
            if step.get("uses") == "./.github/actions/react-native-product":
                yield job_id, job, step["with"]


class ReactNativeXcodeCheckTests(unittest.TestCase):
    """Run the RN Xcode pin check with stub xcode-select and xcodebuild."""

    PIN = {
        "PIN_DIRECTORY": "/Applications/Xcode_27.0.app/Contents/Developer",
        "PIN_VERSION": "27.0",
        "PIN_BUILD": "27A266a",
    }
    ANNOTATION = "::error title=RN Xcode pin not on this runner::"

    def check(self, directory, build, xcodebuild_fails=False):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary)
            (binary / "xcode-select").write_text(
                '#!/usr/bin/env bash\nprintf "%s\\n" "$STUB_DIRECTORY"\n'
            )
            (binary / "xcodebuild").write_text(
                "#!/usr/bin/env bash\n"
                '[[ -z "$STUB_FAIL" ]] || exit 1\n'
                'printf "Xcode 27.0\\nBuild version %s\\n" "$STUB_BUILD"\n'
            )
            for path in binary.iterdir():
                path.chmod(0o755)
            # The command line of a composite step with `shell: bash`.
            return subprocess.run(
                [
                    "bash",
                    "--noprofile",
                    "--norc",
                    "-eo",
                    "pipefail",
                    "-c",
                    XCODE_CHECK["run"],
                ],
                env=os.environ
                | self.PIN
                | {
                    "PATH": str(binary) + os.pathsep + os.environ["PATH"],
                    "STUB_DIRECTORY": directory,
                    "STUB_BUILD": build,
                    "STUB_FAIL": "1" if xcodebuild_fails else "",
                },
                capture_output=True,
                text=True,
            )

    def test_exact_pin_passes(self):
        self.assertEqual(XCODE_CHECK["shell"], "bash")
        result = self.check(self.PIN["PIN_DIRECTORY"], self.PIN["PIN_BUILD"])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn(self.ANNOTATION, result.stdout)

    def test_mismatch_fails_with_annotation(self):
        cases = {
            "directory": (
                "/Applications/Xcode_27.app/Contents/Developer",
                "27A266a",
                False,
            ),
            "build": (self.PIN["PIN_DIRECTORY"], "27A5218g", False),
            "xcodebuild failure": (self.PIN["PIN_DIRECTORY"], "27A266a", True),
        }
        for name, (directory, build, fails) in cases.items():
            with self.subTest(case=name):
                result = self.check(directory, build, fails)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertEqual(result.stdout.count(self.ANNOTATION), 1, result.stdout)


class ReactNativeWorkflowTests(unittest.TestCase):
    def test_trusted_and_release_runs_never_read_the_ci_cache(self):
        for name in ("post-merge.yml", "release-react-native-sdk.yml"):
            text = (WORKFLOWS / name).read_text()
            self.assertNotIn("xmtp-pr", text)
            self.assertNotIn("CACHIX_PR_AUTH_TOKEN", text)
            for job_id, job, inputs in product_calls(workflow(name)):
                with self.subTest(workflow=name, job=job_id):
                    self.assertIn(inputs["cache-mode"], ("read", "trusted"))
                    self.assertNotEqual(inputs.get("package-kind"), "test")

    def test_post_merge_smoke_reads_the_trusted_cache_without_a_token(self):
        smoke = workflow("post-merge.yml")["jobs"]["rn-smoke"]
        self.assertNotIn("environment", smoke)
        self.assertNotIn("secrets.", json.dumps(smoke))
        self.assertEqual(smoke["env"]["XMTP_RN_JOURNEYS"], "full")
        ((_, _, inputs),) = (
            call
            for call in product_calls(workflow("post-merge.yml"))
            if call[0] == "rn-smoke"
        )
        self.assertEqual(
            (inputs["cache-mode"], inputs["exchange"], inputs["substitute-only"]),
            ("read", "false", "true"),
        )
        alert = workflow("post-merge.yml")["jobs"]["rn-alert"]
        self.assertIn("rn-smoke", alert["needs"])
        # The smoke substitutes the backend and the test services that
        # `build` pushes. test_warm_sets.py checks when it runs.
        self.assertIn("build", smoke["needs"])

    def test_release_consumers_test_the_exchanged_full_package(self):
        document = workflow("release-react-native-sdk.yml")
        for job_id, job, inputs in product_calls(document):
            with self.subTest(job=job_id):
                self.assertEqual(inputs.get("exchange", "true"), "true")
                self.assertEqual(inputs.get("package-kind", "release"), "release")
        self.assertEqual(
            document["jobs"]["consumers"]["env"]["XMTP_RN_JOURNEYS"], "full"
        )
        verify = next(
            step["run"]
            for step in document["jobs"]["build"]["steps"]
            if "verify-package" in step.get("run", "")
        )
        self.assertNotIn("allow", verify)

    def test_pull_requests_get_the_test_package_and_forks_the_exchange(self):
        document = workflow("test-react-native.yml")
        kind = inner(document["env"]["RN_PACKAGE_KIND"])
        for event, package, scope, expected in (
            ("pull_request", "auto", "full", "test"),
            ("pull_request", "", "full", "test"),
            ("pull_request", "release", "full", "release"),
            ("push", "auto", "full", "release"),
            ("workflow_dispatch", "auto", "full", "release"),
            ("workflow_dispatch", "test", "full", "test"),
            # The Android smoke uses the Android-only test package on every
            # event; an explicit release request fails the scope check.
            ("pull_request", "auto", "android-smoke", "test-android"),
            ("workflow_dispatch", "auto", "android-smoke", "test-android"),
            ("workflow_dispatch", "", "android-smoke", "test-android"),
            ("workflow_dispatch", "test", "android-smoke", "test-android"),
            ("push", "auto", "android-smoke", "test-android"),
            ("workflow_dispatch", "release", "android-smoke", "release"),
        ):
            with self.subTest(event=event, package=package, scope=scope):
                self.assertEqual(
                    expression(
                        kind,
                        {
                            "inputs.package": package,
                            "inputs.scope": scope,
                            "github.event_name": event,
                        },
                    ),
                    expected,
                )
        # A bad scope fails before any build.
        steps = document["jobs"]["components"]["steps"]
        self.assertEqual(steps[0]["name"], "Check the scope")
        writer = inner(document["env"]["RN_CACHE_WRITER"])
        self.assertEqual(writer, "secrets.CACHIX_PR_AUTH_TOKEN != ''")
        for job_id, job, inputs in product_calls(document):
            with self.subTest(job=job_id):
                for token in (True, False):
                    self.assertEqual(
                        expression(
                            inner(inputs["exchange"]),
                            {"env.RN_CACHE_WRITER": "true" if token else "false"},
                        ),
                        not token,
                    )

    def test_scope_check_rejects_unknown_scope_and_package(self):
        # workflow_call passes scope and package as free strings, and
        # RN_PACKAGE_KIND maps any unknown package to release. The first step
        # of components must reject them before any build.
        document = workflow("test-react-native.yml")
        components = document["jobs"]["components"]
        step = components["steps"][0]
        self.assertEqual(step["name"], "Check the scope")
        self.assertEqual(step["env"]["PACKAGE"], "${{ inputs.package }}")
        # The check runs on every event: neither it nor its job is skipped.
        self.assertNotIn("if", components)
        self.assertNotIn("if", step)
        kind = inner(document["env"]["RN_PACKAGE_KIND"])

        def check(scope, package):
            values = {
                "inputs.package": package,
                "inputs.scope": scope,
                "github.event_name": "workflow_dispatch",
            }
            return subprocess.run(
                ["/bin/bash", "-euo", "pipefail", "-c", step["run"]],
                env={
                    "PATH": os.environ["PATH"],
                    "SCOPE": scope,
                    "PACKAGE": package,
                    "KIND": expression(kind, values),
                },
                capture_output=True,
                text=True,
            )

        for scope, package in (
            ("full", "auto"),
            ("full", "test"),
            ("full", "release"),
            # Empty means auto, as in RN_PACKAGE_KIND.
            ("full", ""),
            ("android-smoke", ""),
            ("android-smoke", "auto"),
            ("android-smoke", "test"),
        ):
            with self.subTest(scope=scope, package=package):
                result = check(scope, package)
                self.assertEqual(result.returncode, 0, result.stdout)
        for scope, package, error in (
            ("full", "relase", "::error::unknown package: relase"),
            ("full", "Test", "::error::unknown package: Test"),
            ("android", "auto", "::error::unknown scope: android"),
            ("android-smoke", "release", "::error::scope android-smoke needs"),
        ):
            with self.subTest(scope=scope, package=package):
                result = check(scope, package)
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn(error, result.stdout)

    def test_macos_rows_get_a_shorter_timeout_than_linux_rows(self):
        # The macOS rows get a timeout from their duration history, so a
        # stalled Nix download fails sooner. A wrong matrix key in the
        # expression would silently give them the Linux 90 minutes.
        jobs = workflow("test-react-native.yml")["jobs"]
        expected = {
            "components": {"Linux": 90, "macOS": 35},
            "consumers": {"Linux": 90, "macOS": 35},
        }
        for job_id, timeouts in expected.items():
            job = jobs[job_id]
            raw = inner(job["strategy"]["matrix"]["include"])
            raw = raw.removeprefix("fromJSON(").removesuffix(")")
            rows = json.loads(expression(raw, {"inputs.scope": "full"}))
            found = {}
            timeout = str(job["timeout-minutes"])
            for row in rows:
                values = {f"matrix.{key}": value for key, value in row.items()}
                os_name = "macOS" if "macos" in row["runner"] else "Linux"
                found[os_name] = (
                    expression(inner(timeout), values)
                    if "${{" in timeout
                    else int(timeout)
                )
            with self.subTest(job=job_id):
                self.assertEqual(found, timeouts)

    def test_android_smoke_runs_no_macos_job(self):
        # The Android smoke builds no iOS part, so no job of the chain runs on
        # a macOS label. The full scope keeps its macOS jobs.
        jobs = workflow("test-react-native.yml")["jobs"]

        def labels(scope):
            found = []
            for job in jobs.values():
                include = job.get("strategy", {}).get("matrix", {}).get("include")
                if isinstance(include, str):
                    raw = inner(include).removeprefix("fromJSON(").removesuffix(")")
                    rows = json.loads(expression(raw, {"inputs.scope": scope}))
                else:
                    rows = include or [{}]
                for row in rows:
                    runner = job["runs-on"]
                    if "${{" in runner:
                        runner = row[inner(runner).removeprefix("matrix.")]
                    found.append(runner)
            return found

        self.assertFalse(
            [label for label in labels("android-smoke") if "macos" in label]
        )
        self.assertTrue([label for label in labels("full") if "macos" in label])


if __name__ == "__main__":
    unittest.main()
