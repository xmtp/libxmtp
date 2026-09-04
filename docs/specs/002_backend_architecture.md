# 002: Backend Architecture

Status: approved on 2026-09-04, with the owner decisions in the review record.

Spec 001 defines the public API. This spec defines storage, transaction boundaries, read routing, and service operation. Spec 003 defines validation and trust limits. Spec 004 defines streaming behavior. Requirements use `ARC-nnn`; “must” is required.

## 1. Service and shared logic

- ARC-001: The backend is one Rust binary behind a load balancer. Durable state lives in Postgres. Subscription state, queues, and caches are disposable. Reconnecting clients need no state from the previous instance. An open transport stays on its serving instance; reconnects need no instance affinity.
- ARC-002: Serve gRPC, gRPC-Web, and standard gRPC health on one port. TLS terminates at the trusted load balancer. The plaintext listener accepts HTTP/1.1 and HTTP/2. CORS and proxy behavior must satisfy spec 001. No version or metadata endpoint is served.
- ARC-003: Shutdown stops new requests, fails open streams with `UNAVAILABLE`, and drains unary requests within the configured deadline. A missing response does not prove that a publish rolled back.
- ARC-004: Package the service as a Nix-built container. Builds must not require a live database.
- ARC-005: Use sqlx with compile-time checked queries and a committed offline query cache. Embed migrations and apply them at startup under a database advisory lock. Do not report readiness until migrations and database initialization succeed. Rolling migrations must remain compatible with running binaries; otherwise the upgrade requires a service stop. Concurrent migration exclusion does not establish schema compatibility.
- ARC-010: Storage and transport remain backend concerns. Payload parsing, topic derivation, canonical envelope encoding, hashing, and validation used by clients and the backend are shared. The shared logic must not depend on a client database or client MLS runtime.
- ARC-011: Shared parsing and validation must work on native and WebAssembly clients. Storage supplies the identity history; signature verification uses the existing identity rules. Database transactions must not surround chain RPC calls.
- ARC-012: Shared test fixtures produce valid and malformed payloads of all five kinds without a full client. Backend integration tests use the real service surface and Postgres.
- ARC-013: Preserve the existing identity and key-package validation rules. Keep the SCW verifier cache, including the latest-state fix and limits in spec 003. Do not add new signature, credential, or payload checks as part of this extraction.

## 2. Logical database schema

Use three tables and one global positive bigint sequence. Columns are non-null unless marked nullable. Sequence allocation starts at 1, uses a cache size of 1, and does not cycle. The stored envelope is its canonical protobuf encoding; inner payload bytes remain unchanged.

| Table | Columns | Keys and indexes |
| --- | --- | --- |
| `envelopes` | `sequence_id bigint`, `topic bytea`, `server_ns bigint`, nullable `expiry_ns bigint`, `message_hash bytea`, `is_commit_or_proposal boolean`, `payload bytea` | Primary key `sequence_id`; ordered index `(topic, sequence_id)`; unique index `(topic, message_hash)` |
| `topic_watermark` | `topic bytea`, `last_sequence_id bigint` | Primary key `topic` |
| `identifier_association` | `identifier text`, `identifier_kind smallint`, `inbox_id bytea`, `association_sequence_id bigint`, nullable `revocation_sequence_id bigint` | Primary key `(identifier, identifier_kind, inbox_id)`; lookup index `(identifier, identifier_kind, association_sequence_id DESC)` for rows with no revocation |

- ARC-020: API reads must have an indexed access path. Measure query plans at production row counts in Phase 4; do not require the planner to choose an index for a tiny table. No payload/metadata split, partitioning, or expiry index is required in Phase 2.
- ARC-021: Update each topic watermark in the transaction that inserts its envelopes. It stores the greatest committed sequence ID and supplies replay ceilings. Newest reads join the watermark to the envelope row for complete metadata; they do not duplicate hash, expiry, or commit/proposal fields in the watermark.
- ARC-022: Update the identifier projection in the same transaction as the identity update. Include all identifier kinds except installation keys. Association and revocation sequence guards prevent older updates from replacing newer state. Normalize projection and lookup keys by kind, without changing signed update fields. The latest active association wins; revocation can expose an older active association to another inbox.
- ARC-023: Keep normal vacuum behavior initially. Measure bloat and tune vacuum in Phase 4. Add the expiry index when pruning is implemented in Phase 5.
- ARC-024: Storage permits topics up to 128 bytes. API validation still enforces the current kind-specific lengths. Message hashes are 32 bytes. Enforce configurable payload-size limits in the server, not in a fixed database constraint.
- ARC-025: Null expiry means never; the wire encodes it as zero. Before Phase 5, expiry is metadata only. Do not delete or hide expired rows. Pruning must later define watermark repair, newest absence, retained cursors, and idempotency after deletion.

