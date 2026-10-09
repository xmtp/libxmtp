#!/usr/bin/env python3
"""Check the actual private export and integration command with host processes."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import export_screenshots as proof

ROOT = Path(__file__).resolve().parents[4]
PNG = proof.PNG + b"fixture image"


class ScreenshotExportTest(unittest.TestCase):
    def test_captured_remote_exit_distinguishes_absent_directory_from_inaccessible_app(
        self,
    ):
        for accessible in (True, False):
            with (
                self.subTest(accessible=accessible),
                tempfile.TemporaryDirectory() as home,
            ):
                calls = []

                def process(arguments, **options):
                    calls.append(arguments)
                    command = arguments[arguments.index(proof.APP) + 1 :]
                    raw = arguments[3] == "exec-out"
                    missing = (
                        f"ls: {proof.PRIVATE}: No such file or directory\n".encode()
                    )
                    denied = f"run-as: unknown package: {proof.APP}\n".encode()
                    # Captured Android transport behavior: exec-out returns 0
                    # and puts remote errors in stdout; shell-v2 separates them.
                    error = (
                        denied
                        if not accessible
                        else missing
                        if command[0] == "ls"
                        else b""
                    )
                    status = 0 if raw or not error else 1
                    stdout, stderr = (error, b"") if raw else (b"", error)
                    if status and options.get("check"):
                        raise subprocess.CalledProcessError(
                            status, arguments, output=stdout, stderr=stderr
                        )
                    return subprocess.CompletedProcess(
                        arguments, status, stdout=stdout, stderr=stderr
                    )

                with patch.object(proof.subprocess, "run", side_effect=process):
                    if accessible:
                        try:
                            self.assertEqual(
                                0, proof.export("emulator-fixture", Path(home))
                            )
                        except ValueError:
                            self.fail(
                                "Absent directory error was parsed as a screenshot name"
                            )
                    else:
                        with self.assertRaises(
                            subprocess.CalledProcessError
                        ) as failure:
                            proof.export("emulator-fixture", Path(home))
                        self.assertIn(b"unknown package", failure.exception.stderr)
                self.assertFalse(list(Path(home).iterdir()))
                self.assertEqual(
                    ["rm", "-rf", proof.PRIVATE],
                    calls[-1][calls[-1].index(proof.APP) + 1 :],
                )

    def test_real_android_columnar_listing_is_avoided_by_direct_canonical_argv(self):
        observed = (
            "scale-app_settings.png           scale-create.png        scale-my_fields.png\n"
            "scale-conversation_settings.png  scale-drafts.png        scale-start.png\n"
            "scale-conversations.png          scale-group_fields.png  scale-timeline.png\n"
        )
        names = observed.split()
        calls = []

        def process(arguments, **options):
            calls.append(arguments)
            command = arguments[arguments.index(proof.APP) + 1 :]
            if command == ["ls", "-1", proof.PRIVATE]:
                stdout = ("\n".join(names) + "\n").encode()
            elif command[0] == "sh" or command[0] == "ls":
                stdout = observed.encode()
            else:
                stdout = PNG if command[0] == "head" else b""
            return subprocess.CompletedProcess(arguments, 0, stdout=stdout, stderr=b"")

        with (
            tempfile.TemporaryDirectory() as home,
            patch.object(proof.subprocess, "run", side_effect=process),
        ):
            try:
                self.assertEqual(9, proof.export("emulator-fixture", Path(home)))
            except ValueError:
                self.fail(
                    "Exporter did not request one canonical fixture name per line"
                )
            self.assertEqual(set(names), {path.name for path in Path(home).iterdir()})
        self.assertEqual(
            ["ls", "-1", proof.PRIVATE], calls[0][calls[0].index(proof.APP) + 1 :]
        )
        self.assertEqual(["shell", "-T"], calls[0][3:5])

    def test_absent_directory_is_empty_only_when_the_owned_app_can_prove_absence(self):
        for accessible in (True, False):
            with (
                self.subTest(accessible=accessible),
                tempfile.TemporaryDirectory() as home,
            ):
                first = subprocess.CalledProcessError(1, ["ls"])
                calls = []

                def owned(serial, *arguments):
                    calls.append(arguments)
                    if arguments[0] == "ls":
                        raise first
                    if arguments[0] == "test" and not accessible:
                        raise subprocess.CalledProcessError(1, ["run-as"])
                    return b""

                with patch.object(proof, "owned", side_effect=owned):
                    if accessible:
                        self.assertEqual(
                            0, proof.export("emulator-fixture", Path(home))
                        )
                    else:
                        with self.assertRaises(
                            subprocess.CalledProcessError
                        ) as failure:
                            proof.export("emulator-fixture", Path(home))
                        self.assertIs(first, failure.exception)
                self.assertEqual(("test", "!", "-e", proof.PRIVATE), calls[1])
                self.assertEqual(("rm", "-rf", proof.PRIVATE), calls[-1])

    def test_only_named_fixture_files_are_exported_then_private_files_are_removed(self):
        calls = []
        with tempfile.TemporaryDirectory() as home:
            output = Path(home)
            (output / "setup.png").write_bytes(b"old screenshot")
            (output / "unrelated.txt").write_text("keep")

            def owned(serial, *arguments):
                calls.append(arguments)
                if arguments[0] == "ls":
                    return b"scale-start.png\n"
                return PNG if arguments[0] == "head" else b""

            with patch.object(proof, "owned", side_effect=owned):
                self.assertEqual(1, proof.export("emulator-fixture", output))
            self.assertEqual(PNG, (output / "scale-start.png").read_bytes())
            self.assertFalse((output / "setup.png").exists())
            self.assertEqual("keep", (output / "unrelated.txt").read_text())
            self.assertEqual(
                (
                    "head",
                    "-c",
                    str(proof.LIMIT + 1),
                    "files/xmtp-messenger-proof/scale-start.png",
                ),
                calls[1],
            )
            self.assertEqual(("rm", "-rf", "files/xmtp-messenger-proof"), calls[-1])

    def test_unknown_filename_is_never_read_or_exported(self):
        calls = []
        with tempfile.TemporaryDirectory() as home:

            def owned(serial, *arguments):
                calls.append(arguments)
                return b"profile.json\n" if arguments[0] == "ls" else PNG

            with patch.object(proof, "owned", side_effect=owned):
                with self.assertRaisesRegex(ValueError, "Unknown or duplicate"):
                    proof.export("emulator-fixture", Path(home))
            self.assertFalse(any(call[0] == "head" for call in calls))
            self.assertFalse((Path(home) / "profile.json").exists())
            self.assertEqual(("rm", "-rf", proof.PRIVATE), calls[-1])

    def test_invalid_png_fails_and_still_cleans_owned_files(self):
        for image in (b"not PNG", PNG + bytes(proof.LIMIT)):
            with self.subTest(size=len(image)), tempfile.TemporaryDirectory() as home:
                calls = []

                def owned(serial, *arguments):
                    calls.append(arguments)
                    return b"scale-start.png\n" if arguments[0] == "ls" else image

                with patch.object(proof, "owned", side_effect=owned):
                    with self.assertRaisesRegex(ValueError, "Invalid fixture PNG"):
                        proof.export("emulator-fixture", Path(home))
                self.assertEqual(("rm", "-rf", proof.PRIVATE), calls[-1])

    def test_read_failure_survives_cleanup_failure_and_cleanup_is_attempted(self):
        first = OSError("fixture read failed")
        calls = []
        with tempfile.TemporaryDirectory() as home:

            def owned(serial, *arguments):
                calls.append(arguments)
                if arguments[0] == "ls":
                    return b"scale-start.png\n"
                raise (
                    first
                    if arguments[0] == "head"
                    else RuntimeError("fixture cleanup failed")
                )

            with patch.object(proof, "owned", side_effect=owned):
                with self.assertRaises(OSError) as failure:
                    proof.export("emulator-fixture", Path(home))
            self.assertIs(first, failure.exception)
            self.assertEqual(("rm", "-rf", proof.PRIVATE), calls[-1])
            self.assertIn("RuntimeError", first.__notes__[0])

    def test_actual_recipe_retains_private_artifacts_for_export_then_removes_them(self):
        recipe = (ROOT / "sdks/android/android.just").read_text()
        line = next(
            line
            for line in recipe.splitlines()
            if "example/fixtures/metadata_backend.py" in line and "bash -euc" in line
        )
        body = line.split("bash -euc '", 1)[1].removesuffix("'")
        with tempfile.TemporaryDirectory() as home:
            fixture = Path(home)
            android = fixture / "android"
            binaries = fixture / "bin"
            binaries.mkdir()
            private = fixture / "private"
            private.mkdir()
            (private / "scale-start.png").write_bytes(PNG)
            installed = fixture / "installed"
            installed.touch()
            scripts = android / "example/fixtures"
            scripts.mkdir(parents=True)
            shutil.copyfile(Path(proof.__file__), scripts / "export_screenshots.py")
            gradle = android / "gradlew"
            gradle.write_text(
                f"#!{sys.executable}\n"
                + """import os, pathlib, shutil, sys
