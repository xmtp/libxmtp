# SubscribeTopics Migration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace `SubscribeEnvelopes` endpoint with the new `SubscribeTopics` endpoint that supports per-topic cursors and status messages, then remove all LCC (lowest common cursor) methods.

**Architecture:** New `SubscribeTopics` endpoint sends per-topic `TopicFilter`s (each with its own cursor). A new `StatusAwareStream` combinator replaces `FlattenedStream`, handling the `oneof` response (Envelopes vs StatusUpdate), tracking subscription status in `Arc<AtomicU8>`, and yielding `Vec<OriginatorEnvelope>`. The existing `OrderedStream` and `TryExtractorStream` remain unchanged.

**Tech Stack:** Rust, prost (protobuf), derive_builder, pin-project, futures, tokio

---

## PR 1: Replace SubscribeEnvelopes with SubscribeTopics

Branch: `02-23-use_new_subscribetopics_endpoint` (existing)

### Task 0: Fix proto rename build error

The proto update renamed `ArchiveOptions` to `BackupOptions` in `xmtp.device_sync`. This breaks the build.

**Files:**

- Modify: `crates/xmtp_archive/src/archive_options.rs:1-3`

**Step 1: Fix the import**

Change:

```rust
use xmtp_proto::xmtp::device_sync::{
    ArchiveOptions as BackupOptionsProto, BackupElementSelection as BackupElementSelectionProto,
};
```

To:

```rust
use xmtp_proto::xmtp::device_sync::{
    BackupOptions as BackupOptionsProto, BackupElementSelection as BackupElementSelectionProto,
};
```

**Step 2: Verify the build**

Run: `cargo check -p xmtp_archive`
Expected: PASS (no more `unresolved import` error)

**Step 3: Commit**

```bash
git add crates/xmtp_archive/src/archive_options.rs
gt modify
```

---

### Task 1: Create SubscribeTopics endpoint

**Files:**

- Create: `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs`

**Reference:** Follow the pattern in `subscribe_envelopes.rs` (derive_builder, Endpoint trait, tests).

**Step 1: Write the failing tests**

At the bottom of the new file, add tests:

```rust
#[cfg(test)]
mod test {
    use super::*;
    use xmtp_proto::{api::QueryStreamExt as _, prelude::*};

    #[xmtp_common::test]
    fn test_grpc_endpoint_returns_correct_path() {
        let endpoint = SubscribeTopics::default();
        assert_eq!(
            endpoint.grpc_endpoint(),
            "/xmtp.xmtpv4.message_api.ReplicationApi/SubscribeTopics"
        );
    }

    #[xmtp_common::test]
    fn test_body_encodes_per_topic_filters() {
        use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeTopicsRequest;
        use prost::Message as _;

        let topic = TopicKind::GroupMessagesV1.create(b"group123");
        let mut cursor = GlobalCursor::default();
        cursor.apply(&Cursor::new(5, 100));

        let endpoint = SubscribeTopics::builder()
            .filter((topic.clone(), Some(cursor.clone())))
            .build()
            .unwrap();

        let body = endpoint.body().unwrap();
        let decoded = SubscribeTopicsRequest::decode(body).unwrap();
        assert_eq!(decoded.filters.len(), 1);
        assert_eq!(decoded.filters[0].topic, topic.cloned_vec());
        assert!(decoded.filters[0].last_seen.is_some());
    }

    #[xmtp_common::test]
    fn test_empty_filters() {
        use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeTopicsRequest;
        use prost::Message as _;

        let endpoint = SubscribeTopics::default();
        let body = endpoint.body().unwrap();
        let decoded = SubscribeTopicsRequest::decode(body).unwrap();
        assert!(decoded.filters.is_empty());
    }

    #[xmtp_common::test]
    async fn test_subscribe_topics() {
        use xmtp_api_grpc::test::XmtpdClient;

        let client = XmtpdClient::create();
        let client = client.build().unwrap();

        let mut endpoint = SubscribeTopics::builder()
            .build()
            .unwrap();
        let rsp = endpoint
            .subscribe(&client)
            .await
            .inspect_err(|e| tracing::info!("{:?}", e));
        assert!(rsp.is_ok());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p xmtp_api_d14n test_grpc_endpoint_returns_correct_path -- --no-run 2>&1 | head -20`
