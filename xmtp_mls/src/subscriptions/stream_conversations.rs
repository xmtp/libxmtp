use super::{LocalEvents, Result, SubscribeError};
use crate::{
    groups::{scoped_client::ScopedGroupClient, MlsGroup},
    Client, XmtpOpenMlsProvider,
};
use xmtp_db::{group::ConversationType, refresh_state::EntityKind, NotFound};

use futures::{prelude::stream::Select, Stream};
use pin_project_lite::pin_project;
use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio_stream::wrappers::BroadcastStream;
use xmtp_common::{retry_async, FutureWrapper, Retry};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::{
    api_client::{trait_impls::XmtpApi, XmtpMlsStreams},
    xmtp::mls::api::v1::{welcome_message, WelcomeMessage},
};

#[derive(thiserror::Error, Debug)]
pub enum ConversationStreamError {
    #[error("unexpected message type in welcome")]
    InvalidPayload,
    #[error("the conversation was filtered because of the given conversation type")]
    InvalidConversationType,
}

impl xmtp_common::RetryableError for ConversationStreamError {
    fn is_retryable(&self) -> bool {
        use ConversationStreamError::*;
        match self {
            InvalidPayload | InvalidConversationType => false,
        }
    }
}

#[derive(Debug)]
pub(super) enum WelcomeOrGroup {
    Group(Vec<u8>),
    Welcome(WelcomeMessage),
}

pin_project! {
    /// Broadcast stream filtered + mapped to WelcomeOrGroup
    pub(super) struct BroadcastGroupStream {
        #[pin] inner: BroadcastStream<LocalEvents>,
    }
}

impl BroadcastGroupStream {
    fn new(inner: BroadcastStream<LocalEvents>) -> Self {
        Self { inner }
    }
}

impl Stream for BroadcastGroupStream {
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let mut this = self.project();
        // loop until the inner stream returns:
        // - Ready with a group
        // - Ready(None) - stream ended
        // ignore None values, since it is not a group, but may indicate more values in the stream
        // itself
        loop {
            if let Some(event) = ready!(this.inner.as_mut().poll_next(cx)) {
                if let Some(group) =
                    xmtp_common::optify!(event, "Missed messages due to event queue lag")
                        .and_then(LocalEvents::group_filter)
                {
                    return Ready(Some(Ok(WelcomeOrGroup::Group(group))));
                }
            } else {
                return Ready(None);
            }
        }
    }
}

pin_project! {
    /// Subscription Stream mapped to WelcomeOrGroup
    pub(super) struct SubscriptionStream<S, E> {
        #[pin] inner: S,
        _marker: std::marker::PhantomData<E>
    }
}

impl<S, E> SubscriptionStream<S, E> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<S, E> Stream for SubscriptionStream<S, E>
where
    S: Stream<Item = std::result::Result<WelcomeMessage, E>>,
    E: xmtp_proto::XmtpApiError + 'static,
{
    type Item = Result<WelcomeOrGroup>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        let this = self.project();

        match this.inner.poll_next(cx) {
            Ready(Some(welcome)) => {
                let welcome = welcome
                    .map_err(xmtp_proto::ApiError::from)
                    .map_err(SubscribeError::from)?;
                Ready(Some(Ok(WelcomeOrGroup::Welcome(welcome))))
            }
            Pending => Pending,
            Ready(None) => Ready(None),
        }
    }
}

pin_project! {
    pub struct StreamConversations<'a, C, Subscription> {
        #[pin] inner: Subscription,
        #[pin] state: ProcessState<'a, C>,
        client: C,
        conversation_type: Option<ConversationType>,
        known_welcome_ids: HashSet<i64>,
    }
}

pin_project! {
    #[project = ProcessProject]
    #[derive(Default)]
    enum ProcessState<'a, C> {
        /// State that indicates the stream is waiting on the next message from the network
        #[default]
        Waiting,
        /// State that indicates the stream is waiting on a IO/Network future to finish processing the current message
        /// before moving on to the next one
        Processing {
            #[pin] future: FutureWrapper<'a, Result<Option<(MlsGroup<C>, Option<i64>)>>>
        }
    }
}

