//! The future and state transition governing resubscribing to a Stream
use super::StateTransitionResult;
use crate::subscriptions::stream_messages::state::ApplyState;
use crate::subscriptions::stream_messages::VersionedMessagesStream;
use crate::subscriptions::stream_messages::{GroupList, StreamGroupMessages};
use crate::subscriptions::SubscribeError;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use xmtp_api::{ApiClientWrapper, GroupFilter};
use xmtp_common::types::GroupId;
use xmtp_common::FutureWrapper;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::prelude::XmtpMlsStreams;

pin_project! {
    pub struct ResubscribeFuture<'a, Out> {
        #[pin] pub wrapped: FutureWrapper<'a, Result<GroupAdded<Out>, SubscribeError>>,
    }
}

/// A group was added to a stream
pub struct GroupAdded<Out> {
    new_stream: Out,
    new_group: GroupId,
    cursor: u64,
}

impl<Out> From<GroupAdded<Out>> for StateTransitionResult<Out> {
    fn from(value: GroupAdded<Out>) -> StateTransitionResult<Out> {
        StateTransitionResult::<Out>::Added(value)
    }
}

impl<'a, Out> ResubscribeFuture<'a, Out> {
    pub fn new<S>(api: &'a ApiClientWrapper<S>, groups: &GroupList, group: GroupId) -> Self
    where
        S: XmtpMlsStreams<GroupMessageStream = Out> + Send + Sync + 'a,
    {
        let filters: Vec<GroupFilter> = groups.filters();
        let future = async move {
            api.subscribe_group_messages(filters)
                .await
                .map(|s| GroupAdded {
                    new_stream: s,
                    new_group: group,
                    cursor: 1,
                })
                .map_err(|e| SubscribeError::BoxError(Box::new(e)))
        };
        Self {
            wrapped: FutureWrapper::new(future),
        }
    }
}

impl<'a, A, D, S, F> ApplyState<GroupAdded<S>> for StreamGroupMessages<'a, A, D, S, F> {
    fn apply(mut self: Pin<&mut Self>, state: GroupAdded<S>) -> Option<StoredGroupMessage> {
        let GroupAdded::<S> {
            new_stream,
            new_group,
            cursor,
        } = state;
        let mut this = self.as_mut().project();
        this.groups.set(&new_group, cursor);
        this.inner.set(VersionedMessagesStream::new(new_stream));
        if let Some(cursor) = self.as_ref().groups.position(&new_group) {
            tracing::debug!(
                "added group_id={} at cursor={} to messages stream",
                hex::encode(&new_group),
                cursor
            );
        }
        // no messages may be produced from adding a group
        None
    }
}

impl<'a, Out> Future for ResubscribeFuture<'a, Out> {
    type Output = Result<GroupAdded<Out>, SubscribeError>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<<Self as futures::Future>::Output> {
        let this = self.as_mut().project();
        this.wrapped.poll(cx)
    }
}
