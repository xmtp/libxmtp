# High-cardinality client-query results

**Keep the existing per-topic index probes for the common client workload.**
With one million stored topics and independent per-topic cursors, frontier-bound
merging has worse p50 and throughput for recent and mixed-cursor reads at both
**10M and 30M envelopes**. It saves candidate work for broad historical reads,
but that is not a reason to replace the production query for every client.

These measurements supersede neither the historical data nor their source scope.
They test the membership/cursor model missing from the
[earlier scaling experiment](SCALING-RESULTS.md).
No production schema or query changes are proposed by this report.

## Scope and method

Measured source: `d0b02d3cf16e6b6a692753bed7b1a57b80a00b44`, after rebasing onto
main `c50668ac1`. Both control and experimental variants use current 33-byte group
topics, identical baseline indexes, one connection, and the same corpus per size.
The [design](HIGH-CARDINALITY.md) and [config](high-cardinality.json) describe the
frontier correctness argument and explicit workload dimensions.

- One million populated topics; clients request 10, 100, or 1,000 distinct topics
  (0.001%, 0.01%, or 0.1% of the population), with a total page limit of 50.
- The deterministic corpus retains hot/active/dormant routing and opaque mixed
  payload sizes. All topics have an initial bootstrap row; subsequent routing is
  skewed. Membership is sampled from the topic population, not from message volume.
- Per-topic recent cursors are each topic's own `head - 1`; mixed cursors are
  90% one-unread, 9% `head / 2`, 1% zero; client polling is 90% caught-up,
  9% one-unread, 1% zero. Categories rotate across topics and requests.
- These synthetic arithmetic floors are legal cursors but need not be actual
  envelope IDs on that topic. They model unread counts/age, not production traces.
- Three one-second trials per workload/strategy, 20 untimed warmups, rotating
  workload and leading-strategy order. The CSV reports the median of trial
  statistics, not percentiles pooled across trials.
- `lateral` is unchanged production paging; `watermarks` prefilters caught-up
  topics; `frontier` first probes each topic's earliest unread ID, derives a safe
  global ceiling when enough topics have hits, then merges bounded candidates.
  Newest SQL is unchanged across strategy labels and serves as a noise control.

Dedicated synthetic PostgreSQL 18.4: eight vCPUs, approximately 30.65 GiB RAM,
4 GiB shared buffers, 16 MiB work memory, 256 MiB maintenance memory, 2 GiB WAL
target, five-minute checkpoints, and parallel gather disabled. Fsync,
synchronous commits, full-page writes, and autovacuum remain enabled. Query
measurements follow seed validation, vacuum/analyze, and checkpoint; OS caches
are not evicted. Seeding and diagnostics are outside query timing.

The original worker connection ended at **02:38:15 UTC on October 23, 2026**
during the 30M seed. Its raw artifact contains a complete 10M cohort but no 30M
trials and no terminal `complete` marker. The reporter validates the 10M cohort
only with explicit `--cohort 10000000 baseline`; default whole-run reporting
rejects it. The abandoned fixture was removed, and only 30M was restarted in a
persistent runner with the same source and settings, a separate output, and a
config differing only in `sizes` and `output`. Partial seeding is never counted
as a measured cohort or combined with the retry's corpus. The retry completed
successfully, including its terminal `complete` event and default whole-run
validation. Recorded environments and all control parameters match exactly
between the original and retry artifacts, apart from `sizes` and `output`.

The reported scope is **264 groups, 792 read trials, 264 paired custom/generic
plan events, and six explicit newest-full skips**: 132 groups per size. The 10M
result is a complete selected cohort, not a claim that its original two-size run
completed. The 30M result is a separate complete single-size run. There are no
write trials or covering-index experiments in this comparison.

## Common client workload

Each cell is **p50 / p95 milliseconds**, median of three trial statistics.

