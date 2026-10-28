# Tens-of-millions experiment results

**Scope update:** the [October 23 high-cardinality follow-up](HIGH-CARDINALITY-RESULTS.md)
now tests 1M stored topics and independent per-topic cursors at 10M/30M envelopes.
The production per-topic query wins on that recent/mixed client workload;
frontier merging helps some historical reads but worsens tails. The results
below remain the earlier dense/uniform-cursor experiment, not a general client
paging recommendation.

**The strongest measured improvement is a query fast path, not partitioning or
a covering index.** On the original unpartitioned indexes, broad uniform-cursor
reads improve at both 10M and 30M envelopes. At 30M, 1,000-topic recent-hit p50
falls from **10.131 to 4.697 ms**, and no-cursor p50 from **21.429 to 4.689 ms**.
Sparse fallbacks and heterogeneous cursors remain important limits; this is not
an unconditional production rollout recommendation.

The metadata-covering replacement is rejected on the complete 10M comparison:
newest metadata becomes slower even with zero heap fetches, insert throughput
falls, and post-write observations remain worse. Production schema and query
SQL are unchanged. See the [implemented design](SCALING.md) and
[earlier partition/watermark comparisons](RESULTS.md).

## Scope and interruption

Measured source: **`7ed4cdc8e95b709e6171216ea7a7633637e08385`**. The dedicated
synthetic PostgreSQL 18.4 instance retained the earlier durability and settings:
8 CPUs, approximately 30.65 GiB RAM, 4 GiB shared buffers, synchronous commits,
full-page writes, and fsync enabled. `scaling.json` specifies three five-second
read trials, 20 warmups, a 100-envelope page, rotating 1/100/1,000-topic requests,
and three ten-second write trials with 1/8 writers and one-message transactions.

The host stopped the benchmark and PostgreSQL at **06:33:51 UTC on October 22,
2026**, after starting the runner at 03:17:15 UTC. The runner log records signal
15; PostgreSQL records an administrator-terminated connection and smart
shutdown. This was not a completed full-matrix run. A shell exit marker recorded
zero during termination; it is **not** evidence of successful completion. The
raw JSONL has no terminal `complete` event, and default reporting rejects it.
The expensive workload was not restarted.

Only these **complete, explicitly scoped cohorts** are reported:

| Size | Index profile | Paging strategies | Read trials | Write trials | Paired plans | Post-write observations |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 10M | Baseline | Lateral / bounded | 210 | 12 | 70 | 12 |
| 10M | Metadata covering | Lateral / bounded | 210 | 12 | 70 | 12 |
| 30M | Baseline | Lateral / bounded | 210 | 0 | 70 | 0 |

That is **218 workload groups, 630 read trials, 24 write trials, 210 paired
custom/generic plan observations, 24 post-write observations, and six explicit
skips**. Full-envelope newest at 1,000 topics exceeds the production API limit
and is skipped, not silently timed. All selected trials, plans, raw samples,
post-write controls, and corpus metadata pass the same coverage checks used for
complete runs. Partial 30M covering-index read trials are excluded entirely;
there is no 30M covering-index latency result or 30M write result here.

## Broad paging results

Each cell is **p50 / p95 milliseconds**, using the median of the three trial
statistics, not pooled percentiles. These use baseline indexes on the same
seeded corpus for each size. `bounded` combines the watermark prefilter and
ordered-prefix attempt; its gains do not isolate the prefix from the prefilter.

| 1,000-topic request | 10M lateral | 10M bounded | 30M lateral | 30M bounded |
| --- | ---: | ---: | ---: | ---: |
| No cursor, hits | 20.321 / 20.502 | 4.702 / 4.829 | 21.429 / 21.646 | 4.689 / 4.831 |
| Old cursor, hits | 18.380 / 18.560 | 4.712 / 4.773 | 20.326 / 20.540 | 4.698 / 4.818 |
| Backlog cursor, hits | 20.001 / 20.173 | 4.713 / 4.849 | 19.983 / 20.228 | 4.726 / 4.854 |
| Recent cursor, hits | 6.470 / 6.651 | 4.784 / 4.903 | 10.131 / 10.315 | 4.697 / 4.778 |
| Mixed cursor ages | 8.040 / 8.246 | 8.000 / 8.147 | 11.367 / 11.615 | 11.360 / 11.577 |
| Sparse recent topics | 5.471 / 5.600 | 4.462 / 4.547 | 7.393 / 7.598 | 4.392 / 4.571 |
| Mostly missing, one populated topic | 3.774 / 4.354 | 4.136 / 4.425 | 3.930 / 4.754 | 4.161 / 45.996 |
| All topics absent | 3.034 / 3.101 | 1.675 / 1.704 | 3.169 / 3.276 | 1.672 / 1.699 |
| Existing topics, caught up | 3.943 / 4.056 | 3.933 / 4.077 | 4.216 / 4.349 | 4.220 / 4.340 |

