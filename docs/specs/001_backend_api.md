# 001: Backend API

Status: approved with the owner decisions recorded in the review log.

This spec states the public API of the self-hosted XMTP backend: what a client sends, what the backend stores, what it returns, and when it fails. The wire package is `xmtp.backend.v1`. Spec 002 covers the backend architecture. Spec 003 states validation and trust limits. Spec 004 covers stream semantics in full; this spec states the rules a client must obey.

Requirements are numbered `API-nnn`. "Must" is a requirement. "Should" is a recommendation.

## 1. Terms

| Term | Meaning |
| --- | --- |
| Envelope | One unit of client data: a group message, welcome, key package, identity update, or commit-log entry. |
| Topic | One byte of kind plus an identifier. Every envelope belongs to exactly one topic. |
| Sequence id | A 64-bit integer the backend assigns to each stored envelope. Unique across all topics. |
| Cursor | A position on one topic: the highest sequence id a client has seen on that topic. 0 means the beginning. |
| Message hash | SHA-256 of the stored envelope bytes. With the topic, the idempotency key. |
| Wave | The catch-up replay a subscription mutation starts. |

## 2. Topics

- API-001: The backend must derive the topic of every envelope from its payload. A client must not send a topic on publish.
- API-002: A topic is one kind byte followed by the identifier bytes, as in the table.

| Kind | Payload | Identifier |
| --- | --- | --- |
| `0x00` | Group message | 16-byte group id, decoded from the MLS message |
| `0x01` | Welcome | 32-byte installation key from the welcome |
| `0x02` | Identity update | 32-byte inbox id, hex-decoded |
| `0x03` | Key package | 32-byte installation key from the key package |
| `0x04` | Commit-log entry | 16-byte group id, decoded from the entry |

- API-003: A read request that names a topic with an unknown kind or an identifier of the wrong length must fail with `INVALID_ARGUMENT`.
- API-004: A cursor is between 0 and `INT64_MAX`, inclusive. A larger wire value fails with `INVALID_ARGUMENT`. An absent cursor means 0.

## 3. Ordering and visibility

- API-010: Sequence ids come from one sequence and are unique across all topics.
- API-011: Within a topic, an envelope becomes visible only when every envelope with a lower sequence id on that topic has committed or aborted. Every read of a topic returns a prefix of that topic, minus gaps left by aborted writes.
- API-012: Across topics, order is not defined. A client must never infer from a sequence id on one topic that any envelope on another topic has been delivered.
- API-013: Gaps in sequence ids are normal and carry no meaning.
- API-014: Identity updates are serialized across all inboxes. Among identity updates, sequence id order equals commit order.
- API-015: The backend must serve a read that starts at a cursor above the topic's newest sequence id as an empty result, not an error.
- API-016: When a publish response returns, every envelope it stored is committed on the primary database. A read served from a read replica may lag behind the primary. Replication lag is acceptable and does not break the ordering guarantees: a replica still returns a prefix of each topic (API-011), so a lagging read looks like an earlier point in time, never a reordering or a gap.
- API-017: A client must not assume that a read issued right after its own publish returns that envelope. A client that needs its own write must retry the read or rely on the sequence id from the publish response. Publish validation (section 5) always runs against the primary, so a publish that depends on an earlier publish (a key package before its identity update) is validated against committed data.
- API-018: Publish and Query use the primary. Newest-envelope reads, subscriptions, and inbox-id lookups may use a replica. Each replica endpoint names one replica instance; lag is expected to be short. This does not make elapsed time a proof that a missing row cannot arrive.

## 4. Envelope metadata

Every stored envelope carries the metadata below. The backend assigns all of it.

| Field | Rule |
| --- | --- |
| `cursor` | The sequence id. |
| `server_ns` | Assigned from the database clock inside the insert, in nanoseconds. Equal values and backwards values after clock adjustment or failover are allowed. The client uses it as the envelope's created time, never as a cursor. |
| `message_hash` | SHA-256 of the stored envelope bytes. |
| `topic` | The derived topic. |
| `expiry_ns` | Earliest eligibility for deletion: `server_ns` plus the retention period. Zero when the row never expires. Not a message expiry or a promise of immediate deletion. |
| `is_commit_or_proposal` | Group messages only: true when the parsed MLS content type is a commit or proposal. False for every other kind. This is not sender authentication. |

