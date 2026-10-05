"""Check the memory sampler, the percentile helper and the sample checks."""

import sys
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))
import bench  # noqa: E402 - requires the path setup above
import processes  # noqa: E402 - bench adds hosts/ to the path


def child(code):
    return [sys.executable, "-c", f"import sys; sys.stdin.read(); {code}"]


class SamplerControls(unittest.TestCase):
    def test_delayed_thread_start_short_child(self):
        original = threading.Thread

        def delayed(*args, **kwargs):
            target = kwargs["target"]

            def held(*values):
                time.sleep(0.2)
                target(*values)

            return original(*args, **{**kwargs, "target": held})

        with patch.object(processes.threading, "Thread", side_effect=delayed):
            code, stdout, _, _, peak = processes.execute(child("print('short')"), "go")
        self.assertEqual((code, stdout.strip()), (0, "short"))
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


if __name__ == "__main__":
    unittest.main()
