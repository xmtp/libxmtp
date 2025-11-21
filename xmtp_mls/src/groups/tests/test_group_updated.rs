use crate::context::XmtpSharedContext;
use crate::groups::{MlsGroup, UpdateAdminListType};
use crate::tester;
use prost::Message as ProstMessage;
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::mls::message_contents::{EncodedContent, GroupUpdated};

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// Decode a GroupUpdated message from encoded bytes
fn decode_group_updated(encoded_bytes: &[u8]) -> GroupUpdated {
    let encoded_content = EncodedContent::decode(encoded_bytes).expect("Failed to decode content");
    GroupUpdatedCodec::decode(encoded_content).expect("Failed to decode GroupUpdated")
}

/// Get the first group for a client
fn get_first_group<C>(client: &crate::Client<C>) -> MlsGroup<C>
where
    C: XmtpSharedContext,
{
    let groups = client.find_groups(Default::default()).unwrap();
    groups
        .into_iter()
        .next()
        .expect("Should have at least one group")
}

/// Sync all welcomes and groups for a client
async fn sync_client_welcomes<C>(client: &crate::Client<C>)
where
    C: XmtpSharedContext,
{
    client
        .sync_all_welcomes_and_groups(None)
        .await
        .expect("Failed to sync welcomes and groups");
}

/// Get and decode the last message from a group
fn get_last_message<C>(group: &MlsGroup<C>) -> GroupUpdated
where
    C: XmtpSharedContext,
{
    let messages = group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let last_msg = messages.last().expect("Should have at least one message");
    decode_group_updated(&last_msg.decrypted_message_bytes)
}

/// Get and decode the nth message from the end (0 = last, 1 = second to last, etc.)
fn get_nth_message_from_end<C>(group: &MlsGroup<C>, n: usize) -> GroupUpdated
where
    C: XmtpSharedContext,
{
    let messages = group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let msg = messages
        .iter()
        .rev()
        .nth(n)
        .expect("Should have message at index");
    decode_group_updated(&msg.decrypted_message_bytes)
}

/// Get the first message from a group
fn get_first_message<C>(group: &MlsGroup<C>) -> GroupUpdated
where
    C: XmtpSharedContext,
{
    let messages = group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let first_msg = messages.first().expect("Should have at least one message");
    decode_group_updated(&first_msg.decrypted_message_bytes)
}

/// Assert admin changes in a GroupUpdated message
fn assert_admin_changes(
    msg: &GroupUpdated,
    expected_added_count: usize,
    expected_removed_count: usize,
    expected_added_inbox_id: Option<&str>,
    expected_removed_inbox_id: Option<&str>,
) {
    assert_eq!(
        msg.added_admin_inboxes.len(),
        expected_added_count,
        "Should have {} added admin(s)",
        expected_added_count
    );
    assert_eq!(
        msg.removed_admin_inboxes.len(),
        expected_removed_count,
        "Should have {} removed admin(s)",
        expected_removed_count
    );

    if let Some(inbox_id) = expected_added_inbox_id {
        assert_eq!(
            msg.added_admin_inboxes[0].inbox_id, inbox_id,
            "Added admin should be {}",
            inbox_id
        );
    }

    if let Some(inbox_id) = expected_removed_inbox_id {
        assert_eq!(
            msg.removed_admin_inboxes[0].inbox_id, inbox_id,
            "Removed admin should be {}",
            inbox_id
        );
    }
}

/// Assert super admin changes in a GroupUpdated message
fn assert_super_admin_changes(
    msg: &GroupUpdated,
    expected_added_count: usize,
    expected_removed_count: usize,
    expected_added_inbox_id: Option<&str>,
    expected_removed_inbox_id: Option<&str>,
) {
    assert_eq!(
        msg.added_super_admin_inboxes.len(),
        expected_added_count,
        "Should have {} added super admin(s)",
        expected_added_count
    );
    assert_eq!(
        msg.removed_super_admin_inboxes.len(),
        expected_removed_count,
        "Should have {} removed super admin(s)",
        expected_removed_count
    );

    if let Some(inbox_id) = expected_added_inbox_id {
        assert_eq!(
            msg.added_super_admin_inboxes[0].inbox_id, inbox_id,
            "Added super admin should be {}",
            inbox_id
        );
    }

    if let Some(inbox_id) = expected_removed_inbox_id {
        assert_eq!(
            msg.removed_super_admin_inboxes[0].inbox_id, inbox_id,
            "Removed super admin should be {}",
            inbox_id
        );
    }
}

