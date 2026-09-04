# 004: Streaming

Status: approved on 2026-09-04 as the self-hosted stream contract.

Spec 001 defines wire types and limits. Spec 002 defines database visibility and tailer recovery. This spec defines subscription state and delivery order. Requirements use `STR-nnn`. A wave is the bounded catch-up work started by one mutation. A topic floor is the greatest sequence ID admitted for delivery on that topic in its current subscription.

## 1. Session and mutations

- STR-001: Every bidirectional stream starts with `Started`. It carries the keepalive interval and an empty capability list in v1. An interval of zero means the client uses its own default. An empty topic set stays open.
- STR-002: Process inbound mutations in receive order. Validate a whole mutation before changing state. Apply its removals and then its additions atomically. Exceeding a structural limit fails the stream with `INVALID_ARGUMENT`.
- STR-003: Count input adds and removes before coalescing duplicates. Repeated adds use their first occurrence. Repeated removes have the same effect as one remove. A remove for an absent topic is a no-op.
- STR-004: A mutation with additions must have a nonzero `mutate_id` that is not used by an in-flight wave. At most 256 waves can be in flight. Removes-only and no-op mutations may use zero. Every accepted mutation receives exactly one `CatchupComplete` while the stream remains healthy.
- STR-005: An add for a live topic with a cursor at or above its floor is a no-op. An add below its floor replaces that subscription, clears its floor, and replays from the supplied cursor. An explicit remove followed by add also clears the floor. A cursor above newest is valid: replay is empty and future delivery still respects the supplied cursor.
- STR-006: Except for the history-only collision rule in STR-018, an add for a topic already replaying replaces its old wave ownership and uses the new cursor. Old fetch results and buffered deliveries for that ownership must not emit new frames. The old wave still acknowledges after its remaining topics finish or are removed.
- STR-007: A topic removed during replay loses its ownership and pending work. Frames already handed to the ordered outbound queue can still arrive. After the removal's acknowledgement, no new frame from the removed ownership may appear. Bytes already sent cannot be retracted.
- STR-008: A stream holds at most 100,000 distinct topics, including active history-only topics. Replaced work must release its topic state. A mutation that breaches the cap fails without partial application.
- STR-009: An inbound request with an unset oneof fails with `INVALID_ARGUMENT`. An unknown future request represented as an unset oneof has the same outcome in v1.

## 2. Replay and live delivery

- STR-010: For each add, establish ownership and gate live delivery before reading that topic's replay ceiling. The ceiling is its committed watermark in the selected read database. Replay and the tailer use that same database instance.
- STR-011: Replay only rows above the supplied cursor and at or below the captured ceiling. A future cursor returns no replay. Process topics in bounded rotating turns so one topic does not monopolize replay work.
- STR-012: Live arrivals for replaying topics are buffered under that ownership. After replay, fold retained arrivals in topic sequence order and suppress duplicates with the topic floor. Concurrent arrivals must not pass the replay/fold boundary out of order.
- STR-013: Emit a wave's replay and fold frames first, then its `TopicsLive` markers, then its `CatchupComplete`. Only then emit live frames for those topics. Output queue order must preserve this barrier; changing an internal phase is not sufficient.
- STR-014: Each message frame has one wave ID or live ID zero. Never mix wave and live payloads in one frame. Sequence IDs ascend within each topic across frames, not only within a frame. Cross-topic order is not promised.
- STR-015: `TopicsLive` is informational. It names only topics that still belong to the completing wave. No more replay for that wave/topic follows the marker. Correctness and acknowledgement tracking must not depend on these markers.
- STR-016: Every lane checks and advances the same topic floor. Clear it on removal or replacement. Duplicates can still occur across reconnects and are removed by the client using its durable state.
- STR-017: A newer wave can take a topic from an older one. The older wave skips that topic in its fold and markers but still acknowledges. Acknowledgements across waves need not follow mutation order; clients correlate by `mutate_id`.
- STR-018: After applying removals, a history-only add naming an already subscribed topic, or any add naming a topic with an in-flight history-only wave, fails the stream with `INVALID_ARGUMENT`. Otherwise its finite replay ends at the captured ceiling and never enters live delivery. It does not buffer later live arrivals. Release its topics at completion and send its acknowledgement. Existing live topics not named by the mutation remain live.
- STR-019: A lost stream can omit acknowledgements. The client must not wait forever for them; it reconnects from durable per-topic cursors and submits new mutations. It must not infer progress on another topic from any received sequence ID.

