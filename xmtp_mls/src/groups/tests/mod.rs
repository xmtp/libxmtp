mod test_dm;

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use super::{group_permissions::PolicySet, MlsGroup};
use crate::groups::group_mutable_metadata::MessageDisappearingSettings;
#[cfg(not(target_arch = "wasm32"))]
use crate::groups::scoped_client::ScopedGroupClient;
use crate::groups::{
    MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH, MAX_GROUP_NAME_LENGTH,
};
use crate::utils::Tester;
use crate::{
    builder::ClientBuilder,
    groups::{
        build_dm_protected_metadata_extension, build_mutable_metadata_extension_default,
        build_protected_metadata_extension,
        group_metadata::GroupMetadata,
        group_mutable_metadata::MetadataField,
        intents::{PermissionPolicyOption, PermissionUpdateType},
        members::{GroupMember, PermissionLevel},
        mls_sync::GroupMessageProcessingError,
        validate_dm_group, DeliveryStatus, GroupError, GroupMetadataOptions, PreconfiguredPolicies,
        UpdateAdminListType,
    },
    utils::test::FullXmtpClient,
};
use diesel::connection::SimpleConnection;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use futures::future::join_all;
use prost::Message;
use std::sync::Arc;
use wasm_bindgen_test::wasm_bindgen_test;
use xmtp_common::time::now_ns;
use xmtp_common::StreamHandle as _;
use xmtp_common::{assert_err, assert_ok};
use xmtp_content_types::{group_updated::GroupUpdatedCodec, ContentCodec};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group::StoredGroup;
use xmtp_db::schema::groups;
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, GroupQueryArgs},
    group_intent::{IntentKind, IntentState},
    group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};
use xmtp_id::associations::test_utils::WalletTestExt;
use xmtp_id::associations::Identifier;
use xmtp_proto::xmtp::mls::api::v1::group_message::Version;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

async fn receive_group_invite(client: &FullXmtpClient) -> MlsGroup<FullXmtpClient> {
    client
        .sync_welcomes(&client.mls_provider().unwrap())
        .await
        .unwrap();
    let mut groups = client.find_groups(GroupQueryArgs::default()).unwrap();

    groups.remove(0)
}

async fn get_latest_message(group: &MlsGroup<FullXmtpClient>) -> StoredGroupMessage {
    group.sync().await.unwrap();
    let mut messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    messages.pop().unwrap()
}

// Adds a member to the group without the usual validations on group membership
// Used for testing adversarial scenarios
#[cfg(not(target_arch = "wasm32"))]
async fn force_add_member(
    sender_client: &FullXmtpClient,
    new_member_client: &FullXmtpClient,
    sender_group: &MlsGroup<FullXmtpClient>,
    sender_mls_group: &mut openmls::prelude::MlsGroup,
    sender_provider: &XmtpOpenMlsProvider,
) {
    use super::intents::{Installation, SendWelcomesAction};
    use openmls::prelude::tls_codec::Serialize;
    let new_member_provider = new_member_client.mls_provider().unwrap();

    let key_package = new_member_client
        .identity()
        .new_key_package(&new_member_provider)
        .unwrap();
    let hpke_init_key = key_package.hpke_init_key().as_slice().to_vec();
    let (commit, welcome, _) = sender_mls_group
        .add_members(
            sender_provider,
            &sender_client.identity().installation_keys,
            &[key_package],
        )
        .unwrap();
    let serialized_commit = commit.tls_serialize_detached().unwrap();
    let serialized_welcome = welcome.tls_serialize_detached().unwrap();
    let send_welcomes_action = SendWelcomesAction::new(
        vec![Installation {
            installation_key: new_member_client.installation_public_key().into(),
            hpke_public_key: hpke_init_key,
        }],
        serialized_welcome,
    );
    let messages = sender_group
        .prepare_group_messages(vec![(serialized_commit.as_slice(), false)])
        .unwrap();
    sender_client
        .api_client
        .send_group_messages(messages)
        .await
        .unwrap();
    sender_group
        .send_welcomes(send_welcomes_action)
        .await
        .unwrap();
}

#[xmtp_common::test]
async fn test_send_message() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");
    group.send_message(b"hello").await.expect("send message");

    let messages = client
        .api_client
        .query_group_messages(group.group_id, None)
        .await
        .expect("read topic");
    assert_eq!(messages.len(), 2);
}

#[xmtp_common::test]
async fn test_receive_self_message() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");
    let msg = b"hello";
    group.send_message(msg).await.expect("send message");

    group
        .receive(&client.store().conn().unwrap().into())
        .await
        .unwrap();
    // Check for messages
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
}

#[xmtp_common::test]
async fn test_receive_message_from_other() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let alix_group = alix
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");
    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();
    let alix_message = b"hello from alix";
    alix_group
        .send_message(alix_message)
        .await
        .expect("send message");

    let bo_group = receive_group_invite(&bo).await;
    let message = get_latest_message(&bo_group).await;
    assert_eq!(message.decrypted_message_bytes, alix_message);

    let bo_message = b"hello from bo";
    bo_group
        .send_message(bo_message)
        .await
        .expect("send message");

    let message = get_latest_message(&alix_group).await;
    assert_eq!(message.decrypted_message_bytes, bo_message);
}

// Test members function from non group creator
#[xmtp_common::test]
async fn test_members_func_from_non_creator() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    // Get bola's version of the same group
    let bola_groups = bola
        .sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_group = bola_groups.first().unwrap();

    // Call sync for both
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify bola can see the group name
    let bola_group_name = bola_group
        .group_name(&bola_group.mls_provider().unwrap())
        .unwrap();
    assert_eq!(bola_group_name, "");

    // Check if both clients can see the members correctly
    let amal_members: Vec<GroupMember> = amal_group.members().await.unwrap();
    let bola_members: Vec<GroupMember> = bola_group.members().await.unwrap();

    assert_eq!(amal_members.len(), 2);
    assert_eq!(bola_members.len(), 2);

    for member in &amal_members {
        if member.inbox_id == amal.inbox_id() {
            assert_eq!(
                member.permission_level,
                PermissionLevel::SuperAdmin,
                "Amal should be a super admin"
            );
        } else if member.inbox_id == bola.inbox_id() {
            assert_eq!(
                member.permission_level,
                PermissionLevel::Member,
                "Bola should be a member"
            );
        }
    }
}

// Amal and Bola will both try and add Charlie from the same epoch.
// The group should resolve to a consistent state
#[xmtp_common::test]
async fn test_add_member_conflict() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    // Add bola
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    // Get bola's version of the same group
    let bola_groups = bola
        .sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    tracing::info!("Adding charlie from amal");
    // Have amal and bola both invite charlie.
    amal_group
        .add_members_by_inbox_id(&[charlie.inbox_id()])
        .await
        .expect("failed to add charlie");
    tracing::info!("Adding charlie from bola");
    bola_group
        .add_members_by_inbox_id(&[charlie.inbox_id()])
        .await
        .expect("bola's add should succeed in a no-op");

    let summary = amal_group
        .receive(&amal.store().conn().unwrap().into())
        .await
        .unwrap();
    assert!(summary.is_errored());

    // Check Amal's MLS group state.
    let amal_db = XmtpOpenMlsProvider::from(amal.context.store().conn().unwrap());
    let amal_members_len = amal_group
        .load_mls_group_with_lock(&amal_db, |mls_group| Ok(mls_group.members().count()))
        .unwrap();

    assert_eq!(amal_members_len, 3);

    // Check Bola's MLS group state.
    let bola_db = XmtpOpenMlsProvider::from(bola.context.store().conn().unwrap());
    let bola_members_len = bola_group
        .load_mls_group_with_lock(&bola_db, |mls_group| Ok(mls_group.members().count()))
        .unwrap();

    assert_eq!(bola_members_len, 3);

    let amal_uncommitted_intents = amal_db
        .conn_ref()
        .find_group_intents(
            amal_group.group_id.clone(),
            Some(vec![
                IntentState::ToPublish,
                IntentState::Published,
                IntentState::Error,
            ]),
            None,
        )
        .unwrap();
    assert_eq!(amal_uncommitted_intents.len(), 0);

    let bola_failed_intents = bola_db
        .conn_ref()
        .find_group_intents(
            bola_group.group_id.clone(),
            Some(vec![IntentState::Error]),
            None,
        )
        .unwrap();
    // Bola's attempted add should be deleted, since it will have been a no-op on the second try
    assert_eq!(bola_failed_intents.len(), 0);

    // Make sure sending and receiving both worked
    amal_group
        .send_message("hello from amal".as_bytes())
        .await
        .unwrap();
    bola_group
        .send_message("hello from bola".as_bytes())
        .await
        .unwrap();

    let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let matching_message = bola_messages
        .iter()
        .find(|m| m.decrypted_message_bytes == "hello from amal".as_bytes());
    tracing::info!("found message: {:?}", bola_messages);
    assert!(matching_message.is_some());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg(not(target_arch = "wasm32"))]
fn test_create_from_welcome_validation() {
    use crate::groups::{build_group_membership_extension, group_membership::GroupMembership};
    use xmtp_common::assert_logged;
    xmtp_common::traced_test!(async {
        tracing::info!("TEST");
        let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

        let alix_group = alix
            .create_group(None, GroupMetadataOptions::default())
            .unwrap();
        let provider = alix.mls_provider().unwrap();
        // Doctor the group membership
        let mut mls_group = alix_group
            .load_mls_group_with_lock(&provider, |mut mls_group| {
                let mut existing_extensions = mls_group.extensions().clone();
                let mut group_membership = GroupMembership::new();
                group_membership.add("deadbeef".to_string(), 1);
                existing_extensions
                    .add_or_replace(build_group_membership_extension(&group_membership));

                mls_group
                    .update_group_context_extensions(
                        &provider,
                        existing_extensions.clone(),
                        &alix.identity().installation_keys,
                    )
                    .unwrap();
                mls_group.merge_pending_commit(&provider).unwrap();

                Ok(mls_group) // Return the updated group if necessary
            })
            .unwrap();

        // Now add bo to the group
        force_add_member(&alix, &bo, &alix_group, &mut mls_group, &provider).await;

        // Bo should not be able to actually read this group
        bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
        let groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
        assert_eq!(groups.len(), 0);
        assert_logged!("failed to create group from welcome", 1);
    });
}

