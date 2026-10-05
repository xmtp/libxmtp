"""Check the benchmark tool: sampler, outer timeout cleanup, result shape, and
run integrity."""

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
    SHORT_CHILD = [sys.executable, "-c", "import sys; sys.stdin.read(); print('short')"]

    @staticmethod
    def delayed_sampler(*args, **kwargs):
        # Replaces only the sampler module's Thread, so no other thread waits.
        target = kwargs["target"]

        def held():
            time.sleep(0.2)
            target()

        kwargs["target"] = held
        return threading.Thread(*args, **kwargs)

    def test_delayed_thread_start_short_child(self):
        with patch.object(processes, "Thread", side_effect=self.delayed_sampler):
            result = processes.execute(self.SHORT_CHILD, "go")
        self.assertEqual(result[0], 0)
        self.assertEqual(result[1].strip(), "short")
        self.assertGreater(result[4], 0)

    def test_zero_first_sample_is_sampled_again(self):
        # A process that has just started can report zero RSS. With the
        # sampler delayed past the child's exit, only the first samples count.
        real = processes.process_table
        calls = []

        def first_reading_zero():
            rows = real()
            calls.append(len(rows))
            if len(calls) == 1:
                return [(pid, parent, 0) for pid, parent, _ in rows]
            return rows

        with (
            patch.object(processes, "Thread", side_effect=self.delayed_sampler),
            patch.object(processes, "process_table", side_effect=first_reading_zero),
        ):
            result = processes.execute(self.SHORT_CHILD, "go")
        self.assertEqual(result[0], 0)
        self.assertGreater(result[4], 0)
        self.assertGreaterEqual(len(calls), 2)

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


# A host adapter double. It answers each request and, during "measure",
# appends to the file named in its second argument, unless that is "none".
ADAPTER_DOUBLE = """
import json, sys
sys.path.insert(0, sys.argv[1])
from fixtures import digest
request = json.loads(sys.stdin.read())
response = {"request_sha256": digest(request), "ready": True}
if request["phase"] == "measure":
    if sys.argv[2] != "none":
        with open(sys.argv[2], "a") as changed:
            changed.write("changed")
    response.update(
        completed=True,
        duration_ms=1.0,
        peak_memory_bytes=1,
        safety={"correctness": True, "deadlock": False,
                "use_after_end": False, "retained_growth": False},
    )
print(json.dumps(response))
"""


class RunIntegrityControls(unittest.TestCase):
    def run_with_change(self, root, changed):
        package = root / "package"
        package.mkdir()
        for name in ("index.js", "native.node", "runtime.js"):
            (package / name).write_text(name)
        adapter = root / "adapter.py"
        adapter.write_text(textwrap.dedent(ADAPTER_DOUBLE))
        targets = {"package": package / "native.node", "adapter": adapter}
        config = {
            "schema": 1,
            "target": "node",
            "samples": 1,
            "timeout_seconds": 30,
            **{
                key: "test"
                for key in (
                    "runner_class",
                    "os",
                    "hardware",
                    "runtime",
                    "clean_build_policy",
                    "warm_build_policy",
                    "memory_scope",
                    "cache_policy",
                )
            },
            "package": {
                "version": "0",
                "commit": "0",
                "compiler": "test",
                "production_flags": "test",
                "profile": "release",
                "public_api": True,
                "root": str(package),
                "assets": {
                    "public": ["index.js"],
                    "native": ["native.node"],
                    "runtime": ["runtime.js"],
                },
                "adapter_sources": [str(adapter)],
                "command": [
                    sys.executable,
                    str(adapter),
                    str(BENCH),
                    str(targets[changed]) if changed else "none",
                ],
            },
        }
        (root / "config.json").write_text(json.dumps(config))
        with patch.object(runner, "WORKLOADS", ("cold_start",)):
            return runner.run(root / "config.json", root / "out")

    def test_unchanged_run_writes_the_report(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.assertEqual(self.run_with_change(root, None), 0)
            self.assertTrue((root / "out/report.json").exists())

    def test_changed_package_or_adapter_fails_the_run(self):
        for changed, error in [
            ("package", "Installed package changed"),
            ("adapter", "Adapter source changed"),
        ]:
            with (
                self.subTest(changed=changed),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                with self.assertRaisesRegex(ValueError, error):
                    self.run_with_change(root, changed)
                self.assertFalse((root / "out/report.json").exists())
                self.assertIn(
                    error, json.loads((root / "out/error.json").read_text())["error"]
                )


if __name__ == "__main__":
    unittest.main()