| Envelopes | Requested topics | Cursor model | Production lateral | Watermarks | Frontier |
| --- | ---: | --- | ---: | ---: | ---: |
| 10M | 10 | Per-topic recent | 0.361 / 0.471 | 0.367 / 0.609 | 0.404 / 0.482 |
| 10M | 10 | Per-topic mixed | 0.367 / 0.492 | 0.408 / 0.885 | 0.405 / 0.529 |
| 10M | 10 | Client polling | 0.280 / 0.386 | 0.271 / 0.394 | 0.289 / 0.382 |
| 10M | 100 | Per-topic recent | 0.956 / 10.311 | 1.164 / 5.313 | 1.014 / 11.032 |
| 10M | 100 | Per-topic mixed | 0.855 / 10.538 | 1.121 / 2.889 | 1.023 / 1.707 |
| 10M | 100 | Client polling | 0.585 / 2.592 | 0.619 / 3.197 | 0.640 / 2.802 |
| 10M | 1,000 | Per-topic recent | 3.183 / 11.033 | 5.181 / 7.618 | 3.569 / 12.726 |
| 10M | 1,000 | Per-topic mixed | 3.256 / 9.218 | 5.233 / 7.804 | 3.579 / 6.681 |
| 10M | 1,000 | Client polling | 3.023 / 17.160 | 3.040 / 16.887 | 3.133 / 18.443 |
| 30M | 10 | Per-topic recent | 0.381 / 0.548 | 0.422 / 1.692 | 0.434 / 0.599 |
| 30M | 10 | Per-topic mixed | 0.468 / 8.472 | 0.479 / 10.852 | 0.512 / 10.106 |
| 30M | 10 | Client polling | 0.285 / 0.561 | 0.272 / 0.809 | 0.304 / 0.647 |
| 30M | 100 | Per-topic recent | 0.994 / 26.688 | 1.128 / 8.499 | 1.053 / 27.804 |
| 30M | 100 | Per-topic mixed | 1.034 / 25.219 | 1.155 / 8.737 | 1.106 / 11.498 |
| 30M | 100 | Client polling | 0.771 / 13.713 | 0.802 / 12.308 | 0.831 / 16.575 |
| 30M | 1,000 | Per-topic recent | 3.568 / 26.982 | 5.571 / 27.596 | 4.037 / 27.728 |
| 30M | 1,000 | Per-topic mixed | 3.993 / 31.118 | 6.093 / 8.712 | 4.155 / 23.328 |
| 30M | 1,000 | Client polling | 3.387 / 17.737 | 3.234 / 16.094 | 3.693 / 28.842 |

At 10M / 1,000 requested topics, frontier p50 is about **12% slower for recent**,
**10% slower for mixed**, and **4% slower for polling**. At 30M those p50
regressions are **13%, 4%, and 9%**. Frontier also has lower median throughput
for these cases at every requested topic count at both sizes. For 30M / 1,000
topics, recent throughput is 145.7 versus 128.9 requests/s, mixed is 163.9
versus 155.5, and polling is 192.1 versus 178.5 (production versus frontier).
Watermark prefiltering adds a second lookup for hit-heavy requests and is
substantially worse there. For 30M polling it lowers p50 by 5% and p95 by 9%,
but median throughput falls from 192.1 to 165.5 requests/s; that is not a clear
overall win. These are closed-loop rates, not service capacity.
The tiny 10-topic polling difference is not evidence for a production dispatch
threshold. Tail differences and trial drift require longer matched runs.

## Historical reads and no-hit controls

| Envelopes | 1,000-topic request | Production lateral | Watermarks | Frontier |
| --- | --- | ---: | ---: | ---: |
| 10M | No cursor | 4.668 / 7.022 | 6.737 / 9.470 | 3.807 / 9.864 |
| 10M | Old global cursor | 4.567 / 6.735 | 6.799 / 8.906 | 3.925 / 6.181 |
| 10M | Backlog global cursor | 3.579 / 3.903 | 5.646 / 6.148 | 3.826 / 4.170 |
| 10M | All requested topics absent | 3.114 / 3.231 | 2.434 / 2.665 | 3.107 / 3.270 |
| 10M | Existing topics, caught up | 2.481 / 2.695 | 2.221 / 2.387 | 2.513 / 2.687 |
| 10M | One populated topic, mostly absent | 3.144 / 3.404 | 2.494 / 2.642 | 3.144 / 3.343 |
| 30M | No cursor | 7.823 / 11.052 | 10.292 / 13.841 | 4.839 / 12.622 |
| 30M | Old global cursor | 7.274 / 11.698 | 9.706 / 15.478 | 4.830 / 19.089 |
| 30M | Backlog global cursor | 4.690 / 8.610 | 6.850 / 7.681 | 4.534 / 10.711 |
| 30M | All requested topics absent | 3.332 / 3.632 | 2.476 / 2.590 | 3.348 / 3.709 |
| 30M | Existing topics, caught up | 2.813 / 2.998 | 2.221 / 2.337 | 2.850 / 3.008 |
| 30M | One populated topic, mostly absent | 3.424 / 3.639 | 2.561 / 2.760 | 3.424 / 3.744 |