home = pathlib.Path(os.environ["SCREENSHOT_FIXTURE_HOME"])
if "-Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true" not in sys.argv:
    (home / "installed").unlink()
    shutil.rmtree(home / "private")
sys.exit(int(os.environ.get("FIXTURE_TEST_STATUS", "0")))
"""
            )
            gradle.chmod(0o755)
            adb = binaries / "adb"
            adb.write_text(
                f"#!{sys.executable}\n"
                + """import os, pathlib, shutil, sys
home = pathlib.Path(os.environ["SCREENSHOT_FIXTURE_HOME"])
args = sys.argv[3:]
if args[0] == "reverse": sys.exit(0)
if args[0] == "shell":
    assert args[:4] == ["shell", "-T", "run-as", "org.xmtp.android.example"]
    args = args[4:]
else:
    assert args[:3] == ["exec-out", "run-as", "org.xmtp.android.example"]
    args = args[3:]
if not (home / "installed").exists(): sys.exit(1)
if args == ["ls", "-1", "files/xmtp-messenger-proof"]:
    print("\\n".join(path.name for path in (home / "private").iterdir()))
elif args[0] == "head":
    assert args[3].startswith("files/xmtp-messenger-proof/")
    sys.stdout.buffer.write((home / "private" / pathlib.Path(args[3]).name).read_bytes())
