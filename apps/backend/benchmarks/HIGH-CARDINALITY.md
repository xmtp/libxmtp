# High-cardinality topics and independent client cursors

**Status: implemented and measured at 10M/30M envelopes across 1M topics.**
See [findings, complete cohort scope, and artifacts](HIGH-CARDINALITY-RESULTS.md).
The 10M cohort survived an interruption; the separate 30M retry completed fully.
Authorized by the
October 23, 2026 follow-up after rebasing onto main `c50668ac1`. All changes are
benchmark/test-only; no production schema, query, or API guarantee changes.

## Question and controls

The earlier broad-query gains relied on uniform cursors and only 10,000 stored
topics. They cannot establish an improvement for a client following a small
subset of millions of topics with different cursor positions. This experiment
tests that workload directly, not another dense global-prefix scan.

`high-cardinality.json` seeds **10M then 30M envelopes, each across 1M topics**.
Each client requests 10, 100, or 1,000 distinct topics (0.001%, 0.01%, or 0.1%
of the population), with a total page limit of 50. The baseline indexes and
global uniqueness constraints remain intact. Both sizes share the original
50/40/10 hot/active/dormant routing and opaque payload distribution; the topic
bootstrap is included, so final fractions differ by size. This is a synthetic
membership model, not a claim to have measured production subscriber membership.

Every algorithm runs on the **same corpus**, with workload/leading-algorithm
order rotated between three one-second trials and 20 untimed warmups. Compare:

- `lateral`: the unchanged production query and strict per-topic index probes.
- `watermarks`: skip caught-up topics using their transactional heads first.
- `frontier`: derive a safe global candidate ceiling from independent first
  unread IDs, then merge bounded per-topic ranges below that ceiling.

The original cursor-age, sparse-hit, absent, caught-up, and newest controls remain.
Three added workloads select from the entire topic population, including dormant
topics, and derive each floor from **that topic's own head**:

- Recent: `head - 1`, exactly one unread envelope per requested topic.
- Mixed: 90% one unread, 9% `head / 2`, 1% no cursor.
- Polling: 90% caught up, 9% one unread, 1% no cursor.

The 90/9/1 assignment is permuted across topics and requests. The arithmetic
floors are legal positions but are not necessarily IDs of envelopes on that
topic. They explicitly model unread counts/age, not a sampled production trace.
Old global-floor cases remain labeled separately. Eligibility catalogs are
shared across strategy/request-count controls rather than copied per case, so
increasing topic cardinality does not multiply fixture RAM by the matrix size.

## Frontier correctness and costs

1. Probe the first unread ID on each requested topic, respecting its own cursor.
2. If at least `limit + 1` topics have hits, use the `(limit + 1)`th smallest first
   ID as a ceiling. At least that many distinct matching envelopes exist at or
   below it; therefore the true first `limit + 1` global matches cannot exceed it.
3. Probe at most `limit + 1` rows per topic below that ceiling, retaining strict
   per-topic floors via the already-found first ID, and merge globally.
4. If fewer topics have hits, use bigint's maximum as the ceiling: this is the
   original bounded merge plus first-hit probe overhead, not an exhausted page.
5. Fetch payloads only for the final page. Every stage is one SQL statement and
   snapshot; the extra matching row determines exact `has_more`.

This works with heterogeneous cursors and never scans unrelated global messages.
It adds first-hit probes and sorting, however; clients with few live topics or
already-small histories may lose. There is no dispatch threshold or universal
replacement claim. Reference tests cover both sides of the bound, sparse/absent
topics, mixed cursor ages, zero/exact/overflow pages, payloads, and bigint's last ID.

## Execution and evidence

```sh
export DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:PORT/postgres'
dev/nix-shell 'just backend bench apps/backend/benchmarks/high-cardinality.json'
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-high-cardinality.jsonl > high-cardinality.csv'
```

Use a dedicated test-only PostgreSQL instance, one read connection, bounded
compilation concurrency, and no physical database copies (`write_sizes: []`).
Sizes run serially; the previous corpus is dropped before the next is seeded.
Keep WAL/fsync/synchronous commits and every production index enabled. Reserve
enough disk for the largest corpus plus WAL before starting. Record actual
PostgreSQL settings, relation sizes, revision, query hash, raw samples, empty
requests, and custom/generic plans. The seed remains bounded server-side batches.

These are warm, closed-loop, read-only microbenchmarks. They do not establish
fixed-arrival-rate p99 SLOs, aged-table costs, mixed read/write capacity, a 64-GiB
memory threshold, or 100M-row behavior. Tiny trial differences and unchanged
newest controls expose run noise; report regressions as well as improvements.
Format-3 reporting rejects missing topic caps, mismatched actual populations,
missing cursor/strategy groups, missing plans, and incomplete trials. Earlier
artifacts retain their existing format-1/2 interpretation without reinterpretation.
