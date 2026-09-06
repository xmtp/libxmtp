# 004: Streaming

Status: approved on 2026-09-06, with the owner comments in the review record.

One logical client owns one ingestion cursor per topic. Local application streams share that ingestion state. Independent cursors for multiple downstream clients on one upstream stream are outside this contract.

Spec 001 defines wire types and public limits. Spec 002 defines storage visibility, tailer recovery, and bounded fetch turns. Requirements use `STR-nnn`.

## 1. Session and interest updates

- STR-001: Every bidirectional stream starts with `Started(keepalive_interval_ms)`. No topics are registered yet. Zero means the client uses its default interval. An empty interest set stays open.
- STR-002: Process `Update` requests in receive order. Validate each update before applying it atomically. Each topic occurs at most once across adds and removes. Duplicate or overlapping entries fail with `INVALID_ARGUMENT`.
- STR-003: Every update ID is nonzero and strictly greater than the previous ID on this connection. Invalid IDs fail the stream. A new connection can restart its IDs.
- STR-004: Adding an absent topic supplies an exclusive starting cursor C. Register the topic before capturing its visible head H from the selected read database. Queue `Applied` before any message for that registration. A publication racing registration must not be lost.
- STR-005: Adding an active topic is a no-op, regardless of cursor. It changes neither its delivery position nor its initial catch-up target. The protocol has no seek operation for an active topic.
- STR-006: Every accepted update receives one `Applied` with the same ID while the stream remains healthy. Acknowledgements follow update order and do not wait for history delivery. `added_targets` contains only newly registered topics, in add order; a new empty topic has target zero. An empty result does not mean existing topics finished processing.
- STR-007: Removing an absent topic is a no-op. Removing an active topic cancels its pending work. Already queued messages may precede its acknowledgement. No message from the removed registration may follow that acknowledgement. Bytes already sent cannot be retracted.
- STR-008: A later add of a removed topic creates a new registration from its supplied cursor. Internal generation checks discard stale fetch results. Ordered acknowledgement boundaries distinguish registrations without tagging every message.
- STR-009: An unset inbound oneof or a structural limit violation fails with `INVALID_ARGUMENT`. Keep the spec 001 add/remove and topic-count limits. There is no wave-count limit.

## 2. Ordered delivery and fixed targets

- STR-010: While a registration remains active, deliver every retained envelope above C in strictly increasing topic sequence order across frames, or explicitly fail the stream. Initialize its delivery floor to C. Gaps between sequence numbers are legal; cross-topic order is unspecified.
- STR-011: Historical and newly published envelopes use the same `Messages` frame. There are no wave IDs, replay/live tags, `TopicsLive`, or `CatchupComplete` frames. A topic can continue delivering after its initial history without waiting for another topic.
- STR-012: H is fixed for the registration. It is the head observed in the serving database, including replica lag, not a promise about the primary's current head. Separate updates do not form one global snapshot. A cursor at or above H owes no initial history and still filters future delivery at or below C.
- STR-013: Catch-up targets describe processing obligations. The SDK reports a topic caught up only after safely processing the requested range through H. Receiving `Applied` or receiving the last envelope alone does not establish processing completion. New publications do not move H.
- STR-014: Expose catch-up status for the application's current interest set. It includes pending add acknowledgements, unfinished targets, and processing that discovers more groups. Register discovered work before completing its parent welcome. A new topic starts its own catch-up; completed topics do not restart.
- STR-015: Removing a topic cancels its outstanding obligation. Cancellation or connection failure must not be reported as successful processing of that history. Connection failure has a distinct status; reconnect produces new targets.
- STR-016: Use bounded fair turns for topics with pending catch-up. A served topic returns to the back of the ready queue when it needs more work. Newly ready topics also join the back. Topics not visited before a byte cutoff retain priority. Do not repeatedly select the first topics by topic or sequence-ID sort order. Spec 002 supplies batch and byte bounds.

Fairness prevents starvation among ready topics; it does not promise equal throughput or a fixed latency. Shared connection bandwidth and a slow consumer still affect the whole stream.

## 3. Client processing and application choice

