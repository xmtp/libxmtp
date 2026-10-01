#!/usr/bin/env python3
"""Prove each gate test detects a weaker implementation in a temporary copy."""

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

MUTATIONS = [
    (
        "ReceiptTests.test_page_cannot_replace_observed_values_with_completion",
        "hosts/driver.py",
        'if workload in {"page", "stream"} and "observed_messages" not in response:',
        "if False:",
    ),
    (
        "StatisticsTests.test_latency_memory_and_build_regressions",
        "benchmark_stats.py",
        "failed = low > LIMIT",
        "failed = False",
    ),
    (
        "StatisticsTests.test_throughput_direction",
        "benchmark_stats.py",
        "failed = high < THROUGHPUT_LIMIT",
        "failed = False",
    ),
    (
        "StatisticsTests.test_exact_threshold_is_not_failure",
        "benchmark_stats.py",
        "failed = low > LIMIT",
        "failed = low >= LIMIT",
    ),
    (
        "StatisticsTests.test_noise_and_faster_control",
        "benchmark_stats.py",
        "failed = low > LIMIT",
        "failed = ratio > LIMIT",
    ),
    (
        "StatisticsTests.test_bootstrap_keeps_pairs",
        "benchmark_stats.py",
        "statistics.median(old[i] for i in indices)",
        "statistics.median(rng.choices(old, k=count))",
    ),
    (
        "StatisticsTests.test_mobile_uses_median_gate_even_with_wide_interval",
        "benchmark_stats.py",
        "failed = ratio > LIMIT",
        "failed = low > LIMIT",
    ),
    (
        "StatisticsTests.test_reject_partial_invalid_samples",
        "benchmark_stats.py",
        "len(old) < MIN_PAIRS",
        "len(old) < 1",
    ),
    (
        "ReceiptTests.test_missing_duplicate_or_reordered_pairs_fail",
        "runner.py",
        "if actual != expected:",
        "if False:",
    ),
    (
        "ReceiptTests.test_debug_or_unequal_enrichment_fails",
        "runner.py",
        'if package.get("profile") != "release" or package.get("public_api") is not True:',
        "if False:",
    ),
    (
        "ReceiptTests.test_wrong_observation_and_slow_callback_fail",
        "runner.py",
        'if response.get("observation") != expected_observation(fixture, row["workload"]):',
        "if False:",
    ),
    (
        "ReceiptTests.test_size_and_safety_gates_remain_separate",
        "runner.py",
        '"decision": "FAIL" if new / old > LIMIT else "RECORDED"',
        '"decision": "RECORDED"',
    ),
    (
        "ReceiptTests.test_mobile_conversion_has_separate_ordered_samples",
        "runner.py",
        'if mobile.get("order") != order:',
        "if False:",
    ),
    (
        "ReceiptTests.test_complete_inventory_catches_native_growth",
        "packages.py",
        '"raw_bytes": sum(row["bytes"] for row in rows)',
        '"raw_bytes": len(rows) * 100',
    ),
]


def main():
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = Path(__file__).parent
    for test, name, before, after in MUTATIONS:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copytree(source / "hosts", root / "hosts")
            for file in source.glob("*.py"):
                shutil.copy2(file, root / file.name)
            file = root / name
            content = file.read_text()
            if before not in content:
                raise ValueError(f"Mutation no longer applies: {test}")
            file.write_text(content.replace(before, after))
            result = subprocess.run(
                [sys.executable, "-m", "unittest", "test_gate." + test],
                cwd=root,
                capture_output=True,
                text=True,
            )
            (output / (test + ".log")).write_text(result.stdout + result.stderr)
            if result.returncode == 0 or "FAIL:" not in result.stderr:
                raise ValueError(
                    f"Mutation did not produce an assertion failure: {test}"
                )
            print(f"RED {test}")
    result = subprocess.run(
        [sys.executable, "-m", "unittest", "test_gate"],
        cwd=source,
        capture_output=True,
        text=True,
    )
    (output / "restored-green.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise ValueError("Restored tests failed")
    print("GREEN restored implementation")


if __name__ == "__main__":
    main()
