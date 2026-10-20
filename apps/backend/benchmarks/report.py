#!/usr/bin/env python3
"""Summarize complete backend benchmark JSONL runs as CSV for schema comparisons.

Operators pass a raw run path; summarize() validates completion and trial coverage,
then reports medians across trials, never averages of pooled tail percentiles.
"""

import argparse
import csv
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path


def summarize(path):
    """Reject partial or duplicate trials and retain the full workload identity."""
    events = [json.loads(line) for line in Path(path).read_text().splitlines()]
    if (
        not events
        or events[0].get("event") != "environment"
        or events[0].get("format_version") != 1
        or events[-1].get("event") != "complete"
    ):
        raise ValueError("expected a complete format-version-1 benchmark run")
    config = events[0]["config"]
    groups = defaultdict(list)
    for event in events:
        if event["event"] in ("read", "write"):
            key = tuple(
                event.get(field, 0)
                for field in ("rows", "case", "topics", "writers", "batch_size")
            )
            groups[key].append(event)
    if not groups or {key[0] for key in groups} != set(config["sizes"]):
        raise ValueError("missing database size measurements")
    results = []
    for key, trials in sorted(groups.items()):
        if sorted(trial["trial"] for trial in trials) != list(range(config["trials"])):
            raise ValueError(f"incomplete or duplicate trials for {key}")
        measurements = [trial["measurement"] for trial in trials]
        if any(
            not measure["requests"] or measure["requests"] != len(measure["latency_ms"])
            for measure in measurements
        ):
            raise ValueError(f"invalid request samples for {key}")
        result = dict(
            zip(("rows", "case", "topics", "writers", "batch_size"), key, strict=True)
        )
        result.update(
            trials=len(trials),
            requests=sum(measure["requests"] for measure in measurements),
        )
        for field in (
            "requests_per_second",
            "envelopes_per_second",
            "p50_ms",
            "p95_ms",
            "p99_ms",
        ):
            result[field] = statistics.median(
                measure[field] for measure in measurements
            )
        result["min_requests_per_second"] = min(
            measure["requests_per_second"] for measure in measurements
        )
        result["max_requests_per_second"] = max(
            measure["requests_per_second"] for measure in measurements
        )
        results.append(result)
    return results


def main():
    """Write CSV to stdout; failures are nonzero and never emit a partial table."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    args = parser.parse_args()
    rows = summarize(args.run)
    writer = csv.DictWriter(sys.stdout, fieldnames=rows[0].keys())
    writer.writeheader()
    writer.writerows(rows)


if __name__ == "__main__":
    main()
