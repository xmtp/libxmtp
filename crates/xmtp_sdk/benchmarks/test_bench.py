"""Check the memory sampler, the sample checks, run integrity and cleanup."""

import argparse
import io
import json
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import bench  # noqa: E402 - requires the path setup above
import android_host  # noqa: E402 - bench adds hosts/ to the path
import ios_host  # noqa: E402
import processes  # noqa: E402


def child(code):
    return [sys.executable, "-c", f"import sys; sys.stdin.read(); {code}"]


class SamplerControls(unittest.TestCase):
    def test_delayed_thread_start_short_child(self):
        def delayed(*args, **kwargs):
            # Only the sampler thread of the processes module waits.
            target = kwargs["target"]

            def held(*values):
                if target.__name__ == "sample":
                    time.sleep(0.2)
                target(*values)

            return threading.Thread(*args, **{**kwargs, "target": held})

        with patch.object(processes, "Thread", side_effect=delayed):
            code, stdout, _, _, peak = processes.execute(child("print('short')"), "go")
        self.assertEqual((code, stdout.strip()), (0, "short"))
        self.assertGreater(peak, 0)

    def test_zero_readings_keep_the_child_high_water(self):
        # A process that has just started can report zero RSS.
        table = SimpleNamespace(stdout=f"{1 << 30} 1 0\n")
        with patch.object(processes.subprocess, "run", return_value=table):
            code, _, _, _, peak = processes.execute(child("pass"), "go")
        self.assertEqual(code, 0)
        self.assertGreater(peak, 0)

    def test_sampling_error_and_child_failure(self):
        with patch.object(processes.subprocess, "run", side_effect=OSError("no ps")):
            with self.assertRaises(OSError):
                processes.execute(child("pass"), "go")
        code, _, _, _, peak = processes.execute(child("sys.exit(7)"), "go")
        self.assertEqual(code, 7)
        self.assertGreater(peak, 0)

    def test_descendant_holding_the_pipes_times_out(self):
        # The child exits at once. Its descendant keeps stdout open for 5 s.
        descendant = [sys.executable, "-c", "import time; time.sleep(5)"]
        start = time.monotonic()
        with self.assertRaises(TimeoutError):
            processes.execute(
                child(f"import subprocess; subprocess.Popen({descendant!r})"),
                "go",
                timeout=1,
            )
        self.assertLess(time.monotonic() - start, 4)

    def test_zero_timeout_still_kills(self):
        # Zero is a timeout, not "no timeout". Only None turns the timer off.
        start = time.monotonic()
        with self.assertRaises(TimeoutError):
            processes.execute(child("import time; time.sleep(5)"), "go", timeout=0)
        self.assertLess(time.monotonic() - start, 4)