#[xmtp_common::test]
async fn test_dm_stitching() {
    let alix = Tester::new().await;
    let bo = Tester::new().await;

    let bo_dm = bo
        .find_or_create_dm_by_inbox_id(alix.inbox_id().to_string(), None)
        .await
        .unwrap();
    let alix_dm = alix
        .find_or_create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await
        .unwrap();

    bo_dm.send_message(b"Hello there").await.unwrap();
    alix_dm
        .send_message(b"No, let's use this dm")
        .await
        .unwrap();

    alix.sync_all_welcomes_and_groups(&alix.provider, None)
        .await
        .unwrap();

    // The dm shows up
    let alix_groups = alix
        .provider
        .conn_ref()
        .raw_query_read(|conn| {
            groups::table
                .order(groups::created_at_ns.desc())
                .load::<StoredGroup>(conn)
        })
        .unwrap();
    assert_eq!(alix_groups.len(), 2);
    // They should have the same ID
    assert_eq!(alix_groups[0].dm_id, alix_groups[1].dm_id);

    // The dm is filtered out up
    let mut alix_filtered_groups = alix
        .provider
        .conn_ref()
        .find_groups(GroupQueryArgs::default())
        .unwrap();
    assert_eq!(alix_filtered_groups.len(), 1);

    let dm_group = alix_filtered_groups.pop().unwrap();

    let now = now_ns();
    let ten_seconds = 10_000_000_000;
    assert!(
        ((now - ten_seconds)..(now + ten_seconds)).contains(&dm_group.last_message_ns.unwrap()),
        "last_message_ns {} was not within one second of current time {}",
        dm_group.last_message_ns.unwrap(),
        now
    );

    let dm_group = alix.group(&dm_group.id).unwrap();
    let alix_msgs = dm_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(alix_msgs.len(), 2);

    let msg = String::from_utf8_lossy(&alix_msgs[0].decrypted_message_bytes);
    assert_eq!(msg, "Hello there");

    let msg = String::from_utf8_lossy(&alix_msgs[1].decrypted_message_bytes);
    assert_eq!(msg, "No, let's use this dm");
}

