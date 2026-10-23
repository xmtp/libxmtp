# Backend database benchmarks

The [high-cardinality experiment](HIGH-CARDINALITY.md) tests one million stored
topics, small client subsets, independent cursor ages, and frontier-bounded
candidate merging against the unchanged production and watermark queries.

See the [unpartitioned baseline](BASELINE.md) and
[measured schema comparison](RESULTS.md) for results and interpretation.
The [next-round scaling experiment](SCALING.md) pairs paging algorithms and index
profiles on shared ten- and thirty-million-row corpora.

Manual schema-comparison suite, not a CI performance gate. The runner calls the
actual `Store` methods; selected reads and diagnostic `EXPLAIN` probes share
the same SQL files. The explicit `layout` selects either production's unpartitioned
schema or the approved sequence-range experiment, only in disposable fixtures.
Experimental Store hooks are excluded from production builds.

## Main-branch compatibility

The suite was rebased onto `main` at
`d0e4bdc69d1ca905d449732f4e48e9631d9df48d` on October 22, 2026. Compared with
the former `self-hosted` base, production database migrations, paging/newest SQL,
and admitted-publish SQL are unchanged. Material backend changes include
32-byte group identifiers, required publish signatures, extracted/validated
configuration, and stream/push handling. The runner now validates its connection
configuration and generates current **33-byte group topics** in both SQL seeds
and Rust requests. Its opaque synthetic payloads still exercise admitted storage,
not RPC admission, signature verification, or end-to-end publishing.

