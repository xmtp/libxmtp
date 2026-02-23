//! Stream combinator that handles `SubscribeTopicsResponse` oneof variants,
//! tracking subscription status via a shared `AtomicU8` and yielding only envelope batches.

use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::task::Poll;

use futures::{Stream, TryStream};
use pin_project::pin_project;
use xmtp_proto::xmtp::xmtpv4::{
    envelopes::OriginatorEnvelope,
    message_api::{SubscribeTopicsResponse, subscribe_topics_response::Response},
};

/// A stream combinator that wraps an inner stream of `SubscribeTopicsResponse`,
/// filtering out status updates (storing them in a shared `AtomicU8`) and `None`
/// responses, yielding only `Vec<OriginatorEnvelope>` batches.
#[pin_project]
pub struct StatusAwareStream<S> {
    #[pin]
    inner: S,
    status: Arc<AtomicU8>,
}

/// Wraps a `TryStream<Ok = SubscribeTopicsResponse>` into a `StatusAwareStream`,
/// returning the stream and a shared `Arc<AtomicU8>` handle that tracks subscription status.
///
/// The status handle is updated atomically whenever a `StatusUpdate` message is received.
/// The returned stream yields only `Vec<OriginatorEnvelope>` from `Envelopes` responses.
pub fn status_aware<S>(s: S) -> (StatusAwareStream<S>, Arc<AtomicU8>) {
    let status = Arc::new(AtomicU8::new(0));
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
                Poll::Ready(Some(Ok(response))) => match response.response {
                    Some(Response::Envelopes(e)) => {
                        return Poll::Ready(Some(Ok(e.envelopes)));
                    }
                    Some(Response::StatusUpdate(u)) => {
                        this.status.store(u.status as u8, Ordering::Relaxed);
                        // Continue polling — don't yield status updates
                    }
                    None => {
                        // Continue polling — skip None responses
                    }
                },
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
    use std::sync::atomic::Ordering;
    use xmtp_proto::xmtp::xmtpv4::message_api::subscribe_topics_response::{
        Envelopes, StatusUpdate, SubscriptionStatus,
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

    fn make_originator_envelope(payload: &[u8]) -> OriginatorEnvelope {
        OriginatorEnvelope {
            unsigned_originator_envelope: payload.to_vec(),
            proof: None,
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_yields_envelopes_from_envelope_response() {
        let envelopes = vec![
            make_originator_envelope(b"msg1"),
            make_originator_envelope(b"msg2"),
        ];
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> =
            vec![Ok(make_envelope_response(envelopes.clone()))];

        let (mut stream, _status) = status_aware(stream::iter(items));

        let result = stream.next().await.unwrap()?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].unsigned_originator_envelope, b"msg1");
        assert_eq!(result[1].unsigned_originator_envelope, b"msg2");

        // Stream should be exhausted
        assert!(stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_skips_status_updates() {
        let envelopes = vec![make_originator_envelope(b"payload")];
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_status_response(SubscriptionStatus::Started)),
            Ok(make_status_response(SubscriptionStatus::CatchupComplete)),
            Ok(make_envelope_response(envelopes.clone())),
        ];

        let (mut stream, _status) = status_aware(stream::iter(items));

        // Should skip both status updates and yield the envelopes
        let result = stream.next().await.unwrap()?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].unsigned_originator_envelope, b"payload");

        // Stream should be exhausted
        assert!(stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_tracks_status_transitions() {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_status_response(SubscriptionStatus::Started)),
            Ok(make_status_response(SubscriptionStatus::CatchupComplete)),
            Ok(make_status_response(SubscriptionStatus::Waiting)),
            Ok(make_envelope_response(vec![make_originator_envelope(
                b"end",
            )])),
        ];

        let (mut stream, status) = status_aware(stream::iter(items));

        // Initially unspecified (0)
        assert_eq!(status.load(Ordering::Relaxed), 0);

        // Consume the stream — all status updates are processed before the envelope is yielded
        let result = stream.next().await.unwrap()?;
        assert_eq!(result.len(), 1);

        // Status should reflect the last status update (Waiting = 3)
        assert_eq!(
            status.load(Ordering::Relaxed),
            SubscriptionStatus::Waiting as u8
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_handles_none_response() {
        let none_response = SubscribeTopicsResponse { response: None };
        let envelopes = vec![make_originator_envelope(b"after_none")];
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(none_response),
            Ok(make_envelope_response(envelopes.clone())),
        ];

        let (mut stream, _status) = status_aware(stream::iter(items));

        // Should skip the None response and yield the envelopes
        let result = stream.next().await.unwrap()?;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].unsigned_originator_envelope, b"after_none");

        // Stream should be exhausted
        assert!(stream.next().await.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_mixed_envelope_and_status_messages() {
        let items: Vec<Result<SubscribeTopicsResponse, EnvelopeError>> = vec![
            Ok(make_status_response(SubscriptionStatus::Started)),
            Ok(make_envelope_response(vec![
                make_originator_envelope(b"batch1_msg1"),
                make_originator_envelope(b"batch1_msg2"),
            ])),
            Ok(make_status_response(SubscriptionStatus::CatchupComplete)),
            Ok(SubscribeTopicsResponse { response: None }),
            Ok(make_envelope_response(vec![make_originator_envelope(
                b"batch2_msg1",
            )])),
            Ok(make_status_response(SubscriptionStatus::Waiting)),
        ];

        let (mut stream, status) = status_aware(stream::iter(items));

        // First yield: batch 1 (status Started is skipped)
        let batch1 = stream.next().await.unwrap()?;
        assert_eq!(batch1.len(), 2);
        assert_eq!(batch1[0].unsigned_originator_envelope, b"batch1_msg1");
        assert_eq!(batch1[1].unsigned_originator_envelope, b"batch1_msg2");

        // Second yield: batch 2 (CatchupComplete + None are skipped)
        let batch2 = stream.next().await.unwrap()?;
        assert_eq!(batch2.len(), 1);
        assert_eq!(batch2[0].unsigned_originator_envelope, b"batch2_msg1");

        // Stream ends after consuming the final Waiting status update
        assert!(stream.next().await.is_none());

        // Final status should be Waiting (3)
        assert_eq!(
            status.load(Ordering::Relaxed),
            SubscriptionStatus::Waiting as u8
        );
    }
}