type MultiplexedSelect<S, E> = Select<BroadcastGroupStream, SubscriptionStream<S, E>>;

pub(super) type WelcomesApiSubscription<'a, C> = MultiplexedSelect<
    <<C as ScopedGroupClient>::ApiClient as XmtpMlsStreams>::WelcomeMessageStream<'a>,
    <<C as ScopedGroupClient>::ApiClient as XmtpMlsStreams>::Error,
>;

impl<'a, A, V> StreamConversations<'a, Client<A, V>, WelcomesApiSubscription<'a, Client<A, V>>>
where
    A: XmtpApi + XmtpMlsStreams + Send + Sync + 'static,
    V: SmartContractSignatureVerifier + Send + Sync + 'static,
{
    pub async fn new(
        client: &'a Client<A, V>,
        conversation_type: Option<ConversationType>,
    ) -> Result<Self> {
        let provider = client.mls_provider()?;
        let conn = provider.conn_ref();
        let installation_key = client.installation_public_key();
        let id_cursor = provider
            .conn_ref()
            .get_last_cursor_for_id(installation_key, EntityKind::Welcome)?;
        tracing::debug!(
            cursor = id_cursor,
            inbox_id = client.inbox_id(),
            "Setting up conversation stream cursor = {}",
            id_cursor
        );

        let events =
            BroadcastGroupStream::new(BroadcastStream::new(client.local_events.subscribe()));

        let subscription = client
            .api_client
            .subscribe_welcome_messages(installation_key.as_ref(), Some(id_cursor as u64))
            .await?;
        let subscription = SubscriptionStream::new(subscription);
        let known_welcome_ids = HashSet::from_iter(conn.group_welcome_ids()?.into_iter());

        let stream = futures::stream::select(events, subscription);

        Ok(Self {
            client: client.clone(),
            inner: stream,
            known_welcome_ids,
            conversation_type,
            state: ProcessState::Waiting,
        })
    }
}

impl<'a, C, Subscription> Stream for StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone + 'a,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'a,
{
    type Item = Result<MlsGroup<C>>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll::*;
        use ProcessProject::*;

        let this = self.as_mut().project();
        let state = this.state.project();

        match state {
            Waiting => {
                match this.inner.poll_next(cx) {
                    Ready(Some(item)) => {
                        let mut this = self.as_mut().project();
                        let future = ProcessWelcomeFuture::new(
                            this.known_welcome_ids.clone(),
                            this.client.clone(),
                            item?,
                            *this.conversation_type,
                        )?;

                        this.state.set(ProcessState::Processing {
                            future: FutureWrapper::new(future.process()),
                        });
                        // try to process the future immediately
                        // this will return immediately if we have already processed the welcome
                        // and it exists in the db
                        let Processing { future } = this.state.project() else {
                            unreachable!()
                        };
                        let poll = future.poll(cx);
                        self.as_mut().try_process(poll, cx)
                    }
                    // stream ended
                    Ready(None) => Ready(None),
                    Pending => Pending,
                }
            }
            Processing { future } => {
                let poll = future.poll(cx);
                self.as_mut().try_process(poll, cx)
            }
        }
    }
}