At 10M, no-cursor frontier p50 improves by 18%, but its p95 regresses by 40% and
median closed-loop throughput is essentially unchanged (193.8 versus 194.2
requests/s). At 30M, no-cursor p50 improves by **38%** and throughput rises from
117.5 to 146.9 requests/s, while p95 regresses by 14%. Old-cursor p50 improves
by 34%, but p95 worsens by 63% and throughput barely changes (119.6 versus
120.3 requests/s). These controls do not support a general latency or
service-capacity claim. Watermarks consistently help broad absent, fully
caught-up, and mostly absent requests; mixed polling remains inconclusive.

### Why the extra probes do not pay for recent clients

At 10M / 1,000 topics, custom-plan no-cursor diagnostics show production's
topic/sequence index-only scan returning about 6.07 candidates per topic across
1,000 probes; at 30M the average rises to **15.88**. Frontier instead does 1,000
first-hit probes and only 51 bounded candidate probes at both sizes. All retain
a final 50-row payload fetch. Both custom and generic diagnostics show these
same candidate counts at 30M.

For independently recent cursors, production already returns exactly one
candidate per topic. Frontier still needs those 1,000 probes, then another 51
probes and ceiling/sort work: little work can be removed. Polling production
averages only about 0.15 candidates per probe at 10M and 0.26 at 30M. The first
stage is overhead when
per-topic histories are already short or empty. Index-only scans report zero
heap fetches after maintenance; the covering index is not needed for this path.

Forced custom/generic plans are diagnostics outside timed requests, not
controlled latency trials. Raw artifacts preserve both; timed queries retain
PostgreSQL's automatic prepared-plan policy.

## Noise and limits

**Sampler follow-up:** a fixed stride can repeat a subset when it shares a
factor with the eligible population (for example, 7,919 topics). The generator
now selects a coprime stride, preserving the old stream wherever already
coprime. All recorded eligible populations across the seven published raw
artifacts were audited; none shares a factor with 7,919. The nine committed CSVs
and source-scoped observations are unaffected by this bug and are not remeasured
or rewritten. This does not remove the cache/order limitations below.

The 10M production 1,000-topic recent trial p50s are **22.360 / 3.183 / 3.125 ms**;
frontier is **3.569 / 3.560 / 3.606 ms**. Twenty warmup requests do not warm a
million-topic population. Strategy/workload rotation reduces order bias but does
not eliminate initial page-cache or I/O effects. Median summaries must not hide
that drift: all raw samples and trial min/max throughput remain available. At
30M, production recent trial p50s are **29.034 / 3.568 / 3.565 ms**, versus
frontier's **4.037 / 4.035 / 4.169 ms**. One-second closed-loop windows also
complete different numbers of requests, covering different lengths of the same
deterministic request prefix; they do not equalize cache residency.

Unchanged newest-metadata SQL at 10M / 1,000 topics has p50s of
**4.871 / 4.798 / 4.890 ms** under the lateral/watermark/frontier labels, but
median p95s are around **96–100 ms**. It receives no optimization here; variations
are noise, not paging gains. Full-envelope newest at 1,000 topics is explicitly
skipped because it exceeds the production API's limit.

At 30M, unchanged newest-metadata production trial p50s are
**242.116 / 5.822 / 5.218 ms**; median p95s across strategy labels are around
**232–238 ms**. Cache/I/O tails remain material even when the steady median is
small. Longer trials after broader priming, plus fresh/cold controls, are needed
before choosing a tail-sensitive dispatch policy.

These are warm, closed-loop read-only microbenchmarks, not a fixed-arrival p99
SLO test. They exclude concurrent publishing, queueing, admission/signatures,
replicas, bloat/aging, and cold-cache controls. They do not measure 100M envelopes
or establish a partitioning threshold for a 64-GiB instance. Historical runs
used different topic widths, populations, page limits, and trial lengths; do
not attribute differences between those runs to one algorithm or to corpus size.

## Seed and storage

