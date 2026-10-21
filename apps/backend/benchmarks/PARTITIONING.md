# Sequence-range partitioning experiment

**Status: approved October 21, 2026; implementation is a benchmark-only experiment.**
The versioned experimental migration is in `benchmarks/migrations/`, deliberately
outside production's migration directory. Provisioning and Store experiment hooks
are compiled only for tests. Production startup and schema remain unchanged.
The benchmark suite is implemented independently. This experiment preserves the
existing API, ordering, retention, and idempotency requirements; it proposes no
spec changes. Approval authorizes the disposable-schema experiment below, not
an unreviewed production migration or rollout.

## Hypothesis and decision

Partition `envelopes` by fixed ranges of **1,000,000 sequence IDs**, not rows.
Most recent-cursor queries should need only the active range(s); older history
and cursor-free queries may pay extra planning and per-partition probe costs.
This is a hypothesis to test, not an assumed improvement over the existing
`(topic, sequence_id)` index. The existing indexed probes already avoid scanning
all history for most queries. Partitioning could lose on inserts and no-hit reads.

The approved design uses range-partitioned envelope payloads,
a global idempotency registry, and automatic creation outside publish locks.
Reject adopting it if the benchmark gains do not justify the registry, WAL,
DDL, migrations, and operational complexity.

## Layout and correctness

Keep `envelopes` as the logical table with its existing columns/checks,
`PARTITION BY RANGE (sequence_id)`, `PRIMARY KEY (sequence_id)`, the local
`(topic, sequence_id)` index, and the partial push-eligible sequence index.
Ranges are half-open:

| Child | Bounds |
| --- | --- |
| `envelopes_p0` | `[1, 1,000,001)` |
| `envelopes_p1` | `[1,000,001, 2,000,001)` |
| `envelopes_p9` | `[9,000,001, 10,000,001)` |

Use checked arithmetic from `(sequence_id - 1) / 1,000,000` and `MAXVALUE`
for the final upper bound so bigint's maximum valid sequence ID remains valid.
There is no DEFAULT partition. Sequence gaps and aborted publishes do not
change boundaries; batches can span multiple ranges. Keep the existing sequence,
topic locks, allocation barrier, closed boundary, and topic watermarks.

### Global idempotency is the hard part

PostgreSQL requires every partitioned unique key to include the partition key.
Thus `UNIQUE (topic, message_hash)` cannot live on a table range-partitioned by
sequence ID. Replacing it with `(topic, message_hash, sequence_id)` would allow
the same message to be stored twice in different partitions and is unacceptable.

Proposed global registry:

```sql
CREATE TABLE envelope_identity (
    topic bytea NOT NULL,
    message_hash bytea NOT NULL,
    sequence_id bigint NOT NULL UNIQUE,
    PRIMARY KEY (topic, message_hash),
    FOREIGN KEY (sequence_id) REFERENCES envelopes (sequence_id)
        ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
);
```

Preserve the existing topic/hash/positive-ID checks on this table too. A parent
`BEFORE INSERT FOR EACH ROW` trigger inserts the key into the registry; its
unique constraints reject a conflicting key even across partitions or direct
SQL writes. PostgreSQL clones parent row triggers to new/attached partitions.
An envelope insert failure rolls back its registry insertion. The deferred FK
allows registration before the envelope row exists but forbids an orphan at
commit; pruning deletes the registry row through the FK cascade. The storage
contract remains append-only: deny updates of sequence/topic/hash through the
writer role and reject such key changes with a trigger, including child writes.
Treat registry rows as internal constraint state: the writer role must not
insert, update, or delete them directly. Use a narrowly owned `SECURITY DEFINER`
registration function with a fixed safe search path and schema-qualified names;
the writer must not own the tables or be able to disable triggers. Protect
registry key updates too. Trusted migration/provisioning privileges remain
separate; validate complete one-to-one registry coverage after any backfill.

The existing preliminary and locked duplicate checks probe the registry's
global key, then join `envelopes` by the exact sequence ID to retrieve the
original metadata. Duplicate retry behavior and retention/pruning semantics
remain unchanged. Do not keep registry keys forever after their envelope is
legitimately pruned: that would change existing republish behavior.

This costs an additional heap plus two global indexes, FK checks, trigger work,
and WAL. Removing the old global envelope uniqueness index recovers only part
of that cost. **Measure total registry + partition storage/WAL and insert
throughput, not just the apparently smaller child indexes.** The registry may
erase any write benefit; a small toy test without it is not a valid comparison.