## 3. Publish ordering and visibility

- ARC-030: Allocate sequence IDs only after all locks required for the publish are held. Allocate in request order after duplicate collapse.
- ARC-031: Hold one transaction advisory lock per distinct topic until commit or rollback. Acquire distinct lock keys in one global order. A hash collision may serialize unrelated topics but must not create inconsistent lock order. Reserve a separate lock domain for global locks.
- ARC-032: Identity publishes first hold a global identity transaction lock. Identity update sequence order therefore equals commit order across inboxes. Other topics retain only per-topic order.
- ARC-033: Per-topic locks make per-topic sequence order equal commit order. Every read from the primary or the single configured replica sees a committed topic prefix. This does not imply that global sequence order equals global commit order.
- ARC-034: Advance each inserted topic's watermark only to a greater sequence ID. An unexpected failure of this guard aborts the whole transaction with `INTERNAL` and an error log. It must not terminate the process or return partial success. Operational metrics are added in Phase 4.
- ARC-035: Read `server_ns` from the database clock inside the insert. Do not read the previous timestamp to clamp it. Equal or backwards timestamps are allowed; sequence IDs, not timestamps, determine order. Check arithmetic when computing finite expiry.
- ARC-036: Publish transactions use `READ COMMITTED`. Duplicate rechecks, identity-head checks, and watermark updates use fresh statements after lock acquisition. Data read before the locks must be rechecked as specified below.

### Closed sequence boundaries

A forward scan above a global cursor can miss a late commit on another topic. A missing sequence value is not proof of rollback. A statement timeout does not bound the full transaction, and replica replay can pause between commits.

- ARC-037: The tailer tracks missing sequence ranges and probes them along with new rows. Gap recovery and forward delivery use one consistent read snapshot. Deliver rows in per-topic sequence order across both sources, including across fetch pages. Do not advance a topic floor past an earlier visible row.
- ARC-038: Retire a missing range only with a closed allocation boundary: database evidence that no transaction can later commit a sequence ID in that range. Time alone must never close a range.
- ARC-039: Establish the boundary with a shared/exclusive allocation barrier. Every publisher takes a shared transaction lock after its topic locks and before sequence allocation. It holds that lock through commit or rollback. A coordinator briefly takes the exclusive lock, reads the sequence high value after acquisition, and records a primary WAL position covering all prior commits. It then releases the barrier. Ordinary publishers remain concurrent.
- ARC-040: On the replica, wait until replay reaches the recorded WAL position. Only a later read snapshot on that replica can retire absent gaps at or below the boundary. On the primary, a post-barrier snapshot suffices. Keep gaps while the barrier or replay check cannot complete. An unused sequence has boundary zero. No writer may bypass the barrier, cache sequence allocations, or rewind the sequence.
- ARC-041: Gap ranges and the forward cursor are instance-local recovery state. A database connection loss, instance restart, or loss of required recovery state fails affected streams and resets the tailer. Establish a new boundary before accepting subscriptions. History replay covers rows at or below that initial boundary. Do not infer a safe resume point from a timestamp or an unproved maximum row ID.
- ARC-042: Bound barrier waits and recovery memory. Capacity failure ends affected streams with `RESOURCE_EXHAUSTED`; database failure uses `UNAVAILABLE`. Never discard unknown gaps to remain within a budget. Keep gaps as ranges, not one allocation per absent integer.

The barrier adds no marker table and no per-publish bookkeeping rows. It can briefly delay sequence allocation while a boundary is captured. This cost must be measured in Phase 4. Retirement on a replica needs replay evidence even when normal lag is short.