| Envelopes | Populated topics | Seed/validate/maintenance seconds | Database GB | Envelope total GB | Envelope indexes GB | Watermark MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 10M | 1,000,000 | 781.4 | 16.538 | 16.394 | 2.670 | 135.7 |
| 30M | 1,000,000 | 3,758.7 | 49.394 | 49.250 | 8.077 | 135.7 |

Units are decimal. Envelope total includes TOAST and associated indexes; the
separate main-heap sizes are 8.192 GB at 10M and 24.576 GB at 30M. Loading uses bounded server-side
50,000-row batches and retains every production index and durability setting.
Seed seconds include sequence/watermark setup, validation, vacuum/analyze, and
checkpoint, not just insertion. The conservative 2-GiB WAL target produces
checkpoint pressure; this is an observed fixture cost, not optimized bulk-ingest
throughput or a production insert-rate measurement.

## Recommendations

1. **Retain production per-topic probes for recent/mixed clients.** They already
   avoid scanning unrelated messages and need no global uniform cursor. Do not
   apply the earlier dense-prefix speedup to this sparse-membership workload.
2. **Keep frontier merging experimental.** Historical reads can benefit, but
   recent/polling overhead and variable tails rule out a blanket replacement.
   Any dispatch policy needs measured unread density, not just total row count.
3. **Use the watermark result narrowly.** Empty/mostly absent controls improve;
   the common mixed polling request does not establish an overall benefit. Head caching
   must preserve snapshot, concurrent-write, and rollback guarantees.
4. **Prioritize mixed-traffic capacity next.** Hold topic population and client
   membership realistic while varying total rows, skew, cursor lag, arrival
   rates, and write concurrency; measure sustained p95/p99, I/O, and queueing on
   target hardware. Do not extrapolate these samples to a 64-GiB capacity limit.
5. **For newest reads, test a compact transactional head-metadata projection.**
   This targets the unchanged metadata bottleneck without copying payloads into
   the envelope index. It remains a hypothesis requiring measured write costs
   and duplicate, pruning, ordering, and rollback tests; it is not implemented.

## Artifacts and reproduction

- [10M CSV: 132 complete groups](results/2026-10-23-postgres18-high-cardinality-10m.csv).
- [30M CSV: 132 complete groups](results/2026-10-23-postgres18-high-cardinality-30m.csv).
- [Original raw JSONL, compressed: complete 10M cohort, interrupted 30M seed](https://murmur-prod-c109d74f-murmur-uploads-prod.storage.googleapis.com/t/github_app/82580170/uploads/github_app/xmtp/w/dev/github_oauth/neekolas/database-performance-benchmarks/596ec83c-805d-45ae-962a-19bc6741aae3-backend-high-cardinality-2026-10-23-d0b02d3cf-interrupted.jsonl.gz).
- Original compressed SHA-256: `6ad6edc865c7ee26fb448527fc21ed532bf45a3c7a5667e915808ff49b01187b`.
- [Complete 30M retry raw JSONL, compressed](https://murmur-prod-c109d74f-murmur-uploads-prod.storage.googleapis.com/t/github_app/82580170/uploads/github_app/xmtp/w/dev/github_oauth/neekolas/database-performance-benchmarks/613d71e1-45dd-4fe1-aa81-9dd3df1d8141-backend-high-cardinality-2026-10-23-d0b02d3cf-30m-complete.jsonl.gz).
- Retry compressed SHA-256: `346ffcbc7f5b590030ade345b75cf1236b3be520ae649bf4a79857fc1cf59c40`.

After decompression, regenerate the two committed CSVs without editing the raw
config or inventing a completion marker:

```sh
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-high-cardinality-2026-10-23-d0b02d3cf-interrupted.jsonl --cohort 10000000 baseline'
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-high-cardinality-2026-10-23-d0b02d3cf-30m-complete.jsonl'
```

The first artifact's default whole-run report and its incomplete 30M cohort
both fail validation. The retry passes default full-run validation. All 27
reporter tests pass, both CSVs regenerate byte-for-byte, and raw artifacts are
unmodified apart from reproducible gzip compression. Frontier query SHA-256:
`9ba7dd3b01ed3b1a17e972254f8bf74fdd59bbad0debe011a65368e397823382`.

Implementation validation also passed 366 backend library and nine binary
tests, strict all-feature/all-target backend Clippy, SQLx metadata checks,
formatting, and a release-mode end-to-end smoke. Four Rust and two Python
requirements were mutation-proven; all seven historical CSVs reproduce exactly.
