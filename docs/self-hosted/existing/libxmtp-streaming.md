<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# libxmtp Streaming and Subscriptions — Current-State Wiki

Status: research record of the **current** implementation, written as input to a future `004_streaming.md` spec.
Repo: `/Users/nickmolnar/code/xmtp/libxmtp` (branch `self-hosted`).
Method: read the source. Every claim cites `path` or `path:symbol`. Items that could not be verified are marked **[UNVERIFIED]**.

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Layer map](#2-layer-map)
3. [Transport layer](#3-transport-layer)
   - 3.1 [gRPC client and HTTP/2 tuning](#31-grpc-client-and-http2-tuning)
   - 3.2 [WASM / browser transport](#32-wasm--browser-transport)
   - 3.3 [Stream combinators in `xmtp_api_grpc`](#33-stream-combinators-in-xmtp_api_grpc)
   - 3.4 [The two streaming traits](#34-the-two-streaming-traits)
   - 3.5 [Legacy subscribe endpoints (v3 and d14n)](#35-legacy-subscribe-endpoints-v3-and-d14n)
   - 3.6 [d14n stream combinators (XIP-49 ordering)](#36-d14n-stream-combinators-xip-49-ordering)
4. [The XIP-83 bidi client (already implemented)](#4-the-xip-83-bidi-client-already-implemented)
   - 4.1 [Where it lives](#41-where-it-lives)
   - 4.2 [Wire protocol as implemented](#42-wire-protocol-as-implemented)
   - 4.3 [Connection actor state machine](#43-connection-actor-state-machine)
   - 4.4 [Transport ledger: leases, waves, delivery rules](#44-transport-ledger-leases-waves-delivery-rules)
   - 4.5 [Reconnect, suspend/resume](#45-reconnect-suspendresume)
   - 4.6 [Constants table](#46-constants-table)
   - 4.7 [Draft `backend.proto` vs implementation](#47-draft-backendproto-vs-implementation)
5. [Client layer (`xmtp_mls/src/subscriptions`)](#5-client-layer-xmtp_mlssrcsubscriptions)
   - 5.1 [Public API surface](#51-public-api-surface)
   - 5.2 [Two parallel architectures and the dispatch gate](#52-two-parallel-architectures-and-the-dispatch-gate)
   - 5.3 [Legacy stack internals](#53-legacy-stack-internals)
   - 5.4 [XIP-83 router internals](#54-xip-83-router-internals)
   - 5.5 [Message processing, sync-on-stream, recovery](#55-message-processing-sync-on-stream-recovery)
   - 5.6 [Cursors, dedup, ordering](#56-cursors-dedup-ordering)
   - 5.7 [Local-only streams (consent, preferences, deletions)](#57-local-only-streams-consent-preferences-deletions)
   - 5.8 [Watchdog](#58-watchdog)
   - 5.9 [`catch_up.rs` — bounded sync](#59-catch_uprs--bounded-sync)
   - 5.10 [Errors and retryability](#510-errors-and-retryability)
6. [Bindings and SDKs](#6-bindings-and-sdks)
7. [Test coverage](#7-test-coverage)
8. [v3-only vs v4/d14n-only complexity](#8-v3-only-vs-v4d14n-only-complexity)
9. [Streaming requirements the new backend and simplified client must meet](#9-streaming-requirements-the-new-backend-and-simplified-client-must-meet)
10. [Candidate simplifications](#10-candidate-simplifications)
11. [Open design questions for `backend.proto`](#11-open-design-questions-for-backendproto)
12. [Review status](#review-status)

---

## 1. Executive summary

Six findings dominate everything below.

**Finding 1 — The XIP-83 client already exists, and it is mature — on the V3 binding only.**
libxmtp already implements an XIP-83-style bidirectional subscription client: `Mutate`/`mutate_id`/`Started`/`CatchUpComplete`/`TopicsLive`/`Ping`/`Pong`, catch-up waves, `history_only`, refcounted topic leases, reconnect with backoff, and suspend/resume. It lives in `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` (6172 lines), `bidi.rs` (1138), and per-backend bindings in `queries/v3/connection.rs` and `queries/d14n/connection.rs`. The client-facing half is `crates/xmtp_mls/src/subscriptions/stream_router.rs` (1685) and `router_callbacks.rs` (865). Generated protos exist for **both** backend flavors: `crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs` (v3) and `xmtp.xmtpv4.message_api.rs` (d14n). The draft `docs/self-hosted/backend.proto` is very close to what the client already speaks.

**The maturity is V3-only.** Only V3 completes the stack. `trait TransportBinding` (`crates/xmtp_api_d14n/src/queries/bidi_transport.rs:342`) has exactly **one** implementor in the repo — `impl TransportBinding for V3Binding` (`crates/xmtp_api_d14n/src/queries/v3/connection.rs:97`, `type Cursor = u64`). `D14nBinding` (`queries/d14n/connection.rs:46`) implements only the lower-level `BidiBinding` (line 88) — the wire-vocabulary half — and never gains the `Cursor`/`build_mutate`/`advance`/`covers`/`meet` half `BidiTransport` needs. The client router is not generic over the binding either: `RouterTask.transport: BidiTransport<V3Binding>` (`crates/xmtp_mls/src/subscriptions/stream_router.rs:387-390`), monomorphized at `StreamRouter::new` (`stream_router.rs:187`). And the d14n API client refuses the surface on purpose: `subscribe_bidi` returns `Err(ApiClientError::OtherUnretryable("the v3 bidi subscription is not available on this client"))` under a reserved `host()` of `"unsupported://d14n"` (`crates/xmtp_api_d14n/src/queries/d14n/streams.rs:120-156`). So **d14n app streams fall back to the legacy path by design**; d14n's XIP-83 code is direct wire handling plus its own tests, not a live app path. §7 records the matching test asymmetry.

**Finding 2 — Two complete streaming stacks run in parallel, selected at runtime.**
The legacy stack (per-stream server-streaming RPCs, teardown-and-resubscribe on group add, client-side watchdog) and the XIP-83 stack (one shared bidi wire, in-place mutation) both exist. Selection is by env var `XMTP_BIDI_STREAMS_ENABLED` read once per process, with a per-destination fallback latch when a backend refuses the bidi surface (`crates/xmtp_mls/src/subscriptions/router_callbacks.rs`). This duplication is the single largest simplification opportunity.

**The gate does not cover every stream.** It is consulted only by the three native `*_with_callback_dispatch` functions (`router_callbacks.rs:670-694, 734, 802`, gate `bidi_streams_active` at `router_callbacks.rs:161-165`). The Rust **pull-stream** forms build legacy streams directly with no gate: `Client::stream_conversations` / `stream_conversations_owned` (`crates/xmtp_mls/src/subscriptions/mod.rs:450-484`), `Client::stream_all_messages` / `stream_all_messages_owned` (`mod.rs:524-546`), and `MlsGroup::stream` / `stream_owned` (`crates/xmtp_mls/src/groups/subscriptions.rs:101-125`) — all bounded on the legacy `XmtpMlsStreams` trait. WASM's `streamLocal` (`bindings/wasm/src/conversations.rs:595-606`) calls `stream_conversations_owned`, so it too is unconditionally legacy. These public pull streams must be rewritten before the legacy stack can be deleted (§10.1).

**Finding 3 — WASM cannot do bidi, and that is a hard transport constraint.**
Browsers use `tonic-web-wasm-client` over fetch (`crates/xmtp_api_grpc/src/grpc_client/wasm.rs`), which is server-streaming only. Both the bidi transport and the router are `#[cfg(not(target_arch = "wasm32"))]`. This is exactly the constraint that motivates `SubscribeOnce` in the draft proto, and it means the browser will keep a distinct code path no matter what.

**Finding 4 — Most app-visible streaming semantics are SDK-level, not protocol-level.**
Retry counts, delays, `onValue`/`onError`/`onFail`/`onRestart`/`onRetry`/`onEnd`, and the sync-before-stream convention live in `sdks/js/*/src/utils/streams.ts` and the binding callback traits. They are preserved by keeping the binding surface stable, independent of wire design. Notably the node and browser SDKs ship **different** retry defaults.

**Finding 5 — Consent, preference and deletion streams are purely local.**
They read a `tokio::sync::broadcast` channel of `LocalEvents` and never touch the network (`crates/xmtp_mls/src/subscriptions/mod.rs`). The new backend needs no proto surface for them. They are not lossless, though: a lagged receiver's events are logged and skipped (`optify!`, `mod.rs:176-207`) and deletion decoding can fail — see §5.7.

**Finding 6 — "Exactly once per lease" is a transport property, not an app guarantee.**
`bidi_transport.rs:24-30` states it as a conditional: exactly-once holds *per lease*, *for cursor-tracked messages*, and only while the backend honors three obligations (tagged frames, total cursor order per kind, no live frames on a wave-owned topic before its `CatchUpComplete`). It is scoped to the V3 ledger. It does not extend to frames with no extractable cursor ("fail open to every holder", `bidi_transport.rs:73-74`), to a *new* lease (which legitimately replays identities an earlier stream delivered — `stream_router.rs:667-675`), to d14n (no `TransportBinding`), to SDK-level retries, or to data outside retention. What the application sees is a *deduplicated* stream, deduplicated against local storage — not an exactly-once wire.

---

## 2. Layer map

| Layer | Crate / path | Role |
| --- | --- | --- |
| Wire (native) | `crates/xmtp_api_grpc/src/grpc_client/native.rs` | tonic channel, HTTP/2 + TCP keepalive, TLS |
| Wire (browser) | `crates/xmtp_api_grpc/src/grpc_client/wasm.rs` | grpc-web over fetch; **server-streaming only** |
| Stream combinators | `crates/xmtp_api_grpc/src/streams/` | multiplexing, non-blocking establishment, item conversion |
| Legacy subscribe endpoints | `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_*.rs`, `endpoints/d14n/subscribe_topics.rs` | one RPC per subscription |
| XIP-83 connection actor | `crates/xmtp_api_d14n/src/queries/bidi.rs` | sole writer of the request half, ping/pong, silence watchdog |
| XIP-83 transport ledger | `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | leases, waves, delivery admission, reconnect, suspend/resume |
| Backend bindings | `crates/xmtp_api_d14n/src/queries/{v3,d14n}/connection.rs` | per-backend wire types and frame construction |
| Streaming traits | `crates/xmtp_proto/src/api_client.rs` | `XmtpMlsStreams` (legacy) and `XmtpMlsBidiStreams` (XIP-83) |
| Legacy client streams | `crates/xmtp_mls/src/subscriptions/stream_{messages,conversations,all}.rs` | `Stream` state machines |
| XIP-83 client router | `crates/xmtp_mls/src/subscriptions/stream_router.rs` | one task per stream, leases + dedup + durable cursors |
| Dispatch + gate | `crates/xmtp_mls/src/subscriptions/router_callbacks.rs` | env gate, per-destination latch, callback bridge |
| Bindings | `bindings/mobile`, `bindings/node`, `bindings/wasm` | closers + callbacks |
| SDKs | `sdks/js/{node-sdk,browser-sdk}`, `sdks/android`, `sdks/ios` | retry wrappers, async iterables/Flow/AsyncStream |

---

## 3. Transport layer

### 3.1 gRPC client and HTTP/2 tuning

Native transport config is in `crates/xmtp_api_grpc/src/grpc_client/native.rs`. Keepalive is env-tunable, captured once into a `KEEPALIVE` static, and the file documents *why* the values were relaxed: tight deadlines "tore down otherwise-healthy long-lived connections whenever one straggled past the tight deadline — reconnect churn observed from server side" (`native.rs` module docs, ~lines 16-20).

| Setting | Env var | Default | Source |
| --- | --- | --- | --- |
| `http2_keep_alive_interval` | `XMTP_GRPC_KEEPALIVE_INTERVAL_SECS` | 45 s | `native.rs:45` `DEFAULT_INTERVAL` |
| `keep_alive_timeout` | `XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS` | 20 s | `native.rs:46` `DEFAULT_TIMEOUT` |
| `tcp_keepalive` (0 disables) | `XMTP_GRPC_TCP_KEEPALIVE_SECS` | 45 s | `native.rs:47` `DEFAULT_TCP_KEEPALIVE` |
| `keep_alive_while_idle` | `XMTP_GRPC_KEEPALIVE_WHILE_IDLE` | true | `native.rs:70-72` |
| `initial_connection_window_size` | — | `(1 << 31) - 1` | `native.rs:132` |
| `connect_timeout` | — | **10 s** | `native.rs:140` |
| `timeout` (per-request deadline) | — | **120 s** | `native.rs:148` |
| `rate_limit` | — | `limit`/60 s (default 5000) | `native.rs:160`, `native.rs:100-105` |
| max encode/decode message size | — | **25 MiB** (`GRPC_PAYLOAD_LIMIT`) | `crates/xmtp_configuration/src/common/api.rs:12`, applied `grpc_client/client.rs:296-313` |

Worst-case detection is therefore ~65 s (interval + timeout). The rationale comment (`native.rs:13-35`) is load-bearing history: previous defaults were 16 s/10 s (~26 s worst case), which "tore down otherwise-healthy long-lived connections whenever one PING ack straggled past the tight deadline — reconnect churn observed from server (herald-lite #70) and mobile clients alike... (2026-08 production incident)". 45 s was chosen "to stay inside common 60s middlebox idle timers". The same file warns that "a missed PING ack here kills the whole connection — **and with it every bidi stream**" (`native.rs:154-157`) — a direct constraint on the new design, since one wire now carries every subscription in the process.

The channel is built with `connect_lazy()` (`native.rs:105`). TLS endpoints are cached in `static TLS_ENDPOINTS: LazyLock<Mutex<HashMap<(String, u64), Endpoint>>>` (`native.rs:177-178`) because `ClientTlsConfig::with_enabled_roots()` re-parses the OS trust store on every call — ~40 ms each on macOS, serialized through Security.framework.

**Resolved — the 120 s endpoint timeout bounds response establishment only.** `.timeout(Duration::from_secs(120))` is set on the tonic `Endpoint` (`native.rs:148`). Tonic installs it as a `GrpcTimeout` layer in the channel stack (tonic 0.14.6 `transport/channel/service/connection.rs:73`, `.layer_fn(|s| GrpcTimeout::new(s, endpoint.timeout))`). `GrpcTimeout::ResponseFuture::poll` races the *inner call future* against the sleep and returns as soon as that future resolves (`transport/service/grpc_timeout.rs:73-93`); once the response is established, the body streams with **no** timeout wrapper. So the 120 s cap applies to getting a response, not to the lifetime of an established server-streaming or bidi body. Long-lived native subscriptions are not capped at 120 s.

Relevance to the new design: these values are the *only* liveness mechanism the legacy v3 path has, because v3 has no application-level keepalive. XIP-83's `Started.keepalive_interval_ms` plus `Ping`/`Pong` supersedes it at the application layer.

**Three separate liveness mechanisms exist today, at three layers:** (a) transport HTTP/2 PING (45 s/20 s, `native.rs:44-47`); (b) `StreamStatus::last_ping` in `queries/stream/status_aware.rs` — computed and currently discarded; (c) the XIP-83 actor's own ping/pong plus silence watchdog (`queries/bidi.rs:44-54`). The new design should collapse these to one.

### 3.2 WASM / browser transport

`crates/xmtp_api_grpc/src/grpc_client/wasm.rs` defines `GrpcWebService`, wrapping `tonic_web_wasm_client::Client` with `FetchOptions::default().referrer_policy(ReferrerPolicy::StrictOriginWhenCrossOrigin)`. It trims a trailing `/` because "envoy does *not* like trailing /" (`wasm.rs:GrpcWebService::new`).

Cargo target gating (`crates/xmtp_api_grpc/Cargo.toml:52-53`) selects `tonic-web-wasm-client` for `cfg(all(target_family = "wasm", target_os = "unknown"))`.

The consequence is stated explicitly in `crates/xmtp_proto/src/api_client.rs:172-177`:

> Native-only — gRPC-Web transports cannot speak full-duplex, so browsers stay on `XmtpMlsStreams` with a client-side watchdog.

So: **browser = unidirectional server streaming only**. This is the direct justification for `SubscribeOnce` in the draft proto.

### 3.3 Stream combinators in `xmtp_api_grpc`

`crates/xmtp_api_grpc/src/streams/` holds small combinators reused by both stacks.

| File | Lines | Purpose (per source) |
| --- | --- | --- |
| `default.rs` | 175 | `XmtpTonicStream<S,T>` — decode `Bytes` → prost `T`, attach `ApiEndpoint` to errors |
| `multiplexed.rs` | 164 | Priority-biased merge of two streams; S1 polled first, S1 ending ends the whole stream |
| `non_blocking_stream.rs` | 374 | **gRPC-Web fetch workaround** (see below) |
| `non_blocking_request.rs` | 95 | Platform-split `send()`: awaits fully on native, polls once on wasm |
| `try_from_item.rs` | 162 | Converts each item via `TryFrom`, preserving errors without terminating |
| `escapable.rs` | 94 | Terminates the stream on unrecoverable HTTP/2 errors (BrokenPipe) |

**`non_blocking_stream.rs` is a browser fetch-semantics workaround, not an executor concern.** Its module doc (`non_blocking_stream.rs:1-27`) explains: "a web 'fetch' request may complete successfully, but the fetch promise does not resolve until the first bytes of the body are received by the browser [tonic-web-wasm-client issue #22]... On native, gRPC streams do not block on the first body received, while web streams do. This is particularly obvious in tests, where: 1. stream is started 2. data is sent 3. inspect sent data — on web, we never get past step 1." It models RPC establishment *as part of the stream* (`StreamState::{NotStarted, Started, Terminated}`), so subscribe-then-publish-then-observe does not deadlock in a browser. On native the wrapper is a pass-through.

**`escapable.rs` is where a dead HTTP/2 connection becomes a stream end.** `maybe_extract_io_err` (`escapable.rs:39-49`) downcasts `tonic::Status` → `hyper::Error` → `h2::Error` → `std::io::Error`; on `ErrorKind::BrokenPipe` — "BrokenPipe results from the HTTP/2 KeepAlive interval being exceeded" (`escapable.rs:12-22`) — it yields the error once and then terminates. This is the only place transport-level death is classified, and it is native-only in practice.

**`multiplexed.rs` has an asymmetry worth knowing:** once S1 ends, S2 is polled once; if S2 is merely `Pending` its buffered items are lost and the stream terminates (`multiplexed.rs:38-45`, test `ignores_items_after_s2_pending`). Its single consumer is `stream_conversations.rs:289` — S1 = network welcome subscription, S2 = local events.

Corresponding asserted behaviors appear as requirements API-REQ-081 through API-REQ-084 (`docs/self-hosted/tests/existing-requirements.md:769-772`), e.g. API-REQ-082 "The stream polls both inputs without starving", API-REQ-083 "Establishment errors yield once the stream is polled".

**Error classification and the absence of stream retry.** `GrpcError::is_retryable()` is a hardcoded `true` (`crates/xmtp_api_grpc/src/error.rs:120-124`) — every gRPC error is nominally retryable at this layer. The one escape hatch is `GrpcError::is_unimplemented()` (`error.rs:109-118`), whose doc explains why: "the backend's own verdict that the RPC surface does not exist, as opposed to a transient failure reaching it. Callers deciding between redialing and falling back need that distinction, which `is_retryable` (a blanket `true` here) erases." That distinction is exactly what drives the bidi fallback latch (§5.2). Real classification happens in `crates/xmtp_proto/src/traits/error.rs:117-135`, where `DecodeError`, `Conversion`, `ProtoError`, `InvalidUri`, `WritesDisabled` and `OtherUnretryable` are `false`.

**No layer retries a stream.** `RetryQuery` implements `Query`/`QueryRaw` but **not** `QueryStream` (`crates/xmtp_proto/src/traits/combinators/retry.rs`); `ApiClientWrapper`'s `retry_strategy` is never referenced by its three subscribe methods (`crates/xmtp_api/src/mls.rs:230-279`); `ClientBuilder::retry` is stored and never read (`crates/xmtp_api_grpc/src/grpc_client/client.rs:296-313`). Stream recovery is entirely the caller's job, driven by cursors. This is a deliberate and correct split, and the new design should keep it.

### 3.4 The two streaming traits

Both live in `crates/xmtp_proto/src/api_client.rs`.

**Legacy — `XmtpMlsStreams` (`api_client.rs:151-170`), available on all targets:**

```rust
pub trait XmtpMlsStreams: MaybeSend + MaybeSync {
    type GroupMessageStream: Stream<Item = Result<GroupMessage, Self::Error>> + MaybeSend;
    type WelcomeMessageStream: Stream<Item = Result<WelcomeMessage, Self::Error>> + MaybeSend;
    type Error: RetryableError + 'static;

    async fn subscribe_group_messages(&self, group_ids: &[&GroupId]) -> Result<Self::GroupMessageStream, Self::Error>;
    async fn subscribe_group_messages_with_cursors(&self, groups_with_cursors: &TopicCursor) -> Result<Self::GroupMessageStream, Self::Error>;
    async fn subscribe_welcome_messages(&self, installations: &[&InstallationId]) -> Result<Self::WelcomeMessageStream, Self::Error>;
}
```

Note the shape: **group messages and welcomes are separate subscriptions**, and there is no way to change a subscription in place — the only "mutation" is to call `subscribe_group_messages_with_cursors` again with a new set.

**XIP-83 — `XmtpMlsBidiStreams` (`api_client.rs:178-201`), native-only** (inside `xmtp_common::if_native!`):

```rust
pub trait XmtpMlsBidiStreams: MaybeSend + MaybeSync {
    type SubscribeStream: Stream<Item = Result<crate::mls_v1::SubscribeResponse, Self::Error>> + MaybeSend;
    type Error: RetryableError + 'static;

    fn host(&self) -> &str;

    async fn subscribe_bidi(
        &self,
        requests: futures::stream::BoxStream<'static, crate::mls_v1::SubscribeRequest>,
    ) -> Result<Self::SubscribeStream, Self::Error>;
}
```

Its doc comment describes exactly the target design: "one long-lived stream carrying group and welcome messages, mutated in place (no reconnect on membership change) and kept alive with WebSocket-style ping/pong."

`host()` is load-bearing: it is the wire-sharing key for the process-shared transport, and each URL carries its own unsupported latch.

### 3.5 Legacy subscribe endpoints (v3 and d14n)

**v3, `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_group_messages.rs`:**
`SubscribeGroupMessages` carries `filters: Vec<SubscribeFilter>` where each filter is `{ group_id, id_cursor }`, posted to `/xmtp.mls.api.v1.MlsApi/SubscribeGroupMessages`. A sibling `subscribe_welcome_messages.rs` does the same for welcomes. This is the "per-group subscribe with `id_cursor`" model: a scalar cursor per group, and a **separate RPC per subscription kind**.

**d14n, `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs`:**
`SubscribeTopics` carries a `TopicCursor` and encodes `TopicFilterProto { topic, last_seen: Some(cursor) }` per topic against `/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeTopics`. Its own doc says it replaces "the old `SubscribeEnvelopes` endpoint which only supported a single shared cursor across all topics". Cursors here are **vector clocks**, not scalars.

Requirements: API-REQ-014 (v3 subscription endpoints) and API-REQ-019 (`SubscribeTopics`, "one filter per topic with its own vector cursor").

### 3.6 d14n stream combinators (XIP-49 ordering)

This is the machinery that exists **only** because d14n is multi-originator.

- `crates/xmtp_api_d14n/src/queries/stream/ordered.rs` — `OrderedStream<S, Store, T>` holds a `cursor_store` and a `topic_cursor`, ordering a stream "according to XMTP XIP-49" (module doc line 1).
- `crates/xmtp_api_d14n/src/protocol/order.rs` — `Ordered<T, R, S>` implements cross-originator ordering per XIP-49 §3.3.5. It resolves missing dependencies: `required_dependencies()` walks `e.depends_on()?` and builds a `HashSet<RequiredDependency>`; unresolvable envelopes go to an "icebox" (API-REQ-052: "items with missing dependencies are stored in the icebox").
- `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` — consumes v4 `SubscriptionStatus` frames (`Started`, `CatchupComplete`) out of the envelope stream and tracks liveness flags in `StreamStatus { has_started, catchup_complete, last_ping }` (API-REQ-053). **Dead plumbing:** every call site in `queries/d14n/streams.rs` binds the returned handle to `_status` (lines 36, 59, 71, 84, 96, 115), and the writers are `pub(crate)`, so no consumer can construct one. The status is computed and discarded — a third liveness mechanism that does nothing today.
- `crates/xmtp_api_d14n/src/queries/stream/extractor.rs` — flattens extracted envelope collections (API-REQ-051).
- `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs`, `resolve.rs`, `sort.rs` — the supporting cursor/dependency machinery.

**The two pipelines side by side** — this is the clearest single measure of what d14n costs.

d14n group messages (`queries/d14n/streams.rs:13-27`), four layers:

```text
Client::stream (gRPC bytes)
  → XmtpStream<SubscribeTopicsResponse>        decode prost, tag endpoint
  → StatusAwareStream                          strip Started/CatchupComplete frames
  → OrderedStream                              XIP-49 causal order + icebox
  → TryExtractorStream<GroupMessageExtractor>  flatten + extract → GroupMessage
```

v3 group messages (`queries/v3/streams.rs:18-20`), two layers:

```text
Client::stream → XmtpStream<V3ProtoGroupMessage> → TryFromItem → GroupMessage
```

The server orders; the client trusts it. Note that **d14n welcomes deliberately skip `OrderedStream`** (`d14n/streams.rs:27`) because welcomes have no causal dependencies — evidence that the ordering layer is needed only for the group-message case.

`depends_on` originates in `ClientEnvelope.aad` — the *publisher* declares what it had seen when it published (`protocol/extractors/depends_on.rs:31-43`), populated on the publish side by `CursorStore::find_message_dependencies`. Its doc notes: "If the envelope does not have dependency, or is already ordered (**as is the case for v3**), then returns `None`." A single-originator backend puts every message in the v3 case.

`payer` appears only on the **publish** path (`endpoints/d14n/publish_client_envelopes.rs`, `middleware/read_write_client/client.rs`) and as an envelope nesting layer (`OriginatorEnvelope` → `PayerEnvelope` → `ClientEnvelope`) that the read-path extractors merely traverse. It is out of scope for the streaming spec.

**Empty-subscription workaround.** All three d14n subscribe methods short-circuit an empty input to `endpoint.fake_stream(&self.client)` instead of dialing (`d14n/streams.rs:33-39, 68-74, 93-98`). `FakeEmptyStream` always returns `Poll::Pending`. Its header records why: "this is a workaround for <https://github.com/xmtp/xmtpd/issues/1440>... it should be used when subscribing with an empty topics list." **The new backend should define zero-topic subscribe behavior explicitly** so this workaround is unnecessary — see Q4.

---

## 4. The XIP-83 bidi client (already implemented)

### 4.1 Where it lives

| Component | Path | Lines |
| --- | --- | --- |
| Connection actor (backend-agnostic control core) | `crates/xmtp_api_d14n/src/queries/bidi.rs` | 1138 |
| Transport ledger (leases, waves, reconnect) | `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` | 6172 |
| Property tests for the ledger | `crates/xmtp_api_d14n/src/queries/bidi_transport_props.rs` | 751 |
| v3 wire binding | `crates/xmtp_api_d14n/src/queries/v3/connection.rs` | 911 |
| d14n wire binding | `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | 379 |
| v3 bidi adapter | `crates/xmtp_api_d14n/src/queries/v3/bidi.rs` | 179 |
| Client-level router | `crates/xmtp_mls/src/subscriptions/stream_router.rs` | 1685 |
| Dispatch, gate, callbacks | `crates/xmtp_mls/src/subscriptions/router_callbacks.rs` | 865 |
| Bounded catch-up | `crates/xmtp_mls/src/subscriptions/catch_up.rs` | 861 |

The split is deliberate and documented in `bidi_transport.rs` module docs (lines 1-10): the transport "speaks *transport* vocabulary — topics, envelopes, leases, waves — and knows nothing about MLS, databases, decryption, or streams. Decoding and dedup belong to the client-level consumer (the stream router), which is also the durable-cursor owner: **the transport never stores a cursor**, it only forwards the resume positions a lease hands it."

### 4.2 Wire protocol as implemented

Generated types confirm both flavors carry the full XIP-83 vocabulary.

v3 flavor, `crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs`:

- `SubscribeRequest.V1.Mutate` with `history_only` (line ~798-806) and `mutate_id` (line 816).
- `SubscribeResponse.V1.CatchupComplete { mutate_id }` (lines 966-969).
- `SubscribeResponse.V1.TopicsLive { topics }` (lines 989-993).
- Delivery frames carry `mutate_id` (line 920-923) — "The catch-up wave that produced this frame".

d14n flavor, `crates/xmtp_proto/src/gen/xmtp.xmtpv4.message_api.rs`: the same tree under `xmtp.xmtpv4.message_api.SubscribeRequest.V1.Mutate.Subscription` (line ~261), with `Envelopes.mutate_id` instead of `Messages.mutate_id`.

The generated comments state the invariants the client relies on:

- `mutate_id` "MUST be nonzero when adds are present (**0 is the live tag**)" and "MUST NOT match the mutate_id of a wave still in flight".
- `CatchupComplete` marks "the seam: live frames (mutate_id 0) for the wave's topics begin only after" it.
- `TopicsLive` is "Informational only: delivery correctness (no duplicates, no gaps) never depends on it."

The single most important protocol requirement the client imposes is stated in `bidi_transport.rs` module docs: the client **requires a tag-serving backend**. Against a backend that serves bidi without delivery tags, replay frames arrive as tag `0` and would be mis-filtered. Such a backend is detected — "its `CatchUpComplete` frames also carry no tag (`0`; ours are minted from 1), and the first one shuts the transport down" (`bidi_transport.rs:2379-2397`, `return self.shutdown()`) — and the destination falls back to legacy streams. There is deliberately **no capability bit and no untagged fallback** at this layer.

**The fallback is not transparent to the stream that discovers a tag failure.** Two different moments must be distinguished:

- **Open-time refusal falls back in place.** When the *first* wire open is refused — gRPC `UNIMPLEMENTED`, or the in-process `OtherUnretryable` refusal from the d14n and migration clients — the dispatch arm latches the destination and then runs the legacy stream **on the same task**, for the same caller (`router_callbacks.rs:536-571`: `Err(e) if is_bidi_unsupported(&e) || is_bidi_dead_end(&e) => { latch_bidi_unsupported(&host); ... run_watchdog_stream(...) }`). That caller never notices.
- **A mid-stream tag failure closes the stream.** A zero-tag `CatchUpComplete` arriving on a live wire tombstones the transport: every lease ends and every existing stream sees end-of-events. The test asserts exactly this — `an_untagged_backends_completion_tombstones_the_transport` requires `recv(&mut lease).await.is_none()`, "an untagged completion must end the lease, not resolve a wave" (`bidi_transport.rs:5605-5620`). Only a *later* `lease()`/subscribe sees `TransportError::Closed`, latches, and gets legacy service. An SDK retry wrapper or the caller must open a new stream to recover.

Requirement 42 is scoped to the first case.

### 4.3 Connection actor state machine

`crates/xmtp_api_d14n/src/queries/bidi.rs` module docs describe the actor precisely. It is "the **sole writer** of the request half", owns both halves, "auto-answers server `Ping`s, correlates client liveness probes, multiplexes caller commands onto the wire, and surfaces only real subscription events — **keepalive never reaches the consumer**."

Liveness policing is self-contained: "a wire with no inbound frames for the silence budget gets one watchdog ping, and if that too goes unanswered the actor tears down — so a half-open link surfaces as end-of-events instead of a stream that hangs forever."

Ownership is the teardown mechanism: "when inbound dies it drops both, so the request half tears down with it and any later `mutate`/`probe` fails with `Closed` by channel ownership — never by silently enqueueing into a stream nothing reads."

Backend-specific concerns sit behind `trait BidiBinding` (`bidi.rs:~88`), which names `Request`, `Response`, the backend's `Mutate` payload ("v3: single `id_cursor`; d14n: vector cursor"), and `handle()` to classify an inbound frame into an `Inbound` instruction — "which is where d14n runs its per-envelope extractors."

Nonce handling: `WATCHDOG_NONCE = u64::MAX` is reserved for the actor's own pings; `next_probe_nonce()` counts from 1 and skips both `0` and `WATCHDOG_NONCE`. The watchdog "doesn't correlate the pong anyway: *any* inbound frame proves the link and resets the window."

### 4.4 Transport ledger: leases, waves, delivery rules

From `bidi_transport.rs` module docs (the authoritative statement of semantics):

- **Every `lease()` is a cursored `Mutate` wave.** Adds are sent even if another lease already holds the topic, because "a cursored re-add is the one catch-up/replay mechanism (XIP-83)".
- **Delivery guarantee: exactly-once per lease for cursor-tracked messages** — a **conditional transport property**, not an app-visible guarantee. It rests on three server obligations: every delivery frame carries the wave's `mutate_id` (0 = live), the server serves live frames and each wave's replay "in total cursor order per kind", and "a topic owned by an in-flight wave receives NO live frames until that wave's `CatchUpComplete`." Its stated carve-outs (`bidi_transport.rs:73-77`): "messages with no extractable cursor (fail open to every holder)", and nothing below a lease's own floor on either lane. It is per **lease** — a new lease over the same topics replays. It holds for the V3 ledger only (`V3Binding` is the sole `TransportBinding`). Above this layer, dedup against local storage — not the wire — is what an application actually sees (§5.4, §5.6).
- **Admission is structural** — no per-lease positions and no consumer dedup contract. A frame past `last_seen` goes to every holder; a holder still owed history takes fresh frames only from a wave it rides, otherwise the frame is **withheld** and flushed just before its `CatchUpComplete`.
- **The sibling-gap rule.** A lease re-adding a shared topic at a lower cursor moves the topic into its wave, and live delivery stops *for every holder*. This is why `last_seen` is tracked **per topic**, not as one high-water mark per wire. The stated reason names an assumption the new backend must honor: while one topic sits gapped inside a wave, "its siblings' live frames (and cross-topic replay, **on the shared per-kind cursor sequence**) would push a global mark past the gap and mis-filter exactly the frames this rule exists to route" (`bidi_transport.rs:77-88`). So the ledger assumes **one total cursor sequence per kind, shared across every topic of that kind** — not merely per-topic monotonicity. See Q2.
- **Removes are refcounted.** "A topic leaves the wire only when the *last* lease holding it derefs. Dropping a `TopicLease` derefs its topics."
- **The wire is lazy** (opens on first lease) and closes gracefully (half-close + bounded drain) when the last lease derefs.
- **Backpressure, one policy at every layer:** "A lease that stops draining its channel is dropped — closed, dereferenced, never blocking the wire or its sibling leases. The dropped consumer's recovery is to re-lease from its durable cursors."

Frame chunking is explicit: `chunk_mutate_adds()` splits an add-set by **both** a count cap and a byte budget (`chunk_by_budget`), because a `Topic` is a `Vec<u8>` with no type-level size bound.

### 4.5 Reconnect, suspend/resume

**Reconnect** (`bidi_transport.rs` module docs, "Reconnect (a wire flap is invisible to leases)"): a dead wire does **not** close leases. The transport re-opens on capped exponential backoff, "forever; an offline process resumes when the network returns", and rebuilds state as two kinds of wave:

1. one **resume wave** re-adding every topic not owned by an interrupted wave, from `last_seen` — "its replay is exactly the outage gap, so no holder sees a repeat";
2. every lease still catching up **re-issues** under a fresh wave id, each topic re-added "at the meet of every stakeholder's position".

"Consumers never observe the flap. `next()` → `None` still means only: this consumer fell behind and was dropped, or the transport itself shut down."

**Suspend/resume** is the app-lifecycle touchpoint, and it applies **only to bidi transports**. `BidiTransport::suspend` half-closes the wire and parks; leases and wire positions are kept; nothing reconnects until `resume`, which "resolves once a live wire has no wave left in flight: 'catch up, then done', against everything owed, **the background-fetch primitive**." At the transport level `resume` is awaitable; the **FFI** wrapper is not — see §6.1. When bidi is off or no stream was ever opened, both are no-ops (`router_callbacks.rs:330-357`), so on the legacy path there is no lifecycle parking at all.

`MIN_STABLE_UPTIME` (10 s) guards against flap: only an open that proves stable resets the backoff floor, so "an accept-then-immediately-close server (an overloaded node RSTing right after accept)" cannot pin a client at the floor.

### 4.6 Constants table

All from `bidi_transport.rs` and `bidi.rs` (values are compile-time constants unless noted).

| Constant | Value | File | Meaning |
| --- | --- | --- | --- |
| `DEFAULT_LEASE_DEPTH` | 64 | bidi_transport.rs | transport→lease channel bound |
| `WITHHELD_FRAMES_CAP` | 512 | bidi_transport.rs | per-lease withheld-frame cap; tripping drops the lease |
| `MAX_MUTATE_TOPICS` | 1000 | bidi_transport.rs:189 | **client** chunk cap: topics packed per `Mutate` frame |
| `MAX_MUTATE_BYTES` | 16 MiB | bidi_transport.rs:197 | **client** byte budget per `Mutate` (under the 25 MiB encode limit) |
| `PER_ENTRY_OVERHEAD` | 64 B | bidi_transport.rs | assumed per-entry encoding overhead |
| `OUTBOX_RETRY_INTERVAL` | 25 ms | bidi_transport.rs | retry when the wire command buffer is momentarily full |
| `RECONNECT_INITIAL_DELAY` | 100 ms | bidi_transport.rs | backoff floor |
| `RECONNECT_MAX_DELAY` | 30 s | bidi_transport.rs | backoff ceiling (plus up to one base of jitter → ~60 s effective) |
| `MIN_STABLE_UPTIME` | 10 s | bidi_transport.rs | uptime needed before a death resets backoff |
| `GRACEFUL_CLOSE_BUDGET` | 5 s | bidi_transport.rs | cap on the half-close discard-drain |
| `WIRE_BUFFER` | 64 | bidi.rs:36 | wire-outbound depth |
| `COMMAND_BUFFER` | 64 | bidi.rs:38 | caller→actor command depth |
| `EVENT_BUFFER` | 1024 | bidi.rs:41 | actor→caller event depth |
| `DEFAULT_KEEPALIVE_MS` | 30 000 | bidi.rs:44 | fallback until `Started` advertises a cadence |
| `PROBE_TIMEOUT_MULTIPLIER` | 3 | bidi.rs:54 | N keepalive intervals = probe deadline and silence budget |
| `MAX_PENDING_FRAMES` | 128 (`WIRE_BUFFER * 2`) | bidi.rs | outbound backlog cap; past it the actor gives up |
| `DRAIN_FLUSH_BUDGET` | 1 s | bidi.rs | post-`finish` flush budget |

**`MAX_MUTATE_TOPICS` and `MAX_MUTATE_BYTES` are client-side chunking limits, not backend minimums.** Their docs say so: a larger add-wave "is split into this many topics per frame ... whichever is reached first closes the frame" (`bidi_transport.rs:181-189`), and the byte ceiling is "well under the transport's 25 MiB encode limit" (`:190-197`); `chunk_waves` applies both (`:1027-1032`). They describe what libxmtp *sends*, so they tell a backend author the largest frame to expect — they do not establish a server obligation. Server limits remain unspecified; see Q4 and requirement 8.

### 4.7 Draft `backend.proto` vs implementation

The draft at `docs/self-hosted/backend.proto` is nearly a match for what the client already speaks. Differences worth noting:

| Aspect | Draft proto | Implementation |
| --- | --- | --- |
| `Mutate.adds` cursor | `TopicQuery { topic, cursor }`, `Cursor { sequence_id }` — **scalar** | v3 binding uses a scalar `id_cursor`; d14n binding uses a **vector** cursor (`BidiBinding::Mutate` doc, `bidi.rs`) |
| `Started` | carries `keepalive_interval_ms` + `capabilities` | consumed for cadence; **capabilities are logged, not consumed** — `bidi_transport.rs` "Not yet here (later phases): `Started` capabilities are logged, not consumed (capability gating phase)" |
| Untagged backend | not addressed | client **refuses and tombstones** the transport; no capability bit, no fallback at that layer |
| `SubscribeOnce` | present, unidirectional, for web | **no client implementation found** for a `SubscribeOnce`-shaped RPC. The nearest analogue is `catch_up.rs`, which achieves bounded sync over the **bidi** wire using `history_only` + half-close — and is native-only. **[UNVERIFIED: no browser-side `SubscribeOnce` consumer exists today.]** |
| `SubscribeOnceResponse.Ping` | present | n/a — no consumer |
| `Capability` enum | `CAPABILITY_UNSPECIFIED` only | matches (nothing to gate on yet) |

The draft's `TopicsLive` and `CatchupComplete` comments match the client's assumptions closely, including "Informational only" for `TopicsLive` and the wave/live seam for `CatchupComplete`.

**The draft does not compile as written.** Two defects, both mechanical:

1. **No imports.** The file has only `syntax` (`backend.proto:2`) and `package xmtp.backend.v1;` (`:3`), yet `ClientEnvelope` references `xmtp.mls.api.v1.GroupMessageInput` (`:43`), `WelcomeMessageInput` (`:44`), `UploadKeyPackageRequest` (`:45`), `xmtp.identity.associations.IdentityUpdate` (`:46`) and `xmtp.mls.message_contents.CommitLogEntry` (`:47`). `IdentityService` (`:192-195`) likewise names four request/response messages that are neither defined nor imported.
2. **`repeated` inside a `oneof`.** `QueryNewestResponse` (`:61-65`) puts `repeated EnvelopeMeta envelope_metas = 1;` and `repeated ServerEnvelope envelopes = 2;` directly in `oneof response` — protobuf forbids this. Each arm needs a wrapper message. The file already does it correctly elsewhere: `SubscribeResponse.Messages` (`:99-106`) and `SubscribeOnceResponse.Messages` (`:153-155`) both wrap their repeated fields, so `:61-65` is an isolated slip.

A third, non-fatal point: proto3 has no `required`, so "mandatory `mutate_id`" is a behavioral rule the comments carry, not something the schema enforces. `0` is overloaded — it means both "live frame" and "a waveless Mutate carried 0" (`:126`, `:132-133`) — which is why the client can only read a zero echo as a capability verdict (§4.2).

---

## 5. Client layer (`xmtp_mls/src/subscriptions`)

### 5.1 Public API surface

All from `crates/xmtp_mls/src/subscriptions/mod.rs` unless noted.

| API | Line | Signature shape | Yields |
| --- | --- | --- | --- |
| `Client::stream_conversations` | 450 | `(Option<ConversationType>, include_duplicate_dms: bool) -> impl Stream<Item = Result<MlsGroup<Context>>>` | new conversations |
| `Client::stream_conversations_owned` | 469 | same, `'static` | new conversations |
| `Client::stream_conversations_with_callback` | 493 | `(Arc<Client>, type, convo_callback, on_close, include_duplicate_dms) -> impl StreamHandle` | callback |
| `Client::stream_all_messages` | 524 | `(Option<ConversationType>, Option<Vec<ConsentState>>) -> impl Stream<Item = Result<StoredGroupMessage>>` | messages across all conversations |
| `Client::stream_all_messages_owned` | 540 | same, `'static` | messages |
| `Client::stream_all_messages_with_callback` | 555 | `(Context, type, consent_state, callback, on_close) -> impl StreamHandle` | callback |
| `Client::stream_consent_with_callback` | 578 | `(Arc<Client>, callback, on_close)` | `Vec<StoredConsentRecord>` — **local only** |
| `Client::stream_preferences_with_callback` | ~604 | `(Arc<Client>, callback, on_close)` | `Vec<PreferenceUpdate>` — **local only** |
| `Client::stream_message_deletions_with_callback` | ~630 | `(Arc<Client>, callback, on_close)` | `DecodedMessage` — **local only** |
| `Client::process_streamed_welcome_message` | 386 | `(envelope_bytes: Vec<u8>) -> Result<Vec<MlsGroup>>` | out-of-process push-notification path |
| `MlsGroup::stream` | `groups/subscriptions.rs:101` | `() -> impl Stream<Item = Result<StoredGroupMessage>>` | messages for one group |
| `MlsGroup::stream_owned` | `groups/subscriptions.rs:116` | same, `'static` | messages for one group |
| `MlsGroup::stream_with_callback` | `groups/subscriptions.rs:127` | `(Context, GroupId, callback, on_close) -> impl StreamHandle` | callback |
| `MlsGroup::process_streamed_group_message` | `groups/subscriptions.rs:39` | `(envelope_bytes: Vec<u8>) -> Result<Vec<StoredGroupMessage>>` | push-notification path (GINLINE-REQ-070) |
| `Client::catch_up_to_live` | `catch_up.rs` | bounded sync, then stop | work report |

Note the asymmetry that the redesign should keep in mind: the *iterator* forms (`stream_*`) exist alongside *callback* forms (`stream_*_with_callback`). The bindings mostly use the callback forms — but not entirely: WASM's `streamLocal` exposes an iterator form directly (`bindings/wasm/src/conversations.rs:595-606`). Only the `*_with_callback_dispatch` wrappers consult the bidi gate; every iterator form and the plain `*_with_callback` forms are legacy-only (§5.2).

### 5.2 Two parallel architectures and the dispatch gate

`crates/xmtp_mls/src/subscriptions/mod.rs:20-55` shows the module gating. The bidi pieces (`catch_up`, `stream_router`, `router_callbacks`) are all `#[cfg(not(target_arch = "wasm32"))]` — "native-only — full-duplex HTTP/2 is unavailable on the wasm gRPC-Web transport".

`router_callbacks.rs` module docs define the dispatch policy:

- **One wire per destination.** "Clients dialing the same backend URL share ONE `BidiTransport`". Wires are keyed by `XmtpMlsBidiStreams::host()` — "connection identity, not network identity: two clients behind the same proxy share that proxy's wire, while a proxied and a direct client to the same backend keep separate wires and separate failure domains".
- **The gate:** `BIDI_STREAMS_ENABLED_ENV = "XMTP_BIDI_STREAMS_ENABLED"` (`router_callbacks.rs:104`), "read once at the first stream call: mobile apps set it at process init, agents in their deploy env. Unset (or anything but a truthy value) keeps the legacy streams."
- **The latch:** whether a backend *can* serve the surface is discovered at first wire open. A refusal — "gRPC `UNIMPLEMENTED` from a v3 node without the surface, an in-process refusal from the d14n and migration api clients" — latches that destination onto legacy streams. "Destinations latch independently". "Latches reset only with the process".
- **Pump semantics:** each stream is one spawned task; `on_close` fires once at natural end; "An `end()`ed handle aborts the task without `on_close`, matching the local-events streams."

So today the bidi path is **opt-in and off by default**, and every deployment still depends on the legacy path working.

**What the gate actually covers.** `bidi_streams_active(host)` (`router_callbacks.rs:161-165`) is called from exactly three places — the `*_with_callback_dispatch` wrappers for all-messages (`:670-694`), conversations (`:734-741`) and per-conversation messages (`:802-814`). Everything else is legacy regardless of the gate:

| Surface | Path | Gate? |
| --- | --- | --- |
| `Client::stream_conversations` / `_owned` | `mod.rs:450-484` → `StreamConversations` | no |
| `Client::stream_all_messages` / `_owned` | `mod.rs:524-546` → `StreamAllMessages` | no |
| `MlsGroup::stream` / `stream_owned` | `groups/subscriptions.rs:101-125` → `StreamGroupMessages` | no |
| `MlsGroup::stream_with_callback` | `groups/subscriptions.rs:127` | no |
| WASM `streamLocal` | `bindings/wasm/src/conversations.rs:595-606` → `stream_conversations_owned` | no (and bidi is compiled out on wasm anyway) |
| the three `*_with_callback_dispatch` fns | `router_callbacks.rs:670,734,802` | **yes** |

Each ungated form is bounded on `Context::ApiClient: XmtpMlsStreams` — the legacy trait — so it cannot take the bidi path even if the gate is on. §10.1 must account for rewriting them.

**d14n never takes the bidi path for app streams.** Its client's `subscribe_bidi` returns an unretryable refusal under a sentinel `host()` of `"unsupported://d14n"` (`crates/xmtp_api_d14n/src/queries/d14n/streams.rs:120-156`), which is exactly the open-time refusal the latch handles: the first stream falls back in place and every later dispatch to that host goes straight to legacy.

### 5.3 Legacy stack internals

**`stream_all.rs` (215 lines)** — `StreamAllMessages` is a `#[pin_project(PinnedDrop)]` struct composing two sub-streams: `conversations` and `messages` (`stream_all.rs:31-42`). Its `poll_next` (`stream_all.rs:~175-215`) does, in order:

1. poll `messages`; if a message arrives and its group is a **sync group**, swallow it, send `SyncWorkerEvent::NewSyncGroupMsg`, and return `Pending` — internal sync traffic never reaches the app;
2. if `messages` returned `None`, end the stream;
3. otherwise poll `conversations`, and on a new group call **`this.messages.as_mut().add(group_result)`** — this is the add-group-to-active-stream mechanism.

Construction (`from_cow`) first runs `WelcomeService::new(...).sync_welcomes()`, then `find_groups(...)` to seed the active conversation set, then builds both sub-streams.

**`stream_messages.rs` (659 lines)** — `StreamGroupMessages` is a poll state machine (`ProjectState::{Waiting, Processing, ...}`). `add()` (`stream_messages.rs:~255`) pushes onto an `add_queue`; `poll_next` in `Waiting` pops the queue and calls `resolve_group_additions`, which ultimately calls `subscribe()`.

**The critical legacy cost is in `subscribe()` (`stream_messages.rs:~285-303`):** it calls `context.api().subscribe_group_messages_with_cursors(&topic_cursor)` with the **whole** topic set. Adding one group therefore tears down and re-opens the entire group-message subscription. This is precisely what XIP-83's incremental `Mutate.adds` removes.

Seeding uses durable cursors: `db.get_last_cursor_for_ids(...)` and `db.messages_newer_than(&cursors_by_group)` build a `seen_cursors` set, and "Identity dedup is exact, so the flattened set is safe across groups" (`stream_messages.rs:177-206`).

**`stream_conversations.rs` (686 lines)** — merges two sources: a `BroadcastGroupStream` over `context.local_events().subscribe()` (for locally created groups) and the network welcome subscription `api.subscribe_welcome_messages(&installation_key)` (`stream_conversations.rs:280-287`). Dedup is a `known_welcome_ids: HashSet<Cursor>` seeded from `conn.group_cursors()` (`stream_conversations.rs:171,287`) and inserted at line 400.

### 5.4 XIP-83 router internals

`stream_router.rs` module docs give the design:

- **Shape:** "One `StreamRouter` per client... Every stream is its own task owning its lease, its dedup state, and its delivery channel. Streams share no mutable state, so a stall in one — say a multi-second recovery sync inside the pipeline — backs up only that stream's own lease channel... and never delays a sibling."
- **Welcomes are not inline:** "joining a group can run a full network sync, and a welcome inline on the lease-draining loop would wedge the whole stream behind it". Welcomes fan out to a capped set of spawned tasks (`WelcomeIntake`).
- **Cursors:** "Streams are seeded from the client's **durable** application cursors (`refresh_state`), never wire cursors: the cursored lease is the one catch-up mechanism".
- **Dedup:** because the transport delivers exactly once per lease, "streams do no wire-level dedup at all". What the wire cannot see is the local store: "the pipeline stores messages without advancing the durable cursor, so a new lease legitimately replays identities an earlier stream already delivered — that overlap is historical, deduped by exact identity during the topic's **catch-up window** (subscribe until its `CatchUpComplete`)." A recovery sync can surface a message ahead of its envelope, so "each surfaced-ahead identity is held until that envelope arrives and consumes it (see `StreamDedup`)."
- **Dedup keys on local persistence, not application consumption.** The seen-set is seeded from the database, not from what a callback actually received: `seed_groups` reads `get_last_cursor_for_ids(...)` then `db.messages_newer_than(&seeds)` (`stream_router.rs:697-716`), and `GroupSeeds`' doc explains why — "the streaming pipeline stores messages WITHOUT advancing the durable cursor ... everything streamed since the last full sync is stored but still above it. Those exact identities seed the window's seen-set, so the server's replay of them is skipped" (`:667-675`). Suppression is then `if self.dedup.suppress(&topic, &cursor)` (`:1377`). The consequence: a message the pipeline **stored but the app never observed** (the process died between store and callback, or the consumer was dropped for backpressure mid-batch) is treated as already delivered and is suppressed on reopen. §6.6 item 9 and requirement 34 are scoped accordingly.
- **Welcome dedup** is a per-stream known-welcome set, made idempotent by known/tracked-topic guards.
- **Backpressure:** identical policy — a stream that stops draining its bounded channel is dropped, its lease derefs, and the consumer recovers by re-subscribing from durable state.

### 5.5 Message processing, sync-on-stream, recovery

`process_message.rs` module docs (line 4) state the core rule: "in a stream, we must defer to the 'sync' function whenever we receive a message" that cannot be processed directly.

`process_one()` (`process_message.rs:178`) is two-phase:

1. **Fast path** — `prepare()` returns `Prepared::Ready` if the message is already stored locally ("e.g. a replayed envelope after a cursor'd re-add — see XIP-83 client integration"), returned without decrypting.
2. Otherwise `Prepared::NeedsProcessing` runs the full decrypt/store pipeline, "which may trigger a **recovery sync** for out-of-order / commit-dependent messages".

Separation of duties is explicit (`process_message.rs:100-116`): "Dedup (skipping already-seen cursors) and cursor bookkeeping are intentionally *not* here... The caller owns dedup and cursor advancement (see `Processed`)." `Processed` carries `next_cursor` (what to advance to) and `tried` (what was attempted).

Recovery behavior is asserted by the unit tests in the same file: `test_process_returns_correct_cursor` forces `process()` to error, expects exactly one `recover()` call, and asserts the returned `next_message` equals the current message's cursor. `test_process_returns_correct_cursor_on_err` covers mixed error/success summaries via an `rstest` template with seven cases.

Requirement MLS-REQ-079 ("Recovery-sync surfacing": "If the triggering envelope fails but...") and GTEST-REQ-063 ("Out-of-order stream processing") cover this.

**Commits and cursors: what "the stream does not apply commits" really means.** The streaming path processes with `trust_message_order = false` (`process_message/factory.rs:115`, `group.process_message(msg, false)`). That flag gates two things at once (`groups/mls_sync.rs:2452-2458`):

```rust
let allow_epoch_increment = trust_message_order;
let allow_cursor_increment = trust_message_order;
if !allow_epoch_increment && envelope.is_commit() {
    return Err(GroupMessageProcessingError::EpochIncrementNotAllowed);
}
```

So a commit arriving **inline on the stream always errors**, and inline processing never advances the durable cursor (`mls_sync.rs:2525-2531`, "will not call update cursor ... allow_cursor_increment is false"). But that error is not the end of the story: `process_or_recover` routes any non-`MessageAlreadyProcessed`/`ProcessIntent` failure to `recover()` (`factory.rs:284-298`), which calls `group.sync_with_conn()` (`factory.rs:142`) → `receive()` → `process_messages()` → `process_message(&message, **true**)` (`mls_sync.rs:2845-2853`). The query path trusts order, so it **applies the commit and advances the cursor**.

The accurate statement is therefore two-part: *streams never apply commits or advance durable cursors inline; a stream-triggered recovery sync applies them in query order and does advance cursors.* Requirement 37 is worded to match. The recovery log line confirms it — "recovery sync processed=[{}] messages, group@[{}] now in epoch=[{}]" (`factory.rs:146-152`).

### 5.6 Cursors, dedup, ordering

| Question | Answer | Evidence |
| --- | --- | --- |
| Does the client drop messages with cursor ≤ stored? | Yes on the legacy path. `if self.groups.has_seen(next_msg.cursor) { warn; return Pending }` | `stream_messages.rs:436-443` |
| Does a stream delivery advance the durable cursor? | **Not inline.** Stream processing runs with `trust_message_order = false`, so `allow_cursor_increment` is false and "the pipeline stores messages without advancing the durable cursor". **But** a stream-triggered recovery sync runs the query path with `true` and does advance it. | `stream_router.rs` module docs; `catch_up.rs` "Durable cursors"; `process_message/factory.rs:115,142,284-298`; `mls_sync.rs:2452-2458,2525-2531,2845-2853` |
| Does a stream apply commits? | **Not inline** — a streamed commit returns `EpochIncrementNotAllowed`. The recovery sync it triggers applies it in query order. | `mls_sync.rs:2454-2458`; `factory.rs:284-298` |
| Welcome cursor on stream path? | Not advanced either: welcomes processed with increment OFF (`process_new_welcome(.., false, ..)`); "Only the legacy full-sync fallback advances the welcome cursor" | `catch_up.rs` module docs |
| Dedup mechanism (legacy messages) | `seen_cursors` set from `messages_newer_than`, plus per-group `has_seen` | `stream_messages.rs:177-206, 436` |
| Dedup mechanism (legacy welcomes) | `known_welcome_ids: HashSet<Cursor>` from `group_cursors()` | `stream_conversations.rs:171,287,400` |
| Dedup mechanism (router) | `StreamDedup` — exact identity within a catch-up window + surfaced-ahead holds | `stream_router.rs` module docs; MLS-REQ-106 |
| Ordering assumption | Legacy v3: per-group `id_cursor` monotonic. d14n: XIP-49 causal order via `OrderedStream`. XIP-83: server serves "in total cursor order per kind" per wave | `protocol/order.rs`; `bidi_transport.rs` docs |

The consequence of *not* advancing durable cursors **inline** on the stream path is a deliberate safety property: "a failed message is never skipped past and a later query-path sync retries it" (`catch_up.rs`). The cost is re-delivery of the tail on the next subscribe, absorbed by the seen-set — which is itself seeded from storage, so it also suppresses anything stored-but-unconsumed (§5.4).

### 5.7 Local-only streams (consent, preferences, deletions)

`LocalEvents` (`mod.rs:95-101`) has exactly three variants: `NewGroup(GroupId)`, `PreferencesChanged(Vec<PreferenceUpdate>)`, `MsgsDeleted(Vec<StoredGroupMessage>)`.

`impl StreamMessages for broadcast::Receiver<LocalEvents>` (`mod.rs:176-208`) provides `stream_consent_updates`, `stream_preference_updates`, `stream_message_deletions`. Each is a `BroadcastStream` + `filter_map` over the corresponding filter fn (`consent_filter`, `preference_filter`, `message_deletion_filter`, `mod.rs:139-167`).

Lag handling is `xmtp_common::optify!(event, "Missed message due to event queue lag")` — a lagged broadcast receiver drops the missed events, logs, and **the stream continues**. Loss without termination, and without any recovery path: there is no replay on a broadcast channel. `BroadcastGroupStream` in `stream_conversations.rs:83-91` does the same for `LocalEvents::NewGroup`.

**"Will not fail like other streams" is about network failure, not infallibility.** Two ways these streams can still lose or fail:

- **Lag drops.** As above — silent, logged, unrecoverable.
- **Deletion decoding can fail.** `stream_message_deletions` ends with `.map(|m| DecodedMessage::try_from(m).map_err(Into::into))` (`mod.rs:198-206`), and the comment concedes it: "let caller handle any potential decode failures". On mobile the caller cannot: `FfiXmtpClient::stream_message_deletions` writes `if let Ok(message) = msg { ... }` with an empty `|| {}` close hook (`bindings/mobile/src/mls.rs:2133-2147`), and `FfiMessageDeletionCallback` (`mls.rs:4010-4012`) has **only** `on_message_deleted` — no `on_error`, no `on_close`, unlike the other four callback traits. So a deletion-decode failure is discarded and invisible to Android and iOS.

The node SDK's phrasing to app developers is "a local stream, does not require network sync, and **will not fail like other streams**" (`sdks/js/node-sdk/src/Conversations.ts:~508-512`) — accurate about the network, silent about the two cases above.

`SyncWorkerEvent` (`mod.rs:104-111`) is a separate internal channel (`NewSyncGroupFromWelcome`, `NewSyncGroupMsg`, `SyncPreferences`, `CycleHMAC`, `Tick`) consumed by the device-sync worker, not by app streams.

### 5.8 Watchdog

`watchdog.rs` (762 lines) exists **only** because v3 has no server keepalive. Its module doc is blunt:

> A long-lived subscription whose transport wedges open (e.g. an L7 proxy keeps answering HTTP/2 keepalive pings while the backend subscription is gone) delivers neither an error nor a stream close — the consumer simply hangs forever. **There is no server-side keepalive on the v3 path today**, so the client cannot distinguish a healthy-but-idle stream from a dead one.

`WatchdogStream` arms an idle timer that resets on every item; on expiry it yields exactly one `SubscribeError::StreamStale` and then terminates (`watchdog.rs:335,551`), and consume loops treat that as "reconnect from the persisted cursor".

Because there is no keepalive, "the timeout is deliberately long: **a healthy dormant stream WILL trip and reconnect periodically**. That cost is accepted for the floor and shrinks once a server heartbeat exists (see XIP-83)."

| Knob | Env var | Default | Max |
| --- | --- | --- | --- |
| enabled | `XMTP_STREAM_WATCHDOG_ENABLED` | **false** | — |
| idle timeout | `XMTP_STREAM_WATCHDOG_IDLE_TIMEOUT_SECS` | 300 s | 86 400 s |
| reconnect base | `XMTP_STREAM_WATCHDOG_RECONNECT_BASE_SECS` | 1 s | 3 600 s |
| reconnect jitter | `XMTP_STREAM_WATCHDOG_RECONNECT_JITTER_MS` | 1000 ms | 3 600 s |

(`watchdog.rs:73-88`.) It is opt-in and off by default.

`spawn_watchdog_stream` is the runner used by `stream_conversations_with_callback` and `stream_all_messages_with_callback` (`mod.rs:508,570`) — it re-subscribes **after a stale trip only**. The loop is explicit: "Reconnect only on a watchdog stale-trip; a clean end or cancellation ends it" (`watchdog.rs:462-465`), and a `None` from the stream is a normal end that terminates the runner (`:458`). Since the watchdog is disabled by default, **the default legacy configuration never reconnects at all** — a normal close is terminal in the core layer, and only the JS SDK's `createStream` retry wrapper reopens it.

`mod.rs:495-503` documents the residual gap, which matters to requirement 30: re-subscribing recreates the `LocalEvents` receiver, which has no replay, so a **locally created** group broadcast in the re-subscription window can be missed. Network welcomes are safe — they replay from the persisted cursor — but "the only residual gap is a *locally* created group (`LocalEvents::NewGroup`) broadcast in the brief window while the new subscription is being built — bounded, since the caller already holds that group."

### 5.9 `catch_up.rs` — bounded sync

`Client::catch_up_to_live` "brings the local store current with the server — every pending welcome joined, every leased-topic message replayed and processed — and then stops, leaving nothing running." It is "XIP-83 bounded sync: one `Subscribe` stream carrying a `history_only` Mutate for every topic this client owns (each from its durable cursor)... a half-close once everything owed has arrived, and a server-side close in reply."

It deliberately uses **its own wire**, not the shared transport, because "a `history_only` wave never registers for live delivery, so it has no place in that bookkeeping".

It has a **discovery loop**: processing a welcome that becomes a group issues a follow-up `history_only` Mutate for that group's topic on the same stream, and the half-close waits until no wave is outstanding and no welcome is still processing, "so chained discoveries extend the run instead of escaping it."

Two things deliberately do not extend the run: locally created groups mid-run, and messages published after a topic's wave was frozen.

This function is the closest existing analogue to the draft proto's `SubscribeOnce`, and it is the primitive mobile background-fetch should use.

### 5.10 Errors and retryability

`SubscribeError` (`mod.rs:211-291`) has 16 variants. Retryability (`mod.rs:319-348`):

| Variant | Retryable | Note |
| --- | --- | --- |
| `Router(RouterError::Closed)` | **false** | client teardown |
| `Router(Transport/Subscribe)` | delegated | |
| `GroupMessageNotFound` | true | |
| `Decode(prost::DecodeError)` | **false** | |
| `Conversion` | **false** (delegated) | |
| `StreamStale` | true | watchdog trip → reconnect |
| `Group`, `Storage`, `NotFound`, `MessageStream`, `ConversationStream`, `ApiClient`, `BoxError`, `Db`, `Envelope`, `Enriched` | delegated via `retryable!` | |

`NeedsDbReconnect` is implemented separately (`mod.rs:350+`) so the device-sync worker stops on a dropped pool; `BoxError` returns `false` because it is opaque.

---

## 6. Bindings and SDKs

### 6.1 `bindings/mobile` (uniffi)

`FfiStreamCloser` (`bindings/mobile/src/mls.rs:3906`) is the app-visible handle:

| Method | Line | Semantics |
| --- | --- | --- |
| `end()` | 3939 | `abort_handle.end()` — abort without `on_close` |
| `end_and_wait()` | 3945 | awaits shutdown; `Err(Cancelled)` → `Ok(())`; `Err(Panicked(msg))` → `FfiError` |
| `is_closed()` | 3968 | |
| `wait_for_ready()` | 3973 | resolves once the stream is subscribed — **but see below** |

**`wait_for_ready` is a barrier, not a success signal.** Both `StreamHandle` implementations take the oneshot receiver and discard the result: `if let Some(s) = self.ready.take() { let _ = s.await; }` (`crates/xmtp_common/src/stream_handles.rs:104-108` wasm, `:234-238` tokio). The trait method returns `()` , not a `Result` (`stream_handles.rs:31`), so it cannot report failure. A **dropped** sender — the spawned task died before signaling ready — resolves the await exactly like a successful subscription. Awaiting it proves the task got far enough to drop or send the sender; it does not prove the subscription is registered.

Callback traits (`mls.rs:3982-4012`), each with the same error/close shape:

| Trait | Value method | Also |
| --- | --- | --- |
| `FfiMessageCallback` | `on_message(FfiMessage)` | `on_error`, `on_close` |
| `FfiConversationCallback` | `on_conversation(Arc<FfiConversation>)` | `on_error`, `on_close` |
| `FfiConsentCallback` | `on_consent_update(Vec<FfiConsent>)` | `on_error`, `on_close` |
| `FfiPreferenceCallback` | `on_preference_update(Vec<FfiPreferenceUpdate>)` | `on_error`, `on_close` |
| `FfiMessageDeletionCallback` | `on_message_deleted(Arc<FfiDecodedMessage>)` | **neither** — no `on_error`, no `on_close` (`mls.rs:4010-4012`) |

The deletion trait's missing methods are load-bearing: the binding drops every `Err` (`if let Ok(message) = msg`) and passes an empty close hook (`mls.rs:2133-2147`), so a deletion-stream decode failure or close is unobservable from Android and iOS.

**Process-scoped lifecycle functions** (`mls.rs:205,222`), both `#[uniffi::export]` free functions because "one streaming wire is shared across every client in the process":

- `suspend_streams()` — "Take the streaming wire off the network — the 'app entered background' half... Kept subscriptions and their wire positions survive". No-op when nothing is streaming.
- `resume_streams()` — "**Fire-and-forget**: it enqueues the resume and returns immediately... Do **not** treat its return as 'synced'". Callers wanting a bounded, awaitable "I am current now" are directed to `FfiXmtpClient::catch_up_to_live`.

These three (`suspend`, `resume`, `catch_up_to_live`) are app-visible API that only the bidi stack can implement, so they constrain the new design directly. Their scope is narrow and must be stated exactly:

- Both delegate straight to `router_callbacks::{suspend,resume}_bidi_streams` (`mls.rs:205-207, 221-224`), which are **no-ops when the bidi path is off or no stream was ever opened** (`router_callbacks.rs:330-357`). On the legacy path — which is the default — an app calling them gets nothing.
- `resume_streams` is **fire-and-forget**. Its own doc: "it enqueues the resume and returns immediately ... Do **not** treat its return as 'synced'." Its return is not catch-up completion; `catch_up_to_live` is the bounded, awaitable form.

**Android and iOS drive these automatically by default.** Both SDKs expose a process-global toggle `manageStreamLifecycle`, default **`true`**: `sdks/android/library/src/main/java/org/xmtp/android/library/Client.kt:171` and `sdks/ios/Sources/XMTPiOS/Client.swift:215`. When on, the first `Client` created registers a lifecycle observer (Android `ProcessLifecycleOwner`, iOS UIKit notifications) that parks the shared wire on background and revives it on foreground (`Client.kt:499-500,707-708`; `Client.swift:325-326,437-438`). It is process-global, not per-client, and must be set **before the first client is created** to opt out. On iOS it "has no effect on platforms without UIKit."

### 6.2 `bindings/node` (napi)

`bindings/node/src/streams.rs` exposes `StreamCloser` with `end()` (35), `end_and_wait()` (44), `wait_for_ready()` (68), `is_closed()` (77) — mirroring the mobile surface. Per-area stream functions live in `src/conversations/streams.rs` and `src/conversation/streams.rs`.

### 6.3 `bindings/wasm`

`bindings/wasm/src/streams.rs` exposes `StreamCloser` (line 53) with the same four methods as node and mobile: `end()` (74), `end_and_wait()` (82), `wait_for_ready()` (107), `is_closed()` (118). The whole bidi stack is compiled out here, so browsers are permanently on the legacy `XmtpMlsStreams` path plus the client-side watchdog (`crates/xmtp_proto/src/api_client.rs:172-177`).

WASM also exposes a **pull-stream** surface the other bindings do not: `Conversations.streamLocal` (`bindings/wasm/src/conversations.rs:595-606`, `#[wasm_bindgen(js_name = streamLocal)]`) returns a `web_sys::ReadableStream` built from `stream_conversations_owned` rather than taking a callback. It has no `StreamCloser`, so `end`/`wait_for_ready`/`is_closed` do not apply to it — the consumer cancels the `ReadableStream` instead. The callback form beside it (`conversations.rs:609`) calls `stream_conversations_with_callback`, not the `_dispatch` variant, so neither wasm surface consults the bidi gate (moot on wasm, where bidi is compiled out).

**The closer contract is identical across all three bindings** — `end` / `end_and_wait` / `wait_for_ready` / `is_closed`. Preserving those four is a hard requirement for any redesign, and they are already backend-agnostic.

### 6.4 JS SDKs — the app-visible retry contract

`sdks/js/node-sdk/src/utils/streams.ts` and `sdks/js/browser-sdk/src/utils/streams.ts` implement `createStream`, the wrapper every JS stream API goes through.

**The two packages differ in more than defaults.** Treat them as two contracts until the state machines are unified.

| Default | node-sdk | browser-sdk |
| --- | --- | --- |
| `DEFAULT_RETRY_ATTEMPTS` | **10** | **6** |
| `DEFAULT_RETRY_DELAY` | **60 000 ms** | **10 000 ms** |

(`node-sdk/src/utils/streams.ts:6-7`; `browser-sdk/src/utils/streams.ts:13-14`.)

| Behavior | node-sdk | browser-sdk |
| --- | --- | --- |
| retry disabled (`retryOnFail: false`), first native close | `fail(new StreamFailedError(0))` — terminal `onError` is invoked (`streams.ts:299-302`) | `void asyncStream.end(); throw new StreamFailedError(0)` from inside `handleNativeClose` — the stream ends and **no terminal `onError` fires**; the throw surfaces as an unhandled rejection, not to the caller (`streams.ts:302-305`) |
| readiness barrier | `await streamCloser.waitForReady()` after every open, with a stopped-flag recheck around it (`streams.ts:313-320`) | **none** — the closer is a bare function and `waitForReady` appears nowhere in the file (`streams.ts:314-319`) |
| what "open" means | the FFI barrier resolved (weak — see §6.1) | the worker acked `stream.` setup. `workers/client.ts:569-583` calls `client.conversations.stream(...)` **without awaiting**, then `postMessage({ id, action, result: undefined })`. The ack means "stream object constructed", not "subscription registered" |

So the browser resolves its stream-open earlier than node does, and reports a non-retryable failure through a different (weaker) channel.

`StreamOptions` (node `streams.ts:9-53`) is the public option surface:

| Option | Meaning |
| --- | --- |
| `onEnd?()` | stream ended |
| `onError?(error)` | a stream error occurred |
| `onFail?()` | the native stream closed unexpectedly |
| `onRestart?()` | the stream was restarted |
| `onRetry?(attempts, maxAttempts)` | a retry was attempted |
| `onValue?(value)` | a value was emitted |
| `retryAttempts?` | default 10 (node) / 6 (browser) |
| `retryDelay?` | default 60 s (node) / 10 s (browser) |
| `retryOnFail?` | default true |
| `disableSync?` | default false — skip the network sync before starting |

The retry state machine has properties the tests pin down and apps depend on:

- **Ending is terminal.** "Ending the stream is terminal: no callbacks are invoked and no native stream is created after the stream ends." `asyncStream.onDone = stop` is registered *before* any async work.
- **The retry budget is monotonic.** "it is never reset for the lifetime of this wrapper, so restarts are bounded even across successful restarts" (`streams.ts`, `remainingRetries`).
- **At most one retry in flight** (`retryInFlight` single-flight guard) — JSDK-REQ-020.
- **A close during an in-flight restart is not lost** (`closePendingDuringRestart`) — the completed attempt reschedules rather than installing a dead closer.
- **Budget exhaustion is terminal:** `fail(new StreamFailedError(retryAttempts))`.
- **`retryOnFail: false`** turns the first native close into `StreamFailedError(0)`.
- **A throwing `onError` must not wedge the stream:** `try { onError?.(error) } finally { void asyncStream.end() }`.
- **`waitForReady()` is awaited** after every open, and the stopped flag is re-checked after each await.

**Sync-before-stream is an SDK convention, not a protocol feature — and the two packages sync differently.** Every JS stream entry point runs a network sync before opening, guarded by `if (!options?.disableSync)`. Apps rely on "subscribe, and I am also caught up". Which sync runs is not uniform:

| Entry point | Sync call |
| --- | --- |
| node `Conversations.stream` | `await this.sync()` (`node-sdk/src/Conversations.ts:349-351`) |
| node `Conversations.streamGroups` / `streamDms` | `await this.sync()` (`Conversations.ts:393, 421`) |
| node `Conversations.streamAllMessages` | **`await this.syncAll(options?.consentStates)`** (`Conversations.ts:452-454`) |
| browser `Conversations.streamAllMessages` | **`await this.sync()`** (`browser-sdk/src/Conversations.ts:493-496`) |
| node `Conversation.stream` | `await this.sync()` (`node-sdk/src/Conversation.ts:152-154`) |
| browser `Conversation.stream` | `await this.sync()` (`browser-sdk/src/Conversation.ts:491-494`) |

The node/browser split on `streamAllMessages` is app-visible: node syncs every conversation matching the consent filter, browser syncs the conversation list only. `disableSync` opts out everywhere and defaults to `false`. Requirements 53 and 54 capture this.

`streamAllMessages` also re-reads the message from the DB for enrichment and warns when absent: `console.warn(\`Streamed message with ID "${value.id}" not found\`)`(`Conversations.ts:~464-467`).

Convenience wrappers `streamAllGroupMessages` / `streamAllDmMessages` just set `conversationType` (`Conversations.ts:471-508`).

**Per-conversation and deletion stream APIs** the inventory above omits:

| API | Path | Shape |
| --- | --- | --- |
| `Conversation.stream(options)` | `node-sdk/src/Conversation.ts:147`; `browser-sdk/src/Conversation.ts:483` | messages for one conversation, via `createStream` |
| `Conversations.streamDeletedMessages` | `node-sdk/src/Conversations.ts:551`; `browser-sdk/src/Conversations.ts:604` | preferred form; yields `DecodedMessage` |
| `Conversations.streamMessageDeletions` | `node-sdk/src/Conversations.ts:523` (`@deprecated Use streamDeletedMessages instead`, `:521`); `browser-sdk/src/Conversations.ts:569` | deprecated alias; yields ids |

Both deletion forms call the same `this.#conversations.streamMessageDeletions(callback)` (`:536, :564`) and both `Omit` the `disableSync`/`onFail`/retry options — they are local streams (§5.7).

### 6.5 Android / iOS

Requirements covered here:

- `ANDROID-REQ-065` Android conversation and message streams (per-conversation and aggregate).
- `ANDROID-REQ-105` consent and preference update streams.
- `ANDROID-REQ-092` conversation maintenance, count, and deletion stream.
- `IOS-REQ-055` `streamMessageDeletions` emits ID removals; `IOS-REQ-070` raw delete-message stream.
- `IOS-REQ-118` "Streaming across a metadata update — one group remains usable".
- `SHARED-IDENTITY-REQ-020` "Android and iOS automatic stream-lifecycle conf[ormance]".

**The concrete APIs.** Android wraps every stream in a `callbackFlow`, iOS in an `AsyncThrowingStream`:

| SDK | API | Path | Shape |
| --- | --- | --- | --- |
| Android | `Conversations.stream(type, onClose)` | `Conversations.kt:582` | `Flow<Conversation>` |
| Android | `Conversations.streamAllMessages(type, consentStates, onClose)` | `Conversations.kt:640` | `Flow<DecodedMessage>` |
| Android | `Conversations.streamMessageDeletions(onClose)` | `Conversations.kt:692` | `Flow` |
| iOS | `Conversations.stream(type:onClose:)` | `Conversations.swift:376` | `AsyncThrowingStream<Conversation, Error>` |
| iOS | `Conversations.streamAllMessages(type:consentStates:onClose:)` | `Conversations.swift:694` | `AsyncThrowingStream<DecodedMessage, Error>` |
| iOS | `Conversations.streamMessageDeletions(onClose:)` | `Conversations.swift:753` | `AsyncThrowingStream` |

**Neither platform surfaces FFI item errors to the consumer.** The FFI distinguishes `on_error` from `on_close` for four of its five callback traits (§6.1), but both mobile SDKs swallow the error at the bridge:

- Android logs and continues: `override fun onError(error: FfiException) { Log.e("XMTP Conversation stream", error.toString()) }` (`Conversations.kt:610-612`), and the same for the aggregate stream (`:653-655`). Nothing calls `close(error)` or emits a failure into the Flow.
- iOS prints: `func onError(error: FfiError) { print("Error ConversationStreamCallback \(error)") }` (`Conversations.swift:74-76`), and `print("Error MessageDeletionCallback \(error)")` (`:99-101`), plus a per-item `print("Error processing conversation type: \(error)")` (`:412`). `onClose` handlers call plain `continuation.finish()` (`:414-417, :724-727`); the file contains **no** `finish(throwing:)`, so the `Throwing` in `AsyncThrowingStream` is vestigial and a consumer cannot tell a clean end from a failure.
- `streamMessageDeletions` has no `onError`/`onClose` on either platform, because the FFI trait offers neither (§6.1). Android's `onClose?.invoke()` fires only from `awaitClose`, i.e. on consumer cancellation.

Requirement 3 is scoped accordingly: error/close are distinct **in the FFI and in JS**, not in the current Android and iOS surfaces.

### 6.6 App-visible semantics that must be preserved

Consolidated from the above; these are the contract, independent of wire design.

1. Start a stream, then create/receive a conversation: it appears (SHARED-GROUP-REQ-028). **One documented exception on the legacy path:** a locally created group broadcast during a watchdog re-subscribe window can be missed, because the recreated `LocalEvents` receiver has no replay (`mod.rs:493-506`; §5.8). Network welcomes replay from the persisted cursor and have no such window.
2. A **newly welcomed or newly created** conversation joins an already-running aggregate message stream with no app action (SHARED-GROUP-REQ-030, MLS-REQ-089).
3. Messages arrive **in order** where multiple messages are asserted, and **without duplicates** (SHARED-GROUP-REQ-029/030, MLS-REQ-089).
4. Conversation and aggregate-message streams can run **concurrently**, each getting its matching events (SHARED-GROUP-REQ-031).
5. Filters are honored: conversation type (Group/DM/All) and consent state (Allowed/Denied/Unknown).
6. Internal **sync groups are hidden** from app streams (SHARED-GROUP-REQ-028, MLS-REQ-110).
7. A stream **closes cleanly** on request; `end()` is terminal and suppresses further callbacks.
8. Error vs close are **distinct** callbacks in the FFI and in JS (`on_error` vs `on_close`; `onError` vs `onFail`/`onEnd`) — but **not in the Android and iOS SDKs today**, which log or print FFI item errors instead of surfacing them, and the deletion callback has neither method (§6.1, §6.5).
9. A closed-and-reopened stream **replays what was missed but not what is already in local storage** (MLS-REQ-092). "Consumed" is not tracked: the dedup seen-set is seeded from the database, so a message stored by the pipeline but never delivered to the app is suppressed on reopen (§5.4).
10. A dead transport is **detected** and surfaces as a close, not a hang (BIND-REQ-059, MLS-REQ-023) — with different mechanisms per path: the bidi actor's silence watchdog is automatic and native-only, while the legacy idle watchdog is **opt-in and off by default**, and its runner reconnects only after a stale trip, never after a clean end (§5.8).
11. Automatic retry with configurable attempts/delay and observable `onRetry`/`onRestart`/`onFail` (JSDK-REQ-013/014/015/020).
12. A removed member's stream **stays open** and resumes after re-add (BIND-REQ-056).
13. Streams stay usable through metadata updates and epoch changes (BIND-REQ-058, IOS-REQ-118).
14. Consent/preference/deletion streams are local and do not fail from network causes. They are **not** lossless or infallible: lagged broadcast events are logged and dropped, and deletion decoding can fail (discarded on mobile) — §5.7.
15. Mobile `suspend_streams`/`resume_streams` keep leases and wire positions and replay the missed payload, once, in order (BIND-REQ-060) — **only on a native bidi transport**; both are no-ops otherwise, and `resume` is fire-and-forget, so its return is not "synced" (§6.1). Android and iOS drive both automatically via `manageStreamLifecycle` (default `true`).
16. `catchUpToLive` is bounded, idempotent, and converges (BIND-REQ-061). It is the awaitable "I am current now", unlike `resume_streams`.

---

## 7. Test coverage

### 7.1 Rust test files

| Path | Lines | Scope |
| --- | --- | --- |
| `crates/xmtp_mls/src/subscriptions/bidi_tests.rs` | 447 | live v3 bidi integration |
| `crates/xmtp_mls/src/subscriptions/d14n_bidi_tests.rs` | 404 | live d14n bidi integration |
| `crates/xmtp_mls/src/subscriptions/bidi_fuzz_tests.rs` | 1378 | randomized delivery fuzz over a live node |
| `crates/xmtp_mls/src/subscriptions/stream_router_tests.rs` | 200 | router integration |
| `crates/xmtp_mls/src/subscriptions/router_callbacks_tests.rs` | 861 | dispatch, gate, latch, callbacks |
| `crates/xmtp_mls/src/subscriptions/stream_all/tests.rs` | 1098 | aggregate stream behavior |
| `crates/xmtp_api_d14n/src/queries/bidi_transport_props.rs` | 751 | property tests over the ledger |
| `bindings/mobile/src/mls/tests/streaming.rs` | — | FFI streaming |
| `bindings/mobile/src/mls/tests/lifecycle.rs` | — | suspend/resume (sets `XMTP_BIDI_STREAMS_ENABLED=1` at lines 46, 120, 187) |
| `sdks/js/{node,browser}-sdk/test/streams.test.ts` | — | SDK retry/lifecycle |

Gating (`mod.rs:20-55`): bidi tests are `#[cfg(all(test, not(target_arch = "wasm32"), not(feature = "d14n")))]`; d14n bidi tests swap the last clause. So the two backends' bidi suites never run together.

### 7.1a Gating map

| Suite | File | Gate | Backend |
| --- | --- | --- | --- |
| XIP-83 wire (live) | `bidi_tests.rs` | `cfg(all(test, not(wasm32), not(feature="d14n")))` (`mod.rs:22`) | **v3-only, native-only** |
| XIP-83 wire (live) | `d14n_bidi_tests.rs` | `cfg(all(test, not(wasm32), feature="d14n"))` (`mod.rs:24`) | **d14n-only, native-only** |
| Randomized live fuzz | `bidi_fuzz_tests.rs` | v3-only gate (`mod.rs:27`) | v3-only, native |
| Router (live) | `stream_router_tests.rs` | v3-only gate (`mod.rs:45`) | v3-only, native |
| Router dedup (unit) | `stream_router.rs` `mod tests` (line 1586) | native | native |
| Callback adapters (live) | `router_callbacks_tests.rs` | v3-only gate (`mod.rs:53`) | v3-only, native |
| Bounded catch-up | `catch_up.rs` `mod tests` (663), `mod plan_tests` (803, ungated) | v3-only for live; planner pure | |
| Legacy stream_all | `stream_all/tests.rs` | per-test `cfg_attr` ignores | both |
| Transport ledger (mock) | `bidi_transport.rs` `mod tests` (2859) | native | `V3Binding` |
| Transport property model | `bidi_transport_props.rs` | native, proptest | `V3Binding` |
| Connection actor | `v3/connection.rs` (169), `bidi.rs` (856), `d14n/connection.rs` (211) | native | v3 / shared / d14n |

**Every XIP-83 bidi test is wasm-excluded by construction.** Wasm streaming coverage is limited to the legacy paths plus `bindings/wasm/test/*.test.ts` and `sdks/js/browser-sdk/test/*`.

**`crates/xmtp_mls/tests/` contains only `assets/`** — there are no Rust integration test targets; all streaming tests are in-crate `#[cfg(test)]` modules.

### 7.2 Behaviors asserted, by category

#### Ordering / no-duplicates / exactly-once

| `path:test` | Asserted |
| --- | --- |
| `bidi_tests.rs:bidi_catch_up_precedes_live_marker_then_streams_live` | Every catch-up cursor is new (`"duplicate cursor {id} in catch-up"`); every live cursor `> catchup_max` and new (`"cursor {id} delivered twice (catch-up/live overlap or dup)"`); `assert_eq!(app_count, TOTAL_APP)` for 5 history + 3 concurrent + 4 live. MLS-REQ-097 |
| `d14n_bidi_tests.rs:d14n_bidi_catch_up_precedes_live_marker_then_streams_live` | Same, keyed on the **full vector cursor** `(sequence_id, originator_id)`; asserts phase + counts rather than one global order. MLS-REQ-097 |
| `bidi_fuzz_tests.rs:fuzz_server_honors_the_bidi_wave_contract` | Per-(wave, topic) replay cursor-ordered; live lane per-topic ordered; a live frame at-or-below the cross-lane high-water is a leak. MLS-REQ-102 |
| `bidi_fuzz_tests.rs:fuzz_transport_delivery_never_loses_above_the_floor` | One sweep proves order + exactly-once + floor: `assert!(*id > last)` from `last = floor`. MLS-REQ-103 |
| `bidi_transport_props.rs:ledger_delivers_exactly_the_asked_suffix_in_order` | Proptest (4-35 ops, 3 topics, 32 cases): strictly increasing per topic from the floor; an undropped lease holds **exactly** the log suffix `> floor`; exactly one `CatchUpComplete` per lease; frames only for asked topics. Also asserts the *client* never sends an untagged Mutate and never reuses a wave id on one connection. API-REQ-078 |
| `bidi_transport_props.rs:chunked_ledger_delivers_exactly_the_asked_suffix_in_order` | Same with `MAX_MUTATE_TOPICS` shrunk to 1-2: **exactly one** `CatchUpComplete` however many chunks carried the replay. API-REQ-078 |
| `stream_all/tests.rs:test_stream_all_messages_does_not_lose_messages` | 45 messages under three concurrent adversaries: `assert!(duplicates.is_empty())` and `assert_eq!(messages.len(), 45)`. **Ignored on d14n and wasm.** MLS-REQ-089 |
| `stream_all/tests.rs:test_stream_all_concurrent_writes` | 100 concurrent messages, 2 shared + 20 spam groups, 4 clients; set-equality of sent vs received; each shared group stores exactly 41. **wasm-ignored.** MLS-REQ-089 |

Ledger unit tests in `bidi_transport.rs` covering the delivery rules: `deliveries_demux_by_topic`, `a_siblings_replay_fills_the_gap_without_repeating_history`, `tagged_replay_below_last_seen_is_the_owners_alone`, `covered_live_frame_is_dropped`, `rotation_ordered_replay_delivers_every_topic`, `shared_topic_fans_out_to_every_lease`, `replay_does_not_repeat_the_pre_mutate_live_window`, `reissued_replay_skips_frames_the_owner_saw_live`, `overlapping_waves_on_a_virgin_topic_lose_nothing`, `reissue_clamps_per_kind_progress_to_each_topics_own_position`, `markers_route_to_their_owners`.

Router dedup, all MLS-REQ-106 (`stream_router.rs mod tests`): `window_dedups_by_stored_identity_only`, `windows_close_per_topic`, `surfaced_ahead_outlives_the_window`, `growth_lease_folds_stored_identities_into_open_windows`.

#### Add-group-to-active-stream

| `path:test` | Asserted |
| --- | --- |
| `router_callbacks_tests.rs:welcomed_group_joins_the_live_stream` | **The headline test.** A conversation joined *after* subscribing reaches the live stream with no re-subscribe. The message is sent *before* the reflex could have leased, so delivery also proves the cursored add replays it — catch-up == subscribe. MLS-REQ-107 |
| `router_callbacks_tests.rs:self_created_group_streams_its_messages` | A self-created group streams — no welcome ever arrives, so this proves the `LocalEvents::NewGroup` fan-in leased its topic. MLS-REQ-107 |
| `router_callbacks_tests.rs:self_created_conversation_surfaces_on_the_stream` | The creator sees its own conversation on the bidi conversations stream. MLS-REQ-107 |
| `stream_all/tests.rs:test_stream_all_messages_changing_group_list` | Legacy path: a mid-stream new group and a third-party DM both start delivering. **wasm-ignored.** |
| `stream_all/tests.rs:test_new_group_does_not_duplicate_messages` | 50 groups + 1 new; asserts only `new_stats.len() < 5`. **Weak — flagged in the catalogue** as not inspecting identities or requiring exactly one. |
| `bindings/mobile/src/mls/tests/streaming.rs:test_stream_all_messages_with_optimistic_group_creation` | Stream started **before any group exists**; two optimistic groups + 3 texts all arrive, stream not killed. BIND-REQ-058 |
| `bidi_fuzz_tests.rs:fuzz_transport_delivery_never_loses_above_the_floor` | Op 18 creates real groups **mid-run**, adding zero-history topics born after the wires opened; a final welcome-sentinel group must reach every consumer's live edge. |

#### Welcome / conversation streams

`bidi_tests.rs:bidi_connection_delivers_live_welcome_over_the_wire` and its d14n twin (MLS-REQ-096) prove `Started` → live welcome frame → `probe()` succeeds. `stream_router_tests.rs:sibling_conversation_streams_both_receive_a_welcome` (MLS-REQ-105) proves per-stream dedup state means one stream's delivery never suppresses another's. `mod.rs:tests::test_process_streamed_welcome_message_{v3,d14n}` (MLS-REQ-114). `stream_conversations.rs mod test`: `stream_welcomes`, `test_sync_groups_are_not_streamed`, `test_dm_stream_filter`, `test_self_group_creation`, `test_add_remove_re_add` (MLS-REQ-086), `test_duplicate_dm_not_streamed` (MLS-REQ-087).

#### Reconnect / wire flap / suspend / resume

The `bidi_transport.rs` ledger suite is the deepest coverage in the repo — 25+ tests including `wire_death_reconnects_from_last_seen_positions`, `half_open_wire_is_reaped_and_reconnected`, `suspend_half_closes_and_resume_completes_at_catch_up`, `concurrent_resumes_join_one_catch_up_wave`, `suspended_transport_stays_off_the_network`, `a_born_suspended_transport_parks_the_first_lease`, `suspend_preempts_a_stuck_dial`, `a_resume_burst_during_an_outage_dials_once`, `interrupted_catch_up_is_re_owed_by_a_reissued_wave`, `interrupted_wave_reissues_past_its_progress_on_reconnect`, `shared_topic_reconnect_replays_a_caught_up_siblings_outage_gap`, `reconnect_folds_a_caught_up_holders_floor_when_nothing_was_delivered`, `a_virgin_topics_interrupted_catch_up_resumes_from_its_floor`, `an_over_cap_lease_still_catching_up_survives_a_wire_death`.

Live fault injection: `bidi_fuzz_tests.rs:fuzz_transport_delivery_never_loses_above_the_floor` runs under toxiproxy with per-consumer wires; op 15 disables a proxy (or, 1-in-4, **all at once** — a correlated outage) for 150-600 ms or 1200-2400 ms; ops 16-17 toggle suspend/resume. MLS-REQ-103.

Legacy: `client.rs:tests::should_reconnect` (MLS-REQ-023) black-holes a stream with a 60 s downstream toxic, asserts the stream is done, then a **new** stream receives later conversations. `stream_all/tests.rs:watchdog_reconnect_keeps_stream_alive` and `watchdog_trips_on_idle_real_stream` (MLS-REQ-095). `watchdog.rs mod tests` (MLS-REQ-093/094) including a proptest `yields_every_item_then_exactly_one_stale`.

Lifecycle: `router_callbacks_tests.rs:suspend_resume_replays_what_was_missed` runs **two full cycles** so the second proves resume positions carry over. `suspend_before_the_first_stream_parks_the_wire` and `lifecycle_helpers_are_noops_without_a_transport` (MLS-REQ-109). FFI: `bindings/mobile/src/mls/tests/lifecycle.rs:bidi_suspend_and_resume_redelivers` asserts resume yields exactly `["before","during"]` — "resume must redeliver exactly the withheld message, and only it, in order". **d14n-ignored** because the d14n gateway client does not yet expose the v3 bidi RPC. BIND-REQ-060.

#### Catch-up correctness

`stream_router_tests.rs:router_catches_up_from_durable_cursor` (history replayed in order from the durable cursor) and `resubscribe_does_not_redeliver` (the server replays an already-delivered message but the fresh stream must skip it — pinning the `messages_newer_than` seeding). MLS-REQ-104.

`catch_up.rs:tests::{catch_up_joins_pending_groups_and_stores_history, catch_up_replays_the_missed_tail_idempotently, catch_up_with_nothing_owed_completes, legacy_catch_up_counts_the_same_way}` (SHARED-GROUP-REQ-039). `plan_tests::plan_splits_a_large_subscription_set_into_bounded_waves` — 5000 topics, ids contiguous from 1, every wave add-only + `history_only`, **every topic carried exactly once** (MLS-REQ-101).

`stream_all/tests.rs:test_stream_all_messages_respects_cursor_between_streams` (MLS-REQ-092) and `stream_messages_keeps_track_of_cursor` — a brand-new installation must set the cursor to the group's latest before any undecryptable message, asserting `s.next()` **times out**.

FFI: `lifecycle.rs:bidi_catch_up_to_live_replays_and_is_idempotent` checks the **real payload** on disk, not just a counter; `bidi_catch_up_to_live_bounded_run_is_cancel_safe` uses a 1 ms deadline that drops the future mid-processing and requires a later full run to converge all five owed messages in order. BIND-REQ-061.

Completeness bounds in the fuzz: per connection, **everything above the lowest cursor it ever asked for** must have been served; and across a recovery chain's links the union must cover everything above the chain's *original* floors — the durable-cursor recovery contract.

Deferred-completion subtleties (found by the live fuzz): `fully_yanked_wave_defers_catch_up_until_the_claiming_wave_resolves`, `yank_defers_even_when_the_wire_position_trails_the_floor`, `yank_chains_re_park_deferred_completions_hop_by_hop`, `a_dropped_owners_in_flight_wave_still_serves_the_leases_it_holds`.

#### Backpressure / slow consumer / dropped lease

`bidi_transport.rs`: `slow_lease_is_dropped_without_blocking_siblings`, `a_withheld_frame_overflow_drops_the_lease_for_recovery` (trips `WITHHELD_FRAMES_CAP` rather than growing memory), `flush_replays_withheld_frames_per_kind_in_arrival_order`, `an_alternating_withheld_window_flushes_within_the_channel_bound`, `deref_is_refcounted_and_last_lease_closes_the_wire`, `deref_purges_only_the_dropped_leases_unsent_waves`.

`v3/connection.rs` (API-REQ-059): `busy_wire_does_not_stall_inbound` — with the old `send().await`-in-`select!` design a full wire parked the actor and an auto-pong behind the wedge was lost; `gives_up_when_wire_wedged_past_backlog_cap`; `finish_is_processed_under_wire_backpressure`; `try_mutate_reports_full_and_recovers_after_drain`. `bidi.rs:consumer_backpressure_is_not_wire_silence` (API-REQ-057) — a slow consumer must never get a healthy link reaped.

#### Error handling / close / abort

`stream_router_tests.rs:a_panicked_welcome_task_surfaces_instead_of_parking` (MLS-REQ-115). `router_callbacks_tests.rs:only_a_backend_refusal_latches` (MLS-REQ-111) is the classifier table: **latch-worthy** = gRPC `UNIMPLEMENTED` buried under the v3 client's blanket-retryable wrapping (the test first asserts `unimplemented.is_retryable()` — "the trap the classifier must not fall into"), the in-process stub refusal, and a tombstoned transport (`TransportError::Closed`); **not latch-worthy** = a transient dial failure, a garbled-handshake `DecodeError`, and `RouterError::Closed`. Plus `pump_latches_and_serves_the_fallback_on_a_grpc_refusal`, `pump_serves_the_fallback_without_latching_on_a_dead_end`, `latched_dispatch_delivers_via_legacy`, `destinations_latch_independently`, `a_resume_time_refusal_latches_at_the_next_lifecycle_fold` (MLS-REQ-113), `stream_all_with_no_conversations_stays_open`.

`v3/connection.rs`: `inbound_error_closes_the_connection`, `closing_inbound_tears_down_sends`, `mutate_and_probe_report_closed_after_finish`, `finish_resolves_in_flight_probe_to_closed`. `bidi.rs`: `drain_after_finish_flushes_pending_before_closing`, `drain_after_finish_bounds_the_flush_on_a_wedged_wire` (API-REQ-056).

`d14n/connection.rs` (API-REQ-048): `bad_payload_is_skipped_without_dropping_the_batch`, `malformed_envelope_is_skipped_without_dropping_the_batch`, `skips_unknown_response_version`, `parses_topics_live_and_skips_malformed` — one bad envelope is skipped **alone**; a batch-level short-circuit would lose the valid welcome beside it.

JS: `sdks/js/{node,browser}-sdk/test/streams.test.ts` `describe("createStream lifecycle")` — 15 tests, the most thorough abort/close coverage in the repo (`does not restart after end() during a pending retry`, `suppresses onValue and onError after end()`, `allows only one retry in flight per stream`, `stops retrying after the retry budget is exhausted`, `counts failed restart attempts against the retry budget`, `ends the stream even when onError throws at terminal failure`, `reschedules when the native stream closes during restart creation`, `does not create a native stream when onRetry ends the stream`). Plus `AsyncStream.test.ts` — 21 tests each for FIFO and terminal cleanup (JSDK-REQ-001/002/006).

`bindings/node/test/Conversations.test.ts:should error when connection dies` (BIND-REQ-059) — toxic proxy, keepalive 10/10 s, the close callback must fire.

#### XIP-83 protocol conformance

**Always-ack:** `bidi_fuzz_tests.rs:fuzz_server_honors_the_bidi_wave_contract` requires every wave to resolve within 30 s (`"waves never acked"`), panics on an ack for an unknown wave and on a double-ack. Removes-only waves are acked too.

**Untagged-backend refusal:** `bidi_transport.rs:an_untagged_backends_completion_tombstones_the_transport` — a pre-tags backend echoes `mutate_id == 0` (proto3 default; client waves mint from 1), so the first such frame is a **capability verdict**: the transport shuts down rather than half-serving a wire whose replays the live-order shield would silently drop. Guarded in the false-positive direction by `a_retires_removes_wave_is_acked_without_tombstoning` — the retire's removes-only wave must carry a minted id, never 0, or a healthy transport is tombstoned. That test was "found by the live fuzz — the scripted suite never acked removes."

**Chunking** (API-REQ-061): `a_lease_over_the_cap_splits_into_bounded_frames`, `a_lease_over_the_byte_budget_splits_into_bounded_frames`, `a_mass_unsubscribe_chunks_the_removes_wave`, plus the reconnect/suspend variants — **a lease is not caught up until its LAST chunk completes**.

**Ping/Pong:** `v3/connection.rs:auto_pongs_server_ping_without_surfacing_it` (API-REQ-058), `probe_round_trips_and_pong_is_not_an_event`, `probe_within_times_out_when_no_pong`, `default_probe_timeout_tracks_server_keepalive`. `bidi.rs:watchdog_probes_then_tears_down_a_silent_wire` (**exactly one** watchdog ping then teardown), `an_answered_watchdog_probe_keeps_the_wire_alive`, `inbound_activity_resets_the_watchdog`, `probe_nonces_never_mint_the_watchdog_nonce` (API-REQ-056/057).

**history_only + half-close:** `bidi_tests.rs:bidi_history_only_catches_up_then_delivers_nothing_live` (MLS-REQ-098) and `bidi_history_only_half_close_drains_then_server_closes` (MLS-REQ-099, then `mutate` must be `Err(BidiError::Closed)`). Note the d14n twin is **stricter**: `Ok(None)` is a **panic** — "stream ended without a half-close: history_only must stay open" — because XIP-83 server req 9 closes only after the *client* half-closes, and tolerating an ended stream would let a dead connection pass the negative check vacuously.

**Wire codec:** `v3/bidi.rs:encodes_outbound_and_decodes_inbound` and `tags_open_error_with_subscribe_endpoint` (API-REQ-055). `v3/connection.rs:preserves_wire_order_of_history_markers_and_live` — a wave's replay carries the initial Mutate's id, the post-seam live frame carries `0`.

#### Consent and preference streams

`client.rs:tests::should_stream_consent` (SHARED-SYNC-REQ-008), `bindings/mobile/.../streaming.rs:test_stream_consent` (**d14n-ignored**) and `test_stream_preferences` (BIND-REQ-100), `stream_all/tests.rs:test_stream_all_messages_filters_by_consent_state` (rstest over Allowed/Denied/Unknown, **wasm-ignored**), `sdks/js/*/test/Preferences.test.ts:should stream preferences` (four ordered updates, JSDK-REQ-122), Android `HistorySyncTest.testStreamConsent`/`testStreamPreferenceUpdates` (ANDROID-REQ-105).

**iOS coverage hole:** `sdks/ios/Tests/XMTPTests/HistorySyncTests.swift:testStreamConsent` and `testStreamPrivatePreferences` **always `XCTSkip` before setup — zero active runtime coverage** (IOS-REQ-136/137).

Adjacent: `router_callbacks_tests.rs:sync_group_messages_are_intercepted_not_delivered` (MLS-REQ-110) sends the sync-group message **first** so a leak would arrive ahead of the normal message.

#### Membership transitions

`bindings/mobile/src/mls/tests/streaming.rs:test_message_streaming_when_removed_then_added` (BIND-REQ-056) is the one membership-transition stream test: bola's stream stays open but receives **nothing** while removed (count frozen at 3 across removal, a post-removal send, and the re-add transcript), then resumes at 4 after re-add; the creator's stream keeps counting to 6 throughout; both close cleanly.

#### Known-weak assertions (flagged by the catalogue itself)

| Test | Weakness |
| --- | --- |
| `stream_conversations.rs:test::test_many_concurrent_dm_invites` (MLS-REQ-088) | Discards all task handles and all stream `Option`/`Result`/values — asserts essentially nothing |
| `stream_all/tests.rs:test_new_group_does_not_duplicate_messages` (MLS-REQ-089) | Only `new_stats.len() < 5`; no identity or exact-count check |
| `stream_all/tests.rs:watchdog_reconnect_keeps_stream_alive` (MLS-REQ-095) | Does not assert the first message is not replayed |
| `stream_messages.rs:test::test_stream_messages` (SHARED-SYNC-REQ-005) | Filters Application-kind caller-side, so it does not prove the raw stream filters protocol messages |
| iOS `testStreamConsent` / `testStreamPrivatePreferences` | Always skip — zero runtime coverage |
| `bindings/mobile/.../streaming.rs:test_can_stream_group_messages_for_updates` | Unconditionally `#[ignore]`d |

#### Deliberate non-assertions in the fuzz

Documented at `bidi_fuzz_tests.rs:45-57`: silence after a remove's ack (in-flight frames are expected; the client drops unheld-topic frames at demux); cross-topic per-kind cursor order within one wave; **`TopicsLive` content and position** ("informational to this client"); `history_only` Mutates on the shared fuzz connection. Fuzz seeds replay the operation *schedule* only — wire timing varies, so a failure is real but may take several replays to re-trigger (`XMTP_BIDI_FUZZ_SEED`, `XMTP_BIDI_FUZZ_ROUNDS` default 60/48, `PROPTEST_CASES` default 32).

#### d14n XIP-83 coverage is much thinner than v3

`bidi_fuzz_tests.rs`, `stream_router_tests.rs`, `router_callbacks_tests.rs`, and `catch_up.rs`'s live tests are **all** `not(feature = "d14n")`. d14n's entire XIP-83 coverage is the four wire tests in `d14n_bidi_tests.rs` plus eight frame-classification unit tests in `d14n/connection.rs`. The FFI suspend/resume test is d14n-ignored.

**The gap is structural, not an oversight.** Those suites cannot run under d14n, because the router is monomorphized on `BidiTransport<V3Binding>` (`stream_router.rs:387-390`), `V3Binding` is the only `TransportBinding` (`v3/connection.rs:97`), and the d14n client refuses `subscribe_bidi` outright (`d14n/streams.rs:140-156`). d14n's tests exercise the *wire binding* it does have, and its app streams run on the legacy path. So this is not "d14n's XIP-83 path is under-tested" — d14n has no XIP-83 app path to test. If d14n is being retired this does not matter; if any of its behavior is meant to carry forward, the missing piece is a `TransportBinding` impl, not more tests.

---

## 8. v3-only vs v4/d14n-only complexity

### 8.1 Exists only because of v3

| Complexity | Where | Why it exists |
| --- | --- | --- |
| Separate group and welcome subscriptions | `XmtpMlsStreams` (3 methods), `endpoints/v3/mls/subscribe_{group_messages,welcome_messages}.rs` | v3 has no unified topic subscribe |
| Teardown-and-resubscribe on group add | `stream_messages.rs:subscribe` | no in-place mutation |
| Client-side idle watchdog | `watchdog.rs` (762 lines) + 4 env vars | "no server-side keepalive on the v3 path today" |
| Long idle timeout with periodic healthy reconnects | `watchdog.rs` DEFAULT_IDLE_TIMEOUT 300 s | cannot distinguish dormant from dead |
| Scalar `id_cursor` per group | `SubscribeFilter { group_id, id_cursor }` | v3 cursor model |
| `stream_conversations` merging local broadcast + network welcomes with a re-subscription gap | `stream_conversations.rs`, `mod.rs:495-503` | no replay on the local channel |

### 8.2 Exists only because of v4/d14n

| Complexity | Where | Why it exists |
| --- | --- | --- |
| XIP-49 cross-originator causal ordering | `protocol/order.rs`, `queries/stream/ordered.rs` | multiple originators |
| `depends_on` dependency resolution + icebox | `protocol/order.rs:required_dependencies`, `protocol/resolve.rs` | out-of-order across originators |
| Vector-clock cursors / cursor maps | `protocol/types.rs` `TopicCursor`, `in_memory_cursor_store.rs`, `queries/combinators/ordered_query.rs` | per-originator positions |
| `SubscribeTopics` replacing `SubscribeEnvelopes` | `endpoints/d14n/subscribe_topics.rs` | single shared cursor was insufficient |
| Status-aware stream (Started/CatchupComplete as in-band envelope status) | `queries/stream/status_aware.rs` | v4 status frames |
| d14n envelope extraction inside the bidi binding | `queries/d14n/connection.rs`, `BidiBinding::handle` | `OriginatorEnvelope` unwrapping |
| Two `Mutate` cursor shapes in the generic transport | `bidi.rs` `BidiBinding::Mutate` ("v3: single `id_cursor`; d14n: vector cursor") | supporting both backends at once |
| `payer` | publish path only (`endpoints/d14n/publish_client_envelopes.rs`) | out of scope for streaming |

**A single-originator self-hosted backend deletes the whole of 8.2.** The draft `backend.proto` already reflects this: `Cursor { uint64 sequence_id }` is scalar, and there is no `depends_on` or originator dimension.

### 8.3 Exists because both must be supported at once

The genericity itself — `BidiBinding`, two `connection.rs` files, two generated proto trees, feature-gated test suites, `d14n_compat.rs`, `V3OrD14n` — exists only to serve two backends simultaneously. One backend collapses this.

---

## 9. Streaming requirements the new backend and simplified client must meet

EARS-style. Each cites its source (SDK API, test/requirement, or client code).

### 9.1 Subscription lifecycle

1. **When** an app opens a stream, the client **shall** deliver a readiness barrier the caller can await. *(Source: `FfiStreamCloser::wait_for_ready` `bindings/mobile/src/mls.rs:3973`; awaited as `streamCloser.waitForReady()` in `sdks/js/node-sdk/src/utils/streams.ts:313-320`.)* **Today it is weaker than "the subscription is registered":** it returns `()` and discards a dropped sender (`crates/xmtp_common/src/stream_handles.rs:104-108, 234-238`), so it cannot report failure; and the browser SDK does not call it at all — its worker acks after constructing the stream object, not after registering the subscription (`sdks/js/browser-sdk/src/utils/streams.ts:314-319`; `workers/client.ts:569-583`). The new design **should** make readiness a real, failable signal and make node and browser agree.
2. **When** an app calls `end()` on a stream handle, the client **shall** stop all delivery and invoke no further callbacks. *(Source: `createStream` "Ending the stream is terminal"; `router_callbacks.rs` "An `end()`ed handle aborts the task without `on_close`".)*
3. **While** a stream is open, the client **shall** distinguish an error (recoverable, stream continues or retries) from a close (terminal for that native stream). *(Source: `FfiMessageCallback::{on_error,on_close}` `mls.rs:3982-3985`; `onError` vs `onFail`/`onEnd` in `StreamOptions`.)* **Not met by the mobile SDKs today:** Android logs FFI errors (`Conversations.kt:610-612, 653-655`) and iOS prints them (`Conversations.swift:74-76, 99-101`) rather than surfacing them, and `FfiMessageDeletionCallback` has neither method (`mls.rs:4010-4012`). Closing that gap is SDK work, not wire work.
4. **When** the last consumer of a topic goes away, the backend **shall** allow the client to stop delivery for that topic without closing the stream. *(Source: refcounted removes, `bidi_transport.rs` "Removes are refcounted".)*

### 9.2 In-place mutation (the core simplification)

5. **When** an app is welcomed into a new conversation while an aggregate message stream is open, the client **shall** begin delivering that conversation's messages without reopening the stream. *(Source: MLS-REQ-089; `stream_all.rs:poll_next` `this.messages.as_mut().add(...)`; today the legacy path reopens — `stream_messages.rs:subscribe`.)*
6. **When** the client adds topics to an open subscription, the backend **shall** apply the change atomically per frame and acknowledge it. *(Source: `SubscribeRequest.Mutate` + `CatchupComplete`; MLS-REQ-102 "every mutate is acknowledged".)*
7. **When** an add-set exceeds the frame budget, the client **shall** split it into bounded frames, each carrying each topic once. *(Source: `chunk_mutate_adds` `bidi_transport.rs:247`; MLS-REQ-101, 5000 topics.)*
8. The client **shall** send at most 1000 topics and at most an estimated 16 MiB of topic payload per `Mutate` frame, so the backend's per-frame limits **shall** be at least that or the spec **shall** state a smaller limit the client can honor. *(Source: `MAX_MUTATE_TOPICS` `bidi_transport.rs:189`, `MAX_MUTATE_BYTES` `:197`, applied by `chunk_waves` `:1027-1032`.)* These are **client chunk caps**, not evidence of a server minimum — the docs describe splitting the client's own frames, and 16 MiB is chosen as headroom "well under the transport's 25 MiB encode limit". Server-side limits are unspecified today; see Q4.

### 9.3 Catch-up and delivery correctness

9. **When** a topic is added with a cursor, the backend **shall** replay that topic's history above the cursor before delivering its live traffic. *(Source: MLS-REQ-097 "Pre-open history is delivered before TopicsLive".)*
10. **While** a catch-up wave is in flight for a topic, the backend **shall not** deliver live frames for that topic until the wave's `CatchupComplete`. *(Source: `bidi_transport.rs` delivery guarantee; MLS-REQ-102 "wave-owned live delivery waits for completion".)*
11. Every delivery frame **shall** carry the `mutate_id` of the wave that produced it, with `0` reserved for live delivery. *(Source: generated proto comments; `bidi_transport.rs` "requires a tag-serving backend".)*
12. **When** a wave completes, the backend **shall** send `CatchupComplete` echoing the wave's `mutate_id`. *(Source: MLS-REQ-097 "CatchUpComplete echoes the mutate ID".)*
13. The backend **shall** serve live frames and each wave's replay in **one total cursor order per kind, shared across every topic of that kind** — not merely per-topic monotonicity. *(Source: `bidi_transport.rs:24-30` "in total cursor order per kind"; `:77-88` names the assumption directly: "cross-topic replay, on the **shared per-kind cursor sequence**", whose absence would let a sibling's live frames "push a global mark past the gap and mis-filter exactly the frames this rule exists to route".)* If the new backend does not give each kind one total order across topics, the ledger's sibling-gap watermark logic must be redesigned. See Q2.
14. **Where** the backend meets requirements 10 through 13, the V3 transport ledger **shall** deliver each **cursor-bearing** message to each **lease** exactly once, strictly increasing above that lease's floor. *(Source: `bidi_transport.rs:24-30`; MLS-REQ-103; `bidi_transport_props.rs:ledger_delivers_exactly_the_asked_suffix_in_order`.)* This is a **conditional transport property, not an app-visible guarantee.** Its scope excludes: frames with no extractable cursor, which "fail open to every holder" (`bidi_transport.rs:73-74`); a *new* lease, which legitimately replays identities an earlier stream delivered (`stream_router.rs:667-675`); the d14n binding, which has no `TransportBinding`; SDK-level retries, which reopen streams; data outside the server's retention window; and what the application actually observed, which the client does not track (requirement 34). The application-facing property is **deduplication against local storage** (requirement 34), not exactly-once delivery.
15. **Where** a client requests history only, the backend **shall** deliver owed history and markers and **shall not** register those topics for live delivery. *(Source: MLS-REQ-098; `Mutate.history_only`.)*
16. **When** the client half-closes, the backend **shall** finish in-flight waves and then close. *(Source: MLS-REQ-099; draft proto behavior described in `GRACEFUL_CLOSE_BUDGET`.)*

### 9.4 Liveness

17. **While** a subscription is open, the backend **shall** advertise its keepalive cadence on the first frame. *(Source: `Started.keepalive_interval_ms`; `DEFAULT_KEEPALIVE_MS` is only a fallback.)*
18. **When** either peer sends `Ping`, the receiver **shall** answer with `Pong` echoing the nonce. *(Source: `bidi.rs` auto-answer; MLS-REQ-096 "answer a probe".)*
19. **If** no inbound frame arrives within the silence budget, the client **shall** tear the wire down rather than hang. *(Source: `bidi.rs` watchdog ping then teardown; BIND-REQ-059; MLS-REQ-023.)* On the **bidi** path this is automatic and native-only. On the **legacy** path the equivalent (`watchdog.rs`) is **opt-in and disabled by default** (`watchdog.rs:20-23, 56-59`), and its runner reconnects only after a stale trip — "Reconnect only on a watchdog stale-trip; a clean end or cancellation ends it" (`watchdog.rs:462-465`) — so with default settings a legacy close is terminal in the core layer and only a JS retry wrapper reopens it. The new design **should** make liveness automatic on every path.
20. Keepalive frames **shall not** reach the application. *(Source: `bidi.rs` "keepalive never reaches the consumer".)*

### 9.5 Reconnection and lifecycle

21. **When** the wire dies unexpectedly, the client **shall** reconnect with capped exponential backoff and **shall not** close the app's streams. *(Source: `bidi_transport.rs` "a wire flap is invisible to leases"; `RECONNECT_INITIAL_DELAY`/`RECONNECT_MAX_DELAY`.)*
22. **When** the wire is re-established, the client **shall** resume each topic from its last delivered position so no holder sees a repeat or a gap, **within the scope of requirement 14** (cursor-bearing frames, one lease, data still inside retention). *(Source: resume-wave design; MLS-REQ-103 "each recovery chain covers the complete owed suffix".)*
23. **Where** a native bidi transport is in use and an app backgrounds, the client **shall** park the wire while keeping subscriptions and wire positions. *(Source: `suspend_streams` `bindings/mobile/src/mls.rs:205`; `router_callbacks.rs:330-357`; BIND-REQ-060.)* It is a **no-op** when bidi is off or no stream was ever opened, so the legacy path has no lifecycle parking.
24. **Where** a native bidi transport is in use and an app foregrounds, the client **shall** reconnect and replay the missed payload, once, in order, **within the scope of requirement 14**. *(Source: `resume_streams` `mls.rs:222`; BIND-REQ-060; MLS-REQ-109.)* The FFI form is **fire-and-forget**: it enqueues the resume and returns before the replay arrives — "Do **not** treat its return as 'synced'" (`mls.rs:210-224`). A caller needing completion uses requirement 25.
25. **Where** an app needs a bounded, awaitable "I am current", the client **shall** provide a catch-up-then-stop operation. *(Source: `catch_up_to_live`, `catch_up.rs`; BIND-REQ-061.)*
26. The catch-up operation **shall** be idempotent and **shall** converge after a canceled run. *(Source: BIND-REQ-061.)*
27. **When** a catch-up joins a group whose history is then owed, the client **shall** extend the run to cover it. *(Source: `catch_up.rs` discovery loop.)*

### 9.6 Backpressure

28. **If** a consumer stops draining, the client **shall** drop that consumer alone and **shall not** stall the wire or sibling consumers. *(Source: one-policy backpressure, `bidi_transport.rs` and `stream_router.rs`.)*
29. **When** a consumer is dropped for backpressure, its recovery **shall** be to re-subscribe from durable cursors. *(Source: same.)*

### 9.7 Application semantics (preserve exactly)

30. **When** a conversation stream is open and a conversation is created or received, the client **shall** emit it. *(SHARED-GROUP-REQ-028.)* Network welcomes satisfy this without qualification, because they replay from the persisted cursor. **Locally created** groups arrive on a `LocalEvents` broadcast channel with no replay, so the legacy watchdog's re-subscribe leaves a bounded loss window — a `NewGroup` broadcast while the new subscription is being built is missed (`crates/xmtp_mls/src/subscriptions/mod.rs:493-506`; `stream_conversations.rs:83-91`). The new design **should** close that window rather than inherit it.
31. The client **shall** exclude internal sync groups from application streams. *(SHARED-GROUP-REQ-028; MLS-REQ-110; `stream_all.rs:poll_next`.)*
32. The client **shall** honor conversation-type and consent-state filters. *(SHARED-GROUP-REQ-030.)*
33. Conversation and message streams **shall** run concurrently, each receiving its matching events. *(SHARED-GROUP-REQ-031.)*
34. **When** a stream is closed and reopened, the client **shall** deliver messages missed while closed and **shall not** re-deliver messages already in local storage. *(MLS-REQ-092; MLS-REQ-104.)* The suppression key is **local persistence, not application consumption**: the seen-set is seeded from `get_last_cursor_for_ids` plus `db.messages_newer_than` (`stream_router.rs:697-716`), because "the streaming pipeline stores messages WITHOUT advancing the durable cursor ... Those exact identities seed the window's seen-set" (`:667-675`). A message the pipeline stored but never delivered to the app is therefore suppressed on reopen. If the new design wants a true "not what the app consumed" guarantee, it needs a delivery acknowledgement the client does not have today.
35. A removed member's stream **shall** stay open and resume delivery after re-add. *(BIND-REQ-056.)*
36. Consent, preference, and deletion streams **shall** remain local and **shall not** fail from network causes. *(`mod.rs:176-208`; node SDK doc comment.)* They are **not** lossless or infallible: a lagged broadcast receiver's events are logged and skipped and the stream continues (`optify!`, `mod.rs:180,189,198`), with no replay to recover them; and deletion decoding can fail (`DecodedMessage::try_from`, `mod.rs:206`), an error the mobile binding discards (`bindings/mobile/src/mls.rs:2139-2144`) because `FfiMessageDeletionCallback` has no `on_error` (`mls.rs:4010-4012`).
37. **If** a streamed message cannot be processed directly, the client **shall** run a recovery sync, and the **inline** stream path **shall not** advance the durable cursor or apply commits. *(MLS-REQ-079; `process_message/factory.rs:115`; `mls_sync.rs:2452-2458, 2525-2531`.)* The recovery sync itself runs the **query** path with `trust_message_order = true` (`factory.rs:142` → `mls_sync.rs:2845-2853`), so it **does** advance cursors and **does** apply the commit. The accurate contract: streams never apply commits or advance cursors themselves; a stream-triggered recovery applies them in query order. A commit delivered on a stream always errors inline with `EpochIncrementNotAllowed` (`mls_sync.rs:2454-2458`) and reaches the group only through that detour.
38. The JS SDKs **shall** retry a failed stream with configurable attempts and delay, report attempts via `onRetry`, restarts via `onRestart`, and terminal failure via `onError` + end. *(JSDK-REQ-013/014/015; `createStream`.)* **Node and browser do not currently agree**, so the spec must fix one contract: defaults differ (10/60 s vs 6/10 s); with retry disabled node calls terminal `onError` via `fail(new StreamFailedError(0))` (`node-sdk/src/utils/streams.ts:299-302`) while browser ends the stream and throws out of its close handler with no terminal `onError` (`browser-sdk/src/utils/streams.ts:302-305`); and browser has no `waitForReady` step at all (`browser-sdk/src/utils/streams.ts:314-319`).
39. The retry budget **shall** be monotonic across restarts, and at most one retry **shall** be in flight. *(JSDK-REQ-020; `createStream` `remainingRetries`, `retryInFlight`.)*

### 9.7a Protocol obligations discovered only through testing

These are requirements the current tests enforce that a naive reading of the proto would miss. Each was a real bug or near-miss.

44. The backend **shall** acknowledge **every** `Mutate`, including a removes-only or fully no-op one. *(MLS-REQ-102 "waves never acked" is a test failure; `bidi_transport_props.rs` models removes-only waves acking immediately.)*
45. An acknowledgement **shall** echo the client's supplied `mutate_id` exactly, and the client **shall** mint a nonzero id for every wave it sends, including a removes-only one. *(`crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs:809-816, 966-968`; `bidi_transport.rs:5623-5645`, test `a_retires_removes_wave_is_acked_without_tombstoning`.)* The protocol does **not** require the backend to invent a nonzero id: `Mutate.mutate_id` "MUST be nonzero **when adds are present** (0 is the live tag)", so a **waveless** Mutate may legitimately carry `0` and its ack then echoes `0` — "echoes the Mutate; 0 only if a waveless Mutate carried 0". The backend rule is *echo exactly*; the nonzero rule binds the client, and it is libxmtp's own choice to mint nonzero ids even for retire waves, which is what makes any `0` echo readable as a pre-tags backend (§4.2).
46. The backend **shall not** acknowledge a wave twice, and **shall not** acknowledge a wave the client never sent. *(MLS-REQ-102 panics on both.)*
47. **When** the client half-closes a `history_only` subscription, the backend **shall** keep the stream open until then, and **shall** close only after. *(`d14n_bidi_tests.rs` treats a premature `Ok(None)` as a panic: "stream ended without a half-close: history_only must stay open" — XIP-83 server req 9.)*
51. The client's **internal silence watchdog** **shall** accept *any* inbound frame as proof of life, while an **explicit** client probe **shall** resolve only on a `Pong` echoing its nonce. *(`bidi.rs:358-362` — a probe "Resolves `Ok` on the matching `Pong` ... or `ProbeTimedOut`"; `:641-647` matches by nonce and ignores unmatched pongs; `:673-690` — "any inbound frame — the pong included — resets the window above". Tests `an_answered_watchdog_probe_keeps_the_wire_alive`, `inbound_activity_resets_the_watchdog`.)* The two mechanisms are separate: a generic inbound frame quiets the watchdog but does **not** satisfy a pending `probe()`. So the backend obligation is to answer every `Ping` with a nonce-echoing `Pong` (requirement 18); traffic alone is not a substitute.
52. A slow *consumer* **shall not** be mistaken for a dead *wire*. *(`bidi.rs:consumer_backpressure_is_not_wire_silence` — the silence window restarts when the actor resumes listening.)*

### 9.7b Client decoder obligations (not backend requirements)

Requirements 48 through 50 were previously stated as backend rules. Their evidence is in the **client's connection bindings**, which decode and classify inbound frames; they constrain libxmtp, not the server. The corresponding *backend* obligation is only to emit well-formed frames and to keep batch ordering (requirement 13). Kept at their original numbers.

48. The client **shall** skip one malformed envelope in a batch without discarding the valid envelopes beside it. *(`crates/xmtp_api_d14n/src/queries/d14n/connection.rs:152-186` — one fallible extraction pass per envelope, `Err(e) => tracing::warn!("d14n bidi: skipping undecodable envelope: {e}"); continue`; API-REQ-048; tests `bad_payload_is_skipped_without_dropping_the_batch`, `malformed_envelope_is_skipped_without_dropping_the_batch`.)* The doc states the stake: "the consumer's cursors advance past a dropped batch, so anything discarded here is never re-fetched."
49. The client **shall** classify an empty envelope batch as a delivery frame and **shall** preserve its wave tag. *(`d14n/connection.rs:empty_envelope_batch_yields_empty_messages` — the transport routes on the tag, so an empty tagged frame must still reach the ledger. An unrecognized oneof arm is `Inbound::Skip` instead, `d14n/connection.rs:132-145`; `v3/connection.rs:82-92`.)*
50. The client **shall** preserve the received order of a batch across the catch-up seam: a wave's replay carries the Mutate's id, the post-seam live frame carries `0`. *(`crates/xmtp_api_d14n/src/queries/v3/connection.rs:47-93` passes `group_messages`/`welcome_messages` straight through in wire order; test `preserves_wire_order_of_history_markers_and_live`.)*

### 9.8 Web

40. **Where** the client runs in a browser, the streaming surface **shall** work over server-streaming only, with no client→server frames after the request. *(Source: `grpc_client/wasm.rs`; `api_client.rs:172-177`; draft `SubscribeOnce`.)*
41. **Where** bidi is unavailable, the client **shall** still satisfy requirements 9, 30-36 by reopening subscriptions. *(Source: today's legacy fallback + watchdog.)* The current legacy path meets this only partially: the idle watchdog that triggers reopening is **opt-in and off by default** (`watchdog.rs:20-23`), it reconnects only on a stale trip and not after a clean end (`watchdog.rs:462-465`), and each reopen leaves the requirement-30 local-broadcast window. The new design **should** make reopening automatic.

### 9.8a Pre-stream sync (SDK convention)

53. **When** a JS SDK opens a stream, it **shall** run a network sync first unless the caller passes `disableSync`. *(Source: `if (!options?.disableSync)` at `sdks/js/node-sdk/src/Conversations.ts:349-351, 393, 421, 452-454`, `node-sdk/src/Conversation.ts:152-154`; `sdks/js/browser-sdk/src/Conversations.ts:493-496`, `browser-sdk/src/Conversation.ts:491-494`.)* Apps rely on "subscribe, and I am also caught up"; the option exists so a caller that already synced can skip it. Default is `false` (sync runs).
54. The **scope** of that pre-stream sync **shall** be specified per entry point, because node and browser differ today: node's `streamAllMessages` calls `syncAll(consentStates)` (`node-sdk/src/Conversations.ts:452-454`) while browser's calls only `sync()` (`browser-sdk/src/Conversations.ts:493-496`). Every other entry point on both packages calls `sync()`. A caller opening an aggregate message stream is therefore caught up on message history in node and only on the conversation list in the browser.

### 9.8b Application behaviors the tests assert but the list omitted

These are pinned by existing tests and belong in the contract.

55. **When** an aggregate message stream is opened while the client has **zero** conversations, the stream **shall** stay open rather than error or close. *(`router_callbacks_tests.rs:737 stream_all_with_no_conversations_stays_open` — asserts `!closed.load(...)`, "an empty subscription must stay open, not error-close", and that nothing is delivered.)* This is also what the d14n empty-subscribe workaround (§3.6) exists to paper over.
56. **When** a conversation is created **locally** while an aggregate stream is open, that conversation's messages **shall** join the running stream with no app action. *(`router_callbacks_tests.rs:57 self_created_group_streams_its_messages` — no welcome ever arrives, so delivery proves the `LocalEvents::NewGroup` fan-in leased the topic; assertion at `:80`.)* Requirement 5 covers the welcomed case; this covers the self-created case.
57. **When** two sibling streams of the same kind run on one client, each **shall** receive the same event independently; one stream's delivery **shall not** suppress another's. *(`stream_router_tests.rs:149 sibling_conversation_streams_both_receive_a_welcome` — both `"first"` and `"second"` assert the welcomed `group_id`, proving dedup state is per stream, not shared.)*
58. **When** a group's metadata changes (e.g. a name update) while streams are open, every open stream **shall** stay usable and the group **shall not** fork. *(`bindings/mobile/src/mls/tests/streaming.rs:674 test_can_stream_and_update_name_without_forking_group` — asserts one group on the receiving side and exact message counts across the update, then a clean close.)*
59. **When** a stream is opened **before any group exists** and optimistic groups are then created, their messages **shall** reach the stream and the stream **shall not** be killed. *(`bindings/mobile/src/mls/tests/streaming.rs:769 test_stream_all_messages_with_optimistic_group_creation` — streaming starts before the first group; two optimistic groups created with members added afterwards; BIND-REQ-058.)*

### 9.9 Capability negotiation

42. **If** a backend refuses the subscription surface **at wire-open time**, the client **shall** detect it, latch the destination, and serve the first caller on the legacy path in place, without losing that caller's stream. *(MLS-REQ-111; `router_callbacks.rs:204-256` classification, `:536-571` in-place fallback.)* This holds only for open-time refusal — gRPC `UNIMPLEMENTED`, or the in-process `OtherUnretryable` refusal from the d14n and migration clients. **A capability failure discovered mid-stream does not fall back in place:** a zero-tag `CatchUpComplete` tombstones the transport, every lease ends, and existing streams see end-of-events (`bidi_transport.rs:2379-2397`; test `an_untagged_backends_completion_tombstones_the_transport` requires the lease to end, `:5605-5620`). Only the *next* subscribe sees `TransportError::Closed`, latches, and is served on legacy. The new design **should** either guarantee capability is knowable at open time (requirement 43) or specify in-place recovery for a mid-stream verdict.
43. **[Design gap]** The backend **should** advertise support explicitly rather than being probed by failure. *(Today: `Started.capabilities` exists but is "logged, not consumed" — `bidi_transport.rs`.)*

---

## 10. Candidate simplifications

Ordered by value. Each lists what could be deleted or collapsed.

### 10.1 Delete the legacy streaming stack (largest win)

If the new backend serves bidi to every client that can speak it, and `SubscribeOnce` covers the browser, the whole legacy path can go:

| Path | Lines | Note |
| --- | --- | --- |
| `crates/xmtp_mls/src/subscriptions/stream_messages.rs` | 659 | + `stream_messages/{types,stream_stats,test_utils}.rs` (~364) |
| `crates/xmtp_mls/src/subscriptions/stream_conversations.rs` | 686 | |
| `crates/xmtp_mls/src/subscriptions/stream_all.rs` + `tests.rs` | 1313 | |
| `crates/xmtp_mls/src/subscriptions/watchdog.rs` | 762 | obsolete once the server sends keepalive |
| `crates/xmtp_api_d14n/src/endpoints/v3/mls/subscribe_group_messages.rs`, `subscribe_welcome_messages.rs` | — | |
| `XmtpMlsStreams` (3 methods) in `crates/xmtp_proto/src/api_client.rs:151-170` | — | collapses to one bidi trait + one once trait |
| The dispatch/gate/latch machinery in `router_callbacks.rs` | large part of 865 | no fallback needed → no latch, no env gate |

**Blocker 1 — the browser.** Requirement 41 means *some* non-bidi path must survive unless `SubscribeOnce` fully replaces it. The cheapest shape is to keep one unidirectional client that speaks `SubscribeOnce` and reopens on change, rather than keeping the whole v3 legacy stack.

**Blocker 2 — the ungated pull streams.** Deleting `stream_messages.rs`, `stream_conversations.rs` and `stream_all.rs` also deletes the only implementation behind every public **iterator** API: `Client::stream_conversations`/`stream_all_messages` and their `_owned` forms (`mod.rs:450-546`), `MlsGroup::stream`/`stream_owned` and `stream_with_callback` (`groups/subscriptions.rs:101-145`), and WASM `streamLocal` (`bindings/wasm/src/conversations.rs:595-606`). None of them consults the bidi gate and all are bounded on `XmtpMlsStreams` (§5.2). They must be rewritten onto the router (or removed) before the legacy files can go — this is the larger of the two blockers by API surface.

**Blocker 3 — d14n.** While d14n remains a supported backend its app streams are legacy by construction: its `subscribe_bidi` refuses (`queries/d14n/streams.rs:140-156`) and it has no `TransportBinding`. Deleting the legacy stack presupposes retiring d14n or giving it a `TransportBinding` (§10.2 assumes the former).

### 10.2 Delete the d14n multi-originator machinery

With a single-originator backend and scalar `Cursor { sequence_id }` (as the draft already has):

| Path | Note |
| --- | --- |
| `crates/xmtp_api_d14n/src/protocol/order.rs` | XIP-49 ordering |
| `crates/xmtp_api_d14n/src/protocol/resolve.rs`, `sort.rs` | dependency resolution |
| `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` | cursor maps |
| `crates/xmtp_api_d14n/src/queries/stream/ordered.rs` | `OrderedStream` |
| `crates/xmtp_api_d14n/src/queries/combinators/ordered_query.rs` | |
| `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs` | v4 status frames — **not** multi-originator machinery; see the note below |
| `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` | |
| `crates/xmtp_api_d14n/src/queries/d14n/connection.rs` | second `BidiBinding` |
| `crates/xmtp_mls/src/subscriptions/d14n_compat.rs` (141) | `V3OrD14n` |
| `crates/xmtp_mls/src/subscriptions/d14n_bidi_tests.rs` (404) | |

Vector cursors disappear from `TopicCursor`, and `BidiBinding::Mutate` no longer needs two cursor shapes.

**Caveat on `status_aware.rs`.** It is listed here because it belongs to the d14n response shape, **not** because it does multi-originator work. Its module doc describes it as a combinator "that handles `SubscribeTopicsResponse` oneof variants, tracking subscription status via a shared `StreamStatus` and yielding only envelope batches" (`status_aware.rs:1-2`); `StreamStatus` is three atomics — `has_started`, `catchup_complete`, `last_ping` (`:20-29`) — and `StatusAwareStream` (`:73-100`) passes batches through untouched. There is no sorting, originator comparison, or sequence merge anywhere in the file. So a scalar cursor does not make it obsolete: it goes away when the **legacy d14n response shape** goes away, and its status-frame classification job is done by the bidi connection bindings instead.

### 10.3 Collapse the `BidiBinding` genericity

`bidi.rs` is generic over `BidiBinding` purely to serve v3 and d14n wire types at once. With one backend, the binding trait, both `connection.rs` files, and the feature-gated duplicate test suites collapse into one concrete implementation. `bidi.rs` (1138) and `bidi_transport.rs` (6172) keep their logic but lose a type parameter and a whole second instantiation.

### 10.4 Remove the untagged-backend detection and tombstone

`bidi_transport.rs` carries explicit machinery to detect a bidi-serving-but-untagged backend and shut itself down (`:2379-2397`), plus `is_bidi_unsupported` classification and the per-destination latch in `router_callbacks.rs`.

**Mandatory tags remove the zero-tag detector, not the latch.** These are two independent mechanisms. Specifying delivery tags from day one deletes the `CatchUpComplete { mutate_id: 0 }` tombstone path. It does **not** delete `is_bidi_unsupported`/`is_bidi_dead_end` and the per-destination latch (`router_callbacks.rs:187-256`), which also fire on gRPC `UNIMPLEMENTED` and on unretryable open failures — a backend that does not serve the RPC at all, an old node, or the in-process refusal from a non-bidi client. The latch can go only when **every** reachable backend must support bidi and no legacy fallback remains — which requires §10.1's three blockers to be cleared first. Until then, keep the latch and delete only the tag detector, replacing it ideally with an explicit `Started.capabilities` check (requirement 43).

### 10.5 Retire the env-var gates

`XMTP_BIDI_STREAMS_ENABLED` (`router_callbacks.rs:104`) and the four `XMTP_STREAM_WATCHDOG_*` vars (`watchdog.rs:85-88`) exist to make an experimental path opt-in. Once bidi is the only path, both sets go, along with their once-per-process capture statics.

### 10.6 Reconsider, do not delete

- **`catch_up.rs` (861)** — keep. It is the background-fetch primitive and the model for `SubscribeOnce`. It could arguably be re-expressed *as* a `SubscribeOnce` call, which would shrink it substantially.
- **`process_message.rs` + factory (845)** — keep. Decryption, recovery sync, and the DB fast path are backend-independent.
- **JS `createStream` (both packages)** — keep, but consider unifying the two divergent default sets (10/60 s vs 6/10 s).
- **`stream_router.rs` (1685)** — keep; it is the intended architecture. It may simplify once there is no legacy fallback to coexist with.

---

## 11. Open design questions for `backend.proto`

These are the top five the streaming API must answer, drawn from gaps between the draft and the implementation.

**Q1 — When does `SubscribeOnce` close, what does it order, and how is it kept alive? Who implements its client?**
The missing `mutate_id` on `SubscribeOnceResponse.CatchupComplete` (`backend.proto:157`) is **not** a gap: `SubscribeOnceRequest` (`:148-150`) carries one topic set and no mutation stream, so there is exactly one wave and nothing to demultiplex. Drop that half of the question.

The real gaps are close semantics, ordering, and liveness. No client for the RPC exists today (§4.7), and the browser must still satisfy requirements 9, 30-36 and 41. Decide: is `SubscribeOnce` a true one-shot catch-up that ends after `CatchupComplete` (then the browser needs a reopen loop and its own dedup, and the spec must say whether the server closes or the client does), or does it stay open for live delivery afterwards (then it needs an idle-liveness story — it has `Ping` but **no** `Pong`, so the server cannot tell whether the client is alive, and a unidirectional client cannot answer)? The spec must also state the ordering guarantee across its topic set, since it has no `TopicsLive` seam to mark where replay ends.

This is the highest-risk question, because the browser is the **only** client that cannot fall back to bidi, and it is also where the current code is weakest: no `SubscribeOnce` client, no bidi tests, and a watchdog whose 300 s default means a dormant browser stream reconnects on a five-minute cycle. Note also that today's browser path has no in-place mutation at all — a new conversation forces a full re-subscribe (`stream_messages.rs:subscribe`), which is exactly the behavior `SubscribeOnce` would have to keep.

**Q2 — Is `Cursor` scalar forever, and what is the ordering guarantee across topics?**
The draft's `Cursor { uint64 sequence_id }` is scalar, which lets §10.2 delete the entire XIP-49 stack. But the client's exactly-once admission relies on the server serving "in total cursor order **per kind**" (`bidi_transport.rs:24-30`), and the sibling-gap rule is written against a **shared per-kind cursor sequence across topics** — `bidi_transport.rs:77-88` names it explicitly ("cross-topic replay, on the shared per-kind cursor sequence"). Per-topic monotonicity alone is not enough for that logic. Specify precisely: is `sequence_id` global, per-topic, or per-kind-across-topics? If each kind does **not** have one total order across every topic, the ledger's watermark algorithm must be redesigned, not merely reconfigured. The client's `last_seen` is per topic, deliberately (requirement 13).

**Q3 — How is capability negotiation done, given that tags are already mandatory in v1?**
Tags are **not** an open question in the draft: `Messages.mutate_id` (`backend.proto:110`) and `CatchupComplete.mutate_id` (`:126`) are unconditional v1 fields, and the comments state the behavior — the ack "echoes the Mutate; 0 only if a waveless Mutate carried 0", and live frames are the `mutate_id 0` ones (`:132-133`). So a v1 backend must serve tags, and the client's zero-tag **detector** is deletable on that basis (§10.4). Two questions remain:

1. **What is `Started.capabilities` for, then?** It exists with only `CAPABILITY_UNSPECIFIED`, and the client "logs, not consumes" it. Either define the first real capability and make the client consume it, or drop the field until there is one.
2. **What replaces failure-probing for backends that do not serve `Subscribe` at all?** The per-destination latch (§10.4) handles `UNIMPLEMENTED` and unretryable opens, and it survives mandatory tags. Decide whether v1 keeps it or requires universal support.

Note also that proto3 cannot express "mandatory" — `mutate_id` always decodes with default `0` — so the `0` sentinel is doing double duty (live frame *and* waveless-Mutate echo). If that ambiguity matters, the spec should separate the two.

**Q4 — What are the server's limits and its behavior at those limits?**
The client chunks at 1000 topics / 16 MiB per `Mutate` (client caps, §4.6), holds up to 512 withheld frames per lease, and gives up after 128 pending outbound frames. The proto specifies none of the following, and each one is load-bearing for an agent process multiplexing many clients onto one wire (§5.2):

- **Maximum concurrent `Subscribe` streams** per client, per process, per identity, and per connection. Nothing in the draft bounds this, and nothing in it says what happens at the bound.
- **Maximum topics per subscription** and maximum bytes per frame — the server-side counterparts to requirement 8.
- **Maximum concurrent in-flight waves** per stream. The client already assumes a wave id is not reused while another is in flight (`bidi_transport.rs:2183-2191`).
- **Duplicate adds.** What does a `Mutate` that adds a topic the stream already holds do — re-run catch-up from the new cursor, or no-op? The client's re-add *is* its replay mechanism, so this must be "re-run".
- **Add/remove conflict.** `backend.proto:77-78` says adds and removes are "applied atomically per frame" but never says what happens when the same topic appears in both lists.
- **Cursor ahead of head.** A resume cursor above the topic's newest sequence id — error, or serve nothing and go live?
- **Unknown removes.** Removing a topic the stream does not hold — error, or silent no-op?
- **Error codes.** The file defines no error enum and no status message at all. `Mutate` validation failures are described in the generated v3 comments as `INVALID_ARGUMENT`, but the draft does not carry that. Limit and validation errors need named codes.

Q2's cross-topic ordering rule and Q5's cursor-below-retention behavior are the other two real gaps in this family.

**Q5 — What exactly does the server guarantee across a reconnect, and is `removes` clearing the cursor floor correct?**
The draft says `removes` "clears the topic's cursor floor so a re-add replays". The client's reconnect issues a resume wave from `last_seen` and re-issues catching-up leases "at the meet of every stakeholder's position", and it depends on a re-add below the server's floor replaying history. Specify the retention window (how far back can a client resume?), what happens when a requested cursor is older than retention, and whether the server may ever *skip* rather than replay. Requirement 22 (no repeat, no gap) is unenforceable without this.

### Secondary questions

- **Q6** — Is there a per-topic `expiry_ns` interaction with subscriptions? `EnvelopeMeta.expiry_ns` exists in the draft; the streaming rules never mention expired envelopes.
- **Q7** — Do welcomes and group messages remain distinct topic *kinds* with separate cursor sequences? The client's per-kind ordering assumption (`held_group`/`held_welcome`, "per kind") depends on the answer.
- **Q8** — Can one `Subscribe` stream be shared by multiple authenticated identities? The client already shares a wire across clients in a process and notes "a shared wire is receive-only" (`router_callbacks.rs`). The proto should say whether that is legitimate.
- **Q9** — Mechanical, but blocking: **the draft does not compile.** It has no `import` statements yet references `xmtp.mls.api.v1.*`, `xmtp.identity.associations.IdentityUpdate` and `xmtp.mls.message_contents.CommitLogEntry` (`backend.proto:43-47`), plus four undefined `IdentityService` message types (`:192-195`); and `QueryNewestResponse` (`:61-65`) puts `repeated` fields directly in a `oneof`, which protobuf forbids. Fix both before treating the file as the interface of record: add the imports, and wrap each `QueryNewest` result in a message the way `SubscribeResponse.Messages` (`:99-106`) already does.

---

## Appendix A — File inventory with line counts

**`crates/xmtp_mls/src/subscriptions/`** (23 727 lines total)

| File | Lines | Role |
| --- | --- | --- |
| `stream_router.rs` | 1685 | XIP-83 client router |
| `bidi_fuzz_tests.rs` | 1378 | fuzz |
| `stream_all/tests.rs` | 1098 | aggregate stream tests |
| `router_callbacks.rs` | 865 | dispatch, gate, latch |
| `mod.rs` | 863 | public API, errors, LocalEvents |
| `catch_up.rs` | 861 | bounded sync |
| `router_callbacks_tests.rs` | 861 | |
| `watchdog.rs` | 762 | legacy liveness floor |
| `stream_conversations.rs` | 686 | legacy conversation stream |
| `stream_messages.rs` | 659 | legacy message stream |
| `process_message.rs` | 482 | decode/recovery pipeline |
| `bidi_tests.rs` | 447 | |
| `process_welcome.rs` | 442 | |
| `d14n_bidi_tests.rs` | 404 | |
| `process_message/factory.rs` | 363 | |
| `stream_messages/stream_stats.rs` | 285 | |
| `stream_all.rs` | 215 | aggregate composition |
| `stream_router_tests.rs` | 200 | |
| `d14n_compat.rs` | 141 | `V3OrD14n` shim |
| `stream_messages/types.rs` | 63 | |
| `stream_messages/test_utils.rs` | 16 | |

**`crates/xmtp_api_d14n/src/queries/` (streaming-relevant)**

| File | Lines |
| --- | --- |
| `bidi_transport.rs` | 6172 |
| `bidi.rs` | 1138 |
| `v3/connection.rs` | 911 |
| `bidi_transport_props.rs` | 751 |
| `d14n/connection.rs` | 379 |
| `stream/status_aware.rs` | 296 |
| `stream/extractor.rs` | 194 |
| `v3/bidi.rs` | 179 |
| `stream/ordered.rs` | 168 |
| `d14n/streams.rs` | 157 |
| `v3/streams.rs` | 107 |

**`crates/xmtp_api_grpc/src/`**

| File | Lines |
| --- | --- |
| `streams/non_blocking_stream.rs` | 374 |
| `grpc_client/client.rs` | 379 |
| `grpc_client/native.rs` | 248 |
| `streams/default.rs` | 175 |
| `streams/multiplexed.rs` | 164 |
| `streams/try_from_item.rs` | 162 |
| `error.rs` | 124 |
| `streams/non_blocking_request.rs` | 95 |
| `streams/escapable.rs` | 94 |
| `grpc_client/wasm.rs` | 40 |
| `streams.rs` | 38 |

## Appendix B — Environment variables affecting streaming

| Variable | Default | Effect | Source |
| --- | --- | --- | --- |
| `XMTP_BIDI_STREAMS_ENABLED` | unset (off) | opt into the XIP-83 path, read once per process | `router_callbacks.rs:104` |
| `XMTP_STREAM_WATCHDOG_ENABLED` | false | enable the legacy idle watchdog | `watchdog.rs:85` |
| `XMTP_STREAM_WATCHDOG_IDLE_TIMEOUT_SECS` | 300 | idle trip threshold | `watchdog.rs:86` |
| `XMTP_STREAM_WATCHDOG_RECONNECT_BASE_SECS` | 1 | reconnect throttle floor | `watchdog.rs:87` |
| `XMTP_STREAM_WATCHDOG_RECONNECT_JITTER_MS` | 1000 | de-sync jitter | `watchdog.rs:88` |
| `XMTP_GRPC_KEEPALIVE_INTERVAL_SECS` | 45 | HTTP/2 keepalive interval | `native.rs:45` |
| `XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS` | 20 | HTTP/2 keepalive timeout | `native.rs:46` |
| `XMTP_GRPC_TCP_KEEPALIVE_SECS` | 45 (0 disables) | TCP keepalive | `native.rs:47` |
| `XMTP_GRPC_KEEPALIVE_WHILE_IDLE` | true | ping while idle | `native.rs:32` |

---

## Review status

Adversarial review by Codex (`gpt-5.6-sol`, read-only, high reasoning effort), thread id **`01a06248-db49-7092-a505-11b3242d8ac2`**; run record at `/Users/nickmolnar/.claude/jobs/55a23e1f/tmp/phase0/runs/review-wiki-streaming.md`. Verdict: ISSUES — 2 blockers, 25 majors, 2 minors. Every finding was re-verified against the source before the page was changed.

| Finding | Applied or rejected | Note |
| --- | --- | --- |
| Blocker: XIP-83 presented as mature on both backends; d14n has no `TransportBinding` | applied | §1 Finding 1, §5.2, §7. Confirmed: `V3Binding` is the sole `TransportBinding` (`v3/connection.rs:97`); `RouterTask` is monomorphized on it (`stream_router.rs:387-390`); d14n `subscribe_bidi` returns `OtherUnretryable` (`d14n/streams.rs:140-156`). |
| Blocker: "exactly once per lease" promoted to an app-visible guarantee | applied | New §1 Finding 6; §4.4; requirement 14 rewritten as a conditional V3 transport property with its exclusions listed; requirements 22 and 24 scoped to it; §6.6 items 9 and 15 reworded. |
| Gate does not cover Rust pull streams or WASM `streamLocal` | applied | §1 Finding 2 and §5.2, with a per-surface table. Verified: all iterator forms are bounded on `XmtpMlsStreams` and bypass `bidi_streams_active`. §10.1 gains it as a blocker. |
| Untagged backend does not fall back in place mid-stream | applied | §4.2 and requirement 42 now separate open-time refusal (falls back in place, `router_callbacks.rs:536-571`) from a mid-stream tag failure (lease ends, `bidi_transport.rs:2379-2397`, test at `:5605-5620`). |
| "Streams never advance the durable cursor" is false as an absolute | applied | §5.5, §5.6 table, requirement 37. Recovery runs `sync_with_conn` → `process_messages` → `process_message(.., true)` (`mls_sync.rs:2845-2853`), which advances cursors. |
| "Streams never apply commits" is incomplete | applied | Same edits. A streamed commit errors inline with `EpochIncrementNotAllowed` (`mls_sync.rs:2454-2458`) and is applied by the recovery sync in query order. |
| `MAX_MUTATE_TOPICS`/`MAX_MUTATE_BYTES` are not backend minimums | applied | Requirement 8 rewritten; §4.6 gains a note and the table cells are relabeled "client". |
| Requirement 13 "per topic and kind" is weaker than the ledger assumption | applied | Requirement 13 and Q2 now state the shared per-kind cursor sequence across topics (`bidi_transport.rs:77-88`). |
| `wait_for_ready` is not a reliable success signal; browser does not call it | applied | §6.1 and §6.4; requirement 1 rewritten. `let _ = s.await` discards a dropped sender (`stream_handles.rs:104-108, 234-238`). |
| Node vs browser retry and error differences beyond defaults | applied | §6.4 comparison table; requirement 38. |
| Android and iOS do not surface FFI item errors; deletion callback discards errors | applied | §6.1, §6.5, §6.6 item 8; requirement 3. Android logs (`Conversations.kt:610-612`), iOS prints (`Conversations.swift:74-76`), and no `finish(throwing:)` exists in the iOS file. |
| Pre-stream sync and `disableSync` missing from requirements | applied | §6.4 sync table; **new requirements 53 and 54**, including node `syncAll` vs browser `sync`. |
| Legacy watchdog is opt-in; reconnects only after a stale trip | applied | §5.8, §6.6 item 10, requirements 19 and 41. |
| Lifecycle scope: suspend/resume are bidi-only, resume is fire-and-forget, mobile auto-toggle | applied | §4.5, §6.1, §6.6 item 15, requirements 23 and 24. `manageStreamLifecycle` defaults to `true` (`Client.kt:171`, `Client.swift:215`). |
| Legacy watchdog re-subscribe gap for locally created conversations | applied | §5.8, §6.6 item 1, requirement 30. |
| Dedup is based on local persistence, not app consumption | applied | §5.4, §5.6, §6.6 item 9, requirement 34 (`stream_router.rs:667-675, 697-716`). |
| Local streams are not lossless or infallible | applied, with one correction | §5.7, §6.6 item 14, requirement 36. Correction: a lagged receiver does **not** drop the stream — `optify!` skips the item inside a `filter_map` and the stream continues. The page states loss-without-termination. |
| Requirement 45: a waveless Mutate with id 0 is echoed as 0 | applied | Requirement 45 rewritten — echo-exactly is the backend rule; nonzero-when-adds-present binds the client (`xmtp.mls.api.v1.rs:809-816, 966-968`). |
| Requirements 48-50 are client decoder behavior | applied | Moved to a new **§9.7b Client decoder obligations**, numbers preserved. |
| Requirement 51: only a matching Pong satisfies an explicit probe | applied | Requirement 51 rewritten, separating the explicit probe (`bidi.rs:358-362, 641-647`) from the silence watchdog (`:673-690`). |
| Tested behaviors omitted from the requirements | applied | **New requirements 55-59**: empty-start aggregate stream, local group growth, sibling stream independence, metadata continuity, optimistic-group continuity. |
| API inventory omits several public streams | applied | §5.1 note, §6.3 (`streamLocal`), §6.4 (`Conversation.stream`, `streamDeletedMessages` + deprecated `streamMessageDeletions`), §6.5 (Android Flow and iOS AsyncThrowingStream tables). |
| §10.4: mandatory tags do not remove the latch | applied | §10.4 rewritten to separate the tag detector from the `UNIMPLEMENTED`/unretryable latch. |
| §10.2 minor: `status_aware.rs` only classifies status frames | applied | §10.2 caveat added. Verified: no sorting or originator comparison in the file (`status_aware.rs:1-2, 20-29, 73-100`). |
| §3.1 minor: the 120 s tonic timeout bounds response establishment only | applied | §3.1 "Open risk" replaced with a resolved statement, verified against tonic 0.14.6 `connection.rs:73` and `grpc_timeout.rs:73-93`. The `[UNVERIFIED]` marker is removed. |
| §11 Q1 and Q3: tags are already mandatory in v1; the real Q1 gap is close and liveness | applied | Q1 and Q3 rewritten. |
| §11 Q4: missing limits and behaviors | applied | Q4 expanded to eight named gaps: stream count, topic count, bytes, in-flight waves, duplicate adds, add/remove conflict, cursor ahead of head, unknown removes, error codes. |
| Draft `backend.proto` does not compile | applied | §4.7 and new **Q9**. Verified: zero `import` lines against five external type references (`:43-47`) plus four undefined `IdentityService` types (`:192-195`); `repeated` inside `oneof` at `:61-65`. |

**Nothing was rejected outright.** One finding was applied with a correction (local-stream lag drops events but does not terminate the stream), and one framing in the review was set aside as a proto3 artifact rather than a defect: "mandatory V1 field" is not expressible in proto3, so the page states the tag rule as a behavioral contract carried by comments, and notes that `0` is overloaded between "live frame" and "waveless Mutate echo".

**Residual risk.** The corrections narrow claims that were previously stated as absolutes, so the page is now more conservative than the code in a few places by design — treat requirement 14's exclusions and requirement 37's inline/recovery split as the load-bearing statements for any spec built on this. Three areas remain thin. First, retention: no test or code path in this repo exercises a resume cursor older than the server's retention window, so requirement 22 stays unenforceable until Q5 is answered — the fuzz proves completeness only *above the lowest cursor a connection ever asked for*. Second, the d14n behavior described in §4 and §7 is verified structurally (missing `TransportBinding`, refusing client) rather than by running the d14n app-stream path, which does not exist; if d14n is meant to carry forward, that verification has to be redone against real code. Third, the SDK-level claims in §6.4 and §6.5 come from reading source, not from running the JS, Android and iOS suites, and the iOS consent and preference tests `XCTSkip` before setup, so iOS behavior in §6.5 rests on source reading alone. Line numbers throughout are accurate as of branch `self-hosted` at commit `cc878025d` and will drift.