## Automatic range creation

Do not create partitions from a row trigger: routing to a missing partition
fails before such a trigger can solve it. Do not upgrade parent locks while
holding topic/allocation/publish locks: concurrent writers can deadlock.

1. Startup creates/verifies the initial required range(s), not every possible
   future range. The provisioner uses a dedicated primary connection and a new,
   documented advisory-lock key distinct from existing global lock domains.
2. The publish transaction locks topics, rechecks duplicates/identity state,
   allocates the actual IDs, and checks that every touched range exists before
   inserting any rows. Existing ranges need no DDL. A verified range cache can
   avoid catalogue work; it must be invalidated after reconnect/schema changes.
3. If a range is missing, return an internal typed `MissingEnvelopePartition`
   outcome, **roll back and release all publish locks**, and provision through
   the dedicated connection. The aborted IDs are gaps, as already permitted.
4. Serialize provisioners with the advisory lock; recheck the actual catalogue,
   bounds, schema, indexes, and registry trigger after acquiring it. A name
   existing is not proof of the right partition. Create a standalone child with
   matching columns/indexes and a validated bound CHECK, then `ATTACH PARTITION`.
   This avoids `CREATE TABLE ... PARTITION OF`'s parent ACCESS EXCLUSIVE lock;
   attach uses SHARE UPDATE EXCLUSIVE on the parent. Include the FK-referencing
   registry's locks in deadlock/rollover tests; attach is not lock-free.
5. Retry the **whole** publish, including duplicate and identity-head checks,
   under the original total request deadline. Use bounded lock/statement
   timeouts and a bounded retry budget, with cancellation cleanup. Never hide
   failures as success, reuse stale validation heads, or spin indefinitely.
6. This first experiment uses the missing-range fallback without speculative
   lookahead or a range cache, making catalogue and rollover costs explicit.
   Future variations may precreate the next range outside publish transactions.
   Creation is driven by inserts, not a required external scheduler. Readers
   remain read-only on replicas.

Use least-privilege, schema-qualified provisioning SQL, validated numeric
identifiers, an explicit migration/provisioning role, and no caller-controlled
DDL identifiers. Provisioned-but-empty partitions surviving a failed publish
are harmless. Observability should report creations, timeouts/retries, and
boundary stalls without topics or payloads. Any new public operational
guarantee/configuration must first follow the repository's Ref/spec process.

## Queries and retention

- Initially keep the existing query shapes unchanged. Per-topic cursors in the
  lateral probes are parameters: **verify runtime pruning**, particularly with
  generic prepared plans, rather than assuming constant-bound pruning applies.
- The final page joins by sequence ID, and newest joins use each watermark's
  exact sequence ID. Confirm actual child execution/loop counts for these joins
  too; a plan scanning every child can erase the candidate-probe improvement.
- Keep newest's watermark shortcut. Do not replace it with a scan/sort merely
  to make partitioning seem necessary. Measure metadata and full payload paths.
- Old/no-cursor, absent-topic, sparse recent, caught-up, and long-idle newest
  cases remain in the comparison. Preserve snapshot-consistent `has_more`,
  total limits, per-topic prefix/order, read-your-writes, and replica routing.
- **Do not drop old sequence partitions for retention.** Sequence age is not
  expiry: a range can contain non-expiring commits/proposals/identity state.
  Existing row-level pruning and watermark behavior remain in force. Test push,
  stream gap probes, identity reads, and closed-boundary scans as well as RPC reads.

## Experimental migration and seed strategy

Apply only the **new versioned experimental migration** and corresponding
test-only Store hooks to disposable fixtures. Never edit the existing migrations.
For large corpus seeding, precreate all ranges touched by the known seed range
outside the timer, then use the same bulk seed and real registry enforcement.
For the steady-state insert comparison, also precreate ranges covering the
configured write-growth budget and report their setup cost/count separately.
This is important at exactly 10,000,000 seeded IDs: the first write enters a
new range, and untimed warmup must not silently hide its creation. Dedicated
timed rollover trials must instead start without that next range and include
the complete provision-and-retry latency.
Do not disable the trigger, omit indexes, or synthesize empty registry state.
Report the slower partitioned seed time separately if trigger/FK cost is material.

