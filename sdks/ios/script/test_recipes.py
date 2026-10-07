"""Check iOS recipe paths without compiling the SDK."""

from pathlib import Path
import os
import json
import shlex
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]


class RecipeTests(unittest.TestCase):
    def test_xcode_recipes_use_supported_environment_and_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            tools = Path(directory)
            producer = tools / "xcodebuild"
            producer.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "assert 'LD' not in os.environ, 'XCODE_LINKER_DRIVER_ENV_RETAINED'\n"
                "print(json.dumps(sys.argv[1:]))\n"
            )
            producer.chmod(0o755)
            count = 0
            for recipe in ("check-examples", "test-simulator"):
                result = subprocess.run(
                    ["just", "--dry-run", "ios", recipe],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    check=True,
                )
                commands = result.stdout + result.stderr
                for line in commands.splitlines():
                    if "xcodebuild " not in line:
                        continue
                    # Run the Xcode command with a recording tool. Keep SDK
                    # generation and backend setup outside this control.
                    command = line.rsplit("&&", 1)[-1].strip()
                    count += 1
                    with self.subTest(recipe=recipe, command=command):
                        run = subprocess.run(
                            ["bash", "-euc", command],
                            cwd=ROOT,
                            env=dict(
                                os.environ,
                                LD="ld",
                                XMTP_BACKEND_URL="http://fixture.invalid",
                                PATH=f"{tools}:{os.environ['PATH']}",
                            ),
                            capture_output=True,
                            text=True,
                        )
                        self.assertEqual(run.returncode, 0, run.stderr)
                        args = json.loads(run.stdout)
                        self.assertEqual(
                            args[0], "build" if recipe == "check-examples" else "test"
                        )
                        if recipe == "check-examples":
                            self.assertIn("ARCHS=arm64", args)
                            self.assertIn("CODE_SIGNING_ALLOWED=NO", args)
            self.assertEqual(count, 3)

    def test_swift_recipes_keep_default_job_counts(self):
        for recipe in ("check", "test"):
            with self.subTest(recipe=recipe):
                result = subprocess.run(
                    ["just", "--dry-run", "ios", recipe],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    check=True,
                )
                commands = result.stdout + result.stderr
                self.assertNotIn("--jobs", commands)
                self.assertNotIn("--num-workers", commands)

    def test_seam_proofs_run_once_across_ci_jobs(self):
        # test-sdk.yml runs `test-seams`; test-ios.yml runs `test skip-seams`.
        # The two must name the same tests, or a proof runs twice or never.
        def swift_test_args(*recipe):
            result = subprocess.run(
                ["just", "--dry-run", "ios", *recipe],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=True,
            )
            command = next(
                line
                for line in (result.stdout + result.stderr).splitlines()
                if "swift test" in line
            )
            return shlex.split(command.split("swift test", 1)[1])

        selected = swift_test_args("test-seams")
        skipped = swift_test_args("test", "skip-seams")
        self.assertEqual(selected[0], "--filter")
        self.assertEqual(skipped[-2], "--skip")
        self.assertEqual(selected[1], skipped[-1])
        self.assertNotIn("--skip", swift_test_args("test"))
        for test_case in ("ReaderTeardownTests", "ListenerGateTests"):
            self.assertRegex(f"XmtpSdkTests.{test_case}/testA", selected[1])
        rejected = subprocess.run(
            ["just", "--dry-run", "ios", "test", "skip"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(rejected.returncode, 0)

    def test_docs_create_output_parent_in_fresh_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            checkout = Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            tools = checkout / "bin"
            tools.mkdir()
            builder = tools / "nix"
            builder.write_text("#!/usr/bin/env bash\nexit 0\n")
            builder.chmod(0o755)
            producer = tools / "python3.11"
            producer.write_text(
                f"#!{sys.executable}\n"
                "from pathlib import Path\n"
                "import sys\n"
                "args = sys.argv[1:]\n"
                "assert args[0].endswith('/sdks/ios/script/docs-package.py')\n"
                "assert '--product' in args and '--receipt' in args\n"
                "output = Path(args[args.index('--output') + 1])\n"
                "assert output.is_absolute()\n"
                "output.mkdir()\n"
                "(output / 'index.html').write_text('DocC output fixture')\n"
            )
            producer.chmod(0o755)
            result = subprocess.run(
                ["bash", str(ROOT / "sdks/ios/script/generate-docs.sh")],
                cwd=checkout,
                env=dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}"),
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(
                (checkout / "apps/docs/generated/reference/swift/index.html").is_file()
            )

    def test_tests_use_repository_backend_helpers(self):
        prefix = f"{ROOT}/dev/worktree-env && . {ROOT}/dev/docker/load-env && "
        for recipe in ("test", "test-seams", "test-simulator"):
            with self.subTest(recipe=recipe):
                result = subprocess.run(
                    ["just", "--dry-run", "ios", recipe],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    check=True,
                )
                commands = result.stdout + result.stderr
                helper_command = next(
                    line
                    for line in commands.splitlines()
                    if "/dev/worktree-env" in line
                )
                self.assertTrue(helper_command.startswith(prefix), helper_command)
                self.assertTrue((ROOT / "dev/worktree-env").is_file())
                self.assertTrue((ROOT / "dev/docker/load-env").is_file())


if __name__ == "__main__":
    unittest.main()
