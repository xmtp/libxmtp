"""Real process controls for outer timeout cleanup; no native SDK is used."""

import json
import os
import signal
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import ios_cleanup
import runner
from fixtures import canonical, digest

BENCH = Path(__file__).resolve().parent
sys.path.insert(0, str(BENCH / "hosts"))
from ios_identity import tree_identity  # noqa: E402 - requires the path setup above


def running(pid):
    result = subprocess.run(
        ["ps", "-p", str(pid), "-o", "stat="], capture_output=True, text=True
    )
    return bool(result.stdout.strip()) and not result.stdout.strip().startswith("Z")


class IOSOuterTimeoutTests(unittest.TestCase):
    def test_outer_timeout_stops_separate_app_during_operation_and_cleanup(self):
        for adapter_timeout, termination_delay, entered_cleanup in [
            (1.2, 3, True),
            (10, 0.2, False),
        ]:
            with (
                self.subTest(adapter_timeout=adapter_timeout),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                app = root / "app"
                app.mkdir()
                (app / "binary").write_bytes(b"process-double-not-native-SDK")
                data = root / "data"
                data.mkdir()
                state = root / "state"
                state.mkdir()
                fixture = {"messages": []}
                (state / "fixture.json").write_bytes(canonical(fixture))
                marker = root / "active-pid"
                shim = root / "simctl-double"
                shim.write_text(
                    "#!"
                    + sys.executable
                    + "\n"
                    + textwrap.dedent("""
                    import sys, os, signal, time, pathlib, subprocess, json
                    root = pathlib.Path(__file__).parent
                    action = sys.argv[2]
                    marker = root / 'active-pid'
                    if action == 'terminate':
                        if marker.exists():
                            with (root / 'terminations').open('a') as log:
                                log.write(str(os.getppid()) + '\\n')
                            time.sleep(json.loads((root / 'delay.json').read_text()))
                            os.kill(int(marker.read_text()), signal.SIGKILL)
                            marker.unlink()
                        else:
                            print('found nothing to terminate', file=sys.stderr)
                            sys.exit(3)
                    elif action == 'get_app_container':
                        print(root / ('app' if sys.argv[-1] == 'app' else 'data'))
                    elif action == 'launch':
                        child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'],
                                                 start_new_session=True, stdout=subprocess.DEVNULL,
                                                 stderr=subprocess.DEVNULL)
                        marker.write_text(str(child.pid))
                        (root / 'launched-pid').write_text(str(child.pid))
                """)
                )
                shim.chmod(0o755)
                (root / "delay.json").write_text(json.dumps(termination_delay))
                receipt = {
                    "side": "new",
                    "package_sha256": "package",
                    "app_build_id": "build",
                    "simulator_udid": "explicit-udid",
                    "bundle_id": "org.xmtp.benchmark.new",
                    "app_path": str(app),
                    "app_sha256": tree_identity(app),
                }
                (root / "receipt.json").write_text(json.dumps(receipt))
                config = {
                    "build_receipt": str(root / "receipt.json"),
                    "simulator_udid": "explicit-udid",
                    "backend_url": "http://localhost:1",
                    "signer_url": "http://localhost:2",
                    "timeout_seconds": adapter_timeout,
                    "xcrun": str(shim),
                }
                (root / "ios.json").write_text(json.dumps(config))
                (root / "driver.json").write_text(
                    json.dumps(
                        {
                            "host_command": [
                                sys.executable,
                                str(BENCH / "hosts/ios.py"),
                                str(root / "ios.json"),
                            ]
                        }
                    )
                )
                request = {
                    "target": "swift",
                    "phase": "setup",
                    "side": "new",
                    "package_sha256": "package",
                    "state_directory": str(state),
                    "fixture_sha256": digest(fixture),
                    "fixture": str(state / "fixture.json"),
                }
                settings = {
                    "new": {
                        "command": [
                            sys.executable,
                            str(BENCH / "hosts/driver.py"),
                            str(root / "driver.json"),
                        ]
                    },
                    "timeout_seconds": 2,
                }
                try:
                    with self.assertRaisesRegex(ValueError, "Adapter timeout"):
                        runner.invoke(settings, "new", request, root / "outer")
                    pid = int((root / "launched-pid").read_text())
                    self.assertFalse(
                        running(pid), "Separate app process survived the outer timeout"
                    )
                    self.assertFalse(marker.exists())
                    cleanups = list(state.glob("ios-operations/*/runner-cleanup.json"))
                    self.assertEqual(
                        len(cleanups), 1, "Outer cleanup receipt was not saved"
                    )
                    cleanup = json.loads(cleanups[0].read_text())
                    self.assertEqual(cleanup["request_sha256"], digest(request))
                    self.assertEqual(cleanup["returncode"], 0)
                    self.assertEqual(cleanup["reason"], "outer-runner-timeout")
                    self.assertFalse(ios_cleanup.registration_path(request).exists())
                    parents = (root / "terminations").read_text().splitlines()
                    self.assertEqual(len(parents), 2 if entered_cleanup else 1)
                    self.assertEqual(int(parents[-1]), os.getpid())
                finally:
                    if marker.exists():
                        try:
                            os.kill(int(marker.read_text()), signal.SIGKILL)
                        except ProcessLookupError:
                            pass

    def test_exited_launcher_does_not_skip_registered_app_cleanup(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            request = {
                "state_directory": str(root),
                "target": "swift",
                "phase": "setup",
            }
            ios_cleanup.register(request, ["simctl-double", "terminate"], root)
            process = Mock(pid=123)
            process.communicate.side_effect = [
                subprocess.TimeoutExpired(["launcher"], 1),
                ("", ""),
            ]
            config = {"new": {"command": ["launcher"]}, "timeout_seconds": 1}
            with (
                patch.object(runner.subprocess, "Popen", return_value=process),
                patch.object(runner.os, "killpg", side_effect=ProcessLookupError),
                patch.object(
                    ios_cleanup.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 0, "", ""),
                ),
            ):
                try:
                    with self.assertRaisesRegex(ValueError, "Adapter timeout"):
                        runner.invoke(config, "new", request, root / "outer")
                except ProcessLookupError:
                    self.fail("Launcher exit race skipped registered app cleanup")
            self.assertFalse(ios_cleanup.registration_path(request).exists())
            self.assertEqual(
                json.loads((root / "runner-cleanup.json").read_text())["returncode"], 0
            )

    def test_outer_cleanup_failure_keeps_registration_and_records_error(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            request = {"state_directory": str(root), "target": "swift"}
            ios_cleanup.register(request, ["simctl-double", "terminate"], root)
            error = subprocess.TimeoutExpired(
                ["simctl-double", "terminate"], ios_cleanup.CLEANUP_SECONDS
            )
            with patch.object(ios_cleanup.subprocess, "run", side_effect=error):
                with self.assertRaises(subprocess.TimeoutExpired):
                    ios_cleanup.terminate_registered(request)
            self.assertTrue(ios_cleanup.registration_path(request).exists())
            self.assertTrue(
                (root / "runner-cleanup.json").exists(),
                "Cleanup failure receipt was not saved",
            )
            self.assertIn(
                "timed out",
                json.loads((root / "runner-cleanup.json").read_text())["error"],
            )


if __name__ == "__main__":
    unittest.main()
