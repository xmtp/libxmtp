# 001: Backend API

Status: draft, seeded from the Phase 0 proto review. Approved sections are marked in the review log at the end.

This spec states the public API of the self-hosted XMTP backend: what a client sends, what the backend stores, what it returns, and when it fails. The wire schema is `xmtp.backend.v1` (draft in `docs/self-hosted/backend.proto` until Phase 1 moves it into the `proto` folder). Spec 002 covers the backend's internal design. Spec 004 covers stream semantics in full; this spec states the rules a client must obey.

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

## 3. Ordering and visibility

- API-010: Sequence ids come from one sequence and are unique across all topics.
- API-011: Within a topic, an envelope becomes visible only when every envelope with a lower sequence id on that topic has committed or aborted. Every read of a topic returns a prefix of that topic, minus gaps left by aborted writes.
- API-012: Across topics, order is not defined. A client must never infer from a sequence id on one topic that any envelope on another topic has been delivered.
- API-013: Gaps in sequence ids are normal and carry no meaning.
- API-014: Identity updates are serialized across all inboxes. Among identity updates, sequence id order equals commit order.
- API-015: The backend must serve a read that starts at a cursor above the topic's newest sequence id as an empty result, not an error.
- API-016: When a publish response returns, every envelope it stored is visible to every later read of its topic, from any backend instance. A client that publishes a key package, waits for the response, and then publishes the identity update is therefore guaranteed that the key package is readable before the identity update is.

## 4. Envelope metadata

Every stored envelope carries the metadata below. The backend assigns all of it.

| Field | Rule |
| --- | --- |
| `cursor` | The sequence id. |
| `server_ns` | Assigned by the database inside the insert, in nanoseconds. Strictly increasing with the sequence id within a topic. Across topics it is database wall-clock time. The client uses it as the envelope's created time. |
| `message_hash` | SHA-256 of the stored envelope bytes. |
| `topic` | The derived topic. |
| `expiry_ns` | The time the backend deletes the row: `server_ns` plus the retention period. Not a message expiry. |
| `is_commit` | Group messages only: true when the MLS message is a commit. False for every other kind. |

- API-020: The backend must store the canonical protobuf re-encoding of the client envelope and must compute `message_hash` over exactly those bytes.
- API-021: The retention period is one server configuration value with a default of 3 months. It applies to every kind. Phase 5 defines what happens to a cursor that points below deleted rows.
- API-022: A client should store `expiry_ns` but must not act on it before Phase 5 defines the behavior.
- API-023: Canonical re-encoding applies to the protobuf framing only. The backend must return every payload byte field (group message data, welcome data, key package bytes, commit-log entry bytes) exactly as received.
- API-024: The client must match its own published messages by `message_hash`, computed over the same canonical envelope encoding, and must store the hash the backend returns as the authoritative value.

Until Phase 5 the backend has no retained-floor signal and the client has no gap detection. A cursor that points below deleted rows silently skips them. For a group message that is a commit, that is a permanent fork. Phase 5 must close this before retention is enabled in production.

## 5. Publish

### 5.1 Atomicity and idempotency

- API-030: A publish request is atomic. Either every envelope in it is stored or none is.
- API-031: A publish request must contain at most 50 envelopes and at most 25 MiB. A larger request fails with `INVALID_ARGUMENT` and no envelope is stored.
- API-032: The response lists one metadata entry per envelope, in request order.
- API-033: An envelope whose `(topic, message_hash)` is already stored is a duplicate. The backend must not store it again and must return the stored metadata as success.
- API-034: Two identical envelopes in one request collapse to one stored row. Both response entries carry the same metadata.
- API-035: The duplicate check must run before validation and again at commit time, so a copy that commits during validation is still answered as a duplicate.

Because the hash covers the whole envelope, a re-signed or re-encrypted copy of the same logical message is a new envelope, not a duplicate. Group messages are deduplicated by the client using the MLS message id, and commit-log entries by their commit sequence id, so this is safe. A client that re-encrypts a message after a failed publish must tolerate the earlier copy arriving later as a message it did not match to an intent.

### 5.2 Validation