At 30M, the p50 improvements are **2.16x recent**, **4.57x no cursor**,
**4.33x old cursor**, and **1.89x absent topics**. Corresponding 10M improvements
are 1.35x, 4.32x, 3.90x, and 1.81x. Broad recent bounded throughput is
204.2 requests/s at 10M and 208.2 at 30M, including request generation in the
closed-loop window; these are not concurrent service-capacity measurements.

The 1/100-topic cases and heterogeneous cursors retain the exact production SQL.
Mixed ages and per-topic caught-up heads therefore show no algorithmic gain.
An unchanged SQL control is not an independent new optimization. A recent
**common global lower bound** is not the same as independently advancing
per-topic client cursors; this eligibility restriction matters in production.

### Plans and adverse cases

Both forced custom and forced generic diagnostics use an ordered
`envelopes_pkey` scan for the prefix and return 4,096 window rows on broad
hit-heavy controls, without a full envelopes sequential scan. Dense prefixes
produce 101 candidates and the per-topic fallback nodes are never executed.
The final payload lookup runs only for the 100 returned envelopes. This proves
the observed access path, **not** a general physical-I/O bound under bloat,
visibility churn, or different statistics.

The rare-hit control retains only one eligible populated topic. Its prefix has
zero matches, and the same-statement per-topic fallback supplies the real page.
Correctness holds, but speculative work is not free. At 30M, rare-hit trial
p95s are **63.483 / 4.754 / 4.102 ms** for lateral versus
**62.437 / 45.996 / 4.239 ms** for bounded. Median-trial p99s are 61.944 and
63.122 ms. Throughput ranges are 74.8–208.0 and 85.6–199.5 requests/s.
Both controls have substantial warmup/order/cache sensitivity; do not attribute
the entire tail difference to one SQL node or advertise the median alone.

Mixed-cursor unchanged-SQL controls also expose priming drift: the 30M lateral
trial p50s are 19.832 / 11.320 / 11.367 ms versus
11.360 / 11.372 / 11.350 ms under the bounded selector. The same-workload
algorithms are adjacent, their leading order alternates by trial, and workload
groups rotate, but three trials do not erase first-pass/cache effects.

## Covering-index result at 10M

Index comparisons below use `strategy=lateral`; duplicate newest observations
under the bounded selector are controls, not six independent trials.

| Newest request | Baseline p50 / p95 ms | Covering p50 / p95 ms |
| --- | ---: | ---: |
| Metadata, 100 topics | 0.585 / 0.667 | 0.705 / 0.747 |
| Metadata, 1,000 topics | 3.423 / 3.648 | 4.759 / 4.903 |
| Full envelopes, 100 topics | 0.781 / 0.915 | 0.744 / 0.898 |

The covering profile actually chooses an index-only scan in **both** plan modes,
with **zero heap fetches** after seed maintenance. Nevertheless, its custom-plan
metadata probe uses 4,496 envelope-index buffer hits versus 4,000 for the
baseline primary-key/heap lookup, and the observed request is slower. An
index-only plan is not sufficient evidence of a performance improvement.
The small full-envelope difference is not a covering-metadata benefit and is
not interpreted causally across the fixed profile order.

### Fresh-copy insert costs

Values are median envelopes/s and median WAL bytes per acknowledged envelope
over three independent ten-second trials. Every trial starts with a pristine
physical copy; copy time is outside throughput timing. No growth cap was hit.

| Insert workload | Writers | Baseline envelopes/s | Covering envelopes/s | Baseline WAL bytes/envelope | Covering WAL bytes/envelope |
| --- | ---: | ---: | ---: | ---: | ---: |
| Single topic | 1 | 486.5 | 467.6 | 2,556 | 2,701 |
| Single topic | 8 | 623.2 | 609.8 | 2,381 | 2,510 |
| Many topics | 1 | 385.4 | 364.3 | 13,902 | 15,323 |
| Many topics | 8 | 1,977.1 | 1,879.9 | 5,444 | 5,569 |

Covering throughput falls approximately **2–5%**; many-topic WAL increases are
also visible. Fresh-copy full-page images make these conditional WAL costs,
not an aged database's steady-state byte rate. Eight writers improve the same
single topic only 1.28x versus one writer, while many topics improve 5.13x:
do not remove ordering/idempotency serialization just to raise a benchmark.

### Newest metadata before vacuum

Every write copy has a separate one-second, 1,000-topic metadata observation,
with 20 warmups and custom/generic diagnostics, before vacuum. This is serial
post-write measurement, **not simultaneous read/write load**.

| Preceding insert workload | Writers | Baseline p50 / p95 ms | Covering p50 / p95 ms | Covering plan heap fetches |
| --- | ---: | ---: | ---: | ---: |
| Single topic | 1 | 3.529 / 3.735 | 4.711 / 4.871 | 2 |
| Single topic | 8 | 3.909 / 4.819 | 5.084 / 5.817 | 2 |
| Many topics | 1 | 3.371 / 3.606 | 4.886 / 5.274 | 1,000 |
| Many topics | 8 | 3.787 / 4.593 | 5.257 / 6.068 | 1,000 |

