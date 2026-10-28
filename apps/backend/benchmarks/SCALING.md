# Large-history query and index experiments

**Status: measured; complete cohorts and limitations published in
[the scaling report](SCALING-RESULTS.md).** The full run was interrupted during
30M covering-index reads; that incomplete cohort is excluded, not reported as
complete. Both paging strategies have complete 10M/30M baseline-index controls,
and the 10M covering read/write comparison is complete. The owner
authorized this next round on October 22, 2026. These are disposable-schema and
test-build experiments, not production migrations or new API guarantees.
The [earlier controls](RESULTS.md) rejected sequence-range partitioning and the
unconditional watermark prefilter as universal replacements.

## Questions and controls

1. Can broad hit-heavy requests stop after a small globally ordered prefix,
   instead of collecting up to `limit + 1` candidates from every topic?
2. Can covering newest metadata avoid its payload-heap lookup, and are insert,
   storage, build, and post-write visibility costs worth the benefit?

`scaling.json` seeds 10M and 30M envelopes with the same skew, payload distribution,
and dormant topics as the baseline. Each corpus is seeded **once**, with all
production constraints and indexes maintained. Both paging algorithms share it;
the metadata-covering index is then installed and the controls repeated without
reseeding. Profile order is fixed, not randomized: report cache/order drift and
unchanged-query controls rather than calling tiny differences causal.

Three five-second trials use 1/100/1,000 requested topics and all cursor ages,
empty controls, newest variants, and a new mostly-missing hit case. Writes at
10M rows use pristine physical copies for both index profiles and 1/8 writers;
30M is explicitly read-only to avoid large write copies. Record actual relation
sizes versus RAM. A larger-than-RAM corpus does not make this a cold-cache,
aged/bloated, fixed-arrival-rate, or concurrent read/write capacity experiment.

## Bounded sequence-window paging

`bounded` retains the exact production query for fewer than 256 requested topics
or heterogeneous per-topic cursors. For broad uniform-cursor requests:

1. Filter impossible topics through their transactional watermarks, in the same
   SQL snapshot as the rest of the page.
2. Materialize at most **4,096 actual envelope rows**, ordered by global sequence ID,
   after the cursor and no later than the largest eligible topic head. Sequence
   gaps do not increase this row budget. Project only ID/topic at this stage.
   This is a prefix-size bound, not a physical-I/O guarantee: verify the ordered
   access path and scanned rows in both custom and generic plans.
3. Keep requested-topic matches in order. If there are `limit + 1`, they are the
   true first `limit + 1` matches: every row outside the scanned prefix has a
   larger sequence ID. Fetch payloads only for the final page.
4. Otherwise run the bounded per-topic candidate query over the same wanted
   topics and snapshot. A short speculative prefix must never be interpreted as
   an exhausted result or used to clear `has_more` incorrectly.

This preserves prefix ordering, strict per-topic cursors, the total page limit,
and exact `has_more`. Sparse matches beyond the scan budget, empty topics,
partitions, gaps, and bigint's final ID are regression-tested against an
independent reference. The scan budget and dispatch threshold are explicit
experimental constants, not deployment tuning recommendations. A fallback can
still add bounded heap work; the rare-hit case measures that cost rather than
assuming a dense global scan is safe for every request.

## Metadata-covering replacement

The versioned benchmark migration replaces the existing `(topic, sequence_id)`
index with the same keys plus `INCLUDE (server_ns, expiry_ns, message_hash,
is_commit_or_proposal)`. It adds no payload bytes to the index and does not
duplicate the old topic/sequence index. Newest SQL remains unchanged, so plans
must demonstrate whether the planner actually uses index-only access.

Index replacement, `VACUUM`, and checkpoint are timed together as setup, not query
latency. Bulk seed time is shared and must not be presented as loading directly
into the covering index. Storage includes every maintained index. Insert trials
include its real maintenance/WAL costs. Column statistics remain those from the
shared seed's `ANALYZE`, avoiding a second random sample between index profiles.

Every write copy also runs untimed warmups, a separate one-second serial
newest-metadata measurement, and both prepared-plan diagnostics **before vacuum**.
The raw `post_write_newest` observations distinguish ideal post-seed visibility
from newly inserted heads; they are neither concurrent read/write load nor part
of insert throughput. The reporter requires every such trial, both plan modes,
and valid raw samples. Do not recommend a covering index from post-vacuum reads
alone.

## Reproduction and reporting

```sh
dev/nix-shell 'just backend bench apps/backend/benchmarks/scaling.json'
dev/nix-shell 'python3 apps/backend/benchmarks/report.py backend-scaling.jsonl > scaling.csv'
```

Use a dedicated PostgreSQL instance and the original durability/settings. Config
format 2 requires independent schema, index-profile, read-strategy, and explicit
write-size selectors. Reports retain profile/strategy identity instead of pooling
different implementations. Legacy format-1 reports still regenerate their
original CSVs byte-for-byte. Full custom/generic plans and post-write metadata
controls remain in the raw artifact.

## References

- [PostgreSQL index-only scans, INCLUDE columns, and visibility](https://www.postgresql.org/docs/18/indexes-index-only-scans.html).
- [PostgreSQL materialized CTE evaluation](https://www.postgresql.org/docs/18/queries-with.html).
- [Backend paging contract](../../../docs/specs/API-backend-api.md).
