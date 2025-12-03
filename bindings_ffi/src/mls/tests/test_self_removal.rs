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
