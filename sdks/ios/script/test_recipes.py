"""Check iOS recipe paths without compiling the SDK."""

from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]


class RecipeTests(unittest.TestCase):
    def test_docs_create_output_parent_in_fresh_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            checkout = Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            tools = checkout / "bin"
            tools.mkdir()
            producer = tools / "swift"
            producer.write_text(
                f"#!{sys.executable}\n"
                "from pathlib import Path\n"
                "import sys\n"
                "args = sys.argv[1:]\n"
                "assert 'generate-documentation' in args\n"
                "output = Path(args[args.index('--output-path') + 1])\n"
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
        for recipe in ("test", "test-simulator"):
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
