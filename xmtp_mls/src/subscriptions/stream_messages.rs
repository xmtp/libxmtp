use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use super::{Result, SubscribeError};
use crate::{
    api::GroupFilter,
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    storage::{
        group::StoredGroup, group_message::StoredGroupMessage, refresh_state::EntityKind,
        StorageError,
    },
    XmtpOpenMlsProvider,
};
use futures::Stream;
use pin_project_lite::pin_project;
use xmtp_common::FutureWrapper;
use xmtp_common::{retry_async, Retry};
use xmtp_id::InboxIdRef;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{group_message, GroupMessage},
};

#[derive(thiserror::Error, Debug)]
pub enum MessageStreamError {
    #[error("received message for not subscribed group {id}", id = hex::encode(_0))]
    NotSubscribed(Vec<u8>),
    #[error("Invalid Payload")]
    InvalidPayload,
}

impl xmtp_common::RetryableError for MessageStreamError {
    fn is_retryable(&self) -> bool {
        use MessageStreamError::*;
        match self {
            NotSubscribed(_) | InvalidPayload => false,
        }
    }
}

fn extract_message_v1(message: GroupMessage) -> Result<group_message::V1> {
    match message.version {
        Some(group_message::Version::V1(value)) => Ok(value),
        _ => Err(MessageStreamError::InvalidPayload.into()),
    }
}

pub(super) type GroupId = Vec<u8>;

/// the position of this message in the backend topic
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessagePositionCursor(u64);

impl MessagePositionCursor {
    pub(super) fn set(&mut self, cursor: u64) {
        self.0 = cursor;
    }
}

impl std::fmt::Display for MessagePositionCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<StoredGroup> for (Vec<u8>, u64) {
    fn from(group: StoredGroup) -> (Vec<u8>, u64) {
        (group.id, 0u64)
    }
}

impl From<StoredGroup> for (Vec<u8>, MessagePositionCursor) {
    fn from(group: StoredGroup) -> (Vec<u8>, MessagePositionCursor) {
        (group.id, 0u64.into())
    }
}

impl std::ops::Deref for MessagePositionCursor {
    type Target = u64;

    fn deref(&self) -> &u64 {
        &self.0
    }
}

impl From<u64> for MessagePositionCursor {
    fn from(v: u64) -> MessagePositionCursor {
        Self(v)
    }
}

pin_project! {
    pub struct StreamGroupMessages<'a, C, Subscription> {
        #[pin] inner: Subscription,
        #[pin] state: ProcessState<'a>,
        client: &'a C,
        group_list: HashMap<GroupId, MessagePositionCursor>,
    }
}

pin_project! {
    #[project = ProcessProject]
    #[derive(Default)]
    enum ProcessState<'a> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// state that indicates the stream is waiting on a IO/Network future to finish processing
        /// the current message before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<(StoredGroupMessage, u64)>>
        }
    }
}

pub(super) type MessagesApiSubscription<'a, C> =
    <<C as ScopedGroupClient>::ApiClient as XmtpMlsStreams>::GroupMessageStream<'a>;

impl<'a, C> StreamGroupMessages<'a, C, MessagesApiSubscription<'a, C>>
where
    C: ScopedGroupClient + 'a,
    <C as ScopedGroupClient>::ApiClient: XmtpApi + XmtpMlsStreams + 'a,
{
    pub async fn new(
        client: &'a C,
        group_list: HashMap<GroupId, MessagePositionCursor>,
    ) -> Result<Self> {
        let filters: Vec<GroupFilter> = group_list
            .iter()
            .map(|(group_id, cursor)| GroupFilter::new(group_id.clone(), Some(**cursor)))
            .collect();
        let subscription = client.api().subscribe_group_messages(filters).await?;

        Ok(Self {
            inner: subscription,
            client,
            state: Default::default(),
            group_list: group_list.into_iter().map(|(g, c)| (g, c.into())).collect(),
        })
    }
}