- API-020: The backend must store the canonical protobuf re-encoding of the client envelope and must compute `message_hash` over exactly those bytes.
- API-021: Retention is set at publish time from server configuration. Defaults: group application messages, welcomes, and key packages use 90 days (the fixed duration called 3 months). Identity updates and commit-log entries never expire. Group messages with `is_commit_or_proposal` true also never expire. These exemptions override the topic-kind duration. Phase 5 defines deletion and cursor behavior. Before that phase, there is no pruning or read-time expiry filter.
- API-022: A client should store `expiry_ns` but must not act on it before Phase 5 defines the behavior.
- API-023: Canonical re-encoding applies to the protobuf framing only. The backend must return every payload byte field (group message data, welcome data, key package bytes, commit-log entry bytes) exactly as received.
- API-024: The client must match its own published messages by `message_hash`, computed over the same canonical envelope encoding, and must store the hash the backend returns as the authoritative value.
- API-025: Shared canonical encoding and hashing must produce the same bytes on the backend and every client. The outer envelope hash does not replace the separate MLS message ID or payload hash used for client processing.

Until Phase 5 the backend has no retained-floor signal and the client has no gap detection. A cursor that points below deleted rows silently skips them. For a group message that is a commit, that is a permanent fork. Phase 5 must close this before retention is enabled in production.

## 5. Publish

### 5.1 Atomicity and idempotency

- API-030: A publish request is atomic. Either every envelope in it is stored or none is.
- API-031: A publish request has no envelope count limit. It must be at most 25 MiB, every envelope must be at most 1 MiB, and it must address at most 1000 distinct topics. A violation fails with `INVALID_ARGUMENT` and no envelope is stored. One MLS commit and its proposals stay in one atomic publish.
- API-032: The response lists one metadata entry per envelope, in request order.
- API-033: An envelope whose `(topic, message_hash)` is already stored is a duplicate. The backend must not store it again and must return the stored metadata as success.
- API-034: Two identical envelopes in one request collapse to one stored row. Both response entries carry the same metadata.
- API-035: The duplicate check must run before validation and again at commit time, so a copy that commits during validation is still answered as a duplicate.
- API-036: Duplicate collapse must preserve original request indexes. A concurrent duplicate succeeds even if validation of that copy failed before the final duplicate check. Unexpected storage conflicts must not produce a successful response for a partially committed request.
- API-037: Failure to receive a successful response does not prove rollback. A publish can commit before its response is lost or rejected for size. Retrying exact envelope bytes is idempotent while the rows remain stored.

Because the hash covers the whole envelope, a re-signed or re-encrypted copy of the same logical message is a new envelope, not a duplicate. Group messages are deduplicated by the client using the MLS message id, and commit-log entries by their commit sequence id, so this is safe. A client that re-encrypts a message after a failed publish must tolerate the earlier copy arriving later as a message it did not match to an intent.

### 5.2 Validation

- API-040: The backend must parse every envelope. A payload that does not parse fails with `INVALID_ARGUMENT`, reason `MALFORMED_PAYLOAD`.
- API-041: A group message must parse as an MLS protocol message. The backend derives the group id and `is_commit_or_proposal` from the parse. Trailing bytes remain accepted and stored verbatim. The backend does not verify group membership or the MLS signature; it has no group key. Spec 003 states the preserved validation limits.
- API-042: A key package must pass key-package validation. A failure is `INVALID_ARGUMENT`, reason `INVALID_KEY_PACKAGE`.
- API-043: An identity update must apply cleanly to the inbox's current association state, read from the inbox's identity topic. A failure is `INVALID_ARGUMENT`, reason `INVALID_IDENTITY_UPDATE`. A signature failure is reason `INVALID_SIGNATURE`.
- API-044: Smart-contract-wallet signatures inside an identity update are verified over chain RPC. A chain RPC failure is `UNAVAILABLE`, not `INVALID_ARGUMENT`. An identity update carries at most 100 such signatures.
- API-045: A welcome is stored without validation beyond parsing. The backend must not check that a welcome's installation key belongs to a registered installation; welcome pointers are addressed to random 32-byte values by design.
- API-046: A commit-log entry must parse as a plaintext commit-log entry that carries a group id. Its signature is stored and returned, not verified.
- API-047: The error detail for an `INVALID_ARGUMENT` names the index of the first failing envelope and a reason code. A request-level error carries no index.

