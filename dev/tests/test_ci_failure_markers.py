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

    def test_failure_causes_and_test_names_are_kept(self):
        rejection = (
            " ! [remote rejected] ios-8.0.0-dev.f694bb7 -> ios-8.0.0-dev.f694bb7 "
            "(refusing to allow a GitHub App to create or update workflow "
            "`.github/workflows/push-backend.yml` without `workflows` permission)"
        )
        # (log line, the marker it must leave, or None for a passing line)
        cases = [
            ("Reference cache failed: Reference inputs changed during the build",) * 2,
            ("SDK product failed: Current source does not match",) * 2,
            ("dev/js/sdk-package: line 13: python3.11: command not found",) * 2,
            ("TIMEOUT [120.003s] sdk::held_callback",) * 2,
            ("FAIL [0.1s] sdk::reader_end",) * 2,
            ("PASS [0.1s] sdk::healthy", None),
            (
                "apps/docs typecheck:prepared: browserType.launch: Executable doesn't exist at /runner/cache/chromium/headless_shell",
            )
            * 2,
            # Timestamps, ANSI colors, and Nix build prefixes are removed.
            (
                "2026-10-02T04:00:00.123Z wasm-nextest> \x1b[31m    TIMEOUT [120.003s] sdk::held\x1b[0m",
                "TIMEOUT [120.003s] sdk::held",
            ),
            ("wasm-nextest>     FAIL [0.2s] sdk::failed", "FAIL [0.2s] sdk::failed"),
            (
                "2026-10-02T04:00:00.123Z wasm-nextest> \x1b[32m    PASS sdk::colored\x1b[0m",
                None,
            ),
            ("Execution failed for task ' :library:check'.",) * 2,
            (
                " > Dependency verification failed for configuration runtime.",
                "Dependency verification failed for configuration runtime.",
            ),
            ("e: file:///checkout/Test.kt:10:2 Unresolved reference 'example'.",) * 2,
            ("org.xmtp.android.library.GroupTest > testReadd[API_34] FAILED",) * 2,
            ("org.xmtp.android.library.GroupTest > testHealthy[API_34] PASSED", None),
            (
                "sdks/agent typecheck: src/migration.test.ts(1,53): error TS2307: Cannot find module '@xmtp/agent-sdk'.",
            )
            * 2,
            (f"2026-10-06T22:55:36.7000470Z {rejection}", rejection),
            ("2026-10-06T22:55:36.7000470Z * [new tag] healthy -> healthy", None),
            ("test result: ok. 2 passed; 0 failed", None),
        ]
        log = "".join(line + "\n" for line, _ in cases)
        expected = sorted({marker for _, marker in cases if marker})
        self.assertEqual(self.filtered(log), expected)
        passing = "".join(line + "\n" for line, marker in cases if not marker)
        self.assertEqual(self.filtered(passing), [])

    def test_failure_output_stays_sorted_unique_and_bounded(self):
        failures = [f"error: case_{number:02d}\n" for number in range(50)]
        self.assertEqual(
            self.filtered("".join(reversed(failures)) + failures[0]),
            [line.rstrip("\n") for line in failures[:40]],
        )


if __name__ == "__main__":
    unittest.main()