A production rollout is a separate approval: create a shadow partitioned root,
backfill bounded sequence ranges, keep new writes synchronized transactionally,
verify hashes/counts/registry mappings, and perform a brief lock-bounded name
swap with preserved sequence ownership/watermarks and rebuilt SQLx metadata.
An ordinary PostgreSQL table is not converted in place with `ALTER ... PARTITION`.
Dual-write failures, in-flight transactions, replica replay, rollback/cutover
semantics, disk headroom, and permissions require a production migration plan;
do not silently substitute an unbounded table rewrite at backend startup.

## Follow-up acceptance experiment

1. Re-run exactly the committed baseline config on the same PostgreSQL version,
   hardware/settings, seed, payload distribution, page limits, warmup, and trial
   order. Keep original raw output; produce a separate partitioned JSONL/CSV.
   Compare per-case trial medians/ranges, p95/p99, rows/bytes, WAL and total storage.
2. Inspect custom/generic plans for actual child pruning and planning overhead.
   Report absolute milliseconds and percent changes, not only a speedup ratio.
3. Add timed rollover trials starting just below 1,000,001 and 10,000,001. Measure
   first creation, precreated steady state, batches spanning an edge, concurrent
   process provisioners, sequence gaps, and a large sequence jump. Do not hide
   rollover DDL in normal untimed write warmup.
4. Add regression tests for retries on either side of an edge, cross-partition
   duplicates, forced SQL duplicate writes, denied direct registry mutation,
   immutable keys on parents/children/registry, cancellation/DDL failure, FK/trigger
   rollback and prune cleanup, wrong child bounds, restart recovery, maximum
   bigint IDs, old cursor paging, empty reads, newest on dormant topics, replica
   reads, push boundaries, and the existing publish/stream/retention suites.
5. Before deployment, extend measurement to realistic concurrent read/write
   mixes, a sustained fixed-arrival-rate test, and an aged/larger-than-RAM corpus.
   The current closed-loop baseline cannot predict production overload tails.

Accept only if repeated results demonstrate a useful recent-cursor benefit
without unacceptable regressions in insert throughput/WAL, no-hit and history
queries, newest, and rollover latency. Define those budgets with deployment
owners before adoption; do not select numerical thresholds after seeing results.

## Running the approved experiment

Use `partitioned.json` with the same recipe and dedicated PostgreSQL instance as
the baseline. Its required `layout` is `sequence_ranges`; `baseline.json` explicitly
selects `unpartitioned`. The experimental migration refuses nonempty tables.
The admin connection needs database/role creation privileges. Provisioning uses
that separate owner connection; measured reads/publishes authenticate directly
as `xmtp_bench_writer`, which has no elevated privileges or role memberships.
Configure local authentication to allow this dedicated synthetic-data role;
the runner does not modify `pg_hba.conf` or authentication settings.
The writer URL removes the administrator's password; provision writer-specific
credentials through PostgreSQL's standard password-file mechanism if needed.
Ordinary correctness tests instead authenticate using their existing fixture
credentials and `SET ROLE` on a separate writer pool, so Docker/password-backed
test setups need no authentication changes. Those tests validate effective-role
SQL permissions; the release benchmark separately verifies that its session user
really is the restricted login, without an administrative login behind it.

Each corpus precreates ranges covering its seed and full write-growth budget,
recording setup separately. Real registration triggers, registry indexes, and
deferred foreign keys remain enabled during bulk seeding and measurement.
Storage records distinguish payload/index bytes from the additional registry;
compare `envelope_and_identity_bytes`, not just the partitioned envelope relation.
Raw output includes a checksum of the versioned experimental migration.

Additional `rollover` records measure missing-range creation versus precreated
controls at 1,000,001 and 10,000,001 with one/eight writers and three trials.
They use fresh 1,000-row fixtures with sparse IDs: these isolate first-use DDL,
retry, and coordination costs, not populated ten-million-row rollover behavior.
No write warmup hides the missing partition in these trials. Do not pool the
few first-use samples into a steady-state tail-latency claim.

## References

- [PostgreSQL 18 partitioning: constraints, pruning, and attach locks](https://www.postgresql.org/docs/18/ddl-partitioning.html)
- [PostgreSQL 18 CREATE TABLE: partition bounds and cloned triggers](https://www.postgresql.org/docs/18/sql-createtable.html)
- Existing contracts: [backend API](../../../docs/specs/API-backend-api.md),
  [operations/retention](../../../docs/specs/OPS-backend-operations.md), and
  [push delivery](../../../docs/specs/PUSH-push-subscriptions.md).
