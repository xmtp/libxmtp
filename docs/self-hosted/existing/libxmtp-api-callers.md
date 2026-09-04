<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# LibXMTP Client → Backend API: Caller Catalog

**Status:** Phase 0 research artifact.
**Repo:** `/Users/nickmolnar/code/xmtp/libxmtp`
**Branch at time of writing:** `self-hosted`
**Purpose:** Input to the design review of `docs/self-hosted/backend.proto`. This page records how the libxmtp client calls its backend *today* — every operation, from where, with what parameters, batch sizes, cursors, ordering assumptions, concurrency, retry and error handling.

Every claim cites `path:function`. Items I could not verify are marked **[UNVERIFIED]**.

---

## Table of Contents

1. [Reading Guide and Layer Map](#1-reading-guide-and-layer-map)
2. [The API Trait Surface](#2-the-api-trait-surface)
   - 2.1 [`XmtpMlsClient`](#21-xmtpmlsclient)
   - 2.2 [`XmtpIdentityClient`](#22-xmtpidentityclient)
   - 2.3 [`XmtpMlsStreams`](#23-xmtpmlsstreams)
   - 2.4 [`XmtpMlsBidiStreams` (XIP-83)](#24-xmtpmlsbidistreams-xip-83)
   - 2.5 [`ApiClientWrapper` helpers](#25-apiclientwrapper-helpers)
   - 2.6 [`CursorStore` (d14n-specific)](#26-cursorstore-d14n-specific)
   - 2.7 [`XmtpQuery` (d14n-specific)](#27-xmtpquery-d14n-specific)
   - 2.8 [Combinators: retry, paging, ignore, ordered](#28-combinators-retry-paging-ignore-ordered)
   - 2.9 [`ApiStats`](#29-apistats)
   - 2.10 [Error taxonomy and retryability](#210-error-taxonomy-and-retryability)
3. [Topic Model](#3-topic-model)
4. [Wire Mapping: trait method → v3 RPC and d14n RPC](#4-wire-mapping-trait-method--v3-rpc-and-d14n-rpc)
5. [Caller Catalog, per operation](#5-caller-catalog-per-operation)
   - 5.1 [Send group messages (publish intents)](#51-send-group-messages-publish-intents)
   - 5.2 [Query group messages (sync one group)](#52-query-group-messages-sync-one-group)
   - 5.3 [Sync all groups / `QueryNewest`](#53-sync-all-groups--querynewest)
   - 5.4 [Send welcome messages](#54-send-welcome-messages)
   - 5.5 [Query welcome messages (sync welcomes)](#55-query-welcome-messages-sync-welcomes)
   - 5.6 [Upload key package (rotation)](#56-upload-key-package-rotation)
   - 5.7 [Fetch key packages](#57-fetch-key-packages)
   - 5.8 [Commit log publish and query](#58-commit-log-publish-and-query)
   - 5.9 [Identity: get identity updates](#59-identity-get-identity-updates)
   - 5.10 [Identity: publish identity update](#510-identity-publish-identity-update)
   - 5.11 [Identity: get inbox ids](#511-identity-get-inbox-ids)
   - 5.12 [Identity: verify smart contract wallet signatures](#512-identity-verify-smart-contract-wallet-signatures)
   - 5.13 [Streams](#513-streams)
   - 5.14 [Device sync / archive](#514-device-sync--archive)
   - 5.15 [v4-only endpoints](#515-v4-only-endpoints)
6. [Client Ordering and Cursor Invariants](#6-client-ordering-and-cursor-invariants)
7. [Environment and Backend Selection](#7-environment-and-backend-selection)
8. [Configuration Constants](#8-configuration-constants)
9. [Requirements the New Backend Must Meet (EARS)](#9-requirements-the-new-backend-must-meet-ears)
10. [Gaps and Open Questions vs `backend.proto`](#10-gaps-and-open-questions-vs-backendproto)
11. [Addendum: Deeper Ordering Detail](#11-addendum-deeper-ordering-detail-refines-52-and-6)
12. [Summary Table: Draft `backend.proto` Coverage](#12-summary-table-draft-backendproto-coverage)
13. [Review status](#review-status)

---

## 1. Reading Guide and Layer Map

The client stack has five layers. Understanding which layer a decision lives at matters, because the new backend replaces layers 4 and 5 and lets us delete most of layer 3.

```text
  Layer 1  bindings/{mobile,node,wasm}       — env / host selection, client construction
  Layer 2  crates/xmtp_mls                   — business logic; every call site lives here
  Layer 3  crates/xmtp_api (ApiClientWrapper)— batching, retry, stats, key-package mapping
  Layer 4a crates/xmtp_api_d14n/queries/v3   — V3Client: trait → xmtp.mls.api.v1 RPCs
  Layer 4b crates/xmtp_api_d14n/queries/d14n — D14nClient: trait → xmtpv4 envelopes + topics
  Layer 5  crates/xmtp_api_grpc              — tonic/grpc-web transport, streams
```

Layer 2 holds nearly every call site, but not all of them: the bindings call `ApiClientWrapper` directly for two operations without going through `xmtp_mls` — `get_newest_message_metadata` (`bindings/mobile/src/mls.rs:287-305`) and `get_inbox_ids` (`bindings/mobile/src/mls.rs:533-550`, `bindings/node/src/inbox_id.rs:14-30`, `bindings/wasm/src/inbox_id.rs:10-27`). Both are static, pre-client entry points.

Key structural fact: **the client-facing trait surface is v3-shaped.** `XmtpMlsClient` is defined in terms of `GroupMessageInput`, `WelcomeMessageInput`, `UploadKeyPackageRequest`, `group_id`, `installation_key` — see `crates/xmtp_proto/src/api_client.rs`. The d14n topic/envelope model is an *implementation detail of `D14nClient`*, which translates those v3-shaped calls into topics and `ClientEnvelope`s. The new `backend.proto` is topic/envelope-shaped, so it maps far more naturally onto `D14nClient` than onto `V3Client`.

Two concrete client implementations satisfy the traits:

| Client | File | Wire |
| --- | --- | --- |
| `V3Client<C, Store>` | `crates/xmtp_api_d14n/src/queries/v3/client.rs` | `xmtp.mls.api.v1.MlsApi` + `xmtp.identity.api.v1.IdentityApi` (xmtp-node-go) |
| `D14nClient<C, Store>` | `crates/xmtp_api_d14n/src/queries/d14n/client.rs` | `xmtp.xmtpv4.message_api.ReplicationApi` + `xmtp.xmtpv4.payer_api.PayerApi` (xmtpd + gateway) |

Both are parameterised by a `Store: CursorStore`, which is how the transport layer reads the client's persisted per-topic cursors. This is important: **in the d14n stack the cursor is read inside the transport, not passed down by the caller.** See `crates/xmtp_api_d14n/src/queries/d14n/mls.rs:query_group_messages`.

---

## 2. The API Trait Surface

### 2.1 `XmtpMlsClient`

Defined at `crates/xmtp_proto/src/api_client.rs:106`.

| Method | Request | Response |
| --- | --- | --- |
| `upload_key_package` | `UploadKeyPackageRequest { key_package: Option<KeyPackageUpload>, is_inbox_id_credential: bool }` | `()` |
| `fetch_key_packages` | `FetchKeyPackagesRequest { installation_keys: Vec<Vec<u8>> }` | `FetchKeyPackagesResponse { key_packages: Vec<KeyPackage> }` |
| `send_group_messages` | `SendGroupMessagesRequest { messages: Vec<GroupMessageInput> }` | `()` |
| `send_welcome_messages` | `SendWelcomeMessagesRequest { messages: Vec<WelcomeMessageInput> }` | `()` |
| `query_group_messages` | `GroupId` (a **16-byte** newtype, `crates/xmtp_proto/src/types/ids/group_id.rs:16-20`) | `Vec<GroupMessage>` — **v3: fully paged. d14n: one page only** |
| `query_latest_group_message` | `GroupId` | `Option<GroupMessage>` |
| `query_welcome_messages` | `InstallationId` (32 bytes) | `Vec<WelcomeMessage>` — **v3: fully paged. d14n: one page only** |
| `publish_commit_log` | `BatchPublishCommitLogRequest { requests: Vec<PublishCommitLogRequest> }` | `()` |
| `query_commit_log` | `BatchQueryCommitLogRequest { requests: Vec<QueryCommitLogRequest> }` | `BatchQueryCommitLogResponse { responses }` |
| `get_newest_group_message` | `GetNewestGroupMessageRequest { group_ids: Vec<Vec<u8>>, include_content: bool }` | `Vec<Option<GroupMessageMetadata>>` |

Three observations that matter for the new API:

1. **`query_group_messages` and `query_welcome_messages` take no cursor argument.** The cursor is resolved inside the implementation from the `CursorStore`. The caller cannot ask for "messages since X" — it can only ask for "everything I have not seen". See `crates/xmtp_api_d14n/src/queries/v3/mls.rs:query_group_messages` and `crates/xmtp_api_d14n/src/queries/d14n/mls.rs:query_group_messages`.
2. **Paging is invisible above layer 4, and the two implementations differ.** `V3Client::query_group_messages` wraps the endpoint in `api::v3_paged(...)`, which loops until a short page, so `xmtp_mls` receives every page concatenated into one flat `Vec`. `D14nClient::query_group_messages` and `D14nClient::query_welcome_messages` send one `QueryEnvelope` with `limit: MAX_PAGE_SIZE` and **do not loop** (`crates/xmtp_api_d14n/src/queries/d14n/mls.rs:136-162,190-212`); each call returns at most one page and the next sync fetches the rest. `xmtp_mls` cannot tell the two apart, so it must be correct under both.
3. **`get_newest_group_message` returns positional `Option`s.** The response vector is index-aligned with the request's `group_ids`; a group with no messages yields `None` at that index. See the warning comment at `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs:22-26`:
   > `Will get latest message for each topic / if there is no latest message, returns null in place of that message / ensure ordering is not affected by this null variable, or that extractors do no unintentionally skip nulls when they should preserve length.`

### 2.2 `XmtpIdentityClient`

Defined at `crates/xmtp_proto/src/api_client.rs:206`.

| Method | Request | Response |
| --- | --- | --- |
| `publish_identity_update` | `PublishIdentityUpdateRequest { identity_update: Option<IdentityUpdate> }` | `Option<Cursor>` |
| `get_identity_updates_v2` | `GetIdentityUpdatesRequest { requests: Vec<{inbox_id: String, sequence_id: u64}> }` | `GetIdentityUpdatesResponse { responses: Vec<{inbox_id, updates: Vec<IdentityUpdateLog>}> }` |
| `get_inbox_ids` | `GetInboxIdsRequest { requests: Vec<{identifier: String, identifier_kind: i32}> }` | `GetInboxIdsResponse { responses: Vec<{identifier, identifier_kind, inbox_id: Option<String>}> }` |
| `verify_smart_contract_wallet_signatures` | `VerifySmartContractWalletSignaturesRequest { signatures: Vec<Signature> }` | `VerifySmartContractWalletSignaturesResponse { responses: Vec<ValidationResponse> }` |

`publish_identity_update` returning `Option<Cursor>` is a d14n-only capability. `V3Client::publish_identity_update` (`crates/xmtp_api_d14n/src/queries/v3/identity.rs:16`) unconditionally returns `Ok(None)`; `D14nClient::publish_identity_update` (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:37`) extracts the cursor from the first `originator_envelope` of the publish response and logs a warning if absent.

### 2.3 `XmtpMlsStreams`

Defined at `crates/xmtp_proto/src/api_client.rs:151`.

```rust
async fn subscribe_group_messages(&self, group_ids: &[&GroupId]) -> Result<Self::GroupMessageStream, Self::Error>;
async fn subscribe_group_messages_with_cursors(&self, groups_with_cursors: &TopicCursor) -> Result<Self::GroupMessageStream, Self::Error>;
async fn subscribe_welcome_messages(&self, installations: &[&InstallationId]) -> Result<Self::WelcomeMessageStream, Self::Error>;
```

Note the asymmetry: `subscribe_group_messages` takes *no* cursors and resolves them from the `CursorStore` (`crates/xmtp_api_d14n/src/queries/v3/streams.rs:32`, `crates/xmtp_api_d14n/src/queries/d14n/streams.rs:42`), whereas `subscribe_group_messages_with_cursors` takes explicit per-topic cursors. The welcome subscribe takes a slice of installation ids, but `ApiClientWrapper::subscribe_welcome_messages` (`crates/xmtp_api/src/mls.rs`) always passes exactly one.

### 2.4 `XmtpMlsBidiStreams` (XIP-83)

Defined at `crates/xmtp_proto/src/api_client.rs:179`, **native-only** (`xmtp_common::if_native!`). The doc comment states the design intent directly:

> The XIP-83 bidirectional subscription: one long-lived stream carrying group and welcome messages, mutated in place (no reconnect on membership change) and kept alive with WebSocket-style ping/pong. Native-only — gRPC-Web transports cannot speak full-duplex, so browsers stay on `XmtpMlsStreams` with a client-side watchdog.

```rust
fn host(&self) -> &str;
async fn subscribe_bidi(&self, requests: BoxStream<'static, mls_v1::SubscribeRequest>)
    -> Result<Self::SubscribeStream, Self::Error>;
```

**Crucially, bidi today is bound to the v3 wire only.** `V3Client` implements it against `/xmtp.mls.api.v1.MlsApi/Subscribe` using `mls_v1::SubscribeRequest`/`SubscribeResponse` frames (`crates/xmtp_api_d14n/src/queries/v3/bidi.rs:13,34`). `D14nClient` implements the trait but **refuses at runtime**, returning an unretryable error and reporting a reserved sentinel host `"unsupported://d14n"` (`crates/xmtp_api_d14n/src/queries/d14n/streams.rs`, in the `if_native!` block). Its comment explains the fallback design:

> The unretryable refusal is what trips xmtp_mls's per-destination fallback latch (its router callbacks): the stream that hit it is served on the legacy path in place and every later dispatch to this client's host goes straight to legacy.

So the XIP-83 frame vocabulary the draft `backend.proto` proposes (`Mutate`/`adds`/`removes`/`mutate_id`, `Started`, `Ping`/`Pong`, `TopicsLive`, `CatchupComplete`) already exists in the client as `xmtp.mls.api.v1` messages. The new backend's version is topic-keyed rather than group-id/installation-key-keyed. Compare the two field sets:

| `backend.proto` `SubscribeRequest.Mutate` | `mls_v1` equivalent used today |
| --- | --- |
| `repeated TopicQuery adds` (topic + cursor) | group filters (`group_id` + `id_cursor`) and welcome filters (`installation_key` + `id_cursor`) |
| `repeated Topic removes` | present in `mls_v1::subscribe_request::v1::Mutate` |
| `bool history_only` | present |
| `uint64 mutate_id` | present |

### 2.5 `ApiClientWrapper` helpers

`crates/xmtp_api/src/lib.rs:69` defines:

```rust
pub struct ApiClientWrapper<ApiClient> {
    pub api_client: ApiClient,
    pub(crate) retry_strategy: Arc<Retry<ExponentialBackoff>>,
    pub(crate) inbox_id: Option<String>,
}
```

This is the layer `xmtp_mls` actually calls. Its behaviours:

| Wrapper method | File:function | Batching / looping | Retry |
| --- | --- | --- | --- |
| `query_group_messages` | `crates/xmtp_api/src/mls.rs:query_group_messages` | none (paging done below) | **none at this layer** |
| `query_latest_group_message` | `crates/xmtp_api/src/mls.rs:query_latest_group_message` | none | **none** |
| `query_welcome_messages` | `crates/xmtp_api/src/mls.rs:query_welcome_messages` | none | **none** |
| `upload_key_package` | `crates/xmtp_api/src/mls.rs:upload_key_package` | none | `retry_async!` |
| `fetch_key_packages` | `crates/xmtp_api/src/mls.rs:fetch_key_packages` | none — one call, all keys | `retry_async!` |
| `send_welcome_messages` | `crates/xmtp_api/src/mls.rs:send_welcome_messages` | none — one call, all welcomes | `retry_async!` |
| `send_group_messages` | `crates/xmtp_api/src/mls.rs:send_group_messages` | none — one call, all messages | `retry_async!` |
| `publish_commit_log` | `crates/xmtp_api/src/mls.rs:publish_commit_log` | **chunks of 10**, sequential `for` loop | none at this layer |
| `query_commit_log` | `crates/xmtp_api/src/mls.rs:query_commit_log` | **chunks of 20**, sequential `for` loop | none at this layer |
| `get_newest_message_metadata` | `crates/xmtp_api/src/mls.rs:get_newest_message_metadata` | **chunks of 1000**, `futures::future::try_join_all` — chunks run **concurrently** | none at this layer |
| `get_identity_updates_v2` | `crates/xmtp_api/src/identity.rs:68` | **chunks of `GET_IDENTITY_UPDATES_CHUNK_SIZE = 50`**, `try_join_all` — concurrent | `retry_async!` per chunk |
| `get_inbox_ids` | `crates/xmtp_api/src/identity.rs:113` | **no chunking** — all identifiers in one request | `retry_async!` |
| `publish_identity_update` | `crates/xmtp_api/src/identity.rs:48` | none | `retry_async!` |
| `verify_smart_contract_wallet_signatures` | `crates/xmtp_api/src/identity.rs:161` | none | `retry_async!` |

Two extra behaviours in the wrapper worth calling out:

**A. `fetch_key_packages` enforces a strict positional contract.**

```rust
// crates/xmtp_api/src/mls.rs:fetch_key_packages
if res.key_packages.len() != installation_keys.len() {
    return Err(crate::ApiError::MismatchedKeyPackages {
        key_packages: res.key_packages.len(),
        installation_keys: installation_keys.len(),
    });
}
let mapping: KeyPackageMap = res.key_packages.into_iter().enumerate()
    .map(|(idx, key_package)| (installation_keys[idx].to_vec(), key_package.key_package_tls_serialized))
    .collect();
```

The response must be the **same length and in the same order** as the request. There is no per-installation key in the response; the mapping is purely positional.

**`MismatchedKeyPackages` is a *length* error, not a "missing key package" error.** On the v3 wire a missing key package does **not** shorten the response: `FetchKeyPackagesResponse` documents *"Returns one key package per installation in the original order of the request. If any installations are missing key packages, an empty entry is left in their respective spots in the array"* (`crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs:421-428`), so absence arrives as an entry with empty `key_package_tls_serialized` and the length check at `crates/xmtp_api/src/mls.rs:167-183` passes. The mapping then silently carries an empty byte string, and the failure surfaces later at MLS validation.

The length check fires when the response is genuinely a different length. The d14n path can cause exactly that: `KeyPackagesExtractor::visit_upload_key_package` only pushes an entry when the envelope carries a key package (`crates/xmtp_api_d14n/src/protocol/extractors/key_packages.rs:33-48`), so an empty newest-envelope result for one installation drops out of the vector and shortens it. `ApiError::MismatchedKeyPackages` is **not retryable** (`crates/xmtp_api/src/lib.rs`, `impl RetryableError for ApiError`). This drives the fallback described in [§5.7](#57-fetch-key-packages).

**B. `get_newest_message_metadata` batches at 1000 and fans out concurrently.**

```rust
// crates/xmtp_api/src/mls.rs:get_newest_message_metadata
const BATCH_SIZE: usize = 1000;
let res = futures::future::try_join_all(group_ids.chunks(BATCH_SIZE).map(|chunk| async move {
    self.api_client.get_newest_group_message(GetNewestGroupMessageRequest {
        group_ids: chunk.iter().map(|id| id.to_vec()).collect(),
        include_content: false,
    }).await.map_err(crate::dyn_err)
})).await?;
```

This is exactly the shape the draft's `QueryNewest` proposes. `include_content: false` corresponds to `QueryNewestRequest.include_full_envelope = false`. The batch size constant 1000 matches the draft's stated "up to 1000 topics".

### 2.6 `CursorStore` (d14n-specific)

`crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs:48`. This trait is how the transport reads client cursor state. Methods:

| Method | Purpose |
| --- | --- |
| `latest(topic, originators: Option<&[&OriginatorId]>)` | highest sequence id per originator for one topic |
| `latest_for_originator(topic, originator)` | single-originator convenience |
| `latest_for_topics(topics)` | batch — used when subscribing to many group topics |
| `find_message_dependencies(hashes)` | `depends_on` cursor for each locally-published payload hash |
| `ice(orphans)` | stash envelopes whose causal dependencies have not arrived ("icebox") |
| `resolve_children(cursors)` | pull envelopes out of the icebox once their parents arrive |
| `set_cutover_ns` / `get_cutover_ns` | d14n migration cutover timestamp |
| `get_last_checked_ns` / `set_last_checked_ns` | throttle for the cutover poll |
| `has_migrated` / `set_has_migrated` | migration completion latch |

`NoCursorStore` (same file, line 408) returns zeros and `i64::MAX` for the cutover — used in tests and where no persistence exists.

**The `ice`/`resolve_children` icebox, `find_message_dependencies`, and the cutover machinery are all d14n-only complexity that a single-originator backend removes.**

### 2.7 `XmtpQuery` (d14n-specific)

`crates/xmtp_api_d14n/src/protocol/traits/xmtp_query.rs`, implemented for `ApiClientWrapper` at `crates/xmtp_api/src/xmtp_query.rs`:

```rust
fn is_d14n(&self) -> Result<bool, Self::Error>;
async fn query_at(&self, topic: Topic, at: Option<GlobalCursor>) -> Result<XmtpEnvelope, Self::Error>;
async fn get_node_clients(&self) -> Result<HashMap<u32, xmtp_api_grpc::GrpcClient>, Self::Error>;
```

This is the *only* place in the codebase where a caller can query an arbitrary topic at an arbitrary cursor. `V3Client::query_at` (`crates/xmtp_api_d14n/src/queries/v3/xmtp_query.rs:23`) dispatches on `topic.kind()` back to the four v3 RPCs. It is used by device-sync/archive and by dependency resolution, not by the ordinary sync path.

### 2.8 Combinators: retry, paging, ignore, ordered

Endpoint combinators live at `crates/xmtp_proto/src/traits/combinators/` and `crates/xmtp_api_d14n/src/queries/combinators/`.

**`v3_paged`** — `crates/xmtp_proto/src/traits/combinators/v3_paged.rs:28`:

```rust
loop {
    let result: T = self.endpoint.query(client).await?;
    let info = *result.info();
    let mut messages = result.messages();
    let num_messages = messages.len();
    out.append(&mut messages);
    if num_messages < MAX_PAGE_SIZE as usize || info.is_none() { break; }
    let paging_info = info.expect("Empty paging info");
    if paging_info.id_cursor == 0 { break; }
    self.endpoint.set_cursor(paging_info.id_cursor);
}
```

Termination conditions, in order: a short page (`< MAX_PAGE_SIZE`), a missing `paging_info`, or `id_cursor == 0`. **There is no page cap, no total-result cap, and no loop counter.** A group with 100k unread messages produces 1000 sequential round trips inside one `query_group_messages` call.

**All three termination conditions are supplied by the backend, and the loop makes no progress check of its own.** It sets the next cursor to whatever `paging_info.id_cursor` the response carried and iterates. A backend that answers a full page with the *same* nonzero `id_cursor` puts the client in an infinite request loop inside a single call — no timeout, no cancellation point, no error. **A full page must therefore carry a cursor strictly greater than the one requested.** See G8. This loop is *not* wrapped in retry by default; callers compose `api::v3_paged(api::retry(endpoint), Some(cursor))` so that each individual page is retried (`crates/xmtp_api_d14n/src/queries/v3/mls.rs:85`).

**`retry`** — `crates/xmtp_proto/src/traits/combinators/retry.rs:43`, wraps a query in `retry_async!` with the default `Retry<ExponentialBackoff>`.

**`ignore`** — discards the response body; used for publish-style endpoints (`api::ignore(...)`).

**`ordered`** — `crates/xmtp_api_d14n/src/queries/combinators/ordered_query.rs:32`. d14n only. After the query returns, it runs the cross-originator causal ordering pipeline (`Ordered::order()`), which sorts by timestamp, then causally, resolves missing `depends_on` parents by re-querying, and ices anything unresolvable. Used by `D14nClient::query_group_messages` only (`crates/xmtp_api_d14n/src/queries/d14n/mls.rs:151`). **Not** applied to welcomes.

### 2.9 `ApiStats`

`crates/xmtp_proto/src/api_client/stats.rs`. A per-endpoint `AtomicUsize` request counter, incremented by the `TrackedStatsClient` decorator (`crates/xmtp_api_d14n/src/queries/api_stats.rs`). Counted endpoints:

`upload_key_package`, `fetch_key_package`, `send_group_messages`, `send_welcome_messages`, `query_group_messages`, `query_welcome_messages`, `subscribe_messages`, `subscribe_welcomes`, `publish_commit_log`, `query_commit_log`, `get_newest_group_message`; and for identity: `publish_identity_update`, `get_identity_updates_v2`, `get_inbox_ids`, `verify_smart_contract_wallet_signature`.

Note `query_latest_group_message` increments the **`query_group_messages`** counter, not its own (`crates/xmtp_api_d14n/src/queries/api_stats.rs`). These counters are used by tests to assert call counts, so they are a de-facto behavioural contract for "how many RPCs should this operation make".

### 2.10 Error taxonomy and retryability

**`ApiClientError`** — `crates/xmtp_proto/src/traits/error.rs`. Retryable: `Client`/`ClientWithEndpoint` (delegates to the network error), `Http`, `Expired`, `Other` (delegates). Not retryable: `DecodeError`, `Conversion`, `ProtoError`, `InvalidUri`, `OtherUnretryable`, `WritesDisabled`, `Body`.

**`GrpcError`** — `crates/xmtp_api_grpc/src/error.rs:120`:

```rust
impl xmtp_common::retry::RetryableError for GrpcError {
    fn is_retryable(&self) -> bool { true }
}
```

**Every gRPC error is retryable, unconditionally — including `InvalidArgument`, `PermissionDenied`, and `FailedPrecondition`.** There is one carve-out helper, `GrpcError::is_unimplemented()` (line 115), used only by the bidi fallback latch to distinguish "the server does not have this RPC" from a transient failure. This means a client that sends a permanently-invalid publish will retry it 5 times over up to 2 minutes before giving up. This is a real risk for the new backend's validation errors.

**`ApiError`** — `crates/xmtp_api/src/lib.rs`. Retryable only for the `Api(Box<dyn RetryableError>)` variant; `MismatchedKeyPackages` and `ProtoConversion` are terminal.

**Retry policy** — `crates/xmtp_common/src/retry.rs`:

```rust
impl Default for Retry {
    fn default() -> Retry { Retry { retries: 5, strategy: ExponentialBackoff::default() } }
}
impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            multiplier: 3,
            duration: Duration::from_millis(50),
            total_wait_max: Duration::from_secs(120),
            individual_wait_max: Duration::from_secs(30),
            max_jitter: Duration::from_millis(25),
        }
    }
}
```

So: **5 retries, 50ms base, ×3 multiplier, ≤25ms jitter, ≤30s between attempts, ≤120s total.**

---

## 3. Topic Model

### 3.1 d14n topic derivation

`crates/xmtp_proto/src/types/topic.rs`. A topic is a byte string whose **first byte is the kind**, followed by the identifier:

```rust
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
}
```

| Kind | Byte | Identifier bytes | Constructor |
| --- | --- | --- | --- |
| `GroupMessagesV1` | `0x00` | MLS `group_id` (**16 bytes**) | `Topic::new_group_message(group_id)` |
| `WelcomeMessagesV1` | `0x01` | installation key / signature public key (32 bytes) | `Topic::new_welcome_message(installation_id)` |
| `IdentityUpdatesV1` | `0x02` | **hex-decoded** inbox id (32 bytes) | `Topic::new_identity_update(inbox_id)` |
| `KeyPackagesV1` | `0x03` | installation key (32 bytes) | `Topic::new_key_package(installation_id)` |

`TopicBytes = SmallVec<[u8; 33]>` — the client sizes its inline buffer for the **largest** identifier, 32 bytes, so a topic is at most 33 bytes (1 kind byte + 32 identifier bytes). Group-message topics are shorter: `GroupId` is exactly 16 bytes by protocol invariant (`crates/xmtp_proto/src/types/ids/group_id.rs:16-20`), so a group topic is 17 bytes. Only welcome, identity-update and key-package topics reach 33 bytes.

The inbox id caveat is explicit in the source (`crates/xmtp_proto/src/types/topic.rs:110-115`):

> this function expects the decoded hex from an InboxId, not the UTF-8 bytes of a InboxId.

**There is no `TopicKind` for the commit log.** The remote commit log is addressed by `group_id` through dedicated v3 RPCs (`BatchPublishCommitLog` / `BatchQueryCommitLog`) and is **not** implemented on the d14n path at all — `D14nClient::publish_commit_log` is a no-op returning `Ok(())` and `D14nClient::query_commit_log` returns an empty response with the log line `"commit log disabled for d14n"` (`crates/xmtp_api_d14n/src/queries/d14n/mls.rs:216-229`). The draft `backend.proto` *does* include `CommitLogEntry` in the `ClientEnvelope` oneof, so a fifth `TopicKind` (or an equivalent addressing scheme) is needed.

### 3.2 Topic extraction from payloads (publish path)

`crates/xmtp_api_d14n/src/protocol/extractors/topics.rs:83` (`impl EnvelopeVisitor for TopicExtractor`). This is how the client decides the `target_topic` when publishing:

| Payload | How the topic is derived | Cost |
| --- | --- | --- |
| `GroupMessageInput.V1` | **TLS-deserialize the MLS message**, take `protocol_message.group_id()` | full MLS parse per message |
| `WelcomeMessageInput.V1` / `WelcomePointer` | read `installation_key` field directly | cheap |
| `UploadKeyPackageRequest` | **TLS-deserialize and cryptographically validate the KeyPackage**, take `leaf_node().signature_key()` | key-package validation per upload |
| `IdentityUpdate` | `hex::decode(update.inbox_id)` | cheap |
| v3 response shapes (`group_message::V1`, `welcome_message::V1`) | read the plain `group_id` / `installation_key` field | cheap |

The key-package case runs a real validation, including lifetime policy:

```rust
// crates/xmtp_api_d14n/src/protocol/extractors/topics.rs:146-155
let kp_in: KeyPackageIn = KeyPackageIn::tls_deserialize_exact(upload.key_package_tls_serialized.as_slice())?;
let rust_crypto = RustCrypto::default();
let kp = kp_in.validate(&rust_crypto, xmtp_configuration::MLS_PROTOCOL_VERSION,
                        openmls::prelude::LeafNodeLifetimePolicy::Verify)?;
let installation_key = kp.leaf_node().signature_key().as_slice();
self.topic = Some(TopicKind::KeyPackagesV1.create(installation_key));
```

**Implication for the new backend:** because the client derives the topic from the payload rather than being told it, the payload and the topic are always consistent by construction. The draft `backend.proto`'s `ClientEnvelope` has **no `target_topic` / `AuthenticatedData` field at all** — the topic is implicit in the payload. That is a simplification the client can support (it already computes the topic from the payload), but it forces the *server* to do the same derivation, including the MLS deserialization for group messages and the full key-package validation for key packages. See [§10](#10-gaps-and-open-questions-vs-backendproto).

### 3.3 `AuthenticatedData` and `depends_on` (d14n)

`crates/xmtp_api_d14n/src/protocol/traits/envelopes.rs:127` (`Envelope::client_envelope`) builds:

```rust
ClientEnvelope {
    aad: Some(AuthenticatedData { target_topic: topic.into(), depends_on: depends_on.map(Into::into) }),
    payload: Some(payload),
}
```

`depends_on` is populated on the publish path for group messages only (`crates/xmtp_api_d14n/src/queries/d14n/mls.rs:send_group_messages`): the client hashes each outgoing payload, asks the `CursorStore` for that payload's dependency cursor, and stamps it into the AAD. The receiving side uses it in the causal sort (`crates/xmtp_api_d14n/src/protocol/order.rs`).

**The draft `backend.proto` drops `depends_on` entirely.** With one originator and one total order per topic, causal dependencies are subsumed by sequence order; this is the intended simplification.

### 3.4 v3 query keys

On the v3 wire the client queries by plain identifiers, not topics:

| RPC | Key |
| --- | --- |
| `QueryGroupMessages` | `group_id: bytes` + `PagingInfo { id_cursor, limit, direction }` |
| `QueryWelcomeMessages` | `installation_key: bytes` + `PagingInfo` |
| `FetchKeyPackages` | `installation_keys: repeated bytes` |
| `GetIdentityUpdates` | `repeated { inbox_id: string (hex), sequence_id: u64 }` |
| `GetInboxIds` | `repeated { identifier: string, identifier_kind: enum }` |
| `BatchQueryCommitLog` | `repeated { group_id: bytes, paging_info }` |
| `GetNewestGroupMessage` | `group_ids: repeated bytes` + `include_content: bool` |
| `SubscribeGroupMessages` | `repeated Filter { group_id, id_cursor }` |
| `SubscribeWelcomeMessages` | `repeated Filter { installation_key, id_cursor }` |

### 3.5 Originator ids (v3 compatibility shims)

`crates/xmtp_configuration/src/common/d14n.rs`:

```rust
impl Originators {
    pub const MLS_COMMITS: u32 = 0;
    pub const INBOX_LOG: u32 = 1;
    pub const APPLICATION_MESSAGES: u32 = 10;
    pub const WELCOME_MESSAGES: u32 = 11;
    pub const INSTALLATIONS: u32 = 13;      // Key Packages
    pub const REMOTE_COMMIT_LOG: u32 = 100;
    pub const DEFAULT: u32 = 100;           // local and tests
}
```

These are synthetic originator ids the client assigns to v3 message streams so that a v3 sequence id can be stored in the same `GlobalCursor` (vector clock) structure as a real d14n originator cursor. `GlobalCursor` is `BTreeMap<OriginatorId, SequenceId>` (`crates/xmtp_proto/src/types/global_cursor.rs:22`), with accessors `v3_message()`, `v3_welcome()`, `commit()`, `inbox_log()`, `v3_installations()`.

**A single-originator backend collapses `GlobalCursor` to a single `u64`.** The draft's `Cursor { uint64 sequence_id }` is exactly that. All of `crates/xmtp_proto/src/types/global_cursor.rs`, `cursor_list.rs`, the `Originators` table and `crates/xmtp_api_d14n/src/protocol/order.rs` become deletable.

---

## 4. Wire Mapping: trait method → v3 RPC and d14n RPC

| Trait method | v3 RPC (`V3Client`) | d14n RPC (`D14nClient`) |
| --- | --- | --- |
| `upload_key_package` | `MlsApi/UploadKeyPackage` | `PayerApi/PublishClientEnvelopes` (1 env, topic `0x03`) |
| `fetch_key_packages` | `MlsApi/FetchKeyPackages` | `ReplicationApi/GetNewestEnvelope` (N topics `0x03`) |
| `send_group_messages` | `MlsApi/SendGroupMessages` | `PayerApi/PublishClientEnvelopes` (+ `depends_on` stamping) |
| `send_welcome_messages` | `MlsApi/SendWelcomeMessages` | `PayerApi/PublishClientEnvelopes` |
| `query_group_messages` | `MlsApi/QueryGroupMessages` (paged loop) | `ReplicationApi/QueryEnvelopes` (1 topic, limit `MAX_PAGE_SIZE`, then `ordered`) |
| `query_latest_group_message` | `MlsApi/QueryGroupMessages` (limit 1, **Descending**, cursor 0) | `ReplicationApi/GetNewestEnvelope` (1 topic) |
| `query_welcome_messages` | `MlsApi/QueryWelcomeMessages` (paged loop) | `ReplicationApi/QueryEnvelopes` (1 topic, limit `MAX_PAGE_SIZE`) |
| `publish_commit_log` | `MlsApi/BatchPublishCommitLog` | **no-op** |
| `query_commit_log` | `MlsApi/BatchQueryCommitLog` | **returns empty** |
| `get_newest_group_message` | `MlsApi/GetNewestGroupMessage` | `ReplicationApi/GetNewestEnvelope` (N topics `0x00`) |
| `publish_identity_update` | `IdentityApi/PublishIdentityUpdate` (returns `None`) | `PayerApi/PublishClientEnvelopes` (returns cursor) |
| `get_identity_updates_v2` | `IdentityApi/GetIdentityUpdates` | `ReplicationApi/QueryEnvelopes` (N topics `0x02`, **one shared `last_seen` = min sequence id**) |
| `get_inbox_ids` | `IdentityApi/GetInboxIds` | `ReplicationApi/GetInboxIds` |
| `verify_smart_contract_wallet_signatures` | `IdentityApi/VerifySmartContractWalletSignatures` | **local** — calls the client's own `scw_verifier` against the chain |
| `subscribe_group_messages` | `MlsApi/SubscribeGroupMessages` | `ReplicationApi/SubscribeTopics` |
| `subscribe_welcome_messages` | `MlsApi/SubscribeWelcomeMessages` | `ReplicationApi/SubscribeTopics` |
| `subscribe_bidi` | `MlsApi/Subscribe` (XIP-83) | **unsupported, hard error** |

One surprise worth flagging:

- **`D14nClient::verify_smart_contract_wallet_signatures` never touches the backend.** It loops over signatures and calls `self.scw_verifier.is_valid_signature(...)` directly against an RPC provider (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:149-177`). The draft `backend.proto` puts this back on the server, matching v3.

Also note the `GetInboxIds` d14n endpoint **loses identifier-kind fidelity on the way back**: `D14nClient::get_inbox_ids` hardcodes `identifier_kind: IdentifierKind::Ethereum as i32` in every response row (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:141`), even for passkey lookups. `ApiClientWrapper::get_inbox_ids` then keys its result map by `ApiIdentifier { identifier_kind, identifier }` (`crates/xmtp_api/src/identity.rs:149-156`), so a passkey lookup on d14n cannot round-trip. The new backend must echo the requested `identifier_kind`.

---

## 5. Caller Catalog, per operation

### 5.1 Send group messages (publish intents)

**The only production call site.**

| Field | Value |
| --- | --- |
| Call site | `crates/xmtp_mls/src/groups/mls_sync.rs:3193-3196`, in `MlsGroup::publish_intents` |
| Purpose | Publish the MLS payload(s) for one queued intent — application message, commit, or proposal |
| Items per call | All payloads for **one intent**. Usually 1; multi-payload only for `ProposeMemberUpdate` (`crates/xmtp_mls/src/groups/mls_sync/update_group_membership.rs:365`). No count cap; bounded by `GRPC_PAYLOAD_LIMIT` = 25 MiB |
| Retry | 5× inside `ApiClientWrapper::send_group_messages`, whole payload replayed each attempt |
| Concurrency | Under the per-group `GroupCommitLock`; one publish per group at a time |

**The publish-then-verify pattern.** This is the single most important client behaviour for the new backend to support. It works as follows:

1. `get_publish_intent_data` produces `payloads_to_publish`, a `post_commit_action`, and (for commits) a `staged_commit`.
2. **Before** the network call, in a DB transaction (`mls_sync.rs:3160-3170`):

   ```rust
   let last_payload = payloads_to_publish.last().ok_or(GroupError::UninitializedResult)?;
   let intent_hash = sha256(last_payload);
   db.set_group_intent_published(intent.id, &intent_hash, post_commit_action, staged_commit, group_epoch as i64)?;
   ```

   The intent row in `group_intents` moves `ToPublish → Published` and stores `payload_hash = sha256(last payload)`, the staged commit, and the epoch. The comment above it is emphatic: *"removing this transaction causes missed messages"*.
   The comment on the hash choice is load-bearing:
   > Hash the last payload for intent matching. For single-payload intents this is the only payload. For multi-payload intents (ProposeMemberUpdate), hashing the last payload ensures all preceding payloads have been received before the intent resolves.
3. `send_group_messages(messages)` publishes.
4. On error, `handle_published_intent_send_failure(&db, &intent)` reverts `Published → ToPublish` (`mls_sync.rs:3216`), so the next sync republishes. The test `send_failures_for_published_intents_revert_to_to_publish` (`mls_sync.rs:5113`) pins this.
5. **The intent is not resolved by the publish response.** It resolves when the message comes *back down* from the network in `query_group_messages`, and `find_group_intent_by_payload_hash(envelope.payload_hash)` matches it (`mls_sync.rs:2461-2463`). `mls_sync` does not hash anything at that point: `payload_hash` is already on the envelope, computed by the extractors as `sha256_bytes(message.data)` — `GroupMessageExtractor::visit_group_message_v1` for d14n and `V3GroupMessageExtractor::visit_v3_group_message` for v3 (`crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs:73-125`). Only then does `process_own_message` apply the staged commit and mark the intent `Processed`.
6. `sync_until_intent_resolved` (`mls_sync.rs:640`) loops up to `MAX_GROUP_SYNC_RETRIES = 3` times, each iteration a full `sync_with_conn` (publish + receive + post_commit), backing off with base `SYNC_BACKOFF_WAIT_MS = 50`ms, `total_wait_max = SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS = 10`s, jitter `SYNC_JITTER_MS = 25`ms, checking the intent's state after each. Terminal states: `Processed` / row gone → `Ok`; `Superseded` / `Error` → `Err`.

**Requirement this places on the backend: a published message MUST become readable through the query path, byte-identical, so that `sha256(returned.data) == sha256(published payload)`.** If the backend re-encodes, re-frames, or normalizes the MLS payload bytes in any way, every intent in the system stalls for 3 retries and then errors.

**Publish-attempt cap.** In the *data-preparation* error branch (`mls_sync.rs:3128-3143`):

```rust
if (intent.publish_attempts + 1) as usize >= MAX_INTENT_PUBLISH_ATTEMPTS {  // = 3
    db.set_group_intent_error_and_fail_msg(&intent, id)?;
} else {
    db.increment_intent_publish_attempt_count(intent.id)?;
}
```

**Early exit on staged commits.** After publishing an intent that carries a staged commit, `publish_intents` **returns immediately** rather than continuing to the next intent (`mls_sync.rs:3232-3242`). The group must observe the commit land before it can build the next one, so commits are strictly serialized per group.

**Push notification hint.** `prepare_group_messages` attaches `should_send_push_notification` per payload; on the wire this is `GroupMessageInput.V1.should_push`. The draft `backend.proto` carries `GroupMessageInput` whole, so this survives.

### 5.2 Query group messages (sync one group)

| Field | Value |
| --- | --- |
| Call chain | `MlsGroup::receive` (`crates/xmtp_mls/src/groups/mls_sync.rs:2908-2915`) → `MlsStore::query_group_messages` (`crates/xmtp_mls/src/mls_store.rs:88-95`) → `ApiClientWrapper::query_group_messages` → `V3Client`/`D14nClient` |
| Items per call | **exactly 1 group id** |
| Cursor source | Implicit. `SqliteCursorStore::latest` for `TopicKind::GroupMessagesV1` reads `refresh_state` where `entity_kind IN (ApplicationMessage, CommitMessage)` (`crates/xmtp_mls/src/cursor_store.rs:49-56`) |
| Result size | Unbounded — the paging loop concatenates every page. On v3 a page is `MAX_PAGE_SIZE` (prod 100 / test 20) |
| Retry | **None at the wrapper.** On v3 each *page* is retried 5× (`api::v3_paged(api::retry(endpoint), ...)`, `crates/xmtp_api_d14n/src/queries/v3/mls.rs:85`). On d14n `QueryEnvelope` is **not** wrapped in retry (`crates/xmtp_api_d14n/src/queries/d14n/mls.rs:146-153`) |
| Concurrency | Under the per-group mutex via `sync_with_conn_locked` (`mls_sync.rs:551`). Across groups, `sync_all_groups` runs an unbounded `FuturesUnordered` (`crates/xmtp_mls/src/groups/welcome_sync.rs:249-282`) |

**d14n adds a limit but no paging loop.** `D14nClient::query_group_messages` sends `QueryEnvelope` with `limit: MAX_PAGE_SIZE` and **does not loop**. It returns at most one page. The next sync picks up the rest. **[Note: this differs from the v3 path, which exhausts all pages in one call.]**

**Ordering requirement.** `process_message` (`mls_sync.rs:2244-2265`) and `process_message_inner` (`mls_sync.rs:2465-2478`) both guard:

```rust
if last_cursor.sequence_id >= envelope.sequence_id() { /* early return, "Message already processed" */ }
```

with the comment: *"Not early returning and re-processing a message that has already been processed, has the potential to result in forks."*

So the client:

- **tolerates duplicates** — they are silently skipped and marked `previously_processed(true)`;
- **requires ascending order within a response** — because the cursor advances as it walks the list;
- **cannot detect a gap.** If the server omits sequence 5 and returns 4, 6, the cursor advances to 6 and message 5 is lost forever. This is why the d14n path has an icebox: `depends_on` lets the client notice a missing causal parent. **With a single-originator, gapless backend, the gap risk is a backend obligation, not something the client can recover from.**

**Error handling inside `process_messages`** (`mls_sync.rs:2845-2900`): each message is individually wrapped in `retry_async!(Retry::default(), ...)` (5 retries), then:

| Outcome | Behaviour | Cursor |
| --- | --- | --- |
| `Err(GroupPaused)` | abandon the rest of the batch, return summary | not advanced |
| `Err(e)` where `e.is_retryable()` | **`break`** — "If the error is retryable we cannot move on to the next message otherwise you can get into a forked group state." | not advanced |
| `Err(e)` non-retryable | record in summary, **continue** to next message | **advanced past the bad message** |
| `Ok(..)` with an `intent_error` | intent marked `Error`, error in summary, sync still counts as success | advanced |

**Epoch handling on commits.** `MlsGroup::validate_message_epoch` (`mls_sync.rs:787-832`):

```rust
if message_epoch.as_u64() + max_past_epochs as u64 <= group_epoch.as_u64() {
    return Err(GroupMessageProcessingError::OldEpoch(message_epoch.as_u64(), group_epoch.as_u64()));
} else if message_epoch.as_u64() > group_epoch.as_u64() {
    tracing::error!(... "message epoch {} is greater than group epoch {} ... Retrying message");
    return Err(GroupMessageProcessingError::FutureEpoch(message_epoch.as_u64(), group_epoch.as_u64()));
}
```

with `MAX_PAST_EPOCHS = 3` (`crates/xmtp_configuration/src/common/mls.rs:33`). A message more than 3 epochs behind is `OldEpoch`; a message ahead of the local epoch is `FutureEpoch` and logged at `error!` with "Should not happen, logging proactively". Both are `GroupMessageProcessingError` variants; whether they abort the batch depends on their `is_retryable()`.

### 5.3 Sync all groups / `QueryNewest`

This is the operation the draft's `QueryNewest` RPC targets.

| Field | Value |
| --- | --- |
| Call site | `WelcomeService::filter_groups_needing_sync` (`crates/xmtp_mls/src/groups/welcome_sync.rs:285-307`), call at line 299. Also called directly from the mobile binding, bypassing `xmtp_mls` (`bindings/mobile/src/mls.rs:287-305`) |
| Purpose | Read-amplification guard. Before syncing N groups, ask the server for each group's newest message cursor and skip groups with nothing new |
| Items per call | **Every group the caller is about to sync.** For `sync_all_welcomes_and_groups` that is every conversation from `fetch_conversation_list` (including duplicate DMs and sync groups). Chunked at **1000 per RPC** by `ApiClientWrapper::get_newest_message_metadata`, all chunks fired **concurrently** via `try_join_all` with **no cap** |
| `include_content` | `false` — metadata only. Maps to `QueryNewestRequest.include_full_envelope = false` |
| Cursor | None sent. Comparison is client-side against `refresh_state` (`ApplicationMessage` / `CommitMessage`) |
| Error handling | Propagates with `?` — a metadata failure **aborts the whole `sync_all_groups`** |

```rust
// crates/xmtp_mls/src/groups/welcome_sync.rs:294-303
let last_synced_cursors = db.get_last_cursor_for_ids(&id_slices,
    &[EntityKind::ApplicationMessage, EntityKind::CommitMessage])?;
let latest_message_metadata = api.get_newest_message_metadata(&group_ids).await?;
let group_ids_needing_sync = filter_groups_with_new_messages(last_synced_cursors, latest_message_metadata);
```

**Downstream fan-out.** `sync_all_welcomes_and_groups` (`welcome_sync.rs:400-434`) then runs `sync_groups_in_batches(filtered_groups, 10)` — `stream::iter(...).for_each_concurrent(10, ...)`, so **at most 10 concurrent group syncs**, each issuing one `query_group_messages` plus possibly `get_identity_updates_v2` and `send_group_messages`.

**But `sync_all_groups` (`welcome_sync.rs:243-283`), the other entry point, uses an unbounded `FuturesUnordered`** — no concurrency limit at all. Called from `sync_all_welcomes_and_history_sync_groups` (`welcome_sync.rs:305-327`) and from the public SDK surface (`crates/xmtp_mls/src/client.rs:1143`).

**Other conversation-list uses of "newest message".** Conversation list ordering by last message is served **from the local DB** (`db.fetch_conversation_list(query_args)`), not from the network. There is no network call for list ordering. `query_latest_group_message` has **no non-test call sites** in `xmtp_mls` — its only occurrence is `crates/xmtp_mls/src/subscriptions/bidi_fuzz_tests.rs:467`. `get_newest_message_metadata` has entirely replaced it.

### 5.4 Send welcome messages

| Field | Value |
| --- | --- |
| Call site | `MlsGroup::send_welcomes` (`crates/xmtp_mls/src/groups/mls_sync.rs:4469-4650`), call at line 4646 |
| Caller | `MlsGroup::post_commit` (`mls_sync.rs:4272-4301`) for each intent in `IntentState::Committed` carrying a `PostCommitAction::SendWelcomes` |
| Items per call | `chunk_size = (GRPC_PAYLOAD_LIMIT / per_welcome).clamp(1, 50)` — at most 50 welcomes per RPC, but as few as **1** when the estimated per-welcome size exceeds 25 MiB (`mls_sync.rs:4636-4648`) |
| Total welcomes | `action.installations.len()` (+1 if a welcome pointer is used) — up to ~2500 for a max-size group (`MAX_GROUP_SIZE = 250` × `MAX_INSTALLATIONS_PER_INBOX = 10`) |
| Concurrency | `try_join_all(futures)` — **every chunk in flight at once, no semaphore and no cap.** The number of concurrent RPCs is `ceil(total_welcomes / chunk_size)`, so it *falls* as the chunk grows: ~50 RPCs at chunk size 50 for 2500 welcomes, and **one RPC per welcome — up to ~2500 concurrent — when the chunk size clamps to 1**. First error aborts |
| Retry | 5× per chunk inside the wrapper |

```rust
// crates/xmtp_mls/src/groups/mls_sync.rs:4640-4649
let per_welcome = welcome_calculated_payload_size.max(1);
let chunk_size = (GRPC_PAYLOAD_LIMIT / per_welcome).clamp(1, 50);
let api = self.context.api();
let mut futures = vec![];
for welcomes in welcomes.chunks(chunk_size) {
    futures.push(api.send_welcome_messages(welcomes));
}
try_join_all(futures).await?;
```

**The size estimate is approximate, and it is not a byte budget.** `welcome_calculated_payload_size` (`mls_sync.rs:4610-4640`) sums four selected fields of the **first** welcome only — `installation_key + data + hpke_public_key + welcome_metadata` for a `V1`, or `installation_key + welcome_pointer + hpke_public_key` for a pointer — and omits protobuf framing, tags, and every other welcome's actual size. The 25 MiB it is divided into is `GRPC_PAYLOAD_LIMIT` (`crates/xmtp_configuration/src/common/api.rs:12`), the **transport frame limit** (`max_encoding_message_size` / `max_decoding_message_size`), not a measured request size. So the chunking keeps the request *roughly* under one frame; it does not guarantee it. The fallback per-welcome estimate, used when the version field is missing, is `GRPC_PAYLOAD_LIMIT / MAX_GROUP_SIZE` = 25 MiB / 250 ≈ **104 KiB** (`mls_sync.rs:4634`).

**Welcome pointers.** When `wp_capable > INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING` (= 2, `crates/xmtp_configuration/src/common/mls.rs:70`), the client publishes **one** symmetric-encrypted "pointee" welcome to a **random 32-byte destination** and sends each installation a small pointer instead of a full copy. The config comment states pointers are ~<100 bytes and "the amount needed to be stored can be 100x less than using regular welcome messages".

**This means the backend sees welcome-topic writes to installation keys that are not real installations** — the pointee destination is a random 32-byte value that is never an installation. See §5.5 for how it is read back.

**Cursor embedded in the payload.** `WelcomeMetadata { message_cursor }` carries `intent.sequence_id` so the receiving installation knows where to start reading the group (`mls_sync.rs:4474-4478`).

**A length mismatch panics.** `assert_eq!(welcomes.len(), total_installations + usize::from(welcome_pointer_bytes.is_some()));` (`mls_sync.rs:4605-4608`).

**Failure handling.** A failure propagates with `?` from `post_commit`, so the intent stays `Committed` and post_commit retries on the next sync. The error is captured via `summary.add_post_commit_err(e)` rather than aborting the sync (`mls_sync.rs:597-601`).

### 5.5 Query welcome messages (sync welcomes)

**Two distinct call patterns.**

**A. Own-installation welcome sync.**

| Field | Value |
| --- | --- |
| Call chain | `WelcomeService::sync_welcomes` (`crates/xmtp_mls/src/groups/welcome_sync.rs:213-238`) → `MlsStore::query_welcome_messages` (`crates/xmtp_mls/src/mls_store.rs:72-85`) |
| Items | **exactly 1 installation key** — the client's own |
| Cursor | `refresh_state` where `entity_kind = EntityKind::Welcome`, keyed by installation id |
| Retry | None at the wrapper; each *welcome* is separately wrapped in `retry_async!(Retry::default(), ...)` during processing |
| Concurrency | Strictly sequential `for` loop in `process_welcomes_with` (`welcome_sync.rs:165-207`) |

The ordering discipline here is stricter than for group messages. The comment at `welcome_sync.rs:165` is explicit:
> Welcome commits advance the durable cursor. Stop after a retryable error so a later envelope cannot skip the failed welcome.

| Outcome | Behaviour |
| --- | --- |
| retryable error | **`break`** — stop the whole batch |
| non-retryable error | `warn!("skipping welcome after non-retryable failure")`, **continue**, cursor advances past it |
| duplicate for a joined group | `WelcomeOutcome::AlreadyProcessed(cursor)`, logged at `warn` (`welcome_sync.rs:140-151`) |

**Side effect:** if `num_envelopes > 0`, `queue_key_rotation(&self.context)` is called (`welcome_sync.rs:229-236`); a failure there is swallowed with a `warn`. The comment explains the choice: *"It is better to over-rotate than to under-rotate, as the latter risks leaving expired key packages on the network."*

**B. Welcome-pointer resolution — a random-key read, bypassing the wrapper.**

`crates/xmtp_mls/src/groups/welcome_pointer.rs:7-52`, `resolve_welcome_pointer`, call at line 30:

```rust
let welcome = retry_async!(
    Retry::default(),
    (context.api().api_client
        // TODO: limit this to a single message somehow (maybe an earliest_welcome_message fn)
        .query_welcome_messages(decrypted_v1.destination.as_slice().try_into()?))
);
```

| Field | Value |
| --- | --- |
| Items | 1 pseudo-installation key (the pointer's random 32-byte `destination`) |
| Cursor | **Always 0** — a random destination has never been seen locally, so this is a full fetch every time |
| Result use | Only the **first** returned welcome is used; the rest are discarded. The TODO acknowledges the client cannot ask for just one |
| Retry | A hand-rolled **outer** loop on top of the inner 5-retry `retry_async!`, because the pointee may not have landed yet. Loops `retries <= retry.retries()` with backoff, returns `Ok(None)` when exhausted |
| Concurrency | Called from the sequential welcome loop, but **also** from the streaming path where welcomes are processed in an unbounded `JoinSet` |

This is a **read-after-write race the client papers over with retries**. It calls `api_client` directly rather than through the wrapper, so it bypasses the wrapper's stats counting and error mapping.

Validation failures (pointer-to-a-pointer, decrypted pointer) return `ConversionError::InvalidValue`; the source comment notes: *"These failure modes are non-retryable and will end up incrementing the cursor and will prevent the welcome message from being retried."*

### 5.6 Upload key package (rotation)

| Field | Value |
| --- | --- |
| Core function | `Identity::rotate_and_upload_key_package` (`crates/xmtp_mls/src/identity.rs:710-747`), call at line 726 |
| Items | **exactly 1 key package**, `is_inbox_id_credential = true` |
| Cursor | None |
| Retry | 5× inside the wrapper |

```rust
let (kp_bytes, history_id) = self.generate_and_store_key_package(mls_storage, include_post_quantum)?;
match api_client.upload_key_package(kp_bytes, true).await {
    Ok(()) => {
        db.mark_key_package_before_id_to_be_deleted(history_id)?;
        mls_storage.db().reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;
        Ok(())
    }
    Err(err) => Err(IdentityError::ApiClient(err)),
}
```

Generate-then-upload is deliberately split so a signature-validation failure cannot orphan a network key package (`identity.rs:749-753`). On terminal failure the local KP is stored but previous ones are **not** deleted and the rotation queue is **not** reset, so the next tick retries.

**Callers:**

| Site | Trigger |
| --- | --- |
| `crates/xmtp_mls/src/identity.rs:697` (`Identity::register`) | first registration; skipped if `StoredIdentity` already exists |
| `crates/xmtp_mls/src/client.rs:1100-1113` | public SDK entry point; follows with `nudge_deletion` |
| `crates/xmtp_mls/src/worker/key_package_maintenance.rs:86-105` (`rotate_if_needed`) | **the periodic path**, guarded by `is_identity_needs_rotation()` — at most once per `KEY_PACKAGE_ROTATION_INTERVAL_NS` = **30 days** |
| `crates/xmtp_mls/src/groups/welcome_sync.rs:233` | after any welcome sync that returned envelopes (via `queue_key_rotation`) |

**Registration ordering.** `Client::register_identity` (`crates/xmtp_mls/src/client.rs:1041-1053`) uploads the key package **before** publishing the identity update:

```rust
// Step 3: Upload key package first (prevents race condition)
self.context.api().upload_key_package(kp_bytes, true).await?;
// Step 4: Publish identity update (makes installation visible)
let registration_cursor = self.context.api().publish_identity_update(identity_update).await?;
```

**An installation must never be visible without a fetchable key package.** This is a cross-topic ordering constraint the new backend inherits.

**Key package lifetime.** `KEY_PACKAGE_ROTATION_INTERVAL_NS = NS_IN_30_DAYS` (`crates/xmtp_configuration/src/common/mls.rs:25`). Its doc comment draws the distinction the backend needs:
> Interval in NS used to compute `next_key_package_rotation_ns`. This defines how often a new KeyPackage should be *rotated*, but does *not* determine the actual KeyPackage expiration.

Expiration is carried in the MLS leaf-node lifetime, validated by `LeafNodeLifetimePolicy::Verify` in `TopicExtractor::visit_upload_key_package`. `KEYS_EXPIRATION_INTERVAL_NS` is `NS_IN_DAY` in prod, `3 * NS_IN_SEC` in test (`crates/xmtp_configuration/src/prod/mls.rs:5`, `test/mls.rs:7`).

**Local cleanup.** `key_package_maintenance::sweep_expired` deletes local key material where `delete_at_ns <= now`. Its comment: *"Late execution is harmless — deletion is local-only; the network copy expires independently."* **The client never asks the backend to delete a key package.** It relies on the backend to serve only the newest, and on expiry to retire old ones.

**Only one key package per installation is live.** `fetch_key_packages` returns exactly one per installation key; the client never enumerates or requests a specific one.

### 5.7 Fetch key packages

| Field | Value |
| --- | --- |
| Wrapper | `MlsStore::get_key_packages_for_installation_ids` (`crates/xmtp_mls/src/mls_store.rs:99-125`) |
| Items | **N installation keys in ONE request, no chunking anywhere.** Practical max = `MAX_GROUP_SIZE` (250) × `MAX_INSTALLATIONS_PER_INBOX` (10) = **2500 keys in one RPC** |
| Cursor | None. `TopicKind::KeyPackagesV1` always yields `GlobalCursor::default()` |
| Retry | 5× inside the wrapper |

**The all-or-nothing failure mode.** As covered in §2.5, a **length** mismatch produces the terminal `ApiError::MismatchedKeyPackages`. This is not the general "missing key package" case: on v3 an absent key package comes back as an empty positional entry and the batch succeeds (`crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs:421-428`). The batch fails only when the response length differs from the request length — which the d14n extractor produces by dropping an empty newest result (`crates/xmtp_api_d14n/src/protocol/extractors/key_packages.rs:33-48`). **When it does fire, one installation fails the entire batch of up to 2500.**

**Call sites:**

| # | Site | Items | Notes |
| --- | --- | --- | --- |
| 1 | `get_keypackages_for_installation_ids` (`crates/xmtp_mls/src/groups/mls_sync.rs:4831-4867`) — the membership-update hot path | all added/updated installations minus self, up to ~2500 | Per-key verification failures are collected into `failed_installations` and persisted in the `GroupMembership` extension so the commit proceeds without them and retries later (`mls_sync.rs:4045-4048`) |
| 2 | `MlsGroup::check_extension_support` (`crates/xmtp_mls/src/groups/mod.rs:599-618`) | all members minus self | Any individual verification `Err(_)` → `Ok(false)` ("not supported"); a whole-batch `MismatchedKeyPackages` propagates as `Err` |
| 3 | `MlsGroup::installation_extensions` (`crates/xmtp_mls/src/groups/mod.rs:724-782`) | batch, then per-key fallback | **The only place in the entire API surface with an explicit concurrency limit** |
| 4 | `Client::get_key_packages_for_installation_ids` (`crates/xmtp_mls/src/client.rs:1117-1128`) | pass-through, unbounded N | public API |
| 5 | `crates/xmtp_mls/src/client.rs:1380, 1397, 1831` | 1 key each | credential/installation validation |

**The fallback path at site 3** is the client's workaround for the all-or-nothing contract:

```rust
// crates/xmtp_mls/src/groups/mod.rs:735-763
// Cap the fallback fan-out: this path fires when the group holds
// installations without published key packages, and large groups can
// hold hundreds of installations.
const MAX_CONCURRENT_KEY_PACKAGE_FETCHES: usize = 16;

let verified = match store.get_key_packages_for_installation_ids(query_ids.clone()).await {
    Ok(key_packages) => key_packages,
    Err(e) if Self::is_missing_key_package(&e) => {
        futures::stream::iter(query_ids.into_iter().map(|id| async move {
            match store.get_key_packages_for_installation_ids(vec![id.clone()]).await { ... }
        }))
        .buffer_unordered(MAX_CONCURRENT_KEY_PACKAGE_FETCHES)
        ...
    }
    Err(e) => return Err(e.into()),
};
```

Worst case for a 250-inbox group: 1 failed batch RPC + up to 2500 individual RPCs at concurrency 16. `is_missing_key_package` (`mod.rs:788-795`) narrowly matches `MlsStoreError::Api(ApiError::MismatchedKeyPackages { .. })` so transient network errors are *not* fanned out.

**Recommendation for the new backend: make the key-package fetch response self-describing (keyed by installation) with explicit absence, so a missing key package is a normal result rather than a batch failure.** This deletes the entire fallback path.

### 5.8 Commit log publish and query

**Publish** — `CommitLogWorker::publish_commit_logs_to_remote` (`crates/xmtp_mls/src/groups/commit_log.rs:262-305`), call at line 288.

| Field | Value |
| --- | --- |
| Purpose | Publish signed plaintext commit-log entries for every conversation where the client is a super admin, plus every DM |
| Items | `all_entries` is the **flattened union across all publishable conversations** — one `PublishCommitLogRequest` per local commit-log row. The wrapper chunks into groups of **10, sequentially** |
| Cursor read | `get_local_commit_log_cursor(conversation.id)` (local sqlite rowid) vs `get_last_cursor_for_originator(id, EntityKind::CommitLogUpload, Originators::REMOTE_COMMIT_LOG).sequence_id`. If `local <= published`, skip |
| Cursor written | `update_cursor(id, EntityKind::CommitLogUpload, Cursor::commit_log(rowid))` — **only after the entire API call succeeds.** A partial-batch failure leaves *all* cursors unadvanced and re-publishes earlier batches next turn |
| Ordering | `LocalCommitLogOrder::AscendingByRowid` — strictly ascending sqlite rowid, starting at 1 |
| Retry | None at the wrapper; failure propagates and the worker retries next tick |

`CommitType::RemovedFromGroup` entries are filtered out of the published set but the cursor still advances past them (`commit_log.rs:342-359` — a removed member's local marker must never shadow the real consensus entry). If no signing key exists, `sign_group_logs` logs `warn` and returns `vec![]`, silently publishing nothing for that group (`commit_log.rs:379-410`).

**Query** — `CommitLogWorker::save_remote_commit_log` (`crates/xmtp_mls/src/groups/commit_log.rs:411-480`), call at line 453.

| Field | Value |
| --- | --- |
| Items | One `QueryCommitLogRequest` per conversation from `get_conversation_ids_for_remote_log_download()` — **every group and DM the client is in, except sync groups.** The wrapper chunks into groups of **20, sequentially** |
| Per-group limit | `MAX_PAGE_SIZE` (prod 100 / test 20), `SortDirection::Ascending` |
| Paging | **Explicitly none.** The source comment: *"For now we will rely on next iteration of the worker to download the next batch of commit log entries if there is more than MAX_PAGE_SIZE entries to download per group"* |
| Cursor | `id_cursor = cursor.sequence_id` from `get_remote_log_cursors(...)` → `refresh_state` `EntityKind::CommitLogDownload` |
| Per-entry errors | A `PlaintextCommitLogEntry::decode` failure logs `warn!("failed to decode commit-log entry, skipping")` and **continues** — the bad entry is skipped, not fatal (`commit_log.rs:497-508`) |

**The commit log is a `xmtp.mls.message_contents.CommitLogEntry` addressed by `group_id`, published in plaintext and signed with a per-group commit-log key.** It is a *separate ordered stream from the group message stream* — its sequence ids are the server's own commit-log sequence, unrelated to message sequence ids. The client tracks four distinct `refresh_state` entity kinds for it (`CommitLogUpload`, `CommitLogDownload`, `CommitLogForkCheckLocal`, `CommitLogForkCheckRemote`).

**Critical: the commit log is exempt from the d14n cutover.** `MigrationClient::publish_commit_log` and `query_commit_log` route unconditionally to `self.v3_client` (`crates/xmtp_api_d14n/src/queries/combined.rs:224-236`). The comment:
> a migrated client whose reads/writes go to xmtpd must keep publishing and reading its commit log on v3, or fork detection silently dies the moment it crosses the cutover (the d14n client's commit-log methods are deliberate no-ops).

**The new backend must serve the commit log natively.** `ENABLE_COMMIT_LOG = true` (`crates/xmtp_configuration/src/common/mls.rs:42`), and fork detection depends on it.

### 5.9 Identity: get identity updates

**The single funnel.** `load_identity_updates` (`crates/xmtp_mls/src/identity_updates.rs:592-635`), call at line 612:

```rust
if inbox_ids.is_empty() { return Ok(HashMap::new()); }
let existing_sequence_ids = conn.get_latest_sequence_id(inbox_ids)?;
let filters: Vec<GetIdentityUpdatesV2Filter> = inbox_ids.iter()
    .map(|inbox_id| GetIdentityUpdatesV2Filter {
        sequence_id: existing_sequence_ids.get(*inbox_id).map(|i| *i as u64),
        inbox_id: inbox_id.to_string(),
    }).collect();
let updates = api_client.get_identity_updates_v2(filters).await?.collect::<HashMap<_, _>>();
conn.insert_or_ignore_identity_updates(&to_store)?;
```

| Field | Value |
| --- | --- |
| Items | N inbox ids, chunked **50 per RPC**, chunks fired **concurrently via `try_join_all`, no cap**. A max-size group's membership refresh = 250 inbox ids → **5 parallel RPCs** |
| Cursor | **Per-inbox-id `sequence_id`**, read from `identity_updates.sequence_id` (max per inbox). `None` maps to `0`. Server returns only *later* updates |
| Cursor written | `insert_or_ignore_identity_updates` → `identity_updates` table (`inbox_id`, `sequence_id`, `server_timestamp_ns`, `payload`, `originator_id = Originators::INBOX_LOG`) |
| Duplicates | Absorbed by `insert_or_ignore` |
| Gaps | **Fatal to correctness.** Association-state reconstruction folds updates in order (`update.update_state(association_state, update.client_timestamp_ns)`, `identity_updates.rs:685-687`); a gap yields a wrong association state |
| Retry | 5× per chunk. A deserialization failure of any single `IdentityUpdateLog` fails the whole chunk |

**⚠ The d14n implementation loses per-inbox cursor fidelity.** `D14nClient::get_identity_updates_v2` (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:75-93`):

```rust
let min_sid = request.requests.iter().map(|r| r.sequence_id).min().unwrap_or(0);
let topics = request.requests.topics()?;
let last_seen = Some(ProtoCursor { node_id_to_sequence_id: [(Originators::INBOX_LOG, min_sid)].into() });
let result: QueryEnvelopesResponse = QueryEnvelopes::builder()
    .envelopes(EnvelopesQuery { topics: ..., originator_node_ids: vec![], last_seen })
    .build()?.query(&self.client).await?;
```

It collapses N per-inbox cursors into **one shared `last_seen` = the minimum sequence id across the batch**, because `EnvelopesQuery` has only one cursor for all topics. For a batch of 50 inboxes with widely differing cursors, this over-fetches everything above the least-advanced inbox. `insert_or_ignore` makes it correct but wasteful.

**The draft `backend.proto` fixes this**: `QueryRequest { repeated TopicQuery queries }` where each `TopicQuery` carries its own `Cursor`. That is the right shape for this caller.

**Non-test call sites of `load_identity_updates`:**

| File:line | Function | Inbox ids per call |
| --- | --- | --- |
| `crates/xmtp_mls/src/builder.rs:329` | `ClientBuilder::build` | 1 (own). Skipped when `allow_offline` |
| `crates/xmtp_mls/src/identity.rs:458` | `Identity::new` | 1 |
| `crates/xmtp_mls/src/client.rs:336` | `inbox_addresses_with_verifier` | N |
| `crates/xmtp_mls/src/client.rs:506` | `Client::inbox_state` | 1, only if `refresh_from_network` |
| `crates/xmtp_mls/src/client.rs:523` | `Client::inbox_addresses` | N, only if `refresh_from_network` |
| `crates/xmtp_mls/src/client.rs:545` | `Client::fetch_inbox_updates_count` | N, only if `refresh_from_network` |
| `crates/xmtp_mls/src/client.rs:586` | `Client::inbox_creation_signature_kind` | 1, only if `refresh_from_network` |
| `crates/xmtp_mls/src/client.rs:1060` | `Client::register_identity` | 1, wrapped in an **extra** `retry_async!` |
| `crates/xmtp_mls/src/identity_updates.rs:216` | `get_latest_association_state` | 1 |
| `crates/xmtp_mls/src/identity_updates.rs:478` | `apply_signature_request` | 1, **extra** `retry_async!` |
| `crates/xmtp_mls/src/identity_updates.rs:512` | `get_installation_diff` | added+updated inboxes, pre-filtered by `filter_inbox_ids_needing_updates` |
| `crates/xmtp_mls/src/groups/welcomes/validated_membership.rs:43` | `check_initial_membership` | full welcome membership, pre-filtered |
| `crates/xmtp_mls/src/groups/mls_sync.rs:3642` | `get_publish_intent_data`, `ProposeMemberUpdate` | `intent_data.add_inbox_ids` |
| `crates/xmtp_mls/src/groups/mls_sync.rs:4023` | commit-from-proposals path | `inbox_ids_to_add` |
| **`crates/xmtp_mls/src/groups/mls_sync.rs:4385`** | **`get_membership_update_intent` — the hot path** | **every member of the group, up to 250, with no pre-filter** |

The 4385 site is the read-amplification hotspot. It is reached from `add_members`, `remove_members`, and — critically — from `maybe_update_installations` → `add_missing_installations` (`mls_sync.rs:4304-4367`), which runs after **every** `sync()` and for every group in `sync_all_groups`. It is throttled only by `SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS` — **30 minutes in prod, 1 second in test** (`crates/xmtp_configuration/src/prod/mls.rs:3`, `test/mls.rs:5`), persisted per group via `get_installations_time_checked` / `update_installations_time_checked`.

**Missing sequence id is a hard error:** `latest_sequence_ids.get(inbox_id).copied().ok_or(GroupError::MissingSequenceId)?` (`mls_sync.rs:4038-4041`, `3661-3664`, `4408-4415`).

**Full-history fetch.** `is_member_of_association_state` (`crates/xmtp_mls/src/identity_updates.rs:655-695`) passes `sequence_id: None` for one inbox → **full history from sequence 0, no cursor, results not persisted**. A stateless verification helper.

### 5.10 Identity: publish identity update

| Site | Items | Notes |
| --- | --- | --- |
| `crates/xmtp_mls/src/identity.rs:574` (`Identity::new`) | 1 | after `register` (which uploads the KP). Guarded by `MAX_INSTALLATIONS_PER_INBOX = 10` → `IdentityError::TooManyInstallations` (`identity.rs:479-486`) |
| `crates/xmtp_mls/src/identity_updates.rs:149-165` (`apply_signature_request_with_verifier`) | 1 | `to_verified(scw_verifier)` runs locally first. Comment: *"We don't need to validate the update, since the server will do this for us"* |
| `crates/xmtp_mls/src/client.rs:1049-1053` (`Client::register_identity`) | 1 | **returns and persists a cursor** |

**The registration cursor** is the one place a publish response is consumed:

```rust
// crates/xmtp_mls/src/client.rs:1082-1088
if let Some(cursor) = registration_cursor {
    stored_identity.registration_cursor_originator_id = Some(cursor.originator_id as i64);
    stored_identity.registration_cursor_sequence_id = Some(cursor.sequence_id as i64);
}
stored_identity.store(&self.context.db())?;
```

It is consumed by `crates/xmtp_mls/src/registration_visible/mod.rs` to poll each node until the registration is visible — a `FuturesUnordered` over node clients with `Retry::builder().retries(10)` and `total_wait_max = options.timeout_ms` (`registration_visible/mod.rs:188-212`).

**This is a read-your-writes visibility requirement.** The draft `backend.proto`'s `PublishResponse { repeated EnvelopeMeta envelope_metas }` supplies the cursor, and with one backend there is no per-node polling. The whole `registration_visible` module becomes deletable.

### 5.11 Identity: get inbox ids

**No chunking anywhere** — neither in the wrapper nor at any call site. Besides the `xmtp_mls` sites below, all three bindings expose a static one-identifier lookup that calls `ApiClientWrapper::get_inbox_ids` directly (`bindings/mobile/src/mls.rs:533-550`, `bindings/node/src/inbox_id.rs:14-30`, `bindings/wasm/src/inbox_id.rs:10-27`).

| # | Site | Items | Notes |
| --- | --- | --- | --- |
| 1 | `crates/xmtp_mls/src/identity.rs:445` (`Identity::new`) | 1 | mismatch → `IdentityError::NewIdentity("Inbox ID mismatch")` |
| 2 | `crates/xmtp_mls/src/client.rs:467` (`find_inbox_ids_from_identifiers`) | cache misses gate the call | **Latent bug at `client.rs:466`: the request is built from the full `identifiers` list, not `missing`.** The cache filter decides *whether* to call, not *what* to send |
| 3 | `crates/xmtp_mls/src/client.rs:1250` (`Client::can_message`) | all supplied | Absent identifiers are filled in as `false` — **absence is a normal result, not an error** |
| 4 | `crates/xmtp_mls/src/groups/mod.rs:1868` (`add_members_by_identity`) | all supplied | **`MAX_GROUP_SIZE` is enforced *after* the RPC** (`mod.rs:1876`). Length mismatch → `GroupError::AddressNotFound(missing)` |
| 5 | `crates/xmtp_mls/src/groups/mod.rs:1963` (`remove_members_by_identity`) | all supplied | Unresolved identifiers **silently dropped**, unlike the add path |

Response mapping treats `IdentifierKind::Unspecified` as `Ethereum` and `filter_map`s out entries with `inbox_id: None` (`crates/xmtp_api/src/identity.rs:141-157`). See §4 for the d14n identifier-kind fidelity loss.

### 5.12 Identity: verify smart contract wallet signatures

| Field | Value |
| --- | --- |
| Call site | `RemoteSignatureVerifier::is_valid_signature` (`crates/xmtp_id/src/scw_verifier/remote_signature_verifier.rs:28-58`), call at line 35 |
| Items | **exactly 1 signature per call**, though the proto accepts a batch. `SmartContractSignatureVerifier::is_valid_signature` is a per-signature trait method, so there is no batching opportunity at this layer |
| Cursor | None. `block_number` is a caller-supplied pin |
| Error | Empty response array → `VerifierError::Io(InvalidData, "API returned empty response for signature verification request")` |
| Retry | 5× inside the wrapper |

**The amplification points.** Every `IdentityUpdate::to_verified` calls this once per SCW signature in the update, and those are fanned out with `try_join_all` and **no cap** at:

- `crates/xmtp_mls/src/identity_updates.rs:639-645` (`verify_updates`) — all updates for an inbox in parallel
- `crates/xmtp_mls/src/identity_updates.rs:175-182` (`batch_get_association_state_with_verifier`) — all inboxes in parallel
- `crates/xmtp_mls/src/identity_updates.rs:682` (`is_member_of_association_state`)
- `crates/xmtp_mls/src/groups/welcomes/validated_membership.rs:54` — every member's `get_association_state`

**Joining a group whose membership includes many smart-contract wallets can produce hundreds of concurrent single-signature verification RPCs.** Batching these would be a real win: the request proto already accepts `repeated`; only the client's trait shape prevents it.

On d14n this never reaches the backend — `D14nClient::verify_smart_contract_wallet_signatures` calls the local `scw_verifier` against an RPC provider (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:149-177`).

### 5.13 Streams

Two coexisting stacks.

#### 5.13.1 Legacy streams (default on all platforms)

**Group messages** — `StreamGroupMessages::new_with_factory` (`crates/xmtp_mls/src/subscriptions/stream_messages.rs:168-219`), call at line 208:

```rust
let cursors_by_group = db.get_last_cursor_for_ids(&groups,
    &[EntityKind::ApplicationMessage, EntityKind::CommitMessage])?;
let seen_cursors_vec = db.messages_newer_than(&cursors_by_group)?;
let seen_cursors: HashSet<_> = seen_cursors_vec.into_iter().map(|(_, cursor)| cursor).collect();
...
let subscription = api.subscribe_group_messages(&groups.iter().collect::<Vec<_>>()).await?;
```

| Field | Value |
| --- | --- |
| Items | **All N group ids in one subscription request.** No fan-out, no chunking |
| **Limit** | **None on the legacy path.** No `MAX_TOPICS`, no cap in `v3/streams.rs`, `d14n/streams.rs`, or `stream_messages.rs`. An account with 5,000 groups sends 5,000 filters in one frame |
| Cursor | Per-group from `refresh_state` (`ApplicationMessage` / `CommitMessage`) |
| Dedup | Client-side `seen_cursors` HashSet, seeded with locally-stored cursors newer than the durable cursor. **Grows unboundedly for the stream's life** — the source acknowledges this at `crates/xmtp_mls/src/subscriptions/types.rs:16`: *"if mem is a concern use a bloom filter or create a garbage collection strategy"* |

**Adding a new group tears down and re-opens the entire stream.** `StreamGroupMessages::add` (`stream_messages.rs:238-256`) only queues; `resolve_group_additions` (`stream_messages.rs:482`) transitions to `State::Adding` with a whole new subscription future, and `subscribe` (`stream_messages.rs:285-305`) re-dials via `subscribe_group_messages_with_cursors(&topic_cursor)` with **all** groups at their tracked cursors. The new group starts at `Cursor::new(1, 0)`.

This is triggered by `StreamAllMessages::poll_next` (`crates/xmtp_mls/src/subscriptions/stream_all.rs:208`) whenever the conversations stream yields a new group. **For an active account this is a repeated full re-subscribe** — precisely the problem XIP-83 bidi solves.

**Welcomes** — `StreamConversations::from_cow` (`crates/xmtp_mls/src/subscriptions/stream_conversations.rs:264-298`), call at line 282. Exactly **1 installation key**; multiplexed with the `LocalEvents` broadcast so locally-created groups also surface. Client-side dedup via `known_welcome_ids` seeded from `conn.group_cursors()`.

Each incoming welcome is processed on an **unbounded `JoinSet`** (`stream_conversations.rs:169, 296`), so the streaming path does **not** have the "stop after a retryable error" ordering guarantee that the sequential `sync_welcomes` loop has.

**Sync-on-subscribe.** `stream_all_messages` calls `WelcomeService::sync_welcomes()` **before** subscribing (`stream_all.rs:109`). `stream_conversations` and single-group `stream()` do **not** sync — they rely on cursor replay.

**Watchdog.** `crates/xmtp_mls/src/subscriptions/watchdog.rs`, rationale in the module docs:
> an L7 proxy keeps answering HTTP/2 keepalive pings while the backend subscription is gone... There is no server-side keepalive on the v3 path today.

| Constant | Value | Line |
| --- | --- | --- |
| `DEFAULT_IDLE_TIMEOUT` | 300s (5 min) | watchdog.rs:73 |
| `DEFAULT_RECONNECT_BASE` | 1s | watchdog.rs:73 |
| `DEFAULT_RECONNECT_JITTER` | 1000ms | watchdog.rs:73 |
| `MAX_IDLE_TIMEOUT` | 86400s | watchdog.rs:73 |
| `MAX_RECONNECT_BASE` | 3600s | watchdog.rs:73 |
| `MAX_RECONNECT_JITTER` | 3600s | watchdog.rs:73 |

Env knobs: `XMTP_STREAM_WATCHDOG_ENABLED`, `XMTP_STREAM_WATCHDOG_IDLE_TIMEOUT_SECS`, `XMTP_STREAM_WATCHDOG_RECONNECT_BASE_SECS`, `XMTP_STREAM_WATCHDOG_RECONNECT_JITTER_MS`.

**It is OFF by default** (`watchdog.rs:101`). When disabled, `WatchdogStream` is a pure passthrough. **So on the legacy path today, a silently-wedged stream hangs indefinitely.**

#### 5.13.2 XIP-83 bidi streams (implemented, native-only, opt-in)

**Gate** — `crates/xmtp_mls/src/subscriptions/router_callbacks.rs:104`:

```rust
pub const BIDI_STREAMS_ENABLED_ENV: &str = "XMTP_BIDI_STREAMS_ENABLED";
pub(crate) fn bidi_streams_enabled() -> bool {
    static ENABLED: LazyLock<bool> = LazyLock::new(|| { ... .unwrap_or(false) });
    *ENABLED
}
```

Read **once** per process via `LazyLock`. **No shipped config sets it**; only `bindings/mobile/src/mls/tests/lifecycle.rs:46,120,187` in tests. Native-only (`crates/xmtp_mls/src/subscriptions/mod.rs:41,51` gate `stream_router` and `router_callbacks` on `not(target_arch = "wasm32")`).

**Adding a group mutates in place, no reconnect.** `MessageConsumer::add_group` (`crates/xmtp_mls/src/subscriptions/stream_router.rs:1314`) takes a new `lease()`, which is a cursored `Mutate` add on the **same existing wire**. Per `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` module docs: *"Every `lease()` is a cursored `Mutate` wave... a cursored re-add is the one catch-up/replay mechanism (XIP-83)."*

**Bidi limits** — `crates/xmtp_api_d14n/src/queries/bidi_transport.rs:189`:

```rust
pub(crate) const MAX_MUTATE_TOPICS: usize = 1000;
pub(crate) const MAX_MUTATE_BYTES: usize = 16 * 1024 * 1024;  // under the 25 MiB encode limit
const PER_ENTRY_OVERHEAD: usize = 64;
```

`chunk_by_budget` / `chunk_mutate_adds` (`bidi_transport.rs:215, 247`) split on whichever of count or bytes is hit first. Other depths: `DEFAULT_LEASE_DEPTH = 64` (`bidi_transport.rs:166`), `DEFAULT_STREAM_DEPTH = 16` (`stream_router.rs:92`), `WIRE_BUFFER = 64`, `COMMAND_BUFFER = 64`, `EVENT_BUFFER = 1024`, `MAX_PENDING_FRAMES = 128` (`bidi.rs:36-78`).

**Keepalive** — `crates/xmtp_api_d14n/src/queries/bidi.rs:44`:

```rust
pub(crate) const DEFAULT_KEEPALIVE_MS: u32 = 30_000;   // XIP-83 client req 2 fallback
pub(crate) const PROBE_TIMEOUT_MULTIPLIER: u32 = 3;    // N from XIP-83 (recommended 2-3)
const WATCHDOG_NONCE: u64 = u64::MAX;
```

The connection actor auto-answers server `Ping`s; after `N - 1` intervals with no inbound frame it sends a watchdog ping, and after the full `N` it tears down. **The server's `Started` frame can advertise its own cadence, replacing the 30s fallback** — this is exactly `SubscribeResponse.Started.keepalive_interval_ms` in the draft.

**Reconnect** — `bidi_transport.rs:256`:

```rust
const RECONNECT_INITIAL_DELAY: Duration = Duration::from_millis(100);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);   // + up to a full extra base of jitter
const MIN_STABLE_UPTIME: Duration = Duration::from_secs(10);
const GRACEFUL_CLOSE_BUDGET: Duration = Duration::from_secs(5);
const OUTBOX_RETRY_INTERVAL: Duration = Duration::from_millis(25);
const WITHHELD_FRAMES_CAP: usize = 512;
```

Exponential doubling capped at 30s, **never gives up**. `MIN_STABLE_UPTIME` prevents an accept-then-RST server from pinning the client at the floor. **A wire flap is invisible to leases** — the transport re-opens and issues a *resume wave* re-adding every topic from `last_seen`.

**Capability negotiation: there is none.** Support is discovered by trying and classifying the failure. `is_bidi_unsupported` (`router_callbacks.rs:204`) and `open_is_backend_refusal` (`:221`) **walk the error source chain** (because `GrpcError` reports itself blanket-retryable) looking for `GrpcError::is_unimplemented` (gRPC `UNIMPLEMENTED`) or `ApiClientError::OtherUnretryable`. On refusal, `pump_stream` (`router_callbacks.rs:536`) latches the destination and serves that same stream on the legacy path in the same call.

Latches are **per destination URL**, held in the process-global `SHARED_WIRES` (`router_callbacks.rs:130-159`), and reset only with the process.

**Hard requirement on delivery tags** — `bidi_transport.rs` module docs:
> This client therefore requires a tag-serving backend (xmtp-node-go ≥ `6e0feb5f`, XIP-83 delivery tags)... its `CatchUpComplete` frames also carry no tag (`0`; ours are minted from 1), and the first one shuts the transport down.

An untagged backend is detected and tombstoned, then latched to legacy. **The draft's `CatchupComplete { uint64 mutate_id }` and `Messages { uint64 mutate_id }` are exactly these tags.** The draft's note "`0` only if a waveless Mutate carried 0" is consistent with the client minting from 1.

Also implemented: `suspend_bidi_streams` / `resume_bidi_streams` (`router_callbacks.rs:312, 346`) for app lifecycle, and `Client::catch_up_to_live` (`crates/xmtp_mls/src/subscriptions/catch_up.rs`), a bounded `history_only` one-shot sync on its own connection — **matching the draft's `Mutate.history_only`.**

#### 5.13.3 Ordering and dedup in streams

**d14n has a re-ordering buffer.** `crates/xmtp_api_d14n/src/queries/stream/ordered.rs:60` implements XIP-49 cross-originator ordering per envelope batch, iceboxing envelopes whose `depends_on` vector clock is not dominated by the topic clock. It uses a `NoopResolver` — the doc comment: *"there is an implicit assumption that if an item in the stream is required for processing, it will at some point be made available in the stream."* **Welcomes skip `ordered`** (`d14n/streams.rs:113-116`).

**v3 and bidi have no re-ordering buffer.** v3 goes straight through `try_from_stream`. Bidi guarantees ordering at the wire: *"the server serves live frames and each wave's replay in total cursor order per kind"*, *"strictly increasing above the lease's floor, nothing at-or-below it"*.

**Deduplication has four layers:**

| Layer | Where | Mechanism |
| --- | --- | --- |
| (a) DB fast path — **the primary stream/query dedup** | `crates/xmtp_mls/src/subscriptions/process_message.rs:139` | `if let Some(stored) = factory.retrieve(&msg)? { return Prepared::Ready { ... } }` — an envelope already stored by a query-path sync is returned **without decrypting** |
| (b) Legacy per-stream seen-set | `stream_messages.rs:189`, checked at `:436` | Unbounded `HashSet<Cursor>` |
| (c) Bidi `StreamDedup` | `stream_router.rs:314` | `seen` (scoped to a per-topic catch-up window closed by `CatchUpComplete`) + `surfaced_ahead` (a recovery sync surfaced a message ahead of its envelope) |
| (d) Welcome dedup | `stream_conversations.rs:287`, `stream_router.rs:737` | `known_welcome_ids` from `db.group_cursors()` |

**No explicit sequence-gap detector exists.** Gaps are handled implicitly: the stream subscribes at the durable cursor so anything missed is replayed; and `ProcessMessageFuture` triggers a **recovery sync** when a message arrives it cannot process in order (`process_message.rs:170`).

**Concurrency between streams and syncs.** There is **no global sync lock — the lock is per-group.** `GroupCommitLock` (`crates/xmtp_mls/src/lib.rs:50`):

```rust
pub struct GroupCommitLock { locks: Mutex<HashMap<GroupId, Arc<TokioMutex<()>>>> }
pub async fn get_lock_async(&self, group_id: GroupId) -> MlsGroupGuard { ... lock.lock_owned().await }
pub fn get_lock_sync(&self, group_id: GroupId) -> Result<MlsGroupGuard, GroupError> {
    lock.try_lock_owned().map_err(|_| GroupError::LockUnavailable)   // fails fast
}
```

A stream's `process_one` and a concurrent `sync_all_groups` on the same group serialize here; different groups proceed fully in parallel. Correctness under that concurrency comes from the DB fast path (a), not from mutual exclusion.

**Streams per client:** a typical mobile client running `stream_all_messages` holds **2 network subscriptions** (`StreamGroupMessages` + `StreamConversations`). On the bidi path all of them collapse onto **one process-shared wire per destination**.

Bidi welcome processing is capped (`stream_router.rs:98`): `MAX_WELCOME_TASKS = 16`, `MAX_WELCOME_BACKLOG = 512`; overflow ends the stream so the consumer re-subscribes from durable state.

### 5.14 Device sync / archive

Device sync moves history between a user's own installations. Its archive payloads go to a **separate history server**, not the message backend:

```rust
// crates/xmtp_configuration/src/common/api.rs:20-24
impl DeviceSyncUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://0.0.0.0:5558";
    pub const DEV_ADDRESS: &'static str = "https://message-history.dev.ephemera.network";
    pub const PRODUCTION_ADDRESS: &'static str = "https://message-history.ephemera.network";
}
```

Its coordination happens **through an ordinary MLS sync group** — `all_sync_groups()` in `sync_all_welcomes_and_history_sync_groups` (`crates/xmtp_mls/src/groups/welcome_sync.rs:305-327`) syncs them with the same `query_group_messages` path as any other group. The device-sync worker is poked via `SyncWorkerEvent::NewSyncGroupMsg`.

**So device sync places no new requirements on the message backend beyond ordinary group messaging.** **[UNVERIFIED: whether the history server is in scope for the self-hosted project. `project.md` does not mention it.]**

### 5.15 v4-only endpoints

| Endpoint | Used by the client? | Where |
| --- | --- | --- |
| `ReplicationApi/GetNewestEnvelope` | **Yes** — backs `fetch_key_packages`, `query_latest_group_message`, `get_newest_group_message` on d14n | `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs` |
| `ReplicationApi/QueryEnvelopes` | **Yes** — backs group/welcome/identity queries on d14n | `crates/xmtp_api_d14n/src/endpoints/d14n/query_envelopes.rs` |
| `ReplicationApi/SubscribeTopics` | **Yes** — backs both d14n stream methods | `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs` |
| `ReplicationApi/GetInboxIds` | **Yes** — on both `V3Client` and `D14nClient` | `crates/xmtp_api_d14n/src/endpoints/d14n/get_inbox_ids.rs` |
| `PayerApi/PublishClientEnvelopes` | **Yes** — every d14n write | `crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs` |
| `PayerApi/GetNodes` (`/xmtp.xmtpv4.payer_api.PayerApi/GetNodes`) | **Yes** — node discovery for `MultiNodeClient` | `crates/xmtp_api_d14n/src/endpoints/d14n/get_nodes.rs:21-23` |
| `grpc.health.v1.Health/Check` (the standard gRPC health probe, not an XMTP RPC) | **Yes** — fastest-node selection | `crates/xmtp_api_d14n/src/endpoints/d14n/health_check.rs:41-46` |
| `D14nMigrationApi/FetchD14nCutover` | Yes, but only from `MigrationClient` | `crates/xmtp_api_d14n/src/endpoints/d14n/fetch_d14n_cutover.rs` |
| `metadata_api` | **No caller found** | generated only |
| `depends_on` in `AuthenticatedData` | **Yes** — stamped on every d14n group-message publish | `crates/xmtp_api_d14n/src/queries/d14n/mls.rs:91-106` |

**Crucially, the `MigrationClient` / cutover machinery is unreachable from every shipped SDK.** All five binding entry points call `build_optional_d14n()`; only `MessageBackendBuilder::build()` produces `ClientBundle::Migration`, and nothing in `bindings/` calls it. `choose_client` / `refresh_cutover` in `crates/xmtp_api_d14n/src/queries/combined.rs` are dormant for app developers. **All of it is deletable.**

---

## 6. Client Ordering and Cursor Invariants

### 6.1 The `refresh_state` table

`crates/xmtp_db/src/encrypted_store/refresh_state.rs:100-107`:

```rust
#[diesel(table_name = refresh_state)]
#[diesel(primary_key(entity_id, entity_kind, originator_id))]
pub struct RefreshState {
    pub entity_id: Vec<u8>,
    pub entity_kind: EntityKind,
    pub sequence_id: i64,
    pub originator_id: i32,
}
```

`EntityKind` (`refresh_state.rs:24-35`):

| Variant | Value | Tracks | `entity_id` is |
| --- | --- | --- | --- |
| `Welcome` | 1 | welcome messages | installation key |
| `ApplicationMessage` | 2 | application messages (originator 10) | group id |
| `CommitLogUpload` | 3 | rowid of the last local entry uploaded to the remote commit log | group id |
| `CommitLogDownload` | 4 | server log sequence id of the last remote entry downloaded | group id |
| `CommitLogForkCheckLocal` | 5 | last rowid verified in the local commit log | group id |
| `CommitLogForkCheckRemote` | 6 | last rowid verified in the remote commit log | group id |
| `CommitMessage` | 7 | MLS commit messages (originator 0) | group id |

`HasEntityKind for GroupMessage` (`refresh_state.rs:40-49`) routes a message to `CommitMessage` or `ApplicationMessage` based on `is_commit()`. **So a single group topic maps to two independently-tracked cursors on the client, distinguished by originator id (0 vs 10 on v3).** A single-originator backend collapses this — but the client's `EntityKind` split is a local storage detail it can keep.

**Cursor writes are monotonic by construction** (`refresh_state.rs`, `fn update_cursor`):

```rust
diesel::insert_into(dsl::refresh_state)
    .values(&state)
    .on_conflict((dsl::entity_id, dsl::entity_kind, dsl::originator_id))
    .do_update()
    .set(dsl::sequence_id.eq(excluded(dsl::sequence_id)))
    .filter(dsl::sequence_id.lt(excluded(dsl::sequence_id)))
    .execute(conn)
```

The `.filter(sequence_id.lt(excluded))` clause means **a cursor can never move backwards.** The function returns `bool` for whether the row actually advanced.

### 6.2 Other cursor-bearing state

| Table | Column(s) | Meaning |
| --- | --- | --- |
| `identity_updates` | `sequence_id`, `originator_id`, `server_timestamp_ns` | per-inbox identity-update log position; `insert_or_ignore` on write |
| `group_intents` | `state`, `payload_hash`, `staged_commit`, `post_commit_data`, `publish_attempts`, `sequence_id` | the publish-then-verify state machine |
| `groups` | `last_message_ns` / conversation-list ordering fields, `installations_time_checked` | local ordering + the 30-minute installation-refresh throttle |
| `identity` | `registration_cursor_originator_id`, `registration_cursor_sequence_id`, `next_key_package_rotation_ns` | registration visibility poll; rotation deadline |
| local commit log | sqlite `rowid` | ascending from 1 |
| `key_package_history` | `delete_at_ns` | local key-package retirement |

### 6.3 What the client assumes about sequence ids

| Assumption | Evidence | Verdict |
| --- | --- | --- |
| **Ascending within a response** | the cursor advances as `process_messages` walks the list (`mls_sync.rs:2845-2900`) | **required** |
| **Monotonic across responses** | `update_cursor`'s `.filter(lt(excluded))` and the `last_cursor >= sequence_id` skip | **required** |
| **Duplicates are safe** | early-return "Message already processed"; `insert_or_ignore` for identity updates; four dedup layers on streams | **tolerated** |
| **No gaps** | no gap detector; a gap silently advances the cursor past the missing message | **required — the client cannot recover** |
| **Sequence ids are dense (contiguous)** | nothing in the client requires `n+1`; only `>` comparisons | **not required** |
| **Sequence ids are global vs per-topic** | on v3 they are per-originator-stream; on d14n per-originator; `TopicQuery` in the draft makes them per-topic | either works, provided ordering holds per topic |
| **Stream and query share one sequence space per topic** | `seen_cursors` dedup compares stream cursors against `messages_newer_than(cursors_by_group)` from the query path (`stream_messages.rs:189`); the bidi lease floor is the same durable `refresh_state` cursor | **required** |
| **A published message returns byte-identical** | the extractor computes `sha256_bytes(message.data)` into `envelope.payload_hash` (`crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs:73-125`), and `mls_sync` matches the intent on it (`mls_sync.rs:2461-2463`) | **required** |

### 6.4 The `depends_on` / icebox mechanism

The d14n path is the only place the client can *detect* a gap, and it does so causally rather than by sequence number. `Ordered::order()` (`crates/xmtp_api_d14n/src/protocol/order.rs`):

1. `recover_lost_children()` — pull anything out of the icebox whose parents just arrived
2. `timestamp_sort()`
3. `causal_sort()` in a loop — anything whose `depends_on` clock is not dominated by the topic clock is "missing"
4. try to resolve missing parents through the resolver
5. if unresolvable, `store.ice(orphans)` and stop

With a single originator and a strict per-topic total order, steps 3–5 are unnecessary: sequence order *is* causal order. **`order.rs`, `sort/`, `resolve/`, `in_memory_cursor_store.rs`, the icebox tables, and `find_message_dependencies` all become deletable.**

---

## 7. Environment and Backend Selection

### 7.1 There is no `ApiUrls`, and `builder.rs` selects nothing

`crates/xmtp_mls/src/builder.rs` contains **no env or URL selection**. There is no `ApiUrls` type anywhere in the repository. `ClientBuilder` (`builder.rs:96-113`) takes an already-constructed `api_client: Option<ApiClient>` and errors `MissingParameter { parameter: "api_client" }` (`builder.rs:290-294`) if absent. It wraps it once:

```rust
// crates/xmtp_mls/src/builder.rs:312
let api_client = ApiClientWrapper::new(api_client, Retry::default());
```

All backend selection lives in `crates/xmtp_api_d14n/src/queries/client_bundle.rs`.

### 7.2 v3 vs d14n: the gateway host is the switch

`ClientBundleBuilder` produces one of three `ClientBundle` variants (`client_bundle.rs:17-27`): `D14n`, `V3`, `Migration { v3, xmtpd }`.

| Method | File:line | Behaviour |
| --- | --- | --- |
| `build_v3()` | `client_bundle.rs:277-279` | V3 only. Errors `MissingV3Host` |
| `build_d14n()` | `client_bundle.rs:248-251` | d14n only. Errors `MissingGatewayHost` |
| `build()` | `client_bundle.rs:284-288` | Both → `Migration`. Requires both hosts |
| **`build_optional_d14n()`** | `client_bundle.rs:293-304` | **`if gateway_host.is_some() { d14n } else { v3 }`** |

**Every binding calls `build_optional_d14n()`.** `MessageBackendBuilder` documents the rule at `crates/xmtp_api_d14n/src/queries/builder.rs:20-21`: *"Passing a gateway host implicitly enables decentralization."*

### 7.3 `XmtpEnv` and its gap

`crates/xmtp_configuration/src/common/env.rs:12-53`. Variants: `Local`, `Dev`, `Production`, `TestnetStaging`, `TestnetDev`, `Testnet`, `Mainnet`.

```rust
// env.rs:37-44
fn default_api_url(&self) -> Option<&'static str> {
    Local => GrpcUrlsLocal::NODE, Dev => GrpcUrlsDev::NODE, Production => GrpcUrlsProduction::NODE,
    /* all four d14n variants */ => None,
}
```

`ClientBundleBuilder::env()` (`client_bundle.rs:162-169`) is a **fallback only** and sets **only the v3 host**:

```rust
self.v3_host = self.v3_host.take().or_else(|| env.default_api_url().map(Into::into));
```

An explicit `v3_host()` always wins.

**⚠ `XmtpEnv` cannot express a d14n deployment.** All four d14n variants return `None`, and no gateway constant is reachable from the env path. `GrpcUrlsStaging/Dev/Production::GATEWAY` exist in `xmtp_configuration` but **nothing in the production path reads them** — they are used only by tests via the feature-gated `GrpcUrls` alias. Selecting `env: Testnet` and nothing else fails with `MissingV3Host`. **App developers must hard-code the payer URL today.**

This is directly relevant: the new project replaces `env` with "URL of the backend", which fixes a real existing defect.

### 7.4 Read/write split and node discovery

`inner_build_d14n()` (`client_bundle.rs:171-243`) wires three hosts:

1. Gateway client for `gateway_host` (required)
2. `AuthMiddleware` wrapping the gateway if `auth_callback` or `auth_handle` is set (`client_bundle.rs:195-204`) — **auth is applied to the gateway only, never to node clients**
3. Read side: a pinned `GrpcClient` if `xmtpd_host` is set, else a `MultiNodeClient`
4. `ReadWriteClient { read: xmtpd, write: gateway_client, filter: PAYER_WRITE_FILTER }`

The split is a **path substring match** on `"xmtp.xmtpv4.payer_api.PayerApi"` in `ReadWriteClient::request/stream/bidi_stream` (`crates/xmtp_api_d14n/src/middleware/read_write_client/client.rs:52, 65, 78`). `ReadWriteClient::host()` returns the **read** host.

**Node discovery** — `MultiNodeClient::init_inner()` (`crates/xmtp_api_d14n/src/middleware/multi_node_client/client.rs:45-53`) runs **once, lazily, on first request** via `OnceCell::get_or_try_init`:

1. `/xmtp.xmtpv4.payer_api.PayerApi/GetNodes` against the gateway, wrapped in `api::retry`. Empty list → `NoNodesFound`
2. `/grpc.health.v1.Health/Check` — the standard gRPC health-checking probe, not an XMTP-specific RPC — to every node concurrently, keeping the **lowest-latency** responder. Timeout `MULTI_NODE_TIMEOUT_MS = 30_000`

**The chosen node is never re-selected.** A TODO at `client.rs:41-43` notes a `refresh()` would need `OnceCell<ArcSwap<GrpcClient>>`. If the fastest node degrades mid-session, the client stays pinned for the process lifetime. `MultiNodeClient::host()` deliberately returns the **gateway** host: *"host() is a connection identity and must not change over the client's lifetime, while the resolved node is a transient the gateway can re-issue."*

**All of `MultiNodeClient`, `ReadWriteClient`, `GetNodes`, `HealthCheck`, and `PAYER_WRITE_FILTER` become deletable with a single self-hosted backend.**

### 7.5 Auth

`AuthMiddleware` (`crates/xmtp_api_d14n/src/middleware/auth.rs:86-161`):

- `Credential { name: HeaderName, value: HeaderValue, expires_at_seconds: i64 }`; `Credential::new` defaults `name` to `http::header::AUTHORIZATION`
- `get_credential()` (`auth.rs:110-146`) lazily initializes via `OnceCell::get_or_try_init`, refreshes when `expires_at_seconds <= now_secs()`, guarded by a `tokio::sync::Mutex` with double-check
- Handle-only mode never refreshes: *"expired credentials will still be used until the credential is set"* (`auth.rs:82`)
- `modify_request()` appends the header, applied identically in `request`, `stream`, and `bidi_stream` (`auth.rs:169-200`)
- **`AuthMiddleware::new` asserts** (panics) if neither callback nor handle is present (`auth.rs:100-103`)

Separately, **every** `GrpcClient` request appends two metadata headers unconditionally (`crates/xmtp_api_grpc/src/grpc_client/client.rs:95-99`):

- `x-app-version` — default `"0.0.0"`
- `x-libxmtp-version` — default `env!("CARGO_PKG_VERSION")`

The comment at `client.rs:96` notes: *"must be lowercase otherwise panics"*.

**This is the existing auth mechanism the project's "some new client authentication mechanisms" can build on** — the header-injection middleware already exists and already covers unary, server-stream, and bidi.

### 7.6 Readonly / notification mode

`ReadonlyClient` (`crates/xmtp_api_d14n/src/middleware/readonly_client.rs:12-21`) rejects any path containing: `UploadKeyPackage`, `RevokeInstallation`, `BatchPublishCommitLog`, `SendWelcomeMessages`, `RegisterInstallation`, `PublishIdentityUpdate`, `PublishClientEnvelopes`, `PublishCommitLog` — returning `ApiClientError::WritesDisabled`. Bindings expose this as `ClientMode::Notification`.

**The new backend needs an equivalent write-suppression story**, since `backend.proto` collapses all writes into one `Publish` RPC — a path-based filter can no longer distinguish them. The client-side filter still works (block `PublishService/Publish` entirely), but the granularity is lost.

### 7.7 The d14n cutover (dormant)

`MigrationClient::choose_client()` (`crates/xmtp_api_d14n/src/queries/combined.rs:74-97`):

```rust
if self.store.has_migrated()? { return Ok(&self.xmtpd_client); }
let cutover_ns = if time_since_refresh >= CUTOVER_REFRESH_TIME || self.always_check_once.set(()).is_ok()
    { self.refresh_cutover().await? } else { cutover_ns };
if now >= cutover_ns { self.store.set_has_migrated(true)?; Ok(&self.xmtpd_client) } else { Ok(&self.v3_client) }
```

`has_migrated` is **sticky** — one-way. `refresh_cutover()` queries `/xmtp.migration.api.v1.D14nMigrationApi/FetchD14nCutover` **against the v3 client**. Default when unknown is `i64::MAX`, so an unreachable cutover query keeps the client on v3.

Server-forced fallback: `write_with_refresh()` (`combined.rs:109-125`) retries a failed write once if the error matches `D14N_MIGRATION_MSG_REGEX`, setting `has_migrated = true` first. Applied to `upload_key_package`, `send_group_messages`, `send_welcome_messages`, `publish_identity_update`. Reads are not wrapped.

**As established in §5.15, this whole path is unreachable from any shipped SDK.**

### 7.8 Bindings

| Binding | env enum | v3 host | gateway host | `is_secure` | readonly | auth |
| --- | --- | --- | --- | --- | --- | --- |
| mobile `connect_to_backend` (`bindings/mobile/src/mls.rs:145-189`) | **no** | `v3_host: String` (**required**) | `Option<String>` | no (scheme-inferred) | `FfiClientMode::Notification` | callback + handle |
| node `createClient` (`bindings/node/src/client/create_client.rs:286-335`) | no | `v3_host: String` (required) | `Option<String>` | no | `ClientMode::Notification` | callback + handle |
| node `BackendBuilder` (`bindings/node/src/client/backend.rs:9-97`) | **yes** (required) | `api_url: Option<String>` | `Option<String>` | no | `readonly: Option<bool>` | callback + handle |
| wasm `createClient` (`bindings/wasm/src/client.rs:429-465`) | no | `host: String` (required) | `Option<String>` | no | `ClientMode::Notification` | callback + handle |
| wasm `BackendBuilder` (`bindings/wasm/src/client/backend.rs:7-69`) | **yes** (required) | `api_url: Option<String>` | `Option<String>` | no | `readonly: Option<bool>` | callback + handle |

`is_secure` **does not exist anywhere in the repo** — TLS is inferred from the URL scheme (`https` or `grpcs`) by `is_url_secure` (`crates/xmtp_api_grpc/src/grpc_client/native.rs:93-95`).

The mobile doc comment (`bindings/mobile/src/mls.rs:139-142`) states the rule plainly: *"connect to the XMTP backend / specifying `gateway_host` enables the D14n backend and assumes `host` is set to the correct d14n backend url."*

**Mobile has no `env` at all** — `XmtpEnv` is not bridged over FFI, so mobile apps hard-code every URL. Only node and wasm's `BackendBuilder` expose `env`. **This makes the project's "URL of the backend instead of `env`" change nearly a no-op for mobile, and a simplification for node/wasm.**

---

## 8. Configuration Constants

All from `crates/xmtp_configuration/src/`. **Structural warning** (`src/lib.rs:1-14`): the crate exports `common::*` always, then **either** `test::*` (when `cfg(any(test, feature = "test-utils"))`) **or** `prod::*`. Three constants have different values in the two builds. Any crate in the dependency graph enabling `test-utils` silently reconfigures a release build.

Time units (`crates/xmtp_common/src/const.rs:1-9`): `NS_IN_SEC = 1_000_000_000`, `NS_IN_MIN`, `NS_IN_HOUR`, `NS_IN_DAY`, `NS_IN_30_DAYS`.

### 8.1 API limits and sizes

| Name | Value | Type | File:line |
| --- | --- | --- | --- |
| `GRPC_PAYLOAD_LIMIT` | `1024 * 1024 * 25` = **26,214,400** (25 MiB) | `usize` | `common/api.rs:12` |
| `MAX_PAGE_SIZE` (**prod**) | **100** | `u32` | `prod/api.rs:2` |
| `MAX_PAGE_SIZE` (**test**) | **20** | `u32` | `test/api.rs:4` |
| `MAX_GROUP_SIZE` | **250** | `usize` | `common/mls.rs:29` |
| `MAX_INSTALLATIONS_PER_INBOX` | **10** | `usize` | `common/mls.rs:31` |
| `MAX_PAST_EPOCHS` | **3** | `usize` | `common/mls.rs:33` |
| `INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING` | **2** | `usize` | `common/mls.rs:70` |
| `MAX_DB_POOL_SIZE` | 25 | `u32` | `common/db.rs:8` |
| `MIN_DB_POOL_SIZE` | 5 | `u32` | `common/db.rs:12` |

`GRPC_PAYLOAD_LIMIT` is applied at `crates/xmtp_api_grpc/src/grpc_client/client.rs:302-303` as both `max_decoding_message_size` and `max_encoding_message_size`. It also drives the welcome chunk size (`crates/xmtp_mls/src/groups/mls_sync.rs:4640`) and the per-welcome size fallback (`:4634`, = 104,857 bytes).

### 8.2 Timeouts and intervals

| Name | Value | Type | File:line |
| --- | --- | --- | --- |
| `MULTI_NODE_TIMEOUT_MS` | **30_000** (30s) | `u64` | `common/api.rs:17` |
| `BUSY_TIMEOUT` | 5_000 (5s) | `i32` | `common/db.rs:10` |
| `WORKER_RESTART_DELAY` | `Duration::from_secs(1)` | `Duration` | `common/mls.rs:8` |
| `CUTOVER_REFRESH_TIME` | `NS_IN_HOUR * 6` (6h) | `i64` | `common/d14n.rs:23` |
| `GROUP_KEY_ROTATION_INTERVAL_NS` | `NS_IN_30_DAYS` | `i64` | `common/mls.rs:18` |
| `KEY_PACKAGE_QUEUE_INTERVAL_NS` | `5 * NS_IN_SEC` | `i64` | `common/mls.rs:20` |
| `KEY_PACKAGE_ROTATION_INTERVAL_NS` | `NS_IN_30_DAYS` (30 days) | `i64` | `common/mls.rs:25` |
| `SEND_MESSAGE_UPDATE_INSTALLATIONS_INTERVAL_NS` | `5 * NS_IN_SEC` | `i64` | `common/mls.rs:27` |
| `SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS` (**prod**) | `NS_IN_HOUR / 2` (**30 min**) | `i64` | `prod/mls.rs:3` |
| `SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS` (**test**) | `NS_IN_SEC` (**1 s**) | `i64` | `test/mls.rs:5` |
| `KEYS_EXPIRATION_INTERVAL_NS` (**prod**) | `NS_IN_DAY` (1 day) | `i64` | `prod/mls.rs:5` |
| `KEYS_EXPIRATION_INTERVAL_NS` (**test**) | `3 * NS_IN_SEC` | `i64` | `test/mls.rs:7` |

**There is no message-expiry / disappearing-message TTL constant.** Disappearing-message lifetimes are per-group settings carried in group metadata, not global config, and are enforced client-side.

### 8.3 Retry and sync backoff

| Name | Value | Type | File:line |
| --- | --- | --- | --- |
| `MAX_GROUP_SYNC_RETRIES` | **3** | `usize` | `common/mls.rs:14` |
| `MAX_INTENT_PUBLISH_ATTEMPTS` | **3** | `usize` | `common/mls.rs:16` |
| `SYNC_BACKOFF_WAIT_MS` | 50 | `u16` | `common/mls.rs:73` |
| `SYNC_BACKOFF_TOTAL_WAIT_MAX_SECS` | 10 | `u16` | `common/mls.rs:75` |
| `SYNC_JITTER_MS` | 25 | `u16` | `common/mls.rs:77` |

Plus the generic API retry (`crates/xmtp_common/src/retry.rs:78-81, 152-155`): **5 retries, 50ms base, ×3 multiplier, 25ms jitter, 30s individual max, 120s total max.**

### 8.4 Concurrency limits (found in code, not config)

| Limit | Value | Where | Scope |
| --- | --- | --- | --- |
| `MAX_CONCURRENT_KEY_PACKAGE_FETCHES` | 16 | `crates/xmtp_mls/src/groups/mod.rs:735` | **the only explicit limit in the whole API surface**, and only on the `installation_extensions` fallback |
| welcome chunk size | `.clamp(1, 50)` | `crates/xmtp_mls/src/groups/mls_sync.rs:4640` | **not a concurrency limit** — every chunk is in flight at once; a chunk size of 1 means one RPC per welcome |
| `sync_groups_in_batches` concurrency | 10 (hardcoded arg) | `crates/xmtp_mls/src/groups/welcome_sync.rs:427` | `sync_all_welcomes_and_groups` only |
| `get_newest_message_metadata` batch | 1000 | `crates/xmtp_api/src/mls.rs` | chunks fired concurrently, uncapped |
| `GET_IDENTITY_UPDATES_CHUNK_SIZE` | 50 | `crates/xmtp_api/src/identity.rs:22` | chunks fired concurrently, uncapped |
| `publish_commit_log` batch | 10 | `crates/xmtp_api/src/mls.rs` | sequential |
| `query_commit_log` batch | 20 | `crates/xmtp_api/src/mls.rs` | sequential |
| `MAX_MUTATE_TOPICS` | 1000 | `crates/xmtp_api_d14n/src/queries/bidi_transport.rs:189` | bidi only |
| `MAX_MUTATE_BYTES` | 16 MiB | `crates/xmtp_api_d14n/src/queries/bidi_transport.rs:189` | bidi only |
| `MAX_WELCOME_TASKS` / `MAX_WELCOME_BACKLOG` | 16 / 512 | `crates/xmtp_mls/src/subscriptions/stream_router.rs:98` | bidi only |

**Places with NO limit**: `sync_all_groups`'s `FuturesUnordered` over every group; the legacy stream's subscribe-frame group count; the legacy welcome-stream `JoinSet`; all SCW verification fan-out; `fetch_key_packages` request cardinality; `get_inbox_ids` request cardinality.

### 8.5 gRPC transport

`crates/xmtp_api_grpc/src/grpc_client/native.rs`:

| Setting | Value | Line |
| --- | --- | --- |
| rate limit default | **5000/minute** (`limit.unwrap_or(5000)`, applied as `.rate_limit(limit, Duration::from_secs(60))`) | :100, :103, :160 |
| `initial_connection_window_size` | 2,147,483,647 | :132 |
| `connect_timeout` | 10s | :140 |
| request `timeout` | **120s** | :148 |
| `XMTP_GRPC_KEEPALIVE_INTERVAL_SECS` → `http2_keep_alive_interval` | 45s | :45 |
| `XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS` → `keep_alive_timeout` | 20s | :46 |
| `XMTP_GRPC_TCP_KEEPALIVE_SECS` → `tcp_keepalive` | 45s | :47 |
| `XMTP_GRPC_KEEPALIVE_WHILE_IDLE` | true | :70-72 |

The keepalive doc (`native.rs:13-35`) records why these were loosened from 16s/10s: tight deadlines *"tore down otherwise-healthy long-lived connections whenever one PING ack straggled"* — reconnect churn and a 2026-08 production incident. 45s *"stay[s] inside common 60s middlebox idle timers"*; worst-case detection is now ~65s.

**`ClientBuilder::retry` is declared (`client.rs:253`) and never read (`:296-312`)** — `NetConnectConfig::set_retry` on a gRPC builder is a silent no-op.

WASM (`grpc_client/wasm.rs`): `_limit` **ignored** — no rate limiting in the browser. Trailing `/` stripped (*"envoy does *not* like trailing /"*). `bidi_stream` unavailable.

### 8.6 URLs

| Env | v3 NODE (native) | v3 NODE (wasm) | XMTPD | GATEWAY |
| --- | --- | --- | --- | --- |
| Local | `http://localhost:5556` | `http://localhost:5557` | `http://localhost:5050` | `http://localhost:5052` |
| Dev | `https://grpc.dev.xmtp.network:443` | `https://api.dev.xmtp.network:5558` | `https://grpc.testnet-dev.xmtp.network:443` | `https://payer.testnet-dev.xmtp.network:443` |
| Staging | `https://grpc.dev.xmtp.network:443` | `https://api.dev.xmtp.network:5558` | `https://grpc.testnet-staging.xmtp.network:443` | `https://payer.testnet-staging.xmtp.network:443` |
| Production | `https://grpc.production.xmtp.network:443` | `https://api.production.xmtp.network:5558` | `https://grpc.testnet.xmtp.network:443` | `https://payer.testnet.xmtp.network:443` |

Naming trap: the "Production" d14n URLs are `testnet.xmtp.network`. There is no `GrpcUrlsMainnet`, yet `XmtpEnv::Mainnet` exists. Also `ToxiProxy` URLs at `common/api.rs:139-153` (ports 6010-6060) for fault-injection tests.

---

## 9. Requirements the New Backend Must Meet (EARS)

Derived strictly from client code. Each is cited.

### Ordering and cursors

**R1.** When the backend returns envelopes for a topic query, the backend **shall** return them in ascending `sequence_id` order **within each cursor partition** — that is, within each `(entity_id, entity_kind, originator_id)` triple the client cursors on.
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:2845-2900` (`process_messages` advances the cursor as it walks the list); `crates/xmtp_mls/src/groups/welcome_sync.rs:165-207`. **Scope note:** the cited code does **not** require one total order across a whole group topic. `maybe_update_cursor` compares against `get_last_cursor_for_originator(group_id, envelope.entity_kind(), envelope.originator_id())` (`crates/xmtp_mls/src/groups/mls_sync.rs:1465-1482`), and `refresh_state` is keyed by that same triple (`crates/xmtp_db/src/encrypted_store/refresh_state.rs:100-107,357-383`). Commit messages (`CommitMessage`, originator 0) and application messages (`ApplicationMessage`, originator 10) advance **independent** cursors, so an interleaving that is descending across the two partitions is harmless today. A single-sequence backend that gives one total order per topic satisfies this requirement strictly — it is a legal design, just not one the current client forces.

**R2.** When the backend returns envelopes for a topic query with cursor `C`, the backend **shall** return every envelope on that topic with `sequence_id > C` up to the response limit, with no omissions.
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:2918-2936` (`maybe_update_cursor` advances past everything returned; there is no gap detector, so an omitted envelope is lost permanently).

**R3.** The backend **shall** assign each envelope a `sequence_id` that is strictly greater than every `sequence_id` previously made visible **in the same cursor partition** (topic plus, on the current wire, message class).
*Cite:* `crates/xmtp_db/src/encrypted_store/refresh_state.rs:357-383`, `fn update_cursor` — `.filter(dsl::sequence_id.lt(excluded(dsl::sequence_id)))` makes cursor writes monotonic, and the conflict target is `(entity_id, entity_kind, originator_id)`, so monotonicity is enforced per partition rather than per topic. A lower id after a higher one **in the same partition** is silently discarded and the envelope is dropped. Across partitions there is no constraint: a commit at sequence 4 may follow an application message at sequence 9 on the same group with no ill effect. A single-sequence-per-topic backend is the stronger design and satisfies this; the current client only requires the weaker per-partition form.

**R4.** When an envelope with `sequence_id = N` is visible to a reader, the backend **shall** ensure every envelope **in the same cursor partition** with `sequence_id < N` is also visible to that reader.
*Cite:* R2's rationale; `crates/xmtp_mls/src/groups/mls_sync.rs:2244-2265` and `:1465-1482` (a reader that sees N advances the cursor for that `(entity_id, entity_kind, originator_id)` triple past everything below it, and nothing later re-reads below the cursor). The partition qualifier matters for the same reason as R1 and R3: `refresh_state` holds a separate cursor for commits and for application messages (`crates/xmtp_db/src/encrypted_store/refresh_state.rs:100-107`), so visibility must be prefix-closed per partition. If the new backend gives one sequence per topic, the partition and the topic coincide and the requirement reads as originally stated.

**R5.** The backend **shall** serve the same `sequence_id` for a given envelope on a given topic through the query path and the subscription path.
*Cite:* `crates/xmtp_mls/src/subscriptions/stream_messages.rs:189` (`seen_cursors` seeded from `db.messages_newer_than(&cursors_by_group)` — query-path cursors — and compared against stream cursors); `crates/xmtp_mls/src/subscriptions/stream_router.rs:720` (the bidi lease floor is the durable `refresh_state` cursor written by the query path).

**R6.** The backend **may** deliver a duplicate envelope; the client tolerates it.
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:2244` ("Message already processed" early return); `crates/xmtp_mls/src/identity_updates.rs:630` (`insert_or_ignore_identity_updates`); four stream dedup layers (§5.13.3).

**R7.** The backend **should** accept a distinct cursor per topic within one multi-topic query, to remove over-fetch.
*Cite:* `crates/xmtp_api_d14n/src/queries/d14n/identity.rs:75-100` — the current d14n implementation collapses N per-inbox cursors into a single shared `last_seen = min(sequence_id)` across up to 50 inbox ids and is **still correct**, because `insert_or_ignore_identity_updates` absorbs the re-delivered rows (`crates/xmtp_mls/src/identity_updates.rs:617-630`). So this is an efficiency requirement, not a correctness one: per-topic cursors avoid re-fetching everything above the least-advanced inbox in the batch. The draft's `TopicQuery { Topic, Cursor }` satisfies it and is strictly better than today.

### Publish

**R8.** When the client publishes a group message and later queries the same topic, the backend **shall** return the message payload byte-identical to what was published.
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:3157-3161` (`intent_hash = sha256(last_payload)`); `crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs:73-125` (the extractors recompute `sha256_bytes(message.data)` on the way back); `crates/xmtp_mls/src/groups/mls_sync.rs:2461-2463` (`find_group_intent_by_payload_hash(envelope.payload_hash)`). Any re-encoding stalls every intent for `MAX_GROUP_SYNC_RETRIES` and then errors it.

**R9.** When the client publishes an **identity update**, the backend **shall** return that envelope's assigned cursor in the publish response. For every other payload type the publish response cursor is optional.
*Cite:* `crates/xmtp_proto/src/api_client.rs:108-121,208-211` — `send_group_messages`, `send_welcome_messages`, `upload_key_package` and `publish_commit_log` all return `()`; their responses are discarded and no caller can observe a cursor. Only `publish_identity_update` returns `Option<Cursor>`, and only `Client::register_identity` consumes it, to persist `registration_cursor_*` (`crates/xmtp_mls/src/client.rs:1081-1087`). Even that is optional in practice: `V3Client::publish_identity_update` returns `Ok(None)` unconditionally and the client proceeds (`crates/xmtp_api_d14n/src/queries/v3/identity.rs:16`), while `D14nClient` extracts it from the first originator envelope and logs a warning if absent (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:53-64`). The draft's `PublishResponse { repeated EnvelopeMeta envelope_metas }` supplies more than the client needs, which is fine — but if the client is ever changed to consume per-envelope cursors, the response must be aligned with or keyed to the request.

**R10.** When the backend rejects a publish for a permanent reason (invalid payload, failed validation, malformed topic), the backend **shall** answer with a distinct canonical gRPC status code, **and** the client shall be changed to classify those codes as non-retryable.
*Cite:* `crates/xmtp_api_grpc/src/error.rs:109-123` — `impl RetryableError for GrpcError { fn is_retryable(&self) -> bool { true } }`. **A server status alone cannot satisfy this today.** No matter what code the backend returns, the client retries it 5 times over up to 120s, because the retryability decision never inspects the status. This requirement therefore has two halves and the client half is a prerequisite: the single existing carve-out, `GrpcError::is_unimplemented()` (`error.rs:115`), is consulted only by the bidi fallback latch and shows the pattern a general classifier would follow. **Schedule the client change in the same project, or the new backend's validation errors are indistinguishable from a flapping network.**

**R11.** When the client publishes a key package for an installation and subsequently publishes an identity update for that installation, the backend **shall** make the key package readable no later than the identity update becomes readable.
*Cite:* `crates/xmtp_mls/src/client.rs:1041-1053` — the source comment reads *"Step 3: Upload key package first (prevents race condition)"* / *"Step 4: Publish identity update (makes installation visible)"*.

**R12.** The backend **shall** accept a publish request carrying up to 50 welcome-message envelopes, and **shall** accept a gRPC frame of up to `GRPC_PAYLOAD_LIMIT` (25 MiB).
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:4640` (`chunk_size = (GRPC_PAYLOAD_LIMIT / per_welcome).clamp(1, 50)`) fixes the 50-item cap; `crates/xmtp_configuration/src/common/api.rs:12` with `crates/xmtp_api_grpc/src/grpc_client/client.rs:302-303` fixes the 25 MiB **transport** limit. Note the two are only loosely coupled: the chunker divides 25 MiB by an *estimate* built from four fields of the first welcome (`mls_sync.rs:4610-4640`), which ignores protobuf overhead, so the request body the client actually sends is not bounded by 25 MiB by construction.

**R13.** The backend **shall** tolerate an unbounded number of concurrent welcome publish requests from one client for one group's fan-out, up to one request per generated welcome (~2500 for a max-size group).
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:4636-4648` — the chunk size is `(GRPC_PAYLOAD_LIMIT / per_welcome).clamp(1, 50)` and every chunk is launched into `try_join_all` with no semaphore. The concurrency is `ceil(total / chunk_size)`, which is **~50 only when the chunk size is 50**; a chunk size of 1 (large welcomes) issues one RPC per welcome, all at once. There is no client-side ceiling.

### Query

**R14.** The backend **shall** accept a **newest-envelope** query naming up to 1000 topics in one request.
*Cite:* `crates/xmtp_api/src/mls.rs:328-343` — `get_newest_message_metadata` chunks at `const BATCH_SIZE: usize = 1000` and fires the chunks concurrently through `try_join_all`. This bound belongs to `GetNewestGroupMessage` / the draft's `QueryNewest` **only**. The generic multi-topic `Query` has no such client-side chunk: `get_identity_updates_v2` chunks at 50 (`crates/xmtp_api/src/identity.rs:22,68`), `query_commit_log` at 20, and `query_group_messages` / `query_welcome_messages` name exactly one topic each. Do not generalize 1000 to `Query`.

**R15.** The backend **shall** return, for a newest-envelope query over N topics, a result that lets the client associate each returned metadata record with its topic. Topics with no envelopes **may** be omitted.
*Cite:* `crates/xmtp_api/src/mls.rs:346-353` — the wrapper's terminal consumer builds a `HashMap` keyed by `msg.group_id` and `filter_map`s out every `None`, so an omitted empty topic and an explicit null are indistinguishable to the caller. Explicit nulls are required only by the *current* positional extractor and its warning comment (`crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs:22-26`), which a topic-keyed response would replace. The draft's `EnvelopeMeta.topic` field (`docs/self-hosted/backend.proto:33-39`) already carries the key; state normatively that the response is keyed by topic and may be shorter than the request, and update the extractor to match. See §10 G3.

**R16.** The backend **shall** support a newest-envelope query that returns metadata only, without the envelope payload.
*Cite:* `crates/xmtp_api/src/mls.rs` (`include_content: false`); the draft's `include_full_envelope` satisfies this.

**R17.** The backend **shall** return each installation's current key package in a way that identifies which installation it belongs to and that represents absence explicitly.
*Cite:* `crates/xmtp_api/src/mls.rs:167-183` (`MismatchedKeyPackages` fires on a response **length** mismatch and the mapping is purely positional) with `crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs:421-428` (v3 today expresses absence as an *empty positional entry*, which the client cannot distinguish from a real key package until MLS validation fails) and the fallback at `crates/xmtp_mls/src/groups/mod.rs:735-763`. A self-describing response — keyed by installation, with absence as its own variant — removes both the positional fragility and the fallback path.

**R18.** The backend **shall** accept a key-package query naming an unbounded number of installation keys in one request, or **shall** publish a per-request limit that the client is changed to chunk against.
*Cite:* `crates/xmtp_mls/src/mls_store.rs:97-110` and `crates/xmtp_api/src/mls.rs:143-164` — the public method takes a `Vec<Vec<u8>>` and neither layer chunks it, so the request size is whatever the caller passes. `MAX_GROUP_SIZE = 250` × `MAX_INSTALLATIONS_PER_INBOX = 10` = 2500 is the *practical* ceiling implied by group size, **not a limit the client enforces**. A backend limit below the request size therefore forces client-side chunking that does not exist today.

**R19.** The backend **shall** serve reads on a welcome topic keyed by an arbitrary 32-byte value that is not a registered installation.
*Cite:* `crates/xmtp_mls/src/groups/welcome_pointer.rs:30` — welcome pointees are published to a random 32-byte destination and read back by that key.

**R20.** The backend **shall** return identity updates for an inbox id in ascending `sequence_id` order with no gaps above the requested cursor.
*Cite:* `crates/xmtp_mls/src/identity_updates.rs:685-687` — association state is folded in order; a gap yields a wrong association state and a wrong membership decision.

**R21.** The backend **shall** echo the requested `identifier_kind` in each `GetInboxIds` response row.
*Cite:* `crates/xmtp_api/src/identity.rs:149-156` (the result map is keyed by `ApiIdentifier { identifier_kind, identifier }`) vs `crates/xmtp_api_d14n/src/queries/d14n/identity.rs:141` (hardcodes `Ethereum`), which breaks passkey lookups.

**R22.** The backend **shall** return a `GetInboxIds` row with no inbox id, rather than an error, for an identifier with no inbox.
*Cite:* `crates/xmtp_mls/src/client.rs:1250` (`can_message` fills absent identifiers as `false`); `crates/xmtp_api/src/identity.rs:144-157` (`filter_map` on `inbox_id?`).

**R23.** The backend **shall** accept a `GetInboxIds` request naming an unbounded number of identifiers in one request, or **shall** publish a per-request limit that the client is changed to chunk against.
*Cite:* `crates/xmtp_api/src/identity.rs:113-139` — the wrapper sends every supplied identifier in one request with no chunking; `crates/xmtp_mls/src/client.rs:1240-1251` (`can_message` passes arbitrary caller input straight through). 250 is **not** a client-enforced maximum: `MAX_GROUP_SIZE` is checked only in `add_members_by_identity` and only *after* the RPC returns (`crates/xmtp_mls/src/groups/mod.rs:1868-1876`), and `can_message` never checks it at all. A backend limit below the request size forces client-side chunking that does not exist today.

### Commit log

**R24.** The backend **shall** accept and serve `xmtp.mls.message_contents.CommitLogEntry` envelopes addressed per group, ordered ascending, independently of that group's message topic.
*Cite:* `crates/xmtp_mls/src/groups/commit_log.rs:262-305` and `:411-480`; `crates/xmtp_configuration/src/common/mls.rs:42` (`ENABLE_COMMIT_LOG = true`); `crates/xmtp_api_d14n/src/queries/combined.rs:215-236` (the cutover carve-out comment: *"fork detection silently dies"* without it).

**R25.** The backend **shall** accept a commit-log publish request carrying up to 10 entries and a commit-log query naming up to 20 groups.
*Cite:* `crates/xmtp_api/src/mls.rs` — `publish_commit_log`'s `BATCH_SIZE = 10`, `query_commit_log`'s `BATCH_SIZE = 20`.

**R26.** The backend **shall** honour the client-supplied per-group `PagingInfo.limit` on a commit-log query, returning no more than the requested number of entries per group; the client **shall** catch up on subsequent worker ticks.
*Cite:* `crates/xmtp_mls/src/groups/commit_log.rs:431-441` — the client sends `PagingInfo { direction: Ascending, id_cursor, limit: MAX_PAGE_SIZE }` per group, and the comment *"we will rely on next iteration of the worker to download the next batch"* explains why no paging loop follows. **100 is not a fixed server maximum**: `MAX_PAGE_SIZE` is 100 in production builds (`crates/xmtp_configuration/src/prod/api.rs:2`) and **20** in any build with `test-utils` enabled (`crates/xmtp_configuration/src/test/api.rs:4`). The requirement is to respect the requested limit, not to hard-code 100.

### Subscriptions

**Scope note for R27–R34.** These are requirements of the **XIP-83 bidirectional path only**. That path is native-only (`crates/xmtp_proto/src/api_client.rs:172-200`, gated by `xmtp_common::if_native!`) and opt-in, off by default behind `XMTP_BIDI_STREAMS_ENABLED` (`crates/xmtp_mls/src/subscriptions/router_callbacks.rs:104`). The current `D14nClient` implements the trait but **refuses at runtime** with an unretryable error and falls the caller back to the legacy `XmtpMlsStreams` path (`crates/xmtp_api_d14n/src/queries/d14n/streams.rs:120-155`); browsers stay on the legacy path unconditionally. So a backend that implements only the legacy server-streaming surface plus R34's `UNIMPLEMENTED` answer is workable today. R27–R34 become unconditional only if the project makes bidi the default, which §10 G10 argues for.

**R27.** When a subscription is opened with a per-topic cursor, the backend **shall** replay every envelope on that topic above the cursor before delivering live envelopes for it, in ascending order.
*Cite:* `crates/xmtp_mls/src/subscriptions/stream_messages.rs:168-219` (cursors read from `refresh_state` at subscribe); `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` module docs (*"strictly increasing above the lease's floor, nothing at-or-below it"*).

**R28.** The backend **shall** accept a subscription mutation frame naming up to 1000 topics.
*Cite:* `crates/xmtp_api_d14n/src/queries/bidi_transport.rs:180-202,211-250` — `MAX_MUTATE_TOPICS = 1000` and `MAX_MUTATE_BYTES = 16 * 1024 * 1024` are the client's **split ceilings**: `chunk_by_budget` / `chunk_mutate_adds` divide a wave on whichever of count or bytes is reached first, so they bound what the client emits, not what the server must accept. The 1000-topic figure is a real obligation because the client will emit a full 1000-topic frame. **16 MiB is not.** A normal topic is at most 33 bytes plus `PER_ENTRY_OVERHEAD = 64` (`crates/xmtp_proto/src/types/topic.rs:11-23`), so 1000 topics is well under 100 KiB; the byte ceiling only binds for unusually large entries, and no code path requires the server to accept a 16 MiB frame. State the accepted frame size from the transport limit (R39) instead.

**R29.** The backend **shall** support adding and removing topics on a live subscription without requiring the client to re-open the stream.
*Cite:* `crates/xmtp_mls/src/subscriptions/stream_router.rs:1314` (`MessageConsumer::add_group` takes a new lease on the same wire). Compare `crates/xmtp_mls/src/subscriptions/stream_messages.rs:285-305`, the legacy path this replaces, which re-dials with every group on every addition.

**R30.** The backend **shall** stamp a client-supplied correlation id on every catch-up delivery frame and echo it on the frame that completes that catch-up.
*Cite:* `crates/xmtp_api_d14n/src/queries/bidi_transport.rs` module docs — *"This client therefore requires a tag-serving backend... its `CatchUpComplete` frames also carry no tag (`0`; ours are minted from 1), and the first one shuts the transport down."* The draft's `mutate_id` on `Mutate`, `Messages` and `CatchupComplete` satisfies this.

**R31.** The backend **shall** answer a client `Ping` with a `Pong`. The backend **may** send its own liveness challenge; the client answers one but does not require one.
*Cite:* `crates/xmtp_api_d14n/src/queries/bidi.rs:42-53,646-688` — the client's watchdog is self-driven: after `PROBE_TIMEOUT_MULTIPLIER - 1` keepalive intervals with no inbound frame it sends its **own** ping (`B::ping_frame(WATCHDOG_NONCE)`), and it tears the wire down only if that ping goes unanswered for one further interval. *Any* inbound frame resets the window, so a server that never pings but answers pings keeps the connection alive indefinitely. The 30-second cadence is a client-side fallback, not a server obligation. The **answering** half is a hard requirement: an unanswered client ping is what tears the transport down.

**R32.** The backend **may** advertise its keepalive cadence on the subscription's first frame; when it does, the value **shall** be positive.
*Cite:* `crates/xmtp_api_d14n/src/queries/bidi.rs:42-53,646-655` — `DEFAULT_KEEPALIVE_MS = 30_000` is explicitly the *fallback* "used until the server's `Started` frame advertises its own cadence", and the actor adopts the advertised value only under `if *keepalive_interval_ms > 0` — a zero or absent value silently keeps the 30-second fallback, which is a working configuration. So advertising is an optimisation, not a requirement. The draft's `Started.keepalive_interval_ms` supports it.

**R33.** The backend **shall** support a catch-up-only mutation that replays history for the named topics without registering them for live delivery.
*Cite:* `crates/xmtp_mls/src/subscriptions/catch_up.rs` (`Client::catch_up_to_live`, "a bounded `history_only` one-shot sync"). The draft's `Mutate.history_only` satisfies this.

**R34.** When the backend does not implement the bidirectional subscription surface, it **shall** answer with gRPC `UNIMPLEMENTED`.
*Cite:* `crates/xmtp_mls/src/subscriptions/router_callbacks.rs:204-240` — `is_bidi_unsupported` / `open_is_backend_refusal` walk the error chain looking for `GrpcError::is_unimplemented`; anything else is treated as transient and retried forever.

**R35.** The backend **shall** provide a subscription surface reachable over gRPC-Web for browser clients.
*Cite:* `crates/xmtp_proto/src/api_client.rs:172-177` (*"gRPC-Web transports cannot speak full-duplex, so browsers stay on `XmtpMlsStreams` with a client-side watchdog"*); `crates/xmtp_api_grpc/src/grpc_client/client.rs:198-200`. The draft's `SubscribeOnce` (server-streaming, "used on the web") satisfies this.

### Retention and expiry

**R36.** The backend **shall** publish a welcome-retention duration, and **shall** expose a per-topic minimum-retained cursor so a client that has fallen behind it can detect the loss instead of silently missing a group.
*Cite:* `crates/xmtp_mls/src/mls_store.rs:70-94` — the only welcome read is cursor-driven, unbounded in time, and has **no truncation-recovery path**: the client resumes from its stored cursor and has no way to learn that anything below the server's retained floor was dropped. The originally-stated form ("retain until the recipient reads it") is **not implementable**: the client never sends a durable acknowledgement of any kind, so the backend cannot know when a welcome has been consumed. A published duration plus a minimum-retained cursor is the implementable equivalent, and the client change to read that cursor is new work. **[UNVERIFIED: no client-side welcome-retention constant exists today, and `backend.proto` has no minimum-retained-cursor field — see §11.14.]**

**R37.** The backend **shall** serve the newest key package for an installation, and **shall** not require the client to delete superseded ones.
*Cite:* `crates/xmtp_mls/src/worker/key_package_maintenance.rs` (`sweep_expired`: *"deletion is local-only; the network copy expires independently"*); `crates/xmtp_mls/src/identity.rs:726-740` (no delete RPC after a successful upload).

**R38.** The backend **shall** publish a group-message retention duration, and **shall** expose a per-topic minimum-retained cursor plus a distinguishable error when a client queries from below it.
*Cite:* `crates/xmtp_mls/src/groups/mls_sync.rs:2903-2915` — the client queries only from its stored cursor, ascending, with no lower bound and no gap detector; an installation offline for a long period resumes from wherever it left off and cannot tell truncation from an empty topic. As with R36, "retain until every installation has advanced past them" is **not implementable**: no installation ever acknowledges consumption to the backend, so the condition is unobservable server-side. A published duration, a minimum-retained cursor, and an explicit "cursor below retained floor" error give the client something it can act on — today it would silently fork instead (§11.14). **[UNVERIFIED: the client has no configured retention expectation; `EnvelopeMeta.expiry_ns` in the draft is a new concept with no current client consumer, and there is no minimum-retained-cursor field.]**

### Transport

**R39.** The backend **shall** accept requests and responses up to 25 MiB.
*Cite:* `crates/xmtp_configuration/src/common/api.rs:12`; `crates/xmtp_api_grpc/src/grpc_client/client.rs:302-303`.

**R40.** The backend **shall** tolerate HTTP/2 keepalive PINGs at the client's configured interval — 45 seconds by default — and **shall not** treat a connection idle at that interval as dead.
*Cite:* `crates/xmtp_api_grpc/src/grpc_client/native.rs:27-32,44-73` — 45s is the **default** value of `http2_keep_alive_interval`, overridable per process by `XMTP_GRPC_KEEPALIVE_INTERVAL_SECS` (with `XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS` = 20s and `XMTP_GRPC_TCP_KEEPALIVE_SECS` = 45s alongside, and `keep_alive_while_idle` = true). It is a client-side probe cadence, not a fixed backend idle guarantee, and an operator can change it. The incident note at `:13-35` explains the choice: 45s *"stay[s] inside common 60s middlebox idle timers"*, and worst-case dead-peer detection is ~65s.

**R41.** The backend **shall** answer a unary request within 120 seconds.
*Cite:* `crates/xmtp_api_grpc/src/grpc_client/native.rs:148` (request `timeout`).

**R42.** The backend **shall** accept at least 5000 requests per minute from a single native client connection, and **shall** apply its own server-side limit for browser clients, which do not self-throttle.
*Cite:* `crates/xmtp_api_grpc/src/grpc_client/native.rs:97-104,126-160` — 5000/minute is the **default** (`limit.unwrap_or(5000)`, applied as `.rate_limit(limit, Duration::from_secs(60))`) and is a constructor parameter, so a caller can raise it; it is a floor for the backend, not a client-enforced ceiling. `crates/xmtp_api_grpc/src/grpc_client/wasm.rs:16-24` shows the browser transport **ignores** the limit entirely (`_limit`), so a wasm client is unthrottled and the backend is the only thing bounding its request rate.

**R43.** The backend **shall** accept `x-app-version` and `x-libxmtp-version` request headers.
*Cite:* `crates/xmtp_api_grpc/src/grpc_client/client.rs:95-99` — appended unconditionally to every request.

**R44.** The backend **shall** accept a caller-supplied credential in a configurable request header — `Authorization` by default — on unary, server-streaming, and bidirectional requests alike.
*Cite:* `crates/xmtp_api_d14n/src/middleware/auth.rs:86-103,169-200` — `Credential { name: HeaderName, .. }` carries the header **name**, defaulted to `http::header::AUTHORIZATION` by `Credential::new` but settable to any header, so the backend must not hard-code `Authorization`; `modify_request` is applied identically in `request`, `stream`, **and** `bidi_stream`, with an explicit comment on why the bidi override must not be omitted. Note the current wiring is narrower than the requirement suggests: `inner_build_d14n` attaches `AuthMiddleware` to the **gateway client only** (`crates/xmtp_api_d14n/src/queries/client_bundle.rs:184-218,253-270`), never to `V3Client` or the d14n read nodes, so no header reaches a read path today. A single self-hosted backend serving reads and writes needs the middleware moved to cover both.

### Metadata content and timestamps

**R45.** When the backend answers a metadata-only newest-envelope query, the metadata **shall** identify the message class of the newest envelope (commit versus application message), **or** the client's local cursor model shall be changed so it does not need it.

*Cite:* `crates/xmtp_api_d14n/src/protocol/extractors/group_message_metadata.rs:58-76` — `visit_v3_group_message` selects the cursor's originator from `is_commit`: `if v1_message.is_commit { Cursor::mls_commits(id) } else { Cursor::v3_messages(id) }`. The result feeds `filter_groups_with_new_messages` (`crates/xmtp_mls/src/groups/welcome_sync.rs:511-524`), which compares that originator's stored cursor against the returned sequence id, and separately triggers a sync when the commit cursor is 0 while the application cursor has advanced. **`is_commit` is what routes the comparison to the right one of the group's two cursors.** The draft's metadata-only `QueryNewest` returns `EnvelopeMeta { cursor, server_ns, message_hash, topic, expiry_ns }` (`docs/self-hosted/backend.proto:33-39,56-65`) with no message-class field, so a metadata-only response cannot be compared against the current two-cursor model at all. Either carry an equivalent class marker in the metadata, or collapse the client to one cursor per topic first — the latter is the natural consequence of a single-sequence backend, but it is client work that must land before `QueryNewest` is used.

**R46.** The backend **shall** stamp a server-assigned timestamp on every envelope and return it on both the query and the subscription paths.

*Cite:* the value flows into user-visible and load-bearing client state in four places. `crates/xmtp_mls/src/groups/mls_sync.rs:1533-1575` — `envelope_timestamp_ns` becomes `StoredGroupMessage.sent_at_ns` for every application message, and is passed to `extract_message_sender`. `crates/xmtp_mls/src/groups/welcomes/xmtp_welcome.rs:488-497` — a DM's `last_message_ns`, which orders the conversation list, is `welcome.timestamp()`. `crates/xmtp_mls/src/groups/welcomes/xmtp_welcome.rs:553-579` — the synthetic "added to group" message derives **both** its idempotency key (`format!("{}_welcome_added", welcome.created_ns)`) and, through `calculate_message_id`, its primary key from `created_ns`; an unstable value produces a duplicate row on re-processing. `crates/xmtp_mls/src/identity_updates.rs:617-625` — `StoredIdentityUpdate.server_timestamp_ns` is persisted per identity update. The draft's `EnvelopeMeta.server_ns` (`docs/self-hosted/backend.proto:33-39`) is the field that must populate all of these; it is not optional and it must be stable across re-delivery of the same envelope.

---

## 10. Gaps and Open Questions vs `backend.proto`

Ordered by the risk they pose to the design review.

### G1. `ClientEnvelope` has no topic field — the server must derive every topic

The draft's `ClientEnvelope` is a bare payload oneof. Today the *client* derives the topic and stamps it into `AuthenticatedData.target_topic` (`crates/xmtp_api_d14n/src/protocol/traits/envelopes.rs:127-145`). Dropping the field means the backend must reproduce `TopicExtractor` (`crates/xmtp_api_d14n/src/protocol/extractors/topics.rs:83-173`), which for two payload types is expensive:

- `GroupMessageInput.V1` → **TLS-deserialize the MLS message** to read `protocol_message.group_id()`
- `UploadKeyPackageRequest` → **TLS-deserialize and cryptographically validate the KeyPackage** (including `LeafNodeLifetimePolicy::Verify`) to read `leaf_node().signature_key()`

This is arguably the right call — the client's stamp was never authenticated anyway, and a server that must validate the payload is already parsing it. But it makes topic derivation a hot-path cost on every publish, and it must be identical to the client's derivation or messages land on unreachable topics. **Design attention: confirm this is intended, and specify the derivation normatively.**

### G2. No commit-log topic

`ClientEnvelope` includes `xmtp.mls.message_contents.CommitLogEntry`, but:

- There is no `TopicKind` for it (`crates/xmtp_proto/src/types/topic.rs:18-23` has only 4 kinds).
- The commit log has **its own sequence space** per group, distinct from that group's message sequence — the client tracks four separate `refresh_state` kinds for it, and the commit-log cursor is a *sqlite rowid* on the publish side and a *server log sequence id* on the download side.
- Query needs a per-group limit and continuation. `TopicQuery` does carry a per-topic `Cursor` (`docs/self-hosted/backend.proto:19-27`), so the cursor half is covered; the limit is global to the request (see G8).

**The signature is not a gap.** `xmtp.mls.message_contents.CommitLogEntry` already carries `signature: Option<RecoverableEd25519Signature>` alongside `sequence_id` and `serialized_commit_log_entry` (`crates/xmtp_proto/src/gen/xmtp.mls.message_contents.rs:35-45`), and the draft puts that whole message into the `ClientEnvelope` oneof (`docs/self-hosted/backend.proto:41-48`), so the signature travels with the entry. `PublishCommitLogRequest`'s separate `signature` field is a v3 wire artifact, not a requirement.

**There is a new problem the draft creates instead: `CommitLogEntry.sequence_id` is server-assigned.** The client reads it back as `log_sequence_id` and stores it as the `CommitLogDownload` cursor (`crates/xmtp_mls/src/groups/commit_log.rs:551-556`), yet the same message is what the *client* publishes. A publishing client has no correct value to put in that field, and the backend must overwrite it on the way out — which means the signed `serialized_commit_log_entry` and the envelope's `sequence_id` are covered by different authorities.

**Design attention: the real gaps are (a) how a commit log is addressed — there is no `TopicKind` for it and no topic convention in the draft; (b) per-group limit and continuation semantics; and (c) who assigns `CommitLogEntry.sequence_id` and how a publishing client fills it.** If the commit log shares a group's message topic, the client's separate `CommitLogDownload` cursor breaks R1/R3.

### G3. `QueryNewestResponse` inherits the positional-null fragility

```protobuf
message QueryNewestResponse {
  oneof response {
    repeated EnvelopeMeta envelope_metas = 1;
    repeated ServerEnvelope envelopes = 2;
  }
}
```

Two problems:

1. **`repeated` inside a `oneof` is not valid protobuf.** This will not compile. It needs wrapper messages.
2. Once fixed, the response is still positional. `EnvelopeMeta` does carry `Topic topic = 4`, which is better than today's `GetNewestEnvelopeResponse` — but only if the response is allowed to be *shorter* than the request (topics with no envelope simply absent). The existing client code assumes index alignment: `CollectionExtractor::new(response.results, MessageMetadataExtractor::new())` with the warning comment at `crates/xmtp_api_d14n/src/endpoints/d14n/get_newest_envelopes.rs:22-26`. **Design attention: state explicitly that the response is keyed by `EnvelopeMeta.topic` and may omit empty topics, and update the client extractor accordingly.**

### G4. Key-package fetch has no dedicated shape, and absence is not expressible

`fetch_key_packages` maps naturally onto `QueryNewest` over `KeyPackagesV1` topics — which is what `D14nClient` already does. But:

- Absence is expressed today as an **empty positional entry** in `FetchKeyPackagesResponse` (`crates/xmtp_proto/src/gen/xmtp.mls.api.v1.rs:421-428`), which the wrapper maps as if it were a real key package; the terminal `MismatchedKeyPackages` batch error fires only when the response *length* changes, which the d14n extractor causes by dropping empty results (`crates/xmtp_api_d14n/src/protocol/extractors/key_packages.rs:33-48`). The client then falls back to N single-key requests at concurrency 16 (§5.7).
- With `QueryNewest` keyed by topic (per G3), absence becomes expressible and that whole fallback disappears.

**Design attention: this is a clear win, but it requires changing `ApiClientWrapper::fetch_key_packages` to return `HashMap<installation_key, Option<key_package>>` rather than enforcing positional length equality. Note that requirement in the plan.**

### G5. `Publish` has one RPC for all writes — readonly mode and per-operation validation both lose granularity

Today `ReadonlyClient` filters by RPC path substring (`crates/xmtp_api_d14n/src/middleware/readonly_client.rs:12-21`), and the bindings' `ClientMode::Notification` depends on it. With one `Publish` RPC, the client can only block writes wholesale. That is probably fine for notification mode. But it also means **the backend must return per-envelope validation results** — `PublishResponse { repeated EnvelopeMeta }` gives no way to say "envelope 3 of 5 was rejected". Since the client publishes multi-envelope batches for welcomes (up to 50) and commit-log entries (10), a partial failure is realistic.

**Design attention: specify whether `Publish` is atomic (all-or-nothing) or partial, and if partial, how per-envelope errors are reported.** Today `send_welcome_messages` treats any chunk error as fatal and aborts the whole `try_join_all`.

### G6. `backend.proto` has no imports at all, so nothing in it compiles

The file declares `syntax` and `package` and then goes straight to message definitions — there is **not one `import` statement** (`docs/self-hosted/backend.proto:1-3`). Every external type it names is therefore undefined, not just the identity ones:

- All five `ClientEnvelope` payload types (`docs/self-hosted/backend.proto:41-48`): `xmtp.mls.api.v1.GroupMessageInput`, `xmtp.mls.api.v1.WelcomeMessageInput`, `xmtp.mls.api.v1.UploadKeyPackageRequest`, `xmtp.identity.associations.IdentityUpdate`, `xmtp.mls.message_contents.CommitLogEntry`.
- `IdentityService`'s four message types (`docs/self-hosted/backend.proto:192-195`): `GetInboxIdsRequest`/`GetInboxIdsResponse` and `VerifySmartContractWalletSignaturesRequest`/`Response`.

The payload types all exist in the repo's generated protos and the identity ones presumably come from `xmtp.identity.api.v1`, so this is a drafting omission rather than a design gap — but the file as written does not compile, and the `ClientEnvelope` oneof is the whole publish surface. Note the requirement from R21: the response must echo `identifier_kind` (Ethereum vs Passkey), which the current d14n implementation does not (`crates/xmtp_api_d14n/src/queries/d14n/identity.rs:141`).

Similarly, `VerifySmartContractWalletSignaturesRequest`/`Response` are referenced but undefined. And `IdentityService` has **no `GetIdentityUpdates` RPC** — identity updates are meant to flow through the generic `Query` on `IdentityUpdatesV1` topics, which works (that is what `D14nClient` does) but changes the client's per-inbox cursor handling. With per-topic `TopicQuery` cursors this is strictly better than today (see R7), but `ApiClientWrapper::get_identity_updates_v2`'s chunk-of-50 shape and its `Vec<GetIdentityUpdatesV2Filter>` interface would need rewriting.

**Design attention: either define these messages in `backend.proto` or state the import; and decide whether identity updates keep a dedicated RPC or move to `Query`.**

### G7. No SCW batching, and it stays a server responsibility

The draft keeps `VerifySmartContractWalletSignatures` on the server, matching v3. Good — but the client calls it **one signature at a time** (`crates/xmtp_id/src/scw_verifier/remote_signature_verifier.rs:35-44`) and fans out with `try_join_all` and no cap in five places (§5.12). Joining a group with many smart-contract-wallet members can produce hundreds of concurrent single-signature RPCs.

**Design attention: the request proto already accepts `repeated`; only the client's `SmartContractSignatureVerifier` trait shape prevents batching. Worth a client-side change in the same project.**

### G8. `QueryRequest` has one global `limit` for many topics

```protobuf
message QueryRequest { repeated TopicQuery queries = 1; uint32 limit = 2; }
```

Unclear whether `limit` is per-topic or across the whole response. The client's uses differ:

- `query_group_messages` (one topic) wants a per-topic page and, on the v3 path, **loops until exhausted** — `crates/xmtp_proto/src/traits/combinators/v3_paged.rs:28-50` uses "returned fewer than `MAX_PAGE_SIZE`" as its termination signal, so it needs to know the page was short *for that topic*.
- `query_commit_log` wants `MAX_PAGE_SIZE` **per group** across 20 groups (`crates/xmtp_mls/src/groups/commit_log.rs:435-441`).
- `get_identity_updates_v2` wants everything above each inbox's cursor across up to 50 topics.

**A full page must make progress, or the client spins forever.** `v3_paged` (`crates/xmtp_proto/src/traits/combinators/v3_paged.rs:28-48`) has no page cap, no total cap, and no loop counter: on a full page it takes `paging_info.id_cursor`, calls `set_cursor`, and iterates. If the backend answers a full page with the **same** nonzero `id_cursor` it was given, the loop re-issues an identical request forever inside one `query_group_messages` call — an unbounded, un-cancellable hot loop, not a slow sync. The three termination conditions are all backend-supplied, so the backend alone controls whether the loop ends.

**Design attention: define `limit` as per-topic, provide an explicit continuation signal (a next-cursor per topic, or an explicit "more available" flag) rather than relying on "short page means done", and make the progress obligation normative — a full page shall carry a cursor strictly greater than the requested one, or an explicit terminal state.** The current v3 loop's three conditions (`num_messages < MAX_PAGE_SIZE`, `info.is_none()`, `id_cursor == 0`) are fragile and would need reworking either way; a client-side page cap is worth adding as a backstop.

### G9. `MessageHash` and `expiry_ns` have no current client consumer

`EnvelopeMeta` carries `MessageHash message_hash` and `optional uint64 expiry_ns`. Neither corresponds to anything the client reads today:

- The client computes the hash itself, in the API layer's extractors — `sha256_bytes(message.data)` on the returned `GroupMessageInput.V1.data` / `group_message::V1.data` (`crates/xmtp_api_d14n/src/protocol/extractors/group_messages.rs:73-125`) — and `mls_sync` only reads the resulting `envelope.payload_hash` (`crates/xmtp_mls/src/groups/mls_sync.rs:2461-2463`). A server-supplied hash could replace that **only if it is over exactly the same bytes** — `sha256` of the MLS payload, not of the envelope.
- `expiry_ns` is new. The client has no message-expiry constant and no retention expectation (R38 is marked unverified for this reason).

**Design attention: specify exactly what `message_hash` covers, and decide whether `expiry_ns` is informational or something the client must act on. If the client is expected to prune on it, that is new client work not currently in the plan.**

### G10. Streams: `Started` capabilities are empty, and the client has no negotiation path

`SubscribeResponse.Started.capabilities` is defined with only `CAPABILITY_UNSPECIFIED`. But the client today does **no capability negotiation at all** — it discovers support by trying and classifying failures, walking the error source chain for `UNIMPLEMENTED` (`crates/xmtp_mls/src/subscriptions/router_callbacks.rs:204-240`). If `Started` becomes the negotiation point, the client needs new code to read it, and the existing `is_bidi_unsupported` latch machinery should be retired rather than left as dead weight.

Also worth confirming: the client's bidi latch is **per destination URL and process-lifetime** (`router_callbacks.rs:130-159`). With a single self-hosted backend and a single subscription surface, the latch, `SHARED_WIRES`, the sentinel `"unsupported://d14n"` host, and the legacy fallback runner all become deletable — **but only if bidi becomes the default.** It is currently off by default behind `XMTP_BIDI_STREAMS_ENABLED` (`router_callbacks.rs:104`), and the legacy path's known defects (unbounded subscribe frames, full re-subscribe on every group addition, unbounded `seen` sets, watchdog off by default) argue strongly for making bidi the only path.

### Additional smaller findings

- **`GrpcError::is_retryable()` returns `true` for every gRPC status** (`crates/xmtp_api_grpc/src/error.rs:120-123`), including `InvalidArgument`. The new backend's validation errors will be retried 5× over up to 120s unless the client is fixed. This is client work the project should schedule.
- **`ClientBuilder::retry` is declared and never read** (`crates/xmtp_api_grpc/src/grpc_client/client.rs:253` vs `:296-312`) — `set_retry` on a gRPC builder is a silent no-op.
- **`Originators::DEFAULT` and `Originators::REMOTE_COMMIT_LOG` are both `100`** (`crates/xmtp_configuration/src/common/d14n.rs:14,16`) — indistinguishable at any match site. Moot once originators are removed.
- **`crates/xmtp_mls/src/client.rs:466` builds the `get_inbox_ids` request from the full identifier list, not the cache-miss list** — the cache filter decides *whether* to call, not *what* to send. A pre-existing bug worth fixing while this code is being touched.
- **`MAX_GROUP_SIZE` is enforced *after* the `get_inbox_ids` RPC** in `add_members_by_identity` (`crates/xmtp_mls/src/groups/mod.rs:1868-1876`), so an oversized add still costs a round trip.
- **The `test-utils` feature swaps production constants** (`crates/xmtp_configuration/src/lib.rs:1-14`): `MAX_PAGE_SIZE` 100→20, `SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS` 30min→1s, `KEYS_EXPIRATION_INTERVAL_NS` 1day→3s. Any crate in the graph enabling it silently reconfigures a release build.
- **The stream watchdog is off by default** (`crates/xmtp_mls/src/subscriptions/watchdog.rs:101`), so a silently-wedged legacy stream hangs indefinitely. The bidi path's server-driven keepalive (R31/R32) is the real fix.

---

## 11. Addendum: Deeper Ordering Detail (refines §5.2 and §6)

This section records findings that sharpen — and in two places correct — the summaries above.

### 11.1 `trust_message_order`: streams never apply commits

`crates/xmtp_mls/src/groups/mls_sync.rs:2230` documents the flag:

> Controls whether to allow epoch increments from commits and msg cursor increments. Set to `true` when processing messages from trusted ordered sources (queries), and `false` when processing from potentially out-of-order sources like streams.

`process_message_inner` (`mls_sync.rs:2451-2459`):

```rust
let allow_epoch_increment = trust_message_order;
let allow_cursor_increment = trust_message_order;
if !allow_epoch_increment && envelope.is_commit() {
    return Err(GroupMessageProcessingError::EpochIncrementNotAllowed);
}
```

`process_messages` (the `receive()` path) always passes `true`. Streams pass `false`.

**Consequence: a commit that arrives on a stream is never applied.** The stream's job is to *notice* that something happened; the actual epoch advance always comes from an ordered `query_group_messages`. This is why `process_message.rs:170` triggers a "recovery sync for out-of-order / commit-dependent messages".

**This is the single strongest ordering statement in the client**: the client does not trust stream ordering at all for state-changing messages. It trusts only the query path's ascending, cursor-anchored delivery. It reinforces R1/R2/R5 — and it means the new backend's stream ordering guarantees, however good, will not be relied on for commits until the client is changed.

### 11.2 The cursor advance is the atomic dedup latch

Three checks guard reprocessing, and the third is the real one (`mls_sync.rs`, inside the storage transaction in `process_message_inner`):

```rust
let requires_processing = if allow_cursor_increment {
    self.maybe_update_cursor(&db, envelope)?          // returns update_cursor's bool
} else {
    let current_cursor = db.get_last_cursor_for_originator(...)?;
    current_cursor.sequence_id < envelope.sequence_id()
};
if !requires_processing { identifier.previously_processed(true); return Ok(Continue(None)); }
```

Because `update_cursor`'s `DO UPDATE ... .filter(sequence_id.lt(excluded))` is a single SQL statement, **"advance the cursor" and "claim the message" are one atomic operation**. Checks 1 (`process_message`, pre-lock) and 2 (`process_message_inner`, post-lock) are fast paths.

The comparison is per-`(group_id, entity_kind, originator_id)` **triple**. Commits and application messages have independent cursors, so **the client does not require a single globally ascending sequence across both streams for a group.** Whether the new backend gives one topic per group or splits commits out, either works — provided each stream is individually ascending and gapless.

### 11.3 Non-retryable errors advance the cursor past the bad message — with two exceptions

`post_process_message`:

```rust
if !e.is_retryable() && mls_group.is_active() && let Err(transaction_error) = ... {
    if let Err(update_cursor_error) = self.maybe_update_cursor(&storage.db(), envelope) {
        // We don't need to propagate the error if the cursor fails to update - the worst case is
        // that the non-retriable error is processed again
```

Two guarded exceptions:

- **`mls_group.is_active()` is required.** The source note: *"Do not update the cursor if you have been removed from the group - you may be readded later."*
- **`ProtocolVersionTooLow` pauses instead of advancing.** `post_process_message` maps `CommitValidationError::ProtocolVersionTooLow(min_version)` to `set_group_paused(&group_id, &min_version)` and returns `GroupMessageProcessingError::GroupPaused`. On the own-intent path the transaction is explicitly rolled back:
  > a below-floor client processing its OWN commit on a migrated group must PAUSE, not fold the failure into a terminal `Error`. Folding would commit the cursor advance above (past its own commit) and leave the intent `Error` — forking from peers who merged the commit, with no upgrade-based recovery.

**Retryability decides the batch outcome.** `GroupMessageProcessingError::is_retryable` marks **`OldEpoch` and `FutureEpoch` as non-retryable**, along with `GroupPaused`, `EpochIncrementNotAllowed`, `IntentAlreadyProcessed`, `MessageAlreadyProcessed`, and app-data decode failures. One error is *forced* retryable on purpose — `EpochAuthenticatorNotAdvanced`:
> Retry so the enclosing transaction rolls back (including the cursor advance) and the message converges via cursor dedup instead of persisting a forked commit log entry.

### 11.4 Own-commit epoch mismatch is exact-equality, and resets to `ToPublish`

§5.2 described `validate_message_epoch`, which is the *non-commit* path. For **own commits**, `stage_and_validate_intent` uses exact equality against `intent.published_in_epoch`:

```rust
if message_epoch != group_epoch {
    let processing_error = if message_epoch < group_epoch {
        GroupMessageProcessingError::OldEpoch(message_epoch as u64, group_epoch as u64)
    } else {
        GroupMessageProcessingError::FutureEpoch(message_epoch as u64, group_epoch as u64)
    };
    return Err(IntentResolutionError { processing_error, next_intent_state: IntentState::ToPublish });
}
```

**→ the intent is reset to `ToPublish` and re-encrypted and republished at the new epoch.** This is the "my commit lost the race" path, and it is the reason a group's commits serialize: two members racing to commit means one of them republishes.

`validate_message_epoch` (with `MAX_PAST_EPOCHS = 3`) applies to `SendMessage | ProposeMemberUpdate | ProposeGroupContextExtensions` and the GCE-proposal phase of `CommitPendingProposals` — application messages tolerate 3 epochs of staleness; commits tolerate none.

**External commits have no explicit epoch comparison** in `validate_and_process_external_message` — OpenMLS does it internally, and a wrong-epoch commit surfaces as a non-retryable `OpenMlsProcessMessage` error, so the cursor advances past it and `mark_failed_commit_logged` records it. That function processes the message **twice** on purpose:
> We need to process the message twice to avoid an async transaction. We'll process for the first time, get the processed message, and roll the transaction back, so we can fetch updates from the server before being ready to process the message for a second time.

### 11.5 Send failure resets to `ToPublish`, not `Published`

Refining §5.1 — and note this is the *outer* layer. Before the intent state machine sees a failure at all, `ApiClientWrapper::send_group_messages` has already retried the **same cloned payload** up to 5 times: it wraps the call in `retry_async!` and rebuilds `SendGroupMessagesRequest { messages: group_messages.clone() }` from the identical `Vec` on every attempt (`crates/xmtp_api/src/mls.rs:208-225`). So a transient failure replays byte-identical bytes, and the intent is untouched.

Only when all of those retries fail does `handle_published_intent_send_failure` run (`crates/xmtp_mls/src/groups/mls_sync.rs:5005-5022`), and it does **not** leave the intent `Published` for a further retry of the same bytes:

```rust
if (intent.publish_attempts + 1) as usize >= MAX_INTENT_PUBLISH_ATTEMPTS {
    let id = utils::id::calculate_message_id_for_intent(intent)?;
    db.set_group_intent_error_and_fail_msg(intent, id)?;
} else {
    // Reset so the next retry re-encrypts at the current epoch.
    db.increment_intent_publish_attempt_count(intent.id)?;
    db.set_group_intent_to_publish(intent.id)?;
}
```

**A later intent cycle then re-encrypts the payload from scratch at the current epoch rather than resending it.** So the hazard is second-order, not immediate: the wrapper's 5 same-payload retries come first, and only after they are exhausted does the next sync's `publish_intents` build fresh bytes with a *different* `payload_hash`. If one of the exhausted attempts had in fact landed on the backend, that first payload arrives later as an unmatched external message while the intent waits on the second. **The backend should be aware that a client retry, once it has crossed the wrapper's retry budget, is not idempotent.** `group_messages.idempotency_key` exists in the schema (`crates/xmtp_db/src/encrypted_store/schema_gen.rs:48`) but is a local message id, not a publish-dedup token.

### 11.6 `IntentState::Superseded` and downgrade tolerance

`IntentState` (`crates/xmtp_db/src/encrypted_store/group_intent.rs`) has six variants: `ToPublish=1, Published=2, Committed=3, Error=4, Processed=5, Superseded=6`. The doc for the last:
> Abandoned before publishing because its compare-and-swap guard no longer matched the committed state — another member changed the field first. Terminal and distinct from `Error`: nothing went wrong, the write is simply stale.

Every production intent query passes `Some(IntentKind::all().collect())` for downgrade tolerance:
> rows written by a NEWER build (which may use discriminants this build has no variant for) are excluded in SQL instead of poisoning the whole `load()` — `FromSql` errors on unknown discriminants, and one such row would otherwise wedge every intent query for the group after an app downgrade.

### 11.7 `PostCommitAction` fires only after network confirmation

Welcomes are **never** sent speculatively. `post_commit_data` is stored on the intent at publish time, and `MlsGroup::post_commit` (`mls_sync.rs:4272`) queries intents in state `Committed` — i.e. intents whose commit has already come back from the network and been applied — and only then dispatches `PostCommitAction::SendWelcomes`.

**Cross-topic ordering consequence for the backend:** a welcome for group G is published strictly *after* the commit that created the membership has been read back from G's message topic. There is no window in which a welcome exists for a commit the group topic has not yet served.

### 11.8 The `refresh_state` migration deliberately replays

`crates/xmtp_db/migrations/2025-08-20-175213_d14n_originator_refresh_state/up.sql` split the old single `Group` cursor into `ApplicationMessage` (originator 10) and `CommitMessage` (originator 0), seeding both from the old value. Its own comments:

> `-- We won't skip any messages (might re-fetch some that were already synced)`
> `-- may get some duplicate messages in sync that will be rejected`

**This is direct evidence that the client treats duplicate delivery as routine and safe** (R6), and that the project's "start fresh with a clean DB" plan removes the only reason this migration exists.

### 11.9 Reading a cursor has a side effect

`get_last_cursor_for_originators` **materializes missing rows**: for any requested originator with no row it constructs `RefreshState { sequence_id: 0, .. }` and calls `.store_or_ignore(self)?` before returning `Cursor::new(0, originator)`. So "no cursor" and "cursor 0" are indistinguishable, and a read creates state. The test `get_cursor_with_no_existing_state` asserts the row exists afterwards.

`get_last_cursor_for_ids` chunks at `const CHUNK: usize = 900` to stay under SQLite's 999-bind limit — **so a client with more than 900 groups already issues multiple DB queries per sync round**, which bounds how large a single `QueryNewest` batch can usefully be from the client's side.

### 11.10 Commit-log ordering is the strictest contract in the client

`crates/xmtp_mls/src/groups/commit_log.rs::should_skip_remote_commit_log_entry` documents a seven-point rule; the ones that constrain the backend:

> 4. The `commit_sequence_id` of the entry is not greater than the most recently stored entry, if one exists.
> 5. The `last_epoch_authenticator` does not match the `epoch_authenticator` of the most recently stored entry with a `CommitResult` of `COMMIT_RESULT_APPLIED`, if one exists.
> 6. The entry has a `CommitResult` of `COMMIT_RESULT_APPLIED`, but the epoch number is **not exactly 1 greater** than the most recently stored entry with a result of `COMMIT_RESULT_APPLIED`.
> 7. The entry `CommitResult` is not `COMMIT_RESULT_APPLIED`, and the epoch authenticator or epoch number does not match the most recently applied values.

**What the client actually enforces is narrower than "strictly ascending, gapless, hash-chained", and the three axes differ** (`crates/xmtp_mls/src/groups/commit_log.rs:562-616`):

- **`commit_sequence_id` must increase, but not by one.** The check is `entry.commit_sequence_id <= latest_saved_remote_log.commit_sequence_id` → skip. Gaps are fine and expected: `commit_sequence_id` points into the *message* stream, where non-commit messages occupy the intervening positions.
- **Applied epoch numbers must increase by exactly one.** For an entry with `CommitResult::Applied`, `entry.applied_epoch_number != latest.applied_epoch_number + 1` → skip. This is the dense, gapless axis, and it is the one a dropped or reordered commit-log entry breaks.
- **The chain links through the last *applied* authenticator.** An applied entry's `last_epoch_authenticator` must equal the stored `applied_epoch_authenticator`; a non-applied entry's authenticator and epoch number must match the last applied values unchanged.
- **`log_sequence_id` continuity is never checked.** The download cursor is set from `commit_log_response.commit_log_entries.last().sequence_id` (`commit_log.rs:551-556`) with no comparison against the previous value and no gap test.

`latest_saved_remote_log` is threaded forward within a batch, so the chain is validated entry-by-entry. An entry failing any rule is skipped silently — **and the download cursor still advances past it**, so a backend that reorders or drops a commit-log entry permanently desynchronizes that group's fork detector: the applied-epoch `+1` rule can never be satisfied again.

Note the two-sequence structure the client tracks in `remote_commit_log`: `log_sequence_id` (the server's commit-log stream position, held by the `CommitLogDownload` cursor) **and** `commit_sequence_id` (the position in the *message* stream of the commit being attested). These are different numbers and both matter.

Also note the asymmetry in what the two commit-log cursors hold: **`CommitLogUpload` stores a local sqlite `rowid`; `CommitLogDownload` stores the server's `log_sequence_id`.** Both live in `refresh_state` under originator 100, distinguished only by `entity_kind`.

### 11.11 Two `refresh_state` details worth carrying into the design

**The kind↔originator mapping is 1:1 for message kinds, but not for the commit log.** From `latest_cursor_for_id`'s doc:
> Each entity kind uses a dedicated originator (e.g. ApplicationMessage -> originator 10, CommitMessage -> originator 0), so MIN vs MAX is equivalent here — each originator only ever has one entity kind.

That holds for `Welcome`, `ApplicationMessage` and `CommitMessage`. It does **not** hold for the commit log: all four commit-log kinds — `CommitLogUpload` (3), `CommitLogDownload` (4), `CommitLogForkCheckLocal` (5) and `CommitLogForkCheckRemote` (6) — share `Originators::REMOTE_COMMIT_LOG` = 100 and are separated only by `entity_kind` (`crates/xmtp_db/src/encrypted_store/refresh_state.rs:27-34,386-400`). Since `Originators::DEFAULT` is also 100, four distinct cursors and the default originator all collide on one id; the composite primary key `(entity_id, entity_kind, originator_id)` is what keeps them apart.

**Identity updates have no `EntityKind`.** They are cursored directly off `identity_updates.sequence_id` (`crates/xmtp_mls/src/cursor_store.rs::SqliteCursorStore::latest`, `IdentityUpdatesV1` arm), stuffed into a `GlobalCursor` under `Originators::INBOX_LOG`. So the identity-update cursor lives in a different table with a different shape from every other cursor.

### 11.12 Key packages: the 24-hour grace window is an unenforced timing bet

Refining §5.6. The deletion sequence is **mark, then sweep**:

**Mark** — `crates/xmtp_db/src/encrypted_store/key_package_history.rs::mark_key_package_before_id_to_be_deleted(id)`:

```rust
let delete_at_24_hrs_ns = now_ns() + KEYS_EXPIRATION_INTERVAL_NS;
diesel::update(dsl::key_package_history
        .filter(dsl::id.lt(id))
        .filter(dsl::delete_at_ns.is_null()))   // Only set if not already set
    .set(dsl::delete_at_ns.eq(delete_at_24_hrs_ns))
```

`KEYS_EXPIRATION_INTERVAL_NS` = `NS_IN_DAY` in prod, `3 * NS_IN_SEC` in test. The `is_null()` guard means grace never extends on repeated rotation.

**Sweep** — `key_package_maintenance.rs::sweep_expired` deletes the actual MLS keystore material for rows where `delete_at_ns <= now_ns()`.

**Why the grace exists:** the client keeps decryption material for superseded key packages for 24 hours after replacing them, because a peer may have fetched the old KP shortly before rotation and will send a welcome encrypted to it. Delete locally at rotation time and those in-flight welcomes become permanently undecryptable.

**So the client's assumption is:** the backend (a) retains the most recently uploaded last-resort key package for an installation indefinitely until replaced, (b) expires old copies on its own schedule the client neither knows nor controls, and (c) will not hand out a key package whose local private material the client has already swept. **Property (c) is guaranteed only by the 24-hour grace window being longer than the real fetch-to-welcome latency. Nothing enforces it.**

The `true` argument in `upload_key_package(kp_bytes, true)` is the "is inbox id credential" flag, and the model is **one long-lived last-resort key package per installation, replaced wholesale** — not a pool of N one-shot packages. `TopicKind::KeyPackagesV1` is uncursored (`SqliteCursorStore::latest` returns `GlobalCursor::default()` for it), consistent with "newest wins".

**Rotation triggers** (`crates/xmtp_mls/src/worker/key_package_maintenance.rs`):

| Trigger | Deadline set |
| --- | --- |
| Scheduled | `reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)` → `now + 30d`, only `WHERE next_key_package_rotation_ns IS NULL OR <= now_ns()` |
| **Post-welcome (a security property)** | `queue_key_rotation` → `now + KEY_PACKAGE_QUEUE_INTERVAL_NS` (**5 s**), only `WHERE next_key_package_rotation_ns > rotate_at_ns OR IS NULL` — monotonically *lowering* only |
| Startup reconcile | `seed_and_reconcile_kp_tasks` — *"pull-ins only LOWER task deadlines to the live DB columns, repairing rows stranded by a crash mid-nudge"* |

The 5-second post-welcome debounce is called out in the source as *"a security property"*: a key package is consumed when someone welcomes you, so the client rotates promptly after being added rather than waiting 30 days.

**A failed upload leaves an orphan.** `generate_and_store_key_package` writes the `key_package_history` row **before** the upload, and `mark_key_package_before_id_to_be_deleted` runs **only on upload success** — so a failed upload leaves an unmarked local row whose material is never swept.

### 11.13 Disappearing messages place no backend requirement

Expiry is computed locally at receipt time and never travels on the wire. `mls_sync.rs::get_message_expire_at_ns`:

```rust
if group_disappearing_settings.is_enabled() { Some(now_ns() + group_disappearing_settings.in_ns) } else { None }
```

The clock starts at the **receiving client's** `now_ns()`, from group mutable metadata (`message_disappear_from_ns` / `message_disappear_in_ns` on the `groups` table). Deletion is local: `crates/xmtp_mls/src/worker/disappearing_messages.rs` sleeps until `db.min_expire_at_ns()` and calls `delete_expired_messages()`.

**So disappearing messages require nothing of the backend.** They do *not* justify `EnvelopeMeta.expiry_ns` — see G9.

### 11.14 The strongest retention statement in the client

Combining §11.1, §11.3 and §6.3: the client fetches only with `id_cursor = <stored cursor>` ascending. Anything the backend drops below a stale cursor is **silently and permanently lost**. If the dropped envelope was a **commit**, the client will never see it, will never advance its epoch, and will fork. The `EntityKind::CommitMessage` cursor has no backfill, no gap detection, and no lower-bound negotiation.

**The only defense is the commit-log fork detector, which detects a fork after the fact rather than preventing it.** This is the sharpest form of R2 and R38, and the strongest argument for the new backend either (a) never expiring commits, or (b) exposing a minimum-retained-sequence per topic so a too-far-behind client can detect the loss and re-join rather than fork silently. **The draft `backend.proto` has no such field.**

---

## 12. Summary Table: Draft `backend.proto` Coverage

| Client need | Draft coverage | Verdict |
| --- | --- | --- |
| Query one topic from a cursor | `Query` + `TopicQuery` | ✅ |
| Query many topics, each with its own cursor | `QueryRequest.queries` | ✅ **better than today** — an efficiency win, not a correctness fix; the current `min(sequence_id)` collapse is correct through dedup, just wasteful (R7) |
| Newest envelope across ≤1000 topics, metadata only | `QueryNewest` + `include_full_envelope` | ⚠️ shape right; the `oneof`+`repeated` will not compile (G3); response should be keyed by `EnvelopeMeta.topic` and may omit empty topics (R15); **metadata carries no commit-vs-application marker, which the current cursor model needs** (R45) |
| Publish group messages / welcomes / key packages / identity updates | `Publish` + `ClientEnvelope` | ✅ shape right; partial-failure semantics undefined (G5) |
| Publish returns a cursor | `PublishResponse.envelope_metas` | ✅ more than needed — only identity-update publish has a consumer today; every other publish response is discarded (R9) |
| Commit log publish + query | `CommitLogEntry` in the oneof | ⚠️ signature **is** present on `CommitLogEntry`; the real gaps are **no topic convention, no per-group limit, and a server-assigned `sequence_id` a publishing client cannot fill** (G2) |
| Key package fetch keyed by installation with explicit absence | `QueryNewest` over key-package topics | ⚠️ depends on G3's resolution; note today's v3 absence is an *empty positional entry*, not an error, and `MismatchedKeyPackages` is a response-**length** mismatch (G4, R17) |
| Get inbox ids | `IdentityService.GetInboxIds` | ⚠️ messages undefined — the file has **no imports at all**, so every external type is undefined; must echo `identifier_kind` (G6, R21) |
| Verify SCW signatures | `IdentityService.VerifySmartContractWalletSignatures` | ⚠️ messages undefined; batching opportunity unused (G7) |
| Identity updates by inbox id + sequence cursor | via generic `Query` on `IdentityUpdatesV1` topics | ⚠️ works, but rewrites `ApiClientWrapper::get_identity_updates_v2` (G6) |
| Bidi subscribe with in-place mutation | `SubscriptionService.Subscribe` + `Mutate` | ✅ matches the client's existing frame vocabulary — but the whole bidi path is native-only and off by default (scope note before R27) |
| Correlation ids on catch-up frames | `mutate_id` on `Mutate` / `Messages` / `CatchupComplete` | ✅ (R30) — the client *requires* these and tombstones an untagged backend |
| Server keepalive cadence advertised | `Started.keepalive_interval_ms` | ✅ optional (R32) — a zero or absent value keeps the client's 30 s fallback, which works |
| Ping / Pong liveness | `Ping` / `Pong` both directions | ✅ (R31) — **answering** a client ping is the hard requirement; a server-initiated ping is optional |
| Catch-up-only replay | `Mutate.history_only` | ✅ (R33) |
| Browser-compatible subscription | `SubscribeOnce` | ✅ (R35) |
| Per-topic limit and continuation | `QueryRequest.limit` (one, global) | ⚠️ semantics undefined; **a full page must return a strictly greater cursor or `v3_paged` loops forever** (G8) |
| Minimum retained sequence per topic | **absent** | ❌ **silent fork risk**; also the only implementable form of R36/R38, since no client ever acknowledges consumption (§11.14) |
| Idempotent publish retry | **absent** | ❌ the wrapper first replays identical bytes 5×; only a later intent cycle re-encrypts and produces a different payload (§11.5) |
| Server timestamp on every envelope | `EnvelopeMeta.server_ns` | ✅ present and **required** — feeds `sent_at_ns`, DM `last_message_ns`, and the synthetic welcome message's id (R46) |
| Permanent-vs-transient publish rejection | canonical gRPC status | ❌ unusable until the client classifies status codes — `GrpcError::is_retryable()` returns `true` for everything (R10) |
| `depends_on` / causal ordering | **removed** | ✅ correct simplification — deletes `order.rs`, `sort/`, `resolve/`, the icebox |
| Originator ids / vector clocks | **removed** | ✅ correct simplification — `Cursor { uint64 }` replaces `GlobalCursor` |
| Payer / gateway split, node discovery | **removed** | ✅ correct simplification — deletes `MultiNodeClient`, `ReadWriteClient`, `GetNodes`, `HealthCheck` |

---

## Review status

**Review thread:** Codex `review-wiki-api-callers`, thread id `01a06248-9f94-7bb2-8955-2162afa1ec20` (model `gpt-5.6-sol`, read-only, `/Users/nickmolnar/.claude/jobs/55a23e1f/tmp/phase0/runs/review-wiki-api-callers.md`).

Every finding below was re-checked against the source before the page was changed. All 35 were confirmed correct, and all 35 are applied. No finding was rejected.

| Finding | Applied or rejected | Note |
| --- | --- | --- |
| §2.1, §3.1 — `GroupId` is 16 bytes, not 32 | applied | `crates/xmtp_proto/src/types/ids/group_id.rs:16-20` — `GroupId([u8; 16])`, "Exactly 16 bytes, by protocol invariant". A group topic is 17 bytes; the 33-byte `TopicBytes` buffer is sized for the 32-byte installation-id kinds. |
| §2.1 — group and welcome queries are not fully paged on d14n | applied | `crates/xmtp_api_d14n/src/queries/d14n/mls.rs:136-162,190-212` — one `QueryEnvelope` at `limit: MAX_PAGE_SIZE`, no loop. Only `V3Client` exhausts pages. |
| §4, §5.15 — two d14n RPC names wrong | applied | `endpoints/d14n/get_nodes.rs:21-23` gives `/xmtp.xmtpv4.payer_api.PayerApi/GetNodes`; `endpoints/d14n/health_check.rs:41-46` gives `/grpc.health.v1.Health/Check`, the standard gRPC health probe. |
| §1, §5.3, §5.11 — bindings also call the API directly | applied | `bindings/mobile/src/mls.rs:287-305,533-550`; `bindings/node/src/inbox_id.rs:14-30`; `bindings/wasm/src/inbox_id.rs:10-27`. |
| §5.1 — `mls_sync.rs:2462` does not compute `sha256(message.data)` | applied | `mls_sync.rs:2461-2463` reads `envelope.payload_hash`; the extractors compute it (`protocol/extractors/group_messages.rs:73-125`). |
| §5.4, R13 — welcome fan-out is not capped at ~50 | applied | `mls_sync.rs:4636-4648` — chunk size 1..50, every chunk in flight; concurrency is `ceil(total / chunk_size)`, up to one RPC per welcome. |
| R12 — the 25 MiB welcome-body claim is overstated | applied | `mls_sync.rs:4610-4640` sums four fields of the first welcome only; 25 MiB is the transport frame limit (`common/api.rs:12`). |
| §5.7, R17, G4 — a missing key package is not generally `MismatchedKeyPackages` | applied | `gen/xmtp.mls.api.v1.rs:421-428` documents empty positional entries; `xmtp_api/src/mls.rs:167-183` checks length only; the d14n extractor shortens the vector (`extractors/key_packages.rs:33-48`). |
| R18 — 2500 is not a client-enforced key-package maximum | applied | `mls_store.rs:97-110`, `xmtp_api/src/mls.rs:143-164` — unbounded `Vec`, no chunking at either layer. |
| R23 — 250 is not a `GetInboxIds` maximum | applied | `client.rs:1240-1251`, `xmtp_api/src/identity.rs:113-139` — arbitrary input, one request. |
| §6, R1, R3, R4 — cursor code does not prove one total order per topic | applied | `refresh_state.rs:100-107,357-383` keys on `(entity_id, entity_kind, originator_id)`; `mls_sync.rs:1465-1482` compares per triple. Commit and application cursors are independent. |
| R7 — per-topic cursors are efficiency, not correctness | applied | `d14n/identity.rs:75-100` with `identity_updates.rs:617-630` — `min` cursor plus `insert_or_ignore` is correct, just wasteful. |
| R9 — publish need not return a cursor for every envelope | applied | `api_client.rs:108-121,208-211` — four of five publish methods return `()`; only `register_identity` consumes a cursor (`client.rs:1081-1087`), and `V3Client` returns `None`. |
| R10 — a server status alone cannot give the distinction today | applied | `xmtp_api_grpc/src/error.rs:109-123` — blanket `is_retryable() == true`. The client must classify codes first. |
| R14 — the 1000 limit belongs to `QueryNewest`, not generic `Query` | applied | `xmtp_api/src/mls.rs:328-343`. |
| R15, G3 — explicit nulls for empty topics are not required | applied | `xmtp_api/src/mls.rs:346-353` builds a map and drops `None`; `backend.proto:33-39` already carries `EnvelopeMeta.topic`. |
| R26 — 100 is not a fixed server maximum | applied | `commit_log.rs:431-441` sends the limit; `prod/api.rs:2` = 100, `test/api.rs:4` = 20. |
| §2.8, G8, §12 — paging progress requirement omitted | applied | `v3_paged.rs:28-48` — an unchanged nonzero cursor on a full page loops forever with no cap or counter. |
| R28 — the 16 MiB mutation-body minimum is unsupported | applied | `bidi_transport.rs:180-202,211-250` are client split ceilings; `topic.rs:11-23` bounds a topic at 33 bytes, so 1000 topics is well under 100 KiB. |
| R27–R34 — written as unconditional | applied | `api_client.rs:172-200` (native-only), `d14n/streams.rs:120-155` (refuses and falls back). Marked as the opt-in XIP-83 path with a scope note. |
| R31, R32 — server pings and advertised cadence are not mandatory | applied | `bidi.rs:42-53,646-688` — client-driven watchdog, 30s fallback, `> 0` guard on the advertised value. Answering a client ping is the hard part. |
| R36, R38 — retention requirements not implementable as written | applied | `mls_store.rs:70-94`, `mls_sync.rs:2903-2915` — no acknowledgement is ever sent, so "retain until read" is unobservable. Restated as a published duration plus a minimum-retained cursor. |
| R40 — 45s is not a fixed backend guarantee | applied | `grpc_client/native.rs:27-32,44-73` — default, overridable by `XMTP_GRPC_KEEPALIVE_INTERVAL_SECS`. |
| R42 — 5000/min is not the full requirement | applied | `native.rs:97-104,126-160` (default, configurable); `wasm.rs:16-24` ignores the limit entirely. |
| R44 — auth requirement too broad and too narrow | applied | `middleware/auth.rs:86-103,169-200` (configurable header name); `client_bundle.rs:184-218,253-270` (gateway only). |
| G2, §12 — the missing commit-log signature is false | applied | `gen/xmtp.mls.message_contents.rs:35-45` — `CommitLogEntry.signature` exists; `backend.proto:19-27,41-48` carries the whole message and a per-topic cursor. Replaced with the topic-convention, limit, and server-assigned `sequence_id` gaps. |
| G6 — the compile gap is wider than stated | applied | `backend.proto:1-3,41-48,192-195` — no imports at all, so all five `ClientEnvelope` payload types are undefined too. |
| §10, §12 — the metadata-only `is_commit` dependency is missing | applied | Added as **R45**. `extractors/group_message_metadata.rs:58-76`; `welcome_sync.rs:511-524`; `backend.proto:33-39,56-65`. |
| §§6, 9, 10, 12 — server timestamps not listed as a requirement | applied | Added as **R46**. `mls_sync.rs:1533-1575`; `welcomes/xmtp_welcome.rs:488-579`; `identity_updates.rs:617-625`. |
| §11.5 — the retry description overstates when new bytes appear | applied | `xmtp_api/src/mls.rs:208-225` retries the same cloned payload first; `mls_sync.rs:5005-5022` runs only after that budget is spent. |
| §11.10 — "strictly ascending, gapless, hash-chained" is false | applied | `commit_log.rs:562-616,551-556` — `commit_sequence_id` must increase but not by one; applied epochs increase by exactly one; `log_sequence_id` continuity is never checked. |
| §11.11 — the kind↔originator mapping is not 1:1 | applied | `refresh_state.rs:27-34,386-400` — four commit-log kinds share originator 100. |

### Residual risk

Two classes of risk remain. First, the page is a snapshot of the `self-hosted` branch and cites line numbers; the substance of each claim was verified against the code, but line references drift with any edit to `mls_sync.rs`, `commit_log.rs` or the d14n query layer, which are the files this project will change first. Treat a citation that no longer resolves as stale rather than wrong, and re-check the surrounding function. Second, and more consequential for the design, several requirements are now explicitly weaker than they first appeared — R1, R3 and R4 are per-cursor-partition rather than per-topic; R7 is efficiency rather than correctness; R9 has one real consumer. That weakness is a statement about the *current* client, not about what a good backend should provide: a single total order per topic, a cursor on every publish, and per-topic query cursors are all sound designs that this client would accept unchanged. The risk runs the other way. R36, R38, R45 and R10 each require **client work that does not exist yet** — a minimum-retained-cursor reader, a message-class-aware or single-cursor model, and a gRPC status classifier — and a backend built to those requirements without the matching client changes will fail silently: dropped envelopes below a retained floor, a `QueryNewest` comparison against the wrong cursor, and validation errors retried for two minutes as if they were network flaps. Sequence those client changes before the backend depends on them. Finally, this page does not cover the device-sync history server (§5.14), whose scope remains unconfirmed.
