//! Tests for group creation, permissions, metadata, membership, listing, and pagination

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_create_group_with_members() {
    let amal = Tester::new().await;
    let bola = Tester::new().await;

    let group = amal
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let members = group.list_members().await.unwrap();
    assert_eq!(members.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_create_group_with_metadata() {
    let amal = new_test_client().await;
    let bola = new_test_client().await;

    let conversation_message_disappearing_settings = FfiMessageDisappearingSettings::new(10, 100);

    let group = amal
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions {
                permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
                group_name: Some("Group Name".to_string()),
                group_image_url_square: Some("url".to_string()),
                group_description: Some("group description".to_string()),
                custom_permission_policy_set: None,
                message_disappearing_settings: Some(
                    conversation_message_disappearing_settings.clone(),
                ),
                app_data: None,
            },
        )
        .await
        .unwrap();

    let members = group.list_members().await.unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(group.group_name().unwrap(), "Group Name");
    assert_eq!(group.group_image_url_square().unwrap(), "url");
    assert_eq!(group.group_description().unwrap(), "group description");
    assert_eq!(
        group
            .conversation_message_disappearing_settings()
            .unwrap()
            .unwrap()
            .from_ns,
        conversation_message_disappearing_settings.clone().from_ns
    );
    assert_eq!(
        group
            .conversation_message_disappearing_settings()
            .unwrap()
            .unwrap()
            .in_ns,
        conversation_message_disappearing_settings.in_ns
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_removed_members_no_longer_update() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    alix_group.sync().await.unwrap();
    let alix_members = alix_group.list_members().await.unwrap();
    assert_eq!(alix_members.len(), 2);

    bo_group.sync().await.unwrap();
    let bo_members = bo_group.list_members().await.unwrap();
    assert_eq!(bo_members.len(), 2);

    let bo_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages.len(), 1);

    alix_group
        .remove_members(vec![bo.account_identifier.clone()])
        .await
        .unwrap();

    alix_group
        .send("hello".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    bo_group.sync().await.unwrap();
    assert!(!bo_group.is_active().unwrap());

    let bo_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(
        bo_messages.first().unwrap().kind,
        FfiConversationMessageKind::MembershipChange
    );
    assert_eq!(bo_messages.len(), 2);

    let bo_members = bo_group.list_members().await.unwrap();
    assert_eq!(bo_members.len(), 1);

    alix_group.sync().await.unwrap();
    let alix_members = alix_group.list_members().await.unwrap();
    assert_eq!(alix_members.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_group_permissions_show_expected_values() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;
    // Create admin_only group
    let admin_only_options = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
        ..Default::default()
    };
    let alix_group_admin_only = alix
        .conversations()
        .create_group(vec![bo.account_identifier.clone()], admin_only_options)
        .await
        .unwrap();

    // Verify we can read the expected permissions
    let alix_permission_policy_set = alix_group_admin_only
        .group_permissions()
        .unwrap()
        .policy_set()
        .unwrap();
    let expected_permission_policy_set = FfiPermissionPolicySet {
        add_member_policy: FfiPermissionPolicy::Admin,
        remove_member_policy: FfiPermissionPolicy::Admin,
        add_admin_policy: FfiPermissionPolicy::SuperAdmin,
        remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
        update_group_name_policy: FfiPermissionPolicy::Admin,
        update_group_description_policy: FfiPermissionPolicy::Admin,
        update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
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
    };
    assert_eq!(alix_permission_policy_set, expected_permission_policy_set);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_permissions_updates() {
    let alix = new_test_client().await;
    let bola = new_test_client().await;

    let admin_only_options = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
        ..Default::default()
    };
    let alix_group = alix
        .conversations()
        .create_group(vec![bola.account_identifier.clone()], admin_only_options)
        .await
        .unwrap();

    let alix_group_permissions = alix_group
        .group_permissions()
        .unwrap()
        .policy_set()
        .unwrap();
    let expected_permission_policy_set = FfiPermissionPolicySet {
        add_member_policy: FfiPermissionPolicy::Admin,
        remove_member_policy: FfiPermissionPolicy::Admin,
        add_admin_policy: FfiPermissionPolicy::SuperAdmin,
        remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
        update_group_name_policy: FfiPermissionPolicy::Admin,
        update_group_description_policy: FfiPermissionPolicy::Admin,
        update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
    };
    assert_eq!(alix_group_permissions, expected_permission_policy_set);

    // Let's update the group so that the image url can be updated by anyone
    alix_group
        .update_permission_policy(
            FfiPermissionUpdateType::UpdateMetadata,
            FfiPermissionPolicy::Allow,
            Some(FfiMetadataField::ImageUrlSquare),
        )
        .await
        .unwrap();
    alix_group.sync().await.unwrap();
    let alix_group_permissions = alix_group
        .group_permissions()
        .unwrap()
        .policy_set()
        .unwrap();
    let new_expected_permission_policy_set = FfiPermissionPolicySet {
        add_member_policy: FfiPermissionPolicy::Admin,
        remove_member_policy: FfiPermissionPolicy::Admin,
        add_admin_policy: FfiPermissionPolicy::SuperAdmin,
        remove_admin_policy: FfiPermissionPolicy::SuperAdmin,
        update_group_name_policy: FfiPermissionPolicy::Admin,
        update_group_description_policy: FfiPermissionPolicy::Admin,
        update_group_image_url_square_policy: FfiPermissionPolicy::Allow,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
    };
    assert_eq!(alix_group_permissions, new_expected_permission_policy_set);

    // Verify that bo can not update the group name
    let bola_conversations = bola.conversations();
    let _ = bola_conversations.sync().await;
    let bola_groups = bola_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();

    let bola_group = bola_groups.first().unwrap();
    bola_group
        .conversation
        .update_group_name("new_name".to_string())
        .await
        .unwrap_err();

    // Verify that bo CAN update the image url
    bola_group
        .conversation
        .update_group_image_url_square("https://example.com/image.png".to_string())
        .await
        .unwrap();

    // Verify we can read the correct values from the group
    bola_group.conversation.sync().await.unwrap();
    alix_group.sync().await.unwrap();
    assert_eq!(
        bola_group.conversation.group_image_url_square().unwrap(),
        "https://example.com/image.png"
    );
    assert_eq!(bola_group.conversation.group_name().unwrap(), "");
    assert_eq!(
        alix_group.group_image_url_square().unwrap(),
        "https://example.com/image.png"
    );
    assert_eq!(alix_group.group_name().unwrap(), "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_group_creation_custom_permissions() {
    let alix = Tester::new().await;
    let bola = Tester::new().await;

    let custom_permissions = FfiPermissionPolicySet {
        add_admin_policy: FfiPermissionPolicy::Admin,
        remove_admin_policy: FfiPermissionPolicy::Admin,
        update_group_name_policy: FfiPermissionPolicy::Admin,
        update_group_description_policy: FfiPermissionPolicy::Allow,
        update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
        add_member_policy: FfiPermissionPolicy::Allow,
        remove_member_policy: FfiPermissionPolicy::Deny,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
    };

    let create_group_options = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
        group_name: Some("Test Group".to_string()),
        group_image_url_square: Some("https://example.com/image.png".to_string()),
        group_description: Some("A test group".to_string()),
        custom_permission_policy_set: Some(custom_permissions),
        message_disappearing_settings: None,
        app_data: None,
    };

    let alix_group = alix
        .conversations()
        .create_group(vec![bola.account_identifier.clone()], create_group_options)
        .await
        .unwrap();

    // Verify the group was created with the correct permissions
    let group_permissions_policy_set = alix_group
        .group_permissions()
        .unwrap()
        .policy_set()
        .unwrap();
    assert_eq!(
        group_permissions_policy_set.add_admin_policy,
        FfiPermissionPolicy::Admin
    );
    assert_eq!(
        group_permissions_policy_set.remove_admin_policy,
        FfiPermissionPolicy::Admin
    );
    assert_eq!(
        group_permissions_policy_set.update_group_name_policy,
        FfiPermissionPolicy::Admin
    );
    assert_eq!(
        group_permissions_policy_set.update_group_description_policy,
        FfiPermissionPolicy::Allow
    );
    assert_eq!(
        group_permissions_policy_set.update_group_image_url_square_policy,
        FfiPermissionPolicy::Admin
    );

    assert_eq!(
        group_permissions_policy_set.update_message_disappearing_policy,
        FfiPermissionPolicy::Admin
    );
    assert_eq!(
        group_permissions_policy_set.add_member_policy,
        FfiPermissionPolicy::Allow
    );
    assert_eq!(
        group_permissions_policy_set.remove_member_policy,
        FfiPermissionPolicy::Deny
    );

    // Verify that Bola can not update the group name
    let bola_conversations = bola.conversations();
    let _ = bola_conversations.sync().await;
    let bola_groups = bola_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();

    let bola_group = bola_groups.first().unwrap();
    bola_group
        .conversation
        .update_group_name("new_name".to_string())
        .await
        .unwrap_err();
    let result = bola_group
        .conversation
        .update_group_name("New Group Name".to_string())
        .await;
    assert!(result.is_err());

    // Verify that Alix can update the group name
    let result = alix_group
        .update_group_name("New Group Name".to_string())
        .await;
    assert!(result.is_ok());

    // Verify that Bola can update the group description
    let result = bola_group
        .conversation
        .update_group_description("New Description".to_string())
        .await;
    assert!(result.is_ok());

    // Verify that Alix can not remove bola even though they are a super admin
    let result = alix_group
        .remove_members(vec![bola.account_identifier.clone()])
        .await;
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_group_creation_custom_permissions_fails_when_invalid() {
    let alix = Tester::new().await;
    let bola = Tester::new().await;

    // Add / Remove Admin must be Admin or Super Admin or Deny
    let custom_permissions_invalid_1 = FfiPermissionPolicySet {
        add_admin_policy: FfiPermissionPolicy::Allow,
        remove_admin_policy: FfiPermissionPolicy::Admin,
        update_group_name_policy: FfiPermissionPolicy::Admin,
        update_group_description_policy: FfiPermissionPolicy::Allow,
        update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
        add_member_policy: FfiPermissionPolicy::Allow,
        remove_member_policy: FfiPermissionPolicy::Deny,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
    };

    let custom_permissions_valid = FfiPermissionPolicySet {
        add_admin_policy: FfiPermissionPolicy::Admin,
        remove_admin_policy: FfiPermissionPolicy::Admin,
        update_group_name_policy: FfiPermissionPolicy::Admin,
        update_group_description_policy: FfiPermissionPolicy::Allow,
        update_group_image_url_square_policy: FfiPermissionPolicy::Admin,
        add_member_policy: FfiPermissionPolicy::Allow,
        remove_member_policy: FfiPermissionPolicy::Deny,
        update_message_disappearing_policy: FfiPermissionPolicy::Admin,
    };

    let create_group_options_invalid_1 = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
        group_name: Some("Test Group".to_string()),
        group_image_url_square: Some("https://example.com/image.png".to_string()),
        group_description: Some("A test group".to_string()),
        custom_permission_policy_set: Some(custom_permissions_invalid_1),
        message_disappearing_settings: None,
        app_data: None,
    };

    let results_1 = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            create_group_options_invalid_1,
        )
        .await;

    assert!(results_1.is_err());

    let create_group_options_invalid_2 = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::Default),
        group_name: Some("Test Group".to_string()),
        group_image_url_square: Some("https://example.com/image.png".to_string()),
        group_description: Some("A test group".to_string()),
        custom_permission_policy_set: Some(custom_permissions_valid.clone()),
        message_disappearing_settings: None,
        app_data: None,
    };

    let results_2 = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            create_group_options_invalid_2,
        )
        .await;

    assert!(results_2.is_err());

    let create_group_options_invalid_3 = FfiCreateGroupOptions {
        permissions: None,
        group_name: Some("Test Group".to_string()),
        group_image_url_square: Some("https://example.com/image.png".to_string()),
        group_description: Some("A test group".to_string()),
        custom_permission_policy_set: Some(custom_permissions_valid.clone()),
        message_disappearing_settings: None,
        app_data: None,
    };

    let results_3 = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            create_group_options_invalid_3,
        )
        .await;

    assert!(results_3.is_err());

    let create_group_options_valid = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::CustomPolicy),
        group_name: Some("Test Group".to_string()),
        group_image_url_square: Some("https://example.com/image.png".to_string()),
        group_description: Some("A test group".to_string()),
        custom_permission_policy_set: Some(custom_permissions_valid),
        message_disappearing_settings: None,
        app_data: None,
    };

    let results_4 = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            create_group_options_valid,
        )
        .await;

    assert!(results_4.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_update_policies_empty_group() {
    let amal = new_test_client().await;
    let bola = new_test_client().await;

    // Create a group with amal and bola with admin-only permissions
    let admin_only_options = FfiCreateGroupOptions {
        permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
        ..Default::default()
    };
    let amal_group = amal
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            admin_only_options.clone(),
        )
        .await
        .unwrap();

    // Verify we can update the group name without syncing first
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    // Verify the name is updated
    amal_group.sync().await.unwrap();
    assert_eq!(amal_group.group_name().unwrap(), "New Group Name 1");

    // Create a group with just amal
    let amal_solo_group = amal
        .conversations()
        .create_group(vec![], admin_only_options)
        .await
        .unwrap();

    // Verify we can update the group name
    amal_solo_group
        .update_group_name("New Group Name 2".to_string())
        .await
        .unwrap();

    // Verify the name is updated
    amal_solo_group.sync().await.unwrap();
    assert_eq!(amal_solo_group.group_name().unwrap(), "New Group Name 2");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_stream_and_receive_metadata_update() {
    // Create test clients
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // If we comment out this stream, the test passes
    let stream_callback = Arc::new(RustStreamCallback::default());
    let stream = bo
        .conversations()
        .stream_all_messages(stream_callback.clone(), None)
        .await;
    stream.wait_for_ready().await;

    // Create group and perform actions
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Send first message
    let mut buf = Vec::new();
    TextCodec::encode("hello1".to_string())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    alix_group
        .send(buf, FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Update group name
    alix_group
        .update_group_name("hello".to_string())
        .await
        .unwrap();

    // Send second message
    let mut buf = Vec::new();
    TextCodec::encode("hello2".to_string())
        .unwrap()
        .encode(&mut buf)
        .unwrap();
    alix_group
        .send(buf, FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Sync Bo's client
    bo.conversations().sync().await.unwrap();

    // Get Bo's groups and verify count
    let bo_groups = bo
        .conversations()
        .list_groups(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(bo_groups.len(), 1);
    let bo_group = bo_groups[0].conversation.clone();

    // Sync both groups
    bo_group.sync().await.unwrap();
    alix_group.sync().await.unwrap();

    // Get Bo's messages and verify content types
    let bo_messages = bo_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(bo_messages.len(), 4);

    // Verify message content types
    let message_types: Vec<String> = bo_messages
        .iter()
        .map(|msg| {
            let encoded_content = EncodedContent::decode(msg.content.as_slice()).unwrap();
            encoded_content.r#type.unwrap().type_id
        })
        .collect();

    assert_eq!(message_types[0], "group_updated");
    assert_eq!(message_types[1], "text");
    assert_eq!(message_types[2], "group_updated");
    assert_eq!(message_types[3], "text");

    assert_eq!(alix_group.group_name().unwrap(), "hello");
    // this assertion will also fail
    assert_eq!(bo_group.group_name().unwrap(), "hello");

    // Clean up stream
    stream.end_and_wait().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_disappearing_messages_deletion() {
    let alix = new_test_client().await;
    let alix_provider = alix.inner_client.context.mls_provider();
    let bola = new_test_client().await;
    let bola_provider = bola.inner_client.context.mls_provider();

    // Step 1: Create a group
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
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
    let mut alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(alix_messages.len(), 2);

    // Step 4: Set disappearing settings to 5ns after the latest message
    let latest_message_sent_at_ns = alix_messages.last().unwrap().sent_at_ns;
    let disappearing_settings = FfiMessageDisappearingSettings::new(latest_message_sent_at_ns, 5);
    alix_group
        .update_conversation_message_disappearing_settings(disappearing_settings.clone())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Verify the settings were applied
    let group_from_db = alix
        .inner_client
        .context
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
    assert_eq!(
        group_from_db.unwrap().message_disappear_in_ns.unwrap(),
        disappearing_settings.in_ns
    );
    assert!(
        alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );

    bola.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let bola_group_from_db = bola_provider
        .key_store()
        .db()
        .find_group(&alix_group.id())
        .unwrap();
    assert_eq!(
        bola_group_from_db
            .clone()
            .unwrap()
            .message_disappear_from_ns
            .unwrap(),
        disappearing_settings.from_ns
    );
    assert_eq!(
        bola_group_from_db.unwrap().message_disappear_in_ns.unwrap(),
        disappearing_settings.in_ns
    );
    assert!(
        alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );

    // Step 5: Send additional messages
    for msg in &["Msg 2 from group", "Msg 3 from group", "Msg 4 from group"] {
        alix_group
            .send(msg.as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
    }
    alix_group.sync().await.unwrap();

    // Step 6: Verify messages after setting disappearing mode
    // With in_ns = 5 nanoseconds, Msg 2, 3, 4 expire almost instantly
    // So they are filtered out immediately from queries
    alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(
        alix_messages.len(),
        4,
        "Should have 4 messages: initial GroupUpdated + Msg 1 + 2 GroupUpdated (settings). Msg 2-4 already expired."
    );

    // Wait to ensure background cleanup worker runs (even though filtering happens at query time)
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Step 7: Verify count remains the same after background cleanup
    alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(
        alix_messages.len(),
        4,
        "Should still have 4 messages after background cleanup"
    );

    // Step 8: Disable disappearing messages
    alix_group
        .remove_conversation_message_disappearing_settings()
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Verify disappearing settings are disabled
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
        0
    );
    assert!(
        !alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );

    assert_eq!(group_from_db.unwrap().message_disappear_in_ns.unwrap(), 0);

    // Step 9: Send another message
    alix_group
        .send(
            "Msg 5 from group".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Step 10: Verify final message count
    alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    // After disabling disappearing messages and sending Msg 5:
    // - 4 messages from step 7 (initial GroupUpdated + Msg 1 + 2 GroupUpdated from settings)
    // - 2 GroupUpdated (from disabling disappearing settings)
    // - 1 Msg 5 (sent after disabling, no expire_at_ns)
    // Total: 7 messages
    // Note: Msg 2, 3, 4 remain filtered out as they expired immediately when sent
    assert_eq!(
        alix_messages.len(),
        7,
        "Should have 7 messages: 4 from before + 2 GroupUpdated (disable) + Msg 5"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_disappearing_messages_with_0_from_ns_settings() {
    let alix = new_test_client().await;
    let alix_provider = alix.inner_client.context.mls_provider();
    let bola = new_test_client().await;
    let bola_provider = bola.inner_client.context.mls_provider();

    // Step 1: Create a group
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
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
    let mut alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    assert_eq!(alix_messages.len(), 2);

    // Step 4: Set disappearing settings to 5ns after the latest message and from ns 0
    let disappearing_settings = FfiMessageDisappearingSettings::new(0, 5);
    alix_group
        .update_conversation_message_disappearing_settings(disappearing_settings.clone())
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Verify the settings were applied and the settings is not enabled
    let group_from_db = alix
        .inner_client
        .context
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
    assert_eq!(
        group_from_db.unwrap().message_disappear_in_ns.unwrap(),
        disappearing_settings.in_ns
    );
    assert!(
        !alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );

    bola.conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    let bola_group_from_db = bola_provider
        .key_store()
        .db()
        .find_group(&alix_group.id())
        .unwrap();
    assert_eq!(
        bola_group_from_db
            .clone()
            .unwrap()
            .message_disappear_from_ns
            .unwrap(),
        disappearing_settings.from_ns
    );
    assert_eq!(
        bola_group_from_db.unwrap().message_disappear_in_ns.unwrap(),
        disappearing_settings.in_ns
    );
    assert!(
        !alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );

    // Step 5: Send additional messages
    for msg in &["Msg 2 from group", "Msg 3 from group", "Msg 4 from group"] {
        alix_group
            .send(msg.as_bytes().to_vec(), FfiSendMessageOpts::default())
            .await
            .unwrap();
    }
    alix_group.sync().await.unwrap();

    // Step 6: Verify total message count before cleanup
    alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let msg_counts_before_cleanup = alix_messages.len();

    // Wait for cleanup to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Step 8: Disable disappearing messages
    alix_group
        .remove_conversation_message_disappearing_settings()
        .await
        .unwrap();
    alix_group.sync().await.unwrap();

    // Verify disappearing settings are disabled
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
        0
    );
    assert!(
        !alix_group
            .is_conversation_message_disappearing_enabled()
            .unwrap()
    );

    assert_eq!(group_from_db.unwrap().message_disappear_in_ns.unwrap(), 0);

    // Step 9: Send another message
    alix_group
        .send(
            "Msg 5 from group".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Step 10: Verify messages after cleanup
    alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    // messages before cleanup + 1 message added for metadataUpdate + 1 message added for 1 normal message
    assert_eq!(msg_counts_before_cleanup + 2, alix_messages.len());
    // 3 messages got deleted, then two messages got added for metadataUpdate and one normal messaged added later
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_set_disappearing_messages_when_creating_group() {
    let alix = new_test_client().await;
    let alix_provider = alix.inner_client.context.mls_provider();
    let bola = new_test_client().await;
    let disappearing_settings = FfiMessageDisappearingSettings::new(now_ns(), 2_000_000_000);
    // Step 1: Create a group
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions {
                permissions: Some(FfiGroupPermissionsOptions::AdminOnly),
                group_name: Some("Group Name".to_string()),
                group_image_url_square: Some("url".to_string()),
                group_description: Some("group description".to_string()),
                custom_permission_policy_set: None,
                message_disappearing_settings: Some(disappearing_settings.clone()),
                app_data: None,
            },
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

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn rapidfire_duplicate_create() {
    let wallet = generate_local_wallet();
    let mut futs = vec![];
    for _ in 0..10 {
        futs.push(new_test_client_no_panic(wallet.clone(), None));
    }

    let results = join_all(futs).await;

    let mut num_okay = 0;
    for result in results {
        if result.is_ok() {
            num_okay += 1;
        }
    }

    // Only one client should get to sign up
    assert_eq!(num_okay, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_group_who_added_me() {
    // Create Clients
    let amal = new_test_client().await;
    let bola = new_test_client().await;

    // Amal creates a group and adds Bola to the group
    amal.conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
    // and then store that value on the group and insert into the database
    let bola_conversations = bola.conversations();
    let _ = bola_conversations.sync().await;

    // Bola gets the group id. This will be needed to fetch the group from
    // the database.
    let bola_groups = bola_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();

    let bola_group = bola_groups.first().unwrap();

    // Check Bola's group for the added_by_inbox_id of the inviter
    let added_by_inbox_id = bola_group.conversation.added_by_inbox_id().unwrap();

    // // Verify the welcome host_credential is equal to Amal's
    assert_eq!(
        amal.inbox_id(),
        added_by_inbox_id,
        "The Inviter and added_by_address do not match!"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_conversation_debug_info_returns_correct_values() {
    // Step 1: Setup test client Alix and bo
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Step 2: Create a group and add messages
    let alix_conversations = alix.conversations();

    // Create a group
    let group = alix_conversations
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let debug_info = group.inner.debug_info().await.unwrap();
    // Ensure the group is included
    assert_eq!(debug_info.epoch, 1, "Group epoch should be 1");
    assert!(!debug_info.maybe_forked, "Group is not marked as forked");
    assert!(
        debug_info.fork_details.is_empty(),
        "Group has no fork details"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_list_conversations_last_message() {
    // Step 1: Setup test client Alix and bo
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Step 2: Create a group and add messages
    let alix_conversations = alix.conversations();

    // Create a group
    let group = alix_conversations
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Add messages to the group
    let text_message_1 = TextCodec::encode("Text message for Group 1".to_string()).unwrap();
    group
        .send(
            encoded_content_to_bytes(text_message_1),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();
    let text_message_2 = TextCodec::encode("Text message for Group 2".to_string()).unwrap();
    group
        .send(
            encoded_content_to_bytes(text_message_2),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Step 3: Synchronize conversations
    alix_conversations
        .sync_all_conversations(None)
        .await
        .unwrap();

    // Step 4: List conversations and verify
    let conversations = alix_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();

    // Ensure the group is included
    assert_eq!(conversations.len(), 1, "Alix should have exactly 1 group");

    let last_message = conversations[0].last_message.as_ref().unwrap();
    assert_eq!(
        TextCodec::decode(bytes_to_encoded_content(last_message.content.clone())).unwrap(),
        "Text message for Group 2".to_string(),
        "Last message content should be the most recent"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_list_conversations_no_messages() {
    // Step 1: Setup test clients Alix and Bo
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_conversations = alix.conversations();

    // Step 2: Create a group with Bo but do not send messages
    alix_conversations
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Step 3: Synchronize conversations
    alix_conversations
        .sync_all_conversations(None)
        .await
        .unwrap();

    // Step 4: List conversations and verify
    let conversations = alix_conversations
        .list(FfiListConversationsOptions::default())
        .unwrap();

    // Ensure the group is included
    assert_eq!(conversations.len(), 1, "Alix should have exactly 1 group");

    // Verify that the last_message is None
    assert!(
        conversations[0].last_message.is_none(),
        "Last message should be None since no messages were sent"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_conversation_list_filters_readable_messages() {
    // Step 1: Setup test client
    let client = new_test_client().await;
    let conversations_api = client.conversations();

    // Step 2: Create 9 groups
    let mut groups = Vec::with_capacity(9);
    for _ in 0..9 {
        let group = conversations_api
            .create_group(vec![], FfiCreateGroupOptions::default())
            .await
            .unwrap();
        groups.push(group);
    }

    // Step 3: Each group gets a message sent in it by type following the pattern:
    //   group[0] -> TextCodec                    (readable)
    //   group[1] -> ReactionCodec                (readable)
    //   group[2] -> AttachmentCodec              (readable)
    //   group[3] -> RemoteAttachmentCodec        (readable)
    //   group[4] -> ReplyCodec                   (readable)
    //   group[5] -> TransactionReferenceCodec    (readable)
    //   group[6] -> GroupUpdatedCodec            (not readable)
    //   group[7] -> GroupMembershipUpdatedCodec  (not readable)
    //   group[8] -> ReadReceiptCodec             (not readable)

    // group[0] sends TextCodec message
    let text_message = TextCodec::encode("Text message for Group 1".to_string()).unwrap();
    groups[0]
        .send(
            encoded_content_to_bytes(text_message),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[1] sends ReactionCodec message
    let reaction_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: ReactionCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let reaction_encoded_content = EncodedContent {
        r#type: Some(reaction_content_type_id),
        content: "reaction content".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[1]
        .send(
            encoded_content_to_bytes(reaction_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[2] sends AttachmentCodec message
    let attachment_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: AttachmentCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let attachment_encoded_content = EncodedContent {
        r#type: Some(attachment_content_type_id),
        content: "attachment content".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[2]
        .send(
            encoded_content_to_bytes(attachment_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[3] sends RemoteAttachmentCodec message
    let remote_attachment_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: RemoteAttachmentCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let remote_attachment_encoded_content = EncodedContent {
        r#type: Some(remote_attachment_content_type_id),
        content: "remote attachment content".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[3]
        .send(
            encoded_content_to_bytes(remote_attachment_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[4] sends ReplyCodec message
    let reply_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: ReplyCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let reply_encoded_content = EncodedContent {
        r#type: Some(reply_content_type_id),
        content: "reply content".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[4]
        .send(
            encoded_content_to_bytes(reply_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[5] sends TransactionReferenceCodec message
    let transaction_reference_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: TransactionReferenceCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let transaction_reference_encoded_content = EncodedContent {
        r#type: Some(transaction_reference_content_type_id),
        content: "transaction reference".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[5]
        .send(
            encoded_content_to_bytes(transaction_reference_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[6] sends GroupUpdatedCodec message
    let group_updated_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: GroupUpdatedCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let group_updated_encoded_content = EncodedContent {
        r#type: Some(group_updated_content_type_id),
        content: "group updated content".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[6]
        .send(
            encoded_content_to_bytes(group_updated_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[7] sends GroupMembershipUpdatedCodec message
    let group_membership_updated_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: GroupMembershipChangeCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let group_membership_updated_encoded_content = EncodedContent {
        r#type: Some(group_membership_updated_content_type_id),
        content: "group membership updated".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[7]
        .send(
            encoded_content_to_bytes(group_membership_updated_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // group[8] sends ReadReceiptCodec message
    let read_receipt_content_type_id = ContentTypeId {
        authority_id: "".to_string(),
        type_id: ReadReceiptCodec::TYPE_ID.to_string(),
        version_major: 0,
        version_minor: 0,
    };
    let read_receipt_encoded_content = EncodedContent {
        r#type: Some(read_receipt_content_type_id),
        content: "read receipt content".as_bytes().to_vec(),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
    };
    groups[8]
        .send(
            encoded_content_to_bytes(read_receipt_encoded_content),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    // Step 4: Synchronize all conversations
    conversations_api
        .sync_all_conversations(None)
        .await
        .unwrap();

    // Step 5: Fetch the list of conversations
    let conversations = conversations_api
        .list(FfiListConversationsOptions::default())
        .unwrap();

    // Step 6: Verify the order of conversations by last readable message sent (or recently created if no readable message)
    // The order should be: 5, 4, 3, 2, 1, 0, 8, 7, 6
    assert_eq!(
        conversations.len(),
        9,
        "There should be exactly 9 conversations"
    );

    assert_eq!(
        conversations[0].conversation.inner.group_id, groups[5].inner.group_id,
        "Group 6 should be the first conversation"
    );
    assert_eq!(
        conversations[1].conversation.inner.group_id, groups[4].inner.group_id,
        "Group 5 should be the second conversation"
    );
    assert_eq!(
        conversations[2].conversation.inner.group_id, groups[3].inner.group_id,
        "Group 4 should be the third conversation"
    );
    assert_eq!(
        conversations[3].conversation.inner.group_id, groups[2].inner.group_id,
        "Group 3 should be the fourth conversation"
    );
    assert_eq!(
        conversations[4].conversation.inner.group_id, groups[1].inner.group_id,
        "Group 2 should be the fifth conversation"
    );
    assert_eq!(
        conversations[5].conversation.inner.group_id, groups[0].inner.group_id,
        "Group 1 should be the sixth conversation"
    );
    assert_eq!(
        conversations[6].conversation.inner.group_id, groups[8].inner.group_id,
        "Group 9 should be the seventh conversation"
    );
    assert_eq!(
        conversations[7].conversation.inner.group_id, groups[7].inner.group_id,
        "Group 8 should be the eighth conversation"
    );
    assert_eq!(
        conversations[8].conversation.inner.group_id, groups[6].inner.group_id,
        "Group 7 should be the ninth conversation"
    );

    // Step 7: Verify that for conversations 0 through 5, last_message is Some
    // Index of group[0] in conversations -> 5
    for i in 0..=5 {
        assert!(
            conversations[5 - i].last_message.is_some(),
            "Group {} should have a last message",
            i + 1
        );
    }

    // Step 8: Verify that for conversations 6, 7, 8, last_message is None
    #[allow(clippy::needless_range_loop)]
    for i in 6..=8 {
        assert!(
            conversations[i].last_message.is_none(),
            "Group {} should have no last message",
            i + 1
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_can_list_messages_with_content_types() {
    // Create test clients
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // Alix creates group with Bo
    let alix_group = alix
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Bo syncs to get the group
    bo.conversations().sync().await.unwrap();
    let bo_group = bo.conversation(alix_group.id()).unwrap();

    // Alix sends first message
    alix_group
        .send("hey".as_bytes().to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Bo syncs and responds
    bo_group.sync().await.unwrap();
    let bo_message_response = TextCodec::encode("hey alix".to_string()).unwrap();
    let mut buf = Vec::new();
    bo_message_response.encode(&mut buf).unwrap();
    bo_group
        .send(buf, FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Bo sends read receipt
    let read_receipt_content_id = ContentTypeId {
        authority_id: "xmtp.org".to_string(),
        type_id: ReadReceiptCodec::TYPE_ID.to_string(),
        version_major: 1,
        version_minor: 0,
    };
    let read_receipt_encoded_content = EncodedContent {
        r#type: Some(read_receipt_content_id),
        parameters: HashMap::new(),
        fallback: None,
        compression: None,
        content: vec![],
    };

    let mut buf = Vec::new();
    read_receipt_encoded_content.encode(&mut buf).unwrap();
    bo_group
        .send(buf, FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Alix syncs and gets all messages
    alix_group.sync().await.unwrap();
    let latest_message = alix_group
        // ... existing code ...
        .find_messages(FfiListMessagesOptions {
            direction: Some(FfiDirection::Descending),
            limit: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();

    // Verify last message is the read receipt
    assert_eq!(latest_message.len(), 1);
    let latest_message_encoded_content =
        EncodedContent::decode(latest_message.last().unwrap().content.clone().as_slice()).unwrap();
    assert_eq!(
        latest_message_encoded_content.r#type.unwrap().type_id,
        "readReceipt"
    );

    // Get only text messages
    let text_messages = alix_group
        .find_messages(FfiListMessagesOptions {
            content_types: Some(vec![FfiContentType::Text]),
            direction: Some(FfiDirection::Descending),
            limit: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();

    // Verify last message is "hey alix" when filtered
    assert_eq!(text_messages.len(), 1);
    let latest_message_encoded_content =
        EncodedContent::decode(text_messages.last().unwrap().content.clone().as_slice()).unwrap();
    let text_message = TextCodec::decode(latest_message_encoded_content).unwrap();
    assert_eq!(text_message, "hey alix");
}

#[tokio::test]
async fn test_get_last_read_times() {
    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();

    let alix_client = new_test_client_with_wallet(alix_wallet).await;
    let bo_client = new_test_client_with_wallet(bo_wallet).await;

    // Create a DM between Alix and Bo
    let alix_dm = alix_client
        .conversations()
        .find_or_create_dm(
            bo_client.account_identifier.clone(),
            FfiCreateDMOptions {
                message_disappearing_settings: None,
            },
        )
        .await
        .unwrap();

    let bo_dm = bo_client
        .conversations()
        .find_or_create_dm(
            alix_client.account_identifier.clone(),
            FfiCreateDMOptions {
                message_disappearing_settings: None,
            },
        )
        .await
        .unwrap();

    // Bo sends a read receipt
    let read_receipt = FfiReadReceipt {};
    let read_receipt_encoded = encode_read_receipt(read_receipt).unwrap();
    bo_dm
        .send(read_receipt_encoded, FfiSendMessageOpts::default())
        .await
        .unwrap();

    alix_client
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();
    bo_client
        .conversations()
        .sync_all_conversations(None)
        .await
        .unwrap();

    // Test get_last_read_times - should return Bo's read receipt timestamp
    let alix_last_read_times = alix_dm.get_last_read_times().unwrap();
    let bo_last_read_times = bo_dm.get_last_read_times().unwrap();

    // Should have one entry for Bo's inbox ID
    assert_eq!(alix_last_read_times.len(), 1);
    assert_eq!(bo_last_read_times.len(), 1);
    assert_eq!(alix_last_read_times, bo_last_read_times);

    // Get Bo's inbox ID
    let bo_inbox_id = bo_client.inbox_id();

    // Verify that Bo's read receipt timestamp is recorded
    assert!(alix_last_read_times.contains_key(&bo_inbox_id));
    let bo_read_time = alix_last_read_times.get(&bo_inbox_id).unwrap();
    assert!(
        *bo_read_time > 0,
        "Read receipt timestamp should be positive"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_pagination_of_conversations_list() {
    let bo_client = new_test_client().await;
    let caro_client = new_test_client().await;

    // Create 15 groups
    let mut groups = Vec::new();
    for i in 0..15 {
        let group = bo_client
            .conversations()
            .create_group(
                vec![caro_client.account_identifier.clone()],
                FfiCreateGroupOptions {
                    group_name: Some(format!("Test Group {}", i)),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        groups.push(group);
    }

    // Send a message to every 7th group to ensure they're ordered by last message
    // and not by created_at
    for (index, group) in groups.iter().enumerate() {
        if index % 2 == 0 {
            group.send_text("Jumbling the sort").await.unwrap();
        }
    }

    // Track all conversations retrieved through pagination
    let mut all_conversations = std::collections::HashSet::new();
    let mut page_count = 0;

    // Get the first page
    let mut page = bo_client
        .conversations()
        .list_groups(FfiListConversationsOptions {
            limit: Some(5),
            order_by: Some(FfiGroupQueryOrderBy::LastActivity),
            ..Default::default()
        })
        .unwrap();

    while !page.is_empty() {
        page_count += 1;

        // Add new conversation IDs to our set
        for conversation in &page {
            let conversation_arc = conversation.conversation();
            assert!(!all_conversations.contains(&conversation_arc.id()));
            all_conversations.insert(conversation_arc.id());
        }

        // If we got fewer than the limit, we've reached the end
        if page.len() < 5 {
            break;
        }

        // Get the oldest (last) conversation's timestamp for the next page
        let last_conversation = page.last().unwrap().conversation();

        let before = if let Some(last_message) = page.last().unwrap().last_message() {
            last_message.sent_at_ns
        } else {
            last_conversation.created_at_ns()
        };

        // Get the next page
        page = bo_client
            .conversations()
            .list_groups(FfiListConversationsOptions {
                last_activity_before_ns: Some(before),
                order_by: Some(FfiGroupQueryOrderBy::LastActivity),
                limit: Some(5),
                ..Default::default()
            })
            .unwrap();

        // Safety check to prevent infinite loop
        if page_count > 10 {
            panic!("Too many pages, possible infinite loop");
        }
    }

    // Validate results
    assert_eq!(
        all_conversations.len(),
        15,
        "Should have retrieved all 15 groups"
    );

    // Verify all created groups are in the results
    for group in &groups {
        assert!(
            all_conversations.contains(&group.id()),
            "Group {} should be in paginated results",
            hex::encode(group.id())
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_membership_state() {
    let amal = new_test_client().await;
    let bola = new_test_client().await;

    // Create a group with amal as creator
    let group = amal
        .conversations()
        .create_group(
            vec![bola.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    // Amal should have Allowed membership state (creator is immediately Allowed)
    let state = group.membership_state().unwrap();
    assert_eq!(state, FfiGroupMembershipState::Allowed);

    // Sync so bola receives the group
    bola.conversations().sync().await.unwrap();
    let bola_group = bola.conversation(group.id()).unwrap();

    // Bola should have Pending membership state when first receiving the welcome
    // (invited members start as Pending until explicitly accepted)
    let bola_state = bola_group.membership_state().unwrap();
    assert_eq!(bola_state, FfiGroupMembershipState::Pending);
}
