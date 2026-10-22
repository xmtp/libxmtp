#!/usr/bin/env python3
"""Summarize complete backend benchmark JSONL runs as CSV for schema comparisons.

Operators pass a raw run path; summarize() validates the configured workload
matrix, explicit skips, and trial coverage before reporting trial medians.
Only fixed workload/profile names and finite numeric cells reach CSV output.
Format 2 validates the paired index/query matrix and its plans, while legacy
format-1 artifacts retain their original CSV columns and byte representation.
An explicit format-2 unpartitioned cohort can be validated independently after
interruption; this never claims whole-run completion or edits the raw artifact.
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


def _valid_measurement(measure):
    """Every observation must retain finite raw samples matching its positive request count."""
    return (
        isinstance(measure, dict)
        and type(measure.get("requests")) is int
        and measure["requests"] > 0
        and isinstance(measure.get("latency_ms"), list)
        and measure["requests"] == len(measure["latency_ms"])
        and all(_number(sample) for sample in measure["latency_ms"])
        and all(_number(measure.get(field)) for field in _STATISTICS)
    )


def _valid_plans(plans):
    """Both prepared policies need nonempty diagnostic lines, not missing or mislabeled plans."""
    return (
        isinstance(plans, dict)
        and set(plans) == {"force_custom_plan", "force_generic_plan"}
        and all(
            isinstance(lines, list)
            and lines
            and all(isinstance(line, str) and line for line in lines)
            for lines in plans.values()
        )
    )


def summarize(path, *, cohort=None):
    """Validate a whole run or explicit (rows, profile) cohort before reporting statistics."""
    events = [json.loads(line) for line in Path(path).read_text().splitlines()]
    if (
        not events
        or events[0].get("event") != "environment"
        or events[0].get("format_version") not in (1, 2)
        or (cohort is None and events[-1].get("event") != "complete")
    ):
        raise ValueError("expected a complete supported benchmark run")
    paired = events[0]["format_version"] == 2
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
    profiles = ["baseline"]
    strategies = ["lateral"]
    if paired:
        profiles = config.get("index_profiles")
        strategies = config.get("read_strategies")
        write_sizes = config.get("write_sizes")
        if (
            profiles not in (["baseline"], ["baseline", "metadata_covering"])
            or not isinstance(strategies, list)
            or not strategies
            or any(
                type(value) is not str
                or value not in ("lateral", "watermarks", "bounded")
                for value in strategies
            )
            or len(set(strategies)) != len(strategies)
            or not isinstance(write_sizes, list)
            or any(
                type(value) is not int or value not in config["sizes"]
                for value in write_sizes
            )
            or len(set(write_sizes)) != len(write_sizes)
            or "measure_writes" in config
            or config.get("layout") not in ("unpartitioned", "sequence_ranges")
            or (config["layout"] == "sequence_ranges" and profiles != ["baseline"])
        ):
            raise ValueError("invalid paired experiment configuration")
    else:
        if type(config.get("measure_writes", True)) is not bool:
            raise ValueError("invalid write-measurement selector")
        write_sizes = config["sizes"] if config.get("measure_writes", True) else []
    if cohort is not None:
        if (
            not paired
            or config["layout"] != "unpartitioned"
            or len(cohort) != 2
            or type(cohort[0]) is not int
            or cohort[0] not in config["sizes"]
            or cohort[1] not in profiles
        ):
            raise ValueError(
                "cohort must name a configured format-2 unpartitioned size and index profile"
            )
        size, profile = cohort
        config = {**config, "sizes": [size]}
        write_sizes = [size] if size in write_sizes else []
        profiles = [profile]
        events = [
            event
            for event in events
            if event.get("rows") == size and event.get("index_profile") == profile
        ]
    _validate_rollovers(events, config, bool(write_sizes))
    corpora = [event for event in events if event["event"] == "corpus"]
    if (
        len(corpora) != len(config["sizes"]) * len(profiles)
        or any(
            type(corpus.get(field)) is not int or corpus[field] <= 0
            for corpus in corpora
            for field in ("rows", "topics")
        )
        or (
            paired
            and any(type(corpus.get("index_profile")) is not str for corpus in corpora)
        )
    ):
        raise ValueError("missing, duplicate, or invalid corpus metadata")
    if {
        (corpus["rows"], corpus["index_profile"] if paired else "baseline")
        for corpus in corpora
    } != {(size, profile) for size in config["sizes"] for profile in profiles} or any(
        len({corpus["topics"] for corpus in corpora if corpus["rows"] == size}) != 1
        for size in config["sizes"]
    ):
        raise ValueError("missing, duplicate, or inconsistent profile corpora")
    read_keys = {
        (size, case, topics, 0, 0, *((profile, strategy) if paired else ()))
        for size in config["sizes"]
        for case in (*_READ_CASES, *(("query_rare_hits",) if paired else ()))
        for topics in config["topic_counts"]
        for profile in profiles
        for strategy in strategies
    }
    write_keys = {
        (
            corpus["rows"],
            case,
            topics,
            writers,
            batch,
            *((corpus["index_profile"], "publish") if paired else ()),
        )
        for corpus in corpora
        for case, topics in (
            ("insert_single_topic", 1),
            ("insert_many_topics", corpus["topics"]),
        )
        for writers in config["writers"]
        for batch in config["batch_sizes"]
        if corpus["rows"] in write_sizes
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
        if paired:
            if any(
                type(event.get(field)) is not str
                for field in ("index_profile", "strategy")
            ):
                raise ValueError("invalid experiment identity")
            key += (event["index_profile"], event["strategy"])
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
    if paired:
        _validate_plans(events, read_keys - skipped)
        _validate_post_writes(events, write_keys, config["trials"])
    results = []
    for key, trials in sorted(groups.items()):
        if any(type(trial["trial"]) is not int for trial in trials) or sorted(
            trial["trial"] for trial in trials
        ) != list(range(config["trials"])):
            raise ValueError(f"incomplete or duplicate trials for {key}")
        measurements = [trial["measurement"] for trial in trials]
        if any(not _valid_measurement(measure) for measure in measurements):
            raise ValueError(f"invalid request samples for {key}")
        fields = ("rows", "case", "topics", "writers", "batch_size")
        if paired:
            fields += ("index_profile", "strategy")
        result = dict(zip(fields, key, strict=True))
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


def _validate_plans(events, expected):
    """Every paired read needs one actual custom/generic diagnostic, not a duplicate or omitted profile."""
    seen = set()
    for event in events:
        if event.get("event") != "plan":
            continue
        key = (
            event.get("rows"),
            event.get("case"),
            event.get("topics"),
            0,
            0,
            event.get("index_profile"),
            event.get("strategy"),
        )
        plans = event.get("plans")
        if (
            any(type(key[index]) is not int for index in (0, 2))
            or any(type(key[index]) is not str for index in (1, 5, 6))
            or key not in expected
            or key in seen
            or not _valid_plans(plans)
        ):
            raise ValueError("invalid or duplicate paired plan")
        seen.add(key)
    if seen != expected:
        raise ValueError("missing paired plans")


def _validate_post_writes(events, write_keys, trials):
    """Every write copy needs a matched, unvacuumed newest control so index-only gains are not presented only at ideal visibility."""
    expected = {(*key, trial) for key in write_keys for trial in range(trials)}
    seen = set()
    for event in events:
        if event.get("event") != "post_write_newest":
            continue
        key = tuple(
            event.get(field)
            for field in (
                "rows",
                "case",
                "topics",
                "writers",
                "batch_size",
                "index_profile",
            )
        )
        key += ("publish", event.get("trial"))
        read = event.get("read")
        if (
            any(type(key[index]) is not int for index in (0, 2, 3, 4, 7))
            or any(type(key[index]) is not str for index in (1, 5))
            or key not in expected
            or key in seen
            or not isinstance(read, dict)
            or type(read.get("topics")) is not int
            or not 1 <= read["topics"] <= 1000
            or not _valid_measurement(read.get("measurement"))
            or not _valid_plans(read.get("plans"))
        ):
            raise ValueError("invalid or duplicate post-write newest control")
        seen.add(key)
    if seen != expected:
        raise ValueError("missing post-write newest controls")


def _validate_rollovers(events, config, writes_enabled):
    """Require every declared rollover trial without accepting unsafe numeric cells."""
    layout = config.get("layout", "unpartitioned")
    if layout not in (
        "unpartitioned",
        "unpartitioned_watermarks",
        "sequence_ranges",
        "sequence_ranges_watermarks",
    ):
        raise ValueError("unknown benchmark layout")
    expected = (
        {
            (edge, precreated, writers, trial)
            for edge in (1_000_001, 10_000_001)
            for precreated in (False, True)
            for writers in config["writers"]
            for trial in range(config["trials"])
        }
        if layout in ("sequence_ranges", "sequence_ranges_watermarks")
        and writes_enabled
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
    parser.add_argument(
        "--cohort",
        nargs=2,
        metavar=("ROWS", "INDEX_PROFILE"),
        help="validate only this complete format-2 unpartitioned cohort, even if the larger run was interrupted",
    )
    args = parser.parse_args()
    cohort = (int(args.cohort[0]), args.cohort[1]) if args.cohort else None
    rows = summarize(args.run, cohort=cohort)
    if cohort is not None:
        print(
            f"Validated cohort only: {cohort}; no whole-run completion claimed",
            file=sys.stderr,
        )
    writer = csv.DictWriter(sys.stdout, fieldnames=rows[0].keys(), lineterminator="\n")
    writer.writeheader()
    writer.writerows(rows)


if __name__ == "__main__":
    main()