- STR-020: The client maintains one ordered ingestion path per topic and persists safe progress with the corresponding local state changes. Transient processing failure leaves the cursor before unfinished work. Existing terminal-error rules still apply; no new payload validation is introduced.
- STR-021: Application developers choose topics and filters. Consent and membership can inform that choice, but streaming denied topics is allowed. The streaming protocol neither authorizes group membership nor forces removal when consent changes. Existing MLS authentication and decryptability rules still apply.
- STR-022: When the selected interest set removes a topic, stop scheduling callbacks for that registration immediately, then send the remove. An already executing callback cannot be undone. A quick remove/re-add must not make old queued callbacks eligible again; keep the new registration pending until its add acknowledgement.
- STR-023: Local application streams receive independent callbacks from shared ingestion. A later local subscriber does not rewind the upstream topic. Preserve SDK entry points, application-selected filters, and lifecycle callbacks. Use local history or sync APIs for historical reads.
- STR-024: Adapt the old lease ledger and catch-up-window dedup to this model. Keep recovery sync until safe ordered processing and cursor advancement are established. No exactly-once application callback guarantee is made across crashes.

## 4. Capacity, liveness, and termination

- STR-030: Retain the configured fetched-data and outbound bounds. A fetch turn is not permission to materialize an arbitrary payload volume. Pause work only while required state remains safe; otherwise fail with `RESOURCE_EXHAUSTED`. Never skip an envelope and advance its floor.
- STR-031: Every legal envelope must fit a delivery frame with metadata and framing. Oversized responses return the existing size error. A slow stream must not block the shared tailer.
- STR-032: Update and client Ping each use a per-stream bucket of 10 frames/s with burst 100. Exhaustion closes the stream with `RESOURCE_EXHAUSTED`. Pong consumes neither bucket. Caller quotas remain Phase 6 work.
- STR-033: Either peer may send Ping. Reply with Pong carrying the same nonce. Keep at most one server challenge outstanding. Unrelated inbound traffic does not satisfy it.
- STR-034: Reset the server send-idle timer on outbound admission, not inbound traffic. Start the pong deadline at Ping transport handoff. Before expiring it, consume already available inbound frames once without blocking. Missing Pong closes with `DEADLINE_EXCEEDED`.
- STR-035: Cancellation, native request half-close, shutdown, and failure end the session and deregister its topics. Half-close is not a catch-up command and does not wait for targets. Tailer or database failure closes affected streams with `UNAVAILABLE`.
- STR-036: Reconnect with backoff, the current desired topic set, and safe durable cursors. Deduplicate overlap from local state. Do not resume from the greatest merely received sequence ID or infer progress on another topic.

## 5. Bounded SDK sync

- STR-040: A catch-up-then-stop operation uses a dedicated instance of the same stream. Enroll its starting topics and process through their returned targets. Welcomes within those targets can discover groups; add those groups and include their targets in the run.
- STR-041: Later traffic and unrelated locally created groups do not extend the run. Remove completed topics when useful, then cancel after every enrolled processing obligation is complete. Cancellation alone is not evidence of success. There is no `history_only` mode or special server drain protocol.

## 6. Static browser subscriptions

- STR-050: A static request supplies 1 to 10,000 unique topic/cursor pairs. Empty sets, duplicates, and excess entries fail with `INVALID_ARGUMENT`.
- STR-051: Its first `Started` frame supplies the keepalive interval and one fixed target per requested topic, in request order, including empty topics. It precedes every message. The stream then delivers ordinary envelopes until cancellation or failure.
- STR-052: Use the same per-topic ordering, fixed targets, processing-completion meaning, fair fetch turns, and capacity rules as native streams. The end of the unary request does not trigger native half-close.
- STR-053: Keepalive frames are one-way. Reopen after three keepalive intervals without any frame; data also proves activity. Use the client default for interval zero.
- STR-054: Changing topics opens a replacement stream from durable cursors and cancels the old one. The SDK handles overlap. Split more than 10,000 topics across streams. An empty logical subscription waits locally for its first topic.
- STR-055: Use the existing gRPC-Web transport, HTTPS, CORS/header handling, and unbuffered proxy configuration. No WebSocket adapter or separate browser control service is required.

## 7. Verification

Contract tests cover registration races; independent topic progress during long catch-up; bounded fair batching with hot, idle, and byte-heavy topics; finite catch-up under continuous publication; slow local processing; empty topics and future cursors; active-topic no-ops; remove/re-add with queued work; explicitly selected denied topics; reconnect with unprocessed data; bounded sync discovery; and matching native/browser completion behavior.

Test each behavior once at its owning layer. Bindings test translation and application lifecycle rather than duplicating the complete wire suite.

## Review record

[Single-client streaming proposal](https://plan.ref.tools/BbNc54CedfhM1Snb), approved 2026-09-06. Replace the undeployed XIP-83 design completely, without reserved proto fields. Define bounded fair batching and preserve application control of the interest set, including denied topics.
