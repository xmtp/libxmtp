//! Groups containing malformed key packages.

use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn test_create_from_welcome_validation() {
    use crate::groups::app_data::stage_app_data_propose_and_commit;
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_common::{
        app_data::{
            component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
            typed::Component,
        },
        inbox_id::InboxId,
        tls_map::TlsMapDelta,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None).unwrap();
    let provider = alix.context.mls_provider();
    // Add a dictionary membership entry for an inbox that has no MLS leaf.
    // The welcome receiver must still reject this malformed membership view.
    let mut mls_group = alix_group
        .load_mls_group_with_lock(alix.context.mls_storage(), |mut mls_group| {
            let phantom = InboxId::from_hex(&"ff".repeat(32)).unwrap();
            let entry = GroupMembershipEntry {
                version: Some(Version::V1(V1 {
                    sequence_id: 1,
                    failed_installations: vec![],
                })),
            };
            let delta = TlsMapDelta::new().insert(phantom, VLBytes::new(entry.encode_to_vec()));
            let payload = <GroupMembershipComponent as Component>::encode_mutation(&delta).unwrap();
            stage_app_data_propose_and_commit(
                &mut mls_group,
                &provider,
                &alix.identity().installation_keys,
                ComponentId::GROUP_MEMBERSHIP,
                payload,
            )
            .unwrap();
            mls_group.merge_pending_commit(&provider).unwrap();

            Ok(mls_group)
        })
        .unwrap();

    // Now add bo to the group
    force_add_member(&alix, &bo, &alix_group, &mut mls_group, &provider).await;

    // Bo should not be able to actually read this group
    bo.sync_welcomes().await.unwrap();
    let groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(groups.len(), 0);
    let topic = xmtp_db::incoming_envelope::StreamTopic {
        entity_id: bo.context.installation_id().to_vec(),
        kind: xmtp_db::incoming_envelope::NetworkEntityKind::Welcome,
    };
    let db = bo.context.db();
    let rejected = db.read_last_rejection(&topic)??;
    assert_eq!(rejected.code, "invalid_welcome");
    assert!(rejected.sequence_id > Cursor(0));
    assert!(db.pending_envelope(&topic, rejected.sequence_id)?.is_none());
    assert!(db.find_group(&alix_group.group_id)?.is_none());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_create_group_with_member_two_installations_one_malformed_keypackage() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    tester!(alix);

    // bola has two installations
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    // 2) Mark the second installation as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bola_2.context.installation_id().to_vec()]),
    );

    // 3) Create the group, inviting bola (which internally includes bola_1 and bola_2)
    let group = alix
        .create_group_with_identifiers(&[bola_1.identifier()], None, None)
        .await
        .unwrap();

    // 4) Sync from Alix's side
    group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    // 5) Bola_1 syncs welcomes and checks for groups
    bola_1.sync_welcomes().await.unwrap();
    bola_2.sync_welcomes().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_1_groups.len(), 1, "Bola_1 should see exactly 1 group");
    assert_eq!(bola_2_groups.len(), 0, "Bola_2 should see no groups!");

    let bola_1_group = bola_1_groups.first().unwrap();
    bola_1_group.sync().await.unwrap();

    // 6) Verify group membership from both sides
    //    Here we expect 2 *members* (Alix + Bola), though internally Bola might have 2 installations.
    assert_eq!(
        group.members().await.unwrap().len(),
        2,
        "Group should have 2 members"
    );
    assert_eq!(
        bola_1_group.members().await.unwrap().len(),
        2,
        "Bola_1 should also see 2 members in the group"
    );

    // 7) Send a message from Alix and confirm Bola_1 receives it
    let message = b"Hello";
    group
        .send_message(message, SendMessageOpts::default())
        .await
        .unwrap();
    bola_1_group
        .send_message(message, SendMessageOpts::default())
        .await
        .unwrap();

    // Sync both sides again
    group.sync().await.unwrap();
    bola_1_group.sync().await.unwrap();

    // Query messages from Bola_1's perspective
    let messages_bola_1 = bola_1
        .context
        .api()
        .query_group_messages(group.group_id)
        .await
        .unwrap();

    // Dictionary-native creation publishes one valid Add proposal, one
    // membership AppDataUpdate proposal, and their commit. The malformed
    // installation has no Add proposal. Two application messages follow.
    let expected_message_count = 3 + 2;
    assert_eq!(messages_bola_1.len(), expected_message_count);
    use openmls::prelude::ContentType;
    assert_eq!(
        messages_bola_1
            .iter()
            .map(|envelope| envelope.message.content_type())
            .collect::<Vec<_>>(),
        vec![
            ContentType::Proposal,
            ContentType::Proposal,
            ContentType::Commit,
            ContentType::Application,
            ContentType::Application,
        ],
    );

    // Query messages from Alix's perspective
    let messages_alix = alix
        .context
        .api()
        .query_group_messages(group.group_id)
        .await
        .unwrap();

    assert_eq!(messages_alix.len(), expected_message_count);
    assert_eq!(
        message.to_vec(),
        get_latest_message(&group).await.decrypted_message_bytes
    );
    assert_eq!(
        message.to_vec(),
        get_latest_message(bola_1_group)
            .await
            .decrypted_message_bytes
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_create_group_with_member_all_malformed_installations() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    tester!(alix);

    // bola has two installations
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    // 2) Mark both installations as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![
            bola_1.context.installation_id().to_vec(),
            bola_2.context.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the group, which should fail
    let result = alix
        .create_group_with_identifiers(&[bola_1.identifier()], None, None)
        .await;
    // 4) Ensure group creation failed
    assert!(
        result.is_err(),
        "Group creation should fail when all installations have bad key packages"
    );

    // 5) Ensure Bola does not have any groups on either installation
    bola_1.sync_welcomes().await.unwrap();
    bola_2.sync_welcomes().await.unwrap();

    let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(
        bola_1_groups.len(),
        0,
        "Bola_1 should have no groups after failed creation"
    );
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should have no groups after failed creation"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_dm_creation_with_user_two_installations_one_malformed() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    tester!(amal);
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    // 2) Mark bola_2's installation as malformed
    assert_ne!(
        bola_1.context.installation_id(),
        bola_2.context.installation_id()
    );
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bola_2.context.installation_id().to_vec()]),
    );

    // 3) Amal creates a DM group targeting Bola
    let amal_dm = amal
        .find_or_create_dm(bola_1.inbox_id().to_string(), None)
        .await
        .unwrap();

    // 4) Ensure the DM is created with only 2 members (Amal + one valid Bola installation)
    // amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(
        members.len(),
        2,
        "DM should contain only Amal and one valid Bola installation"
    );

    // 5) Bola_1 syncs and confirms it has the DM
    bola_1.sync_welcomes().await.unwrap();
    // xmtp_common::time::sleep(std::time::Duration::from_secs(4)).await;

    let bola_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_groups.len(), 1, "Bola_1 should see the DM group");

    let bola_1_dm: &TestMlsGroup = bola_groups.first().unwrap();
    bola_1_dm.sync().await.unwrap();

    // 6) Ensure Bola_2 does NOT have the group
    bola_2.sync_welcomes().await.unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should not have the DM group due to malformed key package"
    );

    // 7) Send a message from Amal to Bola_1
    let message_text = b"Hello from Amal";
    amal_dm
        .send_message(message_text, SendMessageOpts::default())
        .await
        .unwrap();

    // 8) Sync both sides and check message delivery
    amal_dm.sync().await.unwrap();
    bola_1_dm.sync().await.unwrap();

    // Verify Bola_1 received the message
    let messages_bola_1 = bola_1_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        messages_bola_1.len(),
        2,
        "Bola_1 should have received Amal's message"
    );

    let last_message = messages_bola_1.last().unwrap();
    assert_eq!(
        last_message.decrypted_message_bytes, message_text,
        "Bola_1 should receive the correct message"
    );

    // 9) Bola_1 replies, and Amal confirms receipt
    let reply_text = b"Hey Amal!";
    bola_1_dm
        .send_message(reply_text, SendMessageOpts::default())
        .await
        .unwrap();

    amal_dm.sync().await.unwrap();
    let messages_amal = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_amal.len(), 3, "Amal should receive Bola_1's reply");

    let last_message_amal = messages_amal.last().unwrap();
    assert_eq!(
        last_message_amal.decrypted_message_bytes, reply_text,
        "Amal should receive the correct reply from Bola_1"
    );

    // 10) Ensure only valid installations are considered for the DM
    assert_eq!(
        amal_dm.members().await.unwrap().len(),
        2,
        "Only Amal and Bola_1 should be in the DM"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_dm_creation_with_user_all_malformed_installations() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    tester!(amal);
    tester!(bola_1);
    tester!(bola_2, from: bola_1);

    // 2) Mark all of Bola's installations as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![
            bola_1.context.installation_id().to_vec(),
            bola_2.context.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the DM group, which should fail

    let result = amal
        .find_or_create_dm_by_identity(bola_1.identifier(), None)
        .await;

    // 4) Ensure DM creation fails with the correct error
    assert!(result.is_err());

    // 5) Ensure Bola_1 does not have any groups
    bola_1.sync_welcomes().await.unwrap();
    let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_1_groups.len(),
        0,
        "Bola_1 should have no DM group due to malformed key package"
    );

    // 6) Ensure Bola_2 does not have any groups
    bola_2.sync_welcomes().await.unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should have no DM group due to malformed key package"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_add_inbox_with_bad_installation_to_group() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix);
    tester!(caro);
    tester!(bo_1);
    tester!(bo_2, from: bo_1);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[caro.identifier()], None, None)
        .await
        .unwrap();

    let _ = group.add_members_by_identity(&[bo_1.identifier()]).await;

    bo_2.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bo_2_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bo_2_groups.len(), 1);
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(caro_groups.len(), 1);
    let alix_groups = alix.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(alix_groups.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_add_inbox_with_good_installation_to_group_with_bad_installation() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix);
    tester!(bo_1);
    tester!(bo_2, from: bo_1);
    tester!(caro);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[bo_1.identifier()], None, None)
        .await
        .unwrap();

    let _ = group.add_members_by_identity(&[caro.identifier()]).await;

    caro.sync_welcomes().await.unwrap();
    bo_2.sync_welcomes().await.unwrap();
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(caro_groups.len(), 1);
    let bo_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bo_groups.len(), 1);
    let alix_groups = alix.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(alix_groups.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_remove_inbox_with_good_installation_from_group_with_bad_installation() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix_1);
    tester!(alix_2, from: alix_1);
    tester!(bo);
    tester!(caro);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![alix_2.context.installation_id().to_vec()]),
    );

    let group = alix_1
        .create_group_with_identifiers(&[bo.identifier(), caro.identifier()], None, None)
        .await
        .unwrap();

    assert_eq!(group.members().await.unwrap().len(), 3);
    let _ = group.remove_members_by_identity(&[caro.identifier()]).await;

    caro.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();
    group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();
    assert!(!caro_group.is_active().unwrap());
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();
    assert_eq!(bo_group.members().await.unwrap().len(), 2);
    assert_eq!(group.members().await.unwrap().len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_remove_inbox_with_bad_installation_from_group() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    tester!(alix);
    tester!(bo_1);
    tester!(bo_2, from: bo_1);
    tester!(caro);

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[bo_1.identifier(), caro.identifier()], None, None)
        .await
        .unwrap();

    group.sync().await.unwrap();

    let message_from_alix = b"Hello from Alix";
    group
        .send_message(message_from_alix, SendMessageOpts::default())
        .await
        .unwrap();

    bo_2.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    group.sync().await.unwrap();

    let bo_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();
    let bo_msgs = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_msgs.len(), 2);
    assert_eq!(bo_msgs[1].decrypted_message_bytes, message_from_alix);

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();
    let caro_msgs = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(caro_msgs.len(), 2);
    assert_eq!(caro_msgs[1].decrypted_message_bytes, message_from_alix);

    // Bo replies before removal
    let bo_reply = b"Hey Alix!";
    bo_group
        .send_message(bo_reply, SendMessageOpts::default())
        .await
        .unwrap();

    group.sync().await.unwrap();
    let group_msgs = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(group_msgs.len(), 3);
    assert_eq!(group_msgs.last().unwrap().decrypted_message_bytes, bo_reply);

    // Remove Bo
    group
        .remove_members_by_identity(&[bo_2.identifier()])
        .await
        .unwrap();

    bo_2.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    group.sync().await.unwrap();

    // Bo should no longer be active
    bo_group.sync().await.unwrap();
    assert!(!bo_group.is_active().unwrap());

    let post_removal_msg = b"Caro, just us now!";
    group
        .send_message(post_removal_msg, SendMessageOpts::default())
        .await
        .unwrap();
    let caro_post_removal_msg = b"Nice!";
    caro_group
        .send_message(caro_post_removal_msg, SendMessageOpts::default())
        .await
        .unwrap();

    caro_group.sync().await.unwrap();
    let caro_msgs = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(caro_msgs.len(), 6);
    assert_eq!(
        caro_msgs.last().unwrap().decrypted_message_bytes,
        caro_post_removal_msg
    );
    group.sync().await.unwrap();
    let alix_msgs = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix_msgs.len(), 6);
    assert_eq!(
        alix_msgs.last().unwrap().decrypted_message_bytes,
        caro_post_removal_msg
    );

    let bo_msgs = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        bo_msgs.len(),
        4,
        "Bo should not receive messages after being removed"
    );

    assert_eq!(caro_group.members().await.unwrap().len(), 2);
    assert_eq!(group.members().await.unwrap().len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_can_make_inbox_with_a_bad_key_package_an_admin() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    // 1) Prepare clients
    tester!(amal);
    tester!(charlie);

    // Create a wallet for the user with a bad key package
    tester!(bola);
    // Mark bola's installation as having a malformed key package
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bola.context.installation_id().to_vec()]),
    );

    // 2) Create a group with amal as the only member
    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            None,
        )
        .unwrap();
    amal_group.sync().await.unwrap();

    // 3) Add charlie to the group (normal member)
    let result = amal_group.add_members(&[charlie.inbox_id()]).await;
    assert!(result.is_ok());

    // 4) Initially fail to add bola since they only have one bad key package
    let result = amal_group.add_members(&[bola.inbox_id()]).await;
    assert!(result.is_err());

    // 5) Add a second installation for bola and try and re-add them
    tester!(bola_2, from: bola);
    let result = amal_group.add_members(&[bola.inbox_id()]).await;
    assert!(result.is_ok());

    // 6) Test that bola can not perform an admin only action
    bola_2.sync_welcomes().await.unwrap();
    let binding = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = binding.first().unwrap();
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group".to_string())
        .await;
    assert!(result.is_err());

    // 7) Test adding bola as an admin
    let result = amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await;
    assert!(result.is_ok());

    // 8) Verify bola can perform an admin only action
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group".to_string())
        .await;
    assert!(result.is_ok());

    // 9) Verify we can remove bola as an admin
    let result = amal_group
        .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
        .await;
    assert!(result.is_ok());

    // 10) Verify bola is not an admin
    let admins = amal_group.admin_list().unwrap();
    assert!(!admins.contains(&bola.inbox_id().to_string()));

    // 11) verify bola can't perform an admin only action
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group Forever".to_string())
        .await;
    assert!(result.is_err());
}
