# Unpartitioned database baseline

This is the initial unpartitioned schema-comparison baseline.
[Sequence-range partitioning](PARTITIONING.md) was approved on October 21, 2026,
for a benchmark-only experiment; production migration/adoption remains separate.
See the [completed schema comparison](RESULTS.md) for the subsequent experiment.

## Reproduction and scope

Measured on October 20, 2026, using the committed [configuration](baseline.json)
and the release-mode `just backend bench` recipe. The timed paths are the real
Store reads and admitted-publish storage operations, not RPC or cryptographic
validation. See [methodology and correctness checks](README.md).
Measured code revision: `2f134b52d9043374026c3c5043a26877985f7b70`;
Subsequent commits publish results, harden untimed configuration/report
validation, normalize CSV line endings, and implement the disposable partition
experiment. These recorded observations still refer to the source revision above.

- Host: eight available CPUs, Intel Xeon Platinum 8175M at 2.50 GHz,
  approximately 30.65 GiB RAM, Linux x86_64, local EBS-backed storage.
- Dedicated local PostgreSQL 18.4, synthetic data only, no other application
  traffic. All sizes use the production migrations and indexes.
- `shared_buffers=4GB`, `effective_cache_size=4GB`, `work_mem=4MB`,
  `maintenance_work_mem=64MB`, `max_wal_size=16GB`, `checkpoint_timeout=15min`.
- `fsync`, `synchronous_commit`, `full_page_writes`, `autovacuum`, and JIT are
  enabled; WAL compression is off; I/O timing is on. The raw artifact includes
  settings and migration checksums.
- Three trials per feasible workload, five-second read windows after twenty
  warmup requests, ten-second write windows or a ten-percent growth cap.
  Writes use one message per durable transaction and one or eight clients.
- Reads are serial and closed-loop, with a 100-envelope total page limit.
  Requests rotate through distinct topic sets. The mixed cursor hypothesis is
  90% recent, 9% old, and 1% absent; it is not a production telemetry estimate.
- Every write trial starts from a fresh physical copy of the pristine corpus.
  Copying, seeding, maintenance, diagnostic plans, and warmup are not timed as
  client requests. Seed time is reported separately.

## Results

The complete run finished in **5,105.3 seconds (85.1 minutes)**: 339 read trials,
36 write trials, and 113 paired custom/generic plan observations across all
three sizes. Nineteen infeasible workload combinations are explicitly recorded
as skipped. Every feasible timed workload has three trials.

