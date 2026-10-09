"""Check that incomplete or slow device results cannot satisfy the gate."""

import copy
import unittest
from run import validate


def result():
    return {"api": 34, "abi": "x86_64", "cores": 4, "hardware": "ranchu",
            "memoryKb": 4_000_000, "groups": 1000, "messages": "100000",
            "heavyMessages": 50000, "warmups": 5, "measuredRuns": 30,
            "firstMs": [100.0] * 30, "olderMs": [100.0] * 30,
            "listMs": [100.0] * 30, "heapDeltaBytes": 1024,
            "maxTranscriptRows": 500, "maxCacheRows": 1500, "maxPageRows": 500}


class PerformanceGateTest(unittest.TestCase):
    def test_complete_fixed_result_passes(self):
        validate(result())

    def test_rejects_other_device_and_short_workload(self):
        for key, value in (("api", 35), ("abi", "arm64-v8a"), ("cores", 8),
                           ("hardware", "physical"), ("memoryKb", 2_000_000),
                           ("groups", 999), ("messages", "99999"),
                           ("heavyMessages", 49999), ("warmups", 4), ("measuredRuns", 29)):
            with self.subTest(key=key), self.assertRaises(ValueError):
                report = result()
                report[key] = value
                validate(report)

    def test_uses_nearest_rank_p95(self):
        report = result()
        report["firstMs"] = [300.0] * 29 + [1000.0]
        validate(report)
        report["firstMs"][-2] = 301.0
        with self.assertRaisesRegex(ValueError, "firstMs"):
            validate(report)

    def test_rejects_missing_short_invalid_or_slow_measurements(self):
        for metric, limit in (("firstMs", 300), ("olderMs", 250), ("listMs", 1000)):
            for samples in ([], [1.0] * 29, [float("nan")] * 30,
                            [-1.0] * 30, [limit + 1.0] * 30):
                with self.subTest(metric=metric, samples=samples), self.assertRaises(ValueError):
                    report = result()
                    report[metric] = samples
                    validate(report)

    def test_rejects_removed_row_or_cache_bound_and_heap_overflow(self):
        for metric, value in (("maxTranscriptRows", 501), ("maxCacheRows", 1501), ("maxPageRows", 501),
                              ("heapDeltaBytes", 64 * 1024 * 1024 + 1)):
            with self.subTest(metric=metric), self.assertRaisesRegex(ValueError, metric):
                report = copy.deepcopy(result())
                report[metric] = value
                validate(report)


if __name__ == "__main__":
    unittest.main()