impl<'a, C, Subscription> StreamConversations<'a, C, Subscription>
where
    C: ScopedGroupClient + Clone + 'a,
    Subscription: Stream<Item = Result<WelcomeOrGroup>> + 'a,
{
    /// Try to process the welcome future
    #[allow(clippy::type_complexity)]
    fn try_process(
        mut self: Pin<&mut Self>,
        poll: Poll<Result<Option<(MlsGroup<C>, Option<i64>)>>>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<<Self as Stream>::Item>> {
        use Poll::*;
        let mut this = self.as_mut().project();
        match poll {
            Ready(Ok(Some((group, welcome_id)))) => {
                tracing::debug!(
                    group_id = hex::encode(&group.group_id),
                    "finished processing with group {}",
                    hex::encode(&group.group_id)
                );
                if let Some(id) = welcome_id {
                    this.known_welcome_ids.insert(id);
                }
                this.state.set(ProcessState::Waiting);
                Ready(Some(Ok(group)))
            }
            // we are ignoring this payload
            Ready(Ok(None)) => {
                tracing::debug!("ignoring this payload");
                this.state.as_mut().set(ProcessState::Waiting);
                // we have to re-ad this task to the queue
                // to let http know we are waiting on the next item
                self.poll_next(cx)
            }
            Ready(Err(e)) => {
                this.state.as_mut().set(ProcessState::Waiting);
                Ready(Some(Err(e)))
            }
            Pending => Pending,
        }
    }
}

fn extract_welcome_message(welcome: &WelcomeMessage) -> Result<&welcome_message::V1> {
    match welcome.version {
        Some(welcome_message::Version::V1(ref welcome)) => Ok(welcome),
        _ => Err(ConversationStreamError::InvalidPayload.into()),
    }
}

/// Future for processing `WelcomeorGroup`
pub struct ProcessWelcomeFuture<Client> {
    /// welcome ids in DB and which are already processed
    known_welcome_ids: HashSet<i64>,
    /// The libxmtp client
    client: Client,
    /// the welcome or group being processed in this future
    item: WelcomeOrGroup,
    /// the xmtp mls provider
    provider: XmtpOpenMlsProvider,
    /// Conversation type to filter for, if any.
    conversation_type: Option<ConversationType>,
}

impl<C> ProcessWelcomeFuture<C>
where
    C: ScopedGroupClient + Clone,
{
    pub fn new(
        known_welcome_ids: HashSet<i64>,
        client: C,
        item: WelcomeOrGroup,
        conversation_type: Option<ConversationType>,
    ) -> Result<ProcessWelcomeFuture<C>> {
        let provider = client.context().mls_provider()?;

        Ok(Self {
            known_welcome_ids,
            client,
            item,
            provider,
            conversation_type,
        })
    }
}

/// bulk of the processing for a new welcome/group
impl<C> ProcessWelcomeFuture<C>
where
    C: ScopedGroupClient + Clone,
{
    /// Process the welcome. if its a group, create the group and return it.
    #[tracing::instrument(skip_all)]
    pub async fn process(self) -> Result<Option<(MlsGroup<C>, Option<i64>)>> {
        use WelcomeOrGroup::*;
        let (group, welcome_id) = match self.item {
            Welcome(ref w) => {
                let welcome = extract_welcome_message(w)?;
                let id = welcome.id as i64;
                // try to load it from store first and avoid overhead
                // of processing a welcome & erroring
                // for immediate return, this must stay in the top-level future,
                // to avoid a possible yield on the await in on_welcome.
                if self.known_welcome_ids.contains(&id) {
                    tracing::debug!(
                        "Found existing welcome. Returning from db & skipping processing"
                    );
                    let (group, id) = self.load_from_store(id).map(|(g, v)| (g, Some(v)))?;
                    let metadata = group.metadata(&self.provider).await?;
                    return Ok(self
                        .conversation_type
                        .is_none_or(|ct| ct == metadata.conversation_type)
                        .then_some((group, id)));
                }

                let (group, id) = self.on_welcome(welcome).await?;
                (group, Some(id))
            }
            Group(id) => {
                tracing::debug!("Stream conversations got existing group, pulling from db.");
                let (group, stored_group) =
                    MlsGroup::new_validated(self.client, id, &self.provider)?;
                (group, stored_group.welcome_id)
            }
        };

        let metadata = group.metadata(&self.provider).await?;
        Ok(self
            .conversation_type
            .is_none_or(|ct| ct == metadata.conversation_type)
            .then_some((group, welcome_id)))
    }

    /// process a new welcome, returning the Group & Welcome ID
    async fn on_welcome(&self, welcome: &welcome_message::V1) -> Result<(MlsGroup<C>, i64)> {
        let welcome_message::V1 {
            id,
            created_ns: _,
            ref installation_key,
            ..
        } = welcome;
        let id = *id as i64;

        let Self {
            ref client,
            ref provider,
            ..
        } = self;
        tracing::info!(
            installation_id = hex::encode(installation_key),
            welcome_id = &id,
            "Trying to process streamed welcome"
        );

        let group = retry_async!(
            Retry::default(),
            (async { MlsGroup::create_from_welcome(client, provider, welcome).await })
        );

        if let Err(e) = group {
            tracing::info!("Processing welcome failed, trying to load existing..");
            // try to load it from the store again in case of race
            return self
                .load_from_store(id)
                .map_err(|_| SubscribeError::from(e));
        }

        Ok((group?, id))
    }

    /// Load a group from disk by its welcome_id
    fn load_from_store(&self, id: i64) -> Result<(MlsGroup<C>, i64)> {
        let conn = self.provider.conn_ref();
        let group = conn
            .find_group_by_welcome_id(id)?
            .ok_or(NotFound::GroupByWelcome(id))?;
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&group.id),
            welcome_id = ?group.welcome_id,
            "loading existing group for welcome_id: {:?}",
            group.welcome_id
        );
        Ok((
            MlsGroup::new(self.client.clone(), group.id, group.created_at_ns),
            id,
        ))
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::{DMMetadataOptions, GroupMetadataOptions};
    use xmtp_db::group::GroupQueryArgs;

    use futures::StreamExt;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    async fn test_stream_welcomes() {
        let alice = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bob = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let alice_bob_group = alice
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();

        let mut stream = StreamConversations::new(&bob, None).await.unwrap();
        let group_id = alice_bob_group.group_id.clone();
        alice_bob_group
            .add_members_by_inbox_id(&[bob.inbox_id()])
            .await
            .unwrap();
        tracing::info!("WAITING FOR NEXT STREAM ITEM");
        let bob_received_groups = stream.next().await.unwrap().unwrap();
        assert_eq!(bob_received_groups.group_id, group_id);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    async fn test_dm_streaming() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let caro = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let davon = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let eri = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let stream = alix
            .stream_conversations(Some(ConversationType::Group))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm_by_inbox_id(bo.inbox_id().to_string(), DMMetadataOptions::default())
            .await
            .unwrap();
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Stream unexpectedly received a DM group");

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        let group = stream.next().await.unwrap();
        assert!(group.is_ok());

        // Start a stream with only dms
        // Start a stream with conversation_type DM
        let stream = alix
            .stream_conversations(Some(ConversationType::Dm))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();

        // we should not get a message
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Stream unexpectedly received a Group");

        alix.find_or_create_dm_by_inbox_id(
            caro.inbox_id().to_string(),
            DMMetadataOptions::default(),
        )
        .await
        .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());

        // Start a stream with all conversations
        let mut groups = Vec::new();
        // Wait for 2 seconds for the group creation to be streamed
        let stream = alix.stream_conversations(None).await.unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm_by_inbox_id(
            davon.inbox_id().to_string(),
            DMMetadataOptions::default(),
        )
        .await
        .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let dm = eri
            .find_or_create_dm_by_inbox_id(
                alix.inbox_id().to_string(),
                DMMetadataOptions::default(),
            )
            .await
            .unwrap();
        dm.add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[bo.inbox_id()])
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        assert_eq!(groups.len(), 3);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    #[timeout(std::time::Duration::from_secs(5))]
    async fn test_self_group_creation() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let stream = alix
            .stream_conversations(Some(ConversationType::Group))
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let _self_group = stream.next().await.unwrap();

        let group = bo
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        group
            .add_members_by_inbox_id(&[alix.inbox_id()])
            .await
            .unwrap();
        let _bo_group = stream.next().await.unwrap();

        // Verify syncing welcomes while streaming causes no issues
        alix.sync_welcomes(&alix.mls_provider().unwrap())
            .await
            .unwrap();
        let find_groups_results = alix.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(2, find_groups_results.len());
    }
}