#[xmtp_common::test]
async fn test_add_inbox() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");

    group
        .add_members_by_inbox_id(&[client_2.inbox_id()])
        .await
        .unwrap();

    let group_id = group.group_id;

    let messages = client
        .api_client
        .query_group_messages(group_id, None)
        .await
        .unwrap();

    assert_eq!(messages.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_create_group_with_member_two_installations_one_malformed_keypackage() {
    use xmtp_id::associations::test_utils::WalletTestExt;

    use crate::utils::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();

    // bola has two installations
    let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

    // 2) Mark the second installation as malformed
    set_test_mode_upload_malformed_keypackage(true, Some(vec![bola_2.installation_id().to_vec()]));

    // 3) Create the group, inviting bola (which internally includes bola_1 and bola_2)
    let group = alix
        .create_group_with_members(
            &[bola_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

    // 4) Sync from Alix's side
    group.sync().await.unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 5) Bola_1 syncs welcomes and checks for groups
    bola_1
        .sync_welcomes(&bola_1.mls_provider().unwrap())
        .await
        .unwrap();
    bola_2
        .sync_welcomes(&bola_2.mls_provider().unwrap())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

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
    group.send_message(message).await.unwrap();
    bola_1_group.send_message(message).await.unwrap();

    // Sync both sides again
    group.sync().await.unwrap();
    bola_1_group.sync().await.unwrap();

    // Query messages from Bola_1's perspective
    let messages_bola_1 = bola_1
        .api_client
        .query_group_messages(group.clone().group_id.clone(), None)
        .await
        .unwrap();

    // The last message should be our "Hello from Alix"
    assert_eq!(messages_bola_1.len(), 3);

    // Query messages from Alix's perspective
    let messages_alix = alix
        .api_client
        .query_group_messages(group.clone().group_id, None)
        .await
        .unwrap();

    // The last message should be our "Hello from Alix"
    assert_eq!(messages_alix.len(), 3);
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
    use xmtp_id::associations::test_utils::WalletTestExt;

    use crate::utils::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // bola has two installations
    let bola_wallet = generate_local_wallet();
    let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

    // 2) Mark both installations as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![
            bola_1.installation_id().to_vec(),
            bola_2.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the group, which should fail
    let result = alix
        .create_group_with_members(
            &[bola_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await;
    // 4) Ensure group creation failed
    assert!(
        result.is_err(),
        "Group creation should fail when all installations have bad key packages"
    );

    // 5) Ensure Bola does not have any groups on either installation
    bola_1
        .sync_welcomes(&bola_1.mls_provider().unwrap())
        .await
        .unwrap();
    bola_2
        .sync_welcomes(&bola_2.mls_provider().unwrap())
        .await
        .unwrap();

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
    use crate::utils::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();

    // Bola has two installations
    let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

    // 2) Mark bola_2's installation as malformed
    assert_ne!(bola_1.installation_id(), bola_2.installation_id());
    set_test_mode_upload_malformed_keypackage(true, Some(vec![bola_2.installation_id().to_vec()]));

    // 3) Amal creates a DM group targeting Bola
    let amal_dm = amal
        .find_or_create_dm_by_inbox_id(bola_1.inbox_id().to_string(), None)
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
    bola_1
        .sync_welcomes(&bola_1.mls_provider().unwrap())
        .await
        .unwrap();
    // tokio::time::sleep(std::time::Duration::from_secs(4)).await;

    let bola_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_groups.len(), 1, "Bola_1 should see the DM group");

    let bola_1_dm: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_1_dm.sync().await.unwrap();

    // 6) Ensure Bola_2 does NOT have the group
    bola_2
        .sync_welcomes(&bola_2.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_2_groups = bola_2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_2_groups.len(),
        0,
        "Bola_2 should not have the DM group due to malformed key package"
    );

    // 7) Send a message from Amal to Bola_1
    let message_text = b"Hello from Amal";
    amal_dm.send_message(message_text).await.unwrap();

    // 8) Sync both sides and check message delivery
    amal_dm.sync().await.unwrap();
    bola_1_dm.sync().await.unwrap();

    // Verify Bola_1 received the message
    let messages_bola_1 = bola_1_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        messages_bola_1.len(),
        1,
        "Bola_1 should have received Amal's message"
    );

    let last_message = messages_bola_1.last().unwrap();
    assert_eq!(
        last_message.decrypted_message_bytes, message_text,
        "Bola_1 should receive the correct message"
    );

    // 9) Bola_1 replies, and Amal confirms receipt
    let reply_text = b"Hey Amal!";
    bola_1_dm.send_message(reply_text).await.unwrap();

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
    use xmtp_id::associations::test_utils::WalletTestExt;

    use crate::utils::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();

    // Bola has two installations
    let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

    // 2) Mark all of Bola's installations as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![
            bola_1.installation_id().to_vec(),
            bola_2.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the DM group, which should fail

    let result = amal.find_or_create_dm(bola_wallet.identifier(), None).await;

    // 4) Ensure DM creation fails with the correct error
    assert!(result.is_err());

    // 5) Ensure Bola_1 does not have any groups
    bola_1
        .sync_welcomes(&bola_1.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_1_groups = bola_1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(
        bola_1_groups.len(),
        0,
        "Bola_1 should have no DM group due to malformed key package"
    );

    // 6) Ensure Bola_2 does not have any groups
    bola_2
        .sync_welcomes(&bola_2.mls_provider().unwrap())
        .await
        .unwrap();
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
    use crate::utils::set_test_mode_upload_malformed_keypackage;
    use xmtp_id::associations::test_utils::WalletTestExt;

    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let bo_1 = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_2 = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(true, Some(vec![bo_1.installation_id().to_vec()]));

    let group = alix
        .create_group_with_members(
            &[caro_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

    let _ = group.add_members(&[bo_wallet.identifier()]).await;

    bo_2.sync_welcomes(&bo_2.mls_provider().unwrap())
        .await
        .unwrap();
    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();

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
    use crate::utils::set_test_mode_upload_malformed_keypackage;
    use xmtp_id::associations::test_utils::WalletTestExt;

    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo_1 = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_2 = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(true, Some(vec![bo_1.installation_id().to_vec()]));

    let group = alix
        .create_group_with_members(
            &[bo_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

    let _ = group.add_members(&[caro_wallet.identifier()]).await;

    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();
    bo_2.sync_welcomes(&bo_2.mls_provider().unwrap())
        .await
        .unwrap();
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
    use crate::utils::set_test_mode_upload_malformed_keypackage;
    use xmtp_id::associations::test_utils::WalletTestExt;

    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let alix_1 = ClientBuilder::new_test_client(&alix_wallet).await;
    let alix_2 = ClientBuilder::new_test_client(&alix_wallet).await;
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(true, Some(vec![alix_2.installation_id().to_vec()]));

    let group = alix_1
        .create_group_with_members(
            &[bo_wallet.identifier(), caro_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(group.members().await.unwrap().len(), 3);
    let _ = group.remove_members(&[caro_wallet.identifier()]).await;

    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();
    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();
    assert!(!caro_group.is_active(&caro.mls_provider().unwrap()).unwrap());
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();
    assert_eq!(bo_group.members().await.unwrap().len(), 2);
    assert_eq!(group.members().await.unwrap().len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_remove_inbox_with_bad_installation_from_group() {
    use crate::utils::set_test_mode_upload_malformed_keypackage;
    use xmtp_id::associations::test_utils::WalletTestExt;

    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&alix_wallet).await;
    let bo_1 = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_2 = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(true, Some(vec![bo_1.installation_id().to_vec()]));

    let group = alix
        .create_group_with_members(
            &[bo_wallet.identifier(), caro_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

    group.sync().await.unwrap();

    let message_from_alix = b"Hello from Alix";
    group.send_message(message_from_alix).await.unwrap();

    bo_2.sync_welcomes(&bo_2.mls_provider().unwrap())
        .await
        .unwrap();
    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();
    group.sync().await.unwrap();

    let bo_groups = bo_2.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.sync().await.unwrap();
    let bo_msgs = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bo_msgs.len(), 1);
    assert_eq!(bo_msgs[0].decrypted_message_bytes, message_from_alix);

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();
    let caro_msgs = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(caro_msgs.len(), 1);
    assert_eq!(caro_msgs[0].decrypted_message_bytes, message_from_alix);

    // Bo replies before removal
    let bo_reply = b"Hey Alix!";
    bo_group.send_message(bo_reply).await.unwrap();

    group.sync().await.unwrap();
    let group_msgs = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(group_msgs.len(), 3);
    assert_eq!(group_msgs.last().unwrap().decrypted_message_bytes, bo_reply);

    // Remove Bo
    group
        .remove_members(&[bo_wallet.identifier()])
        .await
        .unwrap();

    bo_2.sync_welcomes(&bo_2.mls_provider().unwrap())
        .await
        .unwrap();
    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();
    group.sync().await.unwrap();

    // Bo should no longer be active
    bo_group.sync().await.unwrap();
    assert!(!bo_group.is_active(&bo_2.mls_provider().unwrap()).unwrap());

    let post_removal_msg = b"Caro, just us now!";
    group.send_message(post_removal_msg).await.unwrap();
    let caro_post_removal_msg = b"Nice!";
    caro_group
        .send_message(caro_post_removal_msg)
        .await
        .unwrap();

    caro_group.sync().await.unwrap();
    let caro_msgs = caro_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(caro_msgs.len(), 5);
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
        3,
        "Bo should not receive messages after being removed"
    );

    assert_eq!(caro_group.members().await.unwrap().len(), 2);
    assert_eq!(group.members().await.unwrap().len(), 2);
}

#[xmtp_common::test]
async fn test_add_invalid_member() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");

    let result = group.add_members_by_inbox_id(&["1234".to_string()]).await;

    assert!(result.is_err());
}

#[xmtp_common::test]
async fn test_add_unregistered_member() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let unconnected_ident = Identifier::rand_ethereum();
    let group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    let result = group.add_members(&[unconnected_ident]).await;

    assert!(result.is_err());
}

#[xmtp_common::test]
async fn test_remove_inbox() {
    let client_1 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    // Add another client onto the network
    let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let group = client_1
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");
    group
        .add_members_by_inbox_id(&[client_2.inbox_id()])
        .await
        .expect("group create failure");

    let messages_with_add = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_with_add.len(), 1);

    // Try and add another member without merging the pending commit
    group
        .remove_members_by_inbox_id(&[client_2.inbox_id()])
        .await
        .expect("group remove members failure");

    let messages_with_remove = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_with_remove.len(), 2);

    // We are expecting 1 message on the group topic, not 2, because the second one should have
    // failed
    let group_id = group.group_id;
    let messages = client_1
        .api_client
        .query_group_messages(group_id, None)
        .await
        .expect("read topic");

    assert_eq!(messages.len(), 2);
}

#[xmtp_common::test]
async fn test_key_update() {
    let client = ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await;
    let bola_client = ClientBuilder::new_test_client_no_sync(&generate_local_wallet()).await;

    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");
    group
        .add_members_by_inbox_id(&[bola_client.inbox_id()])
        .await
        .unwrap();

    group.key_update().await.unwrap();

    let messages = client
        .api_client
        .query_group_messages(group.group_id.clone(), None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);

    let provider: XmtpOpenMlsProvider = client.context.store().conn().unwrap().into();
    let pending_commit_is_none = group
        .load_mls_group_with_lock(&provider, |mls_group| {
            Ok(mls_group.pending_commit().is_none())
        })
        .unwrap();

    assert!(pending_commit_is_none);

    group.send_message(b"hello").await.expect("send message");

    bola_client
        .sync_welcomes(&bola_client.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bola_messages.len(), 1);
}

#[xmtp_common::test]
async fn test_post_commit() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .expect("create group");

    group
        .add_members_by_inbox_id(&[client_2.inbox_id()])
        .await
        .unwrap();

    // Check if the welcome was actually sent
    let welcome_messages = client
        .api_client
        .query_welcome_messages(client_2.installation_public_key(), None)
        .await
        .unwrap();

    assert_eq!(welcome_messages.len(), 1);
}

#[xmtp_common::test]
async fn test_remove_by_account_address() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = &generate_local_wallet();
    let bola = ClientBuilder::new_test_client(bola_wallet).await;
    let charlie_wallet = &generate_local_wallet();
    let _charlie = ClientBuilder::new_test_client(charlie_wallet).await;

    let group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    group
        .add_members(&[bola_wallet.identifier(), charlie_wallet.identifier()])
        .await
        .unwrap();
    tracing::info!("created the group with 2 additional members");
    assert_eq!(group.members().await.unwrap().len(), 3);
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(messages[0].decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 2);
    assert_eq!(group_update.removed_inboxes.len(), 0);

    group
        .remove_members(&[bola_wallet.identifier()])
        .await
        .unwrap();
    assert_eq!(group.members().await.unwrap().len(), 2);
    tracing::info!("removed bola");
    let messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(messages[1].decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 0);
    assert_eq!(group_update.removed_inboxes.len(), 1);

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    assert!(!bola_group
        .is_active(&bola_group.mls_provider().unwrap())
        .unwrap())
}

#[xmtp_common::test]
async fn test_removed_members_cannot_send_message_to_others() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = &generate_local_wallet();
    let bola = ClientBuilder::new_test_client(bola_wallet).await;
    let charlie_wallet = &generate_local_wallet();
    let charlie = ClientBuilder::new_test_client(charlie_wallet).await;

    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members(&[bola_wallet.identifier(), charlie_wallet.identifier()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 3);

    amal_group
        .remove_members(&[bola_wallet.identifier()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    assert!(amal_group
        .members()
        .await
        .unwrap()
        .iter()
        .all(|m| m.inbox_id != bola.inbox_id()));
    assert!(amal_group
        .members()
        .await
        .unwrap()
        .iter()
        .any(|m| m.inbox_id == charlie.inbox_id()));

    amal_group.sync().await.expect("sync failed");

    let message_text = b"hello";

    let bola_group = MlsGroup::<FullXmtpClient>::new(
        bola.clone(),
        amal_group.group_id.clone(),
        amal_group.created_at_ns,
    );
    bola_group
        .send_message(message_text)
        .await
        .expect_err("expected send_message to fail");

    amal_group.sync().await.expect("sync failed");
    amal_group.sync().await.expect("sync failed");

    let amal_messages = amal_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .collect::<Vec<StoredGroupMessage>>();

    assert!(amal_messages.is_empty());
}

#[xmtp_common::test]
async fn test_add_missing_installations() {
    // Setup for test
    let amal_wallet = generate_local_wallet();
    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    assert_eq!(group.members().await.unwrap().len(), 2);

    let provider: XmtpOpenMlsProvider = amal.context.store().conn().unwrap().into();
    // Finished with setup

    // add a second installation for amal using the same wallet
    let _amal_2nd = ClientBuilder::new_test_client(&amal_wallet).await;

    // test if adding the new installation(s) worked
    let new_installations_were_added = group.add_missing_installations(&provider).await;
    assert!(new_installations_were_added.is_ok());

    group.sync().await.unwrap();
    let num_members = group
        .load_mls_group_with_lock(&provider, |mls_group| {
            Ok(mls_group.members().collect::<Vec<_>>().len())
        })
        .unwrap();

    assert_eq!(num_members, 3);
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_self_resolve_epoch_mismatch() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let dave_wallet = generate_local_wallet();
    let dave = ClientBuilder::new_test_client(&dave_wallet).await;
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    // Add bola to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    // Both Amal and Bola are up to date on the group state. Now each of them want to add someone else
    amal_group
        .add_members_by_inbox_id(&[charlie.inbox_id()])
        .await
        .unwrap();

    bola_group
        .add_members_by_inbox_id(&[dave.inbox_id()])
        .await
        .unwrap();

    // Send a message to the group, now that everyone is invited
    amal_group.sync().await.unwrap();
    amal_group.send_message(b"hello").await.unwrap();

    let charlie_group = receive_group_invite(&charlie).await;
    let dave_group = receive_group_invite(&dave).await;

    let (amal_latest_message, bola_latest_message, charlie_latest_message, dave_latest_message) = tokio::join!(
        get_latest_message(&amal_group),
        get_latest_message(&bola_group),
        get_latest_message(&charlie_group),
        get_latest_message(&dave_group)
    );

    let expected_latest_message = b"hello".to_vec();
    assert!(expected_latest_message.eq(&amal_latest_message.decrypted_message_bytes));
    assert!(expected_latest_message.eq(&bola_latest_message.decrypted_message_bytes));
    assert!(expected_latest_message.eq(&charlie_latest_message.decrypted_message_bytes));
    assert!(expected_latest_message.eq(&dave_latest_message.decrypted_message_bytes));
}

#[xmtp_common::test]
async fn test_group_permissions() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            GroupMetadataOptions::default(),
        )
        .unwrap();
    // Add bola to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    assert!(bola_group
        .add_members_by_inbox_id(&[charlie.inbox_id()])
        .await
        .is_err(),);
}

#[xmtp_common::test]
async fn test_group_options() {
    let expected_group_message_disappearing_settings = MessageDisappearingSettings::new(100, 200);

    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal
        .create_group(
            None,
            GroupMetadataOptions {
                name: Some("Group Name".to_string()),
                image_url_square: Some("url".to_string()),
                description: Some("group description".to_string()),
                message_disappearing_settings: Some(expected_group_message_disappearing_settings),
            },
        )
        .unwrap();

    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    let amal_group_image_url: &String = binding
        .attributes
        .get(&MetadataField::GroupImageUrlSquare.to_string())
        .unwrap();
    let amal_group_description: &String = binding
        .attributes
        .get(&MetadataField::Description.to_string())
        .unwrap();
    let amal_group_message_disappear_from_ns = binding
        .attributes
        .get(&MetadataField::MessageDisappearFromNS.to_string())
        .unwrap();
    let amal_group_message_disappear_in_ns = binding
        .attributes
        .get(&MetadataField::MessageDisappearInNS.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "Group Name");
    assert_eq!(amal_group_image_url, "url");
    assert_eq!(amal_group_description, "group description");
    assert_eq!(
        amal_group_message_disappear_from_ns.clone(),
        expected_group_message_disappearing_settings
            .from_ns
            .to_string()
    );
    assert_eq!(
        amal_group_message_disappear_in_ns.clone(),
        expected_group_message_disappearing_settings
            .in_ns
            .to_string()
    );
}

#[xmtp_common::test]
#[ignore]
async fn test_max_limit_add() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            GroupMetadataOptions::default(),
        )
        .unwrap();
    let mut clients = Vec::new();
    for _ in 0..249 {
        let wallet = generate_local_wallet();
        ClientBuilder::new_test_client(&wallet).await;
        clients.push(wallet.identifier());
    }
    amal_group.add_members(&clients).await.unwrap();
    let bola_wallet = generate_local_wallet();
    ClientBuilder::new_test_client(&bola_wallet).await;
    assert!(amal_group
        .add_members_by_inbox_id(&[bola_wallet.get_inbox_id(0)])
        .await
        .is_err(),);
}

#[xmtp_common::test]
async fn test_group_mutable_data() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert!(group_mutable_metadata.attributes.len().eq(&3));
    assert!(group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap()
        .is_empty());

    // Add bola to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group
        .mutable_metadata(&bola_group.mls_provider().unwrap())
        .unwrap();
    assert!(group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap()
        .is_empty());

    // Update group name
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    amal_group.send_message("hello".as_bytes()).await.unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group
        .mutable_metadata(&bola_group.mls_provider().unwrap())
        .expect("msg");
    let bola_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");

    // Verify that bola can not update the group name since they are not the creator
    bola_group
        .update_group_name("New Group Name 2".to_string())
        .await
        .expect_err("expected err");

    // Verify bola group does not see an update
    bola_group.sync().await.unwrap();
    let binding = bola_group
        .mutable_metadata(&bola_group.mls_provider().unwrap())
        .expect("msg");
    let bola_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");
}