Heap-fetch counts agree in both policies and all three write copies for each
row. Baseline uses a normal primary-key index scan; lack of a `Heap Fetches`
field is **not** zero heap access. Covering remains slower even for single-topic
updates with only two heap fetches. Visibility loss is an additional cost, not
the sole explanation for the covering profile's regression.

## Seed, storage, and setup

Each size is seeded once with maintained baseline indexes, validation, and
maintenance: **517.1 seconds at 10M**, **2,169.5 seconds at 30M**, with 10,000
populated topics. Covering setup replaces the old index without reseeding.

| Rows | Baseline envelope total GiB | Covering envelope total GiB | Baseline indexes GiB | Covering indexes GiB | Covering setup seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| 10M | 13.907 | 14.159 | 2.143 | 2.395 | 32.9 |
| 30M | 41.780 | 42.470 | 6.487 | 7.177 | 245.0 |

Totals include heap, TOAST, and maintained indexes. Setup includes replacement,
`VACUUM`, and `CHECKPOINT` together, outside query timing. The 30M covering
storage/setup observations completed before interruption; its read cohort did
not. Bulk seed time is shared baseline work, not covering-index ingest speed.
The replacement also rebuilds the physical index; the comparison does not
isolate INCLUDE width from rebuild/compaction effects.

Column statistics are retained between profiles, avoiding an ANALYZE resample.
Baseline precedes covering; profile order is not randomized. Although 30M is
larger than RAM, the workload can still have a small hot working set. There is
no cold-cache, aged/bloated-table, replica, or fixed-arrival-rate saturation
claim. Prepared-policy diagnostics are outside timed requests; timed Store
calls retain PostgreSQL's normal automatic plan policy.

## Recommendations

1. **Keep the unpartitioned schema and its global uniqueness constraints.** The
   earlier sequence-range design lost on these workloads. These gains need no
   partition lifecycle, global identity registry, or weakened deduplication.
2. **Advance adaptive paging, not an unconditional query replacement.** Retain
   the original path for small/heterogeneous requests, preserve same-snapshot
   fallback and exact `has_more`, and measure fallback frequency, prefix-match
   density, real per-topic cursor distributions, and p99 before rollout. The
   256-topic threshold and 4,096-row budget are experiments, not tuned defaults.
3. **Test cursor-grouped candidate merging next.** Uniform-cursor gains do not
   solve mixed clients. A follow-up could produce bounded candidates per cursor
   group and merge globally with strict per-topic floors. This is unimplemented
   and requires independent ordering/`has_more` proofs and sparse controls.
4. **Reject this metadata-covering replacement.** A transactional compact topic
   head-metadata projection is a more targeted next hypothesis for newest reads.
   It would need duplicate, ordering, expiry/pruning, and rollback consistency
   tests plus measured write/storage costs; no benefit is claimed yet.
5. **Measure capacity with mixed traffic before infrastructure changes.** Add
   concurrent reads/writes, realistic batching, fixed arrival rates, queueing,
   and sustained p99 observations on the deployment PostgreSQL version and
   hardware. Do not extrapolate this closed-loop microbenchmark to production
   envelopes/s, or shard away correctness constraints based on corpus size.

## Artifacts and reproduction

- [10M baseline-index CSV: 74 complete groups](results/2026-10-22-postgres18-scaling-10m-baseline.csv).
- [10M covering-index CSV: 74 complete groups](results/2026-10-22-postgres18-scaling-10m-covering.csv).
- [30M baseline-index CSV: 70 complete groups](results/2026-10-22-postgres18-scaling-30m-baseline.csv).
- [Unmodified interrupted raw JSONL, compressed; includes excluded partial observations](https://murmur-prod-c109d74f-murmur-uploads-prod.storage.googleapis.com/t/github_app/82580170/uploads/github_app/xmtp/w/dev/github_oauth/neekolas/database-performance-benchmarks/4e0526da-c62f-446d-b9bb-3f77cecb9016-backend-scaling-2026-10-22-7ed4cdc8e-interrupted.jsonl.gz).
- Compressed SHA-256: `9195a739d6a9041e4fbb234c11c861860f7f70ecc474d1959118c441c67b1ded`.

After decompression, regenerate each CSV with explicit scope:

```sh
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-scaling-2026-10-22.jsonl --cohort 10000000 baseline'
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-scaling-2026-10-22.jsonl --cohort 10000000 metadata_covering'
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-scaling-2026-10-22.jsonl --cohort 30000000 baseline'
```

The reporter does not alter the raw config, append a completion marker, or infer
missing trials. Default full-run reporting and the incomplete 30M covering
cohort both fail. An explicit cohort still requires every configured workload,
trial, paired plan, and post-write observation within that size/profile. This
recovery interface is restricted to format-2 unpartitioned experiments; it does
not bypass legacy validation or partition-rollover requirements.