### 5.3 Identity updates

- API-050: After identical envelopes collapse, a publish request must contain at most one identity update per inbox. Two distinct updates for one inbox fail with `INVALID_ARGUMENT`.
- API-051: The backend validates an identity update against one complete history snapshot. Its read sequence id is the highest sequence id in that exact history, or 0 for empty history. At commit time, under the identity serialization lock, it checks that the inbox's newest sequence id still equals that value. If it does not, the request fails with `ABORTED` and nothing is stored. It must not read the history and then assign a newer watermark from a separate read.
- API-052: On `ABORTED`, the client must re-read the inbox's identity topic, re-validate the update against the new state, and resend it.
- API-053: An identifier may be associated with more than one inbox over time. The backend does not enforce exclusivity. Identifier resolution returns the inbox with the latest association.

### 5.4 Key packages

- API-060: Every key-package upload is stored. The backend does not delete the previous key package for an installation on upload.
- API-061: The newest key package for an installation is the one with the highest sequence id on its topic.
- API-062: Key packages expire after the key-package retention period (3 months). A client must re-upload well inside that period.
- API-063: The client must read the newest-envelope response as a map from topic to an optional key package. It must not require the response to have the same length as the request, and it must not fall back to one request per key.

### 5.5 Commit log

- API-065: The commit-log position of an entry is its sequence id. The client stores that value as the entry's log position.
- API-066: Commit-log entries for one group are totally ordered by the rules in section 3. Epoch continuity and hash-chain checks are client concerns. A skipped entry permanently disables a group's fork detection on that client, so the backend must never drop or reorder an entry on a commit-log topic.
- API-067: The client must take a commit-log entry's position from the envelope metadata. The entry itself carries no sequence id.

## 6. Query

- API-070: A query names up to 1000 `(topic, cursor)` pairs and a `limit`. A request with more than 1000 pairs fails with `INVALID_ARGUMENT`.
- API-071: `limit` is the total number of envelopes across all topics. The maximum is 1000 and the default is 100. A larger value is clamped to 1000.
- API-072: The result is the union of all envelopes with `sequence_id > cursor` on their topic, ascending by sequence id within each topic, cut at `limit`. The order across topics carries no meaning.
- API-073: `continuation.has_more` is true when more envelopes matched than `limit` allowed. The backend must compute it from the same read as the page.
- API-074: Client rule: for each topic that returned rows, set its cursor to the highest sequence id returned for that topic. Leave every other cursor unchanged. When `has_more` is true, query again with the updated cursors.
- API-075: A query on a key-package topic is not supported and fails with `INVALID_ARGUMENT`. Use the newest-envelope read.

API-074 is safe under per-topic order: a topic's cursor moves only when that topic's own rows are returned, and those rows are in order. It makes progress because `has_more` implies at least one row was returned, so a loop that repeats the query until `has_more` is false terminates once every topic is drained.

- API-076: `limit` bounds the whole response, not each topic. A client that needs every envelope above its cursors on many topics must loop on `has_more`. A client that needs a fixed page per topic must query that topic alone.
- API-077: Repeated query topics coalesce at their lowest supplied cursor. Each stored envelope appears once. Validate all entries and apply the input-count limit before coalescing. A page and its `has_more` value use one read snapshot; later pages may observe later commits.

## 7. Newest envelope

- API-080: A newest-envelope request names up to 1000 topics when it asks for metadata only, and up to 100 topics when it asks for full envelopes. A larger request fails with `INVALID_ARGUMENT`.
- API-081: The response holds one result per topic that has at least one envelope. A topic with no envelope is absent from the response.
- API-082: With `include_full_envelope` false, every result carries metadata only. With it true, every result carries metadata and the envelope.
- API-083: The newest envelope of a topic is the visible envelope with the highest sequence id.
- API-084: Repeated newest topics coalesce. Validate all entries and apply the input-count limit before coalescing. A successful result must contain all required metadata, including hash, expiry, and the commit/proposal flag.

## 8. Subscribe (bidirectional)

The bidirectional stream follows XIP-83. Spec 004 states the full protocol. The rules below are the API contract.

