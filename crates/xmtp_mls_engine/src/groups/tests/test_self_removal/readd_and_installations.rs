//! Self-removal tests: readd and installations.

use super::*;

#[xmtp_common::test(flavor = "current_thread", unwrap_try = true)]
async fn test_membership_state_after_readd() {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

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

    amal_group.process_pending_self_removals().await?;

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

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_single_installations() {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

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
    let amal = EngineTestPeer::new().await;
    let bola_i1 = EngineTestPeer::new().await;
    let bola_i2 = bola_i1.new_installation().await;

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
    let amal = EngineTestPeer::new().await;
    let bola_i1 = EngineTestPeer::new().await;

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
    let bola_i3 = bola_i1.new_installation().await;
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

/// A re-add after a removal or a leave keeps the consent the user gave
/// earlier, on every installation, because a join stores nothing.
// verifies: CONS-058
#[rstest]
#[case::allowed(ConsentState::Allowed, false)]
#[case::denied_removed(ConsentState::Denied, false)]
#[case::denied_left(ConsentState::Denied, true)]
#[xmtp_common::test(unwrap_try = true)]
async fn readd_keeps_consent(#[case] state: ConsentState, #[case] leave: bool) {
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;
    let bola2 = bola.new_installation().await;
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    let bola_group = bola
        .sync_welcomes()
        .await
        .unwrap()
        .pop()
        .expect("Bola joined");
    let leaver = bola2
        .sync_welcomes()
        .await
        .unwrap()
        .pop()
        .expect("Bola2 joined");

    bola_group.update_consent_state(state).unwrap();
    assert_eq!(bola_group.consent_state().unwrap(), state);
    let decided_at = bola
        .db()
        .consent_row(&xmtp_db::consent_record::ConsentKey::group(
            bola_group.group_id.as_ref(),
        ))
        .unwrap()
        .unwrap()
        .consented_at_ns;

    if leave {
        leaver.leave_group().await.unwrap();
        amal_group.sync().await.unwrap();
        amal_group.process_pending_self_removals().await.unwrap();
    } else {
        amal_group.remove_members(&[bola.inbox_id()]).await.unwrap();
    }
    bola_group.sync().await.unwrap();
    assert!(!bola_group.is_active().unwrap());

    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();
    bola2.sync_welcomes().await.unwrap();
    assert!(
        bola.group(&bola_group.group_id)
            .unwrap()
            .is_active()
            .unwrap()
    );

    // The re-add wrote no consent on either installation: bola keeps its
    // decision and bola2, which never decided, still has no record.
    let key = xmtp_db::consent_record::ConsentKey::group(bola_group.group_id.as_ref());
    assert_eq!(
        bola.group(&bola_group.group_id)
            .unwrap()
            .consent_state()
            .unwrap(),
        state
    );
    assert_eq!(
        bola.db()
            .consent_row(&key)
            .unwrap()
            .unwrap()
            .consented_at_ns,
        decided_at
    );
    assert!(bola2.db().consent_row(&key).unwrap().is_none());
}

/// A leave request is not a reply: leaving a denied group stores no allow.
// verifies: CONS-057
#[xmtp_common::test(unwrap_try = true)]
async fn leaving_does_not_allow() {
    use xmtp_db::consent_record::ConsentKey;
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    let bola_group = bola
        .sync_welcomes()
        .await
        .unwrap()
        .pop()
        .expect("Bola joined");
    bola_group
        .update_consent_state(ConsentState::Denied)
        .unwrap();
    let key = ConsentKey::group(bola_group.group_id.as_ref());
    let denied = bola.db().consent_row(&key).unwrap().unwrap();
    bola_group.leave_group().await.unwrap();
    let after = bola.db().consent_row(&key).unwrap().unwrap();
    assert_eq!(after.state, ConsentState::Denied);
    assert_eq!(after.consented_at_ns, denied.consented_at_ns);
}