## 3. Capacity and liveness

- STR-020: Bound pending, fetched, and outbound stream data as spec 002 defines. Pause replay while it can wait safely. If a required live batch cannot be retained, fail the stream with `RESOURCE_EXHAUSTED`. This also applies during an unfinished wave.
- STR-021: A legal envelope must fit a delivery frame including framing. A size failure must return an error. Never drop an envelope and advance its floor, or send a successful catch-up acknowledgement for skipped data.
- STR-022: Mutate frames and client Ping frames each use the separate per-stream bucket defined in spec 001: 10 frames/s, burst 100. Rejected frames close the stream with `RESOURCE_EXHAUSTED`. Pong frames do not consume either bucket. These protections are included before Phase 6 caller rate limits.
- STR-023: Either peer may send Ping. The receiver answers Pong with the same nonce. Only that nonce satisfies the pending challenge. Keep at most one server challenge outstanding; unrelated inbound traffic does not clear it.
- STR-024: The server's send-idle timer resets on frames admitted to outbound delivery, not on inbound traffic. The pong deadline begins at transport handoff of Ping. Before expiring the deadline, consume already available inbound frames once without blocking. Missing Pong closes the stream with `DEADLINE_EXCEEDED`.
- STR-025: Native request half-close stops new pings and accepts no more mutations. Finish already accepted waves and close within the configured drain time. Ongoing live traffic must not extend that time. Return `OK` only when accepted waves have completed; otherwise return `DEADLINE_EXCEEDED`.
- STR-026: Cancellation, shutdown, capacity failure, and transport failure deregister the session and cancel its work. Tailer failure closes affected streams with `UNAVAILABLE`, even if queries or keepalives can still work.

## 4. Static subscriptions and browsers

- STR-030: A static subscription accepts 1 to 10,000 topic/cursor entries. Validate and limit entries before coalescing; repeated topics use the first entry. It starts with `Started`, replays one finite wave, sends one `CatchupComplete`, and then stays live until cancellation or failure.
- STR-031: Use the same ownership, replay/live boundary, topic floors, ordering, and backpressure rules as the bidirectional engine. Static mode does not emit `TopicsLive`. Completion of the unary request is not a native half-close.
- STR-032: Send one-way Keepalive frames; no response is expected. After three keepalive intervals without a frame, the client reopens the stream from its durable cursors. If the advertised interval is zero, it uses its default. Data frames also establish activity.
- STR-033: To change topics, the client opens a replacement stream from durable cursors and cancels the old one. It tolerates overlap and drops duplicates. More than 10,000 topics require multiple streams. With no topics, the SDK keeps its logical subscription open and starts transport when a topic is added.
- STR-034: Browser clients use gRPC-Web server streaming. No full-duplex browser transport or WebSocket adapter is required. Public HTTPS, CORS preflight, allowed authorization/version headers, exposed status details, and proxy buffering settings must be tested together.

## 5. Client obligations and verification

- STR-040: Persist and resume per-topic cursors. The old per-kind total-order ledger is not valid. Stream and query processing use the same sequence space and durable topic progress.
- STR-041: Preserve public SDK callback and lifecycle behavior, including empty logical subscriptions, locally created conversations, sibling subscriptions, and pre-stream sync options. These are SDK concerns; the backend must not add client-specific transport paths.
- STR-042: Contract tests cover overlapping waves, replacement during fetch, removal with queued frames, below-floor re-add, future cursors, history-only work, both half-close modes, slow consumers, and late commits. Each test owns one behavior; bindings test their translation and lifecycle rather than duplicating all wire tests.

## Review record

[Approved architecture review](https://plan.ref.tools/xWi9jEu8VHmuLI0W), 2026-09-04. Stream transition rules are approved before Phase 2 implementation. The owner retained replicas and the original per-stream token buckets.
