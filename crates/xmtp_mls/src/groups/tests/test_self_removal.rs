//! Self-removal and pending-remove bookkeeping.

use super::*;

// verifies: GMOD-031
#[xmtp_common::test]
async fn test_self_remove_dm_must_fail() {
    tester!(amal);
    tester!(bola);

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
    tester!(amal);

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
    tester!(amal);
    tester!(bola);

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
    tester!(amal);
    tester!(bola);

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

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal() {
    tester!(amal);
    tester!(bola_i1);
    tester!(bola_i2, from: bola_i1);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    // Verify the pending-remove list is empty on Amal's group
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();
    assert_eq!(bola_i1_group.members().await.unwrap().len(), 2);

    bola_i1_group.sync().await.unwrap();

    // Verify the pending-remove list is empty on Bola_i1's group
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();
    assert!(bola_i1_group_pending_leave_users.is_empty());

    // Verify Amal's as the super admin/admin can't leave the group and their inboxId is not added to the pendingRemoveList
    amal_group
        .leave_group()
        .await
        .expect_err("Amal should not be able to leave the group");
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    // Amal's inboxId shouldn't be in the pending-remove list
    assert!(!amal_group_pending_leave_users.contains(&amal.inbox_id().to_string()));

    // Bola_i1 should be able to leave the group
    bola_i1_group.sync().await.unwrap();
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();
    tracing::info!(
        "Bola_i1_group_pending_leave_users: {:?}",
        bola_i1_group_pending_leave_users
    );
    // Bola's inboxId should be in the pending-remove list on Bola's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    assert_eq!(bola_i1_group_pending_leave_users.len(), 1);

    // Bola's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Amal's state for the group should not change
    amal_group.sync().await.unwrap();
    let amal_group_member_state = amal
        .db()
        .find_group(&amal_group.group_id)
        .unwrap()
        .unwrap()
        .membership_state;
    assert_eq!(amal_group_member_state, GroupMembershipState::Allowed);

    // Check Bola's other installation. Note: with the event-driven self-remove,
    // the super-admin (Amal) removes Bola promptly via the TaskRunner rather than
    // after a fixed poll interval, so Bola_i2 may observe either the transient
    // PendingRemove state or the already-removed state depending on timing. We
    // therefore assert the eventual, deterministic outcome (Bola removed) rather
    // than the transient pending window.
    bola_i2.sync_welcomes().await.unwrap();
    let bola_i2_groups = bola_i2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i2_groups.len(), 1);
    let bola_i2_group = bola_i2_groups.first().unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    let _ = bola_i1_group.sync().await;
    let _ = bola_i2_group.sync().await;
    assert!(!bola_i1_group.is_active().unwrap());
    assert!(!bola_i2_group.is_active().unwrap());
    let _ = amal_group.sync().await;
    assert_eq!(amal_group.members().await.unwrap().len(), 1);
}

// verifies: EVENT-006, GMOD-031
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_simple() {
    tester!(amal);
    tester!(bola);
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    // Verify Bola's membership state is Pending when first invited
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending
    );

    let removed = bola.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationRemoved]),
        Some(10),
    );
    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove
    );

    amal_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    bola_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!bola_group.is_active().unwrap());
    assert!(matches!(
        removed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationRemoved(event)), ..
        }] if event.group_id == bola_group.group_id.as_slice()
            && event.cause == xmtp_events::RemovalCause::Left
    ));
    assert_eq!(amal_group.members().await.unwrap().len(), 1);

    // Verify Amal's membership state remains Allowed
    assert_eq!(
        amal_group.membership_state().unwrap(),
        GroupMembershipState::Allowed
    );
}

