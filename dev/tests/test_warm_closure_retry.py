#!/usr/bin/env python3
"""Exercise warm-closure's command, capture, validation, and retry boundaries."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "dev/ci/warm-closure"
MEMBER = "/nix/store/l8si8gnvvq93yzms1jsgh5aixyf9rl5x-member"
LAYER = "/nix/store/c2h2f4cw9p8i8zcfy52fd1dd6g0yhnki-deps.drv"
HEADER = "these 2 paths will be fetched (1.0 MiB download, 2.0 MiB unpacked):\n"
PATHS = f"  {MEMBER}\n  {LAYER.removesuffix('.drv')}\n"
PLAN = HEADER + PATHS
WARNING = (
    "warning: error: unable to download "
    "'https://cache.nixos.org/xmkcamc87rabl8pjkk4js7zxfqibvri8.narinfo': "
    "Error in the HTTP2 framing layer (16) ; retrying in 283 ms\n"
)

FAKE = """#!{python}
import json, os, pathlib, subprocess, sys
root = pathlib.Path(os.environ['FIXTURE'])
tool = pathlib.Path(sys.argv[0]).name
with (root / 'calls').open('a') as log:
    log.write(json.dumps([tool, *sys.argv[1:]]) + '\\n')
fixture = json.loads((root / 'fixture.json').read_text())
if tool == 'nix':
    command = sys.argv[1]
    if command == fixture.get('fail_command'):
        sys.exit(1)
    if command == 'eval':
        print(json.dumps({{'linux': {{'dev': {{'members': ['{member}'], 'drv': '{layer}'}}}}}}))
    elif command == 'derivation':
        if fixture.get('bad_json'):
            print('not json')
        else:
            print(json.dumps({{'{layer}': {{'env': {{}} if fixture.get('no_layers') else {{'CRANE_BUILD_DEPS_ONLY': '1'}}}}}}))
    elif command == 'build':
        counter = root / 'attempt'
        attempt = int(counter.read_text()) if counter.exists() else 0
        counter.write_text(str(attempt + 1))
        result = fixture['attempts'][min(attempt, len(fixture['attempts']) - 1)]
        sys.stderr.write(result['text'])
        sys.exit(result.get('status', 0))
    else:
        sys.exit('unexpected nix command')
elif tool == 'tee':
    result = subprocess.run([os.environ['REAL_TEE'], *sys.argv[1:]])
    sys.exit(74 if fixture.get('capture_failure') else result.returncode)
elif tool == 'timeout':
    sys.exit(subprocess.run([os.environ['REAL_TIMEOUT'], *sys.argv[1:]]).returncode)
elif tool != 'sleep':
    sys.exit('unexpected fixture tool')