- API-090: The first frame on every stream is `Started`, carrying the server's keepalive interval and its capability list. In v1 the capability list is empty. A keepalive interval of 0 means the server advertises none and the client uses its own default.
- API-091: A `Mutate` frame adds and removes topics atomically. Adds carry a cursor; the server replays every envelope above the cursor, then delivers live. Removes clear the topic's cursor floor.
- API-092: A `Mutate` must carry at most 100,000 adds and at most 100,000 removes. A stream must hold at most 100,000 distinct topics. History-only topics count while their wave is in flight. A violation fails the stream with `INVALID_ARGUMENT`.
- API-093: `mutate_id` must be nonzero when adds are present and must not equal the id of a wave still in flight. A violation fails the stream with `INVALID_ARGUMENT`. At most 256 waves may be in flight on one stream; a `Mutate` that would open the 257th fails the stream with `INVALID_ARGUMENT`.
- API-094: Every `Mutate` is acknowledged with exactly one `CatchupComplete` carrying its `mutate_id`, including removes-only and no-op mutations.
- API-095: A delivery frame belongs to exactly one wave or to live. The frame's `mutate_id` is the wave's id, or 0 for live. The server never mixes lanes in one frame.
- API-096: Within a frame, envelopes of one topic are ascending by sequence id. Topics may interleave in any order.
- API-097: `TopicsLive` names topics whose replay is complete. It is informational. Live frames for a wave's topics begin only after the wave's `CatchupComplete`.
- API-098: With `history_only` true, the adds are replayed through captured ceilings and acknowledged but not registered for live delivery. After removals are applied, a history-only add for an already subscribed topic, or any add for a topic with an in-flight history-only wave, fails with `INVALID_ARGUMENT`. With a half-closed request stream, the server closes after accepted waves finish.
- API-099: Either peer may send `Ping`. The receiver must answer with `Pong` carrying the same nonce. A peer that receives no `Pong` within its deadline closes the stream.
- API-100: A cursor above the topic's newest sequence id replays nothing and is not an error. A remove for a topic that is not subscribed is a no-op. A duplicate add inside one `Mutate` coalesces with the first occurrence. An add for a topic already live, with a cursor not below its floor, is a no-op. A lower cursor replaces the subscription: its floor clears and it replays from that cursor. A topic in both adds and removes is applied as remove then add. Except for the history-only collision rule in API-098, an add for a topic already in replay replaces its old wave ownership and replays from the new cursor; both mutations still receive an acknowledgement.
- API-101: There is no additional application-level quota for concurrent subscription streams in v1. The HTTP/2 concurrency limit in API-132 still applies. Phase 6 adds caller quotas.
- API-102: A client recovers from backpressure or stream loss by opening a new stream from its durable per-topic cursors. Duplicates across the overlap are the client's to drop.
- API-103: Per-topic cursor floors on a stream are independent. The client must track its last-seen position per topic and must not derive a shared watermark across topics of one kind.
- API-104: A bidirectional stream with no subscribed topics stays open.
- API-105: Expiry does not affect stream delivery. An envelope delivered before its `expiry_ns` is never retracted.
- API-106: A subscription request with an unset request oneof fails the stream with `INVALID_ARGUMENT`.
- API-107: Mutate and client Ping frames each have a per-stream token bucket of 10 frames/s with burst 100. Exceeding either bucket fails the stream with `RESOURCE_EXHAUSTED`. Pong frames do not consume either bucket. These protocol protections are the Phase 2 exception to API-133.

## 9. Subscribe (static)

The static stream serves clients that cannot open a bidirectional stream.

- API-110: A static subscription names a fixed list of up to 10,000 `(topic, cursor)` pairs. Zero topics or more than 10,000 fails with `INVALID_ARGUMENT`. A client with no topics opens no stream until it has one.
- API-111: The first frame is `Started`, carrying the keepalive interval.
- API-112: The server replays every envelope above each cursor, sends one `CatchupComplete`, then delivers live until the client cancels.
- API-113: `Keepalive` frames flow from server to client only and are never answered. The client reopens the stream after three keepalive intervals of silence.
- API-114: To change its topic set, a client opens a new stream from its durable cursors and cancels the old one. A client with more than 10,000 topics opens more than one stream.

## 10. Identity reads

