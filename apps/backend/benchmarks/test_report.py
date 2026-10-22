"""Requirements for trustworthy benchmark summaries, using synthetic observations."""

import copy
import json
import subprocess
import sys
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
                "config": {
                    "sizes": [1000],
                    "topic_counts": [10],
                    "writers": [1],
                    "batch_sizes": [1],
                    "trials": 3,
                },
            },
            {"event": "corpus", "rows": 1000, "topics": 100},
            *[
                {
                    "event": "write" if writers else "read",
                    "rows": 1000,
                    "case": case,
                    "topics": topics,
                    "writers": writers,
                    "batch_size": batch,
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
                for case, topics, writers, batch in [
                    *[
                        (case, 10, 0, 0)
                        for case in (
                            "query_no_cursor",
                            "query_old",
                            "query_backlog",
                            "query_recent",
                            "query_mixed",
                            "query_sparse_recent",
                            "query_absent",
                            "query_caught_up",
                            "newest_full",
                            "newest_metadata",
                            "newest_absent",
                        )
                    ],
                    ("insert_single_topic", 1, 1, 1),
                    ("insert_many_topics", 100, 1, 1),
                ]
                for trial in range(3)
            ],
            {"event": "complete"},
        ]

    def write_run(self):
        self.path.write_text("\n".join(json.dumps(event) for event in self.events))

    def summarize(self):
        self.write_run()
        return summarize(self.path)

    def observation(self, trial=0):
        return next(
            event
            for event in self.events
            if event.get("case") == "query_recent" and event.get("trial") == trial
        )

    def partitioned(self):
        self.events[0]["config"]["layout"] = "sequence_ranges"
        self.events[-1:-1] = [
            {
                "event": "rollover",
                "edge": edge,
                "precreated": precreated,
                "writers": 1,
                "seed_rows": 1000,
                "wal_bytes": 100,
                "trial": trial,
                "measurement": dict(
                    self.observation()["measurement"], requests=1, latency_ms=[1]
                ),
            }
            for edge in (1_000_001, 10_000_001)
            for precreated in (False, True)
            for trial in range(3)
        ]

    def test_partitioned_runs_require_the_entire_rollover_matrix(self):
        """Missing first-use/control groups or duplicate trials cannot yield a complete partition report."""
        self.partitioned()
        self.assertEqual(len(self.summarize()), 13)
        original = copy.deepcopy(self.events)
        for corruption in (
            "missing_group",
            "duplicate",
            "missing_trial",
            "unknown_layout",
        ):
            with self.subTest(corruption=corruption):
                self.events = copy.deepcopy(original)
                if corruption == "missing_group":
                    self.events = [
                        event for event in self.events if event.get("edge") != 1_000_001
                    ]
                elif corruption == "missing_trial":
                    self.events.pop(-2)
                elif corruption == "duplicate":
                    self.events.insert(-1, copy.deepcopy(self.events[-2]))
                else:
                    self.events[0]["config"]["layout"] = "unknown"
                with self.assertRaises(ValueError):
                    self.summarize()

    def test_rollover_dimensions_and_measurements_are_typed_and_finite(self):
        """Sparse rollover diagnostics reject booleans-as-dimensions, formulas, and invalid samples."""
        self.partitioned()
        original = copy.deepcopy(self.events)
        for field, value in (
            ("edge", True),
            ("precreated", 0),
            ("writers", "=1"),
            ("seed_rows", 0),
            ("wal_bytes", -1),
            ("trial", 9),
        ):
            with self.subTest(field=field):
                self.events = copy.deepcopy(original)
                self.events[-2][field] = value
                with self.assertRaises(ValueError):
                    self.summarize()
        for field, value in (
            ("requests", True),
            ("p95_ms", float("inf")),
            ("latency_ms", ["=1"]),
        ):
            with self.subTest(field=field):
                self.events = copy.deepcopy(original)
                self.events[-2]["measurement"][field] = value
                with self.assertRaises(ValueError):
                    self.summarize()

    def test_read_only_runs_explicitly_exclude_writes_and_rollovers(self):
        """A focused query experiment excludes writes explicitly but still requires every read group and trial."""
        write = next(event for event in self.events if event["event"] == "write")
        self.events[0]["config"].update(
            layout="sequence_ranges_watermarks", measure_writes=False
        )
        self.events = [event for event in self.events if event["event"] != "write"]
        for layout in ("unpartitioned_watermarks", "sequence_ranges_watermarks"):
            self.events[0]["config"]["layout"] = layout
            self.assertEqual(len(self.summarize()), 11)
        original = copy.deepcopy(self.events)
        for value in (0, "false", None):
            with self.subTest(selector=value):
                self.events = copy.deepcopy(original)
                self.events[0]["config"]["measure_writes"] = value
                with self.assertRaises(ValueError):
                    self.summarize()
        self.events = copy.deepcopy(original)
        self.events.insert(-1, write)
        with self.assertRaises(ValueError):
            self.summarize()
        self.events = [
            event
            for event in copy.deepcopy(original)
            if event.get("case") != "query_recent"
        ]
        with self.assertRaises(ValueError):
            self.summarize()
        self.events = copy.deepcopy(original)
        self.events.insert(
            -1,
            {
                "event": "rollover",
                "edge": 1_000_001,
                "precreated": False,
                "writers": 1,
                "trial": 0,
                "seed_rows": 1000,
                "wal_bytes": 100,
            },
        )
        with self.assertRaises(ValueError):
            self.summarize()

    def test_medians_and_trial_ranges_are_not_pooled_percentiles(self):
        """Repeated trials retain sample counts, median rates, and the range of measured throughput."""
        row = next(row for row in self.summarize() if row["case"] == "query_recent")
        self.assertEqual(row["requests_per_second"], 200)
        self.assertEqual(row["min_requests_per_second"], 100)
        self.assertEqual(row["max_requests_per_second"], 300)
        self.assertEqual(row["requests"], 6)

    def test_partial_runs_are_not_reported_as_baselines(self):
        """A killed experiment must not produce an apparently complete baseline."""
        self.events.pop()
        with self.assertRaises(ValueError):
            self.summarize()

    def test_csv_uses_reproducible_unix_line_endings(self):
        """Versioned summaries need LF output without platform-specific whitespace changes."""
        self.summarize()
        output = subprocess.check_output(
            [sys.executable, str(Path(__file__).with_name("report.py")), str(self.path)]
        )
        self.assertNotIn(b"\r", output)
        self.assertTrue(output.endswith(b"\n"))
        self.assertEqual(len(output.splitlines()), 14)

    def test_missing_and_duplicate_trials_are_rejected(self):
        """Unequal trial coverage must not bias comparisons between schemas."""
        original = copy.deepcopy(self.events)
        for trial in (0, True, "=1+1"):
            with self.subTest(trial=trial):
                self.events = copy.deepcopy(original)
                self.observation(1)["trial"] = trial
                with self.assertRaises(ValueError):
                    self.summarize()

    def test_missing_sizes_and_invalid_sample_counts_are_rejected(self):
        """Every requested size needs measurements, and reported counts must match raw samples."""
        self.events[0]["config"]["sizes"].append(100000)
        with self.assertRaises(ValueError):
            self.summarize()
        self.events[0]["config"]["sizes"].pop()
        self.observation()["measurement"]["requests"] = 3
        with self.assertRaises(ValueError):
            self.summarize()
        self.observation()["measurement"].update(requests=True, latency_ms=[1])
        with self.assertRaises(ValueError):
            self.summarize()

    def test_entire_missing_workloads_are_rejected(self):
        """A completion marker cannot conceal removal of every trial for a read or write workload."""
        original = copy.deepcopy(self.events)
        for case in ("query_recent", "insert_many_topics"):
            with self.subTest(case=case):
                self.events = [event for event in original if event.get("case") != case]
                with self.assertRaises(ValueError):
                    self.summarize()
        self.events = copy.deepcopy(original)
        self.events[0]["config"]["topic_counts"].append(100)
        with self.assertRaises(ValueError):
            self.summarize()

    def test_explicit_skips_preserve_complete_coverage(self):
        """Infeasible reads are valid only when an explicit skip replaces their entire workload."""
        self.events = [
            event for event in self.events if event.get("case") != "query_recent"
        ]
        self.events.insert(-1, self.skipped())
        rows = self.summarize()
        self.assertEqual(len(rows), 12)
        self.assertNotIn("query_recent", {row["case"] for row in rows})

    def skipped(self):
        return {
            "event": "skipped",
            "rows": 1000,
            "case": "query_recent",
            "topics": 10,
            "reason": "not enough distinct eligible topics",
        }

    def test_conflicting_duplicate_and_unexpected_skips_are_rejected(self):
        """Skips must identify one configured read, explain infeasibility, and never overlap samples."""
        original = copy.deepcopy(self.events)
        for changes in (
            {},
            {"topics": 100},
            {"case": "insert_many_topics"},
            {"reason": ""},
        ):
            with self.subTest(changes=changes):
                self.events = copy.deepcopy(original)
                self.events.insert(-1, self.skipped() | changes)
                with self.assertRaises(ValueError):
                    self.summarize()
        self.events = [
            event for event in original if event.get("case") != "query_recent"
        ]
        self.events[-1:-1] = [self.skipped(), self.skipped()]
        with self.assertRaises(ValueError):
            self.summarize()

    def test_duplicate_and_nonnumeric_config_dimensions_are_rejected(self):
        """Dimensions are explicit nonempty sets of positive integers, never spreadsheet formulas."""
        original = copy.deepcopy(self.events)
        for field in ("sizes", "topic_counts", "writers", "batch_sizes"):
            repeated = [original[0]["config"][field][0]] * 2
            for values in ([], [0], [True], ["=1+1"], repeated):
                with self.subTest(field=field, values=values):
                    self.events = copy.deepcopy(original)
                    self.events[0]["config"][field] = values
                    with self.assertRaises(ValueError):
                        self.summarize()
        for trials in (0, True, "=1+1"):
            self.events = copy.deepcopy(original)
            self.events[0]["config"]["trials"] = trials
            with self.assertRaises(ValueError):
                self.summarize()

    def test_corpus_metadata_is_complete_unique_and_numeric(self):
        """Every configured size needs exactly one numeric topic population to validate write coverage."""
        original = copy.deepcopy(self.events)
        for replacement in (
            [],
            [original[1], original[1]],
            [original[1] | {"topics": "=1+1"}],
        ):
            with self.subTest(replacement=replacement):
                self.events = copy.deepcopy(original)
                self.events[1:2] = replacement
                with self.assertRaises(ValueError):
                    self.summarize()

    def test_formula_workload_names_never_emit_csv(self):
        """Untrusted workload labels must fail before even a CSV header can reach a spreadsheet."""
        original = copy.deepcopy(self.events)
        for formula in (
            "=1+1",
            "+1+1",
            "-1+1",
            "@SUM(1,1)",
            "\t=1+1",
            "\r=1+1",
            "unknown",
        ):
            with self.subTest(formula=formula):
                self.events = copy.deepcopy(original)
                for trial in range(3):
                    self.observation(trial)["case"] = formula
                self.write_run()
                result = subprocess.run(
                    [
                        sys.executable,
                        str(Path(__file__).with_name("report.py")),
                        str(self.path),
                    ],
                    capture_output=True,
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")

    def test_workload_identity_matches_its_kind_and_numeric_dimensions(self):
        """CSV dimensions must be typed numbers and identify a workload of the declared kind."""
        original = copy.deepcopy(self.events)
        for field in ("topics", "writers", "batch_size"):
            for value in ("=1+1", True):
                with self.subTest(field=field, value=value):
                    self.events = copy.deepcopy(original)
                    for event in self.events:
                        if event.get("case") == "insert_single_topic":
                            event[field] = value
                    with self.assertRaises(ValueError):
                        self.summarize()
        self.events = copy.deepcopy(original)
        for trial in range(3):
            self.observation(trial)["event"] = "write"
        with self.assertRaises(ValueError):
            self.summarize()

    def test_statistics_and_samples_are_finite_nonnegative_numbers(self):
        """Numeric CSV cells and raw samples must reject formulas, booleans, infinities, and negatives."""
        original = copy.deepcopy(self.events)
        for field in (
            "requests_per_second",
            "envelopes_per_second",
            "p50_ms",
            "p95_ms",
            "p99_ms",
            "latency_ms",
        ):
            for value in ("=1+1", True, -1, float("nan"), float("inf")):
                with self.subTest(field=field, value=value):
                    self.events = copy.deepcopy(original)
                    for trial in range(3):
                        self.observation(trial)["measurement"][field] = (
                            [value, 3] if field == "latency_ms" else value
                        )
                    with self.assertRaises(ValueError):
                        self.summarize()
        self.events = [event for event in original if event.get("trial") != 2]
        self.events[0]["config"]["trials"] = 2
        for trial in range(2):
            self.observation(trial)["measurement"]["p50_ms"] = 1.7e308
        with self.assertRaises(ValueError):
            self.summarize()


if __name__ == "__main__":
    unittest.main()