/// Assert that a message has no admin changes
fn assert_no_admin_changes(msg: &GroupUpdated) {
    assert_admin_changes(msg, 0, 0, None, None);
    assert_super_admin_changes(msg, 0, 0, None, None);
}

/// Assert that admin list contains specific inbox IDs and has expected count
fn assert_admin_list_contains<C>(group: &MlsGroup<C>, expected_inbox_ids: &[&str])
where
    C: XmtpSharedContext,
{
    let admin_list = group.admin_list().expect("Failed to get admin list");
    assert_eq!(
        admin_list.len(),
        expected_inbox_ids.len(),
        "Should have {} admin(s)",
        expected_inbox_ids.len()
    );
    for inbox_id in expected_inbox_ids {
        assert!(
            admin_list.contains(&inbox_id.to_string()),
            "{} should be in admin list",
            inbox_id
        );
    }
}

/// Assert that super admin list contains specific inbox IDs and has expected count
fn assert_super_admin_list_contains<C>(group: &MlsGroup<C>, expected_inbox_ids: &[&str])
where
    C: XmtpSharedContext,
{
    let super_admin_list = group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert_eq!(
        super_admin_list.len(),
        expected_inbox_ids.len(),
        "Should have {} super admin(s)",
        expected_inbox_ids.len()
    );
    for inbox_id in expected_inbox_ids {
        assert!(
            super_admin_list.contains(&inbox_id.to_string()),
            "{} should be in super admin list",
            inbox_id
        );
    }
}

/// Assert that admin list does NOT contain a specific inbox ID
fn assert_admin_list_excludes<C>(group: &MlsGroup<C>, excluded_inbox_id: &str)
where
    C: XmtpSharedContext,
{
    let admin_list = group.admin_list().expect("Failed to get admin list");
    assert!(
        !admin_list.contains(&excluded_inbox_id.to_string()),
        "{} should not be in admin list",
        excluded_inbox_id
    );
}

/// Assert that super admin list does NOT contain a specific inbox ID
fn assert_super_admin_list_excludes<C>(group: &MlsGroup<C>, excluded_inbox_id: &str)
where
    C: XmtpSharedContext,
{
    let super_admin_list = group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert!(
        !super_admin_list.contains(&excluded_inbox_id.to_string()),
        "{} should not be in super admin list",
        excluded_inbox_id
    );
}

