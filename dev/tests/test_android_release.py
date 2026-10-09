#!/usr/bin/env python3
"""Exercise release build wiring without native builds or publication."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/release-android.yml"


def step(name):
    text = WORKFLOW.read_text().split("      - name: " + name + "\n", 1)[1]
    return text.split("      - ", 1)[0]


class AndroidReleaseTest(unittest.TestCase):
    def test_integration_failure_upload_retains_app_and_library_evidence(self):
        workflow = (ROOT / ".github/workflows/test-android.yml").read_text()
        upload = workflow.split("    - name: Upload failed Android test reports\n", 1)[
            1
        ]
        upload = upload.split("  results:", 1)[0]
        self.assertIn("      if: failure()\n", upload)
        paths = upload.split("        path: |\n", 1)[1].split(
            "        include-hidden-files:", 1
        )[0]
        paths = [line.strip() for line in paths.splitlines() if line.strip()]
        required = {
            "sdks/android/example/build/outputs/androidTest-results/connected/**/*.xml",
            "sdks/android/example/build/outputs/androidTest-results/connected/**/*.txt",
            "sdks/android/example/build/reports/androidTests/**",
            "sdks/android/example/build/screenshots/**",
            "sdks/android/library/build/outputs/androidTest-results/connected/**/*.xml",
            "sdks/android/library/build/outputs/androidTest-results/connected/**/logcat-*.txt",
            "${{ runner.temp }}/android-emulator-startup/",
            "${{ runner.temp }}/messenger-emulator-startup/",
        }
        self.assertTrue(required.issubset(paths), paths)

    def test_sdk_integration_cannot_select_app_instrumentation(self):
        recipes = (ROOT / "sdks/android/android.just").read_text()
        body = recipes.split("test-integration: build\n", 1)[1].split("\n\n", 1)[0]
        command = body.split("&& ", 1)[1].strip()
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            launcher = root / "run-test-emulator"
            launcher.write_text(
                '#!/usr/bin/env bash\nset -eu\ntest "$1" = --\nshift\nexec "$@"\n'
            )
            launcher.chmod(0o755)
            adb = root / "adb"
            adb.write_text(
                '#!/usr/bin/env bash\nset -eu\ntest "$1" = -s\ntest "$3" = reverse\n'
            )
            adb.chmod(0o755)
            gradle = root / "gradlew"
            gradle.write_text(
                "#!/usr/bin/env bash\nset -eu\n"
                'printf "%s\\n" "$*" >&2\n'
                'test "$*" = "-p . :library:connectedCheck --continue"\n'
            )
            gradle.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"])
            env["ANDROID_SERIAL"] = "fixture-device"
            env["XMTP_S3_PORT"] = "9067"
            result = subprocess.run(
                ["bash", "-euc", command],
                cwd=root,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_live_environment_reaches_gradle(self):
        command = step("Build and test").split("        run: ", 1)[1].strip()
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "dev/docker").mkdir(parents=True)
            (root / "sdks/android").mkdir(parents=True)
            shutil.copy(ROOT / "dev/docker/load-env", root / "dev/docker/load-env")
            wrapper = root / "dev/nix-shell"
            wrapper.write_text('#!/usr/bin/env bash\nexec bash -euc "$1"\n')
            wrapper.chmod(0o755)
            generator = root / "dev/worktree-env"
            generator.write_text(
                "#!/usr/bin/env bash\ncat > dev/docker/.env <<EOF\n"
                "XMTP_BACKEND_URL=http://127.0.0.1:5150\n"
                "XMTP_BACKEND_TOXIC_URL=http://127.0.0.1:6110\n"
                "XMTP_TOXIPROXY_API=http://127.0.0.1:8574\nEOF\n"
            )
            generator.chmod(0o755)
            gradle = root / "sdks/android/gradlew"
            gradle.write_text(
                "#!/usr/bin/env bash\nset -eu\n"
                'test "$*" = ":library:build"\n'
                'test "$XMTP_BACKEND_URL" = "$EXPECTED_BACKEND"\n'
                'test "$XMTP_BACKEND_TOXIC_URL" = "http://127.0.0.1:6110"\n'
                'test "$XMTP_TOXIPROXY_API" = "http://127.0.0.1:8574"\n'
                'test "$JAVA_TOOL_OPTIONS" = "-Djna.library.path=/fixture/lib"\n'
                'exit "${BUILD_EXIT:-0}"\n'
            )
            gradle.chmod(0o755)
            env = {k: v for k, v in os.environ.items() if not k.startswith("XMTP_")}
            env.update(
                EXPECTED_BACKEND="http://127.0.0.1:5150",
                JAVA_TOOL_OPTIONS="-Djna.library.path=/fixture/lib",
            )
            result = subprocess.run(["bash", "-euc", command], cwd=root, env=env)
            self.assertEqual(result.returncode, 0)
            env.update(
                XMTP_BACKEND_URL="http://custom:5050",
                EXPECTED_BACKEND="http://custom:5050",
            )
            result = subprocess.run(["bash", "-euc", command], cwd=root, env=env)
            self.assertEqual(result.returncode, 0)
            env["BUILD_EXIT"] = "42"
            result = subprocess.run(["bash", "-euc", command], cwd=root, env=env)
            self.assertEqual(result.returncode, 42)

    def test_build_gates_publication_and_keeps_tests(self):
        text = WORKFLOW.read_text()
        self.assertIn('docker-builder: "true"', text)
        self.assertIn("just backend up", step("Start backend"))
        self.assertLess(
            text.index("- name: Start backend"), text.index("- name: Build and test")
        )
        self.assertLess(
            text.index("- name: Build and test"), text.index("- name: Publish\n")
        )
        build = step("Build and test")
        self.assertIn(
            'JAVA_TOOL_OPTIONS: "-Djna.library.path=${{ github.workspace }}/sdks/android/.build/test-host/lib"',
            build,
        )
        self.assertNotIn("publishToSonatype", build)
        self.assertNotIn("-x ", build)
        self.assertNotIn("continue-on-error", text)
        publish = step("Publish")
        self.assertNotIn("if:", publish)
        self.assertNotIn(":library:build", publish)
        self.assertIn(
            "publishToSonatype closeAndReleaseSonatypeStagingRepository", publish
        )

    def test_release_setup_actions_are_immutable(self):
        self.assertRegex(
            WORKFLOW.read_text(),
            r"uses: taiki-e/install-action@[0-9a-f]{40}(?:\s|$)",
        )
        self.assertRegex(
            (ROOT / ".github/actions/setup-nix/action.yml").read_text(),
            r"uses: useblacksmith/setup-docker-builder@[0-9a-f]{40}(?:\s|$)",
        )


if __name__ == "__main__":
    unittest.main()