- API-040: The backend must parse every envelope. A payload that does not parse fails with `INVALID_ARGUMENT`, reason `MALFORMED_PAYLOAD`.
- API-041: A group message must be a valid MLS protocol message. The backend derives the group id and `is_commit` from the parse. The backend does not verify group membership or the MLS signature; MLS confidentiality makes that impossible without the group key.
- API-042: A key package must pass key-package validation. A failure is `INVALID_ARGUMENT`, reason `INVALID_KEY_PACKAGE`.
- API-043: An identity update must apply cleanly to the inbox's current association state, read from the inbox's identity topic. A failure is `INVALID_ARGUMENT`, reason `INVALID_IDENTITY_UPDATE`. A signature failure is reason `INVALID_SIGNATURE`.
- API-044: Smart-contract-wallet signatures inside an identity update are verified over chain RPC. A chain RPC failure is `UNAVAILABLE`, not `INVALID_ARGUMENT`.
- API-045: A welcome is stored without validation beyond parsing. The backend must not check that a welcome's installation key belongs to a registered installation; welcome pointers are addressed to random 32-byte values by design.
- API-046: A commit-log entry must parse as a plaintext commit-log entry that carries a group id. Its signature is stored and returned, not verified.
- API-047: The error detail for an `INVALID_ARGUMENT` names the index of the first failing envelope and a reason code. A request-level error carries no index.

### 5.3 Identity updates

- API-050: A publish request must contain at most one identity update per inbox. Two updates for one inbox in one request fail with `INVALID_ARGUMENT`.
- API-051: The backend validates an identity update against the inbox's state as of a read sequence id. At commit time, under the identity serialization lock, it checks that the inbox's newest sequence id still equals that value. If it does not, the request fails with `ABORTED` and nothing is stored.
- API-052: On `ABORTED`, the client must re-read the inbox's identity topic, re-validate the update against the new state, and resend it.
- API-053: An identifier may be associated with more than one inbox over time. The backend does not enforce exclusivity. Identifier resolution returns the inbox with the latest association.

### 5.4 Key packages

- API-060: Every key-package upload is stored. The backend does not delete the previous key package for an installation on upload.
- API-061: The newest key package for an installation is the one with the highest sequence id on its topic.
- API-062: Key packages expire by the retention period like every other kind. A client must re-upload well inside that period.
- API-063: The client must read the newest-envelope response as a map from topic to an optional key package. It must not require the response to have the same length as the request, and it must not fall back to one request per key.

### 5.5 Commit log

- API-065: The commit-log position of an entry is its sequence id. The client stores that value as the entry's log position.
- API-066: Commit-log entries for one group are totally ordered by the rules in section 3. Epoch continuity and hash-chain checks are client concerns. A skipped entry permanently disables a group's fork detection on that client, so the backend must never drop or reorder an entry on a commit-log topic.
- API-067: The client must take a commit-log entry's position from the envelope metadata. The entry itself carries no sequence id.

## 6. Query

- API-070: A query names up to 1000 `(topic, cursor)` pairs and a `limit`. A request with more than 1000 pairs fails with `INVALID_ARGUMENT`.
- API-071: `limit` is the total number of envelopes across all topics. The maximum and the default are 100. A larger value is clamped to 100.
- API-072: The result is the union of all envelopes with `sequence_id > cursor` on their topic, ascending by sequence id within each topic, cut at `limit`. The order across topics carries no meaning.
- API-073: `continuation.has_more` is true when more envelopes matched than `limit` allowed. The backend must compute it from the same read as the page.
- API-074: Client rule: for each topic that returned rows, set its cursor to the highest sequence id returned for that topic. Leave every other cursor unchanged. When `has_more` is true, query again with the updated cursors.
- API-075: A query on a key-package topic is not supported and fails with `INVALID_ARGUMENT`. Use the newest-envelope read.

API-074 is safe under per-topic order: a topic's cursor moves only when that topic's own rows are returned, and those rows are in order. It makes progress because `has_more` implies at least one row was returned, so a loop that repeats the query until `has_more` is false terminates once every topic is drained.

- API-076: `limit` bounds the whole response, not each topic. A client that needs every envelope above its cursors on many topics must loop on `has_more`. A client that needs a fixed page per topic must query that topic alone.

## 7. Newest envelope

