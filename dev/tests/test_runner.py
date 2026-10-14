"""Check dev/tests/run against fixture start directories, not the real tests."""

import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "dev/tests/run"
START_DIRS = (
    "dev/tests",
    "dev/specs",
    "apps/example-android/fixtures",
    "apps/example-android/performance",
    "nix/lib",
)
PASSING = """
import unittest


class Passing(unittest.TestCase):
    def test_passes(self):
        pass
"""


class RunnerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="dev tests run ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # The runner finds its start directories from its own path.
        (self.root / "dev/tests").mkdir(parents=True)
        shutil.copy2(RUNNER, self.root / "dev/tests/run")
        for start in START_DIRS:
            self.write(f"{start}/test_base.py", PASSING)

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(textwrap.dedent(text))

    def run_runner(self):
        env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}
        return subprocess.run(
            [sys.executable, "-B", str(self.root / "dev/tests/run")],
            # The runner, not the caller, sets the working directory.
            cwd=self.root.parent,
            env=env,
            check=False,
            capture_output=True,
            text=True,
            timeout=60,
        )

    def assert_line(self, result, prefix):
        lines = result.stdout.splitlines()
        self.assertTrue(
            any(line.startswith(prefix) for line in lines),
            f"no line starts with {prefix!r}:\n{result.stdout}{result.stderr}",
        )

    def test_discovers_every_start_directory_and_runs_from_the_root(self):
        self.write("dev/specs/helper_notes.py", "raise RuntimeError('not a test')\n")
        self.write(
            "apps/example-android/fixtures/test_cwd.py",
            """
            import unittest
            from pathlib import Path


            class WorkingDirectory(unittest.TestCase):
                def test_is_the_root(self):
                    root = Path(__file__).resolve().parents[3]
                    self.assertEqual(Path.cwd().resolve(), root)
            """,
        )
        result = self.run_runner()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for start in START_DIRS:
            self.assert_line(result, f"ok    {start}/test_base.py (1 tests")
        self.assert_line(result, "ok    apps/example-android/fixtures/test_cwd.py")
        self.assertEqual(result.stdout.splitlines()[-1], "6 tests; 0 failed modules")

    def test_every_failure_mode_fails_the_run(self):
        """The runner fails open on nothing: each broken module or directory fails it."""
        probes = {
            "test_a_fails.py": """
                import unittest


                class Probe(unittest.TestCase):
                    def test_fails(self):
                        self.fail("probe failure")
                """,
            "test_b_import.py": "import missing_probe_module\n",
            "test_c_exit.py": """
                import sys
                import unittest


                class Probe(unittest.TestCase):
                    @classmethod
                    def setUpClass(cls):
                        sys.exit(0)

                    def test_never_runs(self):
                        pass
                """,
            "test_d_no_tests.py": "def test_pytest_style():\n    pass\n",
            "test_e_skipped.py": """
                import unittest


                class Probe(unittest.TestCase):
                    @unittest.skip("probe")
                    def test_skipped(self):
                        pass
                """,
        }
        for name, text in probes.items():
            self.write(f"dev/specs/{name}", text)
        # A directory process that dies before, or exits non-zero after, its
        # summary, and a start directory with no tests.
        self.write("nix/lib/test_probe.py", "import os\nos._exit(0)\n")
        self.write(
            "apps/example-android/performance/test_probe.py",
            """
            import atexit
            import os
            import unittest

            atexit.register(os._exit, 3)


            class Probe(unittest.TestCase):
                def test_passes(self):
                    pass
            """,
        )
        (self.root / "apps/example-android/fixtures/test_base.py").unlink()
        result = self.run_runner()
        self.assertEqual(result.returncode, 1)
        # Five modules, two directory processes, and one empty start directory.
        self.assertRegex(result.stdout.splitlines()[-1], r"; 8 failed modules$")
        for line in (
            "FAIL  dev/specs/test_a_fails.py (1 tests",
            "FAIL  dev/specs/test_b_import.py (0 tests",
            "FAIL  dev/specs/test_c_exit.py",
            "FAIL  dev/specs/test_d_no_tests.py (0 tests",
            "FAIL  dev/specs/test_e_skipped.py (1 tests, 1 skipped",
            "ok    dev/specs/test_base.py (1 tests",
            "ERROR nix/lib: the test process exited 0",
            "ok    apps/example-android/performance/test_probe.py (1 tests",
            "ERROR apps/example-android/performance: the test process exited 3",
        ):
            self.assert_line(result, line)
        for detail in (
            "probe failure",
            "missing_probe_module",
            "SystemExit",
            "No tests ran",
            "Every test was skipped",
            "===== apps/example-android/fixtures\nNo test_*.py files",
        ):
            self.assertIn(detail, result.stderr)

    def test_sibling_imports_work_and_other_directories_do_not(self):
        self.write("dev/specs/specs_helper.py", "VALUE = 1\n")
        self.write("dev/tests/runner_helper.py", "VALUE = 2\n")
        self.write(
            "dev/specs/test_sibling.py",
            """
            import unittest

            import specs_helper


            class Sibling(unittest.TestCase):
                def test_imports_sibling(self):
                    self.assertEqual(specs_helper.VALUE, 1)
            """,
        )
        for name, helper in [("other", "specs_helper"), ("runner", "runner_helper")]:
            self.write(
                f"nix/lib/test_{name}_directory.py",
                f"""
                import unittest

                import {helper}


                class Elsewhere(unittest.TestCase):
                    def test_imports_elsewhere(self):
                        self.assertTrue({helper}.VALUE)
                """,
            )
        result = self.run_runner()
        self.assertEqual(result.returncode, 1)
        self.assert_line(result, "ok    dev/specs/test_sibling.py (1 tests")
        self.assert_line(result, "FAIL  nix/lib/test_other_directory.py (0 tests")
        self.assert_line(result, "FAIL  nix/lib/test_runner_directory.py (0 tests")


if __name__ == "__main__":
    unittest.main()
