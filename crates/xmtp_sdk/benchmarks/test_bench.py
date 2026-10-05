"""Check the memory sampler, the sample checks, run integrity and iOS cleanup."""

import argparse
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
import ios_host  # noqa: E402 - bench adds hosts/ to the path
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


class SampleChecks(unittest.TestCase):
    def test_percentile_interpolates(self):
        self.assertEqual(bench.percentile([4, 1, 3, 2], 0.5), 2.5)
        self.assertAlmostEqual(bench.percentile(range(1, 101), 0.95), 95.05)

    def test_page_order_and_stream_count_are_checked(self):
        fixture = bench.dataset()
        rows = list(fixture["messages"])
        good = {"duration_ms": 1, "peak_memory_bytes": 1, "observed_messages": rows}
        bench.check_measurement(dict(good), "page", fixture, "node")
        rows[0], rows[1] = rows[1], rows[0]
        with self.assertRaises(bench.BenchError):
            bench.check_measurement(dict(good), "page", fixture, "node")
        short = {"duration_ms": 1, "peak_memory_bytes": 1, "streamed_events": 1}
        with self.assertRaises(bench.BenchError):
            bench.check_measurement(short, "stream", fixture, "node")


class RunIntegrity(unittest.TestCase):
    """A run fails when the package or a runner source changes during it."""

    def run_changing(self, change):
        fixture = bench.dataset()
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            sources, package = temp / "sources", temp / "package"
            sources.mkdir()
            package.mkdir()
            (sources / "workload.mjs").write_text("runner")
            (package / "entry.js").write_text("package")
            subprocess.run(["git", "init", "-q", str(sources)], check=True)
            files = {
                "sources": sources / "workload.mjs",
                "package": package / "entry.js",
            }

            def call(request, log):
                if request["phase"] != "measure":
                    return {"ready": True}
                if change and request["workload"] == "stream":
                    files[change].write_text("changed")
                return {
                    "duration_ms": 1,
                    "peak_memory_bytes": 1,
                    "observed_messages": fixture["messages"],
                    "streamed_events": bench.stream_events(fixture),
                }

            args = argparse.Namespace(
                output=str(temp / "out"), samples=1, keep_state=False
            )
            with (
                patch.object(bench, "HERE", sources),
                patch.object(bench, "open_host", return_value=(call, package)),
                patch("sys.stdout"),
                patch("sys.stderr"),
            ):
                bench.run("node", args)
            return json.loads((temp / "out/results.json").read_text())

    def test_unchanged_run_writes_results(self):
        self.assertEqual(self.run_changing(None)["package"]["files"], 1)

    def test_changed_package_or_source_fails(self):
        for change in ("package", "sources"):
            with self.subTest(change), self.assertRaises(bench.BenchError):
                self.run_changing(change)


class IosCleanup(unittest.TestCase):
    def test_timeout_terminates_the_app(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            (temp / "state").mkdir()
            (temp / "state/fixture.json").write_text("{}")
            calls = []

            def simctl(argv, **kwargs):
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


if __name__ == "__main__":
    unittest.main()
