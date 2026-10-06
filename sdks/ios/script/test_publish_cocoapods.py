"""Check publication recovery with local tools. Never publish a pod."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(
    os.environ.get(
        "XMTP_PUBLICATION_SCRIPT", Path(__file__).with_name("publish-cocoapods.sh")
    )
).resolve()
TOOL = """import os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
calls = Path(os.environ['CALLS'])
previous = calls.read_text().splitlines() if calls.exists() else []
with calls.open('a') as f: f.write(name + '\\n')
scenario = os.environ['SCENARIO']
pushes = previous.count('pod')
if name == 'curl':
    assert sys.argv[-1].endswith('/Specs/a/b/7/XMTP/8.0.0-dev.fixture/XMTP.podspec.json')
    print('200' if scenario == 'existing' or scenario == 'landed' and pushes else '404')
elif name == 'pod':
    assert sys.argv[1:] == ['trunk', 'push', 'XMTP.podspec', '--allow-warnings', '--skip-tests']
    if scenario == 'success' or scenario == 'retry' and pushes:
        sys.exit(0)
    if scenario in ('retry', 'timeout', 'landed') or scenario == 'then-invalid' and not pushes:
        print('[!] Calling the GitHub commit API timed out.', file=sys.stderr)
    else:
        print('[!] The spec did not pass validation.', file=sys.stderr)
    sys.exit(1)
elif name == 'sleep':
    assert sys.argv[1:] == ['20']
else:
    raise AssertionError(name)
"""


class PublicationTests(unittest.TestCase):
    def run_case(self, scenario, pushes, success):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("pod", "curl", "sleep"):
                path = root / name
                path.write_text(f"#!{sys.executable}\n" + TOOL)
                path.chmod(0o755)
            calls = root / "calls"
            result = subprocess.run(
                ["bash", str(SCRIPT)],
                cwd=root,
                env=dict(
                    os.environ,
                    VERSION="8.0.0-dev.fixture",
                    SCENARIO=scenario,
                    CALLS=str(calls),
                    PATH=f"{root}:{os.environ['PATH']}",
                ),
                capture_output=True,
                text=True,
            )
            self.assertEqual(
                result.returncode == 0, success, result.stdout + result.stderr
            )
            events = calls.read_text().splitlines()
            self.assertEqual(events.count("pod"), pushes, result.stdout + result.stderr)
            return events

    def test_existing_version_skips_push(self):
        self.run_case("existing", 0, True)

    def test_successful_push(self):
        self.run_case("success", 1, True)

    def test_commit_timeout_retries(self):
        self.run_case("retry", 2, True)

    def test_repeated_timeouts_stop_after_three_pushes(self):
        self.run_case("timeout", 3, False)

    def test_landed_commit_does_not_push_again(self):
        events = self.run_case("landed", 1, True)
        self.assertNotIn("sleep", events)

    def test_validation_failure_does_not_retry(self):
        self.run_case("invalid", 1, False)

    def test_later_validation_failure_does_not_reuse_timeout_log(self):
        self.run_case("then-invalid", 2, False)


if __name__ == "__main__":
    unittest.main()
