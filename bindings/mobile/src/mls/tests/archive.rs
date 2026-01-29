//! Tests for archive export and import functionality

use xmtp_common::NS_IN_MIN;

use crate::device_sync::{FfiArchiveOptions, FfiBackupElementSelection};

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_archive_excludes_disappearing_messages() {
    let alix_wallet = PrivateKeySigner::random();
    let alix = new_test_client_with_wallet(alix_wallet.clone()).await;
    let bo = new_test_client().await;

    // Step 1: Create a group and send one message
    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    alix_group
        .send(
            "Message 1 - before disappearing".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    alix_group.sync().await.unwrap();

    // Step 2: Set disappearing message settings to expire after 5 minutes
    let five_minutes_ns = 5 * NS_IN_MIN;
    let disappearing_settings = FfiMessageDisappearingSettings {
        from_ns: now_ns(),
        in_ns: five_minutes_ns,
    };
    alix_group
        .update_conversation_message_disappearing_settings(disappearing_settings)
        .await
        .unwrap();

    alix_group.sync().await.unwrap();

    // Step 3: Send 2 more messages after disappearing settings are applied
    alix_group
        .send(
            "Message 2 - after disappearing".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    alix_group
        .send(
            "Message 3 - after disappearing".as_bytes().to_vec(),
            FfiSendMessageOpts::default(),
        )
        .await
        .unwrap();

    alix_group.sync().await.unwrap();

    // Verify alix has all 3 application messages (plus membership change message)
    let alix_messages = alix_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();
    let alix_app_messages: Vec<_> = alix_messages
        .iter()
        .filter(|m| m.kind == FfiConversationMessageKind::Application)
        .collect();
    assert_eq!(
        alix_app_messages.len(),
        3,
        "Alix should have 3 application messages"
    );

    // Step 4: Create an archive with exclude_disappearing_messages set to true
    let archive_path = tmp_path();
    let archive_key = vec![0u8; 32];

    alix.create_archive(
        archive_path.clone(),
        FfiArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![FfiBackupElementSelection::Messages],
            exclude_disappearing_messages: true,
        },
        archive_key.clone(),
    )
    .await
    .unwrap();

    // Step 5: Create a second client and import the archive
    let alix2 = new_test_client_with_wallet(alix_wallet).await;

    alix2
        .import_archive(archive_path, archive_key)
        .await
        .unwrap();

    // Step 6: Find the group and check messages
    let alix2_group = alix2.conversation(alix_group.id()).unwrap();
    let alix2_messages = alix2_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    let alix2_app_messages: Vec<_> = alix2_messages
        .iter()
        .filter(|m| m.kind == FfiConversationMessageKind::Application)
        .collect();

    // Only the first message should be present (before disappearing settings were applied)
    assert_eq!(
        alix2_app_messages.len(),
        1,
        "Alix2 should only have 1 application message (the one sent before disappearing settings)"
    );

    // Verify it's the correct message
    assert_eq!(
        alix2_app_messages[0].content,
        "Message 1 - before disappearing".as_bytes().to_vec(),
        "The message should be the one sent before disappearing settings"
    );

    // Step 7: Create a second archive with exclude_disappearing_messages set to false
    let archive_path2 = tmp_path();
    let archive_key2 = vec![1u8; 32];

    alix.create_archive(
        archive_path2.clone(),
        FfiArchiveOptions {
            start_ns: None,
            end_ns: None,
            elements: vec![FfiBackupElementSelection::Messages],
            exclude_disappearing_messages: false,
        },
        archive_key2.clone(),
    )
    .await
    .unwrap();

    // Step 8: Import the second archive into alix2
    alix2
        .import_archive(archive_path2, archive_key2)
        .await
        .unwrap();

    // Step 9: Query messages again and verify all messages are present
    let alix2_messages_after = alix2_group
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap();

    let alix2_app_messages_after: Vec<_> = alix2_messages_after
        .iter()
        .filter(|m| m.kind == FfiConversationMessageKind::Application)
        .collect();

    // All 3 messages should now be present
    assert_eq!(
        alix2_app_messages_after.len(),
        3,
        "Alix2 should have all 3 application messages after importing archive without exclusion"
    );
}