- [Complete summary CSV: all 125 workload groups](results/2026-10-20-postgres18-unpartitioned.csv).
- [Compressed raw JSONL: every sample, plan, setting, and completion record](https://murmur-prod-c109d74f-murmur-uploads-prod.storage.googleapis.com/t/github_app/xmtp/uploads/github_app/xmtp/w/dev/github_oauth/neekolas/database-performance-benchmarks/23aa6851-e93d-485f-ba60-a87391f4ad0c-backend-baseline-2026-10-20-2f134b52d.jsonl.gz).
- Compressed artifact SHA-256:
  `87e0e44cef7436c7fb2d661ec7d5ca0b36255491635db07426e8816ec586fdf1`.

The artifact passes complete configured workload-matrix validation, including
explicit skips, and regenerates the committed CSV byte-for-byte.
Decompress the artifact and run `report.py` as documented in the
[reproduction instructions](README.md) to regenerate the CSV. Incomplete
diagnostic runs are excluded from both artifacts.

### Seeding and storage

| Seed rows | Topics | Seed seconds | Envelope total GiB | Envelope indexes GiB |
| ---: | ---: | ---: | ---: | ---: |
| 1,000 | 100 | 0.142 | 0.00151 | 0.00026 |
| 100,000 | 10,000 | 3.058 | 0.13799 | 0.02028 |
| 10,000,000 | 10,000 | 465.511 | 13.90722 | 2.14272 |

Seed time includes bulk insertion with maintained indexes and durability,
watermarks, sequence/boundary initialization, validation, vacuum/analyze, and
checkpoint; database creation and migrations precede this timer. Total envelope
size includes TOAST and indexes. Whole-database and per-table sizes are in the
raw artifact. Physical write-trial copies are intentionally outside all request
timers and can dominate total suite wall time.

### Inserts

One envelope per durable transaction. Rates are median trial inserts/second
with the full trial min–max range; latency columns are median trial p50 / p95
in milliseconds. Request counts sum all three trials, while duration and WAL
per insert are trial medians. Topics denotes the available topic population,
not necessarily every topic touched in a finite many-topic trial.

| Seed rows | Topics | Writers | Inserts/s, median [min–max] | p50 / p95 ms | Requests | Seconds/trial | WAL KiB/insert |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 1 | 1 | 456.6 [455.8–480.1] | 2.126 / 2.325 | 297 | 0.217 | 2.17 |
| 1,000 | 1 | 8 | 528.0 [522.2–534.9] | 14.417 / 20.201 | 276 | 0.174 | 1.95 |
| 1,000 | 100 | 1 | 459.9 [452.6–470.0] | 2.113 / 2.336 | 297 | 0.215 | 2.77 |
| 1,000 | 100 | 8 | 1,945.3 [1,945.0–1,946.6] | 3.903 / 4.852 | 276 | 0.047 | 2.49 |
| 100,000 | 1 | 1 | 485.4 [465.2–510.7] | 2.056 / 2.308 | 14,615 | 10.001 | 1.80 |
| 100,000 | 1 | 8 | 624.1 [583.9–634.1] | 13.529 / 14.084 | 18,443 | 10.012 | 1.79 |
| 100,000 | 10,000 | 1 | 490.6 [445.0–502.4] | 2.085 / 2.272 | 14,381 | 10.001 | 2.73 |
| 100,000 | 10,000 | 8 | 2,145.0 [2,062.8–2,150.9] | 3.284 / 4.561 | 29,976 | 4.658 | 1.94 |
| 10,000,000 | 1 | 1 | 495.4 [494.0–506.6] | 2.037 / 2.361 | 14,962 | 10.001 | 2.48 |
| 10,000,000 | 1 | 8 | 622.5 [603.9–627.8] | 13.816 / 14.569 | 18,566 | 10.013 | 2.32 |
| 10,000,000 | 10,000 | 1 | 356.0 [344.5–397.8] | 2.563 / 3.168 | 10,986 | 10.002 | 13.59 |
| 10,000,000 | 10,000 | 8 | 2,013.2 [1,996.3–2,018.8] | 4.059 / 5.400 | 60,301 | 10.003 | 5.32 |

At 1,000 seed rows, growth-bounded trials contain only 99 measured inserts with
one writer or 92 with eight writers; these are **not sustained capacity or tail
SLO estimates**. The 100,000-row many-topic/eight-writer trials also hit the
growth cap before ten seconds. Other larger-corpus trials last approximately
ten seconds. Report longer sustained-load measurements before capacity planning.

Eight clients improve many-topic throughput substantially but only modestly
improve the single-topic path, which retains the real publish locks. Fresh
checkpointed copies incur initial full-page WAL images; these WAL figures are
not steady-state forecasts, especially for scattered many-topic writes.

### Reads at ten million rows

Each cell is **median trial p50 / median trial p95**, in milliseconds, not
pooled percentiles. Page queries return at most 100 envelopes total, not 100
per topic. Recent means a global cursor at 99% of seeded sequence history;
old means 10%, and backlog means 90%.

| Workload | 1 topic | 10 topics | 100 topics | 1,000 topics |
| --- | ---: | ---: | ---: | ---: |
| No cursor, hits | 11.379 / 55.309 | 1.637 / 7.732 | 2.698 / 2.926 | 19.993 / 20.222 |
| Old cursor, hits | 1.390 / 49.247 | 2.291 / 11.955 | 2.503 / 2.640 | 17.965 / 18.158 |
| Backlog cursor, hits | 0.873 / 1.025 | 0.968 / 1.134 | 2.592 / 2.661 | 19.405 / 19.588 |
| Recent cursor, hits | 0.306 / 0.366 | 0.791 / 0.926 | 1.244 / 1.366 | 6.017 / 6.491 |
| Mixed cursor ages, hits | 0.307 / 7.357 | 0.834 / 19.873 | 9.114 / 32.227 | 7.626 / 7.918 |
| Recent, including idle topics | 0.263 / 0.358 | 0.556 / 0.788 | 1.192 / 1.330 | 5.349 / 5.495 |
| Missing topics, no hits | 0.212 / 0.260 | 0.215 / 0.287 | 0.416 / 0.460 | 2.966 / 3.005 |
| Existing topics, caught up | 0.216 / 0.267 | 0.258 / 0.305 | 0.582 / 0.637 | 3.803 / 3.993 |
| Newest, full envelopes | 0.225 / 0.256 | 0.281 / 0.328 | 0.696 / 0.838 | Not supported |
| Newest, metadata only | 0.232 / 0.258 | 0.268 / 0.302 | 0.528 / 0.575 | 3.156 / 3.350 |
| Newest metadata, missing topics | 0.219 / 0.247 | 0.229 / 0.255 | 0.250 / 0.295 | 1.003 / 1.035 |

Full newest is limited to 100 topics by the real API. The 1,000-topic newest
measurement uses its supported metadata path instead.

### Read scaling across sizes

Ten-topic requests, with the same p50 / p95 convention, in milliseconds:

| Workload | 1,000 rows | 100,000 rows | 10,000,000 rows |
| --- | ---: | ---: | ---: |
| No cursor, hits | 0.459 / 0.631 | 0.536 / 0.730 | 1.637 / 7.732 |
| Old cursor, hits | 0.500 / 0.639 | 0.579 / 0.767 | 2.291 / 11.955 |
| Backlog cursor, hits | 0.289 / 0.472 | 0.358 / 0.660 | 0.968 / 1.134 |
| Recent cursor, hits | Skipped | 0.327 / 0.401 | 0.791 / 0.926 |
| Mixed cursor ages, hits | Skipped | 0.323 / 0.739 | 0.834 / 19.873 |
| Recent, including idle topics | 0.229 / 0.291 | 0.250 / 0.303 | 0.556 / 0.788 |
| Missing topics, no hits | 0.211 / 0.260 | 0.224 / 0.276 | 0.215 / 0.287 |
| Existing topics, caught up | 0.238 / 0.274 | 0.221 / 0.280 | 0.258 / 0.305 |
| Newest, full envelopes | 0.455 / 0.507 | 0.255 / 0.307 | 0.281 / 0.328 |
| Newest, metadata only | 0.252 / 0.304 | 0.237 / 0.290 | 0.268 / 0.302 |

The smallest corpus has fewer than ten distinct recent-hit topics. These cases
are explicitly skipped rather than turning a hit-only scenario into a sparse
one; the sparse-recent row still exercises ten-topic polling at that size.

### Cursor effects and plan observations

- Recent, broad queries are materially cheaper in this corpus: the 1,000-topic
  recent p50 is 6.017 ms versus 19.993 ms with no cursor. This is a conditional
  workload comparison, not an isolated estimate of cursor causality.
- The existing index already avoids a history heap scan. In the 1,000-topic
  custom-plan probes, no-cursor reads produce 100,715 candidate IDs and 13,623
  shared-buffer hits; recent reads produce 12,100 candidates and 4,425 buffer
  hits. Both use `envelopes_topic_sequence` index-only probes with zero heap
  fetches, followed by at most 100 payload lookups through `envelopes_pkey`.
- Generic probes show the same access paths and actual candidate counts, but
  estimate ten input topics even when 1,000 are supplied. The follow-up must
  verify actual runtime partition pruning with both parameterized plan shapes.
  Diagnostic execution/planning times include EXPLAIN instrumentation and
  first-plan generation; they are not steady-state client latency measurements.
- Historical and mixed payload reads have large trial/order effects. For ten
  topics, no-cursor trial p95s are 43.790, 7.732, and 1.315 ms; mixed trial p95s
  are 41.446, 19.873, and 2.425 ms. The medians must not conceal this spread.
  Cache/order sensitivity is a plausible explanation, not a measured causal
  attribution; a schema improvement must exceed this variability on repeats.
- The total page limit, different eligible topic sets, and different payload
  locations make latency non-monotonic in topic count. No-hit and newest results
  remain important controls: they are already inexpensive without partitioning.

These observations support testing pruning; they do not establish that range
partitioning will help. It leaves candidate merging and the global page limit
unchanged, can increase old/no-cursor partition probes, and adds a global
idempotency registry and provisioning costs. The proposed experiment therefore
retains every baseline case and separately times missing-range rollover.

## Interpretation limits

This is a warm-working-set, append-oriented local baseline, not a production
capacity or tail-latency SLO. The OS page cache is not evicted, and the largest
envelope relation fits within physical RAM. It does not model production bloat,
row pruning history, replica lag, mixed concurrent reads and writes, or
open-loop overload. Query generation is outside per-request latency but inside
the wall-clock throughput window. Twenty rotating warmup requests do not
prewarm every historical payload page; natural cache/order effects remain.

The small corpus permits only about 100 inserts per write trial before reaching
the growth cap; these short observations do not establish sustained throughput
or reliable tails. Trial medians and ranges are descriptive, not confidence
intervals. A median of trial p95s is not the p95 of pooled requests. Sparse or
slow cases can have too few observations to support p99 conclusions.

Hit-only cursor cases have different eligible topic populations. They answer
conditional workload questions, not a perfectly controlled cursor-only causal
comparison. Recent sparse and caught-up controls retain idle topics. Do not
weight the conditional results into a fictitious production average.

Before adoption, repeat on deployment-class hardware with longer windows,
more independent trials, counterbalanced schema order, an aged/larger-than-RAM
corpus, and a realistic simultaneous read/write mix. Preserve the complete raw
observations rather than selecting only favorable cases.
