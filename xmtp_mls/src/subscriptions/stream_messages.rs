use std::{collections::HashMap, future::Future, pin::Pin, task::Poll};

use super::{temp::Result, FutureWrapper, SubscribeError};
use crate::{
    api::GroupFilter,
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    storage::{
        group_message::StoredGroupMessage, refresh_state::EntityKind,
        StorageError, DbConnection
    },
    XmtpOpenMlsProvider, MlsProviderExt
};
use futures::Stream;
use pin_project_lite::pin_project;
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

type GroupId = Vec<u8>;

/// Information about the current position
/// in a stream of messages from a single group.
#[derive(Debug)]
pub(crate) struct MessagesStreamInfo {
    pub convo_created_at_ns: i64,
    pub cursor: u64,
}

pin_project! {
    pub struct StreamGroupMessages<'a, C, Subscription> {
        #[pin] inner: Subscription,
        client: &'a C,
        #[pin] state: ProcessState<'a>
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
            #[pin] future: FutureWrapper<'a, Result<StoredGroupMessage>>
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
        group_list: &HashMap<GroupId, MessagesStreamInfo>,
    ) -> Result<Self> {
        let filters: Vec<GroupFilter> = group_list
            .iter()
            .map(|(group_id, info)| GroupFilter::new(group_id.clone(), Some(info.cursor)))
            .collect();
        let subscription = client.api().subscribe_group_messages(filters).await?;

        Ok(Self {
            inner: subscription,
            client,
            state: Default::default(),
        })
    }
}

impl<'a, C, Subscription> Stream for StreamGroupMessages<'a, C, Subscription>
where
    C: ScopedGroupClient + 'a,
    Subscription: Stream<Item = std::result::Result<GroupMessage, xmtp_proto::Error>> + 'a,
{
    type Item = Result<StoredGroupMessage>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        use ProcessProject::*;
        let mut this = self.as_mut().project();

        match this.state.as_mut().project() {
            Waiting => match this.inner.poll_next(cx) {
                Ready(Some(envelope)) => {
                    let future =
                        ProcessMessageFuture::new(*this.client, envelope?)?;
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
                Ready(Ok(msg)) => {
                    this.state.set(ProcessState::Waiting);
                    Ready(Some(Ok(msg)))
                }
                Ready(Err(e)) => Ready(Some(Err(e))),
                Pending => Pending,
            },
        }
    }
}

/// Future that processes a group message from the network
pub struct ProcessMessageFuture<Client, Provider> {
    provider: Provider,
    client: Client,
    msg: group_message::V1,
}

impl<C> ProcessMessageFuture<C, XmtpOpenMlsProvider> {
    pub fn new(client: C, envelope: GroupMessage) -> Result<ProcessMessageFuture<C, XmtpOpenMlsProvider>> {
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
}

impl<C, Provider> ProcessMessageFuture<C, Provider>
where
    C: ScopedGroupClient,
    Provider: MlsProviderExt<DbConnection = DbConnection>,
{
    // allows passing an owned or referenced provider
    fn new_with_provider(client: C, envelope: GroupMessage, provider: Provider) -> Result<Self> {
        let msg = extract_message_v1(envelope)?;
        tracing::info!(
            inbox_id = client.inbox_id(),
            group_id = hex::encode(&msg.group_id),
            "Received message streaming payload"
        );

        Ok(Self {
            provider,
            client,
            msg
        })
    }

    fn inbox_id(&self) -> InboxIdRef<'_> {
        self.client.inbox_id()
    }

    pub(crate) async fn process(self) -> Result<StoredGroupMessage> {
        let group_message::V1 {
            id: ref msg_id,
            ref created_ns,
            ..
        } = self.msg;

        tracing::info!(
            inbox_id = self.inbox_id(),
            group_id = hex::encode(&self.msg.group_id),
            msg_id,
            "client [{}]  is about to process streamed envelope: [{}]",
            self.inbox_id(),
            &msg_id
        );

        if !self.has_already_synced(*msg_id).await? {
            self.process_stream_entry().await
        }

        // Load the message from the DB to handle cases where it may have been already processed in
        // another thread
        let new_message = self
            .provider
            .conn_ref()
            .get_group_message_by_timestamp(&self.msg.group_id, *created_ns as i64)?
            .ok_or(SubscribeError::GroupMessageNotFound)?;
        return Ok(new_message);
    }

    /// stream processing function
    async fn process_stream_entry(&self) {
        let process_result = self
            .client
            .store()
            .retryable_transaction_async(&self.provider, None, |provider| async move {
                let group = MlsGroup::new(
                    &self.client,
                    self.msg.group_id.clone(),
                    self.msg.created_ns as i64,
                );
                tracing::info!(
                    inbox_id = self.inbox_id(),
                    group_id = hex::encode(&self.msg.group_id),
                    msg_id = self.msg.id,
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
                msg_id = self.msg.id,
                err = e.to_string(),
                "process stream entry {:?}",
                e
            );
        } else {
            tracing::trace!(
                msg_id = self.msg.id,
                inbox_id = self.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                "message process in stream success"
            );
        }
    }

    // Checks if a message has already been processed through a sync
    async fn has_already_synced(&self, id: u64) -> Result<bool> {
        let check_for_last_cursor = || -> std::result::Result<i64, StorageError> {
            self.provider
                .conn_ref()
                .get_last_cursor_for_id(&self.msg.group_id, EntityKind::Group)
        };

        let last_id = retry_async!(Retry::default(), (async { check_for_last_cursor() }))?;
        Ok(last_id >= id as i64)
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
            msg_id = self.msg.id,
            "attempting recovery sync"
        );
        // Swallow errors here, since another process may have successfully saved the message
        // to the DB
        if let Err(err) = group.sync_with_conn(&self.provider).await {
            tracing::warn!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                msg_id = self.msg.id,
                err = %err,
                "recovery sync triggered by streamed message failed: {}", err
            );
        } else {
            tracing::debug!(
                inbox_id = self.client.inbox_id(),
                group_id = hex::encode(&self.msg.group_id),
                msg_id = self.msg.id,
                "recovery sync triggered by streamed message successful"
            )
        }
    }
}
