use crate::groups::UpdateAdminListType;
use crate::tester;
use prost::Message as ProstMessage;
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_proto::xmtp::mls::message_contents::{EncodedContent, GroupUpdated};

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

/// Helper to decode a GroupUpdated message from encoded bytes
fn decode_group_updated(encoded_bytes: &[u8]) -> GroupUpdated {
    let encoded_content = EncodedContent::decode(encoded_bytes).expect("Failed to decode content");
    GroupUpdatedCodec::decode(encoded_content).expect("Failed to decode GroupUpdated")
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
    bola.sync_all_welcomes_and_groups(None)
        .await
        .expect("Failed to sync");
    caro.sync_all_welcomes_and_groups(None)
        .await
        .expect("Failed to sync");
    devon
        .sync_all_welcomes_and_groups(None)
        .await
        .expect("Failed to sync");
    erin.sync_all_welcomes_and_groups(None)
        .await
        .expect("Failed to sync");

    let bola_groups = bola.find_groups(Default::default()).unwrap();
    let bola_group = &bola_groups[0];

    let caro_groups = caro.find_groups(Default::default()).unwrap();
    let caro_group = &caro_groups[0];

    let devon_groups = devon.find_groups(Default::default()).unwrap();
    let devon_group = &devon_groups[0];

    let erin_groups = erin.find_groups(Default::default()).unwrap();
    let erin_group = &erin_groups[0];

    // Verify welcome messages have empty admin fields but correct member fields
    let bola_messages = bola_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    assert!(
        !bola_messages.is_empty(),
        "Bola should have received welcome message"
    );

    let welcome_msg = decode_group_updated(&bola_messages[0].decrypted_message_bytes);
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
    assert_eq!(
        welcome_msg.added_admin_inboxes.len(),
        0,
        "Welcome should have no admin changes"
    );
    assert_eq!(
        welcome_msg.removed_admin_inboxes.len(),
        0,
        "Welcome should have no admin changes"
    );
    assert_eq!(
        welcome_msg.added_super_admin_inboxes.len(),
        0,
        "Welcome should have no super admin changes"
    );
    assert_eq!(
        welcome_msg.removed_super_admin_inboxes.len(),
        0,
        "Welcome should have no super admin changes"
    );

    // Test 1: Add Bola as admin
    alix_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .expect("Failed to add Bola as admin");

    bola_group.sync().await.expect("Failed to sync");

    // Verify admin_list() returns Bola
    let admin_list = bola_group.admin_list().expect("Failed to get admin list");
    assert!(
        admin_list.contains(&bola.inbox_id().to_string()),
        "Bola should be in admin list"
    );
    assert_eq!(admin_list.len(), 1, "Should have 1 admin");

    // Verify super_admin_list() contains only Alix (creator)
    let super_admin_list = bola_group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert!(
        super_admin_list.contains(&alix.inbox_id().to_string()),
        "Alix should be in super admin list"
    );

    let messages = bola_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let last_msg = decode_group_updated(&messages.last().unwrap().decrypted_message_bytes);

    assert_eq!(
        last_msg.added_admin_inboxes.len(),
        1,
        "Should have 1 added admin"
    );
    assert_eq!(
        last_msg.added_admin_inboxes[0].inbox_id,
        bola.inbox_id(),
        "Added admin should be Bola"
    );
    assert_eq!(
        last_msg.removed_admin_inboxes.len(),
        0,
        "Should have no removed admins"
    );
    assert_eq!(
        last_msg.added_super_admin_inboxes.len(),
        0,
        "Should have no added super admins"
    );
    assert_eq!(
        last_msg.removed_super_admin_inboxes.len(),
        0,
        "Should have no removed super admins"
    );

    // Test 2: Add Caro as super admin
    alix_group
        .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
        .await
        .expect("Failed to add Caro as super admin");

    caro_group.sync().await.expect("Failed to sync");

    // Verify super_admin_list() contains Alix and Caro
    let super_admin_list = caro_group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert!(
        super_admin_list.contains(&alix.inbox_id().to_string()),
        "Alix should be in super admin list"
    );
    assert!(
        super_admin_list.contains(&caro.inbox_id().to_string()),
        "Caro should be in super admin list"
    );
    assert_eq!(super_admin_list.len(), 2, "Should have 2 super admins");

    // Verify admin_list() still contains only Bola
    let admin_list = caro_group.admin_list().expect("Failed to get admin list");
    assert!(
        admin_list.contains(&bola.inbox_id().to_string()),
        "Bola should still be in admin list"
    );
    assert_eq!(admin_list.len(), 1, "Should have 1 admin");

    let messages = caro_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let last_msg = decode_group_updated(&messages.last().unwrap().decrypted_message_bytes);

    assert_eq!(
        last_msg.added_super_admin_inboxes.len(),
        1,
        "Should have 1 added super admin"
    );
    assert_eq!(
        last_msg.added_super_admin_inboxes[0].inbox_id,
        caro.inbox_id(),
        "Added super admin should be Caro"
    );
    assert_eq!(
        last_msg.added_admin_inboxes.len(),
        0,
        "Should have no added admins"
    );
    assert_eq!(
        last_msg.removed_admin_inboxes.len(),
        0,
        "Should have no removed admins"
    );
    assert_eq!(
        last_msg.removed_super_admin_inboxes.len(),
        0,
        "Should have no removed super admins"
    );

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

    // Verify admin_list() contains Bola and Devon
    let admin_list = devon_group.admin_list().expect("Failed to get admin list");
    assert!(
        admin_list.contains(&bola.inbox_id().to_string()),
        "Bola should be in admin list"
    );
    assert!(
        admin_list.contains(&devon.inbox_id().to_string()),
        "Devon should be in admin list"
    );
    assert_eq!(admin_list.len(), 2, "Should have 2 admins");

    // Verify super_admin_list() contains Alix, Caro, and Erin
    let super_admin_list = erin_group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert!(
        super_admin_list.contains(&alix.inbox_id().to_string()),
        "Alix should be in super admin list"
    );
    assert!(
        super_admin_list.contains(&caro.inbox_id().to_string()),
        "Caro should be in super admin list"
    );
    assert!(
        super_admin_list.contains(&erin.inbox_id().to_string()),
        "Erin should be in super admin list"
    );
    assert_eq!(super_admin_list.len(), 3, "Should have 3 super admins");

    // Verify Devon's admin addition
    let devon_messages = devon_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let devon_admin_msg = devon_messages
        .iter()
        .rev()
        .nth(1) // Skip Erin's super admin message
        .expect("Should have Devon's admin message");
    let devon_msg = decode_group_updated(&devon_admin_msg.decrypted_message_bytes);

    assert_eq!(
        devon_msg.added_admin_inboxes.len(),
        1,
        "Should have 1 added admin"
    );
    assert_eq!(
        devon_msg.added_admin_inboxes[0].inbox_id,
        devon.inbox_id(),
        "Added admin should be Devon"
    );

    // Verify Erin's super admin addition
    let erin_messages = erin_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let last_msg = decode_group_updated(&erin_messages.last().unwrap().decrypted_message_bytes);

    assert_eq!(
        last_msg.added_super_admin_inboxes.len(),
        1,
        "Should have 1 added super admin"
    );
    assert_eq!(
        last_msg.added_super_admin_inboxes[0].inbox_id,
        erin.inbox_id(),
        "Added super admin should be Erin"
    );

    // Test 4: Remove Bola as admin
    alix_group
        .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
        .await
        .expect("Failed to remove Bola as admin");

    bola_group.sync().await.expect("Failed to sync");

    // Verify admin_list() no longer contains Bola, only Devon
    let admin_list = bola_group.admin_list().expect("Failed to get admin list");
    assert!(
        !admin_list.contains(&bola.inbox_id().to_string()),
        "Bola should not be in admin list"
    );
    assert!(
        admin_list.contains(&devon.inbox_id().to_string()),
        "Devon should still be in admin list"
    );
    assert_eq!(admin_list.len(), 1, "Should have 1 admin");

    let messages = bola_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let last_msg = decode_group_updated(&messages.last().unwrap().decrypted_message_bytes);

    assert_eq!(
        last_msg.removed_admin_inboxes.len(),
        1,
        "Should have 1 removed admin"
    );
    assert_eq!(
        last_msg.removed_admin_inboxes[0].inbox_id,
        bola.inbox_id(),
        "Removed admin should be Bola"
    );
    assert_eq!(
        last_msg.added_admin_inboxes.len(),
        0,
        "Should have no added admins"
    );
    assert_eq!(
        last_msg.added_super_admin_inboxes.len(),
        0,
        "Should have no added super admins"
    );
    assert_eq!(
        last_msg.removed_super_admin_inboxes.len(),
        0,
        "Should have no removed super admins"
    );

    // Test 5: Remove Caro as super admin
    alix_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            caro.inbox_id().to_string(),
        )
        .await
        .expect("Failed to remove Caro as super admin");

    caro_group.sync().await.expect("Failed to sync");

    // Verify super_admin_list() no longer contains Caro, but still contains Alix and Erin
    let super_admin_list = caro_group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert!(
        !super_admin_list.contains(&caro.inbox_id().to_string()),
        "Caro should not be in super admin list"
    );
    assert!(
        super_admin_list.contains(&alix.inbox_id().to_string()),
        "Alix should still be in super admin list"
    );
    assert!(
        super_admin_list.contains(&erin.inbox_id().to_string()),
        "Erin should still be in super admin list"
    );
    assert_eq!(super_admin_list.len(), 2, "Should have 2 super admins");

    // Verify admin_list() still contains only Devon
    let admin_list = caro_group.admin_list().expect("Failed to get admin list");
    assert!(
        admin_list.contains(&devon.inbox_id().to_string()),
        "Devon should still be in admin list"
    );
    assert_eq!(admin_list.len(), 1, "Should have 1 admin");

    let messages = caro_group
        .find_messages(&MsgQueryArgs::default())
        .expect("Failed to find messages");
    let last_msg = decode_group_updated(&messages.last().unwrap().decrypted_message_bytes);

    assert_eq!(
        last_msg.removed_super_admin_inboxes.len(),
        1,
        "Should have 1 removed super admin"
    );
    assert_eq!(
        last_msg.removed_super_admin_inboxes[0].inbox_id,
        caro.inbox_id(),
        "Removed super admin should be Caro"
    );
    assert_eq!(
        last_msg.added_admin_inboxes.len(),
        0,
        "Should have no added admins"
    );
    assert_eq!(
        last_msg.removed_admin_inboxes.len(),
        0,
        "Should have no removed admins"
    );
    assert_eq!(
        last_msg.added_super_admin_inboxes.len(),
        0,
        "Should have no added super admins"
    );

    // Test 6: Verify that messages with only member changes (no admin changes) have empty admin fields
    // Add a new member to trigger a member-only change
    let alix_group2 = alix.create_group(None, Default::default()).unwrap();
    alix_group2
        .add_members_by_inbox_id(&[bola.inbox_id().to_string()])
        .await
        .unwrap();

    bola.sync_all_welcomes_and_groups(None).await.unwrap();

    let bola_groups2 = bola.find_groups(Default::default()).unwrap();
    let new_group = bola_groups2
        .iter()
        .find(|g| g.group_id != bola_group.group_id)
        .expect("Should find new group");

    // Verify admin_list() is empty (Bola is not an admin)
    let admin_list = new_group.admin_list().expect("Failed to get admin list");
    assert_eq!(admin_list.len(), 0, "Should have no admins");

    // Verify super_admin_list() contains only Alix (the creator)
    let super_admin_list = new_group
        .super_admin_list()
        .expect("Failed to get super admin list");
    assert!(
        super_admin_list.contains(&alix.inbox_id().to_string()),
        "Alix should be in super admin list as creator"
    );
    assert_eq!(super_admin_list.len(), 1, "Should have 1 super admin");

    let new_group_messages = new_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let member_only_msg = decode_group_updated(&new_group_messages[0].decrypted_message_bytes);

    // This is a member add (welcome) with no admin changes
    assert_eq!(
        member_only_msg.added_inboxes.len(),
        1,
        "Should have 1 added member"
    );
    assert_eq!(
        member_only_msg.added_admin_inboxes.len(),
        0,
        "Member-only change should have no added admins"
    );
    assert_eq!(
        member_only_msg.removed_admin_inboxes.len(),
        0,
        "Member-only change should have no removed admins"
    );
    assert_eq!(
        member_only_msg.added_super_admin_inboxes.len(),
        0,
        "Member-only change should have no added super admins"
    );
    assert_eq!(
        member_only_msg.removed_super_admin_inboxes.len(),
        0,
        "Member-only change should have no removed super admins"
    );
}
