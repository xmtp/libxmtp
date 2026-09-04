//! Stream combinator that handles `SubscribeTopicsResponse` oneof variants,
//! tracking subscription status via a shared `StreamStatus` and yielding only envelope batches.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::task::Poll;

use futures::{Stream, TryStream, stream::FusedStream};
use pin_project::pin_project;
use xmtp_proto::xmtp::xmtpv4::{
    envelopes::OriginatorEnvelope,
    message_api::{
        SubscribeTopicsResponse,
        subscribe_topics_response::{Response, SubscriptionStatus},
    },
};

/// Tracks the lifecycle and liveness of a subscription stream.
///
/// Fields are updated atomically as the stream processes server responses:
/// - `has_started` becomes `true` when a `Started` status is received
/// - `catchup_complete` becomes `true` when a `CatchupComplete` status is received
/// - `last_ping` is updated (ms since epoch) on every server response
pub struct StreamStatus {
    has_started: AtomicBool,
    catchup_complete: AtomicBool,
    last_ping: AtomicU64,
}

impl Default for StreamStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamStatus {
    pub fn new() -> Self {
        Self {
            has_started: AtomicBool::new(false),
            catchup_complete: AtomicBool::new(false),
            last_ping: AtomicU64::new(0),
        }
    }

    pub fn has_started(&self) -> bool {
        self.has_started.load(Ordering::Relaxed)
    }

    pub fn catchup_complete(&self) -> bool {
        self.catchup_complete.load(Ordering::Relaxed)
    }

    pub fn last_ping_ms(&self) -> u64 {
        self.last_ping.load(Ordering::Relaxed)
    }

    pub(crate) fn mark_started(&self) {
        self.has_started.store(true, Ordering::Relaxed);
    }

    pub(crate) fn mark_catchup_complete(&self) {
        self.catchup_complete.store(true, Ordering::Relaxed);
    }

    pub(crate) fn touch(&self) {
        self.last_ping
            .store(xmtp_common::time::now_ms(), Ordering::Relaxed);
    }
}

/// A stream combinator that wraps an inner stream of `SubscribeTopicsResponse`,
/// filtering out status updates (storing them in a shared `StreamStatus`) and `None`
/// responses, yielding only `Vec<OriginatorEnvelope>` batches.
#[pin_project]
pub struct StatusAwareStream<S> {
    #[pin]
    inner: S,
    status: Arc<StreamStatus>,
}

/// Wraps a `TryStream<Ok = SubscribeTopicsResponse>` into a `StatusAwareStream`,
/// returning the stream and a shared `Arc<StreamStatus>` handle that tracks subscription status.
///
/// The status handle is updated atomically whenever any response is received from the server.
/// The returned stream yields only `Vec<OriginatorEnvelope>` from `Envelopes` responses.
pub fn status_aware<S>(s: S) -> (StatusAwareStream<S>, Arc<StreamStatus>) {
    let status = Arc::new(StreamStatus::new());
    let stream = StatusAwareStream {
        inner: s,
        status: Arc::clone(&status),
    };
    (stream, status)
}

impl<S> Stream for StatusAwareStream<S>
where
    S: TryStream<Ok = SubscribeTopicsResponse>,
{
    type Item = Result<Vec<OriginatorEnvelope>, S::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        match std::task::ready!(this.inner.as_mut().try_poll_next(cx)) {
            Some(Ok(response)) => {
                this.status.touch();
                match response.response {
                    Some(Response::Envelopes(e)) => Poll::Ready(Some(Ok(e.envelopes))),
                    Some(Response::StatusUpdate(u)) => {
                        match SubscriptionStatus::try_from(u.status) {
                            Ok(SubscriptionStatus::Started) => {
                                this.status.mark_started();
                            }
                            Ok(SubscriptionStatus::CatchupComplete) => {
                                this.status.mark_catchup_complete();
                            }
                            _ => {}
                        }
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    None => {
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }
}

impl<S> FusedStream for StatusAwareStream<S>
where
    S: TryStream<Ok = SubscribeTopicsResponse> + FusedStream,
{
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}
