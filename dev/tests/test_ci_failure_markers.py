#!/usr/bin/env python3
"""Check failure markers through the real Just diagnostic recipe."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
JUSTFILE = Path(os.environ.get("CI_DIAGNOSTIC_JUSTFILE", ROOT / "justfile"))
GH = """#!/usr/bin/env python3
import os
from pathlib import Path
import sys
if sys.argv[1:] == ["api", "--help"]:
    print("--allow-escape-sequences")
elif sys.argv[1:] == ["api", "--allow-escape-sequences", "repos/xmtp/libxmtp/actions/jobs/fixture/logs"]:
    sys.stdout.write(Path(os.environ["CI_FAILURE_FIXTURE"]).read_text())
else:
    raise SystemExit("unexpected gh command: " + repr(sys.argv[1:]))
"""


class FailureMarkers(unittest.TestCase):
    def filtered(self, text):
        with tempfile.TemporaryDirectory(prefix="ci-failure-markers-") as directory:
            folder = Path(directory)
            fixture = folder / "input.txt"
            fixture.write_text(text)
            gh = folder / "gh"
            gh.write_text(GH)
            gh.chmod(0o755)
            result = subprocess.run(
                [
                    "just",
                    "--justfile",
                    str(JUSTFILE),
                    "--working-directory",
                    str(ROOT),
                    "ci-failures",
                    "fixture",
                ],
                env=dict(
                    os.environ,
                    PATH=str(folder) + os.pathsep + os.environ["PATH"],
                    CI_FAILURE_FIXTURE=str(fixture),
                ),
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            return result.stdout.splitlines()

    def test_plain_timeout_and_failure_keep_test_names(self):
        self.assertEqual(
            self.filtered(
                "TIMEOUT [120.003s] sdk::held_callback\n"
                "FAIL [0.1s] sdk::reader_end\n"
                "PASS [0.1s] sdk::healthy\n"
            ),
            ["FAIL [0.1s] sdk::reader_end", "TIMEOUT [120.003s] sdk::held_callback"],
        )

    def test_timestamp_ansi_and_nix_prefix_keep_test_names(self):
        self.assertEqual(
            self.filtered(
                "2026-10-02T04:00:00.123Z wasm-nextest> \x1b[31m    TIMEOUT [120.003s] sdk::held\x1b[0m\n"
                "2026-10-02T04:00:01.123Z wasm-nextest>     FAIL [0.2s] sdk::failed\n"
                "wasm-nextest>     PASS [0.1s] sdk::healthy\n"
            ),
            ["FAIL [0.2s] sdk::failed", "TIMEOUT [120.003s] sdk::held"],
        )

    def test_gradle_and_nix_diagnostics_keep_the_cause(self):
        self.assertEqual(
            self.filtered(
                "Execution failed for task ' :library:check'.\n"
                " > Dependency verification failed for configuration runtime.\n"
                "e: file:///checkout/Test.kt:10:2 Unresolved reference 'example'.\n"
            ),
            [
                "Dependency verification failed for configuration runtime.",
                "Execution failed for task ' :library:check'.",
                "e: file:///checkout/Test.kt:10:2 Unresolved reference 'example'.",
            ],
        )

    def test_gradle_failure_keeps_the_test_name(self):
        self.assertEqual(
            self.filtered(
                "org.xmtp.android.library.GroupTest > testReadd[API_34] FAILED\n"
                "org.xmtp.android.library.GroupTest > testHealthy[API_34] PASSED\n"
            ),
            ["org.xmtp.android.library.GroupTest > testReadd[API_34] FAILED"],
        )

    def test_passing_output_is_empty(self):
        self.assertEqual(
            self.filtered(
                "PASS [0.1s] sdk::plain\n"
                "2026-10-02T04:00:00.123Z wasm-nextest> \x1b[32m    PASS sdk::colored\x1b[0m\n"
                "test result: ok. 2 passed; 0 failed\n"
            ),
            [],
        )

    def test_git_push_rejection_keeps_the_permission_cause(self):
        rejection = (
            " ! [remote rejected] ios-8.0.0-dev.f694bb7 -> ios-8.0.0-dev.f694bb7 "
            "(refusing to allow a GitHub App to create or update workflow "
            "`.github/workflows/push-backend.yml` without `workflows` permission)"
        )
        self.assertEqual(
            self.filtered(
                f"2026-10-06T22:55:36.7000470Z {rejection}\n"
                "2026-10-06T22:55:36.7000470Z * [new tag] healthy -> healthy\n"
            ),
            [rejection],
        )

    def test_failure_output_stays_sorted_unique_and_bounded(self):
        failures = [f"error: case_{number:02d}\n" for number in range(50)]
        self.assertEqual(
            self.filtered("".join(reversed(failures)) + failures[0]),
            [line.rstrip("\n") for line in failures[:40]],
        )


if __name__ == "__main__":
    unittest.main()