- API-120: An inbox-id lookup names up to 250 identifiers, each with its kind. More than 250 fails with `INVALID_ARGUMENT`.
- API-121: The response has one entry per request entry, in order, including repeated identifiers. It echoes the identifier and its kind. The inbox id is absent when the identifier has no active association.
- API-122: An identifier resolves to the inbox with the latest non-revoked association. Revoking that association can reveal an older active association to another inbox. Lookup is scoped by identifier and kind. Normalize lookup keys and the verified projection by kind (lowercase hex for Ethereum); do not rewrite signed identity-update fields.
- API-123: Smart-contract-wallet signature verification takes a list of at most 100 signatures and returns one result per signature, in order. A chain RPC failure is `UNAVAILABLE`.

## 11. Limits

| Limit | Value |
| --- | --- |
| Query topics per request | 1000 |
| Query limit | max 1000, default 100 |
| Newest-envelope topics, metadata only | 1000 |
| Newest-envelope topics, full envelopes | 100 |
| Publish envelopes per request | no count limit; bytes only |
| Distinct publish topics | 1000 |
| Envelope bytes | 1 MiB |
| Request and response bytes | 25 MiB |
| Mutate adds per frame | 100,000 |
| Mutate removes per frame | 100,000 |
| Topics per bidirectional stream | 100,000 |
| Static-subscription topics per request | 10,000 |
| Waves in flight per bidirectional stream | 256 |
| Inbox-id lookup identifiers | 250 |
| Signatures per smart-contract-wallet verify request | 100 |
| Identity-update entries per inbox | 256 |
| Concurrent requests per connection (HTTP/2 streams) | 100 |
| Keepalive interval | 30 s |
| Mutate frames per stream | 10/s, burst 100 |
| Client Ping frames per stream | 10/s, burst 100 |

- API-130: The backend must reject a request above a structural or byte limit with `INVALID_ARGUMENT`, unless a more specific rule states otherwise. Stream token-bucket exhaustion and response-size failure use `RESOURCE_EXHAUSTED`.
- API-131: Every limit is one named configuration value. No limit is a literal in code.
- API-132: The backend must advertise at most 100 concurrent HTTP/2 streams per connection. A client that exceeds it queues locally; the backend does not fail the request.
- API-133: Rate limits are Phase 6 work. Until then the backend applies no per-caller rate limit.
- API-134: An encoded response above 25 MiB must eventually fail with `RESOURCE_EXHAUSTED`. The error may come from the transport. A successful response must not omit results to fit the byte limit or advance cursors past unsent rows. No byte-based pagination or new size-error detail is required. Oversized publish responses may fail after commit (API-037).

### 11.1 Client chunking requirements

- API-140: The client must chunk newest-envelope reads with full envelopes at 100 topics and must cap the number of chunks in flight.
- API-141: The client must chunk inbox-id lookups at 250 identifiers.
- API-142: The client must chunk publishes by encoded size under 25 MiB and must cap the number of chunks in flight. It must measure the encoded request size rather than estimate it, and it must re-chunk on a `TOO_LARGE` reason. A commit and its proposals must stay in one chunk.
- API-143: The client must chunk queries and metadata-only newest-envelope reads at 1000 topics.
- API-144: The client must open a static subscription per 10,000 topics.
- API-145: Phase 3 integration tests must cover every limit at the boundary and one past it.
- API-146: Key-package reads, inbox-id lookups, query paging, and static-subscription splitting are unchunked in the client today. The chunking in API-140, API-141, and API-144 and a `has_more` paging loop for identity-update and commit-log reads are new client work that lands with this API. The status-based retry classifier and per-topic client ledger must land in the same phase.
- API-148: The backend must reject an identity update for an inbox whose log already holds 256 entries with `INVALID_ARGUMENT` and reason `REASON_INVALID_IDENTITY_UPDATE`, as both existing backends do.
- API-147: The client must add a fifth topic kind for the commit log and publish and read commit-log entries as envelopes.

## 12. Error contract

| Condition | Code | Client action |
| --- | --- | --- |
| A request or an envelope violates this spec | `INVALID_ARGUMENT`, with a publish-error detail on publish | Do not retry |
| An identity update lost the commit-time check | `ABORTED` | Re-read, re-validate, resend |
| Backend or database unavailable, chain RPC failure | `UNAVAILABLE` | Retry with backoff |
| Rate limit (Phase 6) | `RESOURCE_EXHAUSTED` | Retry after the delay |
| Stream token bucket or slow consumer | `RESOURCE_EXHAUSTED` | Reconnect with backoff from durable per-topic cursors |
| Oversized response | `RESOURCE_EXHAUSTED` | Reduce read batch size or query limit, or surface the error; publish outcome may be committed |
| Unexpected storage invariant failure | `INTERNAL` | Surface the error; do not infer a partial success |