class CommandLine(unittest.TestCase):
    def test_non_positive_timeout_is_rejected(self):
        for value in ("0", "-1", "nan", "inf"):
            argv = ["bench.py", "node", "--timeout", value]
            with (
                self.subTest(value),
                patch.object(sys, "argv", argv),
                patch.object(bench, "run") as run,
                patch("sys.stderr"),
            ):
                with self.assertRaises(SystemExit):
                    bench.main()
                run.assert_not_called()

    def main_with(self, url, *options):
        """Run bench.main with a backend URL. Returns (run mock, stderr, error)."""
        argv = ["bench.py", "node", "--samples", "10", *options]
        with (
            patch.object(sys, "argv", argv),
            patch.dict("os.environ", {"XMTP_BACKEND_URL": url}),
            patch.object(bench, "run") as run,
            patch("sys.stderr", new_callable=io.StringIO) as stderr,
        ):
            try:
                bench.main()
            except bench.BenchError as error:
                return run, stderr.getvalue(), error
        return run, stderr.getvalue(), None

    def test_loopback_backend_is_unaffected(self):
        for url in (
            "http://localhost:5050",
            "http://127.0.0.1:5050",
            "http://[::1]:5050",
        ):
            with self.subTest(url):
                run, stderr, error = self.main_with(url)
                self.assertIsNone(error)
                self.assertEqual(run.call_args.args[1].samples, 10)
                self.assertEqual(stderr, "")

    def test_remote_backend_is_rejected_by_default(self):
        for url in ("https://grpc.example.com:443", "http://10.0.0.5:5050"):
            with self.subTest(url):
                run, _, error = self.main_with(url)
                self.assertIn("--allow-remote-backend", str(error))
                run.assert_not_called()

    def test_remote_backend_is_accepted_with_the_flag(self):
        run, stderr, error = self.main_with(
            "https://grpc.example.com:443", "--allow-remote-backend"
        )
        self.assertIsNone(error)
        run.assert_called_once()
        self.assertIn("Warning", stderr)
        self.assertIn("grpc.example.com", stderr)

    def test_remote_backend_caps_the_samples(self):
        url, flag = "https://grpc.example.com:443", "--allow-remote-backend"
        run, _, _ = self.main_with(url, flag)
        self.assertEqual(run.call_args.args[1].samples, bench.MAX_REMOTE_SAMPLES)
        # A smaller sample count stays as it is.
        run, _, _ = self.main_with(url, flag, "--samples", "1")
        self.assertEqual(run.call_args.args[1].samples, 1)


class SampleChecks(unittest.TestCase):
    def test_percentile_interpolates(self):
        self.assertEqual(bench.percentile([4, 1, 3, 2], 0.5), 2.5)
        self.assertAlmostEqual(bench.percentile(range(1, 101), 0.95), 95.05)

    def test_messages_per_second_counts_primary_messages_only(self):
        events = bench.stream_events(bench.dataset())
        self.assertGreater(events, bench.ROWS)
        samples = [
            {"workload": workload, "duration_ms": 2000, "peak_memory_bytes": 1}
            for workload in bench.WORKLOADS
        ]
        for sample in samples:
            sample["streamed_events"] = events
        rates = {
            row["metric"]: row["p50"]
            for row in bench.summarize("node", samples)
            if row["workload"] == "stream"
        }
        self.assertEqual(rates["messages_per_second"], bench.ROWS / 2)
        self.assertEqual(rates["events_per_second"], events / 2)

    def test_page_order_and_stream_count_are_checked(self):
        fixture = bench.dataset()
        rows = list(fixture["messages"])
        good = {"duration_ms": 1, "peak_memory_bytes": 1, "observed_messages": rows}
        bench.check_measurement(dict(good), "page", fixture, "node")
        rows[0], rows[1] = rows[1], rows[0]
        with self.assertRaises(bench.BenchError):
            bench.check_measurement(dict(good), "page", fixture, "node")
        count = bench.stream_events(fixture)
        stream = {"duration_ms": 1, "peak_memory_bytes": 1, "streamed_events": count}
        bench.check_measurement(dict(stream), "stream", fixture, "node")
        wrong = {
            "wrong count": {"streamed_events": 1},
            "no completion count": {"streamed_events": None},
            "content, not a count": {"observed_messages": fixture["messages"]},
        }
        for name, change in wrong.items():
            with self.subTest(name), self.assertRaises(bench.BenchError):
                bench.check_measurement({**stream, **change}, "stream", fixture, "node")

    def test_numbers_must_be_finite_and_positive(self):
        # json.loads gives True for true and NaN or inf for NaN and Infinity.
        sample = {"duration_ms": 1.5, "peak_memory_bytes": 1, "long_tasks_ms": []}
        fixture = bench.dataset()
        bench.check_measurement(dict(sample), "cold_start", fixture, "browser")
        bench.check_measurement(
            {**sample, "long_tasks_ms": [60.5]}, "cold_start", fixture, "browser"
        )
        bad = (True, float("nan"), float("inf"), 0, -1.5, "1", None)
        for field in ("duration_ms", "peak_memory_bytes", "long_tasks_ms"):
            for value in bad:
                change = {field: [value] if field == "long_tasks_ms" else value}
                with (
                    self.subTest(field=field, value=value),
                    self.assertRaises(bench.BenchError),
                ):
                    bench.check_measurement(
                        {**sample, **change}, "cold_start", fixture, "browser"
                    )


