"""Check iOS recipe paths without compiling the SDK."""

from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[3]


class RecipeTests(unittest.TestCase):
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