#[xmtp_common::test]
async fn test_update_policies_empty_group() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();
    let _bola = ClientBuilder::new_test_client(&bola_wallet).await;

    // Create a group with amal and bola
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group_with_members(
            &[bola_wallet.identifier()],
            policy_set,
            GroupMetadataOptions::default(),
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
    let group_mutable_metadata = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    let group_name_1 = group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(group_name_1, "New Group Name 1");

    // Create a group with just amal
    let policy_set_2 = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group_2 = amal
        .create_group(policy_set_2, GroupMetadataOptions::default())
        .unwrap();

    // Verify empty group fails to update metadata before syncing
    amal_group_2
        .update_group_name("New Group Name 2".to_string())
        .await
        .expect_err("Should fail to update group name before first sync");

    // Sync the group
    amal_group_2.sync().await.unwrap();

    //Verify we can now update the group name
    amal_group_2
        .update_group_name("New Group Name 2".to_string())
        .await
        .unwrap();

    // Verify the name is updated
    amal_group_2.sync().await.unwrap();
    let group_mutable_metadata = amal_group_2
        .mutable_metadata(&amal_group_2.mls_provider().unwrap())
        .unwrap();
    let group_name_2 = group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(group_name_2, "New Group Name 2");
}

#[xmtp_common::test]
async fn test_update_group_image_url_square() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert!(group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupImageUrlSquare.to_string())
        .unwrap()
        .is_empty());

    // Update group name
    amal_group
        .update_group_image_url_square("a url".to_string())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .expect("msg");
    let amal_group_image_url: &String = binding
        .attributes
        .get(&MetadataField::GroupImageUrlSquare.to_string())
        .unwrap();
    assert_eq!(amal_group_image_url, "a url");
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_update_group_message_expiration_settings() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert_eq!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearInNS.to_string()),
        None
    );
    assert_eq!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearFromNS.to_string()),
        None
    );

    // Update group name
    let expected_group_message_expiration_settings = MessageDisappearingSettings::new(100, 200);

    amal_group
        .update_conversation_message_disappearing_settings(
            expected_group_message_expiration_settings,
        )
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .expect("msg");
    let amal_message_expiration_from_ms: &String = binding
        .attributes
        .get(&MetadataField::MessageDisappearFromNS.to_string())
        .unwrap();
    let amal_message_disappear_in_ns: &String = binding
        .attributes
        .get(&MetadataField::MessageDisappearInNS.to_string())
        .unwrap();
    assert_eq!(
        amal_message_expiration_from_ms.clone(),
        expected_group_message_expiration_settings
            .from_ns
            .to_string()
    );
    assert_eq!(
        amal_message_disappear_in_ns.clone(),
        expected_group_message_expiration_settings.in_ns.to_string()
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_group_mutable_data_group_permissions() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert!(group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap()
        .is_empty());

    // Add bola to the group
    amal_group
        .add_members(&[bola_wallet.identifier()])
        .await
        .unwrap();
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group
        .mutable_metadata(&bola_group.mls_provider().unwrap())
        .unwrap();
    assert!(group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap()
        .is_empty());

    // Update group name
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group
        .mutable_metadata(&bola_group.mls_provider().unwrap())
        .expect("msg");
    let bola_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");

    // Verify that bola CAN update the group name since everyone is admin for this group
    bola_group
        .update_group_name("New Group Name 2".to_string())
        .await
        .expect("non creator failed to udpate group name");

    // Verify amal group sees an update
    amal_group.sync().await.unwrap();
    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 2");
}

#[xmtp_common::test]
async fn test_group_admin_list_update() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    // Add bola to the group
    amal_group
        .add_members(&[bola_wallet.identifier()])
        .await
        .unwrap();
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify Amal is the only admin and super admin
    let provider = amal_group.mls_provider().unwrap();
    let admin_list = amal_group.admin_list(&provider).unwrap();
    let super_admin_list = amal_group.super_admin_list(&provider).unwrap();
    drop(provider); // allow connection to be cleaned
    assert_eq!(admin_list.len(), 0);
    assert_eq!(super_admin_list.len(), 1);
    assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

    // Verify that bola can not add caro because they are not an admin
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .add_members_by_inbox_id(&[caro.inbox_id()])
        .await
        .expect_err("expected err");

    // Add bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(
        bola_group
            .admin_list(&bola_group.mls_provider().unwrap())
            .unwrap()
            .len(),
        1
    );
    assert!(bola_group
        .admin_list(&bola_group.mls_provider().unwrap())
        .unwrap()
        .contains(&bola.inbox_id().to_string()));

    // Verify that bola can now add caro because they are an admin
    bola_group
        .add_members_by_inbox_id(&[caro.inbox_id()])
        .await
        .unwrap();

    bola_group.sync().await.unwrap();

    // Verify that bola can not remove amal as a super admin, because
    // Remove admin is super admin only permissions
    bola_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            amal.inbox_id().to_string(),
        )
        .await
        .expect_err("expected err");

    // Now amal removes bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Remove, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(
        bola_group
            .admin_list(&bola_group.mls_provider().unwrap())
            .unwrap()
            .len(),
        0
    );
    assert!(!bola_group
        .admin_list(&bola_group.mls_provider().unwrap())
        .unwrap()
        .contains(&bola.inbox_id().to_string()));

    // Verify that bola can not add charlie because they are not an admin
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .add_members_by_inbox_id(&[charlie.inbox_id()])
        .await
        .expect_err("expected err");
}

#[xmtp_common::test]
async fn test_group_super_admin_list_update() {
    let bola_wallet = generate_local_wallet();
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    // Add bola to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify Amal is the only super admin
    let provider = amal_group.mls_provider().unwrap();
    let admin_list = amal_group.admin_list(&provider).unwrap();
    let super_admin_list = amal_group.super_admin_list(&provider).unwrap();
    drop(provider); // allow connection to be re-added to pool
    assert_eq!(admin_list.len(), 0);
    assert_eq!(super_admin_list.len(), 1);
    assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

    // Verify that bola can not add caro as an admin because they are not a super admin
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_groups.len(), 1);
    let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
        .await
        .expect_err("expected err");

    // Add bola as a super admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    let provider = bola_group.mls_provider().unwrap();
    assert_eq!(bola_group.super_admin_list(&provider).unwrap().len(), 2);
    assert!(bola_group
        .super_admin_list(&provider)
        .unwrap()
        .contains(&bola.inbox_id().to_string()));
    drop(provider); // allow connection to be re-added to pool

    // Verify that bola can now add caro as an admin
    bola_group
        .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
        .await
        .unwrap();
    bola_group.sync().await.unwrap();
    let provider = bola_group.mls_provider().unwrap();
    assert_eq!(bola_group.admin_list(&provider).unwrap().len(), 1);
    assert!(bola_group
        .admin_list(&provider)
        .unwrap()
        .contains(&caro.inbox_id().to_string()));
    drop(provider); // allow connection to be re-added to pool

    // Verify that no one can remove a super admin from a group
    amal_group
        .remove_members(&[bola_wallet.identifier()])
        .await
        .expect_err("expected err");

    // Verify that bola can now remove themself as a super admin
    bola_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            bola.inbox_id().to_string(),
        )
        .await
        .unwrap();
    bola_group.sync().await.unwrap();
    let provider = bola_group.mls_provider().unwrap();
    assert_eq!(bola_group.super_admin_list(&provider).unwrap().len(), 1);
    assert!(!bola_group
        .super_admin_list(&provider)
        .unwrap()
        .contains(&bola.inbox_id().to_string()));
    drop(provider); // allow connection to be re-added to pool

    // Verify that amal can NOT remove themself as a super admin because they are the only remaining
    amal_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            amal.inbox_id().to_string(),
        )
        .await
        .expect_err("expected err");
}

