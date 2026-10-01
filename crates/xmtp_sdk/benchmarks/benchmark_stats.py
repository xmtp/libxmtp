"""Paired release statistics. No SDK measurement is made in this module."""

import math
import random
import statistics

LIMIT = 1.20
THROUGHPUT_LIMIT = 0.80
MIN_PAIRS = 20
BOOTSTRAPS = 10000


def percentile(values, fraction):
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    left = math.floor(position)
    right = math.ceil(position)
    return ordered[left] + (ordered[right] - ordered[left]) * (position - left)


def paired_summary(old, new, kind, seed=731):
    """Resample pair indices together; estimate the ratio of paired medians."""
    if len(old) != len(new) or len(old) < MIN_PAIRS:
        raise ValueError("At least 20 complete old/new pairs are required")
    if any(not math.isfinite(v) or v <= 0 for v in old + new):
        raise ValueError("Measurements must be finite and positive")
    if kind not in {"latency", "memory", "throughput", "build", "mobile"}:
        raise ValueError("Unknown measurement kind")
    count = len(old)
    rng = random.Random(seed)
    ratios = []
    for _ in range(BOOTSTRAPS):
        indices = rng.choices(range(count), k=count)
        ratios.append(
            statistics.median(new[i] for i in indices)
            / statistics.median(old[i] for i in indices)
        )
    ratio = statistics.median(new) / statistics.median(old)
    low, high = percentile(ratios, 0.025), percentile(ratios, 0.975)
    if kind == "throughput":
        failed = high < THROUGHPUT_LIMIT
    elif kind == "mobile":
        failed = ratio > LIMIT
    else:
        failed = low > LIMIT
    return {
        "pairs": count,
        "old_p50": percentile(old, 0.5),
        "old_p95": percentile(old, 0.95),
        "new_p50": percentile(new, 0.5),
        "new_p95": percentile(new, 0.95),
        "ratio": ratio,
        "ratio_ci95": [low, high],
        "kind": kind,
        "decision": "FAIL" if failed else "RECORDED",
    }