- API-080: A newest-envelope request names up to 1000 topics when it asks for metadata only, and up to 100 topics when it asks for full envelopes. A larger request fails with `INVALID_ARGUMENT`.
- API-081: The response holds one result per topic that has at least one envelope. A topic with no envelope is absent from the response.
- API-082: With `include_full_envelope` false, every result carries metadata only. With it true, every result carries metadata and the envelope.
- API-083: The newest envelope of a topic is the visible envelope with the highest sequence id.

## 8. Subscribe (bidirectional)

The bidirectional stream follows XIP-83. Spec 004 states the full protocol. The rules below are the API contract.

- API-090: The first frame on every stream is `Started`, carrying the server's keepalive interval and its capability list. In v1 the capability list is empty. A keepalive interval of 0 means the server advertises none and the client uses its own default.
- API-091: A `Mutate` frame adds and removes topics atomically. Adds carry a cursor; the server replays every envelope above the cursor, then delivers live. Removes clear the topic's cursor floor.
- API-092: A `Mutate` must carry at most 100,000 adds and at most 100,000 removes. A stream must hold at most 100,000 topics. A violation fails the stream with `INVALID_ARGUMENT`.
- API-093: `mutate_id` must be nonzero when adds are present and must not equal the id of a wave still in flight. A violation fails the stream with `INVALID_ARGUMENT`. The number of waves in flight on one stream is unbounded in v1.
- API-094: Every `Mutate` is acknowledged with exactly one `CatchupComplete` carrying its `mutate_id`, including removes-only and no-op mutations.
- API-095: A delivery frame belongs to exactly one wave or to live. The frame's `mutate_id` is the wave's id, or 0 for live. The server never mixes lanes in one frame.
- API-096: Within a frame, envelopes of one topic are ascending by sequence id. Topics may interleave in any order.
- API-097: `TopicsLive` names topics whose replay is complete. It is informational. Live frames for a wave's topics begin only after the wave's `CatchupComplete`.
- API-098: With `history_only` true, the adds are replayed and acknowledged but not registered for live delivery. With a half-closed request stream, the server closes the stream after the wave.
- API-099: Either peer may send `Ping`. The receiver must answer with `Pong` carrying the same nonce. A peer that receives no `Pong` within its deadline closes the stream.
- API-100: A cursor above the topic's newest sequence id replays nothing and is not an error. A remove for a topic that is not subscribed is a no-op. A duplicate add inside one `Mutate` coalesces with the first occurrence. An add for a topic already live, with a cursor not below its floor, is a no-op. A topic in both the adds and the removes of one `Mutate` is applied as remove then add: its floor is cleared and it replays from the add's cursor.
- API-101: The number of concurrent streams per connection is unbounded in v1. Phase 6 adds a quota.
- API-102: A client recovers from backpressure or stream loss by opening a new stream from its durable per-topic cursors. Duplicates across the overlap are the client's to drop.
- API-103: Per-topic cursor floors on a stream are independent. The client must track its last-seen position per topic and must not derive a shared watermark across topics of one kind.
- API-104: A bidirectional stream with no subscribed topics stays open.
- API-105: Expiry does not affect stream delivery. An envelope delivered before its `expiry_ns` is never retracted.

## 9. Subscribe (static)

The static stream serves clients that cannot open a bidirectional stream.

- API-110: A static subscription names a fixed list of up to 1000 `(topic, cursor)` pairs. Zero topics or more than 1000 fails with `INVALID_ARGUMENT`. A client with no topics opens no stream until it has one.
- API-111: The first frame is `Started`, carrying the keepalive interval.
- API-112: The server replays every envelope above each cursor, sends one `CatchupComplete`, then delivers live until the client cancels.
- API-113: `Keepalive` frames flow from server to client only and are never answered. The client reopens the stream after three keepalive intervals of silence.
- API-114: To change its topic set, a client opens a new stream from its durable cursors and cancels the old one. A client with more than 1000 topics opens more than one stream.

## 10. Identity reads

- API-120: An inbox-id lookup names up to 250 identifiers, each with its kind. More than 250 fails with `INVALID_ARGUMENT`.
- API-121: The response has one entry per request entry, in order, echoing the identifier and its kind. The inbox id is absent when the identifier has no association.
- API-122: An identifier resolves to the inbox with the latest association. Revoked associations do not resolve.
- API-123: Smart-contract-wallet signature verification takes a list of signatures and returns one result per signature, in order. A chain RPC failure is `UNAVAILABLE`.