#[xmtp_common::test]
async fn test_group_members_permission_level_update() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    // Add Bola and Caro to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Initial checks for group members
    let initial_members = amal_group.members().await.unwrap();
    let mut count_member = 0;
    let mut count_admin = 0;
    let mut count_super_admin = 0;

    for member in &initial_members {
        match member.permission_level {
            PermissionLevel::Member => count_member += 1,
            PermissionLevel::Admin => count_admin += 1,
            PermissionLevel::SuperAdmin => count_super_admin += 1,
        }
    }

    assert_eq!(
        count_super_admin, 1,
        "Only Amal should be super admin initially"
    );
    assert_eq!(count_admin, 0, "no members are admin only");
    assert_eq!(count_member, 2, "two members have no admin status");

    // Add Bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Check after adding Bola as an admin
    let members = amal_group.members().await.unwrap();
    let mut count_member = 0;
    let mut count_admin = 0;
    let mut count_super_admin = 0;

    for member in &members {
        match member.permission_level {
            PermissionLevel::Member => count_member += 1,
            PermissionLevel::Admin => count_admin += 1,
            PermissionLevel::SuperAdmin => count_super_admin += 1,
        }
    }

    assert_eq!(
        count_super_admin, 1,
        "Only Amal should be super admin initially"
    );
    assert_eq!(count_admin, 1, "bola is admin");
    assert_eq!(count_member, 1, "caro has no admin status");

    // Add Caro as a super admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, caro.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Check after adding Caro as a super admin
    let members = amal_group.members().await.unwrap();
    let mut count_member = 0;
    let mut count_admin = 0;
    let mut count_super_admin = 0;

    for member in &members {
        match member.permission_level {
            PermissionLevel::Member => count_member += 1,
            PermissionLevel::Admin => count_admin += 1,
            PermissionLevel::SuperAdmin => count_super_admin += 1,
        }
    }

    assert_eq!(
        count_super_admin, 2,
        "Amal and Caro should be super admin initially"
    );
    assert_eq!(count_admin, 1, "bola is admin");
    assert_eq!(count_member, 0, "no members have no admin status");
}

#[xmtp_common::test]
async fn test_staged_welcome() {
    // Create Clients
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a group
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    // Amal adds Bola to the group
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
    // and then store that value on the group and insert into the database
    let bola_groups = bola
        .sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();

    // Bola gets the group id. This will be needed to fetch the group from
    // the database.
    let bola_group = bola_groups.first().unwrap();
    let bola_group_id = bola_group.group_id.clone();

    // Bola fetches group from the database
    let bola_fetched_group = bola.group(&bola_group_id).unwrap();

    // Check Bola's group for the added_by_inbox_id of the inviter
    let added_by_inbox = bola_fetched_group.added_by_inbox_id().unwrap();

    // Verify the welcome host_credential is equal to Amal's
    assert_eq!(
        amal.inbox_id(),
        added_by_inbox,
        "The Inviter and added_by_address do not match!"
    );
}

#[xmtp_common::test]
async fn test_can_read_group_creator_inbox_id() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    let mutable_metadata = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert_eq!(mutable_metadata.super_admin_list.len(), 1);
    assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

    let protected_metadata: GroupMetadata = amal_group
        .metadata(&amal_group.mls_provider().unwrap())
        .await
        .unwrap();
    assert_eq!(
        protected_metadata.conversation_type,
        ConversationType::Group
    );

    assert_eq!(protected_metadata.creator_inbox_id, amal.inbox_id());
}

#[xmtp_common::test]
async fn test_can_update_gce_after_failed_commit() {
    // Step 1: Amal creates a group
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    // Step 2:  Amal adds Bola to the group
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    // Step 3: Verify that Bola can update the group name, and amal sees the update
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .update_group_name("Name Update 1".to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    let name = amal_group
        .group_name(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert_eq!(name, "Name Update 1");

    // Step 4:  Bola attempts an action that they do not have permissions for like add admin, fails as expected
    let result = bola_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await;
    if let Err(e) = &result {
        eprintln!("Error updating admin list: {:?}", e);
    }
    // Step 5: Now have Bola attempt to update the group name again
    bola_group
        .update_group_name("Name Update 2".to_string())
        .await
        .unwrap();

    // Step 6: Verify that both clients can sync without error and that the group name has been updated
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    let binding = amal_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "Name Update 2");
    let binding = bola_group
        .mutable_metadata(&bola_group.mls_provider().unwrap())
        .expect("msg");
    let bola_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "Name Update 2");
}

#[xmtp_common::test]
async fn test_can_update_permissions_after_group_creation() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group: MlsGroup<_> = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();

    // Step 2:  Amal adds Bola to the group
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    amal_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    // Step 3: Bola attemps to add Caro, but fails because group is admin only
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_group: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let result = bola_group.add_members_by_inbox_id(&[caro.inbox_id()]).await;
    if let Err(e) = &result {
        eprintln!("Error adding member: {:?}", e);
    } else {
        panic!("Expected error adding member");
    }

    // Step 4: Bola attempts to update permissions but fails because they are not a super admin
    let result = bola_group
        .update_permission_policy(
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Allow,
            None,
        )
        .await;
    if let Err(e) = &result {
        eprintln!("Error updating permissions: {:?}", e);
    } else {
        panic!("Expected error updating permissions");
    }

    // Step 5: Amal updates group permissions so that all members can add
    amal_group
        .update_permission_policy(
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Allow,
            None,
        )
        .await
        .unwrap();

    // Step 6: Bola can now add Caro to the group
    bola_group
        .add_members_by_inbox_id(&[caro.inbox_id()])
        .await
        .unwrap();
    bola_group.sync().await.unwrap();
    let members = bola_group.members().await.unwrap();
    assert_eq!(members.len(), 3);
}

#[xmtp_common::test]
async fn test_optimistic_send() {
    let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
    let bola_wallet = generate_local_wallet();
    let bola = Arc::new(ClientBuilder::new_test_client(&bola_wallet).await);
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();
    // Add bola to the group
    amal_group
        .add_members(&[bola_wallet.identifier()])
        .await
        .unwrap();
    let bola_group = receive_group_invite(&bola).await;

    let ids = vec![
        amal_group.send_message_optimistic(b"test one").unwrap(),
        amal_group.send_message_optimistic(b"test two").unwrap(),
        amal_group.send_message_optimistic(b"test three").unwrap(),
        amal_group.send_message_optimistic(b"test four").unwrap(),
    ];

    let messages = amal_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap()
        .into_iter()
        .collect::<Vec<StoredGroupMessage>>();

    let text = messages
        .iter()
        .cloned()
        .map(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).to_string())
        .collect::<Vec<String>>();
    assert_eq!(
        ids,
        messages
            .iter()
            .cloned()
            .map(|m| m.id)
            .collect::<Vec<Vec<u8>>>()
    );
    assert_eq!(
        text,
        vec![
            "test one".to_string(),
            "test two".to_string(),
            "test three".to_string(),
            "test four".to_string(),
        ]
    );

    let delivery = messages
        .iter()
        .cloned()
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
            DeliveryStatus::Unpublished,
        ]
    );

    amal_group.publish_messages().await.unwrap();
    bola_group.sync().await.unwrap();

    let messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let delivery = messages
        .iter()
        .cloned()
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
            DeliveryStatus::Published,
        ]
    );
}

#[xmtp_common::test]
async fn test_dm_creation() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a dm group targetting bola
    let amal_dm = amal
        .find_or_create_dm_by_inbox_id(bola.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Amal can not add caro to the dm group
    let result = amal_dm.add_members_by_inbox_id(&[caro.inbox_id()]).await;
    assert!(result.is_err());

    // Bola is already a member
    let result = amal_dm
        .add_members_by_inbox_id(&[bola.inbox_id(), caro.inbox_id()])
        .await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Bola can message amal
    let _ = bola.sync_welcomes(&bola.mls_provider().unwrap()).await;
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_dm: &MlsGroup<_> = bola_groups.first().unwrap();
    bola_dm.send_message(b"test one").await.unwrap();

    // Amal sync and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");

    // Amal can not remove bola
    let result = amal_dm.remove_members_by_inbox_id(&[bola.inbox_id()]).await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Neither Amal nor Bola is an admin or super admin
    amal_dm.sync().await.unwrap();
    bola_dm.sync().await.unwrap();
    let is_amal_admin = amal_dm
        .is_admin(amal.inbox_id().to_string(), &amal.mls_provider().unwrap())
        .unwrap();
    let is_bola_admin = amal_dm
        .is_admin(bola.inbox_id().to_string(), &bola.mls_provider().unwrap())
        .unwrap();
    let is_amal_super_admin = amal_dm
        .is_super_admin(amal.inbox_id().to_string(), &amal.mls_provider().unwrap())
        .unwrap();
    let is_bola_super_admin = amal_dm
        .is_super_admin(bola.inbox_id().to_string(), &bola.mls_provider().unwrap())
        .unwrap();
    assert!(!is_amal_admin);
    assert!(!is_bola_admin);
    assert!(!is_amal_super_admin);
    assert!(!is_bola_super_admin);
}

#[xmtp_common::test]
async fn process_messages_abort_on_retryable_error() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let alix_group = alix
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    alix_group
        .add_members_by_inbox_id(&[bo.inbox_id()])
        .await
        .unwrap();

    // Create two commits
    alix_group
        .update_group_name("foo".to_string())
        .await
        .unwrap();
    alix_group
        .update_group_name("bar".to_string())
        .await
        .unwrap();

    let bo_group = receive_group_invite(&bo).await;
    // Get the group messages before we lock the DB, simulating an error that happens
    // in the middle of a sync instead of the beginning
    let bo_messages = bo
        .query_group_messages(&bo_group.group_id, &bo.store().conn().unwrap())
        .await
        .unwrap();

    let conn_1: XmtpOpenMlsProvider = bo.store().conn().unwrap().into();
    let conn_2 = bo.store().conn().unwrap();
    conn_2
        .raw_query_write(|c| {
            c.batch_execute("BEGIN EXCLUSIVE").unwrap();
            Ok::<_, diesel::result::Error>(())
        })
        .unwrap();

    let process_result = bo_group.process_messages(bo_messages, &conn_1).await;
    assert!(process_result.is_errored());
    assert_eq!(process_result.errored.len(), 1);
    assert!(process_result.errored.iter().any(|(_, err)| err
        .to_string()
        .contains("cannot start a transaction within a transaction")));
}