// verifies: EVENT-006, GMOD-031
#[xmtp_common::test(unwrap_try = true)]
async fn leave_request_emits_left_cause_after_explicit_sync() {
    tester!(amal, disable_workers);
    tester!(bola, disable_workers);
    let amal_group = amal.create_group(None, None)?;
    amal_group.add_members(&[bola.inbox_id()]).await?;
    let bola_group = bola.sync_welcomes().await?.pop()?;
    let removed = bola
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationRemoved,
        ]))?;

    bola_group.leave_group().await?;
    assert_eq!(
        bola_group.membership_state()?,
        GroupMembershipState::PendingRemove
    );
    amal_group.sync().await?;
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
}

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn test_membership_state_after_readd() {
    tester!(amal);
    tester!(bola);

    // Amal creates a group and adds Bola
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Bola syncs and gets the group
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();

    // Verify Bola's initial membership state is Pending
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending,
        "Bola should be in Pending state when first invited"
    );

    // Bola leaves the group
    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove,
        "Bola should be in PendingRemove state after leaving"
    );

    // Amal syncs to process the leave request
    amal_group.sync().await.unwrap();

    // Wait for admin worker to process the removal
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bola syncs to get the final removal
    bola_group.sync().await.unwrap();

    // Verify Bola's group is no longer active
    assert!(
        !bola_group.is_active().unwrap(),
        "Bola's group should be inactive after removal"
    );

    // Amal re-adds Bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Amal syncs to send the add
    amal_group.sync().await.unwrap();

    // Model an older Welcome published after the replacement Welcome.
    // This fixture changes its stored ID; it does not delay network publication.
    let delayed_welcome_sequence = i64::MAX;
    let mut stored_group = bola.context.db().find_group(&amal_group.group_id)?.unwrap();
    stored_group.sequence_id = Some(delayed_welcome_sequence);
    assert_eq!(
        bola.context
            .db()
            .insert_or_replace_group(stored_group)?
            .sequence_id,
        Some(delayed_welcome_sequence)
    );

    // Bola syncs to receive the welcome message for being re-added
    bola.sync_welcomes().await.unwrap();

    // Bola should have the group again (same ID)
    let bola_groups_after_readd = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group_after_readd = bola_groups_after_readd
        .iter()
        .find(|g| g.group_id == bola_group.group_id)
        .expect("Bola should have the group after being re-added");

    // CRITICAL: Verify Bola's membership state is Allowed (not PendingRemove)
    assert_eq!(
        bola_group_after_readd.membership_state().unwrap(),
        GroupMembershipState::Allowed,
        "Bola should be in Allowed state after being re-added, not PendingRemove"
    );

    // Verify the group is active again
    assert!(
        bola_group_after_readd.is_active().unwrap(),
        "Bola's group should be active after re-add"
    );

    // Verify consent state is Unknown (user needs to accept)
    assert_eq!(
        bola_group_after_readd.consent_state().unwrap(),
        ConsentState::Unknown,
        "Bola's consent should be Unknown after re-add, requiring explicit acceptance"
    );

    // Verify both members are back in the group
    amal_group.sync().await.unwrap();
    let members_after_readd = amal_group.members().await.unwrap();
    assert_eq!(
        members_after_readd.len(),
        2,
        "Both Amal and Bola should be in the group"
    );
}

