"""Controlled regressions test gate behavior, not real SDK performance."""

import copy
import tempfile
import sys
import unittest
from pathlib import Path

from benchmark_stats import paired_summary
from fixtures import dataset, digest, expected_observation, expected_stream_counts
from packages import inventory

sys.path.insert(0, str(Path(__file__).parent / "hosts"))
from driver import record_observation
from runner import (
    SAFETY,
    WORKLOADS,
    summarize,
    validate_config,
    validate_measurement,
    validate_mobile,
)


def configuration(target="node"):
    package = {
        "version": "control",
        "commit": "control",
        "compiler": "control",
        "production_flags": "control",
        "provenance": "synthetic control",
        "profile": "release",
        "public_api": True,
        "command": ["control"],
        "adapter_sources": [__file__],
        "published": True,
        "integrated_head": True,
    }
    return {
        "schema": 1,
        "target": target,
        "purpose": "harness-control",
        "pairs": 20,
        "timeout_seconds": 10,
        "runner_class": "control",
        "os": "control",
        "hardware": "control",
        "runtime": "control",
        "clean_build_policy": "control",
        "warm_build_policy": "control",
        "memory_scope": "control",
        "cache_policy": "control",
        "enrichment": dataset()["enrichment"],
        "old": dict(package),
        "new": dict(package),
    }


def ledger(target="node", factor=1):
    fixture = dataset(target)
    rows = []
    for workload in WORKLOADS:
        for pair in range(20):
            for side in ("old", "new") if pair % 2 == 0 else ("new", "old"):
                value = 100 * (factor if side == "new" else 1)
                response = {
                    "observation": expected_observation(fixture, workload),
                    "duration_ms": value,
                    "peak_memory_bytes": value * 1000,
                    "safety": {key: key == "correctness" for key in SAFETY},
                    "long_tasks_ms": [],
                    "mobile_lift": {
                        "record_ms": 100,
                        "class_ms": value,
                        "order": ["record", "class"]
                        if pair % 2 == 0
                        else ["class", "record"],
                    },
                }
                if workload == "stream" and target == "browser":
                    primary, events = expected_stream_counts(fixture)
                    response["streamed_primary"] = primary
                    response["streamed_events"] = events
                rows.append(
                    {
                        "workload": workload,
                        "pair": pair,
                        "side": side,
                        "response": response,
                    }
                )
    return {
        "config": configuration(target),
        "fixture_sha256": digest(fixture),
        "samples": rows,
        "mobile_samples": [
            {
                "workload": "mobile_lift",
                "pair": i,
                "side": "new",
                "response": {
                    "mobile_lift": {
                        "record_ms": 100,
                        "class_ms": 100 * factor,
                        "order": ["record", "class"]
                        if i % 2 == 0
                        else ["class", "record"],
                    }
                },
            }
            for i in range(20)
        ]
        if target in {"swift", "kotlin"}
        else [],
        "packages": {
            side: {"raw_bytes": 1000, "compressed_bytes": 500}
            for side in ("old", "new")
        },
    }


class StatisticsTests(unittest.TestCase):
    def test_latency_memory_and_build_regressions(self):
        old = [100 + i for i in range(20)]
        for kind in ("latency", "memory", "build"):
            with self.subTest(kind=kind):
                self.assertEqual(
                    paired_summary(old, [v * 1.3 for v in old], kind)["decision"],
                    "FAIL",
                )
                self.assertEqual(
                    paired_summary(old, [v * 1.19 for v in old], kind)["decision"],
                    "RECORDED",
                )

    def test_throughput_direction(self):
        self.assertEqual(
            paired_summary([100] * 20, [79] * 20, "throughput")["decision"], "FAIL"
        )
        self.assertEqual(
            paired_summary([100] * 20, [81] * 20, "throughput")["decision"], "RECORDED"
        )

    def test_exact_threshold_is_not_failure(self):
        for kind in ("latency", "memory", "build", "mobile"):
            self.assertEqual(
                paired_summary([100] * 20, [120] * 20, kind)["decision"], "RECORDED"
            )
        self.assertEqual(
            paired_summary([100] * 20, [80] * 20, "throughput")["decision"], "RECORDED"
        )

    def test_noise_and_faster_control(self):
        noisy = [60] * 8 + [130] * 12
        result = paired_summary([100] * 20, noisy, "latency")
        self.assertGreater(result["ratio"], 1.2)
        self.assertLess(result["ratio_ci95"][0], 1)
        self.assertEqual(result["decision"], "RECORDED")
        self.assertEqual(
            paired_summary([100] * 20, [70] * 20, "latency")["decision"], "RECORDED"
        )

    def test_bootstrap_keeps_pairs(self):
        old = [10 ** (i / 5) for i in range(20)]
        result = paired_summary(old, [v * 1.3 for v in old], "latency")
        for bound in result["ratio_ci95"]:
            self.assertAlmostEqual(bound, 1.3)
        self.assertEqual(result["decision"], "FAIL")

    def test_mobile_uses_median_gate_even_with_wide_interval(self):
        result = paired_summary([100] * 20, [60] * 8 + [130] * 12, "mobile")
        self.assertLess(result["ratio_ci95"][0], 1)
        self.assertEqual(result["decision"], "FAIL")

    def test_reject_partial_invalid_samples(self):
        for old, new in [
            ([1] * 19, [1] * 19),
            ([1] * 20, [1] * 21),
            ([1] * 20, [float("nan")] * 20),
            ([0] * 20, [1] * 20),
        ]:
            with self.assertRaises(ValueError):
                paired_summary(old, new, "latency")


