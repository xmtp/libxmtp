//! Tests for multi-device operations, installations, syncing, and fork recovery

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_create_new_installation_without_breaking_group() {
    let wallet1 = PrivateKeySigner::random();
    let wallet2 = PrivateKeySigner::random();

    // Create clients
    let client1 = new_test_client_with_wallet(wallet1).await;
    let client2 = new_test_client_with_wallet(wallet2.clone()).await;
    // Create a new group with client1 including wallet2

    let group = client1
        .conversations()
        .create_group_by_identity(
            vec![client2.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Sync groups
    client1.conversations().sync().await.unwrap();
    client2.conversations().sync().await.unwrap();

    // Find groups for both clients
    let client1_group = client1.conversation(group.id()).unwrap();
    let client2_group = client2.conversation(group.id()).unwrap();

    // Sync both groups
    client1_group.sync().await.unwrap();
    client2_group.sync().await.unwrap();

    // Assert both clients see 2 members
    let client1_members = client1_group.list_members().await.unwrap();
    assert_eq!(client1_members.len(), 2);

    let client2_members = client2_group.list_members().await.unwrap();
    assert_eq!(client2_members.len(), 2);

    // Drop and delete local database for client2
    client2.release_db_connection().unwrap();

    // Recreate client2 (new installation)
    let client2 = new_test_client_with_wallet(wallet2).await;

    client1_group.update_installations().await.unwrap();

    // Send a message that will break the group
    client1_group
        .send(
            "This message will break the group".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Assert client1 still sees 2 members
    let client1_members = client1_group.list_members().await.unwrap();
    assert_eq!(client1_members.len(), 2);

    client2.conversations().sync().await.unwrap();
    let client2_group = client2.conversation(group.id()).unwrap();
    let client2_members = client2_group.list_members().await.unwrap();
    assert_eq!(client2_members.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_create_new_installations_does_not_fork_group() {
    let bo_wallet = PrivateKeySigner::random();

    // Create clients
    let alix = new_test_client().await;
    let bo = new_test_client_with_wallet(bo_wallet.clone()).await;
    let caro = new_test_client().await;

    // Alix begins a stream for all messages
    let message_callbacks = Arc::new(RustStreamCallback::from_client(&alix));
    let stream_messages = alix
        .conversations()
        .stream_all_messages(message_callbacks.clone(), None)
        .await;
    stream_messages.wait_for_ready().await;

    // Alix creates a group with Bo and Caro
    let group = alix
        .conversations()
        .create_group_by_identity(
            vec![
                bo.account_identifier.clone(),
                caro.account_identifier.clone(),
            ],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Alix and Caro Sync groups
    alix.conversations().sync().await.unwrap();
    bo.conversations().sync().await.unwrap();
    caro.conversations().sync().await.unwrap();

    // Alix and Caro find the group
    let alix_group = alix.conversation(group.id()).unwrap();
    let bo_group = bo.conversation(group.id()).unwrap();
    let caro_group = caro.conversation(group.id()).unwrap();

    alix_group.update_installations().await.unwrap();
    log::info!("Alix sending first message");
    // Alix sends a message in the group
    alix_group
        .send(
            "First message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    log::info!("Caro sending second message");
    caro_group.update_installations().await.unwrap();
    // Caro sends a message in the group
    caro_group
        .send(
            "Second message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Wait for 2 seconds to make sure message does not get streamed to Bo's new installation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bo logs back in with a new installation
    let bo2 = new_test_client_with_wallet(bo_wallet).await;

    // Bo begins a stream for all messages
    let bo2_message_callbacks = Arc::new(RustStreamCallback::from_client(&bo2));
    let bo2_stream_messages = bo2
        .conversations()
        .stream_all_messages(bo2_message_callbacks.clone(), None)
        .await;
    bo2_stream_messages.wait_for_ready().await;

    alix_group.update_installations().await.unwrap();

    log::info!("Alix sending third message after Bo's second installation added");
    // Alix sends a message to the group
    alix_group
        .send(
            "Third message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // New installation of bo finds the group
    bo2.conversations().sync().await.unwrap();
    let bo2_group = bo2.conversation(group.id()).unwrap();

    log::info!("Bo sending fourth message");
    // Bo sends a message to the group
    bo2_group.update_installations().await.unwrap();
    bo2_group
        .send(
            "Fourth message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    log::info!("Caro sending fifth message");
    // Caro sends a message in the group
    caro_group.update_installations().await.unwrap();
    // Temporary workaround for OpenMLS issue - make sure Caro's epoch is up-to-date
    // https://github.com/xmtp/libxmtp/issues/1116
    caro_group.sync().await.unwrap();
    caro_group
        .send(
            "Fifth message".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    log::info!("Syncing alix");
    alix_group.sync().await.unwrap();
    log::info!("Syncing bo 1");
    bo_group.sync().await.unwrap();
    log::info!("Syncing bo 2");
    bo2_group.sync().await.unwrap();
    log::info!("Syncing caro");
    caro_group.sync().await.unwrap();

    // Get the message count for all the clients
    let caro_messages = caro_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo2_messages = bo2_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert_eq!(caro_messages.len(), 6);
    assert_eq!(alix_messages.len(), 6);
    assert_eq!(bo_messages.len(), 6);
    // Bo 2 only sees three messages since it joined after the first 2 were sent + plus the groupUpdatedCodec
    assert_eq!(bo2_messages.len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_sync_all_groups() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    for _i in 0..30 {
        alix.conversations()
            .create_group_by_identity(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
    }

    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    let alix_groups = alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();

    if cfg!(feature = "d14n") {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    let alix_group1 = alix_groups[0].clone();
    let alix_group5 = alix_groups[5].clone();
    let bo_group1 = bo.conversation(alix_group1.conversation.id()).unwrap();
    let bo_group5 = bo.conversation(alix_group5.conversation.id()).unwrap();

    alix_group1
        .conversation
        .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    alix_group5
        .conversation
        .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let bo_messages1 = bo_group1
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_messages5 = bo_group5
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages1.len(), 1);
    assert_eq!(bo_messages5.len(), 1);

    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let bo_messages1 = bo_group1
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_messages5 = bo_group5
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages1.len(), 2);
    assert_eq!(bo_messages5.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_sync_all_groups_active_only() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Create 30 groups with alix and bo and sync them
    for _i in 0..30 {
        alix.conversations()
            .create_group_by_identity(
                vec![bo.account_identifier.clone()],
                FfiCreateGroupOptions::default(),
            )
            .await
            .unwrap();
    }
    bo.conversations().sync().await.unwrap();
    let sync_summary_1 = bo
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    assert_eq!(sync_summary_1.num_eligible, 30);

    // Remove bo from all groups and sync
    for group in alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap()
    {
        group
            .conversation
            .remove_members_by_identity(vec![bo.account_identifier.clone()])
            .await
            .unwrap();
    }

    // First sync after removal needs to process all groups and set them to inactive
    let sync_summary_2 = bo
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    assert_eq!(sync_summary_2.num_synced, 30);

    // Send a message to each group to make sure there is something to sync
    for group in alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap()
    {
        group
            .conversation
            .send(vec![4, 5, 6], FfiSendMessageOpts::default())
            .await
            .unwrap();
    }

    // Second sync after removal will not process inactive groups
    let sync_summary_3 = bo
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    assert_eq!(sync_summary_3.num_synced, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_send_message_when_out_of_sync() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;
    let caro = new_test_client().await;
    let davon = new_test_client().await;
    let eri = new_test_client().await;
    let frankie = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    bo_group
        .send("bo1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    // Temporary workaround for OpenMLS issue - make sure Alix's epoch is up-to-date
    // https://github.com/xmtp/libxmtp/issues/1116
    alix_group.sync().await.unwrap();
    alix_group
        .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Move the group forward by 3 epochs (as Alix's max_past_epochs is
    // configured to 3) without Bo syncing
    alix_group
        .add_members_by_identity(vec![
            caro.account_identifier.clone(),
            davon.account_identifier.clone(),
        ])
        .await
        .unwrap();
    alix_group
        .remove_members_by_identity(vec![
            caro.account_identifier.clone(),
            davon.account_identifier.clone(),
        ])
        .await
        .unwrap();
    alix_group
        .add_members_by_identity(vec![
            eri.account_identifier.clone(),
            frankie.account_identifier.clone(),
        ])
        .await
        .unwrap();

    // Bo sends messages to Alix while 3 epochs behind
    bo_group
        .send("bo3".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    alix_group
        .send("alix3".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    bo_group
        .send("bo4".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    bo_group
        .send("bo5".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    alix_group.sync().await.unwrap();
    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    bo_group.sync().await.unwrap();
    let bo_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages.len(), 10);
    assert_eq!(alix_messages.len(), 10);

    assert_eq!(
        bo_messages[bo_messages.len() - 1].id,
        alix_messages[alix_messages.len() - 1].id
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_send_messages_when_epochs_behind() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    bo.conversations().sync().await.unwrap();

    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Move forward 4 epochs
    alix_group
        .update_group_description("change 1".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_description("change 2".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_description("change 3".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_description("change 4".to_string())
        .await
        .unwrap();

    bo_group
        .send(
            "bo message 1".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    let alix_can_see_bo_message = alix_messages
        .iter()
        .any(|message| message.content == "bo message 1".as_bytes());
    assert!(
        alix_can_see_bo_message,
        "\"bo message 1\" not found in alix's messages"
    );

    let bo_can_see_bo_message = bo_messages
        .iter()
        .any(|message| message.content == "bo message 1".as_bytes());
    assert!(
        bo_can_see_bo_message,
        "\"bo message 1\" not found in bo's messages"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_add_members_when_out_of_sync() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;
    let caro = new_test_client().await;
    let davon = new_test_client().await;
    let eri = new_test_client().await;
    let frankie = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    bo_group
        .send("bo1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    alix_group
        .send("alix1".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Move the group forward by 3 epochs (as Alix's max_past_epochs is
    // configured to 3) without Bo syncing
    alix_group
        .add_members_by_identity(vec![
            caro.account_identifier.clone(),
            davon.account_identifier.clone(),
        ])
        .await
        .unwrap();
    alix_group
        .remove_members_by_identity(vec![
            caro.account_identifier.clone(),
            davon.account_identifier.clone(),
        ])
        .await
        .unwrap();
    alix_group
        .add_members_by_identity(vec![eri.account_identifier.clone()])
        .await
        .unwrap();

    // Bo adds a member while 3 epochs behind
    bo_group
        .add_members_by_identity(vec![frankie.account_identifier.clone()])
        .await
        .unwrap();

    bo_group.sync().await.unwrap();
    let bo_members = bo_group.list_members().await.unwrap();
    assert_eq!(bo_members.len(), 4);

    alix_group.sync().await.unwrap();
    let alix_members = alix_group.list_members().await.unwrap();
    assert_eq!(alix_members.len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_revoke_installation_for_two_users_and_group_modification() {
    // Step 1: Create two installations
    let alix_wallet = PrivateKeySigner::random();
    let bola_wallet = PrivateKeySigner::random();
    let alix_client_1 = new_test_client_with_wallet(alix_wallet.clone()).await;
    let alix_client_2 = new_test_client_with_wallet(alix_wallet.clone()).await;
    let bola_client_1 = new_test_client_with_wallet(bola_wallet.clone()).await;

    // Ensure both clients are properly initialized
    let alix_client_1_state = alix_client_1.inbox_state(true).await.unwrap();
    let alix_client_2_state = alix_client_2.inbox_state(true).await.unwrap();
    let bola_client_1_state = bola_client_1.inbox_state(true).await.unwrap();
    assert_eq!(alix_client_1_state.installations.len(), 2);
    assert_eq!(alix_client_2_state.installations.len(), 2);
    assert_eq!(bola_client_1_state.installations.len(), 1);

    // Step 2: Create a group
    let group = alix_client_1
        .conversations()
        .create_group_by_identity(
            vec![bola_client_1.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // No ordering guarantee on members list
    let group_members = group.list_members().await.unwrap();
    assert_eq!(group_members.len(), 2);

    // identify which member is alix
    let alix_member = group_members
        .iter()
        .find(|m| m.inbox_id == alix_client_1.inbox_id())
        .unwrap();
    assert_eq!(alix_member.installation_ids.len(), 2);

    // Step 3: Revoke one installation
    let revoke_request = alix_client_1
        .revoke_installations(vec![alix_client_2.installation_id()])
        .await
        .unwrap();
    revoke_request.add_wallet_signature(&alix_wallet).await;
    alix_client_1
        .apply_signature_request(revoke_request)
        .await
        .unwrap();

    // Validate revocation
    let client_1_state_after_revoke = alix_client_1.inbox_state(true).await.unwrap();
    let client_2_state_after_revoke = alix_client_2.inbox_state(true).await.unwrap();
    assert_eq!(client_1_state_after_revoke.installations.len(), 1);
    assert_eq!(client_2_state_after_revoke.installations.len(), 1);

    alix_client_1
        .conversation(group.id())
        .unwrap()
        .sync()
        .await
        .unwrap();
    alix_client_2.conversations().sync().await.unwrap();
    alix_client_2
        .conversation(group.id())
        .unwrap()
        .sync()
        .await
        .unwrap();
    bola_client_1.conversations().sync().await.unwrap();
    bola_client_1
        .conversation(group.id())
        .unwrap()
        .sync()
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Re-fetch group members
    let group_members = group.list_members().await.unwrap();
    let alix_member = group_members
        .iter()
        .find(|m| m.inbox_id == alix_client_1.inbox_id())
        .unwrap();
    assert_eq!(alix_member.installation_ids.len(), 1);

    let alix_2_groups = alix_client_2
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();

    assert!(
        alix_2_groups
            .first()
            .unwrap()
            .conversation
            .update_group_name("test 2".to_string())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_revoke_installation_for_one_user_and_group_modification() {
    // Step 1: Create two installations
    let alix_wallet = PrivateKeySigner::random();
    let alix_client_1 = new_test_client_with_wallet(alix_wallet.clone()).await;
    let alix_client_2 = new_test_client_with_wallet(alix_wallet.clone()).await;

    // Ensure both clients are properly initialized
    let alix_client_1_state = alix_client_1.inbox_state(true).await.unwrap();
    let alix_client_2_state = alix_client_2.inbox_state(true).await.unwrap();
    assert_eq!(alix_client_1_state.installations.len(), 2);
    assert_eq!(alix_client_2_state.installations.len(), 2);

    // Step 2: Create a group
    let group = alix_client_1
        .conversations()
        .create_group_by_identity(vec![], FfiCreateGroupOptions::default())
        .await
        .unwrap();

    // No ordering guarantee on members list
    let group_members = group.list_members().await.unwrap();
    assert_eq!(group_members.len(), 1);

    // identify which member is alix
    let alix_member = group_members
        .iter()
        .find(|m| m.inbox_id == alix_client_1.inbox_id())
        .unwrap();
    assert_eq!(alix_member.installation_ids.len(), 2);

    // Step 3: Revoke one installation
    let revoke_request = alix_client_1
        .revoke_installations(vec![alix_client_2.installation_id()])
        .await
        .unwrap();
    revoke_request.add_wallet_signature(&alix_wallet).await;
    alix_client_1
        .apply_signature_request(revoke_request)
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Validate revocation
    let client_1_state_after_revoke = alix_client_1.inbox_state(true).await.unwrap();
    let _client_2_state_after_revoke = alix_client_2.inbox_state(true).await.unwrap();

    assert_eq!(client_1_state_after_revoke.installations.len(), 1);

    let alix_conversation_1 = alix_client_1.conversation(group.id()).unwrap();
    alix_conversation_1.sync().await.unwrap();

    alix_client_2.conversations().sync().await.unwrap();
    let alix_conversation_2 = alix_client_2.conversation(group.id()).unwrap();
    alix_conversation_2.sync().await.unwrap();

    // Re-fetch group members
    let group_members = group.list_members().await.unwrap();
    let alix_member = group_members
        .iter()
        .find(|m| m.inbox_id == alix_client_1.inbox_id())
        .unwrap();
    assert_eq!(alix_member.installation_ids.len(), 1);

    let alix_2_groups = alix_client_2
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();

    assert!(
        alix_2_groups
            .first()
            .unwrap()
            .conversation
            .update_group_name("test 2".to_string())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_new_installation_group_message_visibility() {
    let alix = Tester::builder().sync_worker().build().await;
    let bo = Tester::new().await;

    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], Default::default())
        .await
        .unwrap();

    let text_message_alix = TextCodec::encode("hello from alix".to_string()).unwrap();
    group
        .send(
            encoded_content_to_bytes(text_message_alix.clone()),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    let alix2 = alix.builder.build().await;

    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(group.id()).unwrap();
    let text_message_bo = TextCodec::encode("hello from bo".to_string()).unwrap();
    bo_group
        .send(
            encoded_content_to_bytes(text_message_bo.clone()),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    alix.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    alix2
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    alix.inner_client
        .test_has_same_sync_group_as(&alix2.inner_client)
        .await
        .unwrap();

    let group2 = alix2.conversation(group.id()).unwrap();
    let messages = group2
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert_eq!(
        messages.len(),
        2,
        "Expected two message to be visible to new installation"
    );

    let text_message_alix2 = TextCodec::encode("hi from alix2".to_string()).unwrap();
    let msg_from_alix2 = group2
        .send(
            encoded_content_to_bytes(text_message_alix2.clone()),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    let bob_group = bo.conversation(group.id()).unwrap();
    let bob_msgs = bob_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert!(
        bob_msgs.iter().any(|m| m.id == msg_from_alix2),
        "Bob should see the message sent by alix2"
    );

    alix.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    let alice_msgs = group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert!(
        alice_msgs.iter().any(|m| m.id == msg_from_alix2),
        "Original Alix should see the message from alix2"
    );
}

#[tokio::test]
async fn test_sync_consent() {
    // Create two test users
    let alix = Tester::builder().sync_server().sync_worker().build().await;
    let bo = Tester::new().await;

    // Create a group conversation
    let alix_group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], FfiCreateGroupOptions::default())
        .await
        .unwrap();
    let initial_consent = alix_group.consent_state().unwrap();
    assert_eq!(initial_consent, FfiConsentState::Allowed);

    let alix2 = alix.builder.build().await;
    let state = alix2.inbox_state(true).await.unwrap();
    assert_eq!(state.installations.len(), 2);

    alix.sync_preferences().await.unwrap();
    alix_group.sync().await.unwrap();
    alix2.conversations().sync().await.unwrap();

    let sg1 = alix
        .inner_client
        .device_sync_client()
        .get_sync_group()
        .await
        .unwrap();
    let sg2 = alix2
        .inner_client
        .device_sync_client()
        .get_sync_group()
        .await
        .unwrap();

    assert_eq!(sg1.group_id, sg2.group_id);

    alix.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    alix2
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    alix2.inner_client.sync_welcomes().await.unwrap();

    // Update consent state
    alix_group
        .update_consent_state(FfiConsentState::Denied)
        .unwrap();
    alix.worker()
        .register_interest(SyncMetric::ConsentSent, 3)
        .wait()
        .await
        .unwrap();

    sg2.sync().await.unwrap();

    alix2
        .worker()
        .register_interest(SyncMetric::ConsentReceived, 1)
        .wait()
        .await
        .unwrap();

    let alix_group2 = alix2.conversation(alix_group.id()).unwrap();
    assert_eq!(
        alix_group2.consent_state().unwrap(),
        FfiConsentState::Denied
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_set_and_get_group_consent() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let alix_initial_consent = alix_group.consent_state().unwrap();
    assert_eq!(alix_initial_consent, FfiConsentState::Allowed);

    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    let bo_initial_consent = bo_group.consent_state().unwrap();
    assert_eq!(bo_initial_consent, FfiConsentState::Unknown);

    alix_group
        .update_consent_state(FfiConsentState::Denied)
        .unwrap();
    let alix_updated_consent = alix_group.consent_state().unwrap();
    assert_eq!(alix_updated_consent, FfiConsentState::Denied);
    bo.set_consent_states(vec![FfiConsent {
        state: FfiConsentState::Allowed,
        entity_type: FfiConsentEntityType::ConversationId,
        entity: hex::encode(bo_group.id()),
    }])
    .await
    .unwrap();
    let bo_updated_consent = bo_group.consent_state().unwrap();
    assert_eq!(bo_updated_consent, FfiConsentState::Allowed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_set_and_get_member_consent() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    alix.set_consent_states(vec![FfiConsent {
        state: FfiConsentState::Allowed,
        entity_type: FfiConsentEntityType::InboxId,
        entity: bo.inbox_id(),
    }])
    .await
    .unwrap();
    let bo_consent = alix
        .get_consent_state(FfiConsentEntityType::InboxId, bo.inbox_id())
        .await
        .unwrap();
    assert_eq!(bo_consent, FfiConsentState::Allowed);

    if let Some(member) = alix_group
        .list_members()
        .await
        .unwrap()
        .iter()
        .find(|&m| m.inbox_id == bo.inbox_id())
    {
        assert_eq!(member.consent_state, FfiConsentState::Allowed);
    } else {
        panic!("Error: No member found with the given inbox_id.");
    }
}