## 4. Publish transaction

The service follows this order:

1. Parse and size-check the request. Derive topics and canonical hashes. Keep each original input index.
2. Collapse identical `(topic, message_hash)` pairs. Reject distinct identity updates for the same inbox. Identical copies still collapse under spec 001's duplicate rule.
3. Read existing duplicate metadata. Exclude known duplicates from validation, but retain their response positions.
4. Validate survivors without locks. For each identity update, read one complete history snapshot. Its validation position is the greatest sequence ID in that history, or zero. Preserve validation errors until the final duplicate check.
5. Begin the publish transaction. Acquire identity, topic, and allocation-barrier locks as applicable. Recheck duplicates in a fresh statement. A concurrent duplicate succeeds even if its earlier validation failed. For each remaining identity update, compare the current watermark with its validation position; a change aborts the request with `ABORTED`.
6. Reject remaining validation errors at the lowest original failing index. Reject a new identity update when the validated prior history has at least 256 entries. Duplicate retries still succeed at that cap. The head comparison proves that the validated history length is still current.
7. Insert all new envelopes in request order. Assign sequence IDs, timestamps, and expiry. Advance topic watermarks and apply identity projection changes in the same transaction. Commit once.
8. Return one metadata entry per input, in input order, combining duplicate and new rows.

- ARC-050: The locked duplicate check handles normal concurrent duplicate publishers. An unexpected unique constraint error aborts the whole transaction and returns `INTERNAL`. Do not translate every uniqueness error into duplicate success. A retry, if any, repeats the whole transaction.
- ARC-051: Apply a publish statement timeout and a bounded request lifetime. Cancellation must release transaction resources. Neither timeout is used as proof for sequence-gap retirement.
- ARC-052: Keep one publish implementation in the backend's language. Do not split the business rules between application code and database procedures.
- ARC-053: A response-size error may occur after commit. Return an error as required by spec 001; response-size preflight is not required. Never claim rollback solely because the response could not be delivered.

## 5. Read routing and queries

Replicas are supported from day one. Each configured replica URL names one physical streaming-replication instance, not a pool of independently lagging replicas. Normal lag is expected to be short. No multi-replica selection protocol is required.

| Operation | Database |
| --- | --- |
| Publish, including validation history | Primary |
| Query | Primary |
| QueryNewest | Replica, or primary when none is configured |
| Subscribe and SubscribeStatic, replay and live | Replica, or primary when none is configured |
| GetInboxIds | Replica, or primary when none is configured |
| VerifySmartContractWalletSignatures | Configured chain RPC |

- ARC-060: Use one pool when there is no replica. With a replica, express endpoint routing once. After a replica connection loss, restart stream recovery; do not retain unproved tailer state across a database replacement.
- ARC-061: Query uses bounded per-topic index probes, with a per-topic `limit + 1` and a final total `limit + 1` cut to compute `has_more` from the same snapshot. Choose candidate IDs before loading payloads. Worst-case candidate work is topics times `limit + 1`; no fixed latency is promised without measurement.
- ARC-062: A nonempty successful page advances at least one requested topic cursor. Leave other cursors unchanged. Coalesce duplicate inputs as spec 001 requires. The loop drains a finite result set; continuous publication need not terminate a paging loop.
- ARC-063: Newest reads join watermarks and envelopes in one statement. Metadata-only results select all metadata but not the payload. Full results add the payload. Do not omit an existing topic to fit a successful response into the byte limit.
- ARC-064: Identity validation reads complete history from one snapshot. Reads still work if stored history exceeds the write cap. Do not use a separately read head as the validation position.
- ARC-065: Identifier lookup selects the greatest association sequence ID among active associations for each normalized `(identifier, kind)`. Reconstruct the positional response, including duplicate inputs.
- ARC-066: Oversized responses eventually fail with `RESOURCE_EXHAUSTED`, including transport-level rejection. No additional pagination protocol or structured size-error type is required.

## 6. Streaming architecture

