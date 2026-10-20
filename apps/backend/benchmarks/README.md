# Backend database benchmarks

Manual schema-comparison suite, not a CI performance gate. The runner calls the
actual `Store` methods; production reads and diagnostic `EXPLAIN` probes share
the same SQL files. No partitioned schema is installed by this suite.

## Run

Use a **dedicated PostgreSQL 17+ instance** with no other workloads. The admin
role needs `CREATEDB` and permission to run `CHECKPOINT`. The supplied URL is
only used to create and remove uniquely named disposable databases. Existing
databases are never truncated, migrated, or dropped. Fixture cleanup also runs
on errors and panics. Allow roughly 40 GB for the largest corpus, its temporary
write-trial copy, and WAL, plus build artifacts; actual needs depend on settings.

From the repository root:

```sh
export DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:PORT/postgres'
dev/nix-shell 'just backend bench apps/backend/benchmarks/baseline.json'
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-baseline.jsonl > baseline.csv'
```

The JSON config is explicit and rejects unknown/missing fields. Copy it to
change an experiment; use the **identical config except output path** for the
partitioned rerun. Outputs use exclusive creation: choose a new path rather
than overwriting a baseline. A complete run ends with a `complete` event;
the reporter rejects unfinished runs and missing/duplicate trials.
Relative config and output paths resolve from the repository root, independent
of Cargo's package working directory; absolute paths are also accepted.

`sizes` accepts 1,000–1,000,000,000; `topic_counts` 1–1,000;
`writers` 1–64; `batch_sizes` 1–100; `trials`, `read_ms`, `write_ms`, and
`warmup_requests` must be positive. `page_limit` is 1–1,000 and
`write_growth_percent` is 1–10. The write budget must fit both one warmup and
one measured batch per writer. `seed` is 0–1,000,000. These bounds prevent
empty measurements and overflow; they are not recommended production settings.

For a longer reference-machine run, use at least 10 seconds per read case and
10+ trials, and retain the raw observations. Batch size 1 isolates per-message
cost; repeat with batch size 10 to study amortization (raise the smallest corpus
size or lower writer count if the growth budget cannot fit the batch).

## Corpus and efficient seeding

- Seed exactly 1,000, 100,000, and 10,000,000 envelopes using server-side
  `INSERT ... SELECT generate_series` in bounded 100,000-row transactions.
  There is no per-message network round trip. All production migrations,
  constraints, indexes, WAL, fsync, and synchronous commits remain enabled.
- Use `min(rows / 10, 10,000)` topics. Bootstrap one row per topic, then route
  about 50% of subsequent traffic to a hot 1%, about 40% to other active topics,
  and 10% to a dormant half. Dormant topics receive nothing in the second half
  of history; their bootstrapped rows and historical messages remain queryable.
  Later traffic formerly assigned to dormant topics goes to active topics.
- Payload sizes are 50% 256 B, 40% 1 KiB, and 10% 4 KiB: mean 947.2 B.
  Deterministic SHA-256 blocks avoid compressible repeated-character padding;
  100 templates amortize generation. A unique sequence prefix makes each
  payload distinct; stored hashes are SHA-256 of the actual payload bytes.
  Five percent of messages have commit/proposal retention; 90% are push eligible.
  Topics have the real 17-byte group-topic shape. Payloads are opaque test data,
  not validated MLS messages; cryptography/protobuf admission is not measured.
- Set watermarks, advance the real sequence and closed boundary, then
  `VACUUM ANALYZE` and `CHECKPOINT`. Seeding duration and heap/index/total bytes
  are recorded. This is a clean, append-oriented baseline, **not** a model of
  production bloat or pruning history.

## Workloads

Every request uses distinct topics. Sets rotate deterministically rather than
hammering one identical request; the result payloads are fetched/materialized.
The page limit is 100 by default, matching the backend default total page limit.

| Case | Cursor / result |
| --- | --- |
| `query_no_cursor` | Cursor 0; history from the beginning, with hits |
| `query_old` | Global sequence position at 10% of the corpus, with hits |
| `query_backlog` | Global position at 90%, with hits |
| `query_recent` | Global position at 99%, with hits |
| `query_mixed` | Per-topic cursors: 90% recent, 9% old, 1% absent, with hits |
| `query_sparse_recent` | Recent cursor over the entire topic population, including dormant topics; may return zero rows |
| `query_absent` | Nonexistent topics; always zero rows |
| `query_caught_up` | Each existing topic's exact head; always zero rows |
| `newest_full` | Real watermark-to-envelope join including payloads |
| `newest_metadata` | Real metadata-only newest query |
| `newest_absent` | Missing topics through the newest metadata path |
| `insert_single_topic` | Real duplicate lookup, transaction/locks, allocation, insert, watermark, commit |
| `insert_many_topics` | Same path, distributing inserts uniformly across existing topics |

