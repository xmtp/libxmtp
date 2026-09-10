# 002: Backend Architecture

Status: approved on 2026-09-04, with the owner decisions in the review record.

Telemetry amendment reviewed on 2026-09-09: [Phase 4.5: Metrics And Telemetry](https://plan.ref.tools/0cBzvHLvgoCqyGmp).

Spec 001 defines the public API. This spec defines storage, transaction boundaries, read routing, and service operation. Spec 003 defines validation and trust limits. Spec 004 defines streaming behavior. Requirements use `ARC-nnn`; “must” is required.

## 1. Service and shared logic

- ARC-001: The backend is one Rust binary behind a load balancer. Durable state lives in Postgres. Subscription state, queues, and caches are disposable. Reconnecting clients need no state from the previous instance. An open transport stays on its serving instance; reconnects need no instance affinity.
- ARC-002: Serve native gRPC, gRPC-Web, and standard gRPC health directly through Tonic on one port. TLS terminates at the trusted load balancer, which passes HTTPS requests through without gRPC-Web conversion. The plaintext listener accepts HTTP/1.1 and HTTP/2. CORS and proxy behavior must satisfy spec 001. No version or metadata endpoint is served. The metrics listener uses a separate port. The gRPC port serves no additional endpoint.
- ARC-003: Shutdown stops new requests, fails open streams with `UNAVAILABLE`, and drains unary requests within the configured deadline. A missing response does not prove that a publish rolled back.
- ARC-004: Package the service as a Nix-built container. Builds must not require a live database.
- ARC-005: Use sqlx with compile-time checked queries and a committed offline query cache. Embed migrations and apply them at startup under a database advisory lock. Do not report readiness until migrations and database initialization succeed.
- ARC-006: Through completion of Phase 6, keep the backend schema in one mutable migration. The service has no permanent deployments during these phases. Edit the existing migration as the schema changes; incremental upgrade migrations and compatibility with earlier development schemas are not required. Recreate disposable development and test databases when the migration changes. Service startup must not automatically delete a database to apply an edited migration. At completion of Phase 6, freeze the migration. Subsequent schema changes use new migrations. Rolling migrations must then remain compatible with running binaries; otherwise the upgrade requires a service stop. Concurrent migration exclusion does not establish schema compatibility.
- ARC-007: Target PostgreSQL 18 for the backend schema, development and test databases, physical-replica tests, and CI.
- ARC-010: Storage and transport remain backend concerns. Database operations use internal records, opaque payload bytes, and typed errors, not wire messages or transport statuses. The API boundary owns wire conversion. Payload parsing, topic derivation, canonical envelope encoding, hashing, and validation used by clients and the backend are shared. The shared logic must not depend on a client database or client MLS runtime.
- ARC-011: Shared parsing and validation must work on native and WebAssembly clients. Storage supplies the identity history; signature verification uses the existing identity rules. Database transactions must not surround chain RPC calls.
- ARC-012: Shared test fixtures produce valid and malformed payloads of all five kinds without a full client. Backend integration tests use the real service surface and Postgres.
- ARC-013: Preserve the existing identity and key-package validation rules. Keep the SCW verifier cache, including the latest-state fix and limits in spec 003. Do not add new signature, credential, or payload checks as part of this extraction.

## 2. Logical database schema

Use three domain tables, one singleton boundary table, and one global positive bigint sequence. Columns are non-null unless marked nullable. Sequence allocation starts at 1, uses a cache size of 1, and does not cycle. The stored envelope is its canonical protobuf encoding; inner payload bytes remain unchanged.

| Table | Columns | Keys and indexes |
| --- | --- | --- |
| `envelopes` | `sequence_id bigint`, `topic bytea`, `server_ns bigint`, nullable `expiry_ns bigint`, `message_hash bytea`, `is_commit_or_proposal boolean`, `payload bytea` | Primary key `sequence_id`; ordered index `(topic, sequence_id)`; unique index `(topic, message_hash)` |
| `topic_watermark` | `topic bytea`, `last_sequence_id bigint` | Primary key `topic` |
| `identifier_association` | `identifier text`, `identifier_kind smallint`, `inbox_id bytea`, `association_sequence_id bigint`, nullable `revocation_sequence_id bigint` | Primary key `(identifier, identifier_kind, inbox_id)`; lookup index `(identifier, identifier_kind, association_sequence_id DESC)` for rows with no revocation |
| `allocation_boundary` | `singleton boolean`, `closed_sequence_id bigint` | One row with a fixed singleton key; `closed_sequence_id` is nonnegative and starts at zero |

- ARC-020: API reads must have an indexed access path. Measure query plans at production row counts in Phase 4; do not require the planner to choose an index for a tiny table. No payload/metadata split, partitioning, or expiry index is required in Phase 2.
- ARC-021: Update each topic watermark in the transaction that inserts its envelopes. It stores the greatest committed sequence ID and supplies fixed catch-up targets. Newest reads join the watermark to the envelope row for complete metadata; they do not duplicate hash, expiry, or commit/proposal fields in the watermark.
- ARC-022: Update the identifier projection in the same transaction as the identity update. Include all identifier kinds except installation keys. Association and revocation sequence guards prevent older updates from replacing newer state. Normalize projection and lookup keys by kind, without changing signed update fields. The latest active association wins; revocation can expose an older active association to another inbox.
- ARC-023: Keep normal vacuum behavior initially. Measure bloat and tune vacuum in Phase 4. Add the expiry index when pruning is implemented in Phase 5.
- ARC-024: Storage permits topics up to 128 bytes. API validation still enforces the current kind-specific lengths. Message hashes are 32 bytes. Enforce configurable payload-size limits in the server, not in a fixed database constraint.
- ARC-025: Null expiry means never; the wire encodes it as zero. Before Phase 5, expiry is metadata only. Do not delete or hide expired rows. Pruning must later define watermark repair, newest absence, retained cursors, and idempotency after deletion.

## 3. Publish ordering and visibility

- ARC-030: Allocate sequence IDs only after all locks required for the publish are held. Allocate in request order after duplicate collapse.
- ARC-031: Hold one transaction advisory lock per distinct topic until commit or rollback. Acquire distinct lock keys in one global order. A hash collision may serialize unrelated topics but must not create inconsistent lock order. Reserve a separate lock domain for global locks.
- ARC-032: Identity publishes first hold a global identity transaction lock. Identity update sequence order therefore equals commit order across inboxes. Other topics retain only per-topic order.
- ARC-033: Per-topic locks make per-topic sequence order equal commit order. Every read from the primary or the single configured replica sees a committed topic prefix. This does not imply that global sequence order equals global commit order.
- ARC-034: Advance each inserted topic's watermark only to a greater sequence ID. An unexpected failure of this guard aborts the whole transaction with `INTERNAL` and an error log. It must not terminate the process or return partial success. Count storage invariant failures in database error metrics.
- ARC-035: Use the database transaction-start timestamp (`CURRENT_TIMESTAMP`) for `server_ns`. New rows in one publish transaction share that value, including when insertion waits for locks. Do not read the previous timestamp to clamp it. Equal or backwards timestamps are allowed; sequence IDs, not timestamps, determine order. Check arithmetic when computing finite expiry.
- ARC-036: Publish transactions use `READ COMMITTED`. Duplicate rechecks, identity-head checks, and watermark updates use fresh statements after lock acquisition. Data read before the locks must be rechecked as specified below.

### Closed sequence boundaries

A forward scan above a global cursor can miss a late commit on another topic. A missing sequence value is not proof of rollback. A statement timeout does not bound the full transaction, and replica replay can pause between commits.

- ARC-037: The tailer tracks missing sequence ranges and probes them along with new rows. Gap recovery and forward delivery use one consistent read snapshot from the selected read database. Deliver rows in per-topic sequence order across both scan results, including across fetch pages. Do not advance a topic floor past an earlier visible row. Within one poll the forward page and the gap probes are disjoint (gap ranges lie strictly below the forward cursor), and across polls a row is read once, so the tailer never emits a row twice; the per-topic floor of ARC-075 is the only duplicate guard downstream.
- ARC-038: Retire a missing range only with a closed allocation boundary: database evidence that no transaction can later commit a sequence ID in that range. Time alone must never close a range.
- ARC-039: Establish the boundary with a shared/exclusive allocation barrier. Every publisher takes a shared transaction lock after its topic locks and before sequence allocation. It holds that lock through commit or rollback. A separate boundary-maintenance task uses the primary in a short transaction: it takes the exclusive lock, reads the sequence high value after acquisition, updates the singleton `allocation_boundary` row, commits, and then releases the barrier. Ordinary publishers remain concurrent. The application never accesses WAL. There is no leader election or per-publish bookkeeping; any instance may run the boundary task.
- ARC-040: The boundary task runs immediately at startup and on coalesced in-process maintenance requests when unresolved gaps need a newer boundary, at most once per poll interval. One pending bit bounds request storage; duplicate signals add no work. A lock timeout retains the request for a later attempt. The tailer does not wait for maintenance during ordinary delivery and does not run barrier writes. An unused sequence has boundary zero. The closed boundary proves only that an absent gap can be retired; it is not a live-delivery ceiling. Visible rows above it are eligible for delivery. No writer may bypass the barrier, cache sequence allocations, or rewind the sequence.
- ARC-041: Gap ranges and the forward cursor are instance-local recovery state. At startup, the supervisor obtains B from the primary boundary task and the tailer waits until the selected read database exposes a boundary at least B before accepting subscriptions. History replay covers rows at or below that initial boundary. A database connection loss, instance restart, or loss of required recovery state fails affected streams and resets the tailer. Do not infer a safe resume point from a timestamp or an unproved maximum row ID.
- ARC-042: Bound barrier waits and recovery memory. Capacity failure ends affected streams with `RESOURCE_EXHAUSTED`; database failure uses `UNAVAILABLE`. Never discard unknown gaps to remain within a budget. Keep gaps as ranges, not one allocation per absent integer.

The boundary uses one replicated singleton row and no per-publish bookkeeping rows. Every publisher that allocated at or below B settled before the exclusive barrier was acquired, and new allocations cannot begin until that barrier transaction commits. Replication preserves commit order, so a replica snapshot that sees the boundary row at B also sees all earlier committed envelope rows. No WAL position is required. The barrier can briefly delay sequence allocation while the boundary is captured. This cost must be measured in Phase 4. Retirement on a replica needs visibility of the replicated row even when normal lag is short.

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
- ARC-051: Apply a publish statement timeout and a bounded request lifetime. Enforce the publish lifetime in Postgres as well as the service, so a cancelled SQL future cannot keep database locks past that lifetime. Cancellation must release transaction resources. No timeout is used as proof for sequence-gap retirement.
- ARC-052: Keep one publish implementation in the backend's language. Do not split the business rules between application code and database procedures.
- ARC-053: A response-size error may occur after commit. Return an error as required by spec 001; response-size preflight is not required. Never claim rollback solely because the response could not be delivered.

## 5. Read routing and queries

Replicas are supported from day one. Each configured replica URL names one physical streaming-replication instance, not a pool of independently lagging replicas. Normal lag is expected to be short. No multi-replica selection protocol is required.

| Operation | Database |
| --- | --- |
| Publish, including validation history | Primary |
| Query | Primary |
| QueryNewest | Replica, or primary when none is configured |
| Get | Replica, or primary when none is configured |
| Subscribe and SubscribeStatic, replay and live | Replica, or primary when none is configured |
| GetInboxIds | Replica, or primary when none is configured |
| VerifySmartContractWalletSignatures | Configured chain RPC |

- ARC-060: Use one pool when there is no replica. With a replica, express endpoint routing once. A pool release hook must roll back open transactions and discard connections with aborted transactions before reuse. After a replica connection loss, restart stream recovery; do not retain unproved tailer state across a database replacement.
- ARC-061: Query uses bounded per-topic index probes, with a per-topic `limit + 1` only for internal candidate selection and a final total `limit + 1` cut. The configured Query limit bounds the whole response. Compute `has_more` from the same snapshot and choose candidate IDs before loading payloads. Worst-case candidate work is topics times `limit + 1`; no fixed latency is promised without measurement.
- ARC-062: A nonempty successful page advances at least one requested topic cursor. Leave other cursors unchanged. Coalesce duplicate inputs as spec 001 requires. The loop drains a finite result set; continuous publication need not terminate a paging loop.
- ARC-063: Newest reads join watermarks and envelopes in one statement. Metadata-only results select all metadata but not the payload. Full results add the payload. Do not omit an existing topic to fit a successful response into the byte limit.
- ARC-064: Identity validation reads complete history from one snapshot. Reads still work if stored history exceeds the write cap. Do not use a separately read head as the validation position.
- ARC-065: Identifier lookup selects the greatest association sequence ID among active associations for each normalized `(identifier, kind)`. Reconstruct the positional response, including duplicate inputs.
- ARC-067: Get is one primary-key lookup on the envelope table by sequence id, returning the full row as a `ServerEnvelope`. A sequence id with no row visible in the serving database's snapshot is `NOT_FOUND`; the backend does not distinguish never-allocated, aborted, pruned, or not-yet-replicated. No additional index is needed.
- ARC-066: Oversized responses eventually fail under spec 001's size-error contract. Preserve Tonic's normal size-limit statuses and details; do not add custom framing or status translation for them. An application response-size check may return `RESOURCE_EXHAUSTED`. No additional pagination protocol or structured size-error type is required.

## 6. Streaming architecture

- ARC-070: One read-only tailer per instance polls the selected read database on one dedicated connection outside the request pools. This adds one database connection per instance and lets connection loss end the recovery generation without starving a one-connection request pool. It owns no write-pool handle or advisory-lock code. A separate boundary-maintenance task uses the primary and accepts only bounded in-process maintenance signals from the tailer. No LISTEN/NOTIFY path is required. One topic registry serves all sessions and follows the closed-boundary rules above.
- ARC-071: Poll at a fixed interval, default 100 ms. Polls do not overlap. Start the next wait after the previous poll completes. Drain full pages without an extra interval. Trace duration and row count.
- ARC-072: Use existing topic types and a standard registry representation first. Do not prescribe shard counts, inline layouts, custom hashers, or bytes per topic before measurement.
- ARC-073: Batch tailer dispatch by stream and use non-blocking sends. A slow stream must not block the tailer. Immutable payloads may be shared across deliveries.
- ARC-074: One session owner controls registrations, topic floors, pending updates, and keepalive state. Each registration has one starting position and fixed catch-up target. Internal generation checks prevent removed registrations' fetch results from emitting.
- ARC-075: Initialize the per-topic delivery floor to the requested cursor. Every delivery path discards rows at or below it and advances it only on ordered outbound admission. Active-topic adds do not change it. Removal and later re-add create a new registration.
- ARC-076: Register a topic before capturing its head from the same selected database used for fetching. Queue `Applied` with the new target before admitting any of that registration's messages. Empty topics have target zero. A head read is still required when no initial history is owed.
- ARC-077: Share a bounded fetch pool. Permit at most one outstanding catch-up fetch turn per stream. Bound fetched-but-unconsumed data at a fixed 64 MiB per stream (a private implementation constant), and release query resources before waiting for outbound capacity. A legal envelope with framing must fit the budget.
- ARC-078: While a topic catches up, coalesce live notices into its greatest needed sequence ID instead of buffering every live payload. Fetch ordered suffixes through that moving needed position; the client-visible initial target remains fixed. Atomically switch a current topic to direct live delivery. A racing notice is either included in pending work or handled after the switch, never lost.
- ARC-079: Target delivery frames of 2 MiB including framing. This is a private implementation constant, not a config key. Every permitted envelope must fit a frame, the transport cap, the fetched-data budget, and the outbound byte budget with worst-case metadata and framing. Startup rejects a permitted envelope size that cannot satisfy those relationships. Never skip an envelope while advancing its floor.
- ARC-080: Bound outbound queues by frames and bytes, fixed private implementation constants of 64 frames and 16 MiB. Pause fetches while safe. If required state cannot be retained, fail that stream with `RESOURCE_EXHAUSTED`. Do not discard data to remain within capacity.
- ARC-081: Keep separate send-idle and pong-deadline timers. Start the deadline at Ping transport handoff. Only the matching nonce clears it. Before timeout, process already available inbound frames without blocking. Backpressure has its own failure path.
- ARC-082: Every exit deregisters the session and cancels its work. Tailer failure or a required boundary-task database failure fails affected streams with `UNAVAILABLE`; working keepalives do not prove that delivery works.
- ARC-083: Native request half-close ends the session without waiting for catch-up. THE SDK SHALL use spec 004's fixed processing barriers for bounded sync and release only the completed or cancelled run's interests. A shared receiver needed by another operation SHALL remain active. There is no server catch-up drain mode.
- ARC-084: Static subscriptions share the same data path. Their initial `Started` frame includes the fixed topic targets. They use one-way keepalives; ending the unary request does not end the subscription.
- ARC-085: Reject an unset inbound oneof with `INVALID_ARGUMENT`. Apply the Update and Ping buckets in spec 001. Application-selected topics may include denied groups; the streaming layer does not enforce consent or membership.

### Bounded fair fetch turns

- ARC-086: Maintain a FIFO ready queue per stream, with each topic at most once. A turn takes up to 256 topics and at most 64 rows per topic. These are named private implementation constants, not new public API limits. Batch indexed `(topic, sequence_id)` range probes; do not issue one query per topic.
- ARC-087: Limit the whole batch by available fetched-data bytes as well as rows. Select bounded candidate IDs and safe encoded-size bounds before loading payloads. An outer byte cutoff must preserve each topic's prefix. Process topics in ready-queue order; if the cutoff prevents visiting a selected topic, retain its priority for the next turn. A partial topic keeps only the progress actually admitted.
- ARC-088: Requeue served topics with more work at the back; newly ready topics also join the back. A topic current with its known needed position leaves the fetch queue until new work arrives or it enters direct live delivery. Do not repeatedly select the first topics by topic or global sequence-ID order. Process control work between bounded turns; removals never wait for a topic's full backlog.

This is round-robin fairness among ready catch-up topics, not a fixed latency or equal-bandwidth guarantee. Fetching through coalesced notices may add range reads while behind. Measure batch sizes, read cost, and scheduling in Phase 4. There is no wave-wide barrier, fold, or queue of gated live payloads.

## 7. Validation and cache

- ARC-090: Validation errors are typed through the shared logic and map to the spec 001 reason codes. Do not classify errors by matching message substrings.
- ARC-091: Preserve retryability from the identity verifier, including provider, I/O, and missing-verifier failures. A chain failure is not an invalid identity signature.
- ARC-092: Preserve existing validation behavior, including group-message trailing bytes and the absence of an added ciphersuite allow-list. Spec 003 records what checks do and do not run.
- ARC-093: Fold identity history for each publish without an association-state cache. Historical signature conversion can call chain RPC; only the state fold itself is pure CPU work. Keep the separate SCW signature-verdict cache defined in spec 003.

### Metrics, logs, and traces

- ARC-094: Use the shared logging pipeline for stdout and optional OTLP gRPC export. The log level defaults to INFO; a command-line override takes precedence. Stdout uses text by default or one JSON object per line when configured. The stdout level must not suppress INFO operation spans, span duration metrics, or trace export. The request-logger switch controls only completion events. Metrics and request spans remain active.
- ARC-095: When the request logger is enabled and INFO is admitted by the log level, emit one completion event per gRPC request. Include the method path, duration in milliseconds, consumed request-body bytes, emitted response-body bytes, and a server-generated request ID. Response counts exclude queued or unpolled bytes and do not confirm client receipt. HTTP headers and trailers are excluded; gRPC-Web trailers encoded in body data are included. Count bytes without buffering payloads or trusting Content-Length. For bidirectional streams, count all consumed inbound frames; completion means the response body ends, fails, or is cancelled, not that response headers were sent. Exclude CORS preflight from request events.
- ARC-096: Emit an INFO event for each accepted stream-interest mutation with its added and removed topic counts. Do not log topic values, payloads, authorization headers, or keys. Mutation events remain independent of the request-logger toggle and use the request correlation context.
- ARC-097: Export traces only when an OTLP endpoint is configured. If the key is absent, use `OTEL_EXPORTER_OTLP_ENDPOINT` when set. This is the sole implicit environment fallback and an explicit exception to ARC-101.
  - A malformed resolved endpoint fails startup. The error names the key or environment variable and must not include the resolved value. An unreachable endpoint must not prevent serving, metrics, or stdout logs. Count each failed export batch. After the request drain, flush and stop export within a separate five-second bound.
  - Export service identity as `service.name` (default `xmtp-backend`) and the build version as `service.version`. Reject either key in extra resource attributes. The root sample ratio must be finite and between zero and one, inclusive. OTLP logs are disabled by default and no log exporter is built when disabled.
- ARC-098: The optional telemetry section configures a separate Prometheus text listener, default `0.0.0.0:9464`. An empty listen address disables only the listener; metrics remain recorded in process. An occupied address fails startup and the error names that address. Install the recorder before logging. Publish readiness zero and build version before database initialization; change readiness in the same call that reports health Serving or NotServing.
  - Request metrics cover response-body completion, failure, or cancellation, including native and gRPC-Web streams. Preserve the actual status from headers or trailers. With no status, a dropped body is Cancelled and an ended body is Unknown. Decrement in-flight exactly once. Exclude preflight and health from all gRPC metrics; retain health completion events and server-generated request IDs. Use `grpc_type`, `grpc_service`, `grpc_method`, and, for completion count and duration, `grpc_code`. Match only the four backend services and standard health routes; unknown paths use the single unknown service and method label. Subscribe is `bidi_stream`, SubscribeStatic is `server_stream`, and other methods are `unary`.
  - Sample pools, sequence IDs, replica replay delay, process resources, runtime activity, and occupied fetch permits every five seconds. Report the read pool and read sequence only when a distinct read pool exists. An empty envelope table reports sequence zero. Equal receive and replay WAL positions report delay zero; otherwise report the age of the last replayed transaction, and omit the sample when that timestamp is absent. Each database query obeys the configured statement timeout. A failed sample increments its error counter, retains the previous affected gauge values, and does not stop later sampling.
  - Count every publish input position once by response origin: stored, duplicate, or rejected when the request fails. Count every registered stream termination once with a fixed reason. Remove registry gauges once even during recovery failure. Measure delivery lag only for live envelopes at outbound admission, clamp negative lag to zero, and measure outbound waiting only when capacity was unavailable. A new recovery generation resets tailer readiness and gap count until recovery succeeds. Count barrier lock timeouts and emit one warning for each. Describe every backend metric from one catalogue.
- ARC-099: Accept W3C `traceparent` and `tracestate`, including CORS preflight that names these headers. Install propagation even when export is off. With trace export enabled, the incoming context is the request span parent. Request spans identify the server kind, gRPC system, bounded service and method, and final gRPC status. Completion events include the status name and a trace ID only when a valid incoming or generated trace context exists.
  - No metric label, span field, or log field may derive from a topic, inbox ID, installation ID, group ID, cursor, payload, or request header. The only exceptions are W3C trace context and the server-generated request ID. Operation names and status reasons have bounded vocabularies. Never use request data as a metric name or label.

The required operation span names are `db.commit_publish`, `db.find_duplicates`, `db.history`, `db.query`, `db.newest_envelopes`, `db.newest_metadata`, `db.get`, `db.inbox_ids`, `db.advance`, `publish.parse_publish`, `publish.validate_publish`, `publish.locks`, `tailer.poll`, `tailer.bootstrap`, `scw.verify`, `stream.update`, `stream.fetch`. A completed `tailer.poll` span includes INFO-level `rows` and `gaps` fields.

### Backend metric catalogue

Shared logging emits operation-span and export-failure metrics. The backend catalogue describes these metrics.

| Metric | Type | Meaning |
| --- | --- | --- |
| `xmtp_operation_duration_seconds` | histogram | Operation span duration by operation and status. |
| `xmtp_telemetry_export_failures_total` | counter | Failed telemetry export batches. |
| `grpc_server_started_total` | counter | gRPC requests started. |
| `grpc_server_handled_total` | counter | gRPC requests completed. |
| `grpc_server_handling_seconds` | histogram | gRPC response body lifetime. |
| `grpc_server_in_flight` | gauge | gRPC requests in flight. |
| `grpc_server_request_bytes_total` | counter | Consumed gRPC request body bytes. |
| `grpc_server_response_bytes_total` | counter | Emitted gRPC response body bytes. |
| `xmtp_db_released_open_transactions_total` | counter | Open transactions rolled back on pool release. |
| `xmtp_db_errors_total` | counter | Database errors mapped to RPC statuses. |
| `xmtp_publish_envelopes_total` | counter | Publish input positions by response origin. |
| `xmtp_publish_rejections_total` | counter | Rejected publishes by validation reason. |
| `xmtp_scw_verifications_total` | counter | Smart contract wallet verification results. |
| `xmtp_stream_sessions` | gauge | Registered stream sessions. |
| `xmtp_stream_topics_registered` | gauge | Registered stream topic interests. |
| `xmtp_stream_frames_sent_total` | counter | Stream frames admitted to the outbound queue. |
| `xmtp_stream_frames_received_total` | counter | Stream frames received. |
| `xmtp_stream_envelopes_sent_total` | counter | Stream envelopes admitted by delivery phase. |
| `xmtp_stream_updates_total` | counter | Stream interest update results. |
| `xmtp_stream_ended_total` | counter | Stream sessions ended by reason. |
| `xmtp_stream_outbound_wait_seconds` | histogram | Time waiting for outbound capacity. |
| `xmtp_stream_fetch_wait_seconds` | histogram | Time waiting for a stream fetch permit. |
| `xmtp_tailer_polls_total` | counter | Tailer poll results. |
| `xmtp_tailer_rows_total` | counter | Tailer rows read by source. |
| `xmtp_tailer_gap_ranges` | gauge | Unresolved tailer gap ranges. |
| `xmtp_tailer_restarts_total` | counter | Tailer recovery generations started. |
| `xmtp_tailer_ready` | gauge | Whether stream recovery is ready. |
| `xmtp_boundary_advances_total` | counter | Allocation boundary advance results. |
| `xmtp_backend_ready` | gauge | Whether the backend reports Serving. |
| `xmtp_backend_info` | gauge | Backend build version. |

## 8. Configuration

- ARC-100: Read one TOML file selected by `--config`. The database URL is required; other values have the defaults below. Publish a JSON schema usable by Taplo. Reject unknown keys, invalid values, and inconsistent size relationships at startup; the relationships to check are the ones stated in the key comments below. Private implementation constants do not need config keys. Configured timer durations must form representable deadlines on the host's monotonic clock.
- ARC-101: A string of the form `env:NAME` reads that environment variable at startup. A missing variable fails startup. Error messages and logs must not include resolved secrets.
- ARC-102: Each public limit has one named config value. Server-only settings stay with the server; values shared with clients have one shared definition. A lower deployment limit can require smaller client batches. Identity and commit retention exemptions cannot be disabled by a finite duration setting. Retention is configured in seconds and converted to nanoseconds when `expiry_ns` is computed; the config never carries a nanosecond literal. Before readiness, validate each finite retention period against the primary database clock: the resulting expiry must fit the stored timestamp type. Do not substitute the application clock. Later time advances or database clock steps can still cause an insert-time arithmetic error; startup validation does not remove that check.

```toml
#:schema https://raw.githubusercontent.com/xmtp/libxmtp/self-hosted/docs/schemas/backend-v1.json
[server]
# Address the plaintext gRPC listener binds (ARC-002).
listen = "0.0.0.0:5050"
# Basic logging through the shared pipeline. CLI --log-level overrides this.
log_level = "info"  # off, error, warn, info, debug, trace
log_format = "text" # text or json
# Emit one INFO completion event per gRPC request, including long-lived streams.
request_logger = true
# How long shutdown waits for in-flight unary requests before the process exits (ARC-003).
# Open streams fail with UNAVAILABLE at once; only unary requests get this budget.
max_drain_duration_ms = 10000

[telemetry]
# Separate Prometheus listener. Empty disables the listener only.
metrics_listen = "0.0.0.0:9464"
# Optional OTLP gRPC export. Absent uses OTEL_EXPORTER_OTLP_ENDPOINT, if set.
# otlp_endpoint = "http://tempo:4317"
otlp_logs = false
service_name = "xmtp-backend"
sample_ratio = 1.0
resource_attributes = { "deployment.environment" = "local" }

[database]
# Primary connection string. Required. `env:NAME` reads the variable at startup (ARC-101).
url = "env:XMTP_DATABASE_URL"
# Optional read replica, one instance. Reads route to it as section 5 describes (ARC-060).
# replica_url = "env:XMTP_REPLICA_URL"
# Connections per request pool. With a replica there are two pools (ARC-060).
# The tailer uses one additional dedicated connection to the selected read database.
max_connections = 20
# Postgres statement_timeout applied to every statement the backend runs (ARC-051).
max_statement_timeout_ms = 5000

[publishing]
# Hard ceiling on one publish request, from first lock to commit. Bounds how long a
# publisher can hold topic locks and the shared allocation barrier (ARC-051).
# Must be greater than database.max_statement_timeout_ms: a single slow statement should
# fail on its own timeout, not by exhausting the whole request budget.
max_publish_duration_ms = 10000

# How long the boundary task waits to acquire the exclusive allocation barrier before it
# gives up (ARC-039, ARC-042). An acquisition waits at most for the longest in-flight
# publish, so a value below max_publish_duration_ms can time out while a slow publish
# holds the barrier. That is safe: the task retains the pending request and retries later.
# A timeout never retires a range and never fails a stream.
max_barrier_wait_ms = 1000

[streams]
# Interval between tailer polls of the read database (ARC-071). Polls do not overlap.
poll_interval_ms = 100
# Most unresolved gap ranges the tailer keeps before it fails affected streams with
# RESOURCE_EXHAUSTED (ARC-042). Counted as ranges, not as absent integers.
max_gap_ranges = 10000
# Send-idle time before the server sends a Ping (ARC-081). Advertised in Started (API-111).
keepalive_interval_ms = 30000
# How long the server waits for the matching Pong after a Ping before it fails the
# stream (ARC-081). Must be greater than keepalive_interval_ms: a peer needs at least one
# keepalive period to answer.
max_pong_wait_ms = 90000

[retention]
# Age at which a row becomes eligible for deletion, per topic kind (API-021). Stored as
# expiry_ns = server_ns + this value converted to nanoseconds at publish time.
# Identity updates, commit-log entries, and commits/proposals never expire; a finite value
# here cannot override those exemptions (ARC-102).
group_message_seconds = 7776000  # 90 days; commits and proposals are exempt
welcome_seconds = 7776000        # 90 days
key_package_seconds = 7776000    # 90 days

[chains]
# Chain RPC routes for smart-contract-wallet verification, keyed by CAIP-2 chain id
# (section 5). An empty map serves non-SCW identities; SCW operations on an unconfigured
# chain return UNAVAILABLE.
# "eip155:1" = "env:XMTP_RPC_MAINNET"

[validation]
# Entries in the per-instance SCW signature-verdict cache, LRU eviction (SEC-042).
max_scw_cache_entries = 10000

[limits]
# Each key below is one public limit from spec 001 section 11 (API-131). The server rejects
# a request above a limit with INVALID_ARGUMENT unless a more specific rule applies (API-130).

# Topics per Query request (API-143 is the client chunk size).
max_query_topics = 1000
# Total Query limit when the request sets none (ARC-061).
default_query_limit = 100
# Largest total Query limit a request may set. Must be >= default_query_limit.
max_query_limit = 1000
# Topics per metadata-only newest-envelope request (API-143).
max_newest_metadata_topics = 1000
# Topics per full-envelope newest-envelope request (API-140). Should be
# <= max_newest_metadata_topics; a full read costs more per topic.
max_newest_full_topics = 100
# Distinct topics in one publish request. ARC-031 holds one lock per topic.
max_publish_topics = 1000
# Encoded bytes of one envelope. ARC-079 requires it to fit the delivery frame,
# transport cap, fetched-data budget, and outbound byte budget. Must be <=
# max_request_bytes: one envelope must fit one request.
max_envelope_bytes = 1048576
# Encoded bytes of one request (API-031, API-142).
max_request_bytes = 26214400
# Encoded bytes of one response. Above this the response fails under the
# size-error contract; Tonic's normal size errors pass through (API-134, ARC-066).
max_response_bytes = 26214400
# Topics added by one Update frame. Must be <= max_stream_topics: one update cannot
# exceed the stream cap.
max_update_adds = 100000
# Topics removed by one Update frame.
max_update_removes = 100000
# Registered topics per bidirectional stream (ARC-074).
max_stream_topics = 100000
# Topics per static-subscription request (API-110, API-144).
max_static_topics = 10000
# Identifiers per inbox-id lookup request (API-141).
max_lookup_identifiers = 250
# Signatures per SCW verify request or identity update.
max_scw_signatures = 100
# Identity-update entries per inbox. Applies to new writes only (API-148).
max_identity_entries = 256
# Concurrent HTTP/2 streams advertised per connection (API-132).
max_http2_streams = 100
# Update-frame token bucket per stream: refill rate and burst (API-107, ARC-085).
max_update_frames_per_second = 10
max_update_burst = 100
# Client Ping token bucket per stream: refill rate and burst (API-107, ARC-085).
max_ping_frames_per_second = 10
max_ping_burst = 100
```

The raw repository URL is the public schema publication target. Publish and validate it with the backend config implementation. An empty chain map supports non-SCW identities; SCW operations on an unconfigured chain return `UNAVAILABLE`. A minimal deployment therefore needs only the database URL, while SCW support also needs chain routes. The statement timeout, publish duration, and barrier wait bound work; gap correctness does not depend on their values. The relationships stated in the key comments (`max_publish_duration_ms` above `max_statement_timeout_ms`, `max_pong_wait_ms` above `keepalive_interval_ms`, `max_query_limit` at or above `default_query_limit`, `max_envelope_bytes` at or below `max_request_bytes`, and `max_update_adds` at or below `max_stream_topics`) are the size relationships ARC-100 checks at startup. Startup also checks that the permitted envelope size fits the delivery frame, transport cap, fetched-data budget, and outbound byte budget with worst-case metadata and framing.

## 9. Verification and phase limits

- ARC-110: Backend integration tests exercise real Postgres and the real service surface with stateless fixtures. Cover each endpoint's happy path, errors, and limits. Reuse the project's test macro, generators, fault injection, clocks, task handles, and typed errors.
- ARC-111: Concurrency tests cover topic ordering, identical publish races, mixed duplicate/new failures, and identity history changing during validation. A full duplicate at the identity cap succeeds. Unexpected uniqueness errors never produce partial success.
- ARC-112: Exercise late commits after boundary attempts, replica replay pauses, gap/forward snapshot races, and startup during an open publish. Each committed row is delivered in topic order or the affected stream explicitly fails before recovery state is lost.
- ARC-113: Cover idempotent adds, registration and target capture, fair batched catch-up, removal/re-add, native half-close, static continuation, slow consumers, large envelopes, oversized responses, and tailer/database failure. Direct service gRPC-Web tests include preflight and incremental delivery; an HTTPS load-balancer smoke test covers pass-through without conversion or buffering.
- ARC-114: Test each important behavior once on its owning platform. Backend tests own protocol semantics; binding tests own conversion and SDK lifecycle. Phase 4.5 adds metrics, trace propagation, and optional trace export. Backend tests cover telemetry configuration, status and byte accounting, metric ownership, label hygiene, and bounded shutdown. Phase 4.6 owns benchmarks and vacuum tuning. Phase 5 owns pruning. Phase 6 owns caller authentication and quotas.

Supported database failover must preserve acknowledged commits and fence the old primary. Promoting a replica that loses acknowledged data or restoring an old backup is an operator recovery event; durable client cursors cannot repair it. The backend does not implement a database failover manager.

## Review record

The [single-client streaming proposal](https://plan.ref.tools/BbNc54CedfhM1Snb), approved 2026-09-06, replaces the earlier wave design with fixed targets, coalesced live notices, and bounded fair fetch turns. Applications control which topics they stream.

- [Original architecture draft](https://plan.ref.tools/c8yzIJ4kaAAmrqZE).
- [Approved review and owner decisions](https://plan.ref.tools/xWi9jEu8VHmuLI0W): keep replicas, timestamps from the database clock, existing validation, the SCW cache, and stream token buckets. Use simple response-size errors. Replace gap expiry with a proven allocation boundary and fix the identity snapshot and newest metadata rules.
