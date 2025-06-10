//! The future for Stream Messages State transitions
//!
//! A Messages Stream may be either in "Waiting", "Processing", or "Resubscribing" state.
//! The "Waiting" state indicates the stream is waiting on the next message from the network.
//! The "Processsing" state indicates the stream is waiting on a message to finish processing
//! The "Resubscribing" state indicates the stream is waiting on a group add re-subscribe to finish
//! Only valid state transitions are from Waiting to Processing, or Waiting to Resubscribing.
use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};

mod process_message_future;
mod resubscribe;

use futures::future::{try_maybe_done, FusedFuture, MaybeDone, OptionFuture, TryMaybeDone};
pub use process_message_future::*;
pub use resubscribe::*;
use xmtp_api::ApiClientWrapper;
use xmtp_proto::prelude::XmtpMlsStreams;

use super::process_message;
use crate::subscriptions::stream_messages::GroupList;
use crate::subscriptions::stream_messages::StreamGroupMessages;
use crate::subscriptions::{process_message::ProcessedMessage, SubscribeError};
use pin_project_lite::pin_project;
use xmtp_common::types::GroupId;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::xmtp::mls::api::v1::group_message;

#[derive(thiserror::Error, Debug)]
pub enum StateError {
    #[error("Invalid state transition")]
    InvalidStateTransition,
}

pin_project! {
    /// A future which will always return a valid state transition
    ///
    /// If there is no state transition, will always be ready
    /// with an Empty state transition
    pub struct State<'a, Out> {
        #[pin] inner: TryMaybeDone<InnerState<'a, Out>>
    }
}

impl<'a, Out> std::default::Default for State<'a, Out> {
    fn default() -> Self {
        State {
            inner: try_maybe_done(InnerState::Waiting),
        }
    }
}

pin_project! {
    #[project = ProjectState]
    enum InnerState<'a, Out> {
        /// state that indicates the stream is waiting on a IO/Network future to finish processing
        /// the current message before moving on to the next one
        Processing {
            #[pin] f: ProcessMessageFuture<'a>
        },
        /// Resubscribing with a new group
        Resubscribing {
            #[pin] f: ResubscribeFuture<'a, Out>
        },
        /// There is currently no transition
        Waiting
    }
}

/// Result of a State Transition
pub enum StateTransitionResult<Out> {
    /// Added a new group after resubscribing
    Added(GroupAdded<Out>),
    /// A newly processed message
    Processed(ProcessedMessage),
    /// An empty transition
    Empty,
}

impl<S> StateTransitionResult<S> {
    /// Apply a state transition to the stream.
    /// The state transition may result in a new decryptable message.
    pub fn apply_to<T>(self, target: Pin<&mut T>) -> Option<StoredGroupMessage>
    where
        T: ApplyState<GroupAdded<S>> + ApplyState<ProcessedMessage>,
    {
        match self {
            StateTransitionResult::Added(g) => <T as ApplyState<GroupAdded<S>>>::apply(target, g),
            StateTransitionResult::Processed(p) => {
                <T as ApplyState<ProcessedMessage>>::apply(target, p)
            }
            StateTransitionResult::Empty => None,
        }
    }
}

pub(super) trait ApplyState<S> {
    /// Apply a state transition to the stream group state
    /// A state transition may result in a new ready StoredGroupMessage
    fn apply(self: Pin<&mut Self>, state: S) -> Option<StoredGroupMessage>;
}

impl<'a, Out> State<'a, Out> {
    /// ensure the state is in a waiting state before inserting a new state transition.
    /// If the state not waiting, the stream may
    /// overwrite an in-progress transition resulting in missing information.
    fn ensure_waiting(&self) -> Result<(), StateError> {
        if !matches!(self.inner, TryMaybeDone::Future(InnerState::Waiting),) {
            Err(StateError::InvalidStateTransition)
        } else {
            Ok(())
        }
    }

    fn set_waiting(mut self: Pin<&mut Self>) -> Result<(), StateError> {
        // transition the state to Waiting
        // ensure that the future was finished before overwriting
        self.as_mut()
            .project()
            .inner
            .set(try_maybe_done(InnerState::Waiting));
        todo!()
    }

    /// Transition the state to process a new message
    pub fn processing(
        mut self: Pin<&mut Self>,
        factory: impl process_message::Factory<'a>,
        message: group_message::V1,
    ) -> Result<(), StateError> {
        self.ensure_waiting()?;
        let mut this = self.as_mut().project();
        this.inner.set(try_maybe_done(InnerState::Processing {
            f: ProcessMessageFuture::new(factory, message),
        }));
        Ok(())
    }

    /// Transition the state to resubscribing
    pub fn resubscribe<S>(
        mut self: Pin<&mut Self>,
        api: &'a ApiClientWrapper<S>,
        groups: &GroupList,
        group: GroupId,
    ) -> Result<(), StateError>
    where
        S: XmtpMlsStreams<GroupMessageStream = Out> + Send + Sync + 'a,
    {
        self.ensure_waiting()?;
        let mut this = self.as_mut().project();
        let fut = try_maybe_done(InnerState::Resubscribing {
            f: ResubscribeFuture::new(api, groups, group),
        });
        this.inner.set(fut);
        Ok(())
    }
}

impl<'a, Out> Future for InnerState<'a, Out> {
    type Output = Result<StateTransitionResult<Out>, SubscribeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use ProjectState::*;
        let mut this = self.as_mut().project();
        match this {
            Processing { ref mut f } => f
                .as_mut()
                .poll(cx)
                .map_ok(StateTransitionResult::<Out>::Processed),
            Resubscribing { ref mut f } => f
                .as_mut()
                .poll(cx)
                .map_ok(StateTransitionResult::<Out>::Added),
            Waiting => Poll::Ready(Ok(StateTransitionResult::Empty)),
        }
    }
}

impl<'a, Out> Future for State<'a, Out> {
    type Output = Result<StateTransitionResult<Out>, SubscribeError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();
        ready!(this.inner.as_mut().poll(cx))?;
        match this.inner.as_mut().take_output() {
            Some(o) => {
                self.set_waiting()?;
                return Poll::Ready(Ok(o));
            }
            None => {
                // we should never get none because we always replace with a valid InnerState::Waiting
                // state transition, after taking the last state transition
                return Poll::Ready(Err(StateError::InvalidStateTransition)).map_err(Into::into);
            }
        }
    }
}

// we need the fused future implementation
// so that rust wont panic
// when the future is polled in sequence
impl<'a, Out> FusedFuture for State<'a, Out> {
    fn is_terminated(&self) -> bool {
        // future will continue to return valid state transitions
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn state_never_ends() {
        todo!()
    }
}