Hit-only cases select from topics whose heads exceed the relevant cursor.
The mixed hit-only case uses recent-active topics so all three cursor modes
have results; `query_sparse_recent` is the complementary idle-topic workload.
Results are conditional measurements, not an invented weighted production
average. The 90/9/1 weights are a declared hypothesis, not measured telemetry.
The client’s actual topic/activity distribution should determine final weighting.

Infeasible cases are explicitly logged as `skipped`, not replaced with duplicate
topics or different cursors. The 1,000-row corpus has only 100 topics, and only
a subset have recent hits. Missing-topic cases can still request 1,000 topics.
Full newest requests stop at the real 100-topic API limit; metadata goes to 1,000.

### Timing and isolation

- Reads use one serial client and a one-connection production Store pool. Each
  case gets 20 untimed requests; three measured trials rotate case order.
  SQLx's normal `plan_cache_mode=auto` remains active during timing.
- Record separate custom/generic `EXPLAIN (ANALYZE, BUFFERS, WAL, SETTINGS)`
  plans outside timing. They expose planning, actual scanned rows, heap fetches,
  and buffer hits/reads; they are not substituted for client-observed latency.
- Writers run with 1 and 8 concurrent clients and pre-opened pool connections.
  Each write trial gets a new PostgreSQL `FILE_COPY` database from the disconnected
  pristine corpus, including its physical indexes. No preceding trial's growth
  or rollback/deletion bloat survives into the next one.
- One warmup batch per writer is untimed. Measured inserts stop at the requested
  duration or a **10% total growth cap**, including warmup. Starting/warmup/ending
  row counts, cap hits, and cluster WAL-byte deltas are recorded. At 1,000 rows,
  there are only about 100 inserts per trial: these are short growth-bounded
  observations, **not sustained steady-state throughput or trustworthy tail SLOs**.
- Latency excludes request construction but includes real database calls, pool
  behavior, and result consumption. Throughput uses measured wall time, including
  request construction and concurrent stragglers. Requests/s and envelopes/s
  differ for batched writes and multi-row reads.
- This is a **closed-loop** test with no think time, no unrelated app traffic,
  and no tracing subscriber. It does not measure end-to-end RPC/crypto latency,
  replica lag, open-loop overload/queueing, or simultaneous read/write interference.
  No OS cache eviction is performed. Call this **warm-working-set**, not cold-disk
  performance; a new database clone is not a reliable cold-cache test either.

Raw JSONL includes every request latency, per-trial percentiles, counts, throughput,
plans, PostgreSQL settings, migration checksums, source revision, CPU/RAM metadata,
and storage/seed timings. Envelope sizes sum the whole inheritance tree; per-table
and whole-database sizes also expose auxiliary tables in future schema variants.
Freshly checkpointed copies can incur full-page WAL images on first modification;
the recorded WAL deltas are not a steady-state WAL forecast.
CSV reports medians of trial statistics and throughput
min/max, not pooled percentiles or confidence intervals. Keep these caveats with
any quoted number. Re-run on deployment-class hardware and a larger-than-RAM
corpus before extrapolating capacity.

## Correctness checks

```sh
dev/nix-shell 'just backend test --lib db::benchmarks::tests'
dev/nix-shell 'python3 -m unittest discover -s apps/backend/benchmarks -p "test_*.py"'
dev/nix-shell 'just backend sql-check'
```

The database tests check row counts, payload/hash fidelity, topic skew/dormancy,
watermarks, post-seed allocation, duplicates, newest results, page ordering and
`has_more` against an independent reference query, and clone isolation/cleanup.
Pure tests cover cursor proportions, distinct rotating sets, configuration
bounds, output statistics, and rejection of incomplete reports.

See [the partitioning proposal](PARTITIONING.md) for the approval boundary and
the follow-up experiment; see [baseline results](BASELINE.md) for the measured run.
