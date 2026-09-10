//! Live conversation notifications from committed group discovery.

use super::{
    LocalEvents, Result,
    incoming::{IncomingCoordinator, IncomingScope},
};
use crate::{context::XmtpSharedContext, groups::MlsGroup};
use futures::Stream;
use std::{
    collections::{HashMap, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};
use xmtp_common::{BoxDynStream, time::sleep};
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, GroupQueryArgs, StoredGroup},
    prelude::*,
};
use xmtp_proto::{
    api_client::XmtpMlsStreams,
    types::{GroupId, Topic},
};

const ALL_CONSENT_STATES: [ConsentState; 3] = [
    ConsentState::Allowed,
    ConsentState::Unknown,
    ConsentState::Denied,
];

/// Local notification history, not durable receipt or processing progress.
#[derive(Default)]
struct KnownConversations {
    /// A later Welcome for an existing group is a new rejoin notification.
    welcomes: HashMap<GroupId, Option<i64>>,
    /// Keep one selected group per participant pair for this stream's lifetime.
    dms: HashMap<String, GroupId>,
}

impl KnownConversations {
    /// Exclude groups already committed when the live stream starts.
    fn from_groups(groups: Vec<StoredGroup>) -> Self {
        let mut known = Self::default();
        for group in groups {
            known.welcomes.insert(group.id, group.sequence_id);
            if let Some(dm_id) = group.dm_id {
                known.dms.entry(dm_id).or_insert(group.id);
            }
        }
        known
    }

    /// Observe committed discovery without letting DM list ranking change its identity.
    fn observe(&mut self, group: &StoredGroup, include_duplicate_dms: bool) -> bool {
        if self.welcomes.insert(group.id, group.sequence_id) == Some(group.sequence_id)
            || ConversationType::virtual_types().contains(&group.conversation_type)
        {
            return false;
        }
        if !include_duplicate_dms && let Some(dm_id) = &group.dm_id {
            return *self.dms.entry(dm_id.clone()).or_insert(group.id) == group.id;
        }
        true
    }
}

/// Notify one subscriber of committed local creation and Welcome joins.
pub struct StreamConversations<C: XmtpSharedContext> {
    inner: BoxDynStream<'static, Result<MlsGroup<C>>>,
}