Expected: Compilation error — `SubscribeTopics` not defined yet

**Step 3: Write the SubscribeTopics endpoint**

Write the full file `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs`:

```rust
use derive_builder::Builder;
use prost::Message;
use prost::bytes::Bytes;
use std::borrow::Cow;
use xmtp_proto::api::{BodyError, Endpoint};
use xmtp_proto::types::{Cursor, GlobalCursor, Topic, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::{
    SubscribeTopicsRequest, SubscribeTopicsResponse,
    subscribe_topics_request::TopicFilter as TopicFilterProto,
};

/// Subscribe to topics with per-topic cursors
#[derive(Debug, Builder, Default, Clone)]
#[builder(build_fn(error = "BodyError"))]
pub struct SubscribeTopics {
    #[builder(setter(each(name = "filter", into)), default)]
    filters: Vec<TopicFilterInput>,
}

/// A (Topic, Option<GlobalCursor>) pair for building subscription filters
#[derive(Debug, Clone)]
pub struct TopicFilterInput {
    pub topic: Topic,
    pub last_seen: Option<GlobalCursor>,
}

impl From<(Topic, Option<GlobalCursor>)> for TopicFilterInput {
    fn from((topic, last_seen): (Topic, Option<GlobalCursor>)) -> Self {
        Self { topic, last_seen }
    }
}

impl From<(Topic, GlobalCursor)> for TopicFilterInput {
    fn from((topic, last_seen): (Topic, GlobalCursor)) -> Self {
        Self { topic, last_seen: Some(last_seen) }
    }
}

impl SubscribeTopics {
    pub fn builder() -> SubscribeTopicsBuilder {
        Default::default()
    }
}

impl Endpoint for SubscribeTopics {
    type Output = SubscribeTopicsResponse;

    fn grpc_endpoint(&self) -> Cow<'static, str> {
        xmtp_proto::path_and_query::<SubscribeTopicsRequest>()
    }

    fn body(&self) -> Result<Bytes, BodyError> {
        let filters: Vec<TopicFilterProto> = self
            .filters
            .iter()
            .map(|f| {
                tracing::info!("subscribing to {}", f.topic.clone());
                TopicFilterProto {
                    topic: f.topic.cloned_vec(),
                    last_seen: f.last_seen.clone().map(Into::into),
                }
            })
            .collect();
        let request = SubscribeTopicsRequest { filters };
        Ok(request.encode_to_vec().into())
    }
}
```

**Step 4: Register the module**

In `crates/xmtp_api_d14n/src/endpoints/d14n/mod.rs`, add the module and re-export. Replace the subscribe_envelopes lines:

```rust
mod subscribe_envelopes;
pub use subscribe_envelopes::*;
```

With:

```rust
mod subscribe_topics;
pub use subscribe_topics::*;
```

**Step 5: Run tests**

Run: `cargo test -p xmtp_api_d14n subscribe_topics -- --nocapture`
Expected: All 4 tests pass

**Step 6: Commit**

```bash
git add crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_topics.rs crates/xmtp_api_d14n/src/endpoints/d14n/mod.rs
```

---

### Task 2: Create StatusAwareStream combinator

**Files:**

- Create: `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs`

**Reference:** Follow the pattern in `flattened.rs` (pin_project, TryStream wrapping, Stream impl).

**Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod test {
    use super::*;
    use futures::{StreamExt, stream};
    use xmtp_proto::xmtp::xmtpv4::{
        envelopes::OriginatorEnvelope,
        message_api::subscribe_topics_response::{
            Envelopes, Response, StatusUpdate, SubscriptionStatus,
        },
    };
    use crate::protocol::EnvelopeError;

    fn make_envelope_response(envelopes: Vec<OriginatorEnvelope>) -> SubscribeTopicsResponse {
        SubscribeTopicsResponse {
            response: Some(Response::Envelopes(Envelopes { envelopes })),
        }
    }

    fn make_status_response(status: SubscriptionStatus) -> SubscribeTopicsResponse {
        SubscribeTopicsResponse {
            response: Some(Response::StatusUpdate(StatusUpdate {
                status: status as i32,
            })),
        }
    }

    fn make_none_response() -> SubscribeTopicsResponse {
        SubscribeTopicsResponse { response: None }
    }

    #[xmtp_common::test]
    async fn test_yields_envelopes_from_envelope_response() {
        let env = OriginatorEnvelope::default();
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> =
            vec![Ok(make_envelope_response(vec![env.clone()]))];

        let stream = stream::iter(items);
        let (mut s, _status) = status_aware(stream);

        let result = s.next().await.unwrap().unwrap();
        assert_eq!(result.len(), 1);
    }

    #[xmtp_common::test]
    async fn test_skips_status_updates() {
        let env = OriginatorEnvelope::default();
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_status_response(SubscriptionStatus::Started)),
            Ok(make_envelope_response(vec![env.clone()])),
            Ok(make_status_response(SubscriptionStatus::CatchupComplete)),
        ];

        let stream = stream::iter(items);
        let (mut s, _status) = status_aware(stream);

        // Should skip the first status, yield the envelopes, then end
        // (the final status is consumed but stream ends)
        let result = s.next().await.unwrap().unwrap();
        assert_eq!(result.len(), 1);
        assert!(s.next().await.is_none());
    }

    #[xmtp_common::test]
    async fn test_tracks_status_transitions() {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_status_response(SubscriptionStatus::Started)),
            Ok(make_status_response(SubscriptionStatus::CatchupComplete)),
        ];

        let stream = stream::iter(items);
        let (mut s, status) = status_aware(stream);

        assert_eq!(status.load(Ordering::Relaxed), SubscriptionStatus::Unspecified as u8);

        // Consume the stream — status updates are skipped, so stream ends
        assert!(s.next().await.is_none());

        // Status should reflect the last update
        assert_eq!(status.load(Ordering::Relaxed), SubscriptionStatus::CatchupComplete as u8);
    }

    #[xmtp_common::test]
    async fn test_handles_none_response() {
        let env = OriginatorEnvelope::default();
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_none_response()),
            Ok(make_envelope_response(vec![env.clone()])),
        ];

        let stream = stream::iter(items);
        let (mut s, _status) = status_aware(stream);

        let result = s.next().await.unwrap().unwrap();
        assert_eq!(result.len(), 1);
        assert!(s.next().await.is_none());
    }

    #[xmtp_common::test]
    async fn test_mixed_envelope_and_status_messages() {
        let env1 = OriginatorEnvelope::default();
        let env2 = OriginatorEnvelope::default();
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_status_response(SubscriptionStatus::Started)),
            Ok(make_envelope_response(vec![env1])),
            Ok(make_status_response(SubscriptionStatus::CatchupComplete)),
            Ok(make_none_response()),
            Ok(make_envelope_response(vec![env2])),
            Ok(make_status_response(SubscriptionStatus::Waiting)),
        ];

        let stream = stream::iter(items);
        let (s, status) = status_aware(stream);
        let results: Vec<_> = s.map(Result::unwrap).collect().await;

        assert_eq!(results.len(), 2);
        assert_eq!(status.load(Ordering::Relaxed), SubscriptionStatus::Waiting as u8);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p xmtp_api_d14n test_yields_envelopes -- --no-run 2>&1 | head -10`
Expected: Compilation error — `status_aware` not defined

**Step 3: Write the StatusAwareStream**

Write the full file `crates/xmtp_api_d14n/src/queries/stream/status_aware.rs`:

```rust
//! Stream combinator that handles SubscribeTopicsResponse oneof,
//! tracking subscription status and yielding only envelope batches.

use futures::{Stream, TryStream};
use pin_project::pin_project;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Poll, ready};
use xmtp_proto::xmtp::xmtpv4::{
    envelopes::OriginatorEnvelope,
    message_api::{
        SubscribeTopicsResponse,
        subscribe_topics_response::Response,
    },
};

#[pin_project]
pub struct StatusAwareStream<S> {
    #[pin]
    inner: S,
    status: Arc<AtomicU8>,
}

