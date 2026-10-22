# Schema comparison results

The initial million-sequence-ID range layout does **not** justify production
adoption on this workload. At ten million rows it makes recent broad reads
slower, materially regresses empty polling, lowers insert throughput, and adds
storage and provisioning costs. Runtime pruning works; pruning alone is not
enough to beat the existing unpartitioned indexes.

These are measured results, not a production capacity forecast. The
[baseline methodology and caveats](BASELINE.md), [suite instructions](README.md),
and [approved experiment design](PARTITIONING.md) apply. Production migrations
and query SQL remain unchanged.

## Reproduction and artifacts

The full partitioned run started on October 21 and completed on **October 22,
2026**, on the baseline's dedicated synthetic PostgreSQL 18.4 instance. It took
**97.4 minutes**. PostgreSQL settings, available CPUs, CPU model, seed, payload
distribution, workload dimensions, windows, and trial counts match the original
baseline. Reported RAM differs by only 4 KiB after VM resumption. The runs are
historical, not interleaved randomized schema trials; do not interpret small
differences as statistically established causal effects.

- Baseline source: `2f134b52d9043374026c3c5043a26877985f7b70`.
- Partitioned source: `6d8066e0581978d7db459b5d8c3213baebb3f534`.
- [Full partitioned configuration](partitioned.json). The historical raw config
  predates the explicit `measure_writes` selector and contains the full matrix.
- [Partitioned CSV](results/2026-10-21-postgres18-sequence-ranges.csv) and
  [baseline CSV](results/2026-10-20-postgres18-unpartitioned.csv).