- API-150: The backend must not rewrite the message text of a status. The text must state the condition in plain words.
- API-151: A client must not retry `INVALID_ARGUMENT`.
- API-152: Every unimplemented endpoint of a deprecated API returns `UNIMPLEMENTED`.
- API-153: The client must classify status codes before it retries. `INVALID_ARGUMENT`, `UNIMPLEMENTED`, and `ABORTED` (except as API-052 states) are never retried at the transport layer. The client retries every status today; this change must land before any client depends on the backend.

## 13. Transport

- API-160: The backend serves gRPC and gRPC-Web on one port.
- API-161: The backend accepts the `x-app-version` and `x-libxmtp-version` headers and an authorization header on every call, including streams. Phase 6 defines their use.
- API-162: The backend serves the standard gRPC health service. It serves no version or metadata endpoint in v1.
- API-163: Envelopes are unsigned. The transport is trusted. Phase 6 authenticates the caller, not the envelope.
- API-164: There is no version wrapper on frames or payloads. The package name is the version.
- API-165: The public endpoint uses HTTPS. The deployment must pass gRPC-Web requests, CORS preflight, authorization/version headers, and status details. Streaming responses must not be buffered by the proxy. The backend plaintext port is a trusted internal endpoint.

## 14. Out of scope for v1

- Authentication, authorization, and per-caller rate limits (Phase 6). The per-stream protocol buckets in API-107 are included now.
- Retention behavior beyond `expiry_ns` on every row (Phase 5).
- Device-sync history storage. The history server stays a separate service. A later phase may fold it into the backend.
- A version or metadata endpoint for SDK gating. Decided 2026-09-04: left out of v1.

## Review log

| Date | Change |
| --- | --- |
| 2026-09-03 | Seeded from the Phase 0 proto review. Ordering model, wrappers, fresh payloads, idempotency, paging, `SubscribeStatic`, key-package retention, limits, and the error table were approved by the project owner in that review. Section 8 edge cases and the stream cap of 100,000 topics are carried from the review and await confirmation. A coverage check against the client caller and streaming catalogs added API-016, 023, 024, 045 (welcome keys), 063, 066 (drop rule), 067, 076, 090 (zero interval), 093 (waves), 100 (add plus remove), 103 to 105, 110 (no topics), 142 (measured size), 146, 147, and 153. |
| 2026-09-04 | Owner decisions from the questions review: retention is per topic kind (API-021, API-062; identity updates never expire, `expiry_ns` is zero for them); no version or metadata endpoint in v1 (API-162, section 14); envelopes stay unsigned (API-163 confirmed); the stream cap of 100,000 topics and the `Mutate` adds cap of 100,000 are confirmed. Open: retention periods for welcome messages and commit-log entries. |
| 2026-09-04 | Limits cross-check against the existing-behavior wiki and client code: added section 11.0, widened API-146, added API-148 (inbox log cap of 256). Four limits are under review by the owner. |
| 2026-09-04 | Owner decisions on the limits cross-check: publish has no count cap (API-031, API-142); query limit max 1000 (API-071); static subscription 10,000 topics (API-110, API-114, API-144); API-148 confirmed; new limits for envelope bytes (1 MiB), smart-contract-wallet signatures (100, API-044, API-123), waves in flight (256, API-093), and concurrent streams per connection (API-132); rate limits stay Phase 6 (API-133). Section 11.0 removed. |
| 2026-09-04 | PR review: API-016 rewritten for read replicas (a publish response means committed on the primary; replica reads may lag but still return a topic prefix); API-017 added (no read-your-writes across instances; validation runs on the primary). |
| 2026-09-04 | [Architecture review approved with comments](https://plan.ref.tools/xWi9jEu8VHmuLI0W). Keep replicas, database-clock timestamps, existing validation behavior, SCW caching, and per-stream token buckets. Use simple oversized-response errors. Add exact identity-history snapshots, duplicate-input rules, atomic duplicate outcomes, stream transitions, retention exemptions, and explicit client work. |
