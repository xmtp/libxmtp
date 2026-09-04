<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# XIP-83: Mutable Subscription Streams with Liveness

**Source of truth**: fetched successfully from
`https://raw.githubusercontent.com/xmtp/XIPs/tyler/xip-83-mutable-subscription-streams/XIPs/xip-83-mutable-subscription-streams.md`
(766 lines). PR: <https://github.com/xmtp/XIPs/pull/139>, branch `tyler/xip-83-mutable-subscription-streams`,
title "XIP-83: Mutable subscription streams with liveness", status **Draft**, author Tyler Hawkes (@tylerhawkes),
created 2026-06-01, type Standards / category Network. Nothing in this document is reconstructed; every
normative statement below is taken from that file.

**xmtpd implementation compared**: repository `/Users/nickmolnar/code/xmtp/xmtpd`, commit
`822ddc956a4408c8465d911bdf0e44fc2100bd7c` — "feat(api): XIP-83 Subscribe — tag deliveries with wave
mutate_id and enforce ordering guarantees (#2035)".

---

## Table of contents

1. [Why the XIP exists](#1-why-the-xip-exists)
2. [The two bindings](#2-the-two-bindings)
3. [Wire format (message types)](#3-wire-format-message-types)
4. [State machine](#4-state-machine)
5. [Server requirements (all 11)](#5-server-requirements-all-11)
6. [Client requirements (all 4)](#6-client-requirements-all-4)
7. [Core concepts in detail](#7-core-concepts-in-detail)
   - [7.1 mutate_id and delivery tagging](#71-mutate_id-and-delivery-tagging)
   - [7.2 Waves and wave semantics](#72-waves-and-wave-semantics)
   - [7.3 Cursor floor rules](#73-cursor-floor-rules)
   - [7.4 Ordering, duplicate, and gap guarantees](#74-ordering-duplicate-and-gap-guarantees)
   - [7.5 The seam (crossover)](#75-the-seam-crossover)
   - [7.6 TopicsLive](#76-topicslive)
   - [7.7 CatchupComplete](#77-catchupcomplete)
   - [7.8 Ping / Pong](#78-ping--pong)
   - [7.9 history_only and half-close](#79-history_only-and-half-close)
8. [Error handling and status codes](#8-error-handling-and-status-codes)
9. [d14n binding differences](#9-d14n-binding-differences)
10. [Test cases in the spec](#10-test-cases-in-the-spec)
11. [What xmtpd actually implements](#11-what-xmtpd-actually-implements)
12. [Conformance matrix](#12-conformance-matrix)
13. [Deviations, gaps, and additions in xmtpd](#13-deviations-gaps-and-additions-in-xmtpd)
14. [What a new single-Postgres backend should keep or drop](#14-what-a-new-single-postgres-backend-should-keep-or-drop)

---

## 1. Why the XIP exists

The spec (Abstract, Motivation) states two deficiencies of the existing server-streaming subscription RPCs
(`SubscribeGroupMessages`, `SubscribeWelcomeMessages` on v3; `SubscribeTopics` on d14n):

1. **Silent stream death.** A terminating L7 proxy answers HTTP/2 keepalive pings at the edge while the
   backend subscription is already gone. The client sees neither an error nor a close; it just stops
   receiving. Transport keepalives cannot detect this, "precisely because a terminating proxy answers them
   without the origin's participation; only an **application-level payload from the origin** proves the
   subscription is still being served end-to-end."
2. **Subscription churn on membership change.** The topic set is fixed at stream open, so joining a group
   forces a full teardown and reopen — "an O(membership-changes) sequence of reconnects — each a fresh
   stream that must re-run catch-up and is itself a new opportunity for silent death."

The driving deployment is a **multi-tenant agent gateway** ("herald"): one process hosting many XMTP
identities that wants **one** long-lived stream carrying the **union** of its hosted identities'
subscriptions, rather than N×M streams.

---

## 2. The two bindings

XIP-83 is explicitly "a **control protocol**, not a single RPC", with two backend bindings:

| Binding | Service | RPC | Delivery type | Cursor type |
| --- | --- | --- | --- | --- |
| v3 (centralized) | `xmtp.mls.api.v1.MlsApi` | `Subscribe(stream SubscribeRequest) returns (stream SubscribeResponse)` | `GroupMessage` / `WelcomeMessage` | scalar `uint64 id_cursor` |
| d14n (decentralized) | `xmtp.xmtpv4.message_api.QueryApi` | `Subscribe(stream SubscribeRequest) returns (stream SubscribeResponse)` | `OriginatorEnvelope` | `Cursor` = `map<uint32,uint64> node_id_to_sequence_id` |

"The **control protocol is identical** — `Mutate` (adds/removes, `history_only`, `mutate_id`), `Started`,
`CatchupComplete`, `TopicsLive`, `Ping` / `Pong`, the catch-up waves, and the half-close bounded catch-up —
and every server and client requirement above applies unchanged."

Both bindings are **additive**. `SubscribeTopics` (d14n) and `SubscribeGroupMessages` /
`SubscribeWelcomeMessages` (v3) are unchanged. A client calling `Subscribe` against a node that does not
implement it gets `UNIMPLEMENTED` and falls back. Browser / gRPC-web clients that cannot do bidirectional
streaming stay on the old server-streaming RPCs behind a client-side watchdog.

**xmtpd implements only the d14n binding**, on `QueryApi.Subscribe`. This is what this document compares.

---

## 3. Wire format (message types)

The spec's proto (v3 flavour; the d14n flavour is structurally identical but with `OriginatorEnvelope`
delivery and `Cursor` last_seen):

```protobuf
service MlsApi {
  rpc Subscribe(stream SubscribeRequest) returns (stream SubscribeResponse) {}
}

// client -> server, sent one or more times over the life of the stream.
message SubscribeRequest {
  oneof version { V1 v1 = 1; }

  message V1 {
    oneof request {
      Mutate mutate = 1;
      Ping   ping   = 2; // liveness challenge (e.g. probe the link after resuming)
      Pong   pong   = 3; // answer to a server Ping
    }

    message Mutate {
      repeated Subscription adds    = 1; // begin delivering these topics
      repeated bytes        removes = 2; // stop delivering; clears the floor so a re-add replays
      bool             history_only = 3; // catch the adds up, but do not deliver live (req 11)
      uint64           mutate_id    = 4; // stamps the wave's replay frames + CatchupComplete

      message Subscription { bytes topic = 1; uint64 id_cursor = 2; } // cursor 0 = from the beginning
    }
  }
}

// server -> client
message SubscribeResponse {
  oneof version { V1 v1 = 1; }

  message V1 {
    oneof response {
      Messages        messages         = 1;
      Started         started          = 2; // sent once, immediately on open
      Ping            ping             = 3; // idle liveness challenge; receiver MUST answer with Pong
      Pong            pong             = 4; // answer to a client Ping
      TopicsLive      topics_live      = 5; // no more replay for these topics
      CatchupComplete catchup_complete = 6; // acks a Mutate; echoes mutate_id
    }

    message Messages {
      repeated GroupMessage   group_messages   = 1;
      repeated WelcomeMessage welcome_messages = 2;
      uint64                  mutate_id        = 3; // wave tag; 0 = live
    }

    message TopicsLive { repeated bytes topics = 1; }

    message Started {
      uint32 keepalive_interval_ms = 1;
      repeated Capability capabilities = 2;
    }

    message CatchupComplete { uint64 mutate_id = 1; }

    enum Capability { CAPABILITY_UNSPECIFIED = 0; }
  }
}

message Ping { uint64 nonce = 1; }
message Pong { uint64 nonce = 1; } // echoes the nonce of the Ping it answers
```

### Topic representation

Subscriptions carry **kind-prefixed binary topics** — "the representation the decentralized backend already
uses (XIP-49 §3.3.2) ... one kind byte plus the raw identifier, with no string formatting." Kind `0x00` =
group messages (identifier = group_id), kind `0x01` = welcomes (identifier = installation_key). "an
unsupported kind fails the stream with `INVALID_ARGUMENT`." "Future kinds arrive via `Started.capabilities`."

On d14n, "Topics need no translation ... the XIP-49 kind-prefixed binary topic ... *is* the decentralized
backend's native topic representation."

### Version pinning

Both request and response wrap their payload in `oneof version { V1 v1 = 1; }`. "The version is **pinned per
stream**: a stream whose requests are `V1` receives only `V1` responses." `Ping` / `Pong` are declared at top
level and are **version-independent**. The sole exception to pinning is the initial `Started`, which the node
sends "in the base version before it has read any request".

---

## 4. State machine

The spec gives a client-side state diagram:

```text
[*] --> Opening: open Subscribe()
Opening --> CatchingUp: Started + Mutate(add)
CatchingUp --> Live: CatchupComplete
Live --> Live: Messages / Mutate(add, remove)
Live --> Live: Ping and Pong (either direction)
Live --> Stale: no frame for N x interval
Live --> Suspended: process backgrounded
Suspended --> Resuming: process foregrounded
Resuming --> Live: resume-probe Ping answered (socket survived, rare)
Resuming --> Stale: resume-probe Ping unanswered or write fails
Stale --> Reconnecting: close (on_close triggers reconnect)
Reconnecting --> Opening: resume from durable state
```

And a per-topic lifecycle implied by requirements 2/4/5/9:

- **gated / replaying** — a wave owns the topic; only wave-tagged (`mutate_id != 0`) replay frames may carry
  its messages. Live messages that arrive are either folded into the wave or held.
- **live** — after the owning wave's `CatchupComplete`; frames carry `mutate_id = 0`.
- **removed** — the floor is discarded; a later add starts a fresh catch-up and replays.

The canonical sequence (spec Overview mermaid):

```text
C->>N: open bidi stream Subscribe()
C->>N: Mutate (adds g1, g2 @cursors, mutate_id=1)
N-->>C: Started                        # immediate
N-->>C: Messages (catch-up, mutate_id=1)
N-->>C: TopicsLive (g1, g2)
N-->>C: CatchupComplete (mutate_id=1)
C->>N: Mutate (adds g3, mutate_id=2)   # no reconnect
N-->>C: Messages (g3 catch-up, mutate_id=2)
N-->>C: TopicsLive (g3)
N-->>C: CatchupComplete (mutate_id=2)
N-->>C: Messages (g3 live, mutate_id=0)
N-->>C: Ping (nonce=k)                 # idle, every 30s or less
C->>N: Pong (nonce=k)
C->>N: Mutate (removes g1)
N-->>C: CatchupComplete (mutate_id=0)  # immediate ack, waveless Mutate
C->>N: Ping (nonce=j)                  # client-initiated probe
N-->>C: Pong (nonce=j)
```

---

## 5. Server requirements (all 11)

Condensed but faithful; the normative force (MUST / SHOULD / MAY) is preserved.

**Req 1 — Immediate `Started`.** The node MUST send `Started` immediately on accepting the stream, before any
catch-up, "so that proxied/buffered transports keep the connection open". It MUST advertise
`keepalive_interval_ms` whenever it sends Pings, and MUST list supported optional features in `capabilities`.
A client MUST NOT send an optional request type whose capability was not advertised.

**Req 2 — Validate, deliver above cursor, no duplicates.** For each `adds` subscription the node MUST
validate the kind-prefixed topic (fail the stream with `INVALID_ARGUMENT` for an unserved kind), deliver
messages with id greater than `id_cursor` (0 = from the beginning), perform catch-up from history then
transition to live, and MUST NOT deliver an id at or below the subscription's **current floor**. "A remove
discards the floor, and an accepted lower-cursor re-add re-initializes it at the lower cursor."

**Req 3 — Delivery tagging.** Every `Messages` frame is stamped with the wave that produced it. Wave replay
frames (including messages that arrived mid-wave and were folded into the wave) carry that wave's
`mutate_id`; live tail frames carry `0`. "A frame belongs to exactly one wave or to live; the node MUST NOT
mix messages of distinct waves, or wave replay and live messages, in a single `Messages` frame." A `Mutate`
with non-empty `adds` MUST carry a nonzero `mutate_id`, and no `Mutate` may carry the `mutate_id` of a wave
still in flight — the node MUST fail the stream with `INVALID_ARGUMENT` in either case. Beyond that ids
SHOULD be unique for the stream's lifetime. **The tag is REQUIRED, not capability-gated.**

**Req 4 — Delivery order and the catch-up seam.** Four sub-rules:

- *Live order.* Live frames (`mutate_id = 0`) MUST deliver messages in ascending id order per message kind,
  across all live topics on the stream. This is what makes a single stream-wide live high-water mark a valid
  resume cursor for every live topic.
- *Wave order.* Within one wave, replay MUST be delivered in ascending id order per message kind across
  *all* of the wave's topics — "one merged cursor-ordered pass, not per-topic bursts." Distinct waves are
  independent and MAY interleave arbitrarily on the wire; the tag resolves the interleaving.
- *The seam.* A wave replays each topic up to a crossover — no lower than its `id_cursor` — that chases the
  live edge. Per topic, the node MUST NOT deliver a live frame for a wave's topic before that wave's
  `CatchupComplete`. Live frames for *other* subscriptions keep flowing throughout.
- *Exactly once across the seam.* While the stream is open and the topic's registration is unchanged, every
  message for a wave's topic above its `id_cursor` MUST be delivered exactly once — in the wave (tagged) or
  live after `CatchupComplete` (tagged 0), "never both, never neither." A message arriving mid-wave MUST be
  folded into the wave whenever holding it for live delivery would place it below an already-delivered live
  id; the node MAY hold it for live delivery only while no higher live id of its kind has been delivered.

**Req 5 — Mutate deltas, removes-before-adds, floors.** The node MUST process `Mutate` deltas that arrive
after the initial request "without terminating or reopening the stream." Within one `Mutate`, **`removes` are
applied before `adds`**, so a topic in both is reset. Duplicate topics within `adds` are coalesced, the
**lowest `id_cursor` winning** (on d14n: the **first occurrence** wins — vectors have no "lowest"). Removed
topics MUST stop being delivered promptly; already-serialized frames MAY still arrive and the client
discards them. Removing a topic **clears its floor**, so a later add — even with a lower `id_cursor` —
replays that history. A re-add of a still-subscribed topic is a **no-op** unless its `id_cursor` is below the
current floor. A no-op re-add "joins no wave: it appears in no `TopicsLive`, and its `Mutate`'s ack asserts
nothing about delivery." A lower-cursor re-add restarts catch-up "**as part of the new Mutate's wave**". A
topic removed mid-wave leaves its wave; the wave completes over its remaining topics, and a wave whose topics
have all left still yields its `CatchupComplete` (the node MAY emit it immediately). Whether a remove cancels
an in-flight `history_only` replay is **unspecified**.

**Req 6 — Liveness (ping/pong).** When no frame has been sent down the response channel for a bounded idle
interval (server-controlled, RECOMMENDED **≤ 30 s**), the node MUST send a `Ping` with a fresh nonce. "The
idle timer MUST reset whenever any frame is delivered, so the heartbeat adds **no per-message overhead** and
imposes **no per-topic broadcast**." The node MUST close the stream if the matching `Pong` does not arrive
within a bounded deadline (RECOMMENDED ≤ the ping interval). "other client frames do **not** satisfy the
deadline, so a client whose receive path has died ... is reaped even while it keeps sending." The node MUST
also answer any client `Ping` with a `Pong` echoing its nonce.

**Req 7 — Per-subscription authorization.** Mid-stream adds are authorized identically to opening-request
ones. Authorization MUST be evaluated **per subscription, independent of the connection**: one connection
MAY carry subscriptions of multiple identities/installations. Each topic is itself the resource identifier;
no separate identity field is needed. (Note: "v3's read path is topic-keyed and enforces no per-identity read
authorization; this requirement binds any node that does.")

**Req 8 — Resource bounds.** The node SHOULD bound: max subscriptions per stream; max adds per `Mutate`;
**and, where cursors are per-originator vectors, a max total count of cursor entries across those adds**
(explicitly called out for d14n); max mutation rate; max client-`Ping` rate. "Requests exceeding these limits
SHOULD be rejected with a gRPC error rather than silently truncated." "These limits are abuse guards, not
flow control: they SHOULD be generous enough that a well-behaved client never encounters them, which is why
the stream-fatal rejection is acceptable even on a multiplexed stream." A future revision MAY add a
non-fatal per-request rejection frame gated by `Started.capabilities`.

**Req 9 — Live-boundary signals.** When subscriptions finish catch-up the node MUST emit `TopicsLive`
listing their topics, **after** the last history frame for those topics (including mid-wave arrivals folded
into the wave). Each `Mutate` with at least one **effective** add starts a **wave**; once all of a wave's
subscriptions have crossed to live (or left the wave) the node MUST emit `CatchupComplete` echoing the
`mutate_id`, **after** the wave's last `TopicsLive`. **Every `Mutate` is acknowledged by exactly one
`CatchupComplete`**: one that starts no wave is acked immediately (a removes-only Mutate may legitimately
carry `0`). "Immediate acks are emitted in the order their `Mutate`s were received, so `0`-tagged acks
correlate positionally even when pipelined." Waves from overlapping Mutates MAY complete in any order.
`TopicsLive` is **informational only**: "delivery correctness (no duplicates, no gaps — rule 2) never depends
on it, a client MUST NOT rely on it for duplicate suppression, and re-adding a subscription re-runs catch-up
and re-emits it, so receivers treat it idempotently."

**Req 10 — Version pinning.** The node MUST respond on the same `version` arm the client uses. A
`SubscribeRequest` whose `version` arm the node does not recognize MUST fail the stream with
`INVALID_ARGUMENT` rather than being silently ignored. The initial `Started` is the sole exception.

**Req 11 — Bounded catch-up (`history_only`) and graceful shutdown.** A `Mutate` with `history_only = true`
catches its adds up exactly as rule 2 — history, `TopicsLive` markers (which then mean "you have everything
as of the wave's start"), and the wave's `CatchupComplete` — but the node MUST NOT register those topics for
live delivery (removes in the same Mutate apply normally). "The two registration styles never overlap on one
topic: a `history_only` add naming a topic already subscribed on the stream, and any add naming a topic with
an in-flight `history_only` catch-up, MUST fail the stream with `INVALID_ARGUMENT`." On client **half-close**
the node MUST stop sending `Ping`s, MUST finish all in-flight catch-up waves (live delivery continues while
they drain), and MUST then close with `OK`; if no waves are in flight it closes immediately after acking
every Mutate it read. A client that must stop immediately cancels the RPC instead — "no dedicated stop frame
exists because HTTP/2 cancellation already propagates in one round trip."

---

## 6. Client requirements (all 4)

**Req 1.** A client MUST answer a server `Ping` with a `Pong` echoing its nonce, promptly.

**Req 2 — Watchdog.** If no frame of any kind arrives within **N × the heartbeat interval** the client SHOULD
treat the stream as dead, close it, and reconnect. **N of 2–3 is RECOMMENDED.** Derive the threshold from
`Started.keepalive_interval_ms`, else assume 30 s.

**Req 3 — Durable resume state.** On reconnect a client MUST resume every subscription from
**durably-persisted** state. Because of requirements 3 and 4 this state is small: **one live high-water mark
per stream** (the highest live-delivered id per kind on v3; **a per-originator vector on d14n**) plus **one
transient progress mark per in-flight wave**. Re-add each topic with `id_cursor` = the live high-water mark
for topics that were live, `max(add cursor, wave progress)` for topics of an interrupted wave. "this state
MUST be persisted as messages are durably processed — not only on a graceful close." For a newly joined
group the initial cursor SHOULD be seeded from the welcome's encrypted `WelcomeMetadata.message_cursor`.

**Req 4.** A client SHOULD prefer `Mutate` deltas over opening additional streams. For scheduled background
windows it SHOULD use the bounded catch-up flow (`history_only` + half-close + drain to `OK`). "While
draining a bounded catch-up it has half-closed, a client MUST NOT apply its liveness watchdog to that
stream."

**Process suspension (mobile / browser).** Clients in suspending environments SHOULD: treat the stream as a
foreground/online-presence mechanism (delivery while suspended is out of scope; use push); on resume
reconnect-and-resume from persisted cursors immediately rather than waiting for the watchdog; on resume
**actively probe** with a client `Ping` and treat a missing `Pong` or a failed write as death; debounce rapid
background↔foreground transitions and measure staleness against wall-clock.

---

## 7. Core concepts in detail

### 7.1 mutate_id and delivery tagging

`mutate_id` is a `uint64` supplied by the **client** on each `Mutate`. It is the only correlation key between
the client's mutation and the server's frames.

| Value | Meaning |
| --- | --- |
| `mutate_id` on a `Messages`/`Envelopes` frame | the catch-up wave that produced this frame |
| `0` on a `Messages`/`Envelopes` frame | **live tail** |
| `mutate_id` on `CatchupComplete` | echoes the `Mutate` it acks |
| `0` on `CatchupComplete` | only legal when a **waveless** `Mutate` carried `0` |

Rules (req 3):

- `Mutate` with non-empty `adds` and `mutate_id == 0` → **fail the stream with `INVALID_ARGUMENT`**.
- `Mutate` whose `mutate_id` matches a wave **still in flight** → **fail the stream with
  `INVALID_ARGUMENT`**. Reuse after that wave's `CatchupComplete` is legal.
- A `Mutate` with no adds may legitimately carry `0`.
- Ids SHOULD otherwise be unique for the stream's lifetime.

Rationale (spec *Rationale*): "an earlier draft withheld it, and the client had to reconstruct the
replay/live distinction with per-topic cursor floors, advancing per-subscription positions, and seen-sets —
a residual complexity class ... that existed only because the wire hid information the server already had."
Tagging collapses client resume state to one high-water mark plus one progress mark per in-flight wave.

### 7.2 Waves and wave semantics

A **wave** is "the catch-up a `Mutate`'s effective adds start" (req 3, req 9). Properties:

- One wave per `Mutate` **with at least one effective add**. A no-op re-add is not effective.
- A `Mutate` with no effective adds starts **no** wave and is acked with an **immediate** `CatchupComplete`.
- Multiple waves may be in flight concurrently and MAY complete in any order.
- Within a wave, replay is one **merged, cursor-ordered pass across all of the wave's topics** — not
  per-topic bursts (req 4 wave order; test case 16).
- Frames of concurrent waves and live frames MAY interleave arbitrarily on the wire; the per-frame tag
  resolves it.
- A topic can leave a wave (removed, or reassigned to a newer wave by a lower-cursor re-add). A wave whose
  topics have all left still yields its `CatchupComplete`, which the node MAY emit immediately.
- The wave ends with, in order: its last replay frames → its `TopicsLive` → its `CatchupComplete`.

### 7.3 Cursor floor rules

The **floor** is per-registration state (req 2, req 5):

- It **starts at the registration's `id_cursor`** and **rises as the node delivers**.
- The node MUST NOT deliver an id at or below the current floor.
- **A remove discards the floor.**
- An accepted **lower-cursor re-add re-initializes it at the lower cursor**, explicitly requesting replay of
  ids above it.
- A plain re-add of a still-subscribed topic whose cursor is **not** below the floor is a **no-op**.

**On d14n this is different and simpler.** Because vector cursors are only partially ordered, "'below the
current floor' (requirement 5) has no direct analogue: on this binding **a plain re-add of an
actively-subscribed topic — live or still replaying — is always a no-op**, even when the offered cursor is
dominated by the current floor, and a client that wants a replay removes and re-adds the topic."
Likewise, duplicate adds within one `Mutate` "coalesce with the **first** occurrence winning".

"From the beginning" on d14n is an **empty cursor**, not `0`. The no-redelivery guarantee is evaluated **per
originator**: an envelope is delivered iff its `(originator_node_id, originator_sequence_id)` is beyond the
subscription's recorded position for that originator, "with originators absent from the cursor map treated as
sequence `0`."

### 7.4 Ordering, duplicate, and gap guarantees

| Guarantee | Statement (spec req 2 and 4) |
| --- | --- |
| **No duplicates** | The node MUST NOT deliver an id at or below the subscription's current floor — "no duplicates across catch-up/live". |
| **No gaps** | "every message for a wave's topic above that topic's `id_cursor` MUST be delivered exactly once ... never both, never neither." Precondition: the stream is open and the topic's registration is unchanged, and the wave registers live delivery. |
| **Live total order** | Live (`mutate_id=0`) frames deliver messages in ascending id order **per kind**, **across all live topics on the stream**. On d14n: ascending `originator_sequence_id` **per `originator_node_id`**, across all live topics. |
| **Wave order** | Within one wave, ascending id order per kind across all the wave's topics. On d14n: per originator. |
| **Cross-wave** | No ordering guarantee. Distinct waves and live may interleave arbitrarily; the tag disambiguates. |
| **Never-mix** | A single `Messages`/`Envelopes` frame is exactly one wave, or exactly live. Never a mix of two waves, never wave + live. |
| **TopicsLive is not a correctness signal** | "delivery correctness (no duplicates, no gaps — rule 2) never depends on it, a client MUST NOT rely on it for duplicate suppression". |

Consequence for clients (req 3): the live total order is exactly what makes a single stream-wide high-water
mark valid for every live topic — "when delivery reached id N, every live topic's messages at or below N had
already been delivered."

### 7.5 The seam (crossover)

The **seam** is the boundary at which a topic moves from a wave's replay lane to the live lane.

- A wave replays each topic **up to a crossover — no lower than the topic's `id_cursor` — that chases the
  live edge**. Everything at or below the crossover goes out tagged with the wave; everything above it goes
  out tagged `0`.
- **Per topic**, the node MUST NOT deliver a live frame for a wave's topic **before that wave's
  `CatchupComplete`**. Live frames for topics of *other* subscriptions keep flowing throughout.
- The crossover is pinned in practice by the exactly-once rule: "a message that arrives mid-wave MUST be
  folded into the wave (delivered tagged) whenever holding it for live delivery would place it below a live
  id the stream has already delivered; the node MAY hold a mid-wave arrival for live delivery after
  `CatchupComplete` only while no higher live id of its kind has been delivered on the stream."
- On `history_only` there is no live lane, so "the crossover is pinned at the wave's start and later
  messages arrive on no lane of this stream."
- On d14n "the crossover of the seam is chosen per topic per originator (a vector, like the cursor)."

### 7.6 TopicsLive

```protobuf
message TopicsLive { repeated bytes topics = 1; } // kind-prefixed topics done replaying
```

- Emitted when topics finish catch-up, **after their last history frame** — including mid-wave arrivals
  folded into the wave, "which were equally historical from the client's perspective".
- Guarantees: no further replay for a listed topic follows; its live (`mutate_id = 0`) frames begin after the
  wave's `CatchupComplete`.
- **Informational only.** Idempotent from the receiver's point of view; re-adding re-runs catch-up and
  re-emits it.
- Under `history_only` it means "you have everything as of the wave's start."
- Security note: a multiplexer with mutually-untrusting tenants SHOULD route `TopicsLive` only to the
  consumers that subscribed the topic, so co-subscription metadata does not cross tenants.

### 7.7 CatchupComplete

```protobuf
message CatchupComplete { uint64 mutate_id = 1; }
```

- **Exactly one per `Mutate`.**
- For a `Mutate` that started a wave: emitted at wave completion, **after the wave's last `TopicsLive`**.
- For a `Mutate` that started no wave (nothing added, or every add a no-op): emitted **immediately**, echoing
  its `mutate_id` (which may be `0` only if the Mutate itself carried `0` with no adds).
- It is the **catch-up seam boundary**: live (`mutate_id = 0`) frames for the wave's topics begin only after
  this frame.
- Immediate acks are emitted **in the order their `Mutate`s were received**, so `0`-tagged acks correlate
  positionally even when pipelined.
- An immediate ack **asserts nothing about delivery**.

### 7.8 Ping / Pong

```protobuf
message Ping { uint64 nonce = 1; }
message Pong { uint64 nonce = 1; }
```

- Version-independent (declared outside the `V1` arms).
- **Either peer MAY initiate.** The receiver **MUST** reply with a `Pong` echoing the nonce.
- Server side: send a `Ping` when the response channel has been idle for a server-controlled interval,
  RECOMMENDED ≤ 30 s. The idle timer resets on **any** delivered frame. Close the stream if the matching
  `Pong` does not arrive within a bounded deadline (RECOMMENDED ≤ the ping interval). **Other client frames
  do not satisfy the deadline.**
- Client side: answer promptly; run a watchdog at N×interval (N = 2–3); on resume from suspension probe with
  a client `Ping`.
- On half-close, the node **MUST stop sending `Ping`s** and the client **MUST NOT apply its watchdog** to the
  draining stream.
- Rationale: "HTTP/2 PING frames are handled inside the transport and never surface to the application ... a
  terminating L7 proxy answers them at the edge". Challenge/response proves liveness in **both** directions
  from one round trip, and lets the node **reap** a vanished peer.

### 7.9 history_only and half-close

The bounded catch-up ("sync") flow, with no extra protocol:

```text
open Subscribe()
  -> Mutate{ adds:[...], history_only: true, mutate_id: m }
  -> half-close the request stream
  <- Envelopes/Messages (tagged m)
  <- TopicsLive
  <- CatchupComplete(m)
  <- stream closes with OK
```

Rules: no live registration for those topics; removes in the same Mutate apply normally; a `history_only`
add naming an already-subscribed topic, or **any** add naming a topic with an in-flight `history_only`
catch-up, MUST fail the stream with `INVALID_ARGUMENT`. On half-close the node stops pinging, finishes
in-flight waves (live delivery continues while draining), then closes with `OK`.

---

## 8. Error handling and status codes

The spec names exactly three failure modes with codes, plus close semantics.

| Condition | Code / action |
| --- | --- |
| Topic of a kind this RPC does not serve, in `adds` | fail the stream with `INVALID_ARGUMENT` (req 2, d14n binding repeats this) |
| `Mutate` with non-empty `adds` and `mutate_id == 0` | fail the stream with `INVALID_ARGUMENT` (req 3, test 19) |
| `Mutate` carrying the `mutate_id` of a wave still in flight | fail the stream with `INVALID_ARGUMENT` (req 3, test 19) |
| `SubscribeRequest` with no recognized `version` arm | fail the stream with `INVALID_ARGUMENT` (req 10, test 14) |
| `history_only` add naming an already-subscribed topic | fail the stream with `INVALID_ARGUMENT` (req 11) |
| Any add naming a topic with an in-flight `history_only` catch-up | fail the stream with `INVALID_ARGUMENT` (req 11) |
| Per-stream resource limits exceeded | SHOULD be rejected "with a gRPC error rather than silently truncated" — **no specific code named** (req 8) |
| No `Pong` within the deadline | node **closes the stream** (req 6). No code named. |
| Node does not implement `Subscribe` | standard gRPC `UNIMPLEMENTED`; client falls back |
| Graceful half-close drain finished | node closes with **`OK`** (req 11) |
| Client needs to stop immediately | client **cancels the RPC** (no stop frame exists) |

The spec deliberately makes limit violations **stream-fatal**: "These limits are abuse guards, not flow
control: they SHOULD be generous enough that a well-behaved client never encounters them, which is why the
stream-fatal rejection is acceptable even on a multiplexed stream."

---

## 9. d14n binding differences

Summarised from the *Decentralized (d14n) binding* section. Everything not listed is identical.

| Aspect | v3 | d14n |
| --- | --- | --- |
| Service / RPC | `MlsApi.Subscribe` | `QueryApi.Subscribe` |
| Cursor | scalar `uint64 id_cursor` | `Cursor` = `map<uint32,uint64> node_id_to_sequence_id`, field named `last_seen` |
| "From the beginning" | `0` | **empty cursor** |
| No-redelivery evaluated | against a scalar | **per originator**; originators absent from the map are sequence `0` |
| Delivery payload | `GroupMessage` / `WelcomeMessage` lists | `OriginatorEnvelope` list; client demultiplexes by target topic |
| Delivery frame name | `Messages` | `Envelopes` (same `mutate_id` tag, same never-mix rule) |
| Ordering | per message kind (two independent global sequences) | **per `originator_node_id`** ("there is no global sequence") |
| Seam crossover | scalar per topic | **a vector**: per topic per originator |
| Client resume state | per-kind `u64` pair | **per-originator vector** (both the live high-water mark and each wave progress mark) |
| Floor / lower-cursor re-add | lower-cursor re-add restarts catch-up | **no analogue**: a plain re-add of an actively-subscribed topic is **always a no-op**; replay requires remove + re-add |
| Duplicate adds in one Mutate | lowest `id_cursor` wins | **first occurrence wins** |
| Topics | kind-prefixed binary (XIP-49 §3.3.2) | identical — native representation, no translation |
| Lifecycle frames | `Started` / `CatchupComplete` are new | echo the existing `SubscribeTopics` `STARTED` / `CATCHUP_COMPLETE` statuses |

---

## 10. Test cases in the spec

1. **Immediate Started** — first frame received MUST be `Started`, before any `Messages`.
2. **Idle ping** — a `Ping` within the advertised interval (≤30 s), and again each interval while idle.
3. **Ping resets on traffic** — publish at T; the next `Ping` MUST be no earlier than T + interval.
4. **Server reaps a silent client** — client stops answering; node MUST close within its Pong deadline.
5. **Client-initiated ping** — client sends `Ping{nonce=k}`; node MUST reply `Pong{nonce=k}`.
6. **Mutate-add catch-up, no reconnect** — messages with id > C, no duplicates, stream not torn down.
7. **Mutate-remove** — delivery stops; the removes-only Mutate is acked with an immediate `CatchupComplete`.
8. **Watchdog** — black-holed connection; client closes and reconnects, and receives what was published in
   the dead window.
9. **TopicsLive and per-wave CatchupComplete mark the live boundary.**
10. **Bounded catch-up** — `history_only` + half-close: history, `TopicsLive`, `CatchupComplete`, then close
    with `OK`. Messages published after the marker MUST NOT be delivered.
11. **Resume after suspension** — client detects death, reconnects from durable state; node MUST have reaped
    the original stream.
12. **Replay after remove** — remove then re-add at cursor 0 replays the whole history a second time, with an
    immediate `CatchupComplete` acking the removes-only Mutate in between.
13. **Duplicate adds coalesced** — history delivered once, one `TopicsLive` entry, one `CatchupComplete`.
14. **Unknown version rejected** — `INVALID_ARGUMENT`, not silently ignored.
15. **Replay frames tagged with their wave; live frames tagged 0** — with two overlapping Mutates, no frame
    mixes lanes or waves.
16. **Wave replay is merged in cursor order** — g1@0 and g2@0 in one Mutate deliver the union in ascending id
    order, not g1's history then g2's.
17. **Live total order per kind** — concatenating group messages of all `mutate_id = 0` frames in receive
    order yields ascending ids.
18. **The seam** — no `mutate_id = 0` frame containing g1 before `CatchupComplete(9)`; mid-wave g1 messages
    arrive exactly once; g0's live frames keep flowing throughout.
19. **Adds require a nonzero mutate_id; in-flight ids may not collide** — both `INVALID_ARGUMENT`.

---

## 11. What xmtpd actually implements

All code references below are to commit `822ddc95` of `/Users/nickmolnar/code/xmtp/xmtpd`.

### 11.1 Where it lives

| Item | Location |
| --- | --- |
| RPC registration | `pkg/server/server.go`, `registrationFunc` — `message_apiconnect.NewQueryApiHandler(replicationService, queryHandlerOpts...)`. Procedure `/xmtp.xmtpv4.message_api.QueryApi/Subscribe`. |
| Handler | `pkg/api/message/subscribe.go`, `func (s *Service) Subscribe(ctx, stream *connect.BidiStream[message_api.SubscribeRequest, message_api.SubscribeResponse]) error` |
| Session state | `pkg/api/message/subscribe.go`, `type subscribeSession` |
| Mutate handling | `pkg/api/message/subscribe.go`, `func (sess *subscribeSession) handleMutate` |
| Frame dispatch | `pkg/api/message/subscribe.go`, `func (sess *subscribeSession) handleRequest` |
| Catch-up fetcher | `pkg/api/message/subscribe.go`, `func (s *Service) runSubscribeCatchUp` |
| Wave completion | `pkg/api/message/subscribe.go`, `func (sess *subscribeSession) handleCatchUp` |
| Live routing | `pkg/api/message/subscribe.go`, `func (sess *subscribeSession) routeLive`, `advanceLive` |
| Mutable worker registration | `pkg/api/message/mutable_subscription.go`, `newMutableSubscription`, `addTopics`, `removeTopics`, `close` |
| Frame constructors | `pkg/api/message/subscribe.go`, `newSubscribeStarted`, `newSubscribeEnvelopes`, `newSubscribeTopicsLive`, `newSubscribeCatchupComplete`, `newSubscribePing`, `newSubscribePong` |
| Proto | `pkg/proto/xmtpv4/message_api/message_api.pb.go` (`SubscribeRequest`, `SubscribeResponse`, `Ping`, `Pong`) |

### 11.2 Wire format as generated in xmtpd

Field numbers taken from the struct tags in `pkg/proto/xmtpv4/message_api/message_api.pb.go`.

```text
SubscribeRequest
  oneof version:
    1  SubscribeRequest.V1 v1

SubscribeRequest.V1
  oneof request:
    1  Mutate mutate
    2  Ping   ping          // liveness challenge (e.g. probe the link after resuming)
    3  Pong   pong          // answer to a node Ping

SubscribeRequest.V1.Mutate
  1  repeated Subscription adds
  2  repeated bytes        removes
  3  bool                  history_only
  4  uint64                mutate_id

SubscribeRequest.V1.Mutate.Subscription
  1  bytes            topic
  2  envelopes.Cursor last_seen      <-- d14n vector cursor, NOT a scalar id_cursor

SubscribeResponse
  oneof version:
    1  SubscribeResponse.V1 v1

SubscribeResponse.V1
  oneof response:
    1  Envelopes        envelopes
    2  Started          started            // sent once, immediately on open, before any catch-up
    3  Ping             ping               // idle liveness challenge; receiver MUST answer with Pong
    4  Pong             pong               // answer to a client Ping
    5  TopicsLive       topics_live        // no more replay for these topics; live begins after CatchupComplete
    6  CatchupComplete  catchup_complete   // acks a Mutate; wave completion if it started one

SubscribeResponse.V1.Envelopes
  1  repeated envelopes.OriginatorEnvelope envelopes
  2  uint64                                mutate_id

SubscribeResponse.V1.Started
  1  uint32              keepalive_interval_ms
  2  repeated Capability capabilities        (only CAPABILITY_UNSPECIFIED = 0 defined)

SubscribeResponse.V1.TopicsLive
  1  repeated bytes topics

SubscribeResponse.V1.CatchupComplete
  1  uint64 mutate_id

Ping  { 1 uint64 nonce }     // top-level MESSAGE TYPE (see the note below)
Pong  { 1 uint64 nonce }
```

**On "top level", precisely.** `Ping` and `Pong` are declared as **top-level message types** — they
are not nested inside `SubscribeRequest.V1` or `SubscribeResponse.V1`, so the same two types are
reused by both directions and would be reused by a future `V2`. That is the sense in which they are
version-independent: the *type definition* is shared.

The **framing is not** version-independent. A Ping or a Pong can only travel inside a `V1` arm — as
`SubscribeRequest.V1.ping` (field 2) or `.pong` (field 3), and as `SubscribeResponse.V1.ping`
(field 3) or `.pong` (field 4). There is no way to send a bare `Ping` on this stream; every frame is
a `SubscribeRequest` or `SubscribeResponse` whose `version` oneof must be set, and `handleRequest`
rejects a request with no `V1` arm outright (`InvalidArgument`, `unrecognized SubscribeRequest
version`). So liveness frames are version-pinned exactly like every other frame; only their payload
type is shared across versions.

This matches the spec's d14n binding exactly: `Envelopes` instead of `Messages`, `last_seen` (a `Cursor`)
instead of `id_cursor`, `Ping`/`Pong` as shared top-level types.

### 11.3 Concurrency model (xmtpd's own design, not in the spec)

Documented in the `Subscribe` doc comment. **Single writer.** The `select` loop owns all mutable state (per-
topic vector cursors, catch-up gate, pending buffer, ping bookkeeping) and is the only goroutine deciding
what to send and in what order. Three pure producers feed it:

1. **Sender goroutine** — the sole caller of `stream.Send`, fed by the ordered channel `sess.outbound`
   (depth `subscribeSendQueueDepth = 8`). A client that stops reading can never park the writer.
2. **Frame reader goroutine** — `stream.Receive()` into `requestCh` (buffered 16).
3. **Catch-up fetcher goroutines** — one per wave, feeding `sess.catchUpCh` (depth
   `subscribeCatchUpQueueDepth = 16`).

`streamCtx, cancel := context.WithCancel(ctx)` with `defer cancel()`, because "connect-go does NOT cancel the
stream context when the handler returns".

### 11.4 Liveness implementation

`Subscribe` uses **two independent timers**:

- `pingTicker := time.NewTicker(keepAlive)` — the **send-idle** ping cadence. `sess.send` calls
  `sess.pingTicker.Reset(sess.keepAlive)` on every frame actually enqueued, so the cadence tracks real
  outbound traffic.
- `pongDeadline := time.NewTimer(keepAlive)`, stopped initially — the **reap deadline**, armed *only* when a
  Ping is sent (`pongDeadline.Reset(keepAlive)`) and disarmed *only* by a matching Pong.

The comment states why: "(A single shared ticker let ordinary delivery keep resetting the reap, defeating
silent-death detection.)"

Ping is sent only when `!sess.awaitingPong && !sess.halfClosed`. Nonce is a monotonically increasing
`sess.pingNonce++`. A `Pong` disarms only when `v1.GetPong().GetNonce() == sess.pingNonce` (i.e. it must
match the **latest** ping).

On `pongDeadline.C` the handler first calls `sess.drainPendingRequests(requestChannel)` — a non-blocking
drain — "so a Pong that landed in the buffer just as the deadline fired (Go select picks randomly among ready
cases) is still counted, instead of false-reaping a stream that did answer." Only then, if still
`awaitingPong && !halfClosed`, does it return `connect.NewError(connect.CodeDeadlineExceeded, errors.New("no
Pong within deadline"))`.

Client `Ping` is answered unconditionally: `case v1.GetPing() != nil: return sess.send(newSubscribePong(...))`.

`keepAlive` comes from `s.options.SendKeepAliveInterval` (a config option; see `pkg/config`). It is also what
is advertised in `Started`: `newSubscribeStarted(uint32(keepAlive.Milliseconds()))`.

### 11.5 Limits actually enforced

From the `const` block at the top of `pkg/api/message/subscribe.go`:

| Constant | Value | Enforced in | Error |
| --- | --- | --- | --- |
| `subscribeSendQueueDepth` | 8 | channel depth | — |
| `subscribeCatchUpQueueDepth` | 16 | channel depth | — |
| `maxActiveSubscribeTopics` | **1,000,000** | `handleMutate`, against the **projected post-Mutate** live set `\|(live \ removes) ∪ adds\|` | `ResourceExhausted`: `active topic limit %d exceeded` |
| `maxMutateAdds` | **100,000** | `handleMutate`, counted **pre-dedup** | `ResourceExhausted`: `adds per Mutate limit %d exceeded; split adds across multiple Mutates` |
| `maxMutateCursorEntries` | **1,000,000** | `handleMutate`, sum of `len(a.GetLastSeen().GetNodeIdToSequenceId())` across adds, **pre-dedup** | `ResourceExhausted`: `cursor entries per Mutate limit %d exceeded; split adds across multiple Mutates` |
| `maxInflightSubscribeWaves` | **256** | `handleMutate`, only when the Mutate would start a wave | `ResourceExhausted`: `in-flight catch-up limit %d exceeded` |
| `maxSubscribePendingBytes` | **64 MiB** (`64 << 20`) | `bufferLive` | `ResourceExhausted`: `pending buffer exceeded while catching up` |
| `maxSubscribeFrameBytes` | **2 MiB** (`2 << 20`) | `sendEnvelopes` — a soft batching target | frame split, not an error |
| `constants.GRPCPayloadLimit` | **25 MiB** | `sendEnvelopes` hard skip; also `connect.WithReadMaxBytes` / `WithSendMaxBytes` in `pkg/server/server.go` | oversized envelope logged and **skipped** |
| `topicPageLimit` | **500** | `runSubscribeCatchUp` page size (`pkg/api/message/subscribe_topics.go`) | — |
| `maxTopicLength` | **not enforced** | — `handleMutate` calls only `topic.ParseTopic` (2-byte minimum, known kind); the legacy 128-byte limit is not applied on this path | — |
| Per-RPC admission / open rate | **not enforced** | — `QueryApi.Subscribe` is absent from the rate-limit interceptor's procedure switch | — |

Note `maxMutateCursorEntries` is exactly the d14n-specific bound XIP-83 req 8 calls for. The comments justify
it: "each add's cursor is a per-originator vector, so without this cap one Mutate ... could name millions of
(topic, originator) pairs — every pair rides the wave's floor arrays, resent with EVERY page query".

`maxTopicLength` is **not** in this list, and that is not an omission. XIP-83 `Subscribe` enforces
**no maximum topic size**. `handleMutate` validates each add and remove with `topic.ParseTopic`
(`pkg/api/message/subscribe.go`), which checks only a **two-byte minimum** and a known topic kind
(`pkg/topic/topic.go`, `ParseTopic`). The 128-byte `maxTopicLength` that `validateQuery` and
`validateTopicFilter` apply on the legacy paths is never reached here. The only bound on a
subscribed topic's size is the 25 MiB request-frame cap.

**There is no per-RPC admission on this stream either.** An earlier version of this section implied
that the `QueryApi` rate-limit interceptor gives `Subscribe` per-RPC admission. It does not.
`pkg/interceptors/server/rate_limit.go`, `QueryApiMethodFromProcedure`, is a closed switch over
four procedures — `QueryEnvelopes`, `SubscribeTopics`, `GetInboxIds`, `GetNewestEnvelope` — and
`QueryApi.Subscribe` is **not among them**; an unrecognized procedure returns `("", false)` and both
`WrapUnary` and `WrapStreamingHandler` call `next(...)` with no limiting. `WrapStreamingHandler`
narrows it again with `if method != MethodSubscribeTopics { return next(ctx, conn) }`, so the opens
limiter is charged for `SubscribeTopics` and nothing else.

Net: XIP-83 `Subscribe` has **no open limit, no mutation limit, no ping limit, and no lifetime
limit** — none from the interceptor chain and none inside the handler — even with rate limiting fully
enabled. The older, less capable `SubscribeTopics` is the endpoint that pays admission. This is the
largest rate-limiting gap on this path and it is what makes req 8's two missing SHOULDs
(§13.1 item 1) more serious than they first appear: there is no outer limiter behind them.

### 11.6 Errors the handler returns

**IMPORTANT — these messages mostly do not reach the client.** The `LoggingInterceptor`
(`pkg/interceptors/server/logging.go`, `sanitizeError`) wraps every RPC, including this one
(`pkg/api/server.go`, `NewAPIServer` appends it to `serverInterceptors` and passes them to
`cfg.RegistrationFunc`). It logs the real error and returns a sanitized one:

- `InvalidArgument`, `Unimplemented`, `NotFound` — **message preserved verbatim**.
- `Internal` — message replaced with `internal server error`.
- **every other connect code** (`ResourceExhausted`, `Unavailable`, `Aborted`, `DeadlineExceeded`, …)
  — **code preserved, message replaced with `request has failed`**.
- `context.Canceled` → `Canceled` / `request was canceled`; `context.DeadlineExceeded` →
  `DeadlineExceeded` / `request timed out`.

So on this stream, **only the `InvalidArgument` rows below are wire-visible as written**. Every
`ResourceExhausted`, `Unavailable`, `Aborted`, and `DeadlineExceeded` row keeps its *code* and loses
its *message*. A client cannot distinguish `subscription closed: consumer too slow` from
`send stalled; client not reading` from `service is shutting down` by text — only `Aborted` versus
`Unavailable` survives, and the last two share `Unavailable`.

The **Wire message** column below states exactly what the client receives.

| Condition | Code returned by `Subscribe` | Message the handler builds | Wire message |
| --- | --- | --- | --- |
| `SubscribeRequest` with no `V1` arm | `InvalidArgument` | `unrecognized SubscribeRequest version` | **verbatim** |
| `Mutate` with adds and `mutate_id == 0` | `InvalidArgument` | `a Mutate with adds requires a nonzero mutate_id` | **verbatim** |
| `mutate_id` collides with an in-flight wave | `InvalidArgument` | `mutate_id %d is already in flight on this stream` | **verbatim** |
| Unparseable topic in `removes` | `InvalidArgument` | `remove: %w` (wraps `topic.ParseTopic` error, e.g. `unknown topic kind %d`, `topic must be at least 2 bytes long`) | **verbatim** |
| Unparseable topic in `adds` | `InvalidArgument` | `add: %w` | **verbatim** |
| `history_only` add naming an already-subscribed topic | `InvalidArgument` | `history_only add targets a topic already subscribed on this stream` | **verbatim** |
| Any add naming a topic with an in-flight `history_only` catch-up | `InvalidArgument` | `add targets a topic with an in-flight history_only catch-up` | **verbatim** |
| Cursor entry out of signed-column range (`nodeID > MaxInt32` or `seqID > MaxInt64`) | `InvalidArgument` | `cursor entry out of range (originator %d, sequence %d)` | **verbatim** |
| adds-per-Mutate cap | `ResourceExhausted` | `adds per Mutate limit %d exceeded; split adds across multiple Mutates` | `request has failed` |
| cursor-entries-per-Mutate cap | `ResourceExhausted` | `cursor entries per Mutate limit %d exceeded; split adds across multiple Mutates` | `request has failed` |
| active-topic cap | `ResourceExhausted` | `active topic limit %d exceeded` | `request has failed` |
| in-flight-wave cap | `ResourceExhausted` | `in-flight catch-up limit %d exceeded` | `request has failed` |
| pending buffer overflow while gated | `ResourceExhausted` | `pending buffer exceeded while catching up` | `request has failed` |
| Worker reaped the listener (consumer too slow / ctx done) | `Aborted` | `subscription closed: consumer too slow` | `request has failed` |
| Catch-up fetch failed | `Unavailable` | `catch-up failed: %w` | `request has failed` |
| **Ceiling query failed after retries** | **`Unavailable`** — not `Internal`; see below | `catch-up failed: could not select originator ceilings: %w` | `request has failed` |
| **Wave scan page query failed after retries** | **`Unavailable`** — not `Internal`; see below | `catch-up failed: could not select envelopes: %w` | `request has failed` |
| `stream.Receive` failed (non-EOF) | `Unavailable` | `stream recv failed: %w` | `request has failed` |
| `sess.send` blocked past `keepAlive` (client not reading) | `Unavailable` | `send stalled; client not reading` | `request has failed` |
| Service shutting down | `Unavailable` | `service is shutting down` | `request has failed` |
| No `Pong` within the deadline | `DeadlineExceeded` | `no Pong within deadline` | `request has failed` |
| Graceful flush timed out before drain | `DeadlineExceeded` | `flush timed out waiting for sender to drain` | `request has failed` |
| Graceful flush interrupted by ctx | `Canceled` | `flush interrupted before drain completed: %w` | `request was canceled` |

**Why the two catch-up query rows are `Unavailable`.** `fetchWaveCeilingsWithRetry` and
`fetchWaveScanPageWithRetry` (`pkg/api/message/subscribe.go`) each construct a
`connect.CodeInternal` error once their backoff is exhausted. But they run on the **fetcher
goroutine**, not the writer, and never return to the client directly. `runSubscribeCatchUp` puts the
error into `catchUpBatch.err`, and `handleCatchUp` discards the inner code:

```go
if b.err != nil {
    // A fetch error: fail so the client reconnects from its cursors, rather than emit a
    // misleading CatchupComplete over a history gap.
    return false, connect.NewError(
        connect.CodeUnavailable,
        fmt.Errorf("catch-up failed: %w", b.err),
    )
}
```

Both surface from `Subscribe` as `Unavailable`, then lose their message to `request has failed`. The
code is the right one — `Unavailable` tells the client to reconnect from its durable cursors — but a
client cannot tell a ceiling failure from a scan failure from any other catch-up failure.

### 11.6a A present `V1` with an empty request oneof is silently ignored

`handleRequest` (`pkg/api/message/subscribe.go`) rejects a request with **no `V1` arm**, but its
inner dispatch has a permissive default:

```go
switch {
case v1.GetPing() != nil:   return sess.send(newSubscribePong(v1.GetPing().GetNonce()))
case v1.GetPong() != nil:   if v1.GetPong().GetNonce() == sess.pingNonce { sess.awaitingPong = false }
                            return nil
case v1.GetMutate() != nil: return sess.handleMutate(v1.GetMutate())
default:                    return nil
}
```

A `SubscribeRequest` that sets `v1` but none of `mutate` / `ping` / `pong` falls to `default` and is
a **no-op**: no response, no error, no ack. The distinction is worth stating plainly because the two
cases look alike from the client side but behave oppositely:

| Client sends | Server does |
| --- | --- |
| `SubscribeRequest{}` — no version arm | `InvalidArgument`, `unrecognized SubscribeRequest version` (stream-fatal) |
| `SubscribeRequest{v1: V1{}}` — version arm set, request oneof empty | **nothing at all**; the frame is dropped |

A client that miscodes a Mutate this way waits forever for a `CatchupComplete` that will never
arrive, with no error to tell it why. **[FIX]** — reject an empty V1 oneof rather than dropping it.

### 11.7 Mutate processing order in xmtpd

`handleMutate` is explicitly two-phase — "it FULLY validates the frame ... before touching any session
state, so a malformed or over-cap Mutate cannot leave a half-applied subscription."

**Validate phase** (no mutation), in order:

1. adds non-empty ⇒ `mutate_id != 0`.
2. `mutate_id != 0` ⇒ not colliding with any in-flight wave (checked for **any** Mutate, including
   removes-only — "even a removes-only reuse would emit an immediate CatchupComplete ambiguous with the
   in-flight wave's").
3. `len(adds) <= maxMutateAdds` (pre-dedup).
4. total cursor entries `<= maxMutateCursorEntries` (pre-dedup).
5. Parse every `removes` topic.
6. Dedup adds — **first cursor wins** (`if _, dup := byKey[cursorKey]; dup { continue }`), parse each topic,
   and apply the re-add rules:
   - not being removed in this Mutate **and** already in `sess.topics`:
     - `history_only` ⇒ `InvalidArgument`,
     - otherwise **no-op** ("Plain re-add of an already-live topic is a no-op (idempotent): do not re-gate,
       reset the cursor, or start a redundant wave. Replay requires remove+re-add.").
   - `sess.hasInflightHistoryOnly(cursorKey)` ⇒ `InvalidArgument`.
   - Validate each cursor entry against the signed DB column ranges.
7. Projected active-topic cap (live waves only, `!historyOnly`).
8. In-flight wave cap, only if `len(order) > 0`.

**Apply phase**:

1. `for _, parsed := range removes { sess.removeTopic(parsed) }` — **removes first**.
2. If `len(order) == 0`: send an **immediate** `CatchupComplete(m.GetMutateId())` and return.
3. Otherwise build the wave, and for each add either seed `wave.cursors` (history_only) or call
   `sess.gateTopic(...)` — **gate-before-fetch**: the topic is registered with the worker and its live cursor
   seeded *before* the fetcher starts, "so no message published during catch-up is missed."
4. Register `sess.waves[sess.nextWave] = wave` and `go sess.svc.runSubscribeCatchUp(...)`.

### 11.8 Catch-up (wave) implementation

`runSubscribeCatchUp` runs **off the writer goroutine**. Steps:

1. `s.originatorList.GetOriginatorNodeIDs(ctx)` — a cached list (`db.NewCachedOriginatorList` with
   `OriginatorCacheTTL`), a DB round trip on a miss. The comment says this MUST stay off the writer "else a
   slow DB would stall liveness and live delivery and could false-reap a healthy stream."
2. `s.fetchWaveCeilingsWithRetry(ctx, ceilingOriginators(knownOriginators, providedCursors))` —
   `SelectOriginatorCeilings`, the **newest sequence id per originator at wave start**. The ceiling set is
   the **union** of the cached originator list and every originator any provided cursor names. This pins the
   wave's replay boundary so "the scan terminates under sustained publishing".
3. Build **floor cursors**: each topic's provided cursor, `db.FillMissingOriginators(filled,
   knownOriginators)` — filled to the full originator set. Flattened once into
   `queries.SelectGatewayEnvelopesWaveScanParams` via `db.SetWaveScanCursors` / `db.SetWaveScanCeilings` and
   **reused every page**.
4. Loop: `s.fetchWaveScanPageWithRetry(ctx, params)` with `RowLimit: topicPageLimit` (500). Advance
   `params.ScanNodeID` / `params.ScanSequenceID` from the **last raw row** (a keyset `>` row-value
   comparison), emit `catchUpBatch{wave, envs}`. Stop when a page is short.
5. `emit(catchUpBatch{wave, done: true})`.

This is **one merged keyset scan in `(originator, sequence)` order across ALL of the wave's topics** — the
comment says so explicitly, "not per-topic bursts", citing XIP-83 server requirement 4.

**Known residual gap, documented in the code**: "an originator that NEITHER the cache NOR that topic's cursor
names (a brand-new originator this client has never seen on that topic)" is missed by the wave's floor set —
"cache-bounded, like live originator registration". For a *live* wave this is covered by the live gate,
because the listener is registered before the snapshot. For a `history_only` wave it is **not**: "a brand-new
originator that first publishes during the catch-up window is delivered on the client's next subscribe rather
than this bounded sync — an accepted eventual-consistency property of history_only".

### 11.9 The seam in xmtpd

`handleCatchUp`, on `b.done`:

1. Deliver this page's history (already sent for non-done pages), tagged `w.mutateID`. For a live wave, the
   page first passes through `sess.envsOwnedByWave(b.envs, b.wave)` (drop topics removed or reset under a
   newer wave), then `sess.advanceLive(...)` which dedups against and advances **each topic's live cursor**.
   For a `history_only` wave it uses the wave's own throwaway `advanceTopicCursors(w.cursors, ...)`.
2. **Fold** the gated `pending` buffers of the wave's still-owned topics, flip each to `topicLive`, and
   collect its `wire` bytes.
3. **Sort the folded envelopes** by `(OriginatorNodeID, OriginatorSequenceID)` with `sort.SliceStable` —
   "Each topic's buffer is in per-originator dispatch order, but the wave's replay must stay totally ordered
   per originator ACROSS its topics: merge before framing."
4. `sess.sendEnvelopes(sess.advanceLive(folded), w.mutateID)` — the fold is tagged with the **wave's**
   `mutate_id`, matching XIP-83's rule that mid-wave arrivals folded into the wave carry the wave's tag.
5. `sess.send(newSubscribeTopicsLive(wire))` if `len(wire) > 0`.
6. `sess.send(newSubscribeCatchupComplete(w.mutateID))`.
7. `delete(sess.waves, b.wave)`.

The gate itself is `routeLive`: an envelope whose topic is `topicGated` goes to `bufferLive` (held, counted
against `pendingBytes`); everything else goes through `advanceLive` and is sent tagged `0`. So **a live frame
for a wave's topic is never delivered before that wave's `CatchupComplete`**, while other topics' live frames
keep flowing — exactly requirement 4's seam.

One honest caveat is in the code comment: "The fold appends after every scan page, so end-to-end the wave
guarantees only ascending sequence ids per originator, not the scan's global (originator, sequence) tuple
order."

### 11.10 Half-close and graceful shutdown

On `requestChannel` close with `sess.recvErr == nil`:

- if `len(sess.waves) == 0` ⇒ `return sess.flush()` — drain the sender and close with the sender's status
  (`OK` if everything went out).
- otherwise `sess.halfClosed = true`, `requestChannel = nil` (dormant), continue.
- `handleCatchUp` returns `done=true` when `sess.halfClosed && len(sess.waves) == 0`, and the main loop then
  `return sess.flush()`.

Pings are suppressed while `halfClosed` (`if !sess.awaitingPong && !sess.halfClosed`), and the pong deadline
is not enforced while `halfClosed`. This matches req 11 exactly.

`flush()` is bounded by `sess.ctx` or `sess.keepAlive`, and if the drain did **not** finish it returns
`DeadlineExceeded` / `Canceled` "rather than a false OK (the bug the v3 binding's flush originally had)".

### 11.11 Live delivery path

Live envelopes reach the session through `pkg/api/message/mutable_subscription.go` +
`pkg/api/message/subscribe_worker.go`:

- `newMutableSubscription` creates a `listener` with an **empty, non-global** topic set —
  "an empty topic set delivers nothing (a fresh stream that has subscribed to nothing yet), in contrast to
  `newListener`, which treats an empty query as 'all envelopes'."
- `addTopics` / `removeTopics` mutate `worker.topicListeners` under `listener.topicsMu`.
- The `subscribeWorker` polls the DB (`pkg/db/subscription.go`, `SubscribeWorkerPollTime = 100ms`,
  `subscribeWorkerPollRows = 1000`) per originator, unmarshals, and dispatches with
  `dispatchToTopics(envs)` — **one envelope at a time**, keyed by `env.TargetTopic().String()`.
- Listener channels are `subscriptionBufferSize = 1024` deep. On a **full channel the worker closes the
  listener** (`closeListener`), which the `Subscribe` loop observes as the channel closing and turns into
  `Aborted: subscription closed: consumer too slow`.

So xmtpd's backpressure policy is **drop the whole subscription**, not slow down. That is a deliberate choice
consistent with XIP-83's expectation that the client reconnects from durable cursors.

---

## 12. Conformance matrix

| Spec item | xmtpd | Evidence |
| --- | --- | --- |
| **Req 1** `Started` first, before catch-up | ✅ | `Subscribe`: `sess.send(newSubscribeStarted(uint32(keepAlive.Milliseconds())))` is the first send, immediately after goroutine setup, before any request is read. |
| Req 1 advertise `keepalive_interval_ms` | ✅ | `newSubscribeStarted(uint32(keepAlive.Milliseconds()))`. |
| Req 1 list `capabilities` | ⚠️ empty | `newSubscribeStarted` sets only `KeepaliveIntervalMs`. Correct today — no capabilities are defined — but the field is never populated. |
| **Req 2** validate topic kind, `INVALID_ARGUMENT` | ✅ | `topic.ParseTopic` in `handleMutate` for both adds and removes; `ParseTopic` rejects `< 2` bytes and unknown kinds. |
| Req 2 deliver above cursor, no duplicates | ✅ | `advanceLive` skips `last >= seqID` and advances in place; the wave scan's floors come from the same cursor. |
| Req 2 remove discards the floor | ✅ | `removeTopicState` deletes the whole `topicState` including `cursor`. |
| **Req 3** every frame tagged | ✅ | `sendEnvelopes(envs, mutateID)` → `newSubscribeEnvelopes(frame, mutateID)`; live paths pass `0`, wave paths pass `w.mutateID`. |
| Req 3 never mix waves / lanes in a frame | ✅ | `sendEnvelopes` takes a single `mutateID`; `routeLive` and `handleCatchUp` never mix inputs into one call. |
| Req 3 adds ⇒ nonzero `mutate_id`, else `INVALID_ARGUMENT` | ✅ | `handleMutate` first check. |
| Req 3 in-flight `mutate_id` collision ⇒ `INVALID_ARGUMENT` | ✅ | `handleMutate` scans `sess.waves`. Applied to **every** Mutate, even removes-only — stricter than a literal reading, and deliberately so. |
| **Req 4** live total order per originator | ✅ (by construction) | `routeLive` sends in worker arrival order; the worker dispatches each originator's envelopes in ascending sequence. Comment states the assumption: "each originator's envelopes become visible in sequence order (one sequential writer per originator)". |
| Req 4 wave replay merged in cursor order across the wave's topics | ✅ | `runSubscribeCatchUp` is one merged keyset scan ordered by `(originator, sequence)` across all the wave's topics. |
| Req 4 seam: no live frame for a wave's topic before its `CatchupComplete` | ✅ | `topicGated` + `bufferLive`; `phase = topicLive` is set only inside `handleCatchUp`'s done branch, before the `CatchupComplete` send. |
| Req 4 other topics' live frames keep flowing | ✅ | `routeLive` gates per topic, not per stream. |
| Req 4 exactly once across the seam | ⚠️ **partial** | The mechanism is sound: the wave's ceiling pins the replay range; anything above it arrives via the gated live path and is folded into the wave; `advanceLive` dedups on one shared per-topic cursor for both lanes. **The exception is size.** `sendEnvelopes` (`pkg/api/message/subscribe.go`) skips any envelope whose **framed** size exceeds `constants.GRPCPayloadLimit` (25 MiB), logging a warning and advancing the cursor past it. That envelope is delivered **zero** times, on both the replay and the live lane, and the client is never told. So the guarantee is "at most once, and exactly once for every envelope that fits in a frame". See §13.1 item 5. |
| Req 4 mid-wave arrivals folded in are tagged with the wave | ✅ | `sess.sendEnvelopes(sess.advanceLive(folded), w.mutateID)`. |
| **Req 5** mutate without reconnect | ✅ | The whole design. |
| Req 5 removes before adds | ✅ | Apply phase: `for _, parsed := range removes { sess.removeTopic(parsed) }` then adds. |
| Req 5 duplicate adds coalesced, **first wins** (d14n rule) | ✅ | `if _, dup := byKey[cursorKey]; dup { continue }`. |
| Req 5 removed topics stop being delivered promptly | ✅ | `removeTopic` unregisters from the worker and drops state; `advanceLive` drops envelopes for unknown topics with a warning. |
| Req 5 plain re-add of an active topic is a no-op (d14n rule) | ✅ | `handleMutate`: "Plain re-add of an already-live topic is a no-op". No lower-cursor special case — correct for d14n. |
| Req 5 remove + re-add replays | ✅ | The remove clears the floor; the re-add gates a fresh wave from the provided cursor. |
| Req 5 topic removed mid-wave leaves its wave; wave still completes | ✅ | `envsOwnedByWave` drops its pages; the done branch skips it (`!ok \|\| ts.phase != topicGated \|\| ts.wave != b.wave`); `CatchupComplete` is still emitted. |
| Req 5 remove vs. in-flight `history_only` is unspecified | ✅ (documented) | "removeTopic does NOT cancel a history_only wave (it only clears live state)". xmtpd picks *does not cancel*, which is legal. |
| **Req 6** ping when send-idle | ✅ | `pingTicker`, reset by `sess.send` on every enqueued frame. |
| Req 6 idle timer resets on any frame | ✅ | `sess.send`: `sess.pingTicker.Reset(sess.keepAlive)`. |
| Req 6 reap on missing Pong; other frames don't satisfy | ✅ | Separate `pongDeadline`, disarmed **only** by a nonce-matching Pong. |
| Req 6 answer client `Ping` | ✅ | `handleRequest`: `case v1.GetPing() != nil: return sess.send(newSubscribePong(...))`. |
| **Req 7** per-subscription authorization | ⚠️ N/A | xmtpd enforces **no per-topic read authorization at all** on this path — consistent with the spec's own note that "v3's read path is topic-keyed and enforces no per-identity read authorization". Not a violation, but there is nothing to review. |
| **Req 8** max subscriptions per stream | ⚠️ **live topics only** | `maxActiveSubscribeTopics = 1,000,000`, checked in `handleMutate` against the projected post-Mutate **live** set. `history_only` topics never enter `sess.topics` (the cap's input) — `handleMutate` skips `gateTopic` for them and the wave holds its own cursors — so **they bypass this cap entirely**. History-only work is bounded only indirectly, by `maxMutateAdds` (100,000 adds per Mutate) and `maxInflightSubscribeWaves` (256 concurrent waves). A stream can therefore have 1,000,000 live topics *plus* whatever history-only topics 256 in-flight waves of 100,000 adds each are carrying. |
| Req 8 max adds per Mutate | ✅ | `maxMutateAdds = 100,000`. |
| Req 8 max cursor entries per Mutate (d14n-specific) | ✅ | `maxMutateCursorEntries = 1,000,000`. |
| Req 8 max mutation rate | ❌ | Not implemented in the handler. |
| Req 8 max client-Ping rate | ❌ | Not implemented. Each client `Ping` costs one `send`, bounded only by the send queue. |
| Req 8 reject rather than truncate | ✅ | All caps return `ResourceExhausted`, stream-fatal. |
| **Req 9** `TopicsLive` after last history frame | ✅ | Emitted in `handleCatchUp`'s done branch, after the folded send. |
| Req 9 `CatchupComplete` after the wave's last `TopicsLive` | ✅ | Same branch, immediately after. |
| Req 9 exactly one `CatchupComplete` per Mutate | ✅ | Waveless Mutate ⇒ immediate send; wave Mutate ⇒ one on done. |
| Req 9 immediate acks in receipt order | ✅ | Single writer, processed in `requestCh` order. |
| Req 9 waves may complete in any order | ✅ | Independent fetchers; `sess.waves` is a map keyed by wave index. |
| Req 9 `TopicsLive` informational only | ✅ | Dedup is entirely cursor-based. |
| **Req 10** version pinning; unknown arm ⇒ `INVALID_ARGUMENT` | ✅ | `handleRequest`: `if v1 == nil { return InvalidArgument("unrecognized SubscribeRequest version") }`. Responses are always wrapped by `wrapSubscribeV1`. |
| **Req 11** `history_only` catches up but does not register live | ⚠️ **partial** | The mechanism is right: `handleMutate` skips `gateTopic` when `historyOnly`, and the wave carries its own throwaway `cursors`. **But a `history_only` wave can miss an originator entirely.** Its floors come from the client's provided cursor plus the TTL-cached originator list (`runSubscribeCatchUp` → `db.FillMissingOriginators(filled, knownOriginators)`), so an originator that **neither** names is absent from the floor set and its envelopes are never scanned. A *live* wave is covered because the listener is gated before the snapshot; a `history_only` wave has no live gate, so nothing covers it. The code states the tradeoff: "a brand-new originator that first publishes during the catch-up window is delivered on the client's next subscribe rather than this bounded sync — an accepted eventual-consistency property of history_only". Read strictly against req 11 + req 2 ("everything as of the wave's start"), this is non-conforming. See §13.1 item 3. |
| Req 11 `history_only` add on an already-subscribed topic ⇒ `INVALID_ARGUMENT` | ✅ | Explicit check. |
| Req 11 any add on a topic with an in-flight `history_only` ⇒ `INVALID_ARGUMENT` | ✅ | `hasInflightHistoryOnly`. |
| Req 11 half-close: stop Pings | ✅ | `if !sess.awaitingPong && !sess.halfClosed`. |
| Req 11 half-close: finish in-flight waves, then close `OK` | ✅ | `halfClosed` + `handleCatchUp` returning `done` + `sess.flush()`. |
| Req 11 half-close with no waves: close immediately after acking | ✅ | `if len(sess.waves) == 0 { return sess.flush() }`. |
| **Test 1** immediate `Started` | ✅ | |
| **Test 13** duplicate adds coalesced | ✅ | one subscription, one `TopicsLive` entry, one `CatchupComplete`. |
| **Test 16** merged wave replay | ✅ | single keyset scan. |
| **Test 19** both `INVALID_ARGUMENT` cases | ✅ | |

---

## 13. Deviations, gaps, and additions in xmtpd

### 13.1 Gaps against the spec

1. **No mutation-rate limit and no client-`Ping`-rate limit** (req 8, both SHOULD). A client can send Mutates
   and Pings as fast as the transport allows; each costs a `send` into an 8-deep queue.
2. **`Started.capabilities` is never populated.** Harmless today (no capabilities exist), but the
   feature-detection hook is unwired.
3. **`history_only` waves have a real, documented delivery gap** for originators that neither the
   TTL-cached originator list nor the client's cursor names. Live waves are covered by the live gate; bounded
   sync is not. The code calls this "an accepted eventual-consistency property of history_only". This is a
   deviation from req 11 + req 2 read strictly (a bounded catch-up is supposed to give you "everything as of
   the wave's start").
4. **Wave replay order is per-originator ascending, not globally `(originator, sequence)` tuple-ordered
   end-to-end**, once the fold is appended. The spec's d14n binding only requires "ascending
   `originator_sequence_id`" per originator, so this is compliant — but the code comment flags the
   difference, and a client that assumed a global tuple order would be wrong.
5. **Oversized envelopes are silently skipped**, not delivered — a real gap in the exactly-once guarantee.
   `sendEnvelopes` skips any envelope whose framed size exceeds `constants.GRPCPayloadLimit` (25 MiB),
   logging a warning. The comment argues the alternative is worse: "a reconnecting client's wave would hit
   the same row again, wedging it permanently". The spec has no provision for this.
6. **No per-subscription authorization** (req 7). The read path is unauthenticated and topic-keyed.

### 13.2 xmtpd is *stricter* than the spec in two places

1. The in-flight `mutate_id` collision check runs for **every** Mutate, including removes-only ones. The
   reasoning is in the code: a removes-only reuse "would emit an immediate CatchupComplete ambiguous with the
   in-flight wave's."
2. Cursor entries are validated against the **signed** DB column ranges (`nodeID <= MaxInt32`,
   `seqID <= MaxInt64`) and rejected with `InvalidArgument`. The spec says nothing about this. The comment
   explains it is not pedantry: an out-of-range value "would be silently dropped by the catch-up query ...
   AND stored verbatim in the sparse live cursor, where it would mark every real envelope from that
   originator as already seen — permanently killing the topic on this stream."

### 13.3 Implementation mechanics not in the spec (but load-bearing)

| Mechanism | Purpose |
| --- | --- |
| **Wave ceilings** (`SelectOriginatorCeilings`, `MAX(originator_sequence_id)` per originator, pinned at wave start) | Makes the replay scan terminate under sustained publishing. Sound only because of the invariant that each originator's rows become visible in sequence order. |
| **Filled floors** (`db.FillMissingOriginators`) | Turns a sparse client cursor into a dense per-originator floor for the scan; the persisted live cursor stays sparse to bound memory at the 1M topic ceiling. |
| **Gate-before-fetch** | The topic is registered with the live worker *before* the fetcher starts, so nothing published during catch-up is lost. |
| **Single-writer + sender goroutine** | A non-reading client can never park the writer, which must stay free to run the ping/pong reap. |
| **Bounded `send` (`sendTimer`, reused)** | A wedged sender fails the stream with `Unavailable: send stalled; client not reading` after `keepAlive`. |
| **`drainPendingRequests` before reaping** | Prevents a false reap when a Pong lands in the buffer exactly as the deadline fires. |
| **`envsOwnedByWave`** | Keeps a stale wave from advancing a reset topic's live cursor and skipping history the newer wave owes. |
| **Frame splitting at 2 MiB** | Keeps frames an order of magnitude below the 25 MiB transport cap. |
| **`pendingBytes` budget (64 MiB)** | Bounds memory held for gated topics; overflow tears the stream down rather than OOM. |

---

## 14. What a new single-Postgres backend should keep or drop

The new backend has **one Postgres DB, no blockchain, no payer service, no originators**. That last point is
the big one for XIP-83.

### Keep — the control protocol is the valuable part

- The frame set: `Mutate` / `Ping` / `Pong` up; `Envelopes` / `Started` / `Ping` / `Pong` / `TopicsLive` /
  `CatchupComplete` down.
- `mutate_id` tagging with `0` = live. This is what collapses client resume state and it is required, not
  optional.
- Wave semantics: one wave per effective-add Mutate, merged cursor-ordered replay across the wave's topics,
  `TopicsLive` then `CatchupComplete`, exactly one ack per Mutate.
- The seam: gate a wave's topics, buffer live arrivals, fold them into the wave tagged with the wave's id,
  then flip to live after `CatchupComplete`.
- `history_only` + half-close as the bounded-sync flow. It needs no new RPC.
- Ping/pong with **two independent timers** (send-idle cadence vs. reap deadline). xmtpd's comment about why
  a single shared ticker is wrong is the single most valuable operational note in this file.
- Removes-before-adds, first-occurrence-wins dedup, remove-clears-the-floor, plain-re-add-is-a-no-op.
- Version pinning with `INVALID_ARGUMENT` on an unknown arm.
- The stream-fatal limit policy (`ResourceExhausted`), with generous caps.
- `drainPendingRequests` before reaping.
- Gate-before-fetch.

### Simplify dramatically — the cursor becomes a scalar

With **no originators**, the d14n binding's whole reason for existing collapses:

| d14n today | Single-Postgres tomorrow |
| --- | --- |
| `Cursor` = `map<uint32,uint64>` per subscription | a single `uint64` sequence id (the v3 shape) |
| "from the beginning" = empty map | `0` |
| Floors filled from a TTL-cached originator list | no list needed |
| Wave ceilings = a vector per originator | one `MAX(sequence_id)` scalar |
| `maxMutateCursorEntries` (1M) | **delete this limit entirely** |
| Ordering "per originator" | one global monotonic sequence ⇒ **a true total order** |
| `SelectOriginatorCeilings` + `SetWaveScanCursors` + `SetWaveScanCeilings` | one keyset scan `WHERE topic = ANY($1) AND sequence_id > $floor AND sequence_id <= $ceiling ORDER BY sequence_id LIMIT n` |
| Lower-cursor re-add has "no analogue" | the **v3 floor rules become usable again**: a lower-cursor re-add can legitimately restart catch-up |
| Client resume state = a vector | a single `u64` high-water mark per stream |
| The "brand-new originator not in the cache" gap | **disappears** |
| "each originator's rows become visible in sequence order (one sequential writer per originator)" assumption | becomes a single-writer / single-sequence invariant that a single Postgres can actually guarantee |

That last row matters most. xmtpd's dedup, its ceiling pin, and its live ordering all rest on an invariant it
cannot enforce (rows from an originator becoming visible in sequence order). With one Postgres and one
sequence, the backend can enforce it — for example by assigning the sequence id inside the same transaction
that inserts the row, and having readers only read up to a watermark below which no transaction is still
in flight. That is the single hardest correctness problem this protocol has, and consolidating to one DB is
what makes it tractable.

### Drop

- The per-originator vector cursor, `FillMissingOriginators`, the cached originator list, and every
  `originator_node_id` column in the scan.
- The `SubscribeTopics` server-streaming ancestor, unless browser clients still need it. If they do, keep it
  *only* as the browser fallback the spec describes, behind a client watchdog — do not build new features on
  it.
- `SubscribeEnvelopes`, `SubscribeAllEnvelopes`, `SubscribeOriginators` (originator-filtered subscriptions
  have no meaning without originators).
- Payer/originator signature verification in the subscribe path (there is none today, but the
  `OriginatorEnvelope` wrapper it delivers carries them).

### Decide explicitly

- **The oversized-envelope skip.** Today a >25 MiB framed envelope is dropped and the client is never told.
  A new backend should instead cap envelope size at *publish* time so this row can never exist.
- **Backpressure.** xmtpd drops the whole subscription (`Aborted: consumer too slow`) when a 1024-deep
  channel fills. That is defensible under XIP-83 (the client reconnects from durable cursors) but it is a
  policy choice worth making deliberately, not inheriting.
- **Mutation-rate and client-Ping-rate limits** (req 8) — currently missing, and cheap to add.
- **Per-subscription authorization** (req 7). Today there is none. If the new backend ever adds read
  authorization, req 7 binds it: authorize **per topic**, never per connection, or the herald use case
  breaks.
- **Push-based live delivery.** xmtpd polls (`SubscribeWorkerPollTime = 100 ms`, 1000 rows). One Postgres
  makes `LISTEN`/`NOTIFY` viable, which would cut tail latency and remove a per-originator poller per node.

---

## Review status

**Review thread id**: `01a0624f-3cef-79c3-bf49-6859069fe45e` (adversarial review, model `gpt-5.6-sol`,
read-only sandbox, cwd `/Users/nickmolnar/code/xmtp/xmtpd`). Verdict: ISSUES. Each finding below was
re-checked against the cited source before any text in this document was changed. Findings against
the companion document are recorded in `xmtpd.md`'s own Review status section.

| Finding | Applied or rejected | Note |
| --- | --- | --- |
| **blocker** — §11.6 presents handler messages as wire-visible; the logging interceptor rewrites most of them | **applied** | Verified `pkg/interceptors/server/logging.go`, `sanitizeError` (the rule is stated in its own doc comment) and `pkg/api/server.go`, `NewAPIServer`, which installs it on every registered service. Added a marked preamble to §11.6 and a **Wire message** column to its table. Messages such as `subscription closed: consumer too slow`, `no Pong within deadline`, `send stalled; client not reading`, and every `ResourceExhausted` limit message reach the client as `request has failed`; their codes are preserved. Only the `InvalidArgument` rows survive verbatim. |
| **major** — §11.5 implies `Subscribe` gets per-RPC admission through the `QueryApi` interceptor | **applied** | Verified `pkg/interceptors/server/rate_limit.go`: `QueryApiMethodFromProcedure` is a closed switch over `QueryEnvelopes`, `SubscribeTopics`, `GetInboxIds`, `GetNewestEnvelope` — `Subscribe` is absent, and an unrecognized procedure passes straight to `next(...)`. `WrapStreamingHandler` narrows further to `MethodSubscribeTopics`. Rewrote the closing paragraph of §11.5: XIP-83 `Subscribe` has no open, mutation, ping, or lifetime limit. Added a row to the §11.5 limits table. |
| **major** — §11.6 lists ceiling and wave-scan failures as `Internal` | **applied** | Verified `fetchWaveCeilingsWithRetry` and `fetchWaveScanPageWithRetry` build `CodeInternal` but run off the writer goroutine; `runSubscribeCatchUp` routes the error through `catchUpBatch.err` and `handleCatchUp` re-wraps it as `connect.CodeUnavailable` with the `catch-up failed: %w` prefix. Corrected both rows to `Unavailable` and added the quoted code with an explanation. |
| **major** — §12 marks exact-once and `history_only` fully conforming | **applied** | Verified the >25 MiB framed-envelope skip in `sendEnvelopes` (`pkg/api/message/subscribe.go`), and the `history_only` floor gap in `runSubscribeCatchUp` (floors come from the provided cursor plus the TTL-cached originator list; a live wave is covered by gate-before-fetch, a history-only wave is not). Changed both matrix rows from ✅ to ⚠️ partial and stated each exception inline. |
| **major** — §12 "max subscriptions per stream" counts live topics only | **applied** | Verified `handleMutate` checks the projected **live** set, and that `history_only` adds never enter `sess.topics` (no `gateTopic` call; the wave holds its own cursors). Changed the row to ⚠️ and stated the real bound: 1,000,000 live topics, with history-only work bounded indirectly by 100,000 adds per Mutate and 256 in-flight waves. |
| **major** — §11.2 uses `?` for every V1 oneof arm; "top-level, version-independent" is misleading | **applied** | Read the struct tags in `pkg/proto/xmtpv4/message_api/message_api.pb.go`. Filled in the real numbers: request V1 arms `mutate=1`, `ping=2`, `pong=3`; response V1 arms `envelopes=1`, `started=2`, `ping=3`, `pong=4`, `topics_live=5`, `catchup_complete=6`. Added a paragraph distinguishing the shared top-level *message types* from the framing, which is version-pinned inside V1 like every other frame. |
| **minor** — §11.5 limits table omits the missing topic-size check | **applied** | Verified `handleMutate` calls only `topic.ParseTopic` (`pkg/api/message/subscribe.go`), and that `ParseTopic` (`pkg/topic/topic.go`) enforces a 2-byte minimum and a known kind with **no** maximum. Added a paragraph to §11.5 and a `maxTopicLength` row marked "not enforced". |
| **minor** — §11.6 omits that a present V1 with an empty request oneof is silently ignored | **applied** | Verified `handleRequest`'s `switch` ends in `default: return nil`. Added §11.6a contrasting the two cases: no version arm is stream-fatal `InvalidArgument`; an empty V1 oneof is a silent no-op that leaves the client waiting on an ack that never comes. |

### Findings rejected

**None.** Every finding raised against this document was confirmed correct against the cited source.
Nothing was rejected.

### Residual risk

The corrections here were each verified against the specific function the review cited, and the
reviewer independently confirmed this document's mutate, seam, liveness, and half-close citations
(`pkg/api/message/subscribe.go` lines 30, 145, 227, 265, 597, 716, 910, 1038, 1260). The residual
risk sits in three places. First, the conformance matrix is a reading of the spec against the code,
and several rows turn on how strictly a SHOULD is read — the two rows downgraded to ⚠️ here are the
ones where a strict reading and a lenient one disagree, and a reader who disagrees with the strict
reading should treat them as conforming rather than assume a code defect. Second, the spec itself is
a **Draft** on an unmerged branch (`tyler/xip-83-mutable-subscription-streams`, XIPs PR 139); its
requirement numbering and normative text can change under this analysis without any code changing.
Third, `Subscribe` is the newest code in the repository — the commit this document describes
(`822ddc95`) is the one that introduced wave `mutate_id` tagging — so its limits, error codes, and
seam behavior are the most likely of anything documented here to move. Re-verify any specific code,
message, or limit against the current source before encoding it in a client or a conformance test.