- ARC-070: One tailer per instance polls the selected read database. No LISTEN/NOTIFY path is required. It supplies one topic registry shared by all sessions and follows the closed-boundary rules above.
- ARC-071: Poll at a fixed interval, default 100 ms. Polls do not overlap. Schedule the next wait after the previous poll completes. Drain full pages without an extra interval. Trace duration and row count.
- ARC-072: Use the existing topic types and a standard registry representation first. Do not prescribe a shard count, inline key layout, custom hasher, or exact bytes per topic before measurement.
- ARC-073: Batch dispatch by stream and use non-blocking sends. A slow stream must not block the global tailer. Immutable payload storage may be shared across stream deliveries.
- ARC-074: One session owner controls cursor floors, topic ownership, waves, and keepalive state. Replay, live delivery, and control frames obey spec 004. Fetch completions from replaced ownership cannot emit new messages.
- ARC-075: Use one per-topic floor for duplicate suppression across replay, fold, and live delivery. Advance it only when delivery is admitted in order. A removed or replaced topic clears the old floor.
- ARC-076: Register and gate live topics before reading replay ceilings. Read ceilings from the same selected database used for replay. Skip topics already at or above their ceiling; the ceiling lookup itself is still a database read. Replay only through each captured ceiling, using rotating bounded topic scans.
- ARC-077: Share bounded replay fetch workers across sessions. Bound fetched-but-unconsumed bytes, default 64 MiB per stream. A row-count turn is not permission to allocate its full possible payload volume. The progress exception admits one legal envelope with framing, not an arbitrary oversized turn.
- ARC-078: Buffer live arrivals for gated topics within the pending-byte budget, default 64 MiB. At wave completion, fold them in sequence order, suppress duplicates, and follow spec 004's output ordering. Superseded work loses ownership; its mutation still receives an acknowledgement.
- ARC-079: Target delivery frames of 2 MiB, including framing. Every permitted envelope must fit a frame and the transport cap. Never skip a large envelope while advancing its cursor.
- ARC-080: Bound outbound queues by frame count and bytes, defaults 64 frames and 16 MiB. Pause replay when it can wait within its budgets. If required live or outbound data cannot be retained, fail that stream with `RESOURCE_EXHAUSTED`, including during catch-up. Do not silently discard data.
- ARC-081: Use separate send-idle and pong-deadline timers. Start the pong deadline when the ping is handed to the transport, not when it is merely waiting behind queued data. Only a matching nonce clears it. Before timeout, process already received inbound frames without blocking. Backpressure has its own failure path.
- ARC-082: Every exit deregisters the stream and cancels its work. Tailer failure fails affected streams; keepalives alone must not make a broken delivery path appear healthy.
- ARC-083: Native request half-close drains accepted waves and closes within the configured deadline. Continuous live traffic must not prevent termination. An incomplete drain returns `DEADLINE_EXCEEDED`.
- ARC-084: Static subscriptions use the same replay/live engine with an explicit static mode, one wave, and one-way keepalives. Ending their unary request does not trigger native half-close.
- ARC-085: Reject an empty inbound oneof with `INVALID_ARGUMENT`. Apply the two stream token buckets in spec 001. No caller quota is added in Phase 2.

## 7. Validation and cache

- ARC-090: Validation errors are typed through the shared logic and map to the spec 001 reason codes. Do not classify errors by matching message substrings.
- ARC-091: Preserve retryability from the identity verifier, including provider, I/O, and missing-verifier failures. A chain failure is not an invalid identity signature.
- ARC-092: Preserve existing validation behavior, including group-message trailing bytes and the absence of an added ciphersuite allow-list. Spec 003 records what checks do and do not run.
- ARC-093: Fold identity history for each publish without an association-state cache. Historical signature conversion can call chain RPC; only the state fold itself is pure CPU work. Keep the separate SCW signature-verdict cache defined in spec 003.

## 8. Configuration

- ARC-100: Read one TOML file selected by `--config`. The database URL is required; other values have the defaults below. Publish a JSON schema usable by Taplo. Reject unknown keys, invalid values, and inconsistent size relationships at startup. Private implementation constants do not need config keys.
- ARC-101: A string of the form `env:NAME` reads that environment variable at startup. A missing variable fails startup. Error messages and logs must not include resolved secrets.
- ARC-102: Each public limit has one named config value. Server-only settings stay with the server; values shared with clients have one shared definition. A lower deployment limit can require smaller client batches. Identity and commit retention exemptions cannot be disabled by a finite duration setting.

