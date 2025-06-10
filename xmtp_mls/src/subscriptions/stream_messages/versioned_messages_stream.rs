use super::GroupList;
use crate::subscriptions::stream_messages::extract_message_v1;
use crate::subscriptions::stream_messages::MessageStreamError;
use crate::subscriptions::SubscribeError;
use futures::{stream::TryStreamExt, Stream};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::ready;
use std::task::{Context, Poll};
use xmtp_api::ApiClientWrapper;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{group_message, GroupMessage},
};

/// A versioned messages stream
/// Only returns valid envelopes
/// For instance, if only V1 envelopes are valid, returns only V1 envelopes
/// If an envelope version is invalid, returns an error.
pin_project! {
    pub struct VersionedMessagesStream<S> {
        #[pin] inner: S,
    }
}

impl<S> VersionedMessagesStream<S> {
    pub fn new(s: S) -> Self {
        Self { inner: s }
    }
}

impl<S, E> Stream for VersionedMessagesStream<S>
where
    S: Stream<Item = Result<GroupMessage, E>>,
    SubscribeError: From<E>,
{
    type Item = Result<group_message::V1, SubscribeError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(envelope) = ready!(this.inner.poll_next(cx)) {
            let envelope = envelope?;
            let extracted =
                extract_message_v1(envelope).ok_or(MessageStreamError::InvalidPayload)?;
            Poll::Ready(Some(Ok(extracted)))
        } else {
            Poll::Ready(None) // stream ended
        }
    }
}
