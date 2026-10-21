#!/usr/bin/env python3
"""Summarize complete backend benchmark JSONL runs as CSV for schema comparisons.

Operators pass a raw run path; summarize() validates the configured workload
matrix, explicit skips, and trial coverage before reporting trial medians.
Only the fixed format-version-1 workload names and finite numeric cells reach
CSV output, which uses LF line endings for reproducible versioned summaries.
Partition experiments also require complete first-use/control rollover trials;
those sparse-ID diagnostics remain in JSONL, not steady-state CSV rows.
"""

import argparse
import csv
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path

_READ_CASES = (
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
_STATISTICS = (
    "requests_per_second",
    "envelopes_per_second",
    "p50_ms",
    "p95_ms",
    "p99_ms",
)


def _number(value):
    """Accept finite nonnegative numbers, not booleans or numeric strings."""
    return (
        type(value) in (int, float)
        and value >= 0
        and (type(value) is int or math.isfinite(value))
    )


def summarize(path):
    """Reject incomplete matrices and unsafe cells before any CSV is emitted."""
    events = [json.loads(line) for line in Path(path).read_text().splitlines()]
    if (
        not events
        or events[0].get("event") != "environment"
        or events[0].get("format_version") != 1
        or events[-1].get("event") != "complete"
    ):
        raise ValueError("expected a complete format-version-1 benchmark run")
    config = events[0]["config"]
    for field in ("sizes", "topic_counts", "writers", "batch_sizes"):
        values = config.get(field)
        if (
            not isinstance(values, list)
            or not values
            or any(type(value) is not int or value <= 0 for value in values)
            or len(set(values)) != len(values)
        ):
            raise ValueError(f"invalid benchmark dimension: {field}")
    if type(config["trials"]) is not int or config["trials"] <= 0:
        raise ValueError("invalid trial count")
    _validate_rollovers(events, config)
    corpora = [event for event in events if event["event"] == "corpus"]
    if (
        len(corpora) != len(config["sizes"])
        or any(
            type(corpus.get(field)) is not int or corpus[field] <= 0
            for corpus in corpora
            for field in ("rows", "topics")
        )
        or {corpus["rows"] for corpus in corpora} != set(config["sizes"])
    ):
        raise ValueError("missing, duplicate, or invalid corpus metadata")
    read_keys = {
        (size, case, topics, 0, 0)
        for size in config["sizes"]
        for case in _READ_CASES
        for topics in config["topic_counts"]
    }
    write_keys = {
        (corpus["rows"], case, topics, writers, batch)
        for corpus in corpora
        for case, topics in (
            ("insert_single_topic", 1),
            ("insert_many_topics", corpus["topics"]),
        )
        for writers in config["writers"]
        for batch in config["batch_sizes"]
    }
    expected = read_keys | write_keys
    groups = defaultdict(list)
    skipped = set()
    for event in events:
        kind = event["event"]
        if kind not in ("read", "write", "skipped"):
            continue
        key = (
            event["rows"],
            event["case"],
            event["topics"],
            event["writers"] if kind == "write" else 0,
            event["batch_size"] if kind == "write" else 0,
        )
        if (
            type(key[1]) is not str
            or any(type(key[index]) is not int for index in (0, 2, 3, 4))
            or key not in (write_keys if kind == "write" else read_keys)
        ):
            raise ValueError("unexpected workload identity")
        if kind == "skipped":
            if (
                key not in read_keys
                or key in skipped
                or not isinstance(event.get("reason"), str)
                or not event["reason"].strip()
            ):
                raise ValueError("invalid or duplicate skipped workload")
            skipped.add(key)
        else:
            groups[key].append(event)
    if set(groups) != expected - skipped:
        raise ValueError("missing or unexpected workload measurements")
    results = []
    for key, trials in sorted(groups.items()):
        if any(type(trial["trial"]) is not int for trial in trials) or sorted(
            trial["trial"] for trial in trials
        ) != list(range(config["trials"])):
            raise ValueError(f"incomplete or duplicate trials for {key}")
        measurements = [trial["measurement"] for trial in trials]
        if any(
            type(measure["requests"]) is not int
            or measure["requests"] <= 0
            or not isinstance(measure["latency_ms"], list)
            or measure["requests"] != len(measure["latency_ms"])
            or any(not _number(sample) for sample in measure["latency_ms"])
            or any(not _number(measure[field]) for field in _STATISTICS)
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
        for field in _STATISTICS:
            result[field] = statistics.median(
                measure[field] for measure in measurements
            )
            if not _number(result[field]):
                raise ValueError(f"invalid summary statistic for {key}")
        result["min_requests_per_second"] = min(
            measure["requests_per_second"] for measure in measurements
        )
        result["max_requests_per_second"] = max(
            measure["requests_per_second"] for measure in measurements
        )
        results.append(result)
    return results


def _validate_rollovers(events, config):
    """Require every declared rollover trial without accepting unsafe numeric cells."""
    layout = config.get("layout", "unpartitioned")
    if layout not in ("unpartitioned", "sequence_ranges"):
        raise ValueError("unknown benchmark layout")
    expected = (
        {
            (edge, precreated, writers, trial)
            for edge in (1_000_001, 10_000_001)
            for precreated in (False, True)
            for writers in config["writers"]
            for trial in range(config["trials"])
        }
        if layout == "sequence_ranges"
        else set()
    )
    seen = set()
    for event in events:
        if event.get("event") != "rollover":
            continue
        key = tuple(
            event[field] for field in ("edge", "precreated", "writers", "trial")
        )
        if (
            any(type(key[index]) is not int for index in (0, 2, 3))
            or type(key[1]) is not bool
            or key not in expected
            or key in seen
            or type(event["seed_rows"]) is not int
            or event["seed_rows"] != 1000
            or type(event["wal_bytes"]) is not int
            or event["wal_bytes"] < 0
        ):
            raise ValueError("invalid or duplicate rollover trial")
        measure = event["measurement"]
        if (
            type(measure["requests"]) is not int
            or measure["requests"] != key[2]
            or not isinstance(measure["latency_ms"], list)
            or len(measure["latency_ms"]) != key[2]
            or any(not _number(sample) for sample in measure["latency_ms"])
            or any(not _number(measure[field]) for field in _STATISTICS)
        ):
            raise ValueError("invalid rollover measurements")
        seen.add(key)
    if seen != expected:
        raise ValueError("missing rollover trials")


def main():
    """Write CSV to stdout; failures are nonzero and never emit a partial table."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    args = parser.parse_args()
    rows = summarize(args.run)
    writer = csv.DictWriter(sys.stdout, fieldnames=rows[0].keys(), lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)


if __name__ == "__main__":
    main()
