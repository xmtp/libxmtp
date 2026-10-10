#!/usr/bin/env python3
"""Execute the CI selector with app, SDK, and shared input paths."""

import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[2]


def flatten(items):
    for item in items:
        if isinstance(item, list):
            yield from flatten(item)
        else:
            yield item


def expression(source, values):
    source = source.strip().removeprefix("${{").removesuffix("}}")
    source = source.replace("||", " or ").replace("&&", " and ")
    source = re.sub(r"(?<![\'\"])\btrue\b(?![\'\"])", "True", source)
    source = re.sub(r"(?<![\'\"])\bfalse\b(?![\'\"])", "False", source)
    source = re.sub(r"[\w-]+(?:\.[\w-]+)+", lambda m: repr(values[m[0]]), source)
    return str(bool(eval(source, {"__builtins__": {}}, {}))).lower()


def select(paths, *, draft=False, event="pull_request", available=True):
    filters = yaml.safe_load((ROOT / ".github/ci-paths.yml").read_text())
    # Node's glob matcher supports the braces and ** patterns used by these filters.
    # Apply the action's some-with-excludes rule to each changed file.
    matcher = subprocess.run(
        [
            "node",
            "--input-type=module",
            "-e",
            """
import { matchesGlob } from 'node:path';
let data = ''; for await (const part of process.stdin) data += part;
const { paths, filters } = JSON.parse(data);
const result = Object.fromEntries(Object.entries(filters).map(([key, patterns]) => {
  const included = patterns.filter(p => !p.startsWith('!'));
  const excluded = patterns.filter(p => p.startsWith('!')).map(p => p.slice(1));
  return [key, paths.filter(path => included.some(p => matchesGlob(path, p)) &&
    !excluded.some(p => matchesGlob(path, p)))];
}));
process.stdout.write(JSON.stringify(result));
""",
        ],
        input=json.dumps(
            {
                "paths": paths,
                "filters": {k: list(flatten(v)) for k, v in filters.items()},
            }
        ),
        text=True,
        capture_output=True,
        check=True,
    )
    matches = json.loads(matcher.stdout)
    outputs = {"changes": json.dumps([k for k, v in matches.items() if v])}
    outputs.update({k + "_files": json.dumps(v) for k, v in matches.items()})
    loader = importlib.machinery.SourceFileLoader(
        "ci_select", str(ROOT / "dev/ci-select")
    )
    spec = importlib.util.spec_from_loader(loader.name, loader)
    selector = importlib.util.module_from_spec(spec)
    loader.exec_module(selector)
    payload = {
        "repository": {"full_name": "xmtp/libxmtp"},
        "pull_request": {
            "draft": draft,
            "head": {"repo": {"full_name": "xmtp/libxmtp"}},
        },
    }
    return selector.select_checks(
        outputs, event, payload, "success" if available else "failure", len(paths)
    )["plan"]


