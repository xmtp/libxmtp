"""Requirements for trustworthy benchmark summaries, using synthetic observations."""

import json
import tempfile
import unittest
from pathlib import Path

from report import summarize


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "run.jsonl"
        self.events = [
            {
                "event": "environment",
                "format_version": 1,
                "config": {"sizes": [1000], "trials": 2},
            },
            *[
                {
                    "event": "read",
                    "rows": 1000,
                    "case": "query_recent",
                    "topics": 10,
                    "trial": trial,
                    "measurement": {
                        "requests": 2,
                        "latency_ms": [1, 3],
                        "requests_per_second": 100 + trial * 100,
                        "envelopes_per_second": 1000,
                        "p50_ms": 1,
                        "p95_ms": 3,
                        "p99_ms": 3,
                    },
                }
                for trial in range(2)
            ],
            {"event": "complete"},
        ]

    def summarize(self):
        self.path.write_text("\n".join(json.dumps(event) for event in self.events))
        return summarize(self.path)

    def test_medians_and_trial_ranges_are_not_pooled_percentiles(self):
        """Repeated trials retain sample counts, median rates, and the range of measured throughput."""
        (row,) = self.summarize()
        self.assertEqual(row["requests_per_second"], 150)
        self.assertEqual(row["min_requests_per_second"], 100)
        self.assertEqual(row["max_requests_per_second"], 200)
        self.assertEqual(row["requests"], 4)

    def test_partial_runs_are_not_reported_as_baselines(self):
        """A killed experiment must not produce an apparently complete baseline."""
        self.events.pop()
        with self.assertRaises(ValueError):
            self.summarize()

    def test_missing_and_duplicate_trials_are_rejected(self):
        """Unequal trial coverage must not bias comparisons between schemas."""
        self.events[2]["trial"] = 0
        with self.assertRaises(ValueError):
            self.summarize()

    def test_missing_sizes_and_invalid_sample_counts_are_rejected(self):
        """Every requested size needs measurements, and reported counts must match raw samples."""
        self.events[0]["config"]["sizes"].append(100000)
        with self.assertRaises(ValueError):
            self.summarize()
        self.events[0]["config"]["sizes"].pop()
        self.events[1]["measurement"]["requests"] = 3
        with self.assertRaises(ValueError):
            self.summarize()


if __name__ == "__main__":
    unittest.main()