## 11. Limits

| Limit | Value |
| --- | --- |
| Query topics per request | 1000 |
| Query limit | max 100, default 100 |
| Newest-envelope topics, metadata only | 1000 |
| Newest-envelope topics, full envelopes | 100 |
| Publish envelopes per request | 50 |
| Request and response bytes | 25 MiB |
| Mutate adds per frame | 100,000 |
| Mutate removes per frame | 100,000 |
| Topics per bidirectional stream | 100,000 |
| Static-subscription topics per request | 1000 |
| Inbox-id lookup identifiers | 250 |
| Keepalive interval | 30 s |

- API-130: The backend must reject a unary request above a limit with `INVALID_ARGUMENT` and must fail a stream above a limit with `INVALID_ARGUMENT`.
- API-131: Every limit is one named configuration value. No limit is a literal in code.

### 11.1 Client chunking requirements

- API-140: The client must chunk newest-envelope reads with full envelopes at 100 topics and must cap the number of chunks in flight.
- API-141: The client must chunk inbox-id lookups at 250 identifiers.
- API-142: The client must chunk publishes at 50 envelopes and must cap the number of chunks in flight. It must measure the encoded request size rather than estimate it, and it must re-chunk on a `TOO_LARGE` reason.
- API-143: The client must chunk queries and metadata-only newest-envelope reads at 1000 topics.
- API-144: The client must open a static subscription per 1000 topics.
- API-145: Phase 3 integration tests must cover every limit at the boundary and one past it.
- API-146: Key-package reads and inbox-id lookups are unchunked in the client today. The chunking in API-140 and API-141 is new client work that lands with this API.
- API-147: The client must add a fifth topic kind for the commit log and publish and read commit-log entries as envelopes.

## 12. Error contract

| Condition | Code | Client action |
| --- | --- | --- |
| A request or an envelope violates this spec | `INVALID_ARGUMENT`, with a publish-error detail on publish | Do not retry |
| An identity update lost the commit-time check | `ABORTED` | Re-read, re-validate, resend |
| Backend or database unavailable, chain RPC failure | `UNAVAILABLE` | Retry with backoff |
| Rate limit (Phase 6) | `RESOURCE_EXHAUSTED` | Retry after the delay |

- API-150: The backend must not rewrite the message text of a status. The text must state the condition in plain words.
- API-151: A client must not retry `INVALID_ARGUMENT`.
- API-152: Every unimplemented endpoint of a deprecated API returns `UNIMPLEMENTED`.
- API-153: The client must classify status codes before it retries. `INVALID_ARGUMENT`, `UNIMPLEMENTED`, and `ABORTED` (except as API-052 states) are never retried at the transport layer. The client retries every status today; this change must land before any client depends on the backend.

## 13. Transport

- API-160: The backend serves gRPC and gRPC-Web on one port.
- API-161: The backend accepts the `x-app-version` and `x-libxmtp-version` headers and an authorization header on every call, including streams. Phase 6 defines their use.
- API-162: The backend serves the standard gRPC health service.
- API-163: Envelopes are unsigned. The transport is trusted. Phase 6 authenticates the caller, not the envelope.
- API-164: There is no version wrapper on frames or payloads. The package name is the version.

## 14. Out of scope for v1

- Authentication, authorization, and rate limits (Phase 6).
- Retention behavior beyond `expiry_ns` on every row (Phase 5).
- Device-sync history storage. The history server stays a separate service. A later phase may fold it into the backend.
- A version or metadata endpoint for SDK gating. Open question for Phase 6.

## Review log

| Date | Change |
| --- | --- |
| 2026-09-03 | Seeded from the Phase 0 proto review. Ordering model, wrappers, fresh payloads, idempotency, paging, `SubscribeStatic`, key-package retention, limits, and the error table were approved by the project owner in that review. Section 8 edge cases and the stream cap of 100,000 topics are carried from the review and await confirmation. A coverage check against the client caller and streaming catalogs added API-016, 023, 024, 045 (welcome keys), 063, 066 (drop rule), 067, 076, 090 (zero interval), 093 (waves), 100 (add plus remove), 103 to 105, 110 (no topics), 142 (measured size), 146, 147, and 153. |