impl<C: XmtpSharedContext + 'static> StreamConversations<C> {
    pub async fn new(
        context: &C,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        Self::new_owned(
            context.clone(),
            conversation_type,
            include_duplicate_dms,
            consent_states,
        )
        .await
    }

    /// Capture the local discovery baseline before starting the shared receiver.
    pub async fn new_owned(
        context: C,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        let events = context.local_events().subscribe();
        let known = KnownConversations::from_groups(context.db().find_groups(GroupQueryArgs {
            include_sync_groups: true,
            include_duplicate_dms: true,
            consent_states: Some(ALL_CONSENT_STATES.to_vec()),
            ..Default::default()
        })?);
        let coordinator = IncomingCoordinator::enable_stream_transport(&context);
        let lease = coordinator.acquire(IncomingScope::Topics(vec![Topic::new_welcome_message(
            context.installation_id(),
        )]));
        let query = GroupQueryArgs {
            conversation_type,
            consent_states: Some(consent_states.unwrap_or_else(|| ALL_CONSENT_STATES.to_vec())),
            include_duplicate_dms,
            ..Default::default()
        };
        let stream = futures::stream::unfold(
            (context, events, lease, known, VecDeque::new(), query),
            |(context, mut events, lease, mut known, mut ready, query)| async move {
                loop {
                    if let Some(group) = ready.pop_front() {
                        return Some((Ok(group), (context, events, lease, known, ready, query)));
                    }
                    if context.is_closed() {
                        return None;
                    }
                    let groups = if query.consent_states.as_ref().is_some_and(Vec::is_empty) {
                        Ok(Vec::new())
                    } else {
                        context.db().find_groups(&query)
                    };
                    match groups {
                        Ok(groups) => {
                            for group in groups {
                                if known.observe(&group, query.include_duplicate_dms) {
                                    ready.push_back(MlsGroup::new(
                                        context.clone(),
                                        group.id,
                                        group.dm_id,
                                        group.conversation_type,
                                        group.created_at_ns,
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            return Some((
                                Err(error.into()),
                                (context, events, lease, known, ready, query),
                            ));
                        }
                    }
                    if !ready.is_empty() {
                        continue;
                    }
                    tokio::select! {
                        _ = context.cancellation_token().cancelled() => return None,
                        _ = lease.changed() => {},
                        _ = sleep(context.stream_settings().active_database_poll_interval) => {},
                        event = events.recv() => match event {
                            Ok(LocalEvents::NewGroup(_)) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {},
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                            _ => {},
                        },
                    }
                }
            },
        );
        Ok(Self {
            inner: Box::pin(stream),
        })
    }
}

impl<C: XmtpSharedContext> Stream for StreamConversations<C> {
    type Item = Result<MlsGroup<C>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[cfg(test)]
mod test {
    use crate::utils::ClientTester;
    use std::sync::Arc;

    use super::*;
    use crate::builder::ClientBuilder;
    use crate::groups::send_message_opts::SendMessageOpts;
    use crate::tester;
    use crate::utils::fixtures::{alix, bo};
    use xmtp_db::group::GroupQueryArgs;

    use futures::StreamExt;
    use xmtp_cryptography::utils::generate_local_wallet;

    #[xmtp_common::timeout(std::time::Duration::from_secs(10))]
    #[rstest::rstest]
    #[case::two_conversations(2)]
    #[case::five_conversations(5)]
    #[xmtp_common::test]
    #[awt]
    async fn stream_welcomes(
        #[future] alix: ClientTester,
        #[future] bo: ClientTester,
        #[case] group_size: usize,
    ) {
        let mut groups = vec![];
        let mut stream = StreamConversations::new(&bo.context, None, false, None)
            .await
            .unwrap();
        for _ in 0..group_size {
            let alix_bo_group = alix.create_group(None, None).unwrap();
            groups.push(alix_bo_group.group_id);
            alix_bo_group.add_members(&[bo.inbox_id()]).await.unwrap();
        }
        while !groups.is_empty() {
            let bo_received_groups = stream.next().await.unwrap().unwrap();
            let index = groups
                .iter()
                .position(|group_id| bo_received_groups.group_id == *group_id)
                .expect("group must be found");
            groups.remove(index);
        }

        assert!(groups.is_empty(), "Groups must have all been received");
    }

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_sync_groups_are_not_streamed() {
        tester!(alix, sync_worker);
        let stream = alix.stream_conversations(None, false).await?;
        futures::pin_mut!(stream);

        tester!(_alix2, from: alix);

        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Sync group should not stream");
    }

    #[rstest::rstest]
    #[case(ConversationType::Dm, "Unexpectedly received a Group")]
    #[case(ConversationType::Group, "Unexpectedly received a DM")]
    #[xmtp_common::test]
    //TODO: case 2 consistently fails on timeout only in CI in webassembly
    // difficult to tell why. not able to repro locally.
    // CI might have issues with http connection limits
    #[cfg_attr(target_arch = "wasm32", ignore)]
    async fn test_dm_stream_filter(
        #[case] conversation_type: ConversationType,
        #[case] expected: &str,
    ) {
        tester!(alix);
        tester!(bo);
        let stream = alix
            .stream_conversations(Some(conversation_type), false)
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm(bo.inbox_id().to_string(), None)
            .await
            .unwrap();

        let group = alix.create_group(None, None).unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();

        let group = stream.next().await.unwrap();
        let metadata = group.unwrap().metadata().await.unwrap();

        assert_eq!(
            metadata.conversation_type, conversation_type,
            "{}",
            expected
        );
        // there is only one item on the stream
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "should only be one item in the stream");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_dm_stream_all_conversation_types() {
        let alix = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let bo = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let davon = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let eri = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        // Start a stream with all conversations
        let mut groups = Vec::new();
        // Wait for 2 seconds for the group creation to be streamed
        let stream = alix.stream_conversations(None, false).await.unwrap();
        futures::pin_mut!(stream);

        alix.find_or_create_dm(davon.inbox_id().to_string(), None)
            .await
            .unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let dm = eri
            .find_or_create_dm(alix.inbox_id().to_string(), None)
            .await
            .unwrap();
        dm.add_members(&[alix.inbox_id()]).await.unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        let group = alix.create_group(None, None).unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();
        let group = stream.next().await.unwrap();
        assert!(group.is_ok());
        groups.push(group.unwrap());

        assert_eq!(groups.len(), 3);
    }

    #[xmtp_common::timeout(std::time::Duration::from_secs(10))]
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_self_group_creation() {
        tester!(alix);
        tester!(bo);

        let stream = alix
            .stream_conversations(Some(ConversationType::Group), false)
            .await
            .unwrap();
        futures::pin_mut!(stream);

        alix.create_group(None, None).unwrap();
        let _self_group = stream.next().await.unwrap();

        let group = bo.create_group(None, None).unwrap();
        group.add_members(&[alix.inbox_id()]).await.unwrap();
        let _bo_group = stream.next().await.unwrap();

        // Verify syncing welcomes while streaming causes no issues
        alix.sync_welcomes().await.unwrap();
        let find_groups_results = alix.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(2, find_groups_results.len());
    }

    /// Consent filtering cannot turn a group from the live baseline into a new join.
    #[xmtp_common::test(unwrap_try = true)]
    #[rstest::rstest]
    #[case::unfiltered(None, 2)]
    #[case::allowed_only(Some(vec![ConsentState::Allowed]), 1)]
    #[case::empty_filter(Some(Vec::new()), 0)]
    async fn conversation_consent_filter_preserves_live_baseline(
        #[case] consent_states: Option<Vec<ConsentState>>,
        #[case] expected_count: usize,
    ) {
        use xmtp_common::time::{Duration, timeout};

        tester!(alix, disable_workers);
        let old = alix.create_group(None, None)?;
        old.update_consent_state(ConsentState::Denied)?;
        let mut stream = StreamConversations::new(
            &alix.context,
            Some(ConversationType::Group),
            false,
            consent_states,
        )
        .await?;

        old.update_consent_state(ConsentState::Allowed)?;
        let denied = alix.create_group(None, None)?;
        denied.update_consent_state(ConsentState::Denied)?;
        let allowed = alix.create_group(None, None)?;
        let expected = match expected_count {
            2 => vec![denied.group_id, allowed.group_id],
            1 => vec![allowed.group_id],
            _ => Vec::new(),
        };
        for group_id in expected {
            let observed = timeout(Duration::from_secs(5), stream.next()).await???;
            assert_eq!(observed.group_id, group_id);
        }
        assert!(
            timeout(Duration::from_millis(100), stream.next())
                .await
                .is_err()
        );
    }

    #[xmtp_common::timeout(std::time::Duration::from_secs(5))]
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_add_remove_re_add() {
        tester!(alix);
        tester!(bo);

        let alix_group = alix
            .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
            .await
            .unwrap();

        alix_group.remove_members(&[bo.inbox_id()]).await.unwrap();
        bo.sync_welcomes().await.unwrap();
        let stream = bo
            .stream_conversations(Some(ConversationType::Group), false)
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_group
            .add_members(&[bo.inbox_id().to_string()])
            .await
            .unwrap();

        let group_result = stream.next().await.unwrap();
        if let Err(error) = group_result {
            panic!("Error streaming group: {:?}", error);
        }
    }

    #[xmtp_common::timeout(std::time::Duration::from_secs(15))]
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_duplicate_dm_not_streamed() {
        use xmtp_cryptography::utils::generate_local_wallet;

        let client1 = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
        let client2 = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);

        let mut stream = client1.stream_conversations(None, false).await.unwrap();

        // First DM - should stream
        let dm1 = client1
            .find_or_create_dm(client2.inbox_id().to_string(), None)
            .await
            .unwrap();

        let streamed_dm1 = stream.next().await.unwrap();
        assert!(streamed_dm1.is_ok());
        assert_eq!(streamed_dm1.unwrap().group_id, dm1.group_id);

        // Create a second DM with same participants — triggers duplicate logic
        let dm2 = client2
            .find_or_create_dm(client1.inbox_id().to_string(), None)
            .await
            .unwrap();

        // Make sure it's actually a new group
        assert_ne!(dm1.group_id, dm2.group_id);

        // It should NOT appear in the stream
        let result =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), stream.next()).await;
        assert!(result.is_err(), "Duplicate DM was unexpectedly streamed");
    }

    #[xmtp_common::timeout(std::time::Duration::from_secs(120))]
    #[rstest::rstest]
    #[case::five_dms(5)]
    #[case::onehundred_dms(100)]
    #[xmtp_common::test]
    #[awt]

    async fn test_many_concurrent_dm_invites(#[future] alix: ClientTester, #[case] dms: usize) {
        let alix_inbox_id = Arc::new(alix.inbox_id().to_string());
        let mut clients = vec![];
        for _ in 0..dms {
            let client =
                Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);
            clients.push(client);
        }

        let stream = alix.stream_all_messages(None, None).await.unwrap();
        for client in clients.iter().take(dms) {
            xmtp_common::task::spawn({
                let id = alix_inbox_id.clone();
                let c = client.clone();
                async move {
                    xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
                    let dm = c.find_or_create_dm(id.as_ref(), None).await?;
                    dm.send_message(b"hi", SendMessageOpts::default()).await?;
                    Ok::<_, crate::client::ClientError>(())
                }
            });
        }
        futures::pin_mut!(stream);
        for _ in 0..dms {
            let _welcome = stream.next().await;
        }
    }
}
