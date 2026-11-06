//! Self-removal tests: leave rules.

use super::*;

// verifies: GMOD-031
#[xmtp_common::test]
async fn test_self_remove_dm_must_fail() {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

    // Amal creates a dm group with bola
    let amal_dm = amal
        .find_or_create_dm(bola.inbox_id().to_string(), None)
        .await
        .unwrap();
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Bola can message amal
    let _ = bola.sync_welcomes().await;
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_dm = bola_groups.first().unwrap();
    bola_dm
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();

    // Amal syncs and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");

    // Amal cannot remove bola
    let result = amal_dm.remove_members(&[bola.inbox_id()]).await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Neither Amal nor Bola is an admin or super admin
    amal_dm.sync().await.unwrap();
    bola_dm.sync().await.unwrap();
    let is_amal_admin = amal_dm.is_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_admin = amal_dm.is_admin(bola.inbox_id().to_string()).unwrap();
    let is_amal_super_admin = amal_dm.is_super_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_super_admin = amal_dm.is_super_admin(bola.inbox_id().to_string()).unwrap();
    assert!(!is_amal_admin);
    assert!(!is_bola_admin);
    assert!(!is_amal_super_admin);
    assert!(!is_bola_super_admin);

    // Neither Amal nor Bola can leave the DM
    assert_err!(
        amal_dm.leave_group().await,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::DmLeaveForbidden)
    );
    assert_err!(
        bola_dm.leave_group().await,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::DmLeaveForbidden)
    );

    bola_dm
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();

    // Amal syncs and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");
}

// verifies: GMOD-031
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_remove_group_fail_with_one_member() {
    let amal = EngineTestPeer::new().await;

    // Create a group and verify it has the default group name
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.sync().await.unwrap();

    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    let result = amal_group.leave_group().await;
    assert_err!(
        result,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::SingleMemberLeaveRejected)
    );
}

// verifies: GMOD-031
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_remove_super_admin_must_fail() {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    let result = amal_group.leave_group().await;
    assert_err!(
        result,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::SuperAdminLeaveForbidden)
    );
}

// verifies: GMOD-031
#[xmtp_common::test(flavor = "current_thread")]
async fn test_non_member_cannot_leave_group() {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

    // Create a group and verify it has the default group name
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    // Verify the pending-remove list is empty on Amal's group
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    bola_group.sync().await.unwrap();

    // Verify the pending-remove list is empty on Bola_i1's group
    let bola_group_pending_leave_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(bola_group_pending_leave_users.is_empty());

    amal_group.remove_members(&[bola.inbox_id()]).await.unwrap();
    bola_group.sync().await.unwrap();
    let bola_not_member_leave_result = bola_group.leave_group().await;
    assert_err!(
        bola_not_member_leave_result,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::NotAGroupMember)
    );
}

// verifies: GMOD-033
#[xmtp_common::test(unwrap_try = true)]
async fn a_stale_leave_removal_spares_a_readded_member() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    bo_group.sync().await?;
    let caro_group = caro.wait_for_welcomes().await?.pop()?;
    caro_group.sync().await?;
    caro_group.leave_group().await?;
    bo_group.sync().await?;
    group.sync().await?;
    group.process_pending_self_removals().await?;
    caro.rotate_and_upload_key_package().await?;
    group.add_members(&[caro.inbox_id()]).await?;

    // Bo queues from the old epoch, then rebuilds after applying both commits.
    bo_group.remove_members_pending_removal().await?;
    assert!(
        bo_group
            .members()
            .await?
            .iter()
            .any(|m| m.inbox_id == caro.inbox_id())
    );

    caro_group.sync().await?;
    caro.sync_welcomes().await?;
    caro_group.leave_group().await?;
    bo_group.sync().await?;
    bo_group.remove_members_pending_removal().await?;
    assert!(
        !bo_group
            .members()
            .await?
            .iter()
            .any(|m| m.inbox_id == caro.inbox_id())
    );
}

// verifies: EVENT-006, GMOD-031, GMOD-032
#[rstest]
#[case::direct(false)]
#[case::after_single_commit_readd(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn leave_request_emits_left_cause_after_explicit_sync(
    #[case] readd: bool,
) -> Result<(), xmtp_common::BoxDynError> {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;
    let amal_group = amal.create_group(None, None)?;
    amal_group.add_members(&[bola.inbox_id()]).await?;
    let bola_group = bola.wait_for_welcomes().await?.pop().unwrap();
    let removed = bola
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationRemoved,
        ]))
        .unwrap();

    bola_group.leave_group().await?;
    assert_eq!(
        bola_group.membership_state()?,
        GroupMembershipState::PendingRemove
    );
    amal_group.sync().await?;
    if readd {
        amal_group
            .readd_installations(vec![bola.context.installation_id().to_vec()])
            .await?;
        bola_group.sync().await?;
        bola.sync_welcomes().await?;
        assert_eq!(
            bola_group.membership_state()?,
            GroupMembershipState::PendingRemove
        );
        assert!(
            bola.db()
                .get_user_pending_remove_status(&bola_group.group_id, bola.inbox_id())?
        );
        removed.drain();
    }
    amal_group.process_pending_self_removals().await?;
    bola_group.sync().await?;

    assert!(!bola_group.is_active()?);
    assert!(matches!(
        removed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationRemoved(event)), ..
        }] if event.group_id == bola_group.group_id.as_slice()
            && event.cause == xmtp_events::RemovalCause::Left
    ));
    bola_group.sync().await?;
    assert!(removed.drain().is_empty());
    Ok(())
}