// verifies: GMOD-034
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_group_update_message() {
    tester!(amal);
    tester!(bola);
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    bola_group.leave_group().await.unwrap();
    amal_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    bola_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!bola_group.is_active().unwrap());
    assert_eq!(amal_group.members().await.unwrap().len(), 1);
    amal_group.sync().await.unwrap();
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
    tracing::info!("{:?}", messages.len());
    let message = messages[2].clone();
    assert_eq!(message.kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 0);
    assert_eq!(group_update.removed_inboxes.len(), 0);
    assert_eq!(group_update.left_inboxes.len(), 1);
    assert_eq!(
        group_update.left_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string()
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_single_installations() {
    tester!(amal);
    tester!(bola);

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

    // Verify the pending-remove list is empty on Bola's group
    let bola_group_pending_leave_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(bola_group_pending_leave_users.is_empty());

    // Verify Amal as the super admin/admin can't leave the group and their inboxId is not added to the pendingRemoveList
    amal_group
        .leave_group()
        .await
        .expect_err("Amal should not be able to leave the group");
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    // Amal's inboxId shouldn't be in the pending-remove list
    assert!(!amal_group_pending_leave_users.contains(&amal.inbox_id().to_string()));
    // The pending-remove list should be empty
    assert!(amal_group_pending_leave_users.is_empty());

    // Bola should be able to leave the group
    bola_group.sync().await.unwrap();

    // Verify Bola's membership state is Pending when first invited
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending
    );

    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove
    );

    let bola_group_pending_leave_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on Bola's group
    assert!(bola_group_pending_leave_users.contains(&bola.inbox_id().to_string()));

    // Bola's state for the group should be set to PendingRemove
    let bola_group_from_db = bola.db().find_group(&bola_group.group_id).unwrap();
    assert_eq!(
        bola_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Amal's state for the group should not change
    amal_group.sync().await.unwrap();
    let amal_group_member_state = amal
        .db()
        .find_group(&amal_group.group_id)
        .unwrap()
        .unwrap()
        .membership_state;

    assert_eq!(amal_group_member_state, GroupMembershipState::Allowed);
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_with_multiple_initial_installations() {
    tester!(amal);
    tester!(bola_i1);
    tester!(bola_i2, from: bola_i1);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();
    bola_i2.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();

    let bola_i2_groups = bola_i2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i2_groups.len(), 1);
    let bola_i2_group = bola_i2_groups.first().unwrap();

    bola_i1_group.sync().await.unwrap();
    bola_i2_group.sync().await.unwrap();

    // Bola_i1 leaves the group
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on Bola_i1's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));

    // Bola_i1's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Check Bola's other installation (i2)
    bola_i2_group.sync().await.unwrap();
    let bola_i2_group_pending_leave_users = bola_i2
        .db()
        .get_pending_remove_users(&bola_i2_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on i2 as well
    assert!(bola_i2_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    // The pending-remove list should only contain one item
    assert_eq!(bola_i2_group_pending_leave_users.len(), 1);

    let bola_i2_group_state_in_db = bola_i2.db().find_group(&bola_i2_group.group_id).unwrap();
    // Group's state should be set to PendingRemove on Bola's other installation
    assert_eq!(
        bola_i2_group_state_in_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );
}

#[xmtp_common::test(flavor = "current_thread")]
#[ignore] // fix after consent sync
async fn test_self_removal_with_late_installation() {
    tester!(amal);
    tester!(bola_i1);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();

    bola_i1_group.sync().await.unwrap();

    // Bola_i1 leaves the group
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on Bola_i1's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));

    // Bola_i1's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Introduce another installation for Bola after the self-removal
    tester!(bola_i3, from: bola_i1);
    xmtp_common::time::sleep(std::time::Duration::from_secs(5)).await;
    bola_i1_group
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(5)).await;

    // New installation processes the welcome
    bola_i3.sync_welcomes().await.unwrap();
    let bola_i3_groups = bola_i3.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i3_groups.len(), 1);
    let bola_i3_group = bola_i3_groups.first().unwrap();
    assert_eq!(bola_i3_group.members().await.unwrap().len(), 2);

    bola_i3_group.sync().await.unwrap();

    let bola_i3_group_pending_leave_users = bola_i3
        .db()
        .get_pending_remove_users(&bola_i3_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on the new installation
    assert!(bola_i3_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    // The pending-remove list should only contain one item
    assert_eq!(bola_i3_group_pending_leave_users.len(), 1);

    let bola_i3_group_state_in_db = bola_i3.db().find_group(&bola_i1_group.group_id).unwrap();
    // Group's state should be set to PendingRemove on the new installation
    assert_eq!(
        bola_i3_group_state_in_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_clean_pending_remove_list_on_member_removal() {
    // Test that when a member is removed from the group, they are also removed from the pending_remove list
    tester!(amal);
    tester!(bola);
    tester!(caro);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Bola requests to leave the group
    bola_group.leave_group().await.unwrap();

    // Verify Bola is in the pending_remove list
    let pending_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert_eq!(pending_users.len(), 1);
    assert!(pending_users.contains(&bola.inbox_id().to_string()));

    // Amal removes Bola from the group
    amal_group.sync().await.unwrap();
    amal_group
        .remove_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();

    // Sync on all clients
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    caro_group.sync().await.unwrap();

    // Verify Bola is removed from the pending_remove list on all clients
    let amal_pending = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_pending.is_empty());

    let caro_pending = caro
        .db()
        .get_pending_remove_users(&caro_group.group_id)
        .unwrap();
    assert!(caro_pending.is_empty());

    // Verify the group members
    assert_eq!(amal_group.members().await.unwrap().len(), 2); // amal and caro

    // Verify the GroupUpdated message correctly classifies Bola as "left" (not "removed")
    // since they were in the pending_remove list
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();

    // Find all membership change messages
    let membership_messages: Vec<_> = messages
        .iter()
        .filter(|m| m.kind == GroupMessageKind::MembershipChange)
        .collect();

    // Get the last membership change message (should be Bola's removal)
    let removal_message = membership_messages
        .last()
        .expect("Should find membership change message");

    let encoded_content =
        EncodedContent::decode(removal_message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();

    // Bola should be in left_inboxes (not removed_inboxes) because they were in pending_remove
    assert_eq!(
        group_update.left_inboxes.len(),
        1,
        "Should have 1 left inbox"
    );
    assert_eq!(
        group_update.left_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string(),
        "Bola should be in left_inboxes"
    );
    assert_eq!(
        group_update.removed_inboxes.len(),
        0,
        "Should have 0 removed inboxes"
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_super_admin_promotion_marks_pending_leave_requests() {
    // Test that when a user is promoted to super_admin and there are pending remove users,
    // the group is marked as having pending leave requests.
    //
    // Workers are disabled so the TaskRunner doesn't immediately process the
    // removal (which would clear the flag) — this test asserts the transient
    // flag-marking on promotion, not the eventual removal.
    tester!(amal, disable_workers);
    tester!(bola, disable_workers);
    tester!(caro, disable_workers);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Caro requests to leave the group
    caro_group.leave_group().await.unwrap();

    // Verify Caro is in the pending_remove list
    let pending_users = caro
        .db()
        .get_pending_remove_users(&caro_group.group_id)
        .unwrap();
    assert_eq!(pending_users.len(), 1);
    assert!(pending_users.contains(&caro.inbox_id().to_string()));

    // Initially, the group should not have pending leave requests on Bola's side (not super admin)
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, None);

    // Amal promotes Bola to super_admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Bola syncs and should now be marked as having pending leave requests
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the group is marked as having pending leave requests
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, Some(true));
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_super_admin_demotion_clears_pending_leave_requests() {
    // Test that when a user is demoted from super_admin, the pending leave request flag is cleared.
    //
    // Workers are disabled so the TaskRunner doesn't process the removal and
    // clear the flag on its own — this isolates the demotion-clears-flag path.
    tester!(amal, disable_workers);
    tester!(bola, disable_workers);
    tester!(caro, disable_workers);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Amal promotes Bola to super_admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Caro requests to leave
    caro_group.leave_group().await.unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify the group is marked as having pending leave requests on Bola's side
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, Some(true));

    // Bola demotes themselves from super_admin
    bola_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            bola.inbox_id().to_string(),
        )
        .await
        .unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is no longer a super_admin
    assert!(
        !bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the pending leave request flag is cleared
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, Some(false));
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_no_status_change_when_not_in_pending_remove_list() {
    // Test that promotion to super_admin doesn't mark the group when there are no pending remove users
    tester!(amal);
    tester!(bola);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify no pending remove users
    let pending_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(pending_users.is_empty());

    // Amal promotes Bola to super_admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the group is NOT marked as having pending leave requests (no pending users)
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    // The status should be false or None since there are no pending remove users
    assert!(
        bola_group_status.has_pending_leave_request == Some(false)
            || bola_group_status.has_pending_leave_request.is_none()
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_promotion_excludes_self_from_pending_check() {
    // Test that if the promoted user is in the pending_remove list, the group is NOT marked

    tester!(amal);
    tester!(bola);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Bola requests to leave
    bola_group.leave_group().await.unwrap();

    // Verify Bola is in the pending_remove list
    let pending_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(pending_users.contains(&bola.inbox_id().to_string()));

    // Amal promotes Bola to super_admin (edge case)
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the group is NOT marked as having pending leave requests
    // (because the only pending user is Bola themselves)
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert!(
        bola_group_status.has_pending_leave_request == Some(false)
            || bola_group_status.has_pending_leave_request.is_none()
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_admin_removal_without_pending_shows_as_removed() {
    // Test that when an admin removes a member who is NOT in pending_remove,
    // they appear in removed_inboxes (not left_inboxes)
    tester!(amal);
    tester!(bola);
    tester!(caro);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Verify Bola is NOT in the pending_remove list
    let pending_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(pending_users.is_empty());

    // Amal removes Bola from the group (admin removal, not self-removal)
    amal_group
        .remove_members_by_identity(&[bola.identifier()])
        .await
        .unwrap();

    // Sync on all clients
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    caro_group.sync().await.unwrap();

    // Verify the GroupUpdated message correctly classifies Bola as "removed" (not "left")
    // since they were NOT in the pending_remove list
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();

    // Find all membership change messages
    let membership_messages: Vec<_> = messages
        .iter()
        .filter(|m| m.kind == GroupMessageKind::MembershipChange)
        .collect();

    // Get the LAST membership change message (should be Bola's removal, not the addition)
    let removal_message = membership_messages
        .last()
        .expect("Should find membership change message");

    let encoded_content =
        EncodedContent::decode(removal_message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();

    // Bola should be in removed_inboxes (not left_inboxes) because they were NOT in pending_remove
    assert_eq!(
        group_update.removed_inboxes.len(),
        1,
        "Should have 1 removed inbox"
    );
    assert_eq!(
        group_update.removed_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string(),
        "Bola should be in removed_inboxes"
    );
    assert_eq!(
        group_update.left_inboxes.len(),
        0,
        "Should have 0 left inboxes"
    );
}
