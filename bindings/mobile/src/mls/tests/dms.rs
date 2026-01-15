//! Tests for DM-specific functionality including creation, syncing, and threading

use super::*;

#[tokio::test]
async fn test_find_or_create_dm() {
    // Create two test users
    let wallet1 = generate_local_wallet();
    let wallet2 = generate_local_wallet();

    let client1 = new_test_client_with_wallet(wallet1).await;
    let client2 = new_test_client_with_wallet(wallet2).await;

    // Test find_or_create_dm_by_inbox_id
    let inbox_id2 = client2.inbox_id();
    let dm_by_inbox = client1
        .conversations()
        .find_or_create_dm_by_inbox_id(inbox_id2, FfiCreateDMOptions::default())
        .await
        .expect("Should create DM with inbox ID");

    // Verify conversation appears in DM list
    let dms = client1
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(dms.len(), 1, "Should have one DM conversation");
    assert_eq!(
        dms[0].conversation.id(),
        dm_by_inbox.id(),
        "Listed DM should match created DM"
    );

    // Sync both clients
    client1.conversations().sync().await.unwrap();
    client2.conversations().sync().await.unwrap();

    // First client tries to create another DM with the same inbox id
    let dm_by_inbox2 = client1
        .conversations()
        .find_or_create_dm_by_inbox_id(client2.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    // Sync both clients
    client1.conversations().sync().await.unwrap();
    client2.conversations().sync().await.unwrap();

    // Id should be the same as the existing DM and the num of dms should still be 1
    assert_eq!(
        dm_by_inbox2.id(),
        dm_by_inbox.id(),
        "New DM should match existing DM"
    );
    let dms = client1
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(dms.len(), 1, "Should still have one DM conversation");

    // Second client tries to create a DM with the client 1 inbox id
    let dm_by_inbox3 = client2
        .conversations()
        .find_or_create_dm_by_inbox_id(client1.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    // Sync both clients
    client1.conversations().sync().await.unwrap();
    client2.conversations().sync().await.unwrap();

    // Id should be the same as the existing DM and the num of dms should still be 1
    assert_eq!(
        dm_by_inbox3.id(),
        dm_by_inbox.id(),
        "New DM should match existing DM"
    );
    let dms = client2
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(dms.len(), 1, "Should still have one DM conversation");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_dms_sync_but_do_not_list() {
    let alix = Tester::new().await;
    let bola = Tester::new().await;

    let alix_conversations = alix.conversations();
    let bola_conversations = bola.conversations();

    let _alix_dm = alix_conversations
        .find_or_create_dm(
            bola.account_identifier.clone(),
            FfiCreateDMOptions::default(),
        )
        .await
        .unwrap();
    let alix_sync_summary = alix_conversations
        .sync_all_conversations(None)
        .await
        .unwrap();
    bola_conversations.sync().await.unwrap();
    let bola_sync_summary = bola_conversations
        .sync_all_conversations(None)
        .await
        .unwrap();
    assert_eq!(alix_sync_summary.num_eligible, 1);
    assert_eq!(alix_sync_summary.num_synced, 0);
    assert_eq!(bola_sync_summary.num_eligible, 1);
    assert_eq!(bola_sync_summary.num_synced, 0);

    let alix_groups = alix_conversations
        .list_groups(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(alix_groups.len(), 0);

    let bola_groups = bola_conversations
        .list_groups(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(bola_groups.len(), 0);

    let alix_dms = alix_conversations
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(alix_dms.len(), 1);

    let bola_dms = bola_conversations
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(bola_dms.len(), 1);

    let alix_conversations = alix_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(alix_conversations.len(), 1);

    let bola_conversations = bola_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(bola_conversations.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_dm_stream_correct_type() {
    let amal = Tester::new().await;
    let bola = Tester::new().await;

    let stream_callback = Arc::new(RustStreamCallback::default());
    amal.conversations()
        .stream_dms(stream_callback.clone())
        .await;
    amal.conversations()
        .find_or_create_dm(
            bola.account_identifier.clone(),
            FfiCreateDMOptions::default(),
        )
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 1);

    let convo_list = stream_callback.conversations.lock();
    assert_eq!(convo_list.len(), 1);
    assert_eq!(convo_list[0].conversation_type(), FfiConversationType::Dm);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_dm_streaming() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;
    let caro = Tester::new().await;

    // Stream all conversations
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo.conversations().stream(stream_callback.clone()).await;

    alix.conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 1);
    alix.conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();

    assert_eq!(stream_callback.message_count(), 2);

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());

    // Stream just groups
    // Sync bo first to avoid any spillover from the last stream
    bo.conversations().sync().await.unwrap();
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo
        .conversations()
        .stream_groups(stream_callback.clone())
        .await;

    alix.conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    stream_callback.wait_for_delivery(Some(2)).await.unwrap();
    assert_eq!(stream_callback.message_count(), 1);

    alix.conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();
    let result = stream_callback.wait_for_delivery(Some(1)).await;
    assert!(result.is_err(), "Stream unexpectedly received a DM");
    assert_eq!(stream_callback.message_count(), 1);

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());

    // Stream just dms
    // Sync bo before opening the stream
    bo.conversations().sync().await.unwrap();
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo.conversations().stream_dms(stream_callback.clone()).await;
    caro.conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(Some(2)).await.unwrap();
    assert_eq!(stream_callback.message_count(), 1);

    alix.conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let result = stream_callback.wait_for_delivery(Some(2)).await;
    assert!(result.is_err(), "Stream unexpectedly received a Group");
    assert_eq!(stream_callback.message_count(), 1);

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_stream_all_dm_messages() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;
    let alix_dm = alix
        .conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Stream all conversations
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo
        .conversations()
        .stream_all_messages(
            stream_callback.clone(),
            Some(vec![FfiConsentState::Allowed, FfiConsentState::Unknown]),
        )
        .await;
    stream.wait_for_ready().await;

    alix_group
        .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 1);

    alix_dm
        .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 2);

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());
    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    // Stream just groups
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo
        .conversations()
        .stream_all_group_messages(stream_callback.clone(), None)
        .await;
    stream.wait_for_ready().await;

    alix_group
        .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 1);

    alix_dm
        .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    let result = stream_callback.wait_for_delivery(Some(2)).await;
    assert!(result.is_err(), "Stream unexpectedly received a DM message");
    assert_eq!(stream_callback.message_count(), 1);

    stream.end_and_wait().await.unwrap();
    assert!(stream.is_closed());

    bo.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    // Stream just dms
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo
        .conversations()
        .stream_all_dm_messages(stream_callback.clone(), None)
        .await;
    stream.wait_for_ready().await;

    alix_dm
        .send("first".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    stream_callback.wait_for_delivery(None).await.unwrap();
    assert_eq!(stream_callback.message_count(), 1);

    alix_group
        .send("second".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    let result = stream_callback.wait_for_delivery(Some(2)).await;
    assert!(
        result.is_err(),
        "Stream unexpectedly received a Group message"
    );
    assert_eq!(stream_callback.message_count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_dm_first_messages() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates DM with Bo
    let alix_dm = alix
        .conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    // Alix creates group with Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs to get both conversations
    bo.conversations().sync().await.unwrap();
    let bo_dm = bo.conversation(alix_dm.id()).unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Alix sends messages in both conversations
    alix_dm
        .send(
            "Hello in DM".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    alix_group
        .send(
            "Hello in group".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Bo syncs the dm and the group
    bo_dm.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    // Get messages for both participants in both conversations
    let alix_dm_messages = alix_dm
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_dm_messages = bo_dm
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let alix_group_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let bo_group_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    // Verify DM messages
    assert_eq!(alix_dm_messages.len(), 2);
    assert_eq!(bo_dm_messages.len(), 2);
    assert_eq!(
        String::from_utf8_lossy(&alix_dm_messages[1].content),
        "Hello in DM"
    );
    assert_eq!(
        String::from_utf8_lossy(&bo_dm_messages[1].content),
        "Hello in DM"
    );

    // Verify group messages
    assert_eq!(alix_group_messages.len(), 2);
    assert_eq!(bo_group_messages.len(), 2);
    assert_eq!(
        String::from_utf8_lossy(&alix_group_messages[1].content),
        "Hello in group"
    );
    assert_eq!(
        String::from_utf8_lossy(&bo_group_messages[1].content),
        "Hello in group"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_get_dm_peer_inbox_id() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_dm = alix
        .conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    let alix_dm_peer_inbox = alix_dm.dm_peer_inbox_id().unwrap();
    assert_eq!(alix_dm_peer_inbox, bo.inbox_id());

    bo.conversations().sync().await.unwrap();
    let bo_dm = bo.conversation(alix_dm.id()).unwrap();

    let bo_dm_peer_inbox = bo_dm.dm_peer_inbox_id().unwrap();
    assert_eq!(bo_dm_peer_inbox, alix.inbox_id());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_dm_permissions_show_expected_values() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_group_admin_only = alix
        .conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    // Verify we can read the expected permissions
    let alix_permission_policy_set = alix_group_admin_only
        .group_permissions()
        .unwrap()
        .policy_set()
        .unwrap();
    let expected_permission_policy_set = FfiPermissionPolicySet {
        add_member_policy: FfiPermissionPolicy::Deny,
        remove_member_policy: FfiPermissionPolicy::Deny,
        add_admin_policy: FfiPermissionPolicy::Deny,
        remove_admin_policy: FfiPermissionPolicy::Deny,
        update_group_name_policy: FfiPermissionPolicy::Allow,
        update_group_description_policy: FfiPermissionPolicy::Allow,
        update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
        update_message_disappearing_policy: FfiPermissionPolicy::Allow,
        update_app_data_policy: FfiPermissionPolicy::Allow,
    };
    assert_eq!(alix_permission_policy_set, expected_permission_policy_set);

    // Create all_members group
    let all_members_options = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::Default),
        ..Default::default()
    };
    let alix_group_all_members = alix
        .conversations()
        .create_group(vec![bo.account_identifier.clone()], all_members_options)
        .await
        .unwrap();

    // Verify we can read the expected permissions
    let alix_permission_policy_set = alix_group_all_members
        .group_permissions()
        .unwrap()
        .policy_set()
        .unwrap();
    let expected_permission_policy_set = FfiPermissionPolicySet {
        add_member_policy: FfiPermissionPolicy::Allow,
        remove_member_policy: FfiPermissionPolicy::Admin,
        add_admin_policy: FfiPermissionPolicy::SuperAdmin,
        remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
        update_group_name_policy: FfiPermissionPolicy::Allow,
        update_group_description_policy: FfiPermissionPolicy::Allow,
        update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
        update_app_data_policy: FfiPermissionPolicy::Allow,
    };
    assert_eq!(alix_permission_policy_set, expected_permission_policy_set);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_set_disappearing_messages_when_creating_dm() {
    let alix = new_test_client().await;
    let alix_provider = alix.inner_client.context.mls_provider();
    let bola = new_test_client().await;
    let disappearing_settings = FfiMessageDisappearingSettings::new(now_ns(), 2_000_000_000);
    // Step 1: Create a group
    let alix_group = alix
        .conversations()
        .find_or_create_dm(
            bola.account_identifier.clone(),
            FfiCreateDMOptions::new(disappearing_settings.clone()),
        )
        .await
        .unwrap();

    // Step 2: Send a message and sync
    alix_group
        .send(
            "Msg 1 from group".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Step 3: Verify initial messages
    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert_eq!(alix_messages.len(), 2);
    let group_from_db = alix_provider
        .key_store()
        .db()
        .find_group(&alix_group.id())
        .unwrap();
    assert_eq!(
        group_from_db
            .clone()
            .unwrap()
            .message_disappear_from_ns
            .unwrap(),
        disappearing_settings.from_ns
    );
    assert!(
        alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(alix_messages.len(), 1);
}

#[tokio::test]
async fn test_can_successfully_thread_dms() {
    // Create two test users
    let wallet_bo = generate_local_wallet();
    let wallet_alix = generate_local_wallet();

    let client_bo = new_test_client_with_wallet(wallet_bo).await;
    let client_alix = new_test_client_with_wallet(wallet_alix).await;

    let bo_provider = client_bo.inner_client.context.mls_provider();
    let bo_conn = bo_provider.key_store().db();
    let alix_provider = client_alix.inner_client.context.mls_provider();
    let alix_conn = alix_provider.key_store().db();

    // Find or create DM conversations
    let convo_bo = client_bo
        .conversations()
        .find_or_create_dm_by_inbox_id(client_alix.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();
    let convo_alix = client_alix
        .conversations()
        .find_or_create_dm_by_inbox_id(client_bo.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    // Send messages
    convo_bo.send_text("Bo hey").await.unwrap();
    convo_alix.send_text("Alix hey").await.unwrap();

    let group_bo = bo_conn.find_group(&convo_bo.id()).unwrap().unwrap();
    let group_alix = alix_conn.find_group(&convo_alix.id()).unwrap().unwrap();
    assert!(group_bo.last_message_ns.unwrap() < group_alix.last_message_ns.unwrap());
    assert_eq!(group_bo.id, convo_bo.id());

    // Check messages
    let bo_messages = convo_bo
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let alix_messages = convo_alix
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert_eq!(bo_messages.len(), 2, "Bo should see 2 messages");
    assert_eq!(alix_messages.len(), 2, "Alix should see 2 messages");

    // Sync conversations
    client_bo
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    client_alix
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let bo_messages = convo_bo
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let alix_messages = convo_alix
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages.len(), 4, "Bo should see 3 messages after sync");
    assert_eq!(
        alix_messages.len(),
        4,
        "Alix should see 3 messages after sync"
    );

    // Ensure conversations remain the same
    let convo_alix_2 = client_alix
        .conversations()
        .find_or_create_dm_by_inbox_id(client_bo.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();
    let convo_bo_2 = client_bo
        .conversations()
        .find_or_create_dm_by_inbox_id(client_alix.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    let topic_bo_same = client_bo.conversation(convo_bo.id()).unwrap();
    let topic_alix_same = client_alix.conversation(convo_alix.id()).unwrap();

    assert_eq!(
        convo_alix_2.id(),
        convo_bo_2.id(),
        "Conversations should match"
    );
    assert_eq!(
        convo_alix.id(),
        convo_bo_2.id(),
        "Conversations should match"
    );
    assert_ne!(
        convo_alix.id(),
        convo_bo.id(),
        "Conversations id should not match dms should be matched on peerInboxId"
    );
    assert_eq!(convo_alix.id(), topic_bo_same.id(), "Topics should match");
    assert_eq!(convo_alix.id(), topic_alix_same.id(), "Topics should match");
    let alix_dms = client_alix
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    let bo_dms = client_bo
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(
        convo_alix.id(),
        bo_dms[0].conversation.id(),
        "Dms should match"
    );
    assert_eq!(
        convo_alix.id(),
        alix_dms[0].conversation.id(),
        "Dms should match"
    );

    // Send additional messages
    let text_message_bo2 = TextCodec::encode("Bo hey2".to_string()).unwrap();
    convo_alix_2
        .send(
            encoded_content_to_bytes(text_message_bo2),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    let text_message_alix2 = TextCodec::encode("Alix hey2".to_string()).unwrap();
    convo_bo_2
        .send(
            encoded_content_to_bytes(text_message_alix2),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    convo_bo_2.sync().await.unwrap();
    convo_alix_2.sync().await.unwrap();

    // Validate final message count
    let final_bo_messages = convo_alix_2
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let final_alix_messages = convo_bo_2
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    assert_eq!(final_bo_messages.len(), 6, "Bo should see 5 messages");
    assert_eq!(final_alix_messages.len(), 6, "Alix should see 5 messages");
}

#[tokio::test]
async fn test_can_successfully_thread_dms_with_no_messages() {
    // Create two test users
    let wallet_bo = generate_local_wallet();
    let wallet_alix = generate_local_wallet();

    let client_bo = new_test_client_with_wallet(wallet_bo).await;
    let client_alix = new_test_client_with_wallet(wallet_alix).await;

    // Find or create DM conversations
    let convo_bo = client_bo
        .conversations()
        .find_or_create_dm_by_inbox_id(client_alix.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();
    let convo_alix = client_alix
        .conversations()
        .find_or_create_dm_by_inbox_id(client_bo.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    client_bo
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    client_alix
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let group_bo = client_bo.conversation(convo_bo.id()).unwrap();
    let group_alix = client_alix.conversation(convo_alix.id()).unwrap();
    assert_eq!(group_bo.id(), group_alix.id(), "Conversations should match");
}

#[tokio::test]
async fn test_can_quickly_fetch_dm_peer_inbox_id() {
    let wallet_a = generate_local_wallet();
    let wallet_b = generate_local_wallet();

    let client_a = new_test_client_with_wallet(wallet_a).await;
    let client_b = new_test_client_with_wallet(wallet_b).await;

    // Initialize streaming at the beginning, before creating the DM
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = client_a
        .conversations()
        .stream(stream_callback.clone())
        .await;

    // Wait for the streaming to initialize
    stream.wait_for_ready().await;

    // Test find_or_create_dm returns correct dm_peer_inbox_id
    let dm = client_a
        .conversations()
        .find_or_create_dm_by_inbox_id(client_b.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    assert_eq!(dm.dm_peer_inbox_id().unwrap(), client_b.inbox_id());

    // Test conversations.list returns correct dm_peer_inbox_id
    let client_a_conversation_list = client_a
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(client_a_conversation_list.len(), 1);
    assert_eq!(
        client_a_conversation_list[0]
            .conversation()
            .dm_peer_inbox_id()
            .unwrap(),
        client_b.inbox_id()
    );

    // Wait for streaming to receive the conversation
    // This is similar to how test_conversation_streaming and other streaming tests work
    for _ in 0..10 {
        let conversation_count = stream_callback.conversations.lock().len();

        if conversation_count > 0 {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Get the streamed conversations
    let streamed_conversations = stream_callback.conversations.lock().clone();

    // Verify we received the conversation from the stream
    assert!(
        !streamed_conversations.is_empty(),
        "Should have received the conversation from the stream"
    );

    // Verify the streamed conversation has the correct dm_peer_inbox_id
    let found_matching_peer = streamed_conversations.iter().any(|conversation| {
        if let Some(dm_peer_id) = conversation.dm_peer_inbox_id() {
            dm_peer_id == client_b.inbox_id()
        } else {
            false
        }
    });

    assert!(
        found_matching_peer,
        "Should have received conversation with matching peer inbox ID"
    );

    // Clean up the stream
    stream.end_and_wait().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_create_new_installation_can_see_dm() {
    // Create two wallets
    let wallet1 = PrivateKeySigner::random();
    let wallet2 = PrivateKeySigner::random();

    // Create initial clients
    let client1 = new_test_client_with_wallet(wallet1.clone()).await;
    let client2 = new_test_client_with_wallet(wallet2).await;

    // Create DM from client1 to client2
    let dm_group = client1
        .conversations()
        .find_or_create_dm(
            client2.account_identifier.clone(),
            FfiCreateDMOptions::default(),
        )
        .await
        .unwrap();

    // Sync both clients
    client1.conversations().sync().await.unwrap();
    client2.conversations().sync().await.unwrap();

    // Verify both clients can see the DM
    let client1_groups = client1
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    let client2_groups = client2
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(client1_groups.len(), 1, "Client1 should see 1 conversation");
    assert_eq!(client2_groups.len(), 1, "Client2 should see 1 conversation");

    // Create a second client1 with same wallet
    let client1_second = new_test_client_with_wallet(wallet1).await;

    // Verify client1_second starts with no conversations
    let initial_conversations = client1_second
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(
        initial_conversations.len(),
        0,
        "New client should start with no conversations"
    );

    // Send message from client1 to client2
    dm_group
        .send(
            "Hello from client1".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Sync all clients
    client1.conversations().sync().await.unwrap();
    // client2.conversations().sync().await.unwrap();

    tracing::info!(
        "ABOUT TO SYNC CLIENT 1 SECOND: {}",
        client1_second.inbox_id().to_string()
    );
    client1_second.conversations().sync().await.unwrap();

    // Verify second client1 can see the DM
    let client1_second_groups = client1_second
        .conversations()
        .list_dms(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(
        client1_second_groups.len(),
        1,
        "Second client1 should see 1 conversation"
    );
    assert_eq!(
        client1_second_groups[0].conversation.id(),
        dm_group.id(),
        "Second client1's conversation should match original DM"
    );
}

#[tokio::test]
async fn test_can_find_duplicate_dms_for_group() {
    let wallet_a = generate_local_wallet();
    let wallet_b = generate_local_wallet();

    let client_a = new_test_client_with_wallet(wallet_a).await;
    let client_b = new_test_client_with_wallet(wallet_b).await;

    // Create two DMs (same logical participants, will generate duplicate dm_id)
    let dm1 = client_a
        .conversations()
        .find_or_create_dm_by_inbox_id(client_b.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    let _dm2 = client_b
        .conversations()
        .find_or_create_dm_by_inbox_id(client_a.inbox_id(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    client_a
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    client_b
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let group_a = client_a.conversation(dm1.id()).unwrap();
    let duplicates = group_a.find_duplicate_dms().await.unwrap();

    assert_eq!(duplicates.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_set_and_get_dm_consent() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_dm = alix
        .conversations()
        .find_or_create_dm(bo.account_identifier.clone(), FfiCreateDMOptions::default())
        .await
        .unwrap();

    let alix_initial_consent = alix_dm.consent_state().unwrap();
    assert_eq!(alix_initial_consent, FfiConsentState::Allowed);

    bo.conversations().sync().await.unwrap();
    let bo_dm = bo.conversation(alix_dm.id()).unwrap();

    let bo_initial_consent = bo_dm.consent_state().unwrap();
    assert_eq!(bo_initial_consent, FfiConsentState::Unknown);

    alix_dm
        .update_consent_state(FfiConsentState::Denied)
        .unwrap();
    let alix_updated_consent = alix_dm.consent_state().unwrap();
    assert_eq!(alix_updated_consent, FfiConsentState::Denied);
    bo.set_consent_states(vec![FfiConsent {
        state: FfiConsentState::Allowed,
        entity_type: FfiConsentEntityType::ConversationId,
        entity: hex::encode(bo_dm.id()),
    }])
    .await
    .unwrap();
    let bo_updated_consent = bo_dm.consent_state().unwrap();
    assert_eq!(bo_updated_consent, FfiConsentState::Allowed);
}
