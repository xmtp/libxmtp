use super::*;

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
async fn only_test_sync_welcomes() {
    let alice = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bob = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let alice_bob_group = alice.create_group(None, None).unwrap();
    alice_bob_group
        .add_members(&[bob.inbox_id()])
        .await
        .unwrap();

    let bob_received_groups = bob.sync_welcomes().await.unwrap();
    assert_eq!(bob_received_groups.len(), 1);
    assert_eq!(
        bob_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );

    let duplicate_received_groups = bob.sync_welcomes().await.unwrap();
    assert_eq!(duplicate_received_groups.len(), 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(flavor = "multi_thread")]
async fn test_leaf_node_lifetime_validation_disabled() {
    use crate::utils::test_mocks_helpers::set_test_mode_limit_key_package_lifetime;

    // Create a client with default KP lifetime
    tester!(alice);

    // Create a client with default KP lifetime
    set_test_mode_limit_key_package_lifetime(false, 0);
    tester!(cat);

    let alice_bob_group = alice.create_group(None, None).unwrap();
    alice_bob_group
        .add_members(&[cat.inbox_id()])
        .await
        .unwrap();

    let cat_received_groups = cat.sync_welcomes().await.unwrap();
    assert_eq!(cat_received_groups.len(), 1);
    assert_eq!(
        cat_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );

    // Create a client with a KP that expires in 5 seconds
    set_test_mode_limit_key_package_lifetime(true, 5);
    tester!(bob);

    // Alice invites Bob with short living KP
    alice_bob_group
        .add_members(&[bob.inbox_id()])
        .await
        .unwrap();

    // Since Bob's KP is still valid, Bob should successfully process the Welcome
    let bob_received_groups = bob.sync_welcomes().await.unwrap();

    // Wait for Bob's KP and their leafnode's lifetime to expire
    xmtp_common::time::sleep(Duration::from_secs(7)).await;

    assert_eq!(bob_received_groups.len(), 1);
    assert_eq!(
        bob_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );

    let bob_duplicate_received_groups = bob.sync_welcomes().await.unwrap();
    let cat_duplicate_received_groups = cat.sync_welcomes().await.unwrap();
    assert_eq!(bob_duplicate_received_groups.len(), 0);
    assert_eq!(cat_duplicate_received_groups.len(), 0);

    set_test_mode_limit_key_package_lifetime(false, 0);
    tester!(dave);
    alice_bob_group
        .add_members(&[dave.inbox_id()])
        .await
        .unwrap();
    // Dave should be okay receiving a welcome where members of the group are expired
    let dave_received_groups = dave.sync_welcomes().await.unwrap();
    assert_eq!(dave_received_groups.len(), 1);
    assert_eq!(
        dave_received_groups.first().unwrap().group_id,
        alice_bob_group.group_id
    );
    let dave_duplicate_received_groups = dave.sync_welcomes().await.unwrap();
    assert_eq!(dave_duplicate_received_groups.len(), 0);

    // Cat receives commits to add expired group members, they should pass validation and be added
    let cat_group = cat_received_groups.first().unwrap();
    cat_group.sync().await.unwrap();
    assert_eq!(cat_group.members().await.unwrap().len(), 4);
}

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread", worker_threads = 10)]
async fn test_sync_all_groups() {
    tester!(alix);
    tester!(bo);

    let alix_bo_group1 = alix.create_group(None, None).unwrap();
    let alix_bo_group2 = alix.create_group(None, None).unwrap();
    alix_bo_group1.add_members(&[bo.inbox_id()]).await.unwrap();
    alix_bo_group2.add_members(&[bo.inbox_id()]).await.unwrap();

    let bob_received_groups = bo.sync_welcomes().await.unwrap();
    assert_eq!(bob_received_groups.len(), 2);

    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group1 = bo.group(&alix_bo_group1.group_id).unwrap();
    let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages1.len(), 1);
    let bo_group2 = bo.group(&alix_bo_group2.group_id).unwrap();
    let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages2.len(), 1);
    alix_bo_group1
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();
    alix_bo_group2
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    let summary = bo.sync_all_groups(bo_groups).await.unwrap();
    assert_eq!(summary.num_synced, 2);

    let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages1.len(), 2);
    let bo_group2 = bo.group(&alix_bo_group2.group_id).unwrap();
    let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages2.len(), 2);
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_sync_all_groups_and_welcomes() {
    tester!(alix);
    tester!(bo, passkey);

    // Create two groups and add Bob
    let alix_bo_group1 = alix.create_group(None, None).unwrap();
    let alix_bo_group2 = alix.create_group(None, None).unwrap();

    alix_bo_group1.add_members(&[bo.inbox_id()]).await.unwrap();
    alix_bo_group2.add_members(&[bo.inbox_id()]).await.unwrap();

    // Initial sync (None): Bob should fetch both groups
    let bob_received_groups = bo.sync_all_welcomes_and_groups(None).await.unwrap();
    assert_eq!(bob_received_groups.num_synced, 2);

    xmtp_common::time::sleep(Duration::from_millis(100)).await;

    // Verify Bo initially has no messages
    let bo_group1 = bo.group(&alix_bo_group1.group_id).unwrap();
    assert_eq!(
        bo_group1
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );
    let bo_group2 = bo.group(&alix_bo_group2.group_id).unwrap();
    assert_eq!(
        bo_group2
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );

    // Alix sends a message to both groups
    alix_bo_group1
        .send_message(vec![1, 2, 3].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();
    alix_bo_group2
        .send_message(vec![4, 5, 6].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync with `Unknown`: Bob should not fetch new messages
    let bob_received_groups_unknown = bo
        .sync_all_welcomes_and_groups(Some([ConsentState::Allowed].to_vec()))
        .await
        .unwrap();
    assert_eq!(bob_received_groups_unknown.num_synced, 0);

    // Verify Bob still has no messages
    assert_eq!(
        bo_group1
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        bo_group2
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1
    );

    // Alix sends another message to both groups
    alix_bo_group1
        .send_message(vec![7, 8, 9].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();
    alix_bo_group2
        .send_message(vec![10, 11, 12].as_slice(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync with `None`: Bob should fetch all messages
    let bo_sync_summary = bo
        .sync_all_welcomes_and_groups(Some([ConsentState::Unknown].to_vec()))
        .await
        .unwrap();
    assert_eq!(bo_sync_summary.num_synced, 2);

    // Verify Bob now has all messages
    let bo_messages1 = bo_group1.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages1.len(), 3);

    let bo_messages2 = bo_group2.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_messages2.len(), 3);
}

#[xmtp_common::test]
async fn test_sync_100_allowed_groups_performance() {
    tester!(alix);
    tester!(bo, passkey);

    let group_count = 100;
    let mut groups = Vec::with_capacity(group_count);

    for _ in 0..group_count {
        let group = alix.create_group(None, None).unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();
        groups.push(group);
    }

    xmtp_common::time::sleep(Duration::from_millis(100)).await;

    let start = xmtp_common::time::Instant::now();
    let _synced_count = bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let elapsed = start.elapsed();

    let test_group = groups.first().unwrap();
    let bo_group = bo.group(&test_group.group_id).unwrap();
    assert_eq!(
        bo_group
            .find_messages(&MsgQueryArgs::default())
            .unwrap()
            .len(),
        1,
        "Expected 1 welcome message synced"
    );

    println!(
        "Synced {} groups in {:?} (avg per group: {:?})",
        group_count,
        elapsed,
        elapsed / group_count as u32
    );

    let start = xmtp_common::time::Instant::now();
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    let elapsed = start.elapsed();

    println!(
        "Synced {} groups in {:?} (avg per group: {:?})",
        group_count,
        elapsed,
        elapsed / group_count as u32
    );
}