impl<'a, C, Subscription> Stream for StreamGroupMessages<'a, C, Subscription>
where
    C: ScopedGroupClient + 'a,
    Subscription: Stream<Item = std::result::Result<GroupMessage, xmtp_proto::Error>> + 'a,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // tracing::debug!("POLLING STREAM MESSAGES");
        use std::task::Poll::*;
        use ProcessProject::*;
        let mut this = self.as_mut().project();

        match this.state.as_mut().project() {
            Waiting => match this.inner.poll_next(cx) {
                Ready(Some(envelope)) => {
                    tracing::debug!("processing message in stream");
                    let future = ProcessMessageFuture::new(*this.client, envelope?)?;
                    let future = future.process();
                    this.state.set(ProcessState::Processing {
                        future: FutureWrapper::new(future),
                    });
                    cx.waker().wake_by_ref();
                    Pending
                }
                Pending => {
                    cx.waker().wake_by_ref();
                    Pending
                }
                Ready(None) => Ready(None),
            },
            Processing { future } => match future.poll(cx) {
                Ready(Ok((msg, new_cursor))) => {
                    this.state.set(ProcessState::Waiting);
                    if let Some(tracked_cursor) = this.group_list.get_mut(&msg.group_id) {
                        tracked_cursor.set(new_cursor)
                    } else {
                        this.group_list
                            .insert(msg.group_id.clone(), new_cursor.into());
                    }
                    Ready(Some(Ok(msg)))
                }
                // skip if payload GroupMessageNotFound
                Ready(Err(SubscribeError::GroupMessageNotFound)) => {
                    tracing::warn!("skipping message streaming payload");
                    this.state.set(ProcessState::Waiting);
                    cx.waker().wake_by_ref();
                    Pending
                }
                Ready(Err(e)) => Ready(Some(Err(e))),
                Pending => {
                    cx.waker().wake_by_ref();
                    Pending
                }
            },
        }
    }
}

impl<'a, C, S> StreamGroupMessages<'a, C, S> {
    pub(super) fn group_list(&self) -> &HashMap<GroupId, MessagePositionCursor> {
        &self.group_list
    }
}