```toml
#:schema https://xmtp.org/schemas/backend-v1.json
[server]
listen = "0.0.0.0:5050"
drain_timeout_ms = 10000

[database]
url = "env:XMTP_DATABASE_URL"
# replica_url = "env:XMTP_REPLICA_URL" # optional; one replica instance
max_connections = 20                 # per pool
statement_timeout_ms = 5000
publish_timeout_ms = 30000
barrier_wait_ms = 1000

[streams]
poll_interval_ms = 100
frame_bytes = 2097152
outbound_frames = 64
outbound_bytes = 16777216
pending_bytes = 67108864
fetched_bytes = 67108864
gap_range_limit = 10000
keepalive_ms = 30000
pong_timeout_ms = 90000

[retention]
group_message_ns = 7776000000000000  # 90 days, except commits/proposals
welcome_ns = 7776000000000000
key_package_ns = 7776000000000000

[chains]
# "eip155:1" = "env:XMTP_RPC_MAINNET" # operator-selected routes

[validation]
scw_cache_entries = 10000

[limits]
query_topics = 1000
query_default = 100
query_max = 1000
newest_meta_topics = 1000
newest_full_topics = 100
publish_topics = 1000
envelope_bytes = 1048576
request_bytes = 26214400
response_bytes = 26214400
mutate_adds = 100000
mutate_removes = 100000
stream_topics = 100000
static_topics = 10000
waves = 256
lookup_identifiers = 250
scw_signatures = 100                 # per verify request or identity update
identity_entries = 256              # new writes only
http2_streams = 100                 # per connection
mutate_frames_per_second = 10
mutate_burst = 100
ping_frames_per_second = 10
ping_burst = 100
```

The schema URL is the publication target, not a claim that the schema is already hosted. Publish and validate it with the backend config implementation. An empty chain map supports non-SCW identities; SCW operations on an unconfigured chain return `UNAVAILABLE`. A minimal deployment therefore needs only the database URL, while SCW support also needs chain routes. The statement and request timeouts bound work; gap correctness does not depend on their values.

## 9. Verification and phase limits

- ARC-110: Backend integration tests exercise real Postgres and the real service surface with stateless fixtures. Cover each endpoint's happy path, errors, and limits. Reuse the project's test macro, generators, fault injection, clocks, task handles, and typed errors.
- ARC-111: Concurrency tests cover topic ordering, identical publish races, mixed duplicate/new failures, and identity history changing during validation. A full duplicate at the identity cap succeeds. Unexpected uniqueness errors never produce partial success.
- ARC-112: Exercise late commits beyond the former gap timeout, replica replay pauses, gap/forward snapshot races, and startup during an open publish. Each committed row is delivered in topic order or the affected stream explicitly fails before recovery state is lost.
- ARC-113: Cover replay replacement, removal, native half-close, static continuation, slow consumers, large envelopes, oversized responses, and tailer/database failure. Browser transport tests include preflight and unbuffered delivery through the supported proxy.
- ARC-114: Test each important behavior once on its owning platform. Backend tests own protocol semantics; binding tests own conversion and SDK lifecycle. Phase 4 owns benchmarks, vacuum tuning, and full telemetry. Phase 5 owns pruning. Phase 6 owns caller authentication and quotas.

Supported database failover must preserve acknowledged commits and fence the old primary. Promoting a replica that loses acknowledged data or restoring an old backup is an operator recovery event; durable client cursors cannot repair it. The backend does not implement a database failover manager.

## Review record

- [Original architecture draft](https://plan.ref.tools/c8yzIJ4kaAAmrqZE).
- [Approved review and owner decisions](https://plan.ref.tools/xWi9jEu8VHmuLI0W): keep replicas, timestamps from the database clock, existing validation, the SCW cache, and stream token buckets. Use simple response-size errors. Replace gap expiry with a proven allocation boundary and fix the identity snapshot and newest metadata rules.
