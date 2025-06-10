use super::process_message;
use crate::subscriptions::stream_messages::state::ApplyState;
use crate::subscriptions::stream_messages::StreamGroupMessages;
use crate::subscriptions::{process_message::ProcessedMessage, SubscribeError};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use xmtp_common::FutureWrapper;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::xmtp::mls::api::v1::group_message;

pin_project! {
    pub struct ProcessMessageFuture<'a> {
        #[pin] pub wrapped: FutureWrapper<'a, Result<ProcessedMessage, SubscribeError>>,
        pub msg: group_message::V1,
    }
}

impl<'a> ProcessMessageFuture<'a> {
    pub fn new(factory: impl process_message::Factory<'a>, message: group_message::V1) -> Self {
        Self {
            wrapped: factory.create(message.clone()),
            msg: message,
        }
    }
}

// after we process a message
// set the cursor of the group to the message we processed.
// if processing resulted in a new decryptable message, return it.
impl<'a, A, D, S, F> ApplyState<ProcessedMessage> for StreamGroupMessages<'a, A, D, S, F> {
    fn apply(mut self: Pin<&mut Self>, state: ProcessedMessage) -> Option<StoredGroupMessage> {
        let mut this = self.as_mut().project();
        this.groups.set(&state.group_id, state.next_message);
        if let Some(msg) = state.message {
            let id = msg.sequence_id.map(|s| s as u64).unwrap_or(0u64);
            this.returned.push(id);
            Some(msg)
        } else {
            None
        }
    }
}

impl<'a> Future for ProcessMessageFuture<'a> {
    type Output = Result<ProcessedMessage, SubscribeError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();
        this.wrapped.poll(cx)
    }
}
