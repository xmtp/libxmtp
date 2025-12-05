//! Test for self-removal and PendingRemove membership state

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_self_removal_with_pending_state() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates a group and adds Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs and gets the group
    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Verify Bo's membership state is Pending when first invited
    let bo_state_initial = bo_group.membership_state().unwrap();
    assert_eq!(bo_state_initial, FfiGroupMembershipState::Pending);

    // Verify Alix's membership state is Allowed (creator)
    let alix_state = alix_group.membership_state().unwrap();
    assert_eq!(alix_state, FfiGroupMembershipState::Allowed);

    // Bo leaves the group
    bo_group.leave_group().await.unwrap();

    // Verify Bo's membership state is PendingRemove after requesting to leave
    let bo_state_after_leave = bo_group.membership_state().unwrap();
    assert_eq!(bo_state_after_leave, FfiGroupMembershipState::PendingRemove);

    // Alix syncs to process the leave request
    alix_group.sync().await.unwrap();

    // Wait for admin worker to process the removal
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bo syncs to get the final removal
    bo_group.sync().await.unwrap();

    // Verify Bo's group is no longer active
    assert!(!bo_group.is_active().unwrap());

    // Verify Alix's membership state remains Allowed
    let alix_state_final = alix_group.membership_state().unwrap();
    assert_eq!(alix_state_final, FfiGroupMembershipState::Allowed);

    // Verify only Alix remains in the group
    alix_group.sync().await.unwrap();
    let members = alix_group.list_members().await.unwrap();
    assert_eq!(members.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_membership_state_after_readd() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates a group and adds Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs and gets the group
    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Verify Bo's initial membership state is Pending
    let bo_state_initial = bo_group.membership_state().unwrap();
    assert_eq!(
        bo_state_initial,
        FfiGroupMembershipState::Pending,
        "Bo should be in Pending state when first invited"
    );

    // Bo leaves the group
    bo_group.leave_group().await.unwrap();

    // Verify Bo's membership state is PendingRemove after requesting to leave
    let bo_state_after_leave = bo_group.membership_state().unwrap();
    assert_eq!(
        bo_state_after_leave,
        FfiGroupMembershipState::PendingRemove,
        "Bo should be in PendingRemove state after leaving"
    );

    // Alix syncs to process the leave request
    alix_group.sync().await.unwrap();

    // Wait for admin worker to process the removal
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bo syncs to get the final removal
    bo_group.sync().await.unwrap();

    // Verify Bo's group is no longer active
    assert!(
        !bo_group.is_active().unwrap(),
        "Bo's group should be inactive after removal"
    );

    // Alix re-adds Bo to the group
    alix_group
        .add_members(vec![bo.account_identifier.clone()])
        .await
        .unwrap();

    // Alix syncs to send the add
    alix_group.sync().await.unwrap();

    // Bo syncs to receive the welcome message for being re-added
    bo.conversations().sync().await.unwrap();

    // Bo should have the group again (same ID)
    let bo_group_after_readd = bo.conversation(alix_group.id()).unwrap();

    // CRITICAL: Verify Bo's membership state is Allowed (not PendingRemove)
    let bo_state_after_readd = bo_group_after_readd.membership_state().unwrap();
    assert_eq!(
        bo_state_after_readd,
        FfiGroupMembershipState::Allowed,
        "Bo should be in Allowed state after being re-added, not PendingRemove"
    );

    // Verify the group is active again
    assert!(
        bo_group_after_readd.is_active().unwrap(),
        "Bo's group should be active after re-add"
    );

    // Verify consent state is Unknown (user needs to accept)
    let bo_consent_after_readd = bo_group_after_readd.consent_state().unwrap();
    assert_eq!(
        bo_consent_after_readd,
        FfiConsentState::Unknown,
        "Bo's consent should be Unknown after re-add, requiring explicit acceptance"
    );

    // Verify the group shows up correctly in UX logic
    // This mimics the Android logic: isActive() && membershipState() != PENDING_REMOVE
    let is_active_and_not_pending_removal = bo_group_after_readd.is_active().unwrap()
        && bo_group_after_readd.membership_state().unwrap()
            != FfiGroupMembershipState::PendingRemove;
    assert!(
        is_active_and_not_pending_removal,
        "Group should be active and not in PendingRemove state for proper UX rendering"
    );

    // Verify both members are back in the group
    alix_group.sync().await.unwrap();
    let members_after_readd = alix_group.list_members().await.unwrap();
    assert_eq!(
        members_after_readd.len(),
        2,
        "Both Alix and Bo should be in the group"
    );
}