class ReceiptTests(unittest.TestCase):
    def test_browser_stream_keeps_500_primary_and_625_events(self):
        fixture = dataset("browser")
        self.assertEqual(expected_stream_counts(fixture), (500, 625))
        self.assertEqual(expected_observation(fixture, "stream")["count"], 500)
        self.assertEqual(expected_stream_counts(dataset("node")), (10000, 12500))
        row = next(row for row in ledger("browser")["samples"] if row["workload"] == "stream")
        validate_measurement(row, fixture, "browser")
        for key in ("streamed_primary", "streamed_events"):
            missing = copy.deepcopy(row)
            missing["response"][key] -= 1
            with self.assertRaises(ValueError):
                validate_measurement(missing, fixture, "browser")

    def test_missing_duplicate_or_reordered_pairs_fail(self):
        source = ledger()
        for mutation in (
            lambda rows: rows.pop(),
            lambda rows: rows.append(rows[0]),
            lambda rows: rows.reverse(),
        ):
            value = copy.deepcopy(source)
            mutation(value["samples"])
            with self.assertRaises(ValueError):
                summarize(value)

    def test_debug_or_unequal_enrichment_fails(self):
        for path, value in [("profile", "debug"), ("public_api", False)]:
            config = configuration()
            config["new"][path] = value
            with self.assertRaises(ValueError):
                validate_config(config)
        config = configuration()
        config["enrichment"] = ["decoded_content"]
        with self.assertRaises(ValueError):
            validate_config(config)

    def test_wrong_observation_and_slow_callback_fail(self):
        fixture = dataset()
        row = ledger()["samples"][0]
        row["response"]["observation"]["count"] = 0
        with self.assertRaises(ValueError):
            validate_measurement(row, fixture, "node")
        row["workload"] = "callback_slow"
        row["response"]["observation"] = expected_observation(fixture, "callback_slow")
        row["response"]["duration_ms"] = 1
        with self.assertRaises(ValueError):
            validate_measurement(row, fixture, "node")

    def test_size_and_safety_gates_remain_separate(self):
        value = ledger()
        value["packages"]["new"]["raw_bytes"] = 1201
        value["samples"][0]["response"]["safety"]["use_after_end"] = True
        report = summarize(value)
        self.assertEqual(report["performance_decision"], "FAIL")
        self.assertEqual(len(report["safety_failures"]), 1)
        self.assertEqual(report["results"][-2]["decision"], "FAIL")
        self.assertEqual(report["release_gate"], "PENDING")

    def test_page_cannot_replace_observed_values_with_completion(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample"
            for workload in ("page", "stream"):
                with self.assertRaises(ValueError):
                    record_observation({"completed": True}, dataset(), workload, path)
            response = {
                "observed_messages": list(reversed(dataset()["messages"][:1000]))
            }
            record_observation(response, dataset(), "page", path)
            self.assertNotEqual(
                response["observation"], expected_observation(dataset(), "page")
            )
            self.assertNotIn("observed_messages", response)

    def test_mobile_conversion_has_separate_ordered_samples(self):
        value = ledger("swift")
        value["mobile_samples"].pop()
        with self.assertRaises(ValueError):
            summarize(value)
        row = ledger("kotlin")["mobile_samples"][0]
        row["response"]["mobile_lift"]["order"] = ["class", "record"]
        with self.assertRaises(ValueError):
            validate_mobile(row)

    def test_complete_inventory_catches_native_growth(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("index.js", "native.node", "runtime.js"):
                (root / name).write_bytes(b"a" * 100)
            assets = {
                "public": ["index.js"],
                "native": ["native.node"],
                "runtime": ["runtime.js"],
            }
            before = inventory(root, assets, "node")
            (root / "native.node").write_bytes(b"b" * 1000)
            after = inventory(root, assets, "node")
            self.assertEqual(before["raw_bytes"], 300)
            self.assertEqual(after["raw_bytes"], 1200)
            self.assertNotEqual(before["sha256"], after["sha256"])
            self.assertEqual(after, inventory(root, assets, "node"))
            (root / "dependency").symlink_to(root / "native.node")
            with self.assertRaises(ValueError):
                inventory(root, assets, "node")


if __name__ == "__main__":
    unittest.main()
