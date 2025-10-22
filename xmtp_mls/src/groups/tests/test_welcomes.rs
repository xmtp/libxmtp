use crate::groups::send_message_opts::SendMessageOpts;
use std::sync::Arc;

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
        scw_verifier: alix.context.scw_verifier.clone(),
        device_sync: alix.context.device_sync.clone(),
        fork_recovery_opts: alix.context.fork_recovery_opts.clone(),
        workers: alix.context.workers.clone(),
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
