use crate::groups::send_message_opts::SendMessageOpts;
use crate::subscriptions::stream_messages::stream_stats::StreamWithStats;
use std::sync::Arc;
use std::time::Duration;

use crate::context::XmtpMlsLocalContext;
use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::groups::MlsGroup;
use crate::groups::group_permissions::PolicySet;
use crate::groups::intents::PostCommitAction;
use crate::groups::mls_sync::decode_staged_commit;
use crate::groups::mls_sync::update_group_membership::apply_update_group_membership_intent;
use crate::identity::create_credential;
use crate::tester;
use tokio_stream::StreamExt;
use xmtp_common::time::timeout;
use xmtp_configuration::Originators;
use xmtp_db::DbQuery;
use xmtp_db::XmtpOpenMlsProviderRef;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::prelude::QueryRefreshState;
use xmtp_db::refresh_state::EntityKind;
use xmtp_mls_common::group::GroupMetadataOptions;

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_cursor() {
    // Welcomes now come with a cursor so that clients no longer pull down
    // every message in a group that they cannot decrypt.
    // This tests checks that cursor is being consumed from the welcome.
    tester!(alix);
    tester!(bo);

    let (group, _msg) = alix.test_talk_in_new_group_with(&bo).await?;

    tester!(alix2, from: alix);
    group.update_installations().await?;

    alix2.sync_welcomes().await?;
    let alix2_refresh_state = alix2.context.db().latest_cursor_for_id(
        &group.group_id,
        &[EntityKind::CommitMessage],
        None,
    )?;

    assert_eq!(alix2_refresh_state.len(), 1);
    assert!(*alix2_refresh_state.values().last().unwrap() > 0);
}

#[track_caller]
fn assert_cursors(db: &impl DbQuery, db2: &impl DbQuery, group_id: &[u8]) {
    let msg = db
        .get_group_messages(group_id, &Default::default())
        .unwrap();
    let msg = msg.last().unwrap();
    let cursor = db
        .get_last_cursor_for_ids(&[&group_id], &[EntityKind::CommitMessage])
        .unwrap()
        .values()
        .next()
        .unwrap()
        .cursor(&Originators::MLS_COMMITS);

    assert_eq!(
        msg.cursor(),
        cursor,
        "local cursor state of commits must be consistent"
    );

    let other_msg = db2
        .get_group_messages(group_id, &Default::default())
        .unwrap();
    let other_msg = other_msg.last().unwrap();
    assert_eq!(
        msg.cursor(),
        other_msg.cursor(),
        "GroupMessage must equal group message of db2"
    );
    let other_cursor = db2
        .get_last_cursor_for_ids(&[&group_id], &[EntityKind::CommitMessage])
        .unwrap()
        .values()
        .next()
        .unwrap()
        .cursor(&Originators::MLS_COMMITS);
    assert_eq!(
        cursor, other_cursor,
        "commit entry in refresh state cursor store must be equal"
    );
}