#[xmtp_common::test]
async fn skip_already_processed_messages() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let bo_wallet = generate_local_wallet();
    let bo_client = ClientBuilder::new_test_client(&bo_wallet).await;

    let alix_group = alix
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    alix_group
        .add_members_by_inbox_id(&[bo_client.inbox_id()])
        .await
        .unwrap();

    let alix_message = vec![1];
    alix_group.send_message(&alix_message).await.unwrap();
    bo_client
        .sync_welcomes(&bo_client.mls_provider().unwrap())
        .await
        .unwrap();
    let bo_groups = bo_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();

    let mut bo_messages_from_api = bo_client
        .query_group_messages(&bo_group.group_id, &bo_client.store().conn().unwrap())
        .await
        .unwrap();

    // override the messages to contain already processed messaged
    for msg in &mut bo_messages_from_api {
        if let Some(Version::V1(ref mut v1)) = msg.version {
            v1.id = 0;
        }
    }

    let process_result = bo_group
        .process_messages(bo_messages_from_api, &bo_client.mls_provider().unwrap())
        .await;
    assert!(
        process_result.is_errored(),
        "expected process message error"
    );

    assert_eq!(process_result.errored.len(), 2);
    assert!(process_result
        .errored
        .iter()
        .any(|(_, err)| err.to_string().contains("already processed")));
}

#[xmtp_common::test]
async fn skip_already_processed_intents() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let bo_wallet = generate_local_wallet();
    let bo_client = ClientBuilder::new_test_client(&bo_wallet).await;

    let alix_group = alix
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    alix_group
        .add_members_by_inbox_id(&[bo_client.inbox_id()])
        .await
        .unwrap();

    bo_client
        .sync_welcomes(&bo_client.mls_provider().unwrap())
        .await
        .unwrap();
    let bo_groups = bo_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group.send_message(&[2]).await.unwrap();
    let bo_provider = bo_client.mls_provider().unwrap();
    let intent = bo_provider
        .conn_ref()
        .find_group_intents(
            bo_group.clone().group_id,
            Some(vec![IntentState::Processed]),
            None,
        )
        .unwrap();
    assert_eq!(intent.len(), 2); //key_update and send_message

    let process_result = bo_group
        .sync_until_intent_resolved(&bo_provider, intent[1].id)
        .await;
    assert_ok!(process_result);
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_parallel_syncs() {
    let wallet = generate_local_wallet();
    let alix1 = Arc::new(ClientBuilder::new_test_client(&wallet).await);
    alix1.wait_for_sync_worker_init().await;

    let alix1_group = alix1
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    let alix2 = ClientBuilder::new_test_client(&wallet).await;

    let sync_tasks: Vec<_> = (0..10)
        .map(|_| {
            let group_clone = alix1_group.clone();
            // Each of these syncs is going to trigger the client to invite alix2 to the group
            // because of the race
            xmtp_common::spawn(None, async move { group_clone.sync().await }).join()
        })
        .collect();

    let results = join_all(sync_tasks).await;

    // Check if any of the syncs failed
    for result in results.into_iter() {
        assert!(result.is_ok(), "Sync error {:?}", result.err());
    }

    // Make sure that only one welcome was sent
    let alix2_welcomes = alix1
        .api_client
        .query_welcome_messages(alix2.installation_public_key(), None)
        .await
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    // Make sure that only one group message was sent
    let group_messages = alix1
        .api_client
        .query_group_messages(alix1_group.group_id.clone(), None)
        .await
        .unwrap();
    assert_eq!(group_messages.len(), 1);

    let alix2_group = receive_group_invite(&alix2).await;

    // Send a message from alix1
    alix1_group
        .send_message("hi from alix1".as_bytes())
        .await
        .unwrap();
    // Send a message from alix2
    alix2_group
        .send_message("hi from alix2".as_bytes())
        .await
        .unwrap();

    // Sync both clients
    alix1_group.sync().await.unwrap();
    alix2_group.sync().await.unwrap();

    let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix1_messages.len(), alix2_messages.len());

    assert!(alix1_messages
        .iter()
        .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes()));
    assert!(alix2_messages
        .iter()
        .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes()));
}

// Create a membership update intent, but don't sync it yet
async fn create_membership_update_no_sync(
    group: &MlsGroup<FullXmtpClient>,
    provider: &XmtpOpenMlsProvider,
) {
    let intent_data = group
        .get_membership_update_intent(provider, &[], &[])
        .await
        .unwrap();

    // If there is nothing to do, stop here
    if intent_data.is_empty() {
        return;
    }

    group
        .queue_intent(
            provider,
            IntentKind::UpdateGroupMembership,
            intent_data.into(),
            false,
        )
        .unwrap();
}

/**
 * This test case simulates situations where adding missing
 * installations gets interrupted before the sync part happens
 *
 * We need to be safe even in situations where there are multiple
 * intents that do the same thing, leading to conflicts
 */
#[xmtp_common::test(flavor = "multi_thread")]
async fn add_missing_installs_reentrancy() {
    let wallet = generate_local_wallet();
    let alix1 = ClientBuilder::new_test_client(&wallet).await;
    let alix1_group = alix1
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    let alix1_provider = alix1.mls_provider().unwrap();

    let alix2 = ClientBuilder::new_test_client(&wallet).await;

    // We are going to run add_missing_installations TWICE
    // which will create two intents to add the installations
    create_membership_update_no_sync(&alix1_group, &alix1_provider).await;
    create_membership_update_no_sync(&alix1_group, &alix1_provider).await;

    // Now I am going to run publish intents multiple times
    alix1_group
        .publish_intents(&alix1_provider)
        .await
        .expect("Expect publish to be OK");
    alix1_group
        .publish_intents(&alix1_provider)
        .await
        .expect("Expected publish to be OK");

    // Now I am going to sync twice
    alix1_group.sync_with_conn(&alix1_provider).await.unwrap();
    alix1_group.sync_with_conn(&alix1_provider).await.unwrap();

    // Make sure that only one welcome was sent
    let alix2_welcomes = alix1
        .api_client
        .query_welcome_messages(alix2.installation_public_key(), None)
        .await
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    // We expect two group messages to have been sent,
    // but only the first is valid
    let group_messages = alix1
        .api_client
        .query_group_messages(alix1_group.group_id.clone(), None)
        .await
        .unwrap();
    assert_eq!(group_messages.len(), 2);

    let alix2_group = receive_group_invite(&alix2).await;

    // Send a message from alix1
    alix1_group
        .send_message("hi from alix1".as_bytes())
        .await
        .unwrap();
    // Send a message from alix2
    alix2_group
        .send_message("hi from alix2".as_bytes())
        .await
        .unwrap();

    // Sync both clients
    alix1_group.sync().await.unwrap();
    alix2_group.sync().await.unwrap();

    let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix1_messages.len(), alix2_messages.len());

    assert!(alix1_messages
        .iter()
        .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes()));
    assert!(alix2_messages
        .iter()
        .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes()));
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn respect_allow_epoch_increment() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;

    let group = client
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    let _client_2 = ClientBuilder::new_test_client(&wallet).await;

    // Sync the group to get the message adding client_2 published to the network
    group.sync().await.unwrap();

    // Retrieve the envelope for the commit from the network
    let messages = client
        .api_client
        .query_group_messages(group.group_id.clone(), None)
        .await
        .unwrap();

    let first_envelope = messages.first().unwrap();

    let Some(xmtp_proto::xmtp::mls::api::v1::group_message::Version::V1(first_message)) =
        first_envelope.clone().version
    else {
        panic!("wrong message format")
    };
    let provider = client.mls_provider().unwrap();
    let process_result = group
        .process_message(&provider, &first_message, false)
        .await;

    assert_err!(
        process_result,
        GroupMessageProcessingError::EpochIncrementNotAllowed
    );
}

#[xmtp_common::test]
async fn test_get_and_set_consent() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let alix_group = alix
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    // group consent state should be allowed if user created it
    assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Allowed);

    alix_group
        .update_consent_state(ConsentState::Denied)
        .unwrap();
    assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Denied);

    alix_group
        .add_members_by_inbox_id(&[bola.inbox_id()])
        .await
        .unwrap();

    bola.sync_welcomes(&bola.mls_provider().unwrap())
        .await
        .unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    // group consent state should default to unknown for users who did not create the group
    assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Unknown);

    bola_group
        .send_message("hi from bola".as_bytes())
        .await
        .unwrap();

    // group consent state should be allowed if user sends a message to the group
    assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Allowed);

    alix_group
        .add_members_by_inbox_id(&[caro.inbox_id()])
        .await
        .unwrap();

    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();

    caro_group
        .send_message_optimistic("hi from caro".as_bytes())
        .unwrap();

    caro_group.publish_messages().await.unwrap();

    // group consent state should be allowed if user publishes a message to the group
    assert_eq!(caro_group.consent_state().unwrap(), ConsentState::Allowed);
}

#[xmtp_common::test]
// TODO(rich): Generalize the test once fixed - test messages that are 0, 1, 2, 3, 4, 5 epochs behind
async fn test_max_past_epochs() {
    // Create group with two members
    let bo_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;
    let alix_group = alix
        .create_group_with_members(
            &[bo_wallet.identifier()],
            None,
            GroupMetadataOptions::default(),
        )
        .await
        .unwrap();

    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();

    // Both members see the same amount of messages to start
    alix_group.send_message("alix 1".as_bytes()).await.unwrap();
    bo_group.send_message("bo 1".as_bytes()).await.unwrap();
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let alix_messages = alix_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap();
    let bo_messages = bo_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(alix_messages.len(), 2);
    assert_eq!(bo_messages.len(), 2);

    // Alix moves the group forward by 1 epoch
    alix_group
        .update_group_name("new name".to_string())
        .await
        .unwrap();

    // Bo sends a message while 1 epoch behind
    bo_group.send_message("bo 2".as_bytes()).await.unwrap();

    // If max_past_epochs is working, Alix should be able to decrypt Bo's message
    alix_group.sync().await.unwrap();
    bo_group.sync().await.unwrap();

    let alix_messages = alix_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap();
    let bo_messages = bo_group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(bo_messages.len(), 3);
    assert_eq!(alix_messages.len(), 3); // Fails here, 2 != 3
}