def run_bench(temp, call, remove=None, keep_state=False, output="out"):
    """Run bench.run on a fake host in temp. Returns the output directory.

    output is relative to temp. The sources are untracked in a new git
    repository, so the digest sees them as a runner source in progress.
    """
    temp = temp.resolve()
    sources, package = temp / "sources", temp / "package"
    sources.mkdir()
    package.mkdir()
    (sources / "workload.mjs").write_text("runner")
    (package / "entry.js").write_text("package")
    subprocess.run(["git", "init", "-q", str(sources)], check=True)
    args = argparse.Namespace(
        output=str(temp / output), samples=1, keep_state=keep_state
    )
    with (
        patch.object(bench, "HERE", sources),
        patch.object(bench, "open_host", return_value=(call, package, remove)),
        patch("sys.stdout"),
        patch("sys.stderr"),
    ):
        bench.run("node", args)
    return temp / output


def host_call(fixture, measured=lambda request: None):
    """A host that answers every request correctly."""

    def call(request, log):
        if request["phase"] != "measure":
            return {"ready": True}
        measured(request)
        result = {"duration_ms": 1, "peak_memory_bytes": 1}
        if request["workload"] == "page":
            result["observed_messages"] = fixture["messages"]
        if request["workload"] == "stream":
            result["streamed_events"] = bench.stream_events(fixture)
        return result

    return call


class RunIntegrity(unittest.TestCase):
    """A run fails when the package or a runner source changes during it."""

    def run_changing(self, change, output="out"):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            files = {
                "sources": temp / "sources/workload.mjs",
                "package": temp / "package/entry.js",
            }

            def measured(request):
                # A host writes its logs in the output directory, as js_host
                # does, during the run.
                logs = Path(request["state_directory"]).parent / "logs"
                (logs / f"{request['workload']}.stderr").write_text("log")
                if change and request["workload"] == "stream":
                    files[change].write_text("changed")

            out = run_bench(temp, host_call(bench.dataset(), measured), output=output)
            return json.loads((out / "results.json").read_text())

    def test_unchanged_run_writes_results(self):
        self.assertEqual(self.run_changing(None)["package"]["files"], 1)

    def test_changed_package_or_source_fails(self):
        for change in ("package", "sources"):
            with self.subTest(change), self.assertRaises(bench.BenchError):
                self.run_changing(change)

    def test_output_inside_the_sources_is_not_a_source(self):
        # --output can be a new directory in the benchmarks directory.
        inside = "sources/out"
        self.assertEqual(self.run_changing(None, inside)["package"]["files"], 1)
        with self.assertRaisesRegex(bench.BenchError, "source changed"):
            self.run_changing("sources", inside)


class StateCleanup(unittest.TestCase):
    """Without --keep-state a run deletes its databases, also on the device."""

    def test_run_removes_device_state_unless_kept(self):
        for keep in (False, True):
            with self.subTest(keep=keep), tempfile.TemporaryDirectory() as temp:
                removed = []
                out = run_bench(
                    Path(temp),
                    host_call(bench.dataset()),
                    # The device cleanup runs while out/state still exists.
                    lambda: removed.append((Path(temp) / "out/state").exists()),
                    keep_state=keep,
                )
                self.assertEqual(removed, [] if keep else [True])
                self.assertEqual((out / "state").exists(), keep)
                self.assertTrue((out / "results.json").exists())

    def test_failed_run_still_removes_device_state(self):
        def fail(request):
            raise bench.BenchError("host failed")

        with tempfile.TemporaryDirectory() as temp:
            removed = []
            with self.assertRaisesRegex(bench.BenchError, "host failed"):
                run_bench(
                    Path(temp),
                    host_call(bench.dataset(), fail),
                    lambda: removed.append(True),
                )
            self.assertEqual(removed, [True])
            self.assertFalse((Path(temp) / "out/state").exists())

    def test_device_cleanup_failure_keeps_the_results(self):
        def broken():
            raise RuntimeError("no device")

        with tempfile.TemporaryDirectory() as temp:
            out = run_bench(Path(temp), host_call(bench.dataset()), broken)
            self.assertTrue((out / "results.json").exists())