/// Wraps a `TryStream<Ok = SubscribeTopicsResponse>` to:
/// - Yield `Vec<OriginatorEnvelope>` for envelope responses
/// - Update a shared `Arc<AtomicU8>` for status responses
/// - Skip `None` responses
///
/// Returns the stream and a status handle for external inspection.
pub fn status_aware<S>(s: S) -> (StatusAwareStream<S>, Arc<AtomicU8>)
where
    S: TryStream<Ok = SubscribeTopicsResponse>,
{
    let status = Arc::new(AtomicU8::new(0));
    let stream = StatusAwareStream {
        inner: s,
        status: status.clone(),
    };
    (stream, status)
}

impl<S> Stream for StatusAwareStream<S>
where
    S: TryStream<Ok = SubscribeTopicsResponse>,
{
    type Item = Result<Vec<OriginatorEnvelope>, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            let item = match ready!(self.as_mut().project().inner.try_poll_next(cx)) {
                Some(v) => v,
                None => return Poll::Ready(None),
            };
            let response = item?;
            match response.response {
                Some(Response::Envelopes(envelopes)) => {
                    return Poll::Ready(Some(Ok(envelopes.envelopes)));
                }
                Some(Response::StatusUpdate(update)) => {
                    self.status.store(update.status as u8, Ordering::Relaxed);
                    // Continue polling — don't yield for status updates
                    continue;
                }
                None => {
                    // Skip malformed/empty responses
                    continue;
                }
            }
        }
    }
}
```

**Step 4: Register the module**

In `crates/xmtp_api_d14n/src/queries/stream.rs` (this is the module file, which is `stream/mod.rs` or `stream.rs`), replace the flattened module with status_aware:

```rust
mod extractor;
pub use extractor::*;

mod status_aware;
pub use status_aware::*;

mod ordered;
pub use ordered::*;
```

**Step 5: Run tests**

Run: `cargo test -p xmtp_api_d14n status_aware -- --nocapture`
Expected: All 5 tests pass

**Step 6: Commit**

```bash
git add crates/xmtp_api_d14n/src/queries/stream/status_aware.rs crates/xmtp_api_d14n/src/queries/stream.rs
```

---

### Task 3: Rewire streams.rs to use SubscribeTopics + StatusAwareStream

**Files:**

- Modify: `crates/xmtp_api_d14n/src/queries/d14n/streams.rs`

**Step 1: Rewrite streams.rs**

Replace the entire file contents. Key changes:

- Import `SubscribeTopics` instead of `SubscribeEnvelopes`
- Import `StatusAwareStream` and `status_aware`
- Remove `FlattenedStream` from imports
- Remove `Paged` from imports
- Update type aliases to use `StatusAwareStream` and `SubscribeTopicsResponse`
- In each method, build `SubscribeTopics` with per-topic filters instead of flat topics + LCC
- Use `status_aware()` instead of `flattened()`

```rust
use crate::d14n::SubscribeTopics;
use crate::protocol::{CursorStore, GroupMessageExtractor, WelcomeMessageExtractor};
use crate::queries::stream;
use crate::{OrderedStream, StatusAwareStream, TryExtractorStream};

