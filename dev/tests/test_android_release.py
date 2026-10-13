#!/usr/bin/env python3
"""Exercise release build wiring without native builds or publication."""

import os
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/release-android.yml"


def step(name):
    text = WORKFLOW.read_text().split("      - name: " + name + "\n", 1)[1]
    return text.split("      - ", 1)[0]


class AndroidReleaseTest(unittest.TestCase):
    def test_universal_apk_rejects_each_missing_abi(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            readelf = root / "toolchains/llvm/prebuilt/fixture/bin/llvm-readelf"
            readelf.parent.mkdir(parents=True)
            readelf.write_text("#!/bin/sh\nprintf ' [1] .dynsym\\n [2] .dynstr\\n'\n")
            readelf.chmod(0o755)
            abis = {"arm64-v8a", "armeabi-v7a", "x86", "x86_64"}
            for missing in (None, *sorted(abis)):
                with self.subTest(missing=missing):
                    apk = root / "example.apk"
                    with zipfile.ZipFile(apk, "w") as archive:
                        for abi in abis - {missing}:
                            archive.writestr(f"lib/{abi}/libxmtp_sdk.so", b"fixture")
                    result = subprocess.run(
                        [
                            sys.executable,
                            str(ROOT / "sdks/android/dev/check-native-packages.py"),
                            "--require-all-abis",
                            str(apk),
                        ],
                        env=dict(os.environ, ANDROID_NDK_HOME=str(root)),
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(
                        result.returncode, 1 if missing else 0, result.stderr
                    )
                    if missing:
                        self.assertIn(f"missing SDK ABIs: {missing}", result.stderr)

    def test_apk_export_and_failure_gates(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            app = root / "apps/example-android"
            (app / "dev").mkdir(parents=True)
            shutil.copy(ROOT / "apps/example-android/dev/apk", app / "dev/apk")
            nix = root / "nix"
            nix.write_text(
                f"#!{sys.executable}\n"
                "import os, pathlib, sys\n"
                "assert sys.argv[1:4] == ['build', os.environ['EXPECTED_ROOT'] + '#example-android-apk', '--out-link']\n"
                "assert 'LD_LIBRARY_PATH' not in os.environ\n"
                "code = int(os.environ.get('NIX_EXIT', '0'))\n"
                "if code: sys.exit(code)\n"
                "out = pathlib.Path(sys.argv[4]); out.mkdir(parents=True, exist_ok=True)\n"
                "(out / 'version').write_text(os.environ.get('PACKAGE_VERSION', '8.0.0-dev.test') + '\\n')\n"
                "if not os.environ.get('MISSING_APK'): (out / 'xmtp-messenger.apk').write_bytes(b'built by Nix')\n"
            )
            nix.chmod(0o755)
            output = app / "build/apk/xmtp-messenger-8.0.0-dev.test.apk"
            output.parent.mkdir(parents=True)
            env = dict(
                os.environ,
                PATH=str(root) + os.pathsep + os.environ["PATH"],
                EXPECTED_ROOT=str(root),
                XMTP_EXAMPLE_VERSION="8.0.0-dev.test",
                LD_LIBRARY_PATH="/must-not-reach-nix",
            )
            for overrides, status in [
                ({}, 0),
                ({"NIX_EXIT": "42"}, 42),
                ({"PACKAGE_VERSION": "8.0.0"}, 1),
                ({"MISSING_APK": "1"}, 1),
            ]:
                with self.subTest(overrides=overrides):
                    shutil.rmtree(app / ".build", ignore_errors=True)
                    output.write_text("stale APK")
                    result = subprocess.run(
                        ["bash", str(app / "dev/apk")],
                        env=env | overrides,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(
                        result.returncode, status, result.stdout + result.stderr
                    )
                    if status:
                        self.assertFalse(output.exists(), result.stdout + result.stderr)
                    else:
                        self.assertEqual(output.read_bytes(), b"built by Nix")
                        self.assertIn(str(output), result.stdout)
            del env["XMTP_EXAMPLE_VERSION"]
            result = subprocess.run(
                ["bash", str(app / "dev/apk")], env=env, capture_output=True, text=True
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                (output.parent / "xmtp-messenger-local.apk").read_bytes(),
                b"built by Nix",
            )

    def test_apk_release_publication(self):
        workflow = yaml.safe_load(WORKFLOW.read_text())
        self.assertEqual(workflow["permissions"]["contents"], "read")
        jobs = workflow["jobs"]
        caller = yaml.safe_load((WORKFLOW.parent / "release.yml").read_text())["jobs"]
        self.assertEqual(caller["release-android"]["permissions"]["contents"], "write")
        steps = jobs["publish"]["steps"]
        names = [item.get("name") for item in steps]
        build = steps[names.index("Build example APK")]
        self.assertEqual(build["run"], "dev/nix-shell 'just example-android apk'")
        self.assertEqual(
            build["env"]["XMTP_EXAMPLE_VERSION"], "${{ steps.version.outputs.version }}"
        )
        self.assertLess(names.index("Build example APK"), names.index("Publish"))
        upload = steps[names.index("Retain example APK")]
        self.assertLess(names.index("Retain example APK"), names.index("Publish"))
        self.assertEqual(upload["with"]["if-no-files-found"], "error")
        self.assertEqual(
            upload["with"]["path"],
            "apps/example-android/build/apk/xmtp-messenger-${{ steps.version.outputs.version }}.apk",
        )
        job = jobs["publish-apk"]
        self.assertEqual(job["permissions"]["contents"], "write")
        self.assertEqual(set(job["needs"]), {"publish", "push-tag"})
        self.assertNotIn("if", job)
        self.assertNotIn("if", build)
        self.assertEqual(job["steps"][0]["with"]["name"], upload["with"]["name"])
        command = job["steps"][-1]["run"]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "apk").mkdir()
            asset = root / "apk/xmtp-messenger-8.0.0.apk"
            asset.write_bytes(b"APK")
            gh = root / "gh"
            gh.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['GH_LOG'], 'a') as log: log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "sys.exit(int(os.environ['VIEW_EXIT']) if sys.argv[2] == 'view' else int(os.environ.get('UPLOAD_EXIT', '0')))\n"
            )
            gh.chmod(0o755)
            log = root / "calls.jsonl"
            for kind in ("dev", "rc", "final", "nightly"):
                for existing in (False, True):
                    with self.subTest(kind=kind, existing=existing):
                        log.write_text("")
                        env = dict(
                            os.environ,
                            PATH=str(root) + os.pathsep + os.environ["PATH"],
                            GH_LOG=str(log),
                            VERSION="8.0.0",
                            RELEASE_TYPE=kind,
                            VIEW_EXIT="0" if existing else "1",
                        )
                        result = subprocess.run(
                            ["bash", "-euc", command],
                            cwd=root,
                            env=env,
                            capture_output=True,
                            text=True,
                        )
                        self.assertEqual(result.returncode, 0, result.stderr)
                        calls = [
                            json.loads(line) for line in log.read_text().splitlines()
                        ]
                        self.assertEqual(calls[0], ["release", "view", "android-8.0.0"])
                        self.assertEqual(len(calls), 2)
                        self.assertEqual(
                            calls[1][:4],
                            [
                                "release",
                                "upload" if existing else "create",
                                "android-8.0.0",
                                "apk/xmtp-messenger-8.0.0.apk",
                            ],
                        )
                        if existing:
                            self.assertIn("--clobber", calls[1])
                        else:
                            self.assertIn("--verify-tag", calls[1])
                            self.assertEqual(
                                "--prerelease" in calls[1], kind != "final"
                            )
            env["UPLOAD_EXIT"] = "44"
            result = subprocess.run(
                ["bash", "-euc", command], cwd=root, env=env, capture_output=True
            )
            self.assertEqual(result.returncode, 44)
            asset.unlink()
            log.write_text("")
            result = subprocess.run(
                ["bash", "-euc", command], cwd=root, env=env, capture_output=True
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(log.read_text(), "")

    def test_integration_failure_upload_retains_app_and_library_evidence(self):
        paths = []
        for name, title in (
            ("test-android.yml", "Upload failed Android test reports"),
            ("test-example-android.yml", "Upload failed Messenger test reports"),
        ):
            workflow = (ROOT / ".github/workflows" / name).read_text()
            upload = workflow.split(f"    - name: {title}\n", 1)[1]
            upload = upload.split("  results:", 1)[0]
            self.assertIn("      if: failure()\n", upload)
            block = upload.split("        path: |\n", 1)[1].split(
                "        include-hidden-files:", 1
            )[0]
            paths.extend(line.strip() for line in block.splitlines() if line.strip())
        required = {
            "apps/example-android/app/build/outputs/androidTest-results/connected/**/*.xml",
            "apps/example-android/app/build/outputs/androidTest-results/connected/**/*.txt",
            "apps/example-android/app/build/reports/androidTests/**",
            "apps/example-android/app/build/screenshots/**",
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