class IosCleanup(unittest.TestCase):
    def test_timeout_terminates_the_app(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            (temp / "state").mkdir()
            (temp / "state/fixture.json").write_text("{}")
            calls = []

            def simctl(argv, timeout, **kwargs):
                # A real terminate can take a moment. A call without a
                # cleanup budget is killed, as subprocess.run does.
                if argv[2] == "terminate" and timeout < 0.5:
                    raise subprocess.TimeoutExpired(argv, timeout)
                calls.append(argv[2])
                stdout = (
                    f"{temp / 'container'}\n" if argv[2] == "get_app_container" else ""
                )
                return SimpleNamespace(returncode=0, stdout=stdout, stderr="")

            config = {
                "simulator_udid": "udid",
                "backend_url": "http://backend",
                "signer_url": "http://signer",
                "timeout_seconds": 0.2,
            }
            request = {"state_directory": str(temp / "state")}
            # The app never writes a response.
            with patch.object(ios_host.subprocess, "run", side_effect=simctl):
                with self.assertRaises(TimeoutError):
                    ios_host.invoke(config, request, temp / "call")
        self.assertEqual(calls[-2:], ["launch", "terminate"])

    def call_with_failing_terminate(self, answer, failure):
        """One call whose app answers `answer` and whose final terminate fails.

        The terminate before the launch succeeds. Returns the call result and
        the recorded simctl commands.
        """
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            container = temp / "container"
            (temp / "state").mkdir()
            (temp / "state/fixture.json").write_text("{}")
            launched = []

            def simctl(argv, timeout, **kwargs):
                stdout = ""
                if argv[2] == "terminate" and launched:
                    if failure == "timeout":
                        raise subprocess.TimeoutExpired(argv, timeout)
                    if failure == "start":
                        raise FileNotFoundError("xcrun")
                    return SimpleNamespace(returncode=1, stdout="", stderr="busy")
                if argv[2] == "get_app_container":
                    stdout = f"{container}\n"
                if argv[2] == "launch":
                    launched.append(argv)
                    envelope = container / argv[-1]
                    operation = json.loads(envelope.read_text())["operation_id"]
                    response = {"operation_id": operation, **answer}
                    (envelope.parent / "response.json").write_text(json.dumps(response))
                return SimpleNamespace(returncode=0, stdout=stdout, stderr="")

            config = {
                "simulator_udid": "udid",
                "backend_url": "http://backend",
                "signer_url": "http://signer",
                "timeout_seconds": 5,
            }
            request = {"state_directory": str(temp / "state"), "phase": "measure"}
            try:
                with patch.object(ios_host.subprocess, "run", side_effect=simctl):
                    return ios_host.invoke(config, request, temp / "call")
            finally:
                self.commands = json.loads((temp / "call.simctl.json").read_text())

    def test_failed_terminate_keeps_the_sample(self):
        for failure in ("timeout", "start", "exit"):
            with self.subTest(failure):
                sample = {"duration_ms": 12.5}
                result = self.call_with_failing_terminate({"result": sample}, failure)
                self.assertEqual(result, sample)
                self.assertIn("cleanup_error", self.commands[-1])

    def test_failed_terminate_keeps_the_call_error(self):
        for failure in ("timeout", "start", "exit"):
            with self.subTest(failure):
                with self.assertRaisesRegex(RuntimeError, "iOS app failed: no group"):
                    self.call_with_failing_terminate({"error": "no group"}, failure)

    def test_remove_state_deletes_the_run_databases_only(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            container = temp / "container"
            (temp / "state").mkdir()
            (temp / "state/fixture.json").write_text("{}")
            other = container / ios_host.STATE / "other-run"
            other.mkdir(parents=True)

            def simctl(argv, timeout, **kwargs):
                stdout = ""
                if argv[2] == "get_app_container":
                    stdout = f"{container}\n"
                if argv[2] == "launch":
                    # The app answers in the transport directory.
                    envelope = container / argv[-1]
                    operation = json.loads(envelope.read_text())["operation_id"]
                    response = {"operation_id": operation, "result": {"ready": True}}
                    (envelope.parent / "response.json").write_text(json.dumps(response))
                return SimpleNamespace(returncode=0, stdout=stdout, stderr="")

            config = {
                "simulator_udid": "udid",
                "backend_url": "http://backend",
                "signer_url": "http://signer",
                "timeout_seconds": 5,
            }
            # bench sends the state directory as a string and removes it by path.
            request = {"state_directory": str(temp / "state"), "phase": "setup"}
            with patch.object(ios_host.subprocess, "run", side_effect=simctl):
                ios_host.invoke(config, request, temp / "setup")
                self.assertEqual(len(list(other.parent.iterdir())), 2)
                # The call deletes its request and response directory.
                transport = container / ios_host.TRANSPORT
                self.assertEqual(list(transport.iterdir()), [])
                ios_host.remove_state(config, temp / "state")
            self.assertEqual(list(other.parent.iterdir()), [other])


class AndroidHost(unittest.TestCase):
    FORCE_STOP = ["shell", "am", "force-stop", android_host.PACKAGE]

    def invoke(self, instrument):
        """Run one request against a fake adb. Records the adb commands."""
        self.calls = calls = []

        def adb(argv, **kwargs):
            command = argv[3:]
            calls.append(command)
            if command[:3] == ["shell", "am", "instrument"]:
                return instrument(argv, kwargs["timeout"])
            if command == self.FORCE_STOP:
                # Cleanup failures must not hide the result of the request.
                raise subprocess.TimeoutExpired(argv, kwargs["timeout"])
            return SimpleNamespace(returncode=0, stdout="{}", stderr="")

        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            (temp / "state").mkdir()
            (temp / "state/fixture.json").write_text("{}")
            config = {"serial": "device", "timeout_seconds": 0.2, "host": {}}
            request = {"state_directory": str(temp / "state")}
            with patch.object(android_host.subprocess, "run", side_effect=adb):
                android_host.invoke(config, request, temp / "call")

    def test_instrumentation_timeout_force_stops_the_app(self):
        def hang(argv, timeout):
            raise subprocess.TimeoutExpired(argv, timeout)

        with self.assertRaises(subprocess.TimeoutExpired) as raised:
            self.invoke(hang)
        self.assertIn("instrument", raised.exception.cmd)
        self.assertEqual(self.calls[-1], self.FORCE_STOP)

    def test_every_instrumentation_force_stops_the_app(self):
        def complete(argv, timeout):
            stdout = "benchmark=complete\nINSTRUMENTATION_CODE: 0\n"
            return SimpleNamespace(returncode=0, stdout=stdout, stderr="")

        self.invoke(complete)
        names = [command[:3] for command in self.calls]
        stop = names.index(self.FORCE_STOP[:3])
        self.assertEqual(names[stop - 1], ["shell", "am", "instrument"])

    def test_remove_state_asks_the_app_to_delete_the_run_databases(self):
        requests, timeouts = [], []

        def adb(argv, **kwargs):
            command = argv[3:]
            if command[0] == "push" and command[2].endswith("/request.json"):
                requests.append(json.loads(Path(command[1]).read_text()))
            if command[:3] == ["shell", "am", "instrument"]:
                timeouts.append(kwargs["timeout"])
                stdout = "benchmark=complete\nINSTRUMENTATION_CODE: 0\n"
                return SimpleNamespace(returncode=0, stdout=stdout, stderr="")
            return SimpleNamespace(returncode=0, stdout="{}", stderr="")

        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            (temp / "state").mkdir()
            (temp / "state/fixture.json").write_text("{}")
            config = {"serial": "device", "timeout_seconds": 900, "host": {}}
            # bench sends the state directory as a string and removes it by path.
            request = {"state_directory": str(temp / "state"), "phase": "setup"}
            with patch.object(android_host.subprocess, "run", side_effect=adb):
                android_host.invoke(config, request, temp / "setup")
                android_host.remove_state(config, temp / "state", temp / "cleanup")
        setup, cleanup = requests
        self.assertEqual(cleanup["phase"], "cleanup")
        self.assertEqual(cleanup["state_key"], setup["state_key"])
        self.assertEqual(timeouts, [900, android_host.REMOVE_STATE_SECONDS])

    def test_input_directory_keeps_the_setgid_group(self):
        def complete(argv, timeout):
            stdout = "benchmark=complete\nINSTRUMENTATION_CODE: 0\n"
            return SimpleNamespace(returncode=0, stdout=stdout, stderr="")

        self.invoke(complete)
        modes = [c[2] for c in self.calls if c[:2] == ["shell", "chmod"]]
        self.assertEqual(len(modes), 1)
        # The app writes response.json as its own uid with mode 0660. Without
        # setgid the file has the app's group and `adb shell cat` is denied.
        self.assertTrue(int(modes[0], 8) & 0o2000, modes[0])
        self.assertEqual(int(modes[0], 8) & 0o777, 0o777, modes[0])

    def test_backend_without_a_port_uses_the_scheme_default(self):
        cases = {
            "http://localhost:5050": [5050],
            "http://127.0.0.1": [80],
            "https://[::1]": [443],
            "https://grpc.example.com": [],
        }
        for url, ports in cases.items():
            with self.subTest(url):
                self.assertEqual(android_host.backend_ports(url), ports)
        with self.assertRaises(ValueError):
            android_host.backend_ports("grpc://localhost")

    def test_stuck_reverse_removal_does_not_block_the_run(self):
        removed = []

        def adb(argv, **kwargs):
            if "--remove" in argv:
                removed.append((argv[-1], kwargs.get("timeout")))
                # A stuck adb never returns. subprocess.run kills it after
                # its timeout; without a timeout the run would wait forever.
                if kwargs.get("timeout") is None:
                    raise AssertionError(f"{argv} has no deadline and would hang")
                raise subprocess.TimeoutExpired(argv, kwargs["timeout"])
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with patch.object(android_host.subprocess, "run", side_effect=adb):
            with self.assertRaisesRegex(ValueError, "request failed"):
                with android_host.reverse({"serial": "device"}, [5050, 5555]):
                    raise ValueError("request failed")
        # Every port is removed, each within the cleanup deadline.
        deadline = android_host.CLEANUP_SECONDS
        self.assertEqual(removed, [("tcp:5050", deadline), ("tcp:5555", deadline)])

    def test_failed_reverse_removes_the_earlier_mappings(self):
        commands = []

        def adb(argv, **kwargs):
            command = argv[3:]
            commands.append(command)
            if command == ["reverse", "tcp:5555", "tcp:5555"]:
                raise subprocess.CalledProcessError(1, argv)
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with patch.object(android_host.subprocess, "run", side_effect=adb):
            with self.assertRaises(subprocess.CalledProcessError):
                with android_host.reverse({"serial": "device"}, [5050, 5555]):
                    self.fail("the body must not run without every mapping")
        # Only the port that was mapped is removed.
        self.assertEqual(
            commands,
            [
                ["reverse", "tcp:5050", "tcp:5050"],
                ["reverse", "tcp:5555", "tcp:5555"],
                ["reverse", "--remove", "tcp:5050"],
            ],
        )

    def test_reverse_rejects_a_missing_port(self):
        with patch.object(android_host.subprocess, "run") as run:
            with self.assertRaises(ValueError):
                with android_host.reverse({"serial": "device"}, [5050, None]):
                    pass
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