use super::D14nClient;
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, QueryStream, XmtpStream};
use xmtp_proto::api_client::XmtpMlsStreams;
use xmtp_proto::types::{GroupId, InstallationId, TopicCursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::message_api::SubscribeTopicsResponse;

type StatusStreamT<C> = StatusAwareStream<
    XmtpStream<<C as Client>::Stream, SubscribeTopicsResponse>,
>;

type OrderedStreamT<C, Store> = OrderedStream<
    StatusStreamT<C>,
    Store,
    OriginatorEnvelope,
>;

#[xmtp_common::async_trait]
impl<C, Store, E> XmtpMlsStreams for D14nClient<C, Store>
where
    C: Client<Error = E>,
    <C as Client>::Stream: 'static,
    E: RetryableError + 'static,
    Store: CursorStore + Clone,
{
    type Error = ApiClientError<E>;

    type GroupMessageStream = TryExtractorStream<OrderedStreamT<C, Store>, GroupMessageExtractor>;

    type WelcomeMessageStream = TryExtractorStream<
        StatusStreamT<C>,
        WelcomeMessageExtractor,
    >;

    async fn subscribe_group_messages(
        &self,
        group_ids: &[&GroupId],
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if group_ids.is_empty() {
            let s = SubscribeTopics::builder()
                .build()?
                .fake_stream(&self.client);
            let (s, _status) = stream::status_aware(s);
            let s = stream::ordered(
                s,
                self.cursor_store.clone(),
                TopicCursor::default(),
            );
            return Ok(stream::try_extractor(s));
        }
        let topics: Vec<_> = group_ids
            .iter()
            .map(|gid| TopicKind::GroupMessagesV1.create(gid))
            .collect();
        let topic_cursor: TopicCursor = self
            .cursor_store
            .latest_for_topics(&mut topics.iter())?
            .into();
        let mut builder = SubscribeTopics::builder();
        for (topic, cursor) in topic_cursor.iter() {
            tracing::debug!("subscribing to messages for topic {} @cursor={}", topic, cursor);
            builder.filter((topic.clone(), cursor.clone()));
        }
        let s = builder
            .build()?
            .stream(&self.client)
            .await?;
        let (s, _status) = stream::status_aware(s);
        let s = stream::ordered(
            s,
            self.cursor_store.clone(),
            topic_cursor,
        );
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_group_messages_with_cursors(
        &self,
        topics: &TopicCursor,
    ) -> Result<Self::GroupMessageStream, Self::Error> {
        if topics.is_empty() {
            let s = SubscribeTopics::builder()
                .build()?
                .fake_stream(&self.client);
            let (s, _status) = stream::status_aware(s);
            let s = stream::ordered(
                s,
                self.cursor_store.clone(),
                TopicCursor::default(),
            );
            return Ok(stream::try_extractor(s));
        }
        let mut builder = SubscribeTopics::builder();
        for (topic, cursor) in topics.iter() {
            tracing::debug!("subscribing to messages with provided cursor for topic {} @cursor={}", topic, cursor);
            builder.filter((topic.clone(), cursor.clone()));
        }
        let s = builder
            .build()?
            .stream(&self.client)
            .await?;
        let (s, _status) = stream::status_aware(s);
        let s = stream::ordered(
            s,
            self.cursor_store.clone(),
            topics.clone(),
        );
        Ok(stream::try_extractor(s))
    }

    async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<Self::WelcomeMessageStream, Self::Error> {
        if installations.is_empty() {
            let s = SubscribeTopics::builder()
                .build()?
                .fake_stream(&self.client);
            let (s, _status) = stream::status_aware(s);
            return Ok(stream::try_extractor(s));
        }
        let topics: Vec<_> = installations
            .iter()
            .map(|ins| TopicKind::WelcomeMessagesV1.create(ins))
            .collect();
        let mut builder = SubscribeTopics::builder();
        for topic in &topics {
            let cursor = self.cursor_store.latest(topic)?;
            tracing::debug!("subscribing to welcome messages for topic {} @cursor={}", topic, cursor);
            builder.filter((topic.clone(), Some(cursor)));
        }
        let s = builder
            .build()?
            .stream(&self.client)
            .await?;
        let (s, _status) = stream::status_aware(s);
        Ok(stream::try_extractor(s))
    }
}
```

Note: The `_status` handles are intentionally unused for now. They're returned by `status_aware()` so that future code can inspect subscription state. Prefix with `_` to avoid unused variable warnings.

**Step 2: Run tests to check compilation**

Run: `cargo check -p xmtp_api_d14n`
Expected: PASS. If there are type mismatches, investigate and fix.

**Step 3: Run the full test suite for the crate**

Run: `cargo test -p xmtp_api_d14n`
Expected: All tests pass. The old `test_subscribe_envelopes` test is gone (file deleted), replaced by `test_subscribe_topics`.

**Step 4: Commit**

```bash
git add crates/xmtp_api_d14n/src/queries/d14n/streams.rs
```

---

### Task 4: Delete dead code from PR 1

**Files:**

- Delete: `crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs`
- Delete: `crates/xmtp_api_d14n/src/queries/stream/flattened.rs`
- Modify: `crates/xmtp_proto/src/api_client/impls.rs:46-55` — remove `Paged` impl for `SubscribeEnvelopesResponse`
- Modify: `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs:476-530` — remove `EnvelopeCollection` impl for `SubscribeEnvelopesResponse`

**Step 1: Delete the endpoint file**

```bash
rm crates/xmtp_api_d14n/src/endpoints/d14n/subscribe_envelopes.rs
```

**Step 2: Delete the flattened stream file**

```bash
rm crates/xmtp_api_d14n/src/queries/stream/flattened.rs
```

**Step 3: Remove Paged impl for SubscribeEnvelopesResponse**

In `crates/xmtp_proto/src/api_client/impls.rs`, remove lines 46-55:

```rust
impl Paged for SubscribeEnvelopesResponse {
    type Message = OriginatorEnvelope;

    fn info(&self) -> &Option<PagingInfo> {
        &None
    }

    fn messages(self) -> Vec<Self::Message> {
        self.envelopes
    }
}
```

**Step 4: Remove EnvelopeCollection impl for SubscribeEnvelopesResponse**

In `crates/xmtp_api_d14n/src/protocol/impls/protocol_envelopes.rs`, remove lines 476-530 (the full `impl EnvelopeCollection<'_> for SubscribeEnvelopesResponse` block).

**Step 5: Fix any remaining import references**

Run: `cargo check -p xmtp_api_d14n 2>&1 | grep -i "subscribe_envelope\|SubscribeEnvelope\|FlattenedStream\|flattened"`

Fix any remaining imports of deleted types. Check:

- `crates/xmtp_api_d14n/src/lib.rs` — may re-export `FlattenedStream`
- Any test files that import `SubscribeEnvelopes`

**Step 6: Verify**

Run: `cargo check -p xmtp_api_d14n && cargo check -p xmtp_proto`
Expected: PASS

Run: `cargo test -p xmtp_api_d14n`
Expected: All tests pass

**Step 7: Commit**

```bash
git add -A
```

---

### Task 5: Lint, format, and finalize PR 1

**Step 1: Format**

Run: `dev/fmt`

**Step 2: Lint**

Run: `dev/lint`
Expected: PASS — no warnings about unused code, no clippy errors

**Step 3: Run broader test suite**

Run: `cargo test -p xmtp_mls --lib`
Expected: PASS — xmtp_mls uses `XmtpMlsStreams` trait, which we changed the implementation of

**Step 4: Commit if there are formatting changes**

```bash
git add -A
gt modify
```

---

## PR 2: LCC Cleanup + Query Migration

Create with: `gt create -am "Remove LCC methods and migrate queries to latest()"`

### Task 6: Update mls.rs queries to use latest()

**Files:**

- Modify: `crates/xmtp_api_d14n/src/queries/d14n/mls.rs`

**Step 1: Find and replace LCC calls**

In `query_group_messages` (around line 144), replace:

```rust
let lcc = self
    .cursor_store
    .lowest_common_cursor(&[&topic])?;
```

With:

```rust
let cursor = self.cursor_store.latest(&topic)?;
```

Then update the usage of this variable downstream (it's passed as `last_seen` to a query endpoint). The variable name changes from `lcc` to `cursor`.

In `query_welcome_messages` (around line 198), make the same replacement:

```rust
let lcc = self
    .cursor_store
    .lowest_common_cursor(&[&topic])?;
```

To:

```rust
let cursor = self.cursor_store.latest(&topic)?;
```

Update the downstream usage accordingly.

**Step 2: Verify compilation**

Run: `cargo check -p xmtp_api_d14n`
Expected: PASS

**Step 3: Run tests**

Run: `cargo test -p xmtp_api_d14n`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/xmtp_api_d14n/src/queries/d14n/mls.rs
```

---

### Task 7: Update network_backoff resolver

**Files:**

- Modify: `crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs`

**Step 1: Update the local lcc() function**

The function at ~line 88-108 computes a single LCC from missing dependencies. It needs to return per-topic cursors instead. Change the return type from `(Vec<Topic>, GlobalCursor)` to `Vec<(Topic, GlobalCursor)>`:

```rust
/// Get per-topic cursors from a list of missing envelopes
fn per_topic_cursors(missing: &HashSet<RequiredDependency>) -> Vec<(Topic, GlobalCursor)> {
    missing
        .iter()
        .into_grouping_map_by(|m| m.topic.clone())
        .fold(GlobalCursor::default(), |mut acc, _key, val| {
            acc.apply_least(&val.cursor);
            acc
        })
        .into_iter()
        .collect()
}
```

Then update the caller(s) of this function (in the `resolve()` method, ~line 63) to build `SubscribeTopics` with per-topic filters instead of a single LCC.

**Step 2: Verify compilation**

Run: `cargo check -p xmtp_api_d14n`
Expected: PASS

**Step 3: Run tests**

Run: `cargo test -p xmtp_api_d14n`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/xmtp_api_d14n/src/protocol/resolve/network_backoff.rs
```

---

### Task 8: Remove LCC methods from CursorStore trait and all implementations

**Files:**

- Modify: `crates/xmtp_api_d14n/src/protocol/traits/cursor_store.rs` — remove `lowest_common_cursor()` (~line 54) and `lcc_maybe_missing()` (~line 85) from trait + blanket impls for `Option<T>`, `&T`, `Arc<T>`, `Box<T>`, `NoCursorStore`
- Modify: `crates/xmtp_api_d14n/src/protocol/in_memory_cursor_store.rs` — remove `lowest_common_cursor()` method and `lcc_maybe_missing()` method from impl block, and remove the `lowest_common_cursor` test
- Modify: `crates/xmtp_mls/src/cursor_store.rs` — remove `lowest_common_cursor()` and `lcc_maybe_missing()` from `SqliteCursorStore` impl
- Modify: `crates/xmtp_db/src/encrypted_store/refresh_state.rs` — remove `lowest_common_cursor()` and `lowest_common_cursor_combined()` from `QueryRefreshState` trait and impls

**Step 1: Remove from CursorStore trait**

In `cursor_store.rs`, remove the two method declarations:

- `fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError>;`
- `fn lcc_maybe_missing(&self, topic: &[&Topic]) -> Result<GlobalCursor, CursorStoreError>;`

Remove from all blanket impls too (Option<T>, &T, Arc<T>, Box<T>, NoCursorStore).

**Step 2: Remove from InMemoryCursorStore**

In `in_memory_cursor_store.rs`, remove:

- The `lowest_common_cursor()` method on the struct impl
- The `lowest_common_cursor()` and `lcc_maybe_missing()` in the `CursorStore` impl
- Any tests that exercise `lowest_common_cursor`

**Step 3: Remove from SqliteCursorStore**

In `crates/xmtp_mls/src/cursor_store.rs`, remove the `lowest_common_cursor()` and `lcc_maybe_missing()` method impls.

**Step 4: Remove from DB layer**

In `crates/xmtp_db/src/encrypted_store/refresh_state.rs`, remove:

- `lowest_common_cursor()` from `QueryRefreshState` trait definition and impl
- `lowest_common_cursor_combined()` from trait definition and impl

**Step 5: Verify compilation**

Run: `cargo check -p xmtp_api_d14n && cargo check -p xmtp_mls && cargo check -p xmtp_db`
Expected: PASS

**Step 6: Commit**

```bash
git add -A
```

---

### Task 9: Remove TopicCursor::lcc() and gcc()

**Files:**

- Modify: `crates/xmtp_proto/src/types/topic_cursor.rs`

**Step 1: Remove both methods**

Remove the `lcc()` method (~lines 43-48) and `gcc()` method (nearby) from the impl block.

**Step 2: Verify no remaining callers**

Run: `grep -r "\.lcc()\|\.gcc()" crates/ --include="*.rs"`
Expected: No matches

**Step 3: Verify compilation**

Run: `cargo check -p xmtp_proto`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/xmtp_proto/src/types/topic_cursor.rs
```

---

### Task 10: Lint, format, and finalize PR 2

**Step 1: Format**

Run: `dev/fmt`

**Step 2: Lint**

Run: `dev/lint`
Expected: PASS

**Step 3: Run full test suite**

Run: `cargo test -p xmtp_api_d14n && cargo test -p xmtp_mls --lib && cargo test -p xmtp_db`
Expected: All pass

**Step 4: Verify no references remain**

Run: `grep -r "lowest_common_cursor\|lcc_maybe_missing\|SubscribeEnvelopes\|FlattenedStream" crates/ --include="*.rs"`
Expected: No matches (or only in comments/docs that can be cleaned up)

**Step 5: Commit and submit stack**

```bash
git add -A
gt modify
gt submit --no-interactive
```
