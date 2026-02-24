//! Stream combinator that handles `SubscribeTopicsResponse` oneof variants,
//! tracking subscription status via a shared `StreamStatus` and yielding only envelope batches.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::task::Poll;

use futures::{Stream, TryStream};
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
        loop {
            match this.inner.as_mut().try_poll_next(cx) {
                Poll::Ready(Some(Ok(response))) => {
                    this.status.touch();
                    match response.response {
                        Some(Response::Envelopes(e)) => {
                            return Poll::Ready(Some(Ok(e.envelopes)));
                        }
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
                            // Continue polling — don't yield status updates
                        }
                        None => {
                            // Continue polling — skip None responses
                        }
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, stream};
    use rstest::rstest;
    use xmtp_proto::xmtp::xmtpv4::message_api::subscribe_topics_response::{
        Envelopes, StatusUpdate, SubscriptionStatus,
    };

    use crate::protocol::EnvelopeError;

    fn envelope(payload: &[u8]) -> OriginatorEnvelope {
        OriginatorEnvelope {
            unsigned_originator_envelope: payload.to_vec(),
            proof: None,
        }
    }

    fn envelope_resp(payloads: &[&[u8]]) -> SubscribeTopicsResponse {
        SubscribeTopicsResponse {
            response: Some(Response::Envelopes(Envelopes {
                envelopes: payloads.iter().map(|p| envelope(p)).collect(),
            })),
        }
    }

    fn status_resp(status: SubscriptionStatus) -> SubscribeTopicsResponse {
        SubscribeTopicsResponse {
            response: Some(Response::StatusUpdate(StatusUpdate {
                status: status as i32,
            })),
        }
    }

    fn none_resp() -> SubscribeTopicsResponse {
        SubscribeTopicsResponse { response: None }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_initial_state() {
        let status = StreamStatus::new();
        assert!(!status.has_started());
        assert!(!status.catchup_complete());
        assert_eq!(status.last_ping_ms(), 0);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_yields_envelopes_from_envelope_response() {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> =
            vec![Ok(envelope_resp(&[b"msg1", b"msg2"]))];

        let (mut stream, _status) = status_aware(stream::iter(items));

        let result = stream.next().await.unwrap()?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].unsigned_originator_envelope, b"msg1");
        assert_eq!(result[1].unsigned_originator_envelope, b"msg2");
        assert!(stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_skips_non_envelope_responses() {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(status_resp(SubscriptionStatus::Started)),
            Ok(status_resp(SubscriptionStatus::CatchupComplete)),
            Ok(none_resp()),
            Ok(envelope_resp(&[b"payload"])),
        ];

        let (mut stream, _status) = status_aware(stream::iter(items));

        let result = stream.next().await.unwrap()?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].unsigned_originator_envelope, b"payload");
        assert!(stream.next().await.is_none());
    }

    /// Table-driven: each response type should update last_ping.
    /// Note: rstest is incompatible with unwrap_try, so these use .unwrap() directly.
    #[rstest]
    #[case::envelope(vec![Ok(envelope_resp(&[b"msg"]))])]
    #[case::status(vec![Ok(status_resp(SubscriptionStatus::Started)), Ok(envelope_resp(&[b"end"]))])]
    #[case::none(vec![Ok(none_resp()), Ok(envelope_resp(&[b"end"]))])]
    #[xmtp_common::test]
    async fn test_last_ping_updated(
        #[case] items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>>,
    ) {
        let (mut stream, status) = status_aware(stream::iter(items));
        assert_eq!(status.last_ping_ms(), 0);

        let _result = stream.next().await.unwrap().unwrap();
        assert!(status.last_ping_ms() > 0);
    }

    /// Table-driven: verify each status message sets only its corresponding flag.
    #[rstest]
    #[case::started(SubscriptionStatus::Started, true, false)]
    #[case::catchup(SubscriptionStatus::CatchupComplete, false, true)]
    #[xmtp_common::test]
    async fn test_status_flag_set_independently(
        #[case] status_msg: SubscriptionStatus,
        #[case] expect_started: bool,
        #[case] expect_catchup: bool,
    ) {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> =
            vec![Ok(status_resp(status_msg)), Ok(envelope_resp(&[b"end"]))];

        let (mut stream, status) = status_aware(stream::iter(items));
        assert!(!status.has_started());
        assert!(!status.catchup_complete());

        let _result = stream.next().await.unwrap().unwrap();
        assert_eq!(status.has_started(), expect_started);
        assert_eq!(status.catchup_complete(), expect_catchup);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_full_lifecycle() {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(status_resp(SubscriptionStatus::Started)),
            Ok(envelope_resp(&[b"batch1_msg1", b"batch1_msg2"])),
            Ok(status_resp(SubscriptionStatus::CatchupComplete)),
            Ok(none_resp()),
            Ok(envelope_resp(&[b"batch2_msg1"])),
            Ok(status_resp(SubscriptionStatus::Waiting)),
        ];

        let (mut stream, status) = status_aware(stream::iter(items));

        // First yield: batch 1 (Started is consumed, not yielded)
        let batch1 = stream.next().await.unwrap()?;
        assert_eq!(batch1.len(), 2);
        assert_eq!(batch1[0].unsigned_originator_envelope, b"batch1_msg1");
        assert_eq!(batch1[1].unsigned_originator_envelope, b"batch1_msg2");
        assert!(status.has_started());

        // Second yield: batch 2 (CatchupComplete + None consumed)
        let batch2 = stream.next().await.unwrap()?;
        assert_eq!(batch2.len(), 1);
        assert_eq!(batch2[0].unsigned_originator_envelope, b"batch2_msg1");
        assert!(status.catchup_complete());

        // Stream ends after consuming the final Waiting status
        assert!(stream.next().await.is_none());
        assert!(status.last_ping_ms() > 0);
    }
}