- [Compressed partitioned raw JSONL, including samples and plans](https://murmur-prod-c109d74f-murmur-uploads-prod.storage.googleapis.com/t/github_app/82580170/uploads/github_app/xmtp/w/dev/github_oauth/neekolas/database-performance-benchmarks/30c76363-a69f-4128-a364-6e08a0a2f59d-backend-sequence-ranges-2026-10-21-6d8066e05.jsonl.gz).
- Compressed SHA-256:
  `ba80aa39f338f70de1484f2043e45574f671a57f2755321104cd1a0fc6ab2368`.

The artifact passes complete coverage validation: **125 workload groups,
339 read trials, 36 write trials, 113 paired custom/generic plan observations,
19 explicit skips, and 24 additional rollover trials**. The current reporter
regenerates both committed full-run CSVs byte-for-byte. See the reproduction
instructions to decompress the artifact and run `report.py`.

## Seeding and storage

All timings include bulk load, maintained indexes, actual registry triggers/FKs,
watermark construction, sequence/boundary initialization, validation, maintenance,
and checkpoint. Creation and known-range provisioning precede the seed timer and
are recorded separately. No constraints or durability settings are disabled.

| Seed rows | Baseline seed s | Range seed s | Baseline envelope GiB | Range envelope GiB | Global registry GiB | Range + registry GiB |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 0.142 | 0.168 | 0.00151 | 0.00140 | 0.00027 | 0.00168 |
| 100,000 | 3.058 | 6.081 | 0.13799 | 0.12852 | 0.02029 | 0.14881 |
| 10,000,000 | 465.511 | 1,203.393 | 13.90722 | 12.88651 | 2.02032 | 14.90683 |

At ten million rows, registry-inclusive storage rises **7.2%** and seeding takes
**2.59 times** as long. Comparing only the smaller envelope children would hide
the global uniqueness cost. These sizes include TOAST and indexes, but are not
whole-cluster disk requirements; write trials also need a fresh physical copy.

## Durable inserts

One envelope per transaction. Rates are median trial envelopes/second; the
bracketed range is trial min–max. Latencies are **median trial p50 / median trial
p95**, in milliseconds, not percentiles pooled across trials. Available topics
does not imply every topic was touched in a finite write trial.

| Seed rows | Topics | Writers | Baseline inserts/s | Range inserts/s [min–max] | Rate change | Range p50 / p95 ms |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1,000 | 1 | 1 | 456.6 | 370.8 [365.2–378.6] | -18.8% | 2.620 / 3.036 |
| 1,000 | 1 | 8 | 528.0 | 379.4 [374.5–382.2] | -28.2% | 19.798 / 35.919 |
| 1,000 | 100 | 1 | 459.9 | 373.1 [365.2–418.6] | -18.9% | 2.603 / 3.108 |
| 1,000 | 100 | 8 | 1,945.3 | 1,517.2 [1,475.8–1,526.5] | -22.0% | 4.682 / 6.766 |
| 100,000 | 1 | 1 | 485.4 | 406.5 [395.5–409.2] | -16.3% | 2.511 / 2.818 |
| 100,000 | 1 | 8 | 624.1 | 501.7 [486.6–510.0] | -19.6% | 17.037 / 17.901 |
| 100,000 | 10,000 | 1 | 490.6 | 399.9 [399.2–402.1] | -18.5% | 2.453 / 2.823 |
| 100,000 | 10,000 | 8 | 2,145.0 | 1,780.8 [1,774.3–1,784.8] | -17.0% | 4.101 / 5.353 |
| 10,000,000 | 1 | 1 | 495.4 | 305.2 [304.1–312.0] | -38.4% | 3.253 / 3.592 |
| 10,000,000 | 1 | 8 | 622.5 | 378.8 [374.2–406.7] | -39.2% | 20.520 / 24.057 |
| 10,000,000 | 10,000 | 1 | 356.0 | 270.8 [268.8–284.3] | -23.9% | 3.576 / 4.182 |
| 10,000,000 | 10,000 | 8 | 2,013.2 | 1,388.4 [1,378.1–1,396.4] | -31.0% | 5.637 / 6.936 |

At 1,000 rows, only 99 measured inserts with one writer or 92 with eight fit
the growth budget; these are not sustained capacity or reliable tail estimates.
The 100,000-row many-topic/eight-writer case also ends at its growth cap. The
larger remaining trials last approximately ten seconds. Catalogue validation
remains on the actual partitioned publish path; this experiment does not hide
that work behind a cache. Registry maintenance is not its only additional cost.

Ten-million-row median trial WAL KiB per envelope:

| Topics | Writers | Baseline | Sequence ranges |
| ---: | ---: | ---: | ---: |
| 1 | 1 | 2.484 | 3.099 |
| 1 | 8 | 2.325 | 2.864 |
| 10,000 | 1 | 13.592 | 7.872 |
| 10,000 | 8 | 5.317 | 4.771 |

WAL does not move uniformly: many-topic writes emit less in this run. Freshly
checkpointed physical copies incur layout-dependent first-modification full-page
images; these are not steady-state logical WAL forecasts or an isolated estimate
of registry WAL. Each raw trial retains its WAL delta and insert count.

## Reads at ten million rows

Cells are **median trial p50 / median trial p95**, milliseconds. Page reads have
one total limit of 100 envelopes. Full newest is limited to 100 topics by the
real API; metadata newest supports 1,000. The CSV retains every topic count and
corpus size, including explicit infeasible-case skips in the raw artifact.

| Workload | Topics | Baseline p50 / p95 | Sequence ranges p50 / p95 |
| --- | ---: | ---: | ---: |
| No cursor, hits | 1,000 | 19.993 / 20.222 | 31.862 / 32.217 |
| Old cursor, hits | 1,000 | 17.965 / 18.158 | 34.272 / 34.700 |
| Backlog cursor, hits | 1,000 | 19.405 / 19.588 | 23.220 / 23.474 |
| Recent cursor, hits | 1,000 | 6.017 / 6.491 | 7.290 / 7.460 |
| Mixed cursor ages, hits | 1,000 | 7.626 / 7.918 | 9.793 / 9.964 |
| Recent, including idle topics | 1,000 | 5.349 / 5.495 | 5.579 / 5.753 |
| Missing topics, no hits | 1,000 | 2.966 / 3.005 | 31.904 / 33.629 |
| Existing topics, caught up | 1,000 | 3.803 / 3.993 | 9.599 / 9.892 |
| Newest, full envelopes | 100 | 0.696 / 0.838 | 1.009 / 1.157 |
| Newest, metadata only | 1,000 | 3.156 / 3.350 | 5.660 / 5.851 |
| Newest metadata, missing topics | 1,000 | 1.003 / 1.035 | 1.058 / 1.106 |

Recent cursors are much cheaper than historical cursors on both layouts, but
range partitioning does not improve the broad recent case: its p50 rises 21.2%.
Missing-topic p50 rises 10.8-fold; caught-up p50 rises 2.5-fold. This is especially
important for clients that spend much of their time polling near the head.

Some small-topic historical and mixed cases appear faster than the original
baseline. Its documented large trial/order effects remain a caveat; do not
generalize those isolated improvements to the workload or hide broad regressions
behind a weighted average with invented traffic weights.

### What the plans show

Both forced custom and actual prepared generic diagnostics show:

- At 1,000 recent-hit topics, only `envelopes_p9` supplies candidates and payloads.
  Older children are not executed. The precreated empty growth child `p10` is
  still probed for 979 topics after their candidate stream is exhausted.
- Missing topics with cursor zero execute **11,000 child index-only probes**:
  all eleven children, including the empty growth range, for all 1,000 topics.
  No returned payloads does not imply no database work.
- Caught-up topics prune ranges below their per-topic heads, but dormant heads
  still probe later ranges: children `p4`–`p8` each run 471 times, while `p9` and
  `p10` each run 1,000 times.
- Newest metadata prunes to exactly the head's child: `p4` for 471 dormant topics
  and `p9` for 529 active topics. This still loses to the unpartitioned control.
- Candidate probes remain index-only with zero heap fetches. Partitioning does
  not remove the per-topic candidate merge or improve its total-page bound.

EXPLAIN instrumentation and first planning are outside latency windows. Its
execution/planning times are not substituted for steady-state client latency.

## First-use rollover

These **sparse-ID, 1,000-row fixtures** isolate range creation and concurrent
retry costs; the 10,000,001 boundary is not a populated ten-million-row rollover.
Every group has three trials. Each trial has one request per writer, so the
reported medians and maxima are observations, not production p95/p99 estimates.
Missing-range requests include rollback, owner DDL, and the complete publish retry.
Precreated controls still include first-use backend/checkpoint effects, not the
steady-state write warmup used in the full matrix.

| First ID of range | Writers | Missing range, median trial p50 ms | Precreated, median trial p50 ms | Missing range, maximum observed ms |
| ---: | ---: | ---: | ---: | ---: |
| 1,000,001 | 1 | 21.685 | 9.333 | 21.900 |
| 1,000,001 | 8 | 30.721 | 16.112 | 35.781 |
| 10,000,001 | 1 | 22.174 | 9.837 | 22.488 |
| 10,000,001 | 8 | 31.256 | 15.731 | 35.652 |

Median trial cluster WAL is approximately 326–376 KiB with creation versus
23–71 KiB for the precreated controls. A long-lived, contended deployment needs
separate rollover and catalogue-lock testing before adoption.

## Query-shape controls

The suite also supports the watermark-based paging variation on **both** schemas,
with explicit read-only configurations. This isolates a query optimization from
the choice to partition. Newest remains unchanged. Comparative measurements are
reported separately from the complete original-query matrix.

## Next experiments

1. **Optimize broad ordered paging before adding schema complexity.** Keep the
   watermark bounds, then compare a sequence-ordered top-K path against the
   current bounded per-topic merge for broad hit-heavy requests. Retain absent,
   sparse, and old-cursor cases: a global ordered scan that is excellent for dense
   hits can be disastrous for rare topics. Preserve one snapshot, prefix order,
   and exact `has_more`; verify custom and generic plans rather than forcing one
   shape for every selectivity.
2. **Small metadata covering indexes, then metadata/payload separation if needed.**
   Test coverage for newest metadata without putting large payloads in B-trees.
   Account for added insert WAL/storage and visibility-map behavior. Candidate
   probes already have zero heap fetches, so extra coverage must target an actual
   remaining heap lookup rather than duplicate the existing paging index.
3. **Hash partition by topic as a separate design.** Equal topics route together,
   which can preserve `(topic, message_hash)` uniqueness locally without adding
   the sequence ID to that key. It still needs an explicit solution for global
   sequence-only uniqueness/lookups, correct topic-aware joins, and cross-topic
   ordered pages. Broad requests may touch every bucket; this is a hypothesis,
   not an assumed improvement or a registry-free replacement already validated.

A validated range cache/preprovisioning variation could reduce this experiment's
per-publish catalogue cost, but cannot remove its global registry or the observed
read-probe amplification. Do not recommend range partitioning from this first
layout. Before any production choice, repeat the winning query/schema controls
with realistic concurrent read/write load, longer fixed-arrival-rate trials,
and an aged corpus larger than RAM. Sequence ranges are not expiry ranges and
must not be dropped as a substitute for the existing retention semantics.