The committed baseline and comparison CSVs remain historical measurements of
their documented source revisions, using **17-byte group topics** before the
cutover. They have not been remeasured on `main`; wider topic keys can change
index size and performance. New comparisons must measure both control and
experimental variants on the current revision with the same topic width. Do not
treat the historical insert rates as current signed-publish throughput.

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
dev/nix-shell 'just backend bench apps/backend/benchmarks/partitioned.json'
```

The JSON config is explicit and rejects unknown/missing fields and duplicate
values in every workload dimension. Copy it to
change an experiment; use the **identical config except layout and output path** for the
partitioned rerun. Outputs use exclusive creation: choose a new path rather
than overwriting a baseline. A complete run ends with a `complete` event;
the reporter rejects unfinished runs and missing/duplicate trials.
Relative config and output paths resolve from the repository root, independent
of Cargo's package working directory; absolute paths are also accepted.
The committed baseline uses three trials, five-second read windows, and
ten-second write windows (subject to the growth cap below).

Format-3 configuration separates the required experimental dimensions:

- `layout`: `unpartitioned` or `sequence_ranges`.
- `max_corpus_topics`: explicit corpus population cap, independent of request sizes.
- `index_profiles`: `["baseline"]`, or `["baseline", "metadata_covering"]` on
  unpartitioned data. Each size is seeded once with its original indexes, then
  the covering replacement is built and maintained before its measurements.
- `read_strategies`: a nonempty unique list of `lateral`, `watermarks`, `bounded`,
  or `frontier`. Strategies share each corpus/index profile and rotate within trials.
- `write_sizes`: a unique subset of `sizes`; `[]` explicitly excludes inserts and
  rollovers. This avoids enormous write copies for read-only scaling controls.

Historical format-1 raw artifacts retain their original columns and complete
coverage validation. Their old `measure_writes` and suffixed layout selectors are
accepted by the reporter only, not as new runner configuration. Format 2 adds a
mostly-missing, one-populated-topic read and validates a separate complete matrix
for each profile/strategy, paired plans, and post-write visibility controls.
Format 3 additionally requires `max_corpus_topics` (100 through 10,000,000),
supports `frontier`, and requires all three independent per-topic cursor cases.
Historical format-1/2 artifacts retain their original matrices and CSV bytes.
The partitioned
experiment additionally requires admin role-creation privileges and local
authentication for the unprivileged `xmtp_bench_writer` login. It creates no
production migrations. See [partitioning](PARTITIONING.md#running-the-approved-experiment)
for registry costs, privilege boundaries, setup accounting, and timed rollover controls.

`watermarks.json` is a focused ten-million-row / 1,000-topic read experiment,
selecting `read_strategies: ["watermarks"]` independently of the range schema.
It keeps the same schema, seed, future-range inventory, read windows, and trial
count as the corresponding full-run cases. Its paging query first filters
topics whose transactional watermark cannot exceed their cursor, then uses
the unchanged bounded candidate/page logic. The prefilter relies on the same
atomic Store-maintained watermark invariant as newest reads, and all query
stages remain in one statement/snapshot. Each probe is also bounded above by that
snapshot's head to avoid empty future ranges. Full and metadata newest SQL is unchanged.
This experiment changes no production query SQL and makes no write-throughput claim.
`watermarks-unpartitioned.json` runs the same optimization without partitioning or
the registry: compare both query shapes on both schemas before attributing a gain
to partitioning. The newest queries remain unchanged controls in all layouts.

`scaling.json` compares production lateral paging with a bounded sequence-window
strategy on both baseline and metadata-covering indexes at 10M and 30M rows.
It measures insert and post-insert newest costs at 10M only. Allow at least
100 GB of additional free space for corpus/index builds, write copies, and WAL;
actual requirements depend on the recorded sizes and PostgreSQL settings.

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
  `INSERT ... SELECT generate_series` in bounded 50,000-row transactions.
  There is no per-message network round trip. All production migrations,
  constraints, indexes, WAL, fsync, and synchronous commits remain enabled.
- Use `min(rows / 10, max_corpus_topics)` topics. Historical configurations
  explicitly retain a 10,000-topic cap; the high-cardinality control uses one
  million. Bootstrap one row per topic, then route
  about 50% of subsequent traffic to a hot 1%, about 40% to other active topics,
  and 10% to a dormant half. Dormant topics receive nothing in the second half
  of history; their bootstrapped rows and historical messages remain queryable.
  Later traffic formerly assigned to dormant topics goes to active topics.
- Payload sizes are 50% 256 B, 40% 1 KiB, and 10% 4 KiB: mean 947.2 B.
  Deterministic SHA-256 blocks avoid compressible repeated-character padding;
  100 templates amortize generation. A unique sequence prefix makes each
  payload distinct; stored hashes are SHA-256 of the actual payload bytes.
  Five percent of messages have commit/proposal retention; 90% are push eligible.
  Topics have the current 33-byte group-topic shape. Payloads are opaque test data,
  not validated MLS messages; cryptography/protobuf admission is not measured.
- Set watermarks, advance the real sequence and closed boundary, then
  `VACUUM ANALYZE` and `CHECKPOINT`. Seeding duration and heap/index/total bytes
  are recorded. This is a clean, append-oriented baseline, **not** a model of
  production bloat or pruning history.
  Watermarks are reduced from each batch's returned heads and inserted once in
  sorted order, avoiding a full-table aggregate over the seeded payload heap.
  Setup/maintenance statements have a separate 15-minute timeout; timed request
  pools are re-opened with a 120-second statement timeout before measurement.

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
| `query_rare_hits` | One dormant populated topic and otherwise absent topics, cursor 0; exercises sparse ordered-scan fallback |
| `query_absent` | Nonexistent topics; always zero rows |
| `query_caught_up` | Each existing topic's exact head; always zero rows |
| `query_per_topic_recent` | Independent `head - 1` floors; exactly one unread message per topic |
| `query_per_topic_mixed` | Per-topic floors: 90% `head - 1`, 9% `head / 2`, 1% cursor 0 |
| `query_client_poll` | 90% exact heads, 9% `head - 1`, 1% cursor 0; distinct from all-caught-up |
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
- Record separate custom/generic `EXPLAIN (ANALYZE, VERBOSE, BUFFERS, WAL, SETTINGS)
  EXECUTE` plans of an explicitly prepared production SELECT outside timing.
  Preparing EXPLAIN itself would still customize the underlying SELECT, even
  with `force_generic_plan`. Diagnostics expose planning, actual scanned rows,
  heap fetches, and buffer hits/reads; they are not substituted for latency.
- Writers run with 1 and 8 concurrent clients and pre-opened pool connections.
  Each write trial gets a new PostgreSQL `FILE_COPY` database from the disconnected
  pristine corpus, including its physical indexes. No preceding trial's growth
  or rollback/deletion bloat survives into the next one.
- After every write copy, a separate one-second serial newest-metadata window
  and custom/generic plans run without vacuum. These are raw `post_write_newest`
  records, not insert latency or pooled read CSV rows. They expose visibility-map
  sensitivity and must cover every write trial. The preceding writer count is
  context, not the number of concurrent read clients.
- Write payloads are unique opaque 1 KiB messages; many-topic writes distribute
  uniformly across existing topics. One warmup batch per writer is untimed.
  Measured inserts stop at the requested
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
The reporter checks the complete configured read/write matrix, subtracting only
explicitly skipped reads with reasons, and rejects duplicate or missing trials.
Unknown workloads and nonnumeric/nonfinite cells are rejected before any CSV is
written, including when processing an externally supplied run. A completion
marker alone does not establish coverage. CSV reports medians of trial
statistics and throughput min/max, not pooled percentiles or confidence
intervals. Keep these caveats with
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

See [the approved partitioning experiment](PARTITIONING.md) for its implementation
and separate production-rollout boundary; see [baseline results](BASELINE.md) for
the unpartitioned reference run.

See [the 10M/30M scaling results](SCALING-RESULTS.md) for matched paging controls,
covering-index costs, and explicitly scoped recovery from an interrupted run.
`report.py RUN --cohort ROWS INDEX_PROFILE` validates a complete format-2/3
unpartitioned cohort independently; it still rejects missing trials, plans, or
post-write controls and never claims the parent run completed. Omitting
`--cohort` retains strict full-run completion checks.