class SelectionTest(unittest.TestCase):
    def assert_app(self, result):
        self.assertIn("lint-example-android", result["lint_jobs"])
        self.assertIn("test-example-android", result["test_jobs"])

    def test_app_inputs_select_app_without_sdk_platform_or_bindings(self):
        for path in (
            "apps/example-android/app/src/main/java/Screen.kt",
            "apps/example-android/shared/src/commonMain/kotlin/Screen.kt",
            "apps/example-android/app/src/main/AndroidManifest.xml",
            "apps/example-android/settings.gradle",
            "apps/example-android/app/gradle.lockfile",
            "apps/example-android/fixtures/metadata_backend.py",
            "apps/example-android/performance/run.py",
            "apps/example-android/gradlew",
        ):
            with self.subTest(path=path):
                result = select([path])
                self.assert_app(result)
                for check in (
                    "test_android",
                    "test_android_consumers",
                    "test_android_platform",
                    "test_sdk_staging",
                    "check_bindings_android",
                    "test_bindings",
                ):
                    self.assertFalse(result["checks"][check], check)

    def test_sdk_native_and_shared_changes_keep_compatibility(self):
        for path in (
            "sdks/android/library/src/main/java/Client.kt",
            "sdks/android/gradle/toolchain.properties",
            "sdks/android/gradle/wrapper/gradle-wrapper.properties",
            "apps/xmtp_sdk_bindgen/runtime/kotlin/Client.kt",
            "crates/xmtp_sdk/src/client.rs",
            "flake.lock",
        ):
            with self.subTest(path=path):
                result = select([path])
                self.assert_app(result)
                for check in (
                    "test_android",
                    "test_android_consumers",
                    "test_android_platform",
                    "test_sdk_staging",
                    "check_bindings_android",
                ):
                    self.assertTrue(result["checks"][check], check)

    def test_sdk_gradle_inputs_select_only_android_compatibility(self):
        for path in (
            "sdks/android/gradle/toolchain.properties",
            "sdks/android/gradle/wrapper/gradle-wrapper.properties",
            "sdks/android/gradle/wrapper/gradle-wrapper.jar",
            "sdks/android/gradle/verification-metadata.xml",
            "sdks/android/gradle/android-ndk.gradle",
            "sdks/android/gradle.properties",
            "sdks/android/gradlew",
            "sdks/android/gradlew.bat",
        ):
            with self.subTest(path=path):
                result = select([path])
                self.assert_app(result)
                for check in (
                    "lint_android",
                    "test_android",
                    "test_android_consumers",
                    "test_android_platform",
                    "test_sdk_staging",
                    "check_bindings_android",
                ):
                    self.assertTrue(result["checks"][check], check)
                for check in (
                    "check_rust",
                    "test_workspace",
                    "test_node",
                    "test_browser",
                    "test_ios",
                    "check_bindings_ios",
                ):
                    self.assertFalse(result["checks"][check], check)

    def test_mixed_changes_keep_sdk_platform_checks(self):
        result = select(
            [
                "apps/example-android/app/build.gradle",
                "sdks/android/library/src/main/java/Client.kt",
            ]
        )
        self.assert_app(result)
        self.assertTrue(result["checks"]["test_android_platform"])

    def test_documentation_does_not_select_runtime_checks(self):
        for path in (
            "apps/example-android/README.md",
            "apps/example-android/AGENTS.md",
            "docs/guide.md",
        ):
            with self.subTest(path=path):
                result = select([path])
                self.assertFalse(result["checks"]["test_example_android"])
                self.assertFalse(result["checks"]["test_android_platform"])
                self.assertTrue(result["checks"]["docs_quality"])

    def test_unknown_or_failed_detection_selects_all_gates(self):
        for result in (select(["new-unknown-file"]), select([], available=False)):
            self.assert_app(result)
            self.assertTrue(result["checks"]["test_android_platform"])

    def test_draft_keeps_source_policy_and_push_keeps_app_gate(self):
        result = select(
            ["apps/example-android/app/src/main/java/Screen.kt"], draft=True
        )
        self.assertFalse(result["checks"]["test_example_android"])
        self.assertTrue(result["checks"]["lint_config"])
        self.assert_app(
            select(["apps/example-android/app/src/main/java/Screen.kt"], event="push")
        )

    def test_app_jobs_require_every_child_and_gate_ci(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/test-example-android.yml").read_text()
        )
        gate = workflow["jobs"]["results"]
        required = ("unit-tests", "integration-tests", "messenger-performance")
        self.assertEqual(set(gate["needs"]), set(required))
        predicate = gate["steps"][0]["env"]["PASSED"]
        values = {f"needs.{job}.result": "success" for job in required}
        self.assertEqual(expression(predicate, values), "true")
        for job in required:
            for status in ("skipped", "failure", "cancelled", ""):
                with self.subTest(job=job, status=status):
                    self.assertEqual(
                        expression(
                            predicate, dict(values, **{f"needs.{job}.result": status})
                        ),
                        "false",
                    )
        ci = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        for name, job in ci["jobs"].items():
            if name in ("lint", "test"):
                self.assertIn(
                    "lint-example-android"
                    if name == "lint"
                    else "test-example-android",
                    job["needs"],
                )


