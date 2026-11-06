use std::sync::Arc;
use xmtp_mls::prelude::*;
use xmtp_mls::utils::ClientTester;

use xmtp_db::group::GroupQueryArgs;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_db::prelude::*;
use xmtp_db::{consent_record::ConsentState, group::ConversationType};
use xmtp_mls::builder::ClientBuilder;
use xmtp_mls::subscriptions::stream_conversations::*;
use xmtp_mls::tester;
use xmtp_mls::utils::fixtures::{alix, bo};
use xmtp_mls_engine::groups::send_message_opts::SendMessageOpts;
#[cfg(not(target_arch = "wasm32"))]
use xmtp_mls_engine::subscriptions::incoming::IncomingCoordinator;

use futures::StreamExt;
use xmtp_cryptography::utils::generate_local_wallet;

// verifies: CONF-075
#[xmtp_common::test(unwrap_try = true)]
async fn a_blocked_connection_fences_queued_conversations() {
    use xmtp_mls::subscriptions::SubscribeError;
    use xmtp_mls_server_config::server_configuration::BlockedConnection;

    tester!(alix, disable_workers);
    let mut stream = StreamConversations::new(&alix.context, None, false, None).await?;
    alix.create_group(None, None)?;
    alix.create_group(None, None)?;
    stream.next().await.unwrap()?;

    alix.context
        .server_configuration()
        .block_connection(BlockedConnection::ClientVersionTooOld {
            client: "1.0.0".into(),
            minimum: "9999.0.0".into(),
        });
    alix.context.cancellation_token().cancel();
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(matches!(
        error,
        SubscribeError::Blocked(BlockedConnection::ClientVersionTooOld { ref minimum, .. })
            if minimum == "9999.0.0"
    ));
    assert!(stream.next().await.is_none());
}

xmtp_common::if_native! {
// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn storage_error_ends_notifications_once_and_releases_the_lease() {
    use xmtp_db::ConnectionExt;
    tester!(alix, persistent_db, disable_workers);
    let coordinator = IncomingCoordinator::for_context(&alix.context);
    let baseline_owners = Arc::strong_count(&coordinator);
    let mut stream = StreamConversations::new(&alix.context, None, false, None).await?;
    assert_eq!(Arc::strong_count(&coordinator), baseline_owners + 1);
    alix.context.db().disconnect()?;
    let expected = alix.context.db().find_groups(GroupQueryArgs::default()).unwrap_err();
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(error.to_string(), expected.to_string());
    assert!(matches!(error, xmtp_mls::subscriptions::SubscribeError::Db(
        xmtp_db::ConnectionError::Platform(xmtp_db::PlatformStorageError::PoolNeedsConnection)
    )), "{error:?}");
    assert_eq!(Arc::strong_count(&coordinator), baseline_owners);
    assert!(stream.next().await.is_none());
    assert!(!alix.context.is_closed());

    alix.context.db().reconnect()?;
    let mut replacement = StreamConversations::new(&alix.context, None, false, None).await?;
    let group = alix.create_group(None, None)?;
    assert_eq!(replacement.next().await.unwrap()?.group_id, group.group_id);
}
}

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
#[rstest::rstest]
#[case::unfiltered(None, 2)]
#[case::allowed_only(Some(vec![ConsentState::Allowed]), 1)]
#[case::empty_filter(Some(Vec::new()), 0)]
#[xmtp_common::test(unwrap_try = true)]
async fn conversation_consent_filter_preserves_live_baseline(
    #[case] consent_states: Option<Vec<ConsentState>>,
    #[case] expected_count: usize,
) {
    use xmtp_common::time::{Duration, timeout};

    tester!(alix, disable_workers);
    let old = alix.create_group(None, None).unwrap();
    old.update_consent_state(ConsentState::Denied).unwrap();
    let mut stream = StreamConversations::new(
        &alix.context,
        Some(ConversationType::Group),
        false,
        consent_states,
    )
    .await
    .unwrap();

    old.update_consent_state(ConsentState::Allowed).unwrap();
    let denied = alix.create_group(None, None).unwrap();
    denied.update_consent_state(ConsentState::Denied).unwrap();
    let allowed = alix.create_group(None, None).unwrap();
    let expected = match expected_count {
        2 => vec![denied.group_id, allowed.group_id],
        1 => vec![allowed.group_id],
        _ => Vec::new(),
    };
    for group_id in expected {
        let observed = timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
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

    // Create a second DM with same participants — triggers duplicate logic.
    // client2 has never synced welcomes, so its database holds no DM for
    // this pair and it must build a second group rather than find dm1.
    let dm2 = client2
        .find_or_create_dm(client1.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Make sure it's actually a new group
    assert_ne!(dm1.group_id, dm2.group_id);

    // It should NOT appear in the stream. Wait longer than the stream's
    // database poll interval, otherwise the stream is merely being given
    // too little time to emit and a broken filter would still pass here.
    let wait = xmtp_configuration::ACTIVE_DATABASE_POLL_INTERVAL * 3;
    let result = xmtp_common::time::timeout(wait, stream.next()).await;
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
                Ok::<_, xmtp_mls::client::ClientError>(())
            }
        });
    }
    futures::pin_mut!(stream);
    for _ in 0..dms {
        let _welcome = stream.next().await;
    }
}