elif args == ["rm", "-rf", "files/xmtp-messenger-proof"]:
    shutil.rmtree(home / "private")
else: raise AssertionError(args)
"""
            )
            adb.chmod(0o755)
            environment = dict(
                os.environ,
                SCREENSHOT_FIXTURE_HOME=home,
                PATH=str(binaries) + os.pathsep + os.environ["PATH"],
                ANDROID_SERIAL="emulator-fixture",
            )
            for key in (
                "XMTP_METADATA_BACKEND_PORT",
                "XMTP_BACKEND_PORT",
                "XMTP_S3_PORT",
                "XMTP_ANDROID_S3_GATE_PORT",
                "XMTP_TOXIPROXY_PORT",
                "XMTP_TOXIPROXY_API_PORT",
                "XMTP_ANDROID_UNSUPPORTED_BACKEND_PORT",
            ):
                environment[key] = "1234"
            for key in (
                "XMTP_METADATA_BACKEND_URL",
                "XMTP_BACKEND_TOXIC_URL",
                "XMTP_TOXIPROXY_API",
                "XMTP_ANDROID_UNSUPPORTED_BACKEND_URL",
            ):
                environment[key] = "http://127.0.0.1:1234"
            command = body.replace("{{ root }}", home).replace("{{ args }}", "")
            output = fixture / "sdks/android/example/build/screenshots/scale-start.png"
            for status, image in (
                (0, PNG),
                (7, PNG),
                (7, b"invalid PNG"),
                (0, b"invalid PNG"),
            ):
                with self.subTest(test_status=status, valid=image == PNG):
                    private.mkdir(exist_ok=True)
                    (private / "scale-start.png").write_bytes(image)
                    installed.touch()
                    environment["FIXTURE_TEST_STATUS"] = str(status)
                    result = subprocess.run(
                        ["bash", "-euc", command],
                        cwd=android,
                        env=environment,
                        text=True,
                        capture_output=True,
                        timeout=10,
                    )
                    expected = status if status else 0 if image == PNG else 1
                    self.assertEqual(
                        expected, result.returncode, result.stdout + result.stderr
                    )
                    if image == PNG:
                        self.assertEqual(PNG, output.read_bytes())
                    else:
                        self.assertFalse(output.exists())
                    self.assertFalse(
                        private.exists(), "Owned private screenshot survived export"
                    )


if __name__ == "__main__":
    unittest.main()