class CommandPathTest(unittest.TestCase):
    def test_moved_recipes_run_in_app_root_and_retain_sdk_staging(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            files = subprocess.check_output(
                ["git", "ls-files", "*.just", "justfile"], cwd=ROOT, text=True
            ).splitlines()
            for name in files:
                source = ROOT / name
                if source.is_file():
                    target = root / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)

            def executable(name, body):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body)
                path.chmod(0o755)

            checker = root / "sdks/android/dev/check-native-packages.py"
            checker.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "sdks/android/dev/check-native-packages.py", checker)
            exporter = root / "apps/example-android/fixtures/export_screenshots.py"
            exporter.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(
                ROOT / "apps/example-android/fixtures/export_screenshots.py", exporter
            )
            # These archives prove path selection. Real packages use the NDK reader.
            for name in (
                "sdks/android/library/build/outputs/aar/library-debug.aar",
                "sdks/android/library/build/outputs/aar/library-release.aar",
                "apps/example-android/app/build/outputs/apk/debug/example-debug.apk",
                "apps/example-android/app/build/outputs/apk/release/example-release-unsigned.apk",
            ):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr("jni/arm64-v8a/libxmtp_sdk.so", b"path-fixture")
            executable(
                "ndk/toolchains/llvm/prebuilt/fixture/bin/llvm-readelf",
                "printf ' [1] .dynsym\\n [2] .dynstr\\n'\n",
            )

            executable("dev/nix-shell", 'exec bash -euc "$1"\n')
            executable("dev/worktree-env", "true\n")
            executable(
                "dev/docker/load-env",
                "export XMTP_BACKEND_PORT=15150\nexport XMTP_METADATA_BACKEND_PORT=15151\nexport XMTP_METADATA_BACKEND_URL=http://127.0.0.1:15151\nexport XMTP_S3_PORT=15152\nexport XMTP_ANDROID_S3_GATE_PORT=15153\nexport XMTP_TOXIPROXY_PORT=15154\nexport XMTP_TOXIPROXY_API_PORT=15155\nexport XMTP_BACKEND_TOXIC_URL=http://127.0.0.1:15154\nexport XMTP_TOXIPROXY_API=http://127.0.0.1:15155\nexport XMTP_ANDROID_UNSUPPORTED_BACKEND_PORT=15156\nexport XMTP_ANDROID_UNSUPPORTED_BACKEND_URL=http://127.0.0.1:15156\n",
            )
            executable(
                "sdks/android/dev/bindings", 'printf "bindings\\n" >> "$STAGE_LOG"\n'
            )
            executable("bin/nix", 'printf "%s\\n" "$*" >> "$STAGE_LOG"\n')
            executable(
                "bin/run-test-emulator",
                'export ANDROID_SERIAL=fixture-device\ntest "$1" = --\nshift\nexec "$@"\n',
            )
            executable(
                "bin/adb",
                'if [[ "$3" == exec-out ]]; then\n'
                ' test "$4" = run-as\n'
                ' test "$5" = org.xmtp.android.example\n'
                ' printf "%s\\n" "$*" >> "$SCREENSHOT_LOG"\n'
                ' case "$6" in\n'
                '  ls) if [[ "$7" == -1 ]]; then\n'
                "       printf 'setup.png\\n'\n"
                "      else printf 'setup.png  conversations.png\\n'; fi ;;\n"
                "  head) printf '\\211PNG\\r\\n\\032\\nfixture-image' ;;\n"
                '  rm) test "${@: -1}" = files/xmtp-messenger-proof ;;\n'
                "  *) exit 1 ;;\n"
                " esac\n"
                "fi\n",
            )
            for name in ("attachment-io-proxy", "unsupported-backend"):
                executable("apps/example-android/dev/" + name, 'exec "$@"\n')
            fixture = root / "apps/example-android/fixtures/metadata_backend.py"
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(
                "import subprocess,sys\nassert sys.argv[1] == '--backend'\nassert '/apps/example-android/.build/metadata-backend/' in sys.argv[2]\nassert sys.argv[3] == '--'\nsys.exit(subprocess.call(sys.argv[4:]))\n"
            )
            executable(
                "apps/example-android/gradlew",
                "python3.11 - <<'PY'\nimport json,os\nwith open(os.environ['COMMAND_LOG'],'a') as stream:\n json.dump({'cwd':os.getcwd(),'host':os.environ.get('JAVA_TOOL_OPTIONS'),'port':os.environ.get('XMTP_BACKEND_PORT'),'args':os.environ['GRADLE_ARGS']},stream);stream.write('\\n')\nPY\n",
            )
            gradle = root / "apps/example-android/gradlew"
            gradle.write_text(
                gradle.read_text().replace(
                    "python3.11 -", 'export GRADLE_ARGS="$*"\npython3.11 -'
                )
            )
            environment = dict(
                os.environ,
                PATH=str(root / "bin") + os.pathsep + os.environ["PATH"],
                COMMAND_LOG=str(root / "commands.jsonl"),
                STAGE_LOG=str(root / "stages.txt"),
                SCREENSHOT_LOG=str(root / "screenshots.txt"),
                ANDROID_NDK_HOME=str(root / "ndk"),
            )
            for recipe in (
                "check",
                "test",
                "test-integration",
                "test-release-integration",
            ):
                result = subprocess.run(
                    [
                        "just",
                        "--justfile",
                        str(root / "justfile"),
                        "example-android",
                        recipe,
                    ],
                    cwd=root,
                    env=environment,
                    text=True,
                    capture_output=True,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                if recipe == "check":
                    self.assertIn("example-debug.apk", result.stdout)
                    self.assertIn("example-release-unsigned.apk", result.stdout)
            result = subprocess.run(
                [
                    "just",
                    "--justfile",
                    str(root / "justfile"),
                    "android",
                    "check-packages",
                ],
                cwd=root,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("library-debug.aar", result.stdout)
            self.assertNotIn("example-debug.apk", result.stdout)
            commands = [
                json.loads(line)
                for line in (root / "commands.jsonl").read_text().splitlines()
            ]
            self.assertEqual(len(commands), 4)
            self.assertTrue(
                all(c["cwd"] == str(root / "apps/example-android") for c in commands)
            )
            self.assertIn(":example:assembleRelease", commands[0]["args"])
            self.assertIn(":example-shared:assembleDebug", commands[0]["args"])
            self.assertIn("sdks/android/.build/test-host/lib", commands[1]["host"])
            self.assertIn(":example-shared:testDebugUnitTest", commands[1]["args"])
            self.assertIn(
                "metadataBackendUrl=http://127.0.0.1:15151", commands[2]["args"]
            )
            self.assertIn(
                "android.injected.androidTest.leaveApksInstalledAfterRun=true",
                commands[2]["args"],
            )
            self.assertIn(":example:connectedReleaseAndroidTest", commands[3]["args"])
            self.assertEqual(commands[2]["port"], "15150")
            self.assertTrue(
                (
                    root / "apps/example-android/app/build/screenshots/setup.png"
                ).is_file()
            )
            screenshot_calls = (root / "screenshots.txt").read_text()
            self.assertIn("run-as org.xmtp.android.example head", screenshot_calls)
            self.assertIn(
                "run-as org.xmtp.android.example rm -rf files/xmtp-messenger-proof",
                screenshot_calls,
            )
            self.assertNotIn("/sdcard/", screenshot_calls)
            self.assertFalse((root / "sdks/android/app").exists())
            self.assertEqual((root / "stages.txt").read_text().count("bindings\n"), 4)


if __name__ == "__main__":
    unittest.main()