#[xmtp_common::test]
async fn test_group_updated_admin_changes() {
    // Create 5 test clients
    tester!(alix);
    tester!(bola);
    tester!(caro);
    tester!(devon);
    tester!(erin);

    // Alix creates a group and adds the other 4 members
    let alix_group = alix
        .create_group(None, Default::default())
        .expect("Failed to create group");

    alix_group
        .add_members_by_inbox_id(&[
            bola.inbox_id().to_string(),
            caro.inbox_id().to_string(),
            devon.inbox_id().to_string(),
            erin.inbox_id().to_string(),
        ])
        .await
        .expect("Failed to add members");

    // Sync all members
    sync_client_welcomes(&bola).await;
    sync_client_welcomes(&caro).await;
    sync_client_welcomes(&devon).await;
    sync_client_welcomes(&erin).await;

    let bola_group = get_first_group(&bola);
    let caro_group = get_first_group(&caro);
    let devon_group = get_first_group(&devon);
    let erin_group = get_first_group(&erin);

    // Verify welcome messages have empty admin fields but correct member fields
    let welcome_msg = get_first_message(&bola_group);
    assert_eq!(
        welcome_msg.added_inboxes.len(),
        1,
        "Welcome should have 1 added inbox"
    );
    assert_eq!(
        welcome_msg.added_inboxes[0].inbox_id,
        bola.inbox_id(),
        "Added inbox should be Bola"
    );
    assert_no_admin_changes(&welcome_msg);

    // Test 1: Add Bola as admin
    alix_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .expect("Failed to add Bola as admin");

    bola_group.sync().await.expect("Failed to sync");

    // Verify admin and super admin lists
    assert_admin_list_contains(&bola_group, &[bola.inbox_id()]);
    assert_super_admin_list_contains(&bola_group, &[alix.inbox_id()]);

    // Verify the message
    let last_msg = get_last_message(&bola_group);
    assert_admin_changes(&last_msg, 1, 0, Some(bola.inbox_id()), None);
    assert_super_admin_changes(&last_msg, 0, 0, None, None);

    // Test 2: Add Caro as super admin
    alix_group
        .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
        .await
        .expect("Failed to add Caro as super admin");

    caro_group.sync().await.expect("Failed to sync");

    // Verify admin and super admin lists
    assert_super_admin_list_contains(&caro_group, &[alix.inbox_id(), caro.inbox_id()]);
    assert_admin_list_contains(&caro_group, &[bola.inbox_id()]);

    // Verify the message
    let last_msg = get_last_message(&caro_group);
    assert_super_admin_changes(&last_msg, 1, 0, Some(caro.inbox_id()), None);
    assert_admin_changes(&last_msg, 0, 0, None, None);

    // Test 3: Add Devon as admin and Erin as super admin in sequence
    alix_group
        .update_admin_list(UpdateAdminListType::Add, devon.inbox_id().to_string())
        .await
        .expect("Failed to add Devon as admin");

    alix_group
        .update_admin_list(UpdateAdminListType::AddSuper, erin.inbox_id().to_string())
        .await
        .expect("Failed to add Erin as super admin");

    devon_group.sync().await.expect("Failed to sync");
    erin_group.sync().await.expect("Failed to sync");

    // Verify admin and super admin lists
    assert_admin_list_contains(&devon_group, &[bola.inbox_id(), devon.inbox_id()]);
    assert_super_admin_list_contains(
        &erin_group,
        &[alix.inbox_id(), caro.inbox_id(), erin.inbox_id()],
    );

    // Verify Devon's admin addition (second to last message)
    let devon_msg = get_nth_message_from_end(&devon_group, 1);
    assert_admin_changes(&devon_msg, 1, 0, Some(devon.inbox_id()), None);

    // Verify Erin's super admin addition (last message)
    let erin_msg = get_last_message(&erin_group);
    assert_super_admin_changes(&erin_msg, 1, 0, Some(erin.inbox_id()), None);

    // Test 4: Remove Bola as admin
    alix_group
        .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
        .await
        .expect("Failed to remove Bola as admin");

    bola_group.sync().await.expect("Failed to sync");

    // Verify admin list no longer contains Bola
    assert_admin_list_excludes(&bola_group, bola.inbox_id());
    assert_admin_list_contains(&bola_group, &[devon.inbox_id()]);

    // Verify the message
    let last_msg = get_last_message(&bola_group);
    assert_admin_changes(&last_msg, 0, 1, None, Some(bola.inbox_id()));
    assert_super_admin_changes(&last_msg, 0, 0, None, None);

    // Test 5: Remove Caro as super admin
    alix_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            caro.inbox_id().to_string(),
        )
        .await
        .expect("Failed to remove Caro as super admin");

    caro_group.sync().await.expect("Failed to sync");

    // Verify super admin list no longer contains Caro
    assert_super_admin_list_excludes(&caro_group, caro.inbox_id());
    assert_super_admin_list_contains(&caro_group, &[alix.inbox_id(), erin.inbox_id()]);
    assert_admin_list_contains(&caro_group, &[devon.inbox_id()]);

    // Verify the message
    let last_msg = get_last_message(&caro_group);
    assert_super_admin_changes(&last_msg, 0, 1, None, Some(caro.inbox_id()));
    assert_admin_changes(&last_msg, 0, 0, None, None);

    // Test 6: Verify that messages with only member changes (no admin changes) have empty admin fields
    // Add a new member to trigger a member-only change
    let alix_group2 = alix.create_group(None, Default::default()).unwrap();
    alix_group2
        .add_members_by_inbox_id(&[bola.inbox_id().to_string()])
        .await
        .unwrap();

    sync_client_welcomes(&bola).await;

    let bola_groups2 = bola.find_groups(Default::default()).unwrap();
    let new_group = bola_groups2
        .iter()
        .find(|g| g.group_id != bola_group.group_id)
        .expect("Should find new group");

    // Verify admin and super admin lists for new group
    assert_admin_list_contains(new_group, &[]);
    assert_super_admin_list_contains(new_group, &[alix.inbox_id()]);

    // Verify the welcome message has no admin changes
    let member_only_msg = get_first_message(new_group);
    assert_eq!(
        member_only_msg.added_inboxes.len(),
        1,
        "Should have 1 added member"
    );
    assert_no_admin_changes(&member_only_msg);
}
