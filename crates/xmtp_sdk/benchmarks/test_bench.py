"""Check the benchmark tool: sampler, outer timeout cleanup, and result shape."""

import json
import os
import signal
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

BENCH = Path(__file__).resolve().parent
sys.path.insert(0, str(BENCH / "hosts"))
import driver  # noqa: E402 - requires the path setup above
import ios_cleanup  # noqa: E402
import processes  # noqa: E402
import runner  # noqa: E402
from fixtures import canonical, dataset, digest, expected_stream_counts  # noqa: E402

# A simctl double. "launch" starts a separate session, like the Simulator app,
# so a process-group kill of the launcher cannot reach it. "terminate" waits
# for delay.json seconds, then kills that process.
SIMCTL_DOUBLE = """
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
    print(root / 'data')
elif action == 'launch':
    child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'],
                             start_new_session=True, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL)
    marker.write_text(str(child.pid))
    (root / 'launched-pid').write_text(str(child.pid))
"""


def running(pid):
    result = subprocess.run(
        ["ps", "-p", str(pid), "-o", "stat="], capture_output=True, text=True
    )
    return bool(result.stdout.strip()) and not result.stdout.strip().startswith("Z")


class SamplerControls(unittest.TestCase):
    def test_delayed_thread_start_short_child(self):
        original = threading.Thread

        def delayed(*args, **kwargs):
            target = kwargs["target"]

            def held():
                time.sleep(0.2)
                target()

            kwargs["target"] = held
            return original(*args, **kwargs)

        with patch.object(processes.threading, "Thread", side_effect=delayed):
            result = processes.execute(
                [sys.executable, "-c", "import sys; sys.stdin.read(); print('short')"],
                "go",
            )
        self.assertEqual(result[0], 0)
        self.assertEqual(result[1].strip(), "short")
        self.assertGreater(result[4], 0)

    def test_sampling_error_and_child_failure(self):
        with patch.object(
            processes.subprocess, "run", side_effect=OSError("ps unavailable")
        ):
            with self.assertRaises(OSError):
                processes.execute(
                    [sys.executable, "-c", "import sys; sys.stdin.read()"], "go"
                )
        code, _, _, _, peak = processes.execute(
            [sys.executable, "-c", "import sys; sys.stdin.read(); sys.exit(7)"], "go"
        )
        self.assertEqual(code, 7)
        self.assertGreater(peak, 0)


class OuterTimeoutControls(unittest.TestCase):
    def test_outer_timeout_stops_separate_app(self):
        # The outer kill lands during the iOS launcher's own cleanup, then
        # during its response wait. Both times the runner must stop the app.
        for ios_timeout, termination_delay, entered_cleanup in [
            (1.2, 3, True),
            (10, 0.2, False),
        ]:
            with (
                self.subTest(ios_timeout=ios_timeout),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                (root / "app").mkdir()
                (root / "data").mkdir()
                state = root / "state"
                state.mkdir()
                fixture = {"messages": []}
                (root / "fixture.json").write_bytes(canonical(fixture))
                marker = root / "active-pid"
                shim = root / "simctl-double"
                shim.write_text(
                    f"#!{sys.executable}\n" + textwrap.dedent(SIMCTL_DOUBLE)
                )
                shim.chmod(0o755)
                (root / "delay.json").write_text(json.dumps(termination_delay))
                ios_config = {
                    "app_path": str(root / "app"),
                    "bundle_id": "org.xmtp.benchmark",
                    "simulator_udid": "explicit-udid",
                    "backend_url": "http://localhost:1",
                    "signer_url": "http://localhost:2",
                    "timeout_seconds": ios_timeout,
                    "xcrun": str(shim),
                }
                (root / "ios.json").write_text(json.dumps(ios_config))
                host = [
                    sys.executable,
                    str(BENCH / "hosts/ios.py"),
                    str(root / "ios.json"),
                ]
                (root / "driver.json").write_text(json.dumps({"host_command": host}))
                request = {
                    "target": "swift",
                    "phase": "setup",
                    "package_sha256": "package",
                    "state_directory": str(state),
                    "fixture_sha256": digest(fixture),
                    "fixture": str(root / "fixture.json"),
                }
                command = [
                    sys.executable,
                    str(BENCH / "hosts/driver.py"),
                    str(root / "driver.json"),
                ]
                config = {"package": {"command": command}, "timeout_seconds": 2}
                try:
                    with self.assertRaisesRegex(ValueError, "Adapter timeout"):
                        runner.invoke(config, request, root / "outer")
                    pid = int((root / "launched-pid").read_text())
                    self.assertFalse(
                        running(pid), "Separate app process survived the outer timeout"
                    )
                    self.assertFalse(marker.exists())
                    records = list(state.glob("ios-operations/*/runner-cleanup.json"))
                    self.assertEqual(len(records), 1, "Outer cleanup was not recorded")
                    self.assertEqual(
                        json.loads(records[0].read_text())["returncode"], 0
                    )
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


def stream_response(fixture, **changes):
    primary, events = expected_stream_counts(fixture)
    response = {
        "completed": True,
        "streamed_primary": primary,
        "streamed_events": events,
        "duration_ms": 10.0,
        "peak_memory_bytes": 1,
        "safety": {key: None for key in runner.SAFETY},
        "long_tasks_ms": [],
    }
    response.update(changes)
    return response


class StreamResultControls(unittest.TestCase):
    def test_stream_records_counts_without_an_observation(self):
        response = stream_response(dataset("node"))
        driver.record_observation(response, "stream", Path("unused"))
        self.assertNotIn("observation", response)
        self.assertIs(response["completed"], True)
        for change, error in [
            ({"completed": False}, "did not complete"),
            ({"streamed_events": None}, "delivered-ID counts"),
            ({"observed_messages": []}, "Only page"),
        ]:
            with self.subTest(change=change):
                with self.assertRaisesRegex(ValueError, error):
                    driver.record_observation(
                        stream_response(dataset("node"), **change),
                        "stream",
                        Path("unused"),
                    )

    def test_stream_counts_are_checked_on_every_target(self):
        for target in runner.TARGETS:
            fixture = dataset(target)
            primary, events = expected_stream_counts(fixture)
            with self.subTest(target=target):
                row = {"workload": "stream", "response": stream_response(fixture)}
                runner.validate_measurement(row, fixture, target)
                for change, error in [
                    ({"streamed_events": events - 1}, "delivered-ID counts"),
                    ({"streamed_primary": primary + 1}, "delivered-ID counts"),
                    ({"completed": False}, "did not complete"),
                    ({"observation": {"count": primary}}, "Only page"),
                ]:
                    row = {
                        "workload": "stream",
                        "response": stream_response(fixture, **change),
                    }
                    with self.assertRaisesRegex(ValueError, error):
                        runner.validate_measurement(row, fixture, target)


if __name__ == "__main__":
    unittest.main()