#[wasm_bindgen_test(unsupported = tokio::test)]
async fn test_validate_dm_group() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let added_by_inbox = "added_by_inbox_id";
    let creator_inbox_id = client.context.identity.inbox_id();
    let dm_target_inbox_id = added_by_inbox.to_string();

    // Test case 1: Valid DM group
    let valid_dm_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
        client.clone().into(),
        dm_target_inbox_id.clone(),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(valid_dm_group
        .load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group| {
            validate_dm_group(&client, &mls_group, added_by_inbox)
        })
        .is_ok());

    // Test case 2: Invalid conversation type
    let invalid_protected_metadata =
        build_protected_metadata_extension(creator_inbox_id, ConversationType::Group).unwrap();
    let invalid_type_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
        client.clone().into(),
        dm_target_inbox_id.clone(),
        Some(invalid_protected_metadata),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        invalid_type_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
            validate_dm_group(&client, &mls_group, added_by_inbox)
        ),
        Err(GroupError::Generic(msg)) if msg.contains("Invalid conversation type")
    ));
    // Test case 3: Missing DmMembers
    // This case is not easily testable with the current structure, as DmMembers are set in the protected metadata

    // Test case 4: Mismatched DM members
    let mismatched_dm_members =
        build_dm_protected_metadata_extension(creator_inbox_id, "wrong_inbox_id".to_string())
            .unwrap();
    let mismatched_dm_members_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
        client.clone().into(),
        dm_target_inbox_id.clone(),
        Some(mismatched_dm_members),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        mismatched_dm_members_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
            validate_dm_group(&client, &mls_group, added_by_inbox)
        ),
        Err(GroupError::Generic(msg)) if msg.contains("DM members do not match expected inboxes")
    ));

    // Test case 5: Non-empty admin list
    let non_empty_admin_list =
        build_mutable_metadata_extension_default(creator_inbox_id, GroupMetadataOptions::default())
            .unwrap();
    let non_empty_admin_list_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
        client.clone().into(),
        dm_target_inbox_id.clone(),
        None,
        Some(non_empty_admin_list),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        non_empty_admin_list_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
            validate_dm_group(&client, &mls_group, added_by_inbox)
        ),
        Err(GroupError::Generic(msg)) if msg.contains("DM group must have empty admin and super admin lists")
    ));

    // Test case 6: Non-empty super admin list
    // Similar to test case 5, but with super_admin_list

    // Test case 7: Invalid permissions
    let invalid_permissions = PolicySet::default();
    let invalid_permissions_group = MlsGroup::<FullXmtpClient>::create_test_dm_group(
        client.clone().into(),
        dm_target_inbox_id.clone(),
        None,
        None,
        None,
        Some(invalid_permissions),
        None,
    )
    .unwrap();
    assert!(matches!(
        invalid_permissions_group.load_mls_group_with_lock(client.mls_provider().unwrap(), |mls_group|
            validate_dm_group(&client, &mls_group, added_by_inbox)
        ),
        Err(GroupError::Generic(msg)) if msg.contains("Invalid permissions for DM group")
    ));
}

#[xmtp_common::test]
async fn test_respects_character_limits_for_group_metadata() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal
        .create_group(policy_set, GroupMetadataOptions::default())
        .unwrap();
    amal_group.sync().await.unwrap();

    let overlong_name = "a".repeat(MAX_GROUP_NAME_LENGTH + 1);
    let overlong_description = "b".repeat(MAX_GROUP_DESCRIPTION_LENGTH + 1);
    let overlong_image_url =
        "http://example.com/".to_string() + &"c".repeat(MAX_GROUP_IMAGE_URL_LENGTH);

    // Verify that updating the name with an excessive length fails
    let result = amal_group.update_group_name(overlong_name).await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_NAME_LENGTH)
    );

    // Verify that updating the description with an excessive length fails
    let result = amal_group
        .update_group_description(overlong_description)
        .await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_DESCRIPTION_LENGTH)
    );

    // Verify that updating the image URL with an excessive length fails
    let result = amal_group
        .update_group_image_url_square(overlong_image_url)
        .await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_GROUP_IMAGE_URL_LENGTH)
    );

    // Verify updates with valid lengths are successful
    let valid_name = "Valid Group Name".to_string();
    let valid_description = "Valid group description within limit.".to_string();
    let valid_image_url = "http://example.com/image.png".to_string();

    amal_group
        .update_group_name(valid_name.clone())
        .await
        .unwrap();
    amal_group
        .update_group_description(valid_description.clone())
        .await
        .unwrap();
    amal_group
        .update_group_image_url_square(valid_image_url.clone())
        .await
        .unwrap();

    // Sync and verify stored values
    amal_group.sync().await.unwrap();

    let provider = amal_group.mls_provider().unwrap();
    let metadata = amal_group.mutable_metadata(&provider).unwrap();

    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap(),
        &valid_name
    );
    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::Description.to_string())
            .unwrap(),
        &valid_description
    );
    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap(),
        &valid_image_url
    );
}

fn increment_patch_version(version: &str) -> Option<String> {
    // Split version into numeric part and suffix (if any)
    let (version_part, suffix) = match version.split_once('-') {
        Some((v, s)) => (v, Some(s)),
        None => (version, None),
    };

    // Split numeric version string into components
    let mut parts: Vec<&str> = version_part.split('.').collect();

    // Ensure we have exactly 3 parts (major.minor.patch)
    if parts.len() != 3 {
        return None;
    }

    // Parse the patch number and increment it
    let patch = parts[2].parse::<u32>().ok()?;
    let new_patch = patch + 1;

    // Replace the patch number with the incremented value
    let binding = new_patch.to_string();
    parts[2] = &binding;

    // Join the parts back together with dots and add suffix if it existed
    let new_version = parts.join(".");
    match suffix {
        Some(s) => Some(format!("{}-{}", new_version, s)),
        None => Some(new_version),
    }
}

#[xmtp_common::test]
fn test_increment_patch_version() {
    assert_eq!(increment_patch_version("1.2.3"), Some("1.2.4".to_string()));
    assert_eq!(increment_patch_version("0.0.9"), Some("0.0.10".to_string()));
    assert_eq!(increment_patch_version("1.0.0"), Some("1.0.1".to_string()));
    assert_eq!(
        increment_patch_version("1.0.0-alpha"),
        Some("1.0.1-alpha".to_string())
    );

    // Invalid inputs should return None
    assert_eq!(increment_patch_version("1.2"), None);
    assert_eq!(increment_patch_version("1.2.3.4"), None);
    assert_eq!(increment_patch_version("invalid"), None);
}

#[xmtp_common::test]
async fn test_can_set_min_supported_protocol_version_for_commit() {
    // Step 1: Create two clients, amal is one version ahead of bo
    let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_version = amal.version_info().pkg_version();
    amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

    let mut bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal updates the group name and sends a message to the group
    amal_group
        .update_group_name("new name".to_string())
        .await
        .unwrap();
    amal_group
        .send_message("Hello, world!".as_bytes())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message even though they are on different client versions
    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);

    let message_text = String::from_utf8_lossy(&messages[1].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group version to match their client version
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes())
        .await
        .unwrap();

    // Step 6: Bo should now be unable to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);

    // Step 7: Bo updates their client, and see if we can then download latest messages
    let bo_version = bo.version_info().pkg_version();
    bo.test_update_version(increment_patch_version(bo_version).unwrap().as_str());

    // Refresh Bo's group context
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();

    bo_group.sync().await.unwrap();
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 4);
}

#[xmtp_common::test]
async fn test_client_on_old_version_pauses_after_joining_min_version_group() {
    // Step 1: Create three clients, amal and bo are one version ahead of caro
    let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_version = amal.version_info().pkg_version();
    amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

    let mut bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo_version = bo.version_info().pkg_version();
    bo.test_update_version(increment_patch_version(bo_version).unwrap().as_str());

    let mut caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    assert!(caro.version_info().pkg_version() != amal.version_info().pkg_version());
    assert!(bo.version_info().pkg_version() == amal.version_info().pkg_version());

    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal sends a message to the group
    amal_group
        .send_message("Hello, world!".as_bytes())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message
    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 1);

    let message_text = String::from_utf8_lossy(&messages[0].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group to have a min version of current version + 1
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes())
        .await
        .unwrap();

    // Step 6: Bo should still be able to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);

    // Step 7: Amal adds caro as a member
    amal_group
        .add_members_by_inbox_id(&[caro.context.identity.inbox_id()])
        .await
        .unwrap();

    // Caro received the invite for the group
    caro.sync_welcomes(&caro.mls_provider().unwrap())
        .await
        .unwrap();
    let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = binding.first().unwrap();
    assert!(caro_group.group_id == amal_group.group_id);

    // Caro group is paused immediately after joining
    let is_paused = caro_group
        .paused_for_version(&caro.mls_provider().unwrap())
        .unwrap()
        .is_some();
    assert!(is_paused);
    let result = caro_group.send_message("Hello from Caro".as_bytes()).await;
    assert!(matches!(result, Err(GroupError::GroupPausedUntilUpdate(_))));

    // Caro updates their client to the same version as amal and syncs to unpause the group
    let caro_version = caro.version_info().pkg_version();
    caro.test_update_version(increment_patch_version(caro_version).unwrap().as_str());
    let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = binding.first().unwrap();
    assert!(caro_group.group_id == amal_group.group_id);
    caro_group.sync().await.unwrap();

    // Caro should now be able to send a message
    caro_group
        .send_message("Hello from Caro".as_bytes())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(
        messages[messages.len() - 1].decrypted_message_bytes,
        "Hello from Caro".as_bytes()
    );
}