""".format(python=sys.executable, member=MEMBER, layer=LAYER)


def run_closure(attempts, **options):
    with tempfile.TemporaryDirectory(prefix="warm-closure-") as directory:
        root = Path(directory)
        binary = root / "bin"
        binary.mkdir()
        for name in ("nix", "tee", "timeout", "sleep"):
            executable = binary / name
            executable.write_text(FAKE)
            executable.chmod(0o755)
        (root / "fixture.json").write_text(
            json.dumps({"attempts": attempts, **options})
        )
        result = subprocess.run(
            ["bash", str(SCRIPT), "--system", "x86_64-linux", "sets: sets"],
            env=os.environ
            | {
                "PATH": f"{binary}:{os.environ['PATH']}",
                "RUNNER_TEMP": str(root),
                "FIXTURE": str(root),
                "REAL_TEE": shutil.which("tee"),
                "REAL_TIMEOUT": shutil.which("timeout"),
            },
            capture_output=True,
            text=True,
            timeout=15,
        )
        calls = [json.loads(line) for line in (root / "calls").read_text().splitlines()]
        captures = {path.name: path.read_text() for path in root.glob("dry-run*.txt")}
        return result, calls, captures


class WarmClosureRetryTests(unittest.TestCase):
    def check_result(self, attempts, expected_attempts, success=False, **options):
        result, calls, captures = run_closure(attempts, **options)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode == 0, success, output)
        builds = [call for call in calls if call[:2] == ["nix", "build"]]
        self.assertEqual(len(builds), expected_attempts, output)
        self.assertEqual("Every set member" in output, success, output)
        return output, calls, captures

    def test_clean_plan_keeps_all_members_layers_and_dry_run_options(self):
        _, calls, _ = self.check_result([{"text": PLAN}], 1, success=True)
        build = next(call for call in calls if call[:2] == ["nix", "build"])
        self.assertEqual(
            build[2:],
            [
                "--dry-run",
                "--store",
                build[4],
                "--option",
                "always-allow-substitutes",
                "true",
                MEMBER,
                f"{LAYER}^out",
            ],
        )
        self.assertTrue(build[4].endswith("/empty"))
        self.assertFalse(any(call[0] == "sleep" for call in calls))

    def test_transient_then_clean_requires_a_fresh_unfiltered_attempt(self):
        for text in (WARNING + PLAN, HEADER + WARNING + PATHS, PLAN + WARNING * 5):
            with self.subTest(text=text):
                output, calls, captures = self.check_result(
                    [{"text": text}, {"text": PLAN}], 2, success=True
                )
                self.assertIn(WARNING.strip(), output)
                self.assertIn(text, captures.values())
                self.assertIn(PLAN, captures.values())
                builds = [call for call in calls if call[:2] == ["nix", "build"]]
                self.assertEqual(builds[0], builds[1])
                self.assertEqual(
                    [c for c in calls if c[0] == "sleep"], [["sleep", "5"]]
                )

    def test_repeated_transients_exhaust_three_attempts(self):
        output, calls, _ = self.check_result([{"text": WARNING + PLAN}], 3)
        self.assertIn("exhausted", output)
        self.assertEqual(
            [c for c in calls if c[0] == "sleep"], [["sleep", "5"], ["sleep", "10"]]
        )
        self.assertEqual(
            [c[1:3] for c in calls if c[0] == "timeout"],
            [["--kill-after=5s", "300s"]] * 3,
        )

    def test_invalid_plans_never_acquire_retries(self):
        for text in (
            "",
            "these 0 paths will be fetched (0 MiB download, 0 MiB unpacked):\n",
            PLAN.replace("these 2", "these 3"),
            HEADER + f"  {MEMBER}\n",
            PLAN + PLAN,
            PATHS + HEADER,
            PLAN + "this derivation will be built:\n  " + LAYER + "\n",
            PLAN + "don't know how to build these paths:\n  " + MEMBER + "\n",
            PLAN.replace(MEMBER, MEMBER.replace("l8si", "eeee")),
            PLAN.replace("MiB download", "bytes download"),
            PLAN + "unknown output without a newline",
        ):
            with self.subTest(text=text):
                self.check_result([{"text": WARNING + text}, {"text": PLAN}], 1)

    def test_unsupported_diagnostics_never_acquire_retries(self):
        for diagnostic in (
            "warning: unrelated configuration warning\n",
            "error: invalid signature\n",
            '@nix {"action":"unknown"}\n',
            WARNING.replace("cache.nixos.org", "unknown.example"),
            WARNING.replace(".narinfo", ".nar.zst"),
            WARNING.replace("283 ms", "283 ms unexpected suffix"),
            "warning: error: unable to download 'https://xmtp.cachix.org/x.narinfo': HTTP error 502\n"
            "response body:\n<html>502 Bad Gateway</html>\nretrying in 308 ms\n",
            "error: unable to download 'https://cache.nixos.org/nar/x.nar.zst': HTTP error 416\n",
        ):
            with self.subTest(diagnostic=diagnostic):
                self.check_result(
                    [{"text": WARNING + PLAN + diagnostic}, {"text": PLAN}], 1
                )

    def test_each_later_attempt_must_independently_pass(self):
        for second in (
            {"text": PLAN + "error: terminal failure\n"},
            {"text": WARNING + PLAN.replace("these 2", "these 3")},
            {"text": WARNING + PLAN, "status": 1},
        ):
            with self.subTest(second=second):
                self.check_result([{"text": WARNING + PLAN}, second, {"text": PLAN}], 2)

    def test_nonzero_commands_and_timeouts_are_fatal(self):
        for status in (1, 124, 137):
            with self.subTest(status=status):
                self.check_result(
                    [{"text": WARNING + PLAN, "status": status}, {"text": PLAN}], 1
                )

    def test_capture_failure_is_fatal_even_with_a_retryable_plan(self):
        self.check_result(
            [{"text": WARNING + PLAN}, {"text": PLAN}], 1, capture_failure=True
        )

    def test_unterminated_unknown_output_is_not_ignored(self):
        for diagnostic in ("unknown", WARNING.rstrip("\n")[:-2]):
            with self.subTest(diagnostic=diagnostic):
                self.check_result([{"text": PLAN + diagnostic}, {"text": PLAN}], 1)

    def test_preflight_errors_never_reach_the_dry_run(self):
        for options in (
            {"fail_command": "eval"},
            {"fail_command": "derivation"},
            {"bad_json": True},
            {"no_layers": True},
        ):
            with self.subTest(options=options):
                self.check_result([{"text": PLAN}], 0, **options)


if __name__ == "__main__":
    unittest.main()
