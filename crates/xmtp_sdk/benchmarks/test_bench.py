"""Check the process-tree memory sampler that the host driver uses."""

import sys
import threading
import time
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent / "hosts"))
import processes  # noqa: E402 - requires the path setup above


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


if __name__ == "__main__":
    unittest.main()