impl<'a, C, S> StreamGroupMessages<'a, C, S>
where
    S: Stream<Item = std::result::Result<GroupMessage, xmtp_proto::Error>> + 'a,
    C: ScopedGroupClient + 'a,
{
    pub(super) fn drain(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Vec<Option<Result<StoredGroupMessage>>> {
        let mut drained = Vec::new();
        while let Poll::Ready(msg) = self.as_mut().poll_next(cx) {
            drained.push(msg);
        }
        drained
    }
}

/// Future that processes a group message from the network
pub struct ProcessMessageFuture<Client> {
    provider: XmtpOpenMlsProvider,
    client: Client,
    msg: group_message::V1,
}

impl<C> ProcessMessageFuture<C>
where
    C: ScopedGroupClient,
{
    /// Create a new Future to process a GroupMessage.
    pub fn new(client: C, envelope: GroupMessage) -> Result<ProcessMessageFuture<C>> {
        let msg = extract_message_v1(envelope)?;
        let provider = client.mls_provider()?;
        tracing::info!(
            inbox_id = client.inbox_id(),
            group_id = hex::encode(&msg.group_id),
            "Received message streaming payload"
        );

        Ok(Self {
            provider,
            client,
            msg,
        })
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.client.inbox_id()
    }

    /// process a message, returning the message from the database and the cursor of the message.
    pub(crate) async fn process(self) -> Result<(StoredGroupMessage, u64)> {
        let group_message::V1 {
            // the cursor ID is the position in the monolithic backend topic
            id: ref cursor_id,
            ref created_ns,
            ..
        } = self.msg;

        tracing::info!(
            inbox_id = self.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id,
            "client [{}]  is about to process streamed envelope: [{}]",
            self.inbox_id(),
            &cursor_id
        );

        if self.needs_to_sync(*cursor_id).await? {
            self.process_stream_entry().await
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .provider
            .conn_ref()
            .get_group_message_by_timestamp(&self.msg.group_id, *created_ns as i64)?
            .ok_or(SubscribeError::GroupMessageNotFound)
            .inspect_err(|e| {
                if matches!(e, SubscribeError::GroupMessageNotFound) {
                    tracing::warn!(
                        cursor_id,
                        inbox_id = self.inbox_id(),
                        group_id = hex::encode(&self.msg.group_id),
                        "group message not found"
                    );
                }
            })?;
        return Ok((new_message, *cursor_id));
    }

    /// stream processing function
    async fn process_stream_entry(&self) {
        let process_result = self
            .client
            .store()
            .retryable_transaction_async(&self.provider, |provider| async move {
                let (group, _) =
                    MlsGroup::new_validated(&self.client, self.msg.group_id.clone(), provider)?;
                tracing::info!(
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    cursor_id = self.msg.id,
                    "current epoch for [{}] in process_stream_entry()",
                    self.inbox_id(),
                );
                group
                    .process_message(provider, &self.msg, false)
                    .await
                    // NOTE: We want to make sure we retry an error in process_message
                    .map_err(SubscribeError::ReceiveGroup)
            })
            .await;

        if let Err(SubscribeError::ReceiveGroup(_)) = process_result {
            self.attempt_message_recovery().await
        // This should never occur because we map the error to `ReceiveGroup`
        // But still exists defensively
        } else if let Err(e) = process_result {
            tracing::error!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                err = e.to_string(),
                "process stream entry {:?}",
                e
            );
        } else {
            tracing::trace!(
                cursor_id = self.msg.id,
                inbox_id = self.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                "message process in stream success"
            );
        }
    }

    /// Checks if a message has already been processed through a sync
    async fn needs_to_sync(&self, current_msg_cursor: u64) -> Result<bool> {
        let check_for_last_cursor = || -> std::result::Result<i64, StorageError> {
            self.provider
                .conn_ref()
                .get_last_cursor_for_id(&self.msg.group_id, EntityKind::Group)
        };

        let last_synced_id = retry_async!(Retry::default(), (async { check_for_last_cursor() }))?;
        Ok(last_synced_id < current_msg_cursor as i64)
    }

    /// Attempt a recovery sync if a group message failed to process
    async fn attempt_message_recovery(&self) {
        let group = MlsGroup::new(
            &self.client,
            self.msg.group_id.clone(),
            self.msg.created_ns as i64,
        );
        tracing::debug!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            cursor_id = self.msg.id,
            "attempting recovery sync"
        );
        // Swallow errors here, since another process may have successfully saved the message
        // to the DB
        if let Err(err) = group.sync_with_conn(&self.provider).await {
            tracing::warn!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                err = %err,
                "recovery sync triggered by streamed message failed: {}", err
            );
        } else {
            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                cursor_id = self.msg.id,
                "recovery sync triggered by streamed message successful"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::stream::StreamExt;
    use wasm_bindgen_test::wasm_bindgen_test;

    use crate::assert_msg;
    use crate::{builder::ClientBuilder, groups::GroupMetadataOptions};
    use xmtp_cryptography::utils::generate_local_wallet;

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "current_thread"))]
    async fn test_stream_messages() {
        xmtp_common::logger();
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alice_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        tracing::info!("Group Id = [{}]", hex::encode(&alice_group.group_id));

        alice_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        let bob_groups = bob
            .sync_welcomes(&bob.mls_provider().unwrap())
            .await
            .unwrap();
        let bob_group = bob_groups.first().unwrap();
        alice_group.sync().await.unwrap();

        let stream = alice_group.stream().await.unwrap();
        futures::pin_mut!(stream);
        bob_group.send_message(b"hello").await.unwrap();

        // implicitly skips the first message (add bob to group message)
        // since that is an epoch increment.
        assert_msg!(stream, "hello");

        bob_group.send_message(b"hello2").await.unwrap();
        assert_msg!(stream, "hello2");
    }
}
