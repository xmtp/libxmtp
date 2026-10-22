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


class AndroidReleaseTest(unittest.TestCase):
    def test_publish_credentials_and_failure_gate(self):
        workflow = yaml.safe_load(WORKFLOW.read_text())
        job = workflow["jobs"]["publish"]
        self.assertEqual(job["environment"], "release")
        self.assertEqual(
            workflow["jobs"]["publish-packages"]["permissions"]["packages"], "write"
        )
        caller = yaml.safe_load((WORKFLOW.parent / "release.yml").read_text())
        self.assertEqual(
            caller["jobs"]["release-android"]["permissions"]["packages"], "write"
        )
        publish = next(
            step
            for step in job["steps"]
            if step.get("name") == "Prepare signed Maven bundle"
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sdk = root / "sdks/android"
            sdk.mkdir(parents=True)
            dev = root / "dev"
            dev.mkdir()
            wrapper = dev / "nix-shell"
            wrapper.write_text('#!/bin/bash\nexec bash -euc "$1"\n')
            wrapper.chmod(0o755)
            gradle = sdk / "gradlew"
            gradle.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "Path(os.environ['GRADLE_LOG']).write_text(json.dumps(sys.argv[1:]))\n"
                "sys.exit(int(os.environ.get('GRADLE_EXIT', '0')))\n"
            )
            gradle.chmod(0o755)
            log = root / "gradle.json"
            credentials = {name: "fixture-secret" for name in publish["env"]}
            env = dict(os.environ, **credentials, GRADLE_LOG=str(log))
            for missing in (None, *credentials):
                for status in (0, 42) if missing is None else (0,):
                    with self.subTest(missing=missing, gradle_exit=status):
                        log.unlink(missing_ok=True)
                        current = env | {"GRADLE_EXIT": str(status)}
                        if missing:
                            current[missing] = ""
                        result = subprocess.run(
                            ["bash", "-euc", publish["run"]],
                            cwd=root,
                            env=current,
                            capture_output=True,
                            text=True,
                        )
                        self.assertEqual(result.returncode, 1 if missing else status)
                        self.assertNotIn(
                            "fixture-secret", result.stdout + result.stderr
                        )
                        if missing:
                            self.assertFalse(log.exists())
                            self.assertIn(missing, result.stdout)
                        else:
                            self.assertEqual(
                                json.loads(log.read_text()),
                                [
                                    ":library:publishReleasePublicationToReleaseBundleRepository",
                                ],
                            )

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
        self.assertEqual(build["env"]["XMTP_EXAMPLE_VERSION"], "${{ inputs.version }}")
        self.assertLess(
            names.index("Build example APK"), names.index("Prepare signed Maven bundle")
        )
        upload = steps[names.index("Retain example APK")]
        self.assertLess(
            names.index("Retain example APK"),
            names.index("Prepare signed Maven bundle"),
        )
        self.assertEqual(upload["with"]["if-no-files-found"], "error")
        self.assertEqual(
            upload["with"]["path"],
            "apps/example-android/build/apk/xmtp-messenger-${{ inputs.version }}.apk",
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
            for kind in ("dev", "rc", "final"):
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

    def test_release_reads_no_ci_gradle_cache(self):
        # CI jobs write the Gradle cache (setup-gradle); a release must not
        # read a store that CI jobs write, and its Gradle properties enable no
        # build or configuration cache. Compose builds no image, so no buildx
        # disk.
        setup = yaml.safe_load(WORKFLOW.read_text())["jobs"]["publish"]["steps"]
        inputs = [
            item.get("with", {})
            for item in setup
            if item.get("uses") == "./.github/actions/setup-nix"
        ]
        self.assertEqual(len(inputs), 1)
        self.assertNotIn("gradle-disk", inputs[0])
        self.assertNotIn("docker-builder", inputs[0])
        for path in (WORKFLOW, ROOT / "sdks/android/gradle.properties"):
            with self.subTest(path=path.name):
                self.assertNotRegex(
                    path.read_text(),
                    r"setup-gradle|actions/cache|configuration-cache|build-cache"
                    r"|org\.gradle\.(caching|configuration-cache)",
                )


if __name__ == "__main__":
    unittest.main()