// it is very important for this behavior to be true,
// in order to maintain dependency consistency in d14n
#[xmtp_common::test(unwrap_try = true)]
async fn test_inviting_members_results_in_consistent_state() {
    use EntityKind::CommitMessage;
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;
    let group_id = &alix_group.group_id;
    assert_cursors(&alix.db(), &alix.db(), group_id);

    let bo_group = bo.sync_welcomes().await?.pop()?;
    assert_cursors(&alix.db(), &bo.db(), group_id);

    alix_group
        .add_members_by_inbox_id(&[caro.inbox_id()])
        .await?;

    let caro_group = caro.sync_welcomes().await?.pop()?;
    alix_group.sync().await?;
    assert_cursors(&caro.db(), &caro.db(), group_id);
    assert_cursors(&caro.db(), &alix.db(), group_id);

    // ensure all groups have the latest
    bo_group.sync().await?;
    alix_group.sync().await?;
    caro_group.sync().await?;
    assert_cursors(&caro.db(), &caro.db(), group_id);
    assert_cursors(&caro.db(), &bo.db(), group_id);
    assert_cursors(&caro.db(), &alix.db(), group_id);

    // alix has the membership commit
    let alix_commit = alix
        .db()
        .get_last_cursor_for_ids(&[group_id], &[CommitMessage])?;
    let bo_commit = bo
        .db()
        .get_last_cursor_for_ids(&[group_id], &[CommitMessage])?;
    let caro_commit = caro
        .db()
        .get_last_cursor_for_ids(&[group_id], &[CommitMessage])?;
    assert_eq!(bo_commit, caro_commit);
    assert_eq!(alix_commit, bo_commit);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_spoofed_inbox_id() {
    // In this scenario, Alix is malicious but Bo is not
    tester!(alix);
    tester!(bo);
    tester!(caro);

    // Our goal is simply to create a group with a credential where the inbox ID (openMLS credential)
    // does not match the installation ID (openMLS signing key)
    // To do this via libxmtp without re-implementing everything, we need to reach into some internals
    let malicious_credential = create_credential("spoofed_inbox_id").unwrap();
    let mut malicious_identity = alix.context.identity.clone();
    malicious_identity.credential = malicious_credential;
    let malicious_context = Arc::new(XmtpMlsLocalContext {
        identity: malicious_identity,
        api_client: alix.context.api_client.clone(),
        sync_api_client: alix.context.sync_api_client.clone(),
        store: alix.context.store.clone(),
        mls_storage: alix.context.mls_storage.clone(),
        mutexes: alix.context.mutexes.clone(),
        mls_commit_lock: alix.context.mls_commit_lock.clone(),
        version_info: alix.context.version_info.clone(),
        local_events: alix.context.local_events.clone(),
        worker_events: alix.context.worker_events.clone(),
        events: alix.context.events.clone(),
        scw_verifier: alix.context.scw_verifier.clone(),
        device_sync: alix.context.device_sync.clone(),
        fork_recovery_opts: alix.context.fork_recovery_opts.clone(),
        task_channels: alix.context.task_channels.clone(),
        worker_metrics: alix.context.worker_metrics.clone(),
    });
    let group = MlsGroup::create_and_insert(
        malicious_context,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
    )?;

    // Now we send a welcome from this group. To disable validation on Alix's side (as Alix is malicious),
    // we reach into some internals.
    let intent = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    let signer = &group.context.identity().installation_keys;
    let context = &group.context;
    let send_welcome_action = group
        .load_mls_group_with_lock_async(|mut openmls_group| async move {
            let publish_intent_data =
                apply_update_group_membership_intent(&context, &mut openmls_group, intent, signer)
                    .await?
                    .unwrap();
            let post_commit_action = PostCommitAction::from_bytes(
                publish_intent_data.post_commit_data().unwrap().as_slice(),
            )?;
            let PostCommitAction::SendWelcomes(action) = post_commit_action;
            let staged_commit = publish_intent_data.staged_commit().unwrap();
            openmls_group.merge_staged_commit(
                &XmtpOpenMlsProviderRef::new(context.mls_storage()),
                decode_staged_commit(staged_commit.as_slice())?,
            )?;

            Ok::<_, GroupError>(action)
        })
        .await?;
    group.send_welcomes(send_welcome_action, None).await?;

    // We want Bo to reject this welcome, because the inbox ID is spoofed
    tracing::info!("Bo is receiving now");
    let groups = bo.sync_welcomes().await?;
    if !groups.is_empty() {
        // Test is already failed if we reach this point, the rest of the test explores
        // how this can be abused
        tracing::error!("We should reject a welcome with spoofed credentials, test failed");

        let bo_group = &groups[0];
        let added_by_inbox_id = bo_group.added_by_inbox_id()?;
        // Bo thinks they were added by spoofed_inbox_id
        tracing::error!(
            "Bo thinks they were added by inbox id: {}",
            added_by_inbox_id
        );

        // Alix sends a message using their spoofed inbox ID
        group
            .send_message(
                "Message from spoofed inbox id".as_bytes(),
                SendMessageOpts::default(),
            )
            .await?;
        bo_group.sync().await?;
        let bo_msgs = bo_group.find_messages(&MsgQueryArgs::default())?;
        tracing::error!(
            "Bo received a message from {}",
            bo_msgs.first().unwrap().sender_inbox_id
        );

        // Bo and other members can continue to interact with this group as if nothing is wrong
        bo_group
            .send_message("hi".as_bytes(), SendMessageOpts::default())
            .await?;
        bo_group.add_members_by_inbox_id(&[caro.inbox_id()]).await?;
        let caro_groups = caro.sync_welcomes().await?;
        let caro_group = caro_groups.first().unwrap();
        caro_group.sync().await?;
        caro_group
            .send_message("hi".as_bytes(), SendMessageOpts::default())
            .await?;
        bo_group.sync().await?;

        panic!("Test failed");
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_welcomes_are_not_streamed_again() {
    // tester!(alix, sync_worker, sync_server);
    tester!(alix, sync_worker, sync_server, snapshot_file: "tests/assets/alix.1.5.sqlite");

    tester!(alix2, from: alix);
    tester!(bo);

    let bo_alix_group = bo
        .create_group_with_inbox_ids(&[alix.inbox_id()], None, None)
        .await?;
    let bo_alix_dm = bo
        .find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
        .await?;
    bo_alix_group
        .send_message(b"hi", Default::default())
        .await?;

    alix.sync_all_welcomes_and_groups(None).await?;
    let mut stream = alix
        .stream_all_messages_owned_with_stats(None, None)
        .await?;
    let stats = stream.stats();

    while let Ok(_) = timeout(Duration::from_millis(100), stream.next()).await {}
    let updates = stats.new_stats().await;

    let mut stream2 = alix2
        .stream_all_messages_owned_with_stats(None, None)
        .await?;
    let stats2 = stream2.stats();
    while let Ok(_) = timeout(Duration::from_millis(100), stream2.next()).await {}
    let updates = stats2.new_stats().await;

    dbg!(updates);
}