#[xmtp_common::test]
async fn test_only_super_admins_can_set_min_supported_protocol_version() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();
    amal_group
        .update_admin_list(
            UpdateAdminListType::Add,
            bo.context.identity.inbox_id().to_string(),
        )
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    let is_bo_admin = amal_group
        .is_admin(
            bo.context.identity.inbox_id().to_string(),
            &amal.mls_provider().unwrap(),
        )
        .unwrap();
    assert!(is_bo_admin);

    let is_bo_super_admin = amal_group
        .is_super_admin(
            bo.context.identity.inbox_id().to_string(),
            &amal.mls_provider().unwrap(),
        )
        .unwrap();
    assert!(!is_bo_super_admin);

    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    let metadata = bo_group
        .mutable_metadata(&amal_group.mls_provider().unwrap())
        .unwrap();
    let min_version = metadata
        .attributes
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version, None);

    let result = bo_group.update_group_min_version_to_match_self().await;
    assert!(result.is_err());
    bo_group.sync().await.unwrap();

    let metadata = bo_group
        .mutable_metadata(&bo_group.mls_provider().unwrap())
        .unwrap();
    let min_version = metadata
        .attributes
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version, None);

    amal_group.sync().await.unwrap();
    let result = amal_group.update_group_min_version_to_match_self().await;
    assert!(result.is_ok());
    bo_group.sync().await.unwrap();

    let metadata = bo_group
        .mutable_metadata(&bo_group.mls_provider().unwrap())
        .unwrap();
    let min_version = metadata
        .attributes
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version.unwrap(), amal.version_info().pkg_version());
}

#[xmtp_common::test]
async fn test_send_message_while_paused_after_welcome_returns_expected_error() {
    // Create two clients with different versions
    let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_version = amal.version_info().pkg_version();
    amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a group and adds bo
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Amal sets minimum version requirement
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Bo joins group and attempts to send message
    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();

    // If bo tries to send a message before syncing the group, we get a SyncFailedToWait error
    let result = bo_group.send_message("Hello from Bo".as_bytes()).await;
    assert!(
        matches!(result, Err(GroupError::SyncFailedToWait(_))),
        "Expected SyncFailedToWait error, got {:?}",
        result
    );

    bo_group.sync().await.unwrap();

    // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
    let result = bo_group.send_message("Hello from Bo".as_bytes()).await;
    if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
        assert_eq!(version, amal.version_info().pkg_version());
    } else {
        panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
    }
}

#[xmtp_common::test]
async fn test_send_message_after_min_version_update_gets_expected_error() {
    // Create two clients with different versions
    let mut amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_version = amal.version_info().pkg_version();
    amal.test_update_version(increment_patch_version(amal_version).unwrap().as_str());

    let mut bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a group and adds bo
    let amal_group = amal
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    amal_group
        .add_members_by_inbox_id(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Bo joins group and successfully sends initial message
    bo.sync_welcomes(&bo.mls_provider().unwrap()).await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    bo_group
        .send_message("Hello from Bo".as_bytes())
        .await
        .unwrap();

    // Amal sets new minimum version requirement
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Bo's attempt to send message before syncing should now fail with SyncFailedToWait error
    let result = bo_group
        .send_message("Second message from Bo".as_bytes())
        .await;
    assert!(
        matches!(result, Err(GroupError::SyncFailedToWait(_))),
        "Expected SyncFailedToWait error, got {:?}",
        result
    );

    // Bo syncs to get the version update
    bo_group.sync().await.unwrap();

    // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
    let result = bo_group.send_message("Hello from Bo".as_bytes()).await;
    if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
        assert_eq!(version, amal.version_info().pkg_version());
    } else {
        panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
    }

    // Verify Bo can send again after updating their version
    let bo_version = bo.version_info().pkg_version();
    bo.test_update_version(increment_patch_version(bo_version).unwrap().as_str());

    // Need to get fresh group reference after version update
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    // Should now succeed
    let result = bo_group
        .send_message("Message after update".as_bytes())
        .await;
    assert!(result.is_ok());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_can_make_inbox_with_a_bad_key_package_an_admin() {
    use crate::utils::set_test_mode_upload_malformed_keypackage;

    // 1) Prepare clients
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a wallet for the user with a bad key package
    let bola_wallet = generate_local_wallet();
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    // Mark bola's installation as having a malformed key package
    set_test_mode_upload_malformed_keypackage(true, Some(vec![bola.installation_id().to_vec()]));

    // 2) Create a group with amal as the only member
    let amal_group = amal
        .create_group(
            Some(PreconfiguredPolicies::AdminsOnly.to_policy_set()),
            GroupMetadataOptions::default(),
        )
        .unwrap();
    amal_group.sync().await.unwrap();

    // 3) Add charlie to the group (normal member)
    let result = amal_group
        .add_members_by_inbox_id(&[charlie.inbox_id()])
        .await;
    assert!(result.is_ok());

    // 4) Initially fail to add bola since they only have one bad key package
    let result = amal_group.add_members_by_inbox_id(&[bola.inbox_id()]).await;
    assert!(result.is_err());

    // 5) Add a second installation for bola and try and re-add them
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;
    let result = amal_group.add_members_by_inbox_id(&[bola.inbox_id()]).await;
    assert!(result.is_ok());

    // 6) Test that bola can not perform an admin only action
    bola_2
        .sync_welcomes(&bola_2.mls_provider().unwrap())
        .await
        .unwrap();
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
    let admins = amal_group
        .admin_list(&amal_group.mls_provider().unwrap())
        .unwrap();
    assert!(!admins.contains(&bola.inbox_id().to_string()));

    // 11) verify bola can't perform an admin only action
    bola_group.sync().await.unwrap();
    let result = bola_group
        .update_group_name("Bola's Group Forever".to_string())
        .await;
    assert!(result.is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_when_processing_message_return_future_wrong_epoch_group_marked_probably_forked() {
    use crate::utils::set_test_mode_future_wrong_epoch;

    let client_a = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let client_b = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let group_a = client_a
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();
    group_a
        .add_members_by_inbox_id(&[client_b.inbox_id()])
        .await
        .unwrap();

    client_b
        .sync_welcomes(&client_b.mls_provider().unwrap())
        .await
        .unwrap();

    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    group_a.send_message(&[1]).await.unwrap();
    set_test_mode_future_wrong_epoch(true);
    group_b.sync().await.unwrap();
    set_test_mode_future_wrong_epoch(false);
    let group_debug_info = group_b.debug_info().await.unwrap();
    assert!(group_debug_info.maybe_forked);
    assert!(!group_debug_info.fork_details.is_empty());
    client_b
        .mls_provider()
        .unwrap()
        .conn_ref()
        .clear_fork_flag_for_group(&group_b.group_id)
        .unwrap();
    let group_debug_info = group_b.debug_info().await.unwrap();
    assert!(!group_debug_info.maybe_forked);
    assert!(group_debug_info.fork_details.is_empty());
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn can_stream_out_of_order_without_forking() {
    let wallet_a = generate_local_wallet();
    let wallet_b = generate_local_wallet();
    let wallet_c = generate_local_wallet();
    let client_a1 = ClientBuilder::new_test_client(&wallet_a).await;
    let client_b = ClientBuilder::new_test_client(&wallet_b).await;
    let client_c = ClientBuilder::new_test_client(&wallet_c).await;

    // Create a group
    let group_a = client_a1
        .create_group(None, GroupMetadataOptions::default())
        .unwrap();

    // Add client_b and client_c to the group
    group_a
        .add_members_by_inbox_id(&[client_b.inbox_id(), client_c.inbox_id()])
        .await
        .unwrap();

    // Sync the group
    client_b
        .sync_welcomes(&client_b.mls_provider().unwrap())
        .await
        .unwrap();
    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    client_c
        .sync_welcomes(&client_c.mls_provider().unwrap())
        .await
        .unwrap();
    let binding = client_c.find_groups(GroupQueryArgs::default()).unwrap();
    let group_c = binding.first().unwrap();

    // Each client sends a message and syncs (ensures any key update commits are sent)
    group_a
        .send_message_optimistic("Message a1".as_bytes())
        .unwrap();
    group_a
        .publish_intents(&group_a.mls_provider().unwrap())
        .await
        .unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_b
        .send_message_optimistic("Message b1".as_bytes())
        .unwrap();
    group_b
        .publish_intents(&group_b.mls_provider().unwrap())
        .await
        .unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_c
        .send_message_optimistic("Message c1".as_bytes())
        .unwrap();
    group_c
        .publish_intents(&group_c.mls_provider().unwrap())
        .await
        .unwrap();

    // Sync the groups
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    // After client a adds b and c, and they each sent a message, all groups are in the same epoch
    assert_eq!(
        group_a
            .epoch(&client_a1.mls_provider().unwrap())
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        group_b
            .epoch(&client_b.mls_provider().unwrap())
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        group_c
            .epoch(&client_c.mls_provider().unwrap())
            .await
            .unwrap(),
        3
    );

    // Client b updates the group name, (incrementing the epoch from 3 to 4), and syncs
    group_b
        .update_group_name("Group B".to_string())
        .await
        .unwrap();
    group_b.sync().await.unwrap();

    // Client c sends two text messages before incrementing the epoch
    group_c
        .send_message_optimistic("Message c2".as_bytes())
        .unwrap();
    group_c
        .publish_intents(&group_c.mls_provider().unwrap())
        .await
        .unwrap();
    group_b.sync().await.unwrap();

    // Retrieve all messages from group B, verify they contain the two messages from client c even though they were sent from the wrong epoch
    let messages = client_b
        .api_client
        .query_group_messages(group_b.group_id.clone(), None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 8);

    // Get reference to last message
    let last_message = messages.last().unwrap();

    // Simulating group_a streaming out of order by processing the last_message first
    let v1_last_message = match &last_message.version {
        Some(xmtp_proto::xmtp::mls::api::v1::group_message::Version::V1(v1)) => v1,
        _ => panic!("Expected V1 message"),
    };

    // This is the key line, because we pass in false for incrementing epoch/cursor (simulating streaming)
    // This processing will not longer update the cursor, so we will not be forked
    let increment_epoch = false;
    let result = group_a
        .process_message(
            &client_a1.mls_provider().unwrap(),
            v1_last_message,
            increment_epoch,
        )
        .await;
    assert!(result.is_ok());

    // Now syncing a will update group_a group name since the cursor has NOT moved on past it
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    assert_eq!(
        group_b
            .epoch(&client_b.mls_provider().unwrap())
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        group_c
            .epoch(&client_c.mls_provider().unwrap())
            .await
            .unwrap(),
        4
    );
    // We pass on the last line because a's cursor has not moved past any commits, even though it processed
    // messages out of order
    assert_eq!(
        group_a
            .epoch(&client_a1.mls_provider().unwrap())
            .await
            .unwrap(),
        4
    );
}
