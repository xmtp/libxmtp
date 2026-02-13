mod test_commit_log_fork_detection;
mod test_commit_log_local;
mod test_commit_log_readd_requests;
mod test_commit_log_remote;
mod test_consent;
mod test_delete_message;
mod test_dm;
mod test_extract_readded_installations;
mod test_group_updated;
mod test_libxmtp_version;
mod test_message_disappearing_settings;
#[cfg(not(target_arch = "wasm32"))]
mod test_network;
mod test_prepare_message_for_later_publish;
mod test_proposals;
mod test_send_message_opts;
mod test_welcome_pointers;
mod test_welcomes;

xmtp_common::if_d14n! {
    mod test_message_dependencies;
}

use crate::groups::send_message_opts::SendMessageOpts;
use chrono::DateTime;
use openmls::prelude::MlsMessageIn;
use prost::Message;
use tls_codec::Deserialize;
use xmtp_api_d14n::protocol::XmtpQuery;
use xmtp_configuration::Originators;
use xmtp_db::XmtpOpenMlsProviderRef;
use xmtp_db::refresh_state::EntityKind;
use xmtp_proto::types::{Cursor, TopicKind};

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use super::group_permissions::PolicySet;
use crate::context::XmtpSharedContext;
use crate::groups::intents::QueueIntent;
use crate::groups::{DmValidationError, GroupLeaveValidationError, MetadataPermissionsError};
use crate::groups::{
    MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
    MAX_GROUP_NAME_LENGTH,
};
use crate::tester;
use crate::utils::fixtures::{alix, bola, caro};
use crate::utils::{ClientTester, LocalTester, TestMlsGroup, Tester, VersionInfo};
use crate::{
    builder::ClientBuilder,
    groups::{
        DeliveryStatus, GroupError, GroupMetadataOptions, PreconfiguredPolicies,
        UpdateAdminListType, build_dm_protected_metadata_extension,
        build_mutable_metadata_extension_default, build_protected_metadata_extension,
        intents::{PermissionPolicyOption, PermissionUpdateType},
        members::{GroupMember, PermissionLevel},
        mls_sync::GroupMessageProcessingError,
        validate_dm_group,
    },
    utils::test::FullXmtpClient,
};
use diesel::connection::SimpleConnection;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use futures::future::join_all;
use rstest::*;
use std::sync::Arc;
use wasm_bindgen_test::wasm_bindgen_test;
use xmtp_common::RetryableError;
use xmtp_common::StreamHandle as _;
use xmtp_common::time::now_ns;
use xmtp_common::{assert_err, assert_ok};
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group::{GroupMembershipState, StoredGroup};
use xmtp_db::pending_remove::QueryPendingRemove;
use xmtp_db::schema::groups;
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, GroupQueryArgs},
    group_intent::IntentState,
    group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
    prelude::*,
};
use xmtp_id::associations::Identifier;
use xmtp_id::associations::test_utils::WalletTestExt;
use xmtp_mls_common::group_metadata::GroupMetadata;
use xmtp_mls_common::group_mutable_metadata::{MessageDisappearingSettings, MetadataField};
use xmtp_proto::xmtp::mls::message_contents::{EncodedContent, PlaintextEnvelope};

async fn receive_group_invite(client: &FullXmtpClient) -> TestMlsGroup {
    client.sync_welcomes().await.unwrap();
    let mut groups = client.find_groups(GroupQueryArgs::default()).unwrap();

    groups.remove(0)
}

async fn get_latest_message(group: &TestMlsGroup) -> StoredGroupMessage {
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
    sender_group: &TestMlsGroup,
    sender_mls_group: &mut openmls::prelude::MlsGroup,
    sender_provider: &impl xmtp_db::MlsProviderExt,
) {
    use crate::groups::mls_ext::{WelcomePointersExtension, WrapperAlgorithm};
    use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;

    use super::intents::{Installation, SendWelcomesAction};
    use openmls::prelude::tls_codec::Serialize;
    let new_member_provider = new_member_client.context.mls_provider();

    let key_package_result = new_member_client
        .identity()
        .new_key_package(&new_member_provider, CREATE_PQ_KEY_PACKAGE_EXTENSION)
        .unwrap();
    let hpke_init_key = key_package_result
        .key_package
        .hpke_init_key()
        .as_slice()
        .to_vec();
    let (commit, welcome, _) = sender_mls_group
        .add_members(
            sender_provider,
            &sender_client.identity().installation_keys,
            &[key_package_result.key_package],
        )
        .unwrap();
    let serialized_commit = commit.tls_serialize_detached().unwrap();
    let serialized_welcome = welcome.tls_serialize_detached().unwrap();
    let send_welcomes_action = SendWelcomesAction::new(
        vec![Installation {
            installation_key: new_member_client.installation_public_key().into(),
            hpke_public_key: hpke_init_key,
            welcome_wrapper_algorithm: WrapperAlgorithm::Curve25519,
            welcome_pointee_encryption_aead_types: WelcomePointersExtension::empty(),
        }],
        serialized_welcome,
    );
    let messages = sender_group
        .prepare_group_messages(vec![(serialized_commit.as_slice(), false)])
        .unwrap();
    sender_client
        .context
        .api()
        .send_group_messages(messages)
        .await
        .unwrap();
    sender_group
        .send_welcomes(send_welcomes_action, None)
        .await
        .unwrap();
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_send_message() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;
    let messages = alix
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group.group_id), None)
        .await?;

    group.sync().await?;
    let decrypted_messages = group.find_messages(&MsgQueryArgs::default())?;

    tracing::info!("The messages: {decrypted_messages:?}");

    // KP update and the msg itself
    assert_eq!(messages.len(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receive_self_message() {
    tester!(alix);
    let group = alix.create_group(None, None).expect("create group");
    let msg = b"hello";

    group
        .send_message(msg, SendMessageOpts::default())
        .await
        .expect("send message");

    group.receive().await?;
    // Check for messages
    let messages = group.find_messages(&MsgQueryArgs::default())?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages.first().unwrap().decrypted_message_bytes, msg);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receive_message_from_other() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let alix_group = alix.create_group(None, None).expect("create group");
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    let alix_message = b"hello from alix";
    alix_group
        .send_message(alix_message, SendMessageOpts::default())
        .await
        .expect("send message");

    let bo_group = receive_group_invite(&bo).await;
    let message = get_latest_message(&bo_group).await;
    assert_eq!(message.decrypted_message_bytes, alix_message);

    let bo_message = b"hello from bo";
    bo_group
        .send_message(bo_message, SendMessageOpts::default())
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

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Get bola's version of the same group
    let bola_groups = bola.sync_welcomes().await.unwrap();
    let bola_group = bola_groups.first().unwrap();

    // Call sync for both
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify bola can see the group name
    let bola_group_name = bola_group.group_name().unwrap();
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

    let amal_group = amal.create_group(None, None).unwrap();
    // Add bola
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Get bola's version of the same group
    let bola_groups = bola.sync_welcomes().await.unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    tracing::info!("Adding charlie from amal");
    // Have amal and bola both invite charlie.
    amal_group
        .add_members(&[charlie.inbox_id()])
        .await
        .expect("failed to add charlie");
    tracing::info!("Adding charlie from bola");
    bola_group
        .add_members(&[charlie.inbox_id()])
        .await
        .expect("bola's add should succeed in a no-op");

    let summary = amal_group.receive().await.unwrap();
    assert!(summary.is_errored());

    // Check Amal's MLS group state.
    let amal_db = amal.context.db();
    let amal_members_len = amal_group
        .load_mls_group_with_lock(amal.context.mls_storage(), |mls_group| {
            Ok(mls_group.members().count())
        })
        .unwrap();

    assert_eq!(amal_members_len, 3);

    // Check Bola's MLS group state.
    let bola_db = bola.context.db();
    let bola_members_len = bola_group
        .load_mls_group_with_lock(amal.context.mls_storage(), |mls_group| {
            Ok(mls_group.members().count())
        })
        .unwrap();

    assert_eq!(bola_members_len, 3);

    let amal_uncommitted_intents = amal_db
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
        .send_message("hello from amal".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    bola_group
        .send_message("hello from bola".as_bytes(), SendMessageOpts::default())
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

        let alix_group = alix.create_group(None, None).unwrap();
        let provider = alix.context.mls_provider();
        // Doctor the group membership
        let mut mls_group = alix_group
            .load_mls_group_with_lock(alix.context.mls_storage(), |mut mls_group| {
                let mut existing_extensions = mls_group.extensions().clone();
                let mut group_membership = GroupMembership::new();
                group_membership.add("deadbeef".to_string(), 1);
                existing_extensions
                    .add_or_replace(build_group_membership_extension(&group_membership))
                    .unwrap();

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
        bo.sync_welcomes().await.unwrap();
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
        .find_or_create_dm(alix.inbox_id().to_string(), None)
        .await
        .unwrap();
    let alix_dm = alix
        .find_or_create_dm(bo.inbox_id().to_string(), None)
        .await
        .unwrap();

    bo_dm
        .send_message(b"Hello there", SendMessageOpts::default())
        .await
        .unwrap();
    alix_dm
        .send_message(b"No, let's use this dm", SendMessageOpts::default())
        .await
        .unwrap();

    alix.sync_all_welcomes_and_groups(None).await.unwrap();

    // The dm shows up
    let alix_groups = alix
        .context
        .db()
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
        .context
        .db()
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
    let group = client.create_group(None, None).expect("create group");

    group.add_members(&[client_2.inbox_id()]).await.unwrap();

    let group_id = group.group_id;

    let messages = client
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group_id), None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "current_thread")]
async fn test_create_group_with_member_two_installations_one_malformed_keypackage() {
    use xmtp_id::associations::test_utils::WalletTestExt;

    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
    // 1) Prepare clients
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();

    // bola has two installations
    let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

    // 2) Mark the second installation as malformed
    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bola_2.context.installation_id().to_vec()]),
    );

    // 3) Create the group, inviting bola (which internally includes bola_1 and bola_2)
    let group = alix
        .create_group_with_identifiers(&[bola_wallet.identifier()], None, None)
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
        .query_at(TopicKind::GroupMessagesV1.create(&group.group_id), None)
        .await
        .unwrap();

    // The last message should be our "Hello from Alix"
    assert_eq!(messages_bola_1.len(), 3);

    // Query messages from Alix's perspective
    let messages_alix = alix
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group.group_id), None)
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

    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
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
            bola_1.context.installation_id().to_vec(),
            bola_2.context.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the group, which should fail
    let result = alix
        .create_group_with_identifiers(&[bola_wallet.identifier()], None, None)
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
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = generate_local_wallet();

    // Bola has two installations
    let bola_1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;

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
    use xmtp_id::associations::test_utils::WalletTestExt;

    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
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
            bola_1.context.installation_id().to_vec(),
            bola_2.context.installation_id().to_vec(),
        ]),
    );

    // 3) Attempt to create the DM group, which should fail

    let result = amal
        .find_or_create_dm_by_identity(bola_wallet.identifier(), None)
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
    use xmtp_id::associations::test_utils::WalletTestExt;

    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let bo_1 = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_2 = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[caro_wallet.identifier()], None, None)
        .await
        .unwrap();

    let _ = group
        .add_members_by_identity(&[bo_wallet.identifier()])
        .await;

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
    use xmtp_id::associations::test_utils::WalletTestExt;

    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo_1 = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_2 = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(&[bo_wallet.identifier()], None, None)
        .await
        .unwrap();

    let _ = group
        .add_members_by_identity(&[caro_wallet.identifier()])
        .await;

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
    use xmtp_id::associations::test_utils::WalletTestExt;

    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let alix_1 = ClientBuilder::new_test_client(&alix_wallet).await;
    let alix_2 = ClientBuilder::new_test_client(&alix_wallet).await;
    let bo = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![alix_2.context.installation_id().to_vec()]),
    );

    let group = alix_1
        .create_group_with_identifiers(
            &[bo_wallet.identifier(), caro_wallet.identifier()],
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(group.members().await.unwrap().len(), 3);
    let _ = group
        .remove_members_by_identity(&[caro_wallet.identifier()])
        .await;

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
    use xmtp_id::associations::test_utils::WalletTestExt;

    let alix_wallet = generate_local_wallet();
    let bo_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();
    let alix = ClientBuilder::new_test_client(&alix_wallet).await;
    let bo_1 = ClientBuilder::new_test_client(&bo_wallet).await;
    let bo_2 = ClientBuilder::new_test_client(&bo_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    set_test_mode_upload_malformed_keypackage(
        true,
        Some(vec![bo_1.context.installation_id().to_vec()]),
    );

    let group = alix
        .create_group_with_identifiers(
            &[bo_wallet.identifier(), caro_wallet.identifier()],
            None,
            None,
        )
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
        .remove_members_by_identity(&[bo_wallet.identifier()])
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

#[xmtp_common::test]
async fn test_add_invalid_member() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group = client.create_group(None, None).expect("create group");

    let result = group.add_members(&["1234".to_string()]).await;

    assert!(result.is_err());
}

#[xmtp_common::test]
async fn test_add_unregistered_member() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let unconnected_ident = Identifier::rand_ethereum();
    let group = amal.create_group(None, None).unwrap();
    let result = group.add_members_by_identity(&[unconnected_ident]).await;

    assert!(result.is_err());
}

#[xmtp_common::test]
async fn test_remove_inbox() {
    let client_1 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    // Add another client onto the network
    let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let group = client_1.create_group(None, None).expect("create group");
    group
        .add_members(&[client_2.inbox_id()])
        .await
        .expect("group create failure");

    let messages_with_add = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_with_add.len(), 1);

    // Try and add another member without merging the pending commit
    group
        .remove_members(&[client_2.inbox_id()])
        .await
        .expect("group remove members failure");

    let messages_with_remove = group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages_with_remove.len(), 2);

    // We are expecting 1 message on the group topic, not 2, because the second one should have
    // failed
    let group_id = group.group_id;
    let messages = client_1
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group_id), None)
        .await
        .expect("read topic");

    assert_eq!(messages.len(), 2);
}

#[xmtp_common::test]
async fn test_self_remove_dm_must_fail() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a dm group with bola
    let amal_dm = amal
        .find_or_create_dm(bola.inbox_id().to_string(), None)
        .await
        .unwrap();
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Bola can message amal
    let _ = bola.sync_welcomes().await;
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_dm = bola_groups.first().unwrap();
    bola_dm
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();

    // Amal syncs and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");

    // Amal cannot remove bola
    let result = amal_dm.remove_members(&[bola.inbox_id()]).await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Neither Amal nor Bola is an admin or super admin
    amal_dm.sync().await.unwrap();
    bola_dm.sync().await.unwrap();
    let is_amal_admin = amal_dm.is_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_admin = amal_dm.is_admin(bola.inbox_id().to_string()).unwrap();
    let is_amal_super_admin = amal_dm.is_super_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_super_admin = amal_dm.is_super_admin(bola.inbox_id().to_string()).unwrap();
    assert!(!is_amal_admin);
    assert!(!is_bola_admin);
    assert!(!is_amal_super_admin);
    assert!(!is_bola_super_admin);

    // Neither Amal nor Bola can leave the DM
    assert_err!(
        amal_dm.leave_group().await,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::DmLeaveForbidden)
    );
    assert_err!(
        bola_dm.leave_group().await,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::DmLeaveForbidden)
    );

    bola_dm
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();

    // Amal syncs and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");
}
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_remove_group_fail_with_one_member() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group and verify it has the default group name
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.sync().await.unwrap();

    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    let result = amal_group.leave_group().await;
    assert_err!(
        result,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::SingleMemberLeaveRejected)
    );
}
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_remove_super_admin_must_fail() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    let result = amal_group.leave_group().await;
    assert_err!(
        result,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::SuperAdminLeaveForbidden)
    );
}
#[xmtp_common::test(flavor = "current_thread")]
async fn test_non_member_cannot_leave_group() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group and verify it has the default group name
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    // Verify the pending-remove list is empty on Amal's group
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    bola_group.sync().await.unwrap();

    // Verify the pending-remove list is empty on Bola_i1's group
    let bola_group_pending_leave_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(bola_group_pending_leave_users.is_empty());

    amal_group.remove_members(&[bola.inbox_id()]).await.unwrap();
    bola_group.sync().await.unwrap();
    let bola_not_member_leave_result = bola_group.leave_group().await;
    assert_err!(
        bola_not_member_leave_result,
        GroupError::LeaveCantProcessed(GroupLeaveValidationError::NotAGroupMember)
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal() {
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola_i1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_i2 = ClientBuilder::new_test_client(&bola_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    // Verify the pending-remove list is empty on Amal's group
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();
    assert_eq!(bola_i1_group.members().await.unwrap().len(), 2);

    bola_i1_group.sync().await.unwrap();

    // Verify the pending-remove list is empty on Bola_i1's group
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();
    assert!(bola_i1_group_pending_leave_users.is_empty());

    // Verify Amal's as the super admin/admin can't leave the group and their inboxId is not added to the pendingRemoveList
    amal_group
        .leave_group()
        .await
        .expect_err("Amal should not be able to leave the group");
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    // Amal's inboxId shouldn't be in the pending-remove list
    assert!(!amal_group_pending_leave_users.contains(&amal.inbox_id().to_string()));

    // Bola_i1 should be able to leave the group
    bola_i1_group.sync().await.unwrap();
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();
    tracing::info!(
        "Bola_i1_group_pending_leave_users: {:?}",
        bola_i1_group_pending_leave_users
    );
    // Bola's inboxId should be in the pending-remove list on Bola's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    assert_eq!(bola_i1_group_pending_leave_users.len(), 1);

    // Bola's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Amal's state for the group should not change
    amal_group.sync().await.unwrap();
    let amal_group_member_state = amal
        .db()
        .find_group(&amal_group.group_id)
        .unwrap()
        .unwrap()
        .membership_state;
    assert_eq!(amal_group_member_state, GroupMembershipState::Allowed);

    // Check Bola's other installations
    bola_i2.sync_welcomes().await.unwrap();
    let bola_i2_groups = bola_i2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i2_groups.len(), 1);
    let bola_i2_group = bola_i2_groups.first().unwrap();
    assert_eq!(bola_i2_group.members().await.unwrap().len(), 2);
    bola_i2_group.sync().await.unwrap();

    let bola_i2_group_pending_leave_users = bola_i2
        .db()
        .get_pending_remove_users(&bola_i2_group.group_id)
        .unwrap();
    // Bola's inboxId should be in the pending-remove list
    assert!(bola_i2_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    // The pending-remove list should only contain one item
    assert_eq!(bola_i2_group_pending_leave_users.len(), 1);
    let bola_i2_group_state_in_db = bola_i2.db().find_group(&bola_i2_group.group_id).unwrap();

    // group's state should be set to PendingRemove on Bola's other installation
    assert_eq!(
        bola_i2_group_state_in_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    let _ = bola_i1_group.sync().await;
    let _ = bola_i2_group.sync().await;
    assert!(!bola_i1_group.is_active().unwrap());
    assert!(!bola_i2_group.is_active().unwrap());
    let _ = amal_group.sync().await;
    assert_eq!(amal_group.members().await.unwrap().len(), 1);
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_simple() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    // Verify Bola's membership state is Pending when first invited
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending
    );

    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove
    );

    amal_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    bola_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!bola_group.is_active().unwrap());
    assert_eq!(amal_group.members().await.unwrap().len(), 1);

    // Verify Amal's membership state remains Allowed
    assert_eq!(
        amal_group.membership_state().unwrap(),
        GroupMembershipState::Allowed
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_membership_state_after_readd() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a group and adds Bola
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Bola syncs and gets the group
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();

    // Verify Bola's initial membership state is Pending
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending,
        "Bola should be in Pending state when first invited"
    );

    // Bola leaves the group
    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove,
        "Bola should be in PendingRemove state after leaving"
    );

    // Amal syncs to process the leave request
    amal_group.sync().await.unwrap();

    // Wait for admin worker to process the removal
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    // Bola syncs to get the final removal
    bola_group.sync().await.unwrap();

    // Verify Bola's group is no longer active
    assert!(
        !bola_group.is_active().unwrap(),
        "Bola's group should be inactive after removal"
    );

    // Amal re-adds Bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Amal syncs to send the add
    amal_group.sync().await.unwrap();

    // Bola syncs to receive the welcome message for being re-added
    bola.sync_welcomes().await.unwrap();

    // Bola should have the group again (same ID)
    let bola_groups_after_readd = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group_after_readd = bola_groups_after_readd
        .iter()
        .find(|g| g.group_id == bola_group.group_id)
        .expect("Bola should have the group after being re-added");

    // CRITICAL: Verify Bola's membership state is Allowed (not PendingRemove)
    assert_eq!(
        bola_group_after_readd.membership_state().unwrap(),
        GroupMembershipState::Allowed,
        "Bola should be in Allowed state after being re-added, not PendingRemove"
    );

    // Verify the group is active again
    assert!(
        bola_group_after_readd.is_active().unwrap(),
        "Bola's group should be active after re-add"
    );

    // Verify consent state is Unknown (user needs to accept)
    assert_eq!(
        bola_group_after_readd.consent_state().unwrap(),
        ConsentState::Unknown,
        "Bola's consent should be Unknown after re-add, requiring explicit acceptance"
    );

    // Verify both members are back in the group
    amal_group.sync().await.unwrap();
    let members_after_readd = amal_group.members().await.unwrap();
    assert_eq!(
        members_after_readd.len(),
        2,
        "Both Amal and Bola should be in the group"
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_group_update_message() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    bola_group.leave_group().await.unwrap();
    amal_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    bola_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!bola_group.is_active().unwrap());
    assert_eq!(amal_group.members().await.unwrap().len(), 1);
    amal_group.sync().await.unwrap();
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
    tracing::info!("{:?}", messages.len());
    let message = messages[2].clone();
    assert_eq!(message.kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 0);
    assert_eq!(group_update.removed_inboxes.len(), 0);
    assert_eq!(group_update.left_inboxes.len(), 1);
    assert_eq!(
        group_update.left_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string()
    );
}
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_single_installations() {
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    // Verify the pending-remove list is empty on Amal's group
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    bola_group.sync().await.unwrap();

    // Verify the pending-remove list is empty on Bola's group
    let bola_group_pending_leave_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(bola_group_pending_leave_users.is_empty());

    // Verify Amal as the super admin/admin can't leave the group and their inboxId is not added to the pendingRemoveList
    amal_group
        .leave_group()
        .await
        .expect_err("Amal should not be able to leave the group");
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    // Amal's inboxId shouldn't be in the pending-remove list
    assert!(!amal_group_pending_leave_users.contains(&amal.inbox_id().to_string()));
    // The pending-remove list should be empty
    assert!(amal_group_pending_leave_users.is_empty());

    // Bola should be able to leave the group
    bola_group.sync().await.unwrap();

    // Verify Bola's membership state is Pending when first invited
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending
    );

    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove
    );

    let bola_group_pending_leave_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on Bola's group
    assert!(bola_group_pending_leave_users.contains(&bola.inbox_id().to_string()));

    // Bola's state for the group should be set to PendingRemove
    let bola_group_from_db = bola.db().find_group(&bola_group.group_id).unwrap();
    assert_eq!(
        bola_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Amal's state for the group should not change
    amal_group.sync().await.unwrap();
    let amal_group_member_state = amal
        .db()
        .find_group(&amal_group.group_id)
        .unwrap()
        .unwrap()
        .membership_state;

    assert_eq!(amal_group_member_state, GroupMembershipState::Allowed);
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_with_multiple_initial_installations() {
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola_i1 = ClientBuilder::new_test_client(&bola_wallet).await;
    let bola_i2 = ClientBuilder::new_test_client(&bola_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();
    bola_i2.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();

    let bola_i2_groups = bola_i2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i2_groups.len(), 1);
    let bola_i2_group = bola_i2_groups.first().unwrap();

    bola_i1_group.sync().await.unwrap();
    bola_i2_group.sync().await.unwrap();

    // Bola_i1 leaves the group
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on Bola_i1's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));

    // Bola_i1's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Check Bola's other installation (i2)
    bola_i2_group.sync().await.unwrap();
    let bola_i2_group_pending_leave_users = bola_i2
        .db()
        .get_pending_remove_users(&bola_i2_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on i2 as well
    assert!(bola_i2_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    // The pending-remove list should only contain one item
    assert_eq!(bola_i2_group_pending_leave_users.len(), 1);

    let bola_i2_group_state_in_db = bola_i2.db().find_group(&bola_i2_group.group_id).unwrap();
    // Group's state should be set to PendingRemove on Bola's other installation
    assert_eq!(
        bola_i2_group_state_in_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );
}

#[xmtp_common::test(flavor = "current_thread")]
#[ignore] // fix after consent sync
async fn test_self_removal_with_late_installation() {
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola_i1 = ClientBuilder::new_test_client(&bola_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();

    bola_i1_group.sync().await.unwrap();

    // Bola_i1 leaves the group
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on Bola_i1's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));

    // Bola_i1's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Introduce another installation for Bola after the self-removal
    let bola_i3 = ClientBuilder::new_test_client(&bola_wallet).await;
    xmtp_common::time::sleep(std::time::Duration::from_secs(5)).await;
    bola_i1_group
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(5)).await;

    // New installation processes the welcome
    bola_i3.sync_welcomes().await.unwrap();
    let bola_i3_groups = bola_i3.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i3_groups.len(), 1);
    let bola_i3_group = bola_i3_groups.first().unwrap();
    assert_eq!(bola_i3_group.members().await.unwrap().len(), 2);

    bola_i3_group.sync().await.unwrap();

    let bola_i3_group_pending_leave_users = bola_i3
        .db()
        .get_pending_remove_users(&bola_i3_group.group_id)
        .unwrap();

    // Bola's inboxId should be in the pending-remove list on the new installation
    assert!(bola_i3_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    // The pending-remove list should only contain one item
    assert_eq!(bola_i3_group_pending_leave_users.len(), 1);

    let bola_i3_group_state_in_db = bola_i3.db().find_group(&bola_i1_group.group_id).unwrap();
    // Group's state should be set to PendingRemove on the new installation
    assert_eq!(
        bola_i3_group_state_in_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_clean_pending_remove_list_on_member_removal() {
    // Test that when a member is removed from the group, they are also removed from the pending_remove list
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();

    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Bola requests to leave the group
    bola_group.leave_group().await.unwrap();

    // Verify Bola is in the pending_remove list
    let pending_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert_eq!(pending_users.len(), 1);
    assert!(pending_users.contains(&bola.inbox_id().to_string()));

    // Amal removes Bola from the group
    amal_group.sync().await.unwrap();
    amal_group
        .remove_members_by_identity(&[bola_wallet.identifier()])
        .await
        .unwrap();

    // Sync on all clients
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    caro_group.sync().await.unwrap();

    // Verify Bola is removed from the pending_remove list on all clients
    let amal_pending = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_pending.is_empty());

    let caro_pending = caro
        .db()
        .get_pending_remove_users(&caro_group.group_id)
        .unwrap();
    assert!(caro_pending.is_empty());

    // Verify the group members
    assert_eq!(amal_group.members().await.unwrap().len(), 2); // amal and caro

    // Verify the GroupUpdated message correctly classifies Bola as "left" (not "removed")
    // since they were in the pending_remove list
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();

    // Find all membership change messages
    let membership_messages: Vec<_> = messages
        .iter()
        .filter(|m| m.kind == GroupMessageKind::MembershipChange)
        .collect();

    // Get the last membership change message (should be Bola's removal)
    let removal_message = membership_messages
        .last()
        .expect("Should find membership change message");

    let encoded_content =
        EncodedContent::decode(removal_message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();

    // Bola should be in left_inboxes (not removed_inboxes) because they were in pending_remove
    assert_eq!(
        group_update.left_inboxes.len(),
        1,
        "Should have 1 left inbox"
    );
    assert_eq!(
        group_update.left_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string(),
        "Bola should be in left_inboxes"
    );
    assert_eq!(
        group_update.removed_inboxes.len(),
        0,
        "Should have 0 removed inboxes"
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_super_admin_promotion_marks_pending_leave_requests() {
    // Test that when a user is promoted to super_admin and there are pending remove users,
    // the group is marked as having pending leave requests
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();

    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Caro requests to leave the group
    caro_group.leave_group().await.unwrap();

    // Verify Caro is in the pending_remove list
    let pending_users = caro
        .db()
        .get_pending_remove_users(&caro_group.group_id)
        .unwrap();
    assert_eq!(pending_users.len(), 1);
    assert!(pending_users.contains(&caro.inbox_id().to_string()));

    // Initially, the group should not have pending leave requests on Bola's side (not super admin)
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, None);

    // Amal promotes Bola to super_admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Bola syncs and should now be marked as having pending leave requests
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the group is marked as having pending leave requests
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, Some(true));
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_super_admin_demotion_clears_pending_leave_requests() {
    // Test that when a user is demoted from super_admin, the pending leave request flag is cleared
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();

    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Amal promotes Bola to super_admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Caro requests to leave
    caro_group.leave_group().await.unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify the group is marked as having pending leave requests on Bola's side
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, Some(true));

    // Bola demotes themselves from super_admin
    bola_group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            bola.inbox_id().to_string(),
        )
        .await
        .unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is no longer a super_admin
    assert!(
        !bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the pending leave request flag is cleared
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert_eq!(bola_group_status.has_pending_leave_request, Some(false));
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_no_status_change_when_not_in_pending_remove_list() {
    // Test that promotion to super_admin doesn't mark the group when there are no pending remove users
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();

    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify no pending remove users
    let pending_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(pending_users.is_empty());

    // Amal promotes Bola to super_admin
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the group is NOT marked as having pending leave requests (no pending users)
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    // The status should be false or None since there are no pending remove users
    assert!(
        bola_group_status.has_pending_leave_request == Some(false)
            || bola_group_status.has_pending_leave_request.is_none()
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_promotion_excludes_self_from_pending_check() {
    // Test that if the promoted user is in the pending_remove list, the group is NOT marked
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();

    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Bola requests to leave
    bola_group.leave_group().await.unwrap();

    // Verify Bola is in the pending_remove list
    let pending_users = bola
        .db()
        .get_pending_remove_users(&bola_group.group_id)
        .unwrap();
    assert!(pending_users.contains(&bola.inbox_id().to_string()));

    // Amal promotes Bola to super_admin (edge case)
    amal_group
        .update_admin_list(UpdateAdminListType::AddSuper, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();

    // Verify Bola is a super_admin
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify the group is NOT marked as having pending leave requests
    // (because the only pending user is Bola themselves)
    let bola_group_status = bola.db().find_group(&bola_group.group_id).unwrap().unwrap();
    assert!(
        bola_group_status.has_pending_leave_request == Some(false)
            || bola_group_status.has_pending_leave_request.is_none()
    );
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_admin_removal_without_pending_shows_as_removed() {
    // Test that when an admin removes a member who is NOT in pending_remove,
    // they appear in removed_inboxes (not left_inboxes)
    let amal_wallet = generate_local_wallet();
    let bola_wallet = generate_local_wallet();
    let caro_wallet = generate_local_wallet();

    let amal = ClientBuilder::new_test_client(&amal_wallet).await;
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
    let caro = ClientBuilder::new_test_client(&caro_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await
        .unwrap();

    amal_group.sync().await.unwrap();
    bola.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();
    caro_group.sync().await.unwrap();

    // Verify Bola is NOT in the pending_remove list
    let pending_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(pending_users.is_empty());

    // Amal removes Bola from the group (admin removal, not self-removal)
    amal_group
        .remove_members_by_identity(&[bola_wallet.identifier()])
        .await
        .unwrap();

    // Sync on all clients
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    caro_group.sync().await.unwrap();

    // Verify the GroupUpdated message correctly classifies Bola as "removed" (not "left")
    // since they were NOT in the pending_remove list
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();

    // Find all membership change messages
    let membership_messages: Vec<_> = messages
        .iter()
        .filter(|m| m.kind == GroupMessageKind::MembershipChange)
        .collect();

    // Get the LAST membership change message (should be Bola's removal, not the addition)
    let removal_message = membership_messages
        .last()
        .expect("Should find membership change message");

    let encoded_content =
        EncodedContent::decode(removal_message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();

    // Bola should be in removed_inboxes (not left_inboxes) because they were NOT in pending_remove
    assert_eq!(
        group_update.removed_inboxes.len(),
        1,
        "Should have 1 removed inbox"
    );
    assert_eq!(
        group_update.removed_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string(),
        "Bola should be in removed_inboxes"
    );
    assert_eq!(
        group_update.left_inboxes.len(),
        0,
        "Should have 0 left inboxes"
    );
}

#[xmtp_common::test]
async fn test_key_update() {
    tester!(client);
    tester!(bola_client);

    let group = client.create_group(None, None).expect("create group");
    group.add_members(&[bola_client.inbox_id()]).await.unwrap();

    group.key_update().await.unwrap();

    let messages = client
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group.group_id), None)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);

    let pending_commit_is_none = group
        .load_mls_group_with_lock(client.context.mls_storage(), |mls_group| {
            Ok(mls_group.pending_commit().is_none())
        })
        .unwrap();

    assert!(pending_commit_is_none);

    group
        .send_message(b"hello", SendMessageOpts::default())
        .await
        .expect("send message");

    bola_client.sync_welcomes().await.unwrap();
    let bola_groups = bola_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let bola_messages = bola_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(bola_messages.len(), 2);
}

#[xmtp_common::test]
async fn test_post_commit() {
    let client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let client_2 = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let group = client.create_group(None, None).expect("create group");

    group.add_members(&[client_2.inbox_id()]).await.unwrap();

    // Check if the welcome was actually sent
    let welcome_messages = client
        .context
        .api()
        .query_at(
            TopicKind::WelcomeMessagesV1.create(client_2.installation_public_key()),
            None,
        )
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

    let group = amal.create_group(None, None).unwrap();
    group
        .add_members_by_identity(&[bola_wallet.identifier(), charlie_wallet.identifier()])
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
    assert_eq!(group_update.left_inboxes.len(), 0);

    group
        .remove_members_by_identity(&[bola_wallet.identifier()])
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
    assert_eq!(group_update.left_inboxes.len(), 0);

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    assert!(!bola_group.is_active().unwrap())
}

#[xmtp_common::test]
async fn test_removed_members_cannot_send_message_to_others() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola_wallet = &generate_local_wallet();
    let bola = ClientBuilder::new_test_client(bola_wallet).await;
    let charlie_wallet = &generate_local_wallet();
    let charlie = ClientBuilder::new_test_client(charlie_wallet).await;

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members_by_identity(&[bola_wallet.identifier(), charlie_wallet.identifier()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 3);

    amal_group
        .remove_members_by_identity(&[bola_wallet.identifier()])
        .await
        .unwrap();
    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    assert!(
        amal_group
            .members()
            .await
            .unwrap()
            .iter()
            .all(|m| m.inbox_id != bola.inbox_id())
    );
    assert!(
        amal_group
            .members()
            .await
            .unwrap()
            .iter()
            .any(|m| m.inbox_id == charlie.inbox_id())
    );

    amal_group.sync().await.expect("sync failed");

    let message_text = b"hello";

    let bola_group = TestMlsGroup::new(
        bola.context.clone(),
        amal_group.group_id.clone(),
        amal_group.dm_id.clone(),
        amal_group.conversation_type,
        amal_group.created_at_ns,
    );
    bola_group
        .send_message(message_text, SendMessageOpts::default())
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

    let group = amal.create_group(None, None).unwrap();
    group.add_members(&[bola.inbox_id()]).await.unwrap();

    assert_eq!(group.members().await.unwrap().len(), 2);
    // Finished with setup

    // add a second installation for amal using the same wallet
    let _amal_2nd = ClientBuilder::new_test_client(&amal_wallet).await;

    // test if adding the new installation(s) worked
    let new_installations_were_added = group.add_missing_installations().await;
    assert!(new_installations_were_added.is_ok());

    group.sync().await.unwrap();
    let num_members = group
        .load_mls_group_with_lock(amal.context.mls_storage(), |mls_group| {
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
    let amal_group = amal.create_group(None, None).unwrap();
    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    // Both Amal and Bola are up to date on the group state. Now each of them want to add someone else
    amal_group.add_members(&[charlie.inbox_id()]).await.unwrap();

    bola_group.add_members(&[dave.inbox_id()]).await.unwrap();

    // Send a message to the group, now that everyone is invited
    amal_group.sync().await.unwrap();
    amal_group
        .send_message(b"hello", SendMessageOpts::default())
        .await
        .unwrap();

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
            None,
        )
        .unwrap();
    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    let bola_group = receive_group_invite(&bola).await;
    bola_group.sync().await.unwrap();
    assert!(bola_group.add_members(&[charlie.inbox_id()]).await.is_err(),);
}

#[xmtp_common::test]
async fn test_group_options() {
    let expected_group_message_disappearing_settings = MessageDisappearingSettings::new(100, 200);

    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let amal_group = amal
        .create_group(
            None,
            Some(GroupMetadataOptions {
                name: Some("Group Name".to_string()),
                image_url_square: Some("url".to_string()),
                description: Some("group description".to_string()),
                message_disappearing_settings: Some(expected_group_message_disappearing_settings),
                app_data: None,
            }),
        )
        .unwrap();

    let binding = amal_group.mutable_metadata().expect("msg");
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
            None,
        )
        .unwrap();
    let mut clients = Vec::new();
    for _ in 0..249 {
        let wallet = generate_local_wallet();
        ClientBuilder::new_test_client(&wallet).await;
        clients.push(wallet.identifier());
    }
    amal_group.add_members_by_identity(&clients).await.unwrap();
    let bola_wallet = generate_local_wallet();
    ClientBuilder::new_test_client(&bola_wallet).await;
    assert!(
        amal_group
            .add_members(&[bola_wallet.get_inbox_id(0)])
            .await
            .is_err(),
    );
}

#[xmtp_common::test]
async fn test_group_mutable_data() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a group and verify it has the default group name
    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert!(group_mutable_metadata.attributes.len().eq(&5));
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
    );

    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();

    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
    );

    // Update group name
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    amal_group
        .send_message("hello".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
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
    let binding = bola_group.mutable_metadata().expect("msg");
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
        .create_group_with_identifiers(&[bola_wallet.identifier()], policy_set, None)
        .await
        .unwrap();

    // Verify we can update the group name without syncing first
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    // Verify the name is updated
    amal_group.sync().await.unwrap();
    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    let group_name_1 = group_mutable_metadata
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(group_name_1, "New Group Name 1");

    // Create a group with just amal
    let policy_set_2 = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group_2 = amal.create_group(policy_set_2, None).unwrap();

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
    let group_mutable_metadata = amal_group_2.mutable_metadata().unwrap();
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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupImageUrlSquare.to_string())
            .unwrap()
            .is_empty()
    );

    // Update group name
    amal_group
        .update_group_image_url_square("a url".to_string())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
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
    let binding = amal_group.mutable_metadata().expect("msg");
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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let group_mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
    );

    // Add bola to the group
    amal_group
        .add_members_by_identity(&[bola_wallet.identifier()])
        .await
        .unwrap();
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let group_mutable_metadata = bola_group.mutable_metadata().unwrap();
    assert!(
        group_mutable_metadata
            .attributes
            .get(&MetadataField::GroupName.to_string())
            .unwrap()
            .is_empty()
    );

    // Update group name
    amal_group
        .update_group_name("New Group Name 1".to_string())
        .await
        .unwrap();

    // Verify amal group sees update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().unwrap();
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "New Group Name 1");

    // Verify bola group sees update
    bola_group.sync().await.unwrap();
    let binding = bola_group.mutable_metadata().expect("msg");
    let bola_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(bola_group_name, "New Group Name 1");

    // Verify that bola CAN update the group name since everyone is admin for this group
    bola_group
        .update_group_name("New Group Name 2".to_string())
        .await
        .expect("non creator failed to update group name");

    // Verify amal group sees an update
    amal_group.sync().await.unwrap();
    let binding = amal_group.mutable_metadata().expect("msg");
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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Add bola to the group
    amal_group
        .add_members_by_identity(&[bola_wallet.identifier()])
        .await
        .unwrap();
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify Amal is the only admin and super admin
    let admin_list = amal_group.admin_list().unwrap();
    let super_admin_list = amal_group.super_admin_list().unwrap();
    assert_eq!(admin_list.len(), 0);
    assert_eq!(super_admin_list.len(), 1);
    assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

    // Verify that bola can not add caro because they are not an admin
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .add_members(&[caro.inbox_id()])
        .await
        .expect_err("expected err");

    // Add bola as an admin
    amal_group
        .update_admin_list(UpdateAdminListType::Add, bola.inbox_id().to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.admin_list().unwrap().len(), 1);
    assert!(
        bola_group
            .admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that bola can now add caro because they are an admin
    bola_group.add_members(&[caro.inbox_id()]).await.unwrap();

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
    assert_eq!(bola_group.admin_list().unwrap().len(), 0);
    assert!(
        !bola_group
            .admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that bola can not add charlie because they are not an admin
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .add_members(&[charlie.inbox_id()])
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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Add bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_groups.len(), 1);
    let bola_group = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();

    // Verify Amal is the only super admin
    let admin_list = amal_group.admin_list().unwrap();
    let super_admin_list = amal_group.super_admin_list().unwrap();
    assert_eq!(admin_list.len(), 0);
    assert_eq!(super_admin_list.len(), 1);
    assert!(super_admin_list.contains(&amal.inbox_id().to_string()));

    // Verify that bola can not add caro as an admin because they are not a super admin
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    assert_eq!(bola_groups.len(), 1);
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
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
    assert_eq!(bola_group.super_admin_list().unwrap().len(), 2);
    assert!(
        bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

    // Verify that bola can now add caro as an admin
    bola_group
        .update_admin_list(UpdateAdminListType::Add, caro.inbox_id().to_string())
        .await
        .unwrap();
    bola_group.sync().await.unwrap();
    assert_eq!(bola_group.admin_list().unwrap().len(), 1);
    assert!(
        bola_group
            .admin_list()
            .unwrap()
            .contains(&caro.inbox_id().to_string())
    );

    // Verify that no one can remove a super admin from a group
    amal_group
        .remove_members_by_identity(&[bola_wallet.identifier()])
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
    assert_eq!(bola_group.super_admin_list().unwrap().len(), 1);
    assert!(
        !bola_group
            .super_admin_list()
            .unwrap()
            .contains(&bola.inbox_id().to_string())
    );

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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Add Bola and Caro to the group
    amal_group
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
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
    let amal_group = amal.create_group(None, None).unwrap();

    // Amal adds Bola to the group
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Bola syncs groups - this will decrypt the Welcome, identify who added Bola
    // and then store that value on the group and insert into the database
    let bola_groups = bola.sync_welcomes().await.unwrap();

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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(mutable_metadata.super_admin_list.len(), 1);
    assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

    let protected_metadata: GroupMetadata = amal_group.metadata().await.unwrap();
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
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Step 2:  Amal adds Bola to the group
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Step 3: Verify that Bola can update the group name, and amal sees the update
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    bola_group
        .update_group_name("Name Update 1".to_string())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    let name = amal_group.group_name().unwrap();
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
    let binding = amal_group.mutable_metadata().expect("msg");
    let amal_group_name: &String = binding
        .attributes
        .get(&MetadataField::GroupName.to_string())
        .unwrap();
    assert_eq!(amal_group_name, "Name Update 2");
    let binding = bola_group.mutable_metadata().expect("msg");
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
    let amal_group: &TestMlsGroup = &amal.create_group(policy_set, None).unwrap();

    // Step 2:  Amal adds Bola to the group
    let bola = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    // Step 3: Bola attempts to add Caro, but fails because group is admin only
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_group: &TestMlsGroup = bola_groups.first().unwrap();
    bola_group.sync().await.unwrap();
    let result = bola_group.add_members(&[caro.inbox_id()]).await;
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
    bola_group.add_members(&[caro.inbox_id()]).await.unwrap();
    bola_group.sync().await.unwrap();
    let members = bola_group.members().await.unwrap();
    assert_eq!(members.len(), 3);
}

#[xmtp_common::test]
async fn test_optimistic_send() {
    let amal = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
    let bola_wallet = generate_local_wallet();
    let bola = Arc::new(ClientBuilder::new_test_client(&bola_wallet).await);
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.sync().await.unwrap();
    // Add bola to the group
    amal_group
        .add_members_by_identity(&[bola_wallet.identifier()])
        .await
        .unwrap();
    let bola_group = receive_group_invite(&bola).await;

    let ids = vec![
        amal_group
            .send_message_optimistic(b"test one", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test two", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test three", SendMessageOpts::default())
            .unwrap(),
        amal_group
            .send_message_optimistic(b"test four", SendMessageOpts::default())
            .unwrap(),
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
        .map(|m| m.delivery_status)
        .collect::<Vec<DeliveryStatus>>();
    assert_eq!(
        delivery,
        vec![
            DeliveryStatus::Published,
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

    // Amal creates a dm group targeting bola
    let amal_dm = amal
        .find_or_create_dm(bola.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Amal can not add caro to the dm group
    let result = amal_dm.add_members(&[caro.inbox_id()]).await;
    assert!(result.is_err());

    // Bola is already a member
    let result = amal_dm
        .add_members(&[bola.inbox_id(), caro.inbox_id()])
        .await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Bola can message amal
    let _ = bola.sync_welcomes().await;
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();

    let bola_dm: &TestMlsGroup = bola_groups.first().unwrap();
    bola_dm
        .send_message(b"test one", SendMessageOpts::default())
        .await
        .unwrap();

    // Amal sync and reads message
    amal_dm.sync().await.unwrap();
    let messages = amal_dm.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);
    let message = messages.last().unwrap();
    assert_eq!(message.decrypted_message_bytes, b"test one");

    // Amal can not remove bola
    let result = amal_dm.remove_members(&[bola.inbox_id()]).await;
    assert!(result.is_err());
    amal_dm.sync().await.unwrap();
    let members = amal_dm.members().await.unwrap();
    assert_eq!(members.len(), 2);

    // Neither Amal nor Bola is an admin or super admin
    amal_dm.sync().await.unwrap();
    bola_dm.sync().await.unwrap();
    let is_amal_admin = amal_dm.is_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_admin = amal_dm.is_admin(bola.inbox_id().to_string()).unwrap();
    let is_amal_super_admin = amal_dm.is_super_admin(amal.inbox_id().to_string()).unwrap();
    let is_bola_super_admin = amal_dm.is_super_admin(bola.inbox_id().to_string()).unwrap();
    assert!(!is_amal_admin);
    assert!(!is_bola_admin);
    assert!(!is_amal_super_admin);
    assert!(!is_bola_super_admin);
}

#[xmtp_common::test]
async fn process_messages_abort_on_retryable_error() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let alix_group = alix.create_group(None, None).unwrap();

    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();

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
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&bo_group.group_id), None)
        .await
        .unwrap()
        .group_messages()
        .unwrap();

    let db = bo.context.store().db();
    db.raw_query_write(|c| {
        c.batch_execute("BEGIN EXCLUSIVE").unwrap();
        Ok::<_, diesel::result::Error>(())
    })
    .unwrap();

    let process_result = bo_group.process_messages(bo_messages).await;
    assert!(process_result.is_errored());
    assert_eq!(process_result.errored.len(), 1);
    assert!(process_result.errored.iter().any(|(_, err)| {
        err.to_string()
            .contains("cannot start a transaction within a transaction")
    }));
}

#[xmtp_common::test]
async fn skip_already_processed_messages() {
    tester!(alix, with_name: "alix");
    tester!(bo, with_name: "bo");

    let alix_group = alix.create_group(None, None).unwrap();

    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();

    let alix_message = vec![1];
    alix_group
        .send_message(&alix_message, SendMessageOpts::default())
        .await
        .unwrap();
    bo.sync_welcomes().await.unwrap();
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();

    let mut bo_messages_from_api = bo
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&bo_group.group_id), None)
        .await
        .unwrap()
        .group_messages()
        .unwrap();

    // _NOTE:_ care should be taken with d14n since
    // messages are either commits or application messages which effects
    // the sequence_id semantics here
    let _process_result = bo_group
        .process_messages(bo_messages_from_api.clone())
        .await;
    alix_group
        .send_message(&alix_message, SendMessageOpts::default())
        .await
        .unwrap();

    // get new, unprocessed messages
    let new_message = bo
        .mls_store()
        .query_group_messages(&bo_group.group_id)
        .await
        .unwrap();
    bo_messages_from_api.extend(new_message);

    let process_result = bo_group.process_messages(bo_messages_from_api).await;
    assert_eq!(process_result.new_messages.len(), 3);
    // We no longer error when the message is previously processed
    assert_eq!(process_result.errored.len(), 0);
}
#[xmtp_common::test]
async fn skip_already_processed_intents() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    let bo_wallet = generate_local_wallet();
    let bo_client = ClientBuilder::new_test_client_vanilla(&bo_wallet).await;

    let alix_group = alix.create_group(None, None).unwrap();

    alix_group
        .add_members(&[bo_client.inbox_id()])
        .await
        .unwrap();

    bo_client.sync_welcomes().await.unwrap();
    let bo_groups = bo_client.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();
    bo_group
        .send_message(&[2], SendMessageOpts::default())
        .await
        .unwrap();
    let intent = bo_client
        .context
        .db()
        .find_group_intents(
            bo_group.clone().group_id,
            Some(vec![IntentState::Processed]),
            None,
        )
        .unwrap();
    assert_eq!(intent.len(), 2); //key_update and send_message

    let process_result = bo_group.sync_until_intent_resolved(intent[1].id).await;
    assert_ok!(process_result);
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_parallel_syncs() {
    tester!(alix1, sync_worker, sync_server);

    let alix1_group = alix1.create_group(None, None).unwrap();

    tester!(alix2, from: alix1);

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
        .context
        .api()
        .query_at(
            TopicKind::WelcomeMessagesV1.create(alix2.installation_public_key()),
            None,
        )
        .await
        .unwrap()
        .welcome_messages()
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    // Make sure that only one group message was sent
    let group_messages = alix1
        .context
        .api()
        .query_at(
            TopicKind::GroupMessagesV1.create(&alix1_group.group_id),
            None,
        )
        .await
        .unwrap();
    assert_eq!(group_messages.len(), 1);

    let alix2_group = receive_group_invite(&alix2).await;

    // Send a message from alix1
    alix1_group
        .send_message("hi from alix1".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    // Send a message from alix2
    alix2_group
        .send_message("hi from alix2".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync both clients
    alix1_group.sync().await.unwrap();
    alix2_group.sync().await.unwrap();

    let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix1_messages.len(), alix2_messages.len() - 1);

    assert!(
        alix1_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes())
    );
    assert!(
        alix2_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes())
    );
}

// Create a membership update intent, but don't sync it yet
async fn create_membership_update_no_sync(group: &TestMlsGroup) {
    let intent_data = group.get_membership_update_intent(&[], &[]).await.unwrap();

    // If there is nothing to do, stop here
    if intent_data.is_empty() {
        return;
    }

    QueueIntent::update_group_membership()
        .data(intent_data)
        .queue(group)
        .unwrap();
}

/**
 * This test case simulates situations where adding missing
 * installations gets interrupted before the sync part happens
 *
 * We need to be safe even in situations where there are multiple
 * intents that do the same thing, leading to conflicts
 */
#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn add_missing_installs_reentrancy() {
    let wallet = generate_local_wallet();
    let alix1 = ClientBuilder::new_test_client(&wallet).await;
    let alix1_group = alix1.create_group(None, None).unwrap();

    let alix2 = ClientBuilder::new_test_client(&wallet).await;

    // We are going to run add_missing_installations TWICE
    // which will create two intents to add the installations
    create_membership_update_no_sync(&alix1_group).await;
    create_membership_update_no_sync(&alix1_group).await;

    // Now I am going to run publish intents multiple times
    alix1_group
        .publish_intents()
        .await
        .expect("Expect publish to be OK");
    alix1_group
        .publish_intents()
        .await
        .expect("Expected publish to be OK");

    // Now I am going to sync twice
    alix1_group.sync_with_conn().await.unwrap();
    alix1_group.sync_with_conn().await.unwrap();

    // Make sure that only one welcome was sent
    let alix2_welcomes = alix1
        .context
        .api()
        .query_at(
            TopicKind::WelcomeMessagesV1.create(alix2.installation_public_key()),
            None,
        )
        .await
        .unwrap()
        .welcome_messages()
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    // We expect two group messages to have been sent,
    // but only the first is valid
    let group_messages = alix1
        .context
        .api()
        .query_at(
            TopicKind::GroupMessagesV1.create(&alix1_group.group_id),
            None,
        )
        .await
        .unwrap();
    assert_eq!(group_messages.len(), 2);

    let alix2_group = receive_group_invite(&alix2).await;

    // Send a message from alix1
    alix1_group
        .send_message("hi from alix1".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    // Send a message from alix2
    alix2_group
        .send_message("hi from alix2".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync both clients
    alix1_group.sync().await.unwrap();
    alix2_group.sync().await.unwrap();

    let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix1_messages.len(), alix2_messages.len() - 1);

    assert!(
        alix1_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes())
    );
    assert!(
        alix2_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes())
    );
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn respect_allow_epoch_increment() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;

    let group = client.create_group(None, None).unwrap();

    let _client_2 = ClientBuilder::new_test_client(&wallet).await;

    // Sync the group to get the message adding client_2 published to the network
    group.sync().await.unwrap();

    // Retrieve the envelope for the commit from the network
    let messages = client
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group.group_id), None)
        .await
        .unwrap()
        .group_messages()
        .unwrap();

    let first_message = messages.first().unwrap();

    let process_result = group.process_message(first_message, false).await;

    assert_err!(
        process_result,
        GroupMessageProcessingError::EpochIncrementNotAllowed
    );
}

#[rstest]
#[xmtp_common::test]
#[awt]
async fn test_get_and_set_consent(
    #[future] alix: ClientTester,
    #[future] bola: ClientTester,
    #[future] caro: ClientTester,
) {
    let alix_group = alix.create_group(None, None).unwrap();

    // group consent state should be allowed if user created it
    assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Allowed);

    alix_group
        .update_consent_state(ConsentState::Denied)
        .unwrap();
    assert_eq!(alix_group.consent_state().unwrap(), ConsentState::Denied);

    alix_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    // group consent state should default to unknown for users who did not create the group
    assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Unknown);

    bola_group
        .send_message("hi from bola".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // group consent state should be allowed if user sends a message to the group
    assert_eq!(bola_group.consent_state().unwrap(), ConsentState::Allowed);

    alix_group.add_members(&[caro.inbox_id()]).await.unwrap();

    caro.sync_welcomes().await.unwrap();
    let caro_groups = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = caro_groups.first().unwrap();

    caro_group
        .send_message_optimistic("hi from caro".as_bytes(), SendMessageOpts::default())
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
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&bo_wallet).await;
    let alix_group = alix
        .create_group_with_identifiers(&[bo_wallet.identifier()], None, None)
        .await
        .unwrap();

    bo.sync_welcomes().await.unwrap();
    let bo_groups = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = bo_groups.first().unwrap();

    // Both members see the same amount of messages to start
    alix_group
        .send_message("alix 1".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    bo_group
        .send_message("bo 1".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
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
    bo_group
        .send_message("bo 2".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

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
    let valid_dm_group = TestMlsGroup::create_test_dm_group(
        client.context.clone(),
        dm_target_inbox_id.clone(),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        valid_dm_group
            .load_mls_group_with_lock(client.context.mls_storage(), |mls_group| {
                validate_dm_group(&client.context, &mls_group, added_by_inbox).map_err(Into::into)
            })
            .is_ok()
    );

    // Test case 2: Invalid conversation type
    let invalid_protected_metadata =
        build_protected_metadata_extension(creator_inbox_id, ConversationType::Group, None)
            .unwrap();
    let invalid_type_group = TestMlsGroup::create_test_dm_group(
        client.context.clone(),
        dm_target_inbox_id.clone(),
        Some(invalid_protected_metadata),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let err =
        invalid_type_group.load_mls_group_with_lock(client.context.mls_storage(), |mls_group| {
            validate_dm_group(&client.context, &mls_group, added_by_inbox).map_err(Into::into)
        });
    assert!(matches!(
        err,
        Err(GroupError::MetadataPermissionsError(
            MetadataPermissionsError::DmValidation(DmValidationError::InvalidConversationType)
        ))
    ));
    // Test case 3: Missing DmMembers
    // This case is not easily testable with the current structure, as DmMembers are set in the protected metadata

    // Test case 4: Mismatched DM members
    let mismatched_dm_members =
        build_dm_protected_metadata_extension(creator_inbox_id, "wrong_inbox_id".to_string())
            .unwrap();
    let mismatched_dm_members_group = TestMlsGroup::create_test_dm_group(
        client.context.clone(),
        dm_target_inbox_id.clone(),
        Some(mismatched_dm_members),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let err = mismatched_dm_members_group.load_mls_group_with_lock(
        client.context.mls_storage(),
        |mls_group| {
            validate_dm_group(&client.context, &mls_group, added_by_inbox).map_err(Into::into)
        },
    );
    assert!(matches!(
        err,
        Err(GroupError::MetadataPermissionsError(
            MetadataPermissionsError::DmValidation(DmValidationError::ExpectedInboxesDoNotMatch)
        ))
    ));

    // Test case 5: Non-empty admin list
    let non_empty_admin_list =
        build_mutable_metadata_extension_default(creator_inbox_id, GroupMetadataOptions::default())
            .unwrap();
    let non_empty_admin_list_group = TestMlsGroup::create_test_dm_group(
        client.context.clone(),
        dm_target_inbox_id.clone(),
        None,
        Some(non_empty_admin_list),
        None,
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        non_empty_admin_list_group.load_mls_group_with_lock(
            client.context.mls_storage(),
            |mls_group| {
                validate_dm_group(&client.context, &mls_group, added_by_inbox).map_err(Into::into)
            }
        ),
        Err(GroupError::MetadataPermissionsError(
            MetadataPermissionsError::DmValidation(
                DmValidationError::MustHaveEmptyAdminAndSuperAdmin
            )
        ))
    ));

    // Test case 6: Non-empty super admin list
    // Similar to test case 5, but with super_admin_list

    // Test case 7: Invalid permissions
    let invalid_permissions = PolicySet::default();
    let invalid_permissions_group = TestMlsGroup::create_test_dm_group(
        client.context.clone(),
        dm_target_inbox_id.clone(),
        None,
        None,
        None,
        Some(invalid_permissions),
        None,
    )
    .unwrap();
    assert!(matches!(
        invalid_permissions_group.load_mls_group_with_lock(
            client.context.mls_storage(),
            |mls_group| {
                validate_dm_group(&client.context, &mls_group, added_by_inbox).map_err(Into::into)
            }
        ),
        Err(GroupError::MetadataPermissionsError(
            MetadataPermissionsError::DmValidation(DmValidationError::InvalidPermissions)
        ))
    ));
}

#[xmtp_common::test]
async fn test_respects_character_limits_for_group_metadata() {
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
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

    // Verify that updating the app data with an excessive length fails
    let overlong_app_data = "d".repeat(MAX_APP_DATA_LENGTH + 1);
    let result = amal_group.update_app_data(overlong_app_data).await;
    assert!(
        matches!(result, Err(GroupError::TooManyCharacters { length }) if length == MAX_APP_DATA_LENGTH)
    );

    // Verify updates with valid lengths are successful
    let valid_name = "Valid Group Name".to_string();
    let valid_description = "Valid group description within limit.".to_string();
    let valid_image_url = "http://example.com/image.png".to_string();
    let valid_app_data = "Valid app data content".to_string();

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
    amal_group
        .update_app_data(valid_app_data.clone())
        .await
        .unwrap();

    // Sync and verify stored values
    amal_group.sync().await.unwrap();

    let metadata = amal_group.mutable_metadata().unwrap();

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
    assert_eq!(
        metadata
            .attributes
            .get(&MetadataField::AppData.to_string())
            .unwrap(),
        &valid_app_data
    );
}

#[xmtp_common::test]
async fn test_update_app_data() {
    tester!(amal);

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Update app data with a valid value
    let app_data = "Test application data".to_string();
    amal_group.update_app_data(app_data.clone()).await.unwrap();
    amal_group.sync().await.unwrap();

    // Verify the app data was updated using the getter
    let retrieved_app_data = amal_group.app_data().unwrap();
    assert_eq!(retrieved_app_data, app_data);

    // Update with maximum allowed size (8KB)
    let max_size_data = "x".repeat(MAX_APP_DATA_LENGTH);
    amal_group
        .update_app_data(max_size_data.clone())
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    let retrieved_max_data = amal_group.app_data().unwrap();
    assert_eq!(retrieved_max_data, max_size_data);
}

#[xmtp_common::test]
async fn test_app_data_in_dm() {
    tester!(amal);
    tester!(bola);

    // Create a DM
    let dm = amal
        .find_or_create_dm(bola.inbox_id().to_string(), None)
        .await
        .unwrap();

    // Verify that updating app_data on a DM fails
    let result = dm.update_app_data("test data".to_string()).await;
    assert!(matches!(
        result,
        Err(GroupError::MetadataPermissionsError(
            MetadataPermissionsError::DmGroupMetadataForbidden
        ))
    ));
}

#[xmtp_common::test]
async fn test_create_group_with_app_data() {
    tester!(amal);

    let initial_app_data = "Initial app data from options".to_string();

    // Create a group with app_data set through GroupMetadataOptions
    let group = amal
        .create_group(
            None,
            Some(GroupMetadataOptions {
                name: Some("Test Group".to_string()),
                description: Some("Test Description".to_string()),
                image_url_square: None,
                message_disappearing_settings: None,
                app_data: Some(initial_app_data.clone()),
            }),
        )
        .unwrap();

    group.sync().await.unwrap();

    // Verify the app_data was set correctly
    let retrieved_app_data = group.app_data().unwrap();
    assert_eq!(retrieved_app_data, initial_app_data);

    // Verify we can also update it
    let updated_app_data = "Updated app data".to_string();
    group
        .update_app_data(updated_app_data.clone())
        .await
        .unwrap();
    group.sync().await.unwrap();

    let final_app_data = group.app_data().unwrap();
    assert_eq!(final_app_data, updated_app_data);
}

#[xmtp_common::test]
async fn test_create_group_with_default_app_data() {
    tester!(amal);

    // Create a group without specifying app_data (should default to empty string)
    let group = amal
        .create_group(None, Some(GroupMetadataOptions::default()))
        .unwrap();

    group.sync().await.unwrap();

    // Verify the app_data defaults to empty string
    let retrieved_app_data = group.app_data().unwrap();
    assert_eq!(retrieved_app_data, "");
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
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    tester!(amal, version: amal_version.clone());
    tester!(bo);

    // ensure the version is as expected
    assert!(bo.context.version_info() != &amal_version);
    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal updates the group name and sends a message to the group
    amal_group
        .update_group_name("new name".to_string())
        .await
        .unwrap();
    amal_group
        .send_message("Hello, world!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message even though they are on different client versions
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);

    let message_text = String::from_utf8_lossy(&messages[2].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group version to match their client version
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 6: Bo should now be unable to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 3);

    // Step 7: Bo updates their client, and see if we can then download latest messages
    let mut bo_version = bo.version_info().clone();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let bo = ClientBuilder::from_client(bo.client)
        .version(bo_version.clone())
        .build()
        .await
        .unwrap();

    assert_eq!(bo.context.version_info(), amal.context.version_info());

    // Refresh Bo's group context
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();

    bo_group.sync().await.unwrap();
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 5);
}

#[xmtp_common::test]
async fn test_client_on_old_version_pauses_after_joining_min_version_group() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    // Step 1: Create three clients, amal and bo are one version ahead of caro
    let amal =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), amal_version).await;

    let mut bo_version = VersionInfo::default();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let bo =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), bo_version).await;

    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    assert!(caro.version_info().pkg_version() != amal.version_info().pkg_version());
    assert!(bo.version_info().pkg_version() == amal.version_info().pkg_version());

    // Step 2: Amal creates a group and adds bo as a member
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Step 3: Amal sends a message to the group
    amal_group
        .send_message("Hello, world!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 4: Verify that bo can read the message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 2);

    let message_text = String::from_utf8_lossy(&messages[1].decrypted_message_bytes);
    assert_eq!(message_text, "Hello, world!");

    // Step 5: Amal updates the group to have a min version of current version + 1
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();
    amal_group
        .send_message("new version only!".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Step 6: Bo should still be able to sync messages for the group
    let _ = bo_group.sync().await;
    let messages = bo_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(messages.len(), 4);

    // Step 7: Amal adds caro as a member
    amal_group
        .add_members(&[caro.context.identity.inbox_id()])
        .await
        .unwrap();

    // Caro received the invite for the group
    caro.sync_welcomes().await.unwrap();
    let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = binding.first().unwrap();
    assert!(caro_group.group_id == amal_group.group_id);

    // Caro group is paused immediately after joining
    let is_paused = caro_group.paused_for_version().unwrap().is_some();
    assert!(is_paused);
    let result = caro_group
        .send_message("Hello from Caro".as_bytes(), SendMessageOpts::default())
        .await;
    assert!(matches!(result, Err(GroupError::GroupPausedUntilUpdate(_))));

    // Caro updates their client to the same version as amal and syncs to unpause the group
    let mut caro_version = caro.version_info().clone();
    caro_version.test_update_version(
        increment_patch_version(caro_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    let caro = ClientBuilder::from_client(caro)
        .version(caro_version)
        .build()
        .await
        .unwrap();
    let binding = caro.find_groups(GroupQueryArgs::default()).unwrap();
    let caro_group = binding.first().unwrap();
    assert!(caro_group.group_id == amal_group.group_id);
    caro_group.sync().await.unwrap();

    // Caro should now be able to send a message
    caro_group
        .send_message("Hello from Caro".as_bytes(), SendMessageOpts::default())
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

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
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
        .is_admin(bo.context.identity.inbox_id().to_string())
        .unwrap();
    assert!(is_bo_admin);

    let is_bo_super_admin = amal_group
        .is_super_admin(bo.context.identity.inbox_id().to_string())
        .unwrap();
    assert!(!is_bo_super_admin);

    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    let metadata = bo_group.mutable_metadata().unwrap();
    let min_version = metadata
        .attributes
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version, None);

    let result = bo_group.update_group_min_version_to_match_self().await;
    assert!(result.is_err());
    bo_group.sync().await.unwrap();

    let metadata = bo_group.mutable_metadata().unwrap();
    let min_version = metadata
        .attributes
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version, None);

    amal_group.sync().await.unwrap();
    let result = amal_group.update_group_min_version_to_match_self().await;
    assert!(result.is_ok());
    bo_group.sync().await.unwrap();

    let metadata = bo_group.mutable_metadata().unwrap();
    let min_version = metadata
        .attributes
        .get(&MetadataField::MinimumSupportedProtocolVersion.to_string());
    assert_eq!(min_version.unwrap(), amal.version_info().pkg_version());
}

#[xmtp_common::test]
async fn test_send_message_while_paused_after_welcome_returns_expected_error() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    // Create two clients with different versions
    let amal =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), amal_version).await;

    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a group and adds bo
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Amal sets minimum version requirement
    amal_group
        .update_group_min_version_to_match_self()
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Bo joins group and attempts to send message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();

    // If bo tries to send a message before syncing the group, we get a SyncFailedToWait error
    let result = bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await;
    assert!(
        matches!(result, Err(GroupError::SyncFailedToWait(_))),
        "Expected SyncFailedToWait error, got {:?}",
        result
    );

    bo_group.sync().await.unwrap();

    // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
    let result = bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await;
    if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
        assert_eq!(version, amal.version_info().pkg_version());
    } else {
        panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
    }
}

#[xmtp_common::test]
async fn test_send_message_after_min_version_update_gets_expected_error() {
    let mut amal_version = VersionInfo::default();
    amal_version.test_update_version(
        increment_patch_version(amal_version.pkg_version())
            .unwrap()
            .as_str(),
    );

    // Create two clients with different versions
    let amal =
        ClientBuilder::new_test_client_with_version(&generate_local_wallet(), amal_version.clone())
            .await;
    assert!(amal.context.version_info() != &VersionInfo::default());
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Amal creates a group and adds bo
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await
        .unwrap();

    // Bo joins group and successfully sends initial message
    bo.sync_welcomes().await.unwrap();
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
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
        .send_message(
            "Second message from Bo".as_bytes(),
            SendMessageOpts::default(),
        )
        .await;
    assert!(
        matches!(result, Err(GroupError::SyncFailedToWait(_))),
        "Expected SyncFailedToWait error, got {:?}",
        result
    );

    // Bo syncs to get the version update
    bo_group.sync().await.unwrap();

    // After syncing if we attempt to send message - should fail with GroupPausedUntilUpdate error
    let result = bo_group
        .send_message("Hello from Bo".as_bytes(), SendMessageOpts::default())
        .await;
    if let Err(GroupError::GroupPausedUntilUpdate(version)) = result {
        assert_eq!(version, amal.version_info().pkg_version());
    } else {
        panic!("Expected GroupPausedUntilUpdate error, got {:?}", result);
    }

    // Verify Bo can send again after updating their version
    let mut bo_version = bo.version_info().clone();
    bo_version.test_update_version(
        increment_patch_version(bo_version.pkg_version())
            .unwrap()
            .as_str(),
    );
    let bo = ClientBuilder::from_client(bo)
        .version(bo_version)
        .build()
        .await
        .unwrap();

    // Need to get fresh group reference after version update
    let binding = bo.find_groups(GroupQueryArgs::default()).unwrap();
    let bo_group = binding.first().unwrap();
    bo_group.sync().await.unwrap();

    // Should now succeed
    let result = bo_group
        .send_message(
            "Message after update".as_bytes(),
            SendMessageOpts::default(),
        )
        .await;
    assert!(result.is_ok());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_can_make_inbox_with_a_bad_key_package_an_admin() {
    use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

    // 1) Prepare clients
    let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let charlie = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    // Create a wallet for the user with a bad key package
    let bola_wallet = generate_local_wallet();
    let bola = ClientBuilder::new_test_client(&bola_wallet).await;
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
    let bola_2 = ClientBuilder::new_test_client(&bola_wallet).await;
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

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_when_processing_message_return_future_wrong_epoch_group_marked_probably_forked() {
    use crate::utils::test_mocks_helpers::set_test_mode_future_wrong_epoch;

    let client_a = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let client_b = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let group_a = client_a.create_group(None, None).unwrap();
    group_a.add_members(&[client_b.inbox_id()]).await.unwrap();

    client_b.sync_welcomes().await.unwrap();

    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    group_a
        .send_message(&[1], SendMessageOpts::default())
        .await
        .unwrap();
    set_test_mode_future_wrong_epoch(true);
    group_b.sync().await.unwrap();
    set_test_mode_future_wrong_epoch(false);
    let group_debug_info = group_b.debug_info().await.unwrap();
    assert!(group_debug_info.maybe_forked);
    assert!(!group_debug_info.fork_details.is_empty());
    client_b
        .context
        .db()
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
    let group_a = client_a1.create_group(None, None).unwrap();

    // Add client_b and client_c to the group
    group_a
        .add_members(&[client_b.inbox_id(), client_c.inbox_id()])
        .await
        .unwrap();

    // Sync the group
    client_b.sync_welcomes().await.unwrap();
    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    client_c.sync_welcomes().await.unwrap();
    let binding = client_c.find_groups(GroupQueryArgs::default()).unwrap();
    let group_c = binding.first().unwrap();

    // Each client sends a message and syncs (ensures any key update commits are sent)
    group_a
        .send_message_optimistic("Message a1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_a.publish_intents().await.unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_b
        .send_message_optimistic("Message b1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_b.publish_intents().await.unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_c
        .send_message_optimistic("Message c1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_c.publish_intents().await.unwrap();

    // Sync the groups
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    // After client a adds b and c, and they each sent a message, all groups are in the same epoch
    assert_eq!(group_a.epoch().await.unwrap(), 3);
    assert_eq!(group_b.epoch().await.unwrap(), 3);
    assert_eq!(group_c.epoch().await.unwrap(), 3);

    // Client b updates the group name, (incrementing the epoch from 3 to 4), and syncs
    group_b
        .update_group_name("Group B".to_string())
        .await
        .unwrap();
    group_b.sync().await.unwrap();

    // Client c sends two text messages before incrementing the epoch
    group_c
        .send_message_optimistic("Message c2".as_bytes(), SendMessageOpts::default())
        .unwrap();
    group_c.publish_intents().await.unwrap();
    group_b.sync().await.unwrap();

    // Retrieve all messages from group B, verify they contain the two messages from client c even though they were sent from the wrong epoch
    let messages = client_b
        .context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(&group_b.group_id), None)
        .await
        .unwrap()
        .group_messages()
        .unwrap();
    assert_eq!(messages.len(), 8);

    // Get reference to last message
    let last_message = messages.last().unwrap();

    // This is the key line, because we pass in false for incrementing epoch/cursor (simulating streaming)
    // This processing will not longer update the cursor, so we will not be forked
    let increment_epoch = false;
    let result = group_a.process_message(last_message, increment_epoch).await;
    assert!(result.is_ok());

    // Now syncing a will update group_a group name since the cursor has NOT moved on past it
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    assert_eq!(group_b.epoch().await.unwrap(), 4);
    assert_eq!(group_c.epoch().await.unwrap(), 4);
    // We pass on the last line because a's cursor has not moved past any commits, even though it processed
    // messages out of order
    assert_eq!(group_a.epoch().await.unwrap(), 4);
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn non_retryable_error_increments_cursor() {
    let alice = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    // Create a group
    let group = alice.create_group(None, None).unwrap();
    group.add_members::<String>(&[]).await.unwrap();

    let storage = alice.context.mls_storage();

    // create a fake message with an invalid body
    // an envelope with an empty content is a non-retryable error.
    // since we are also trying to decrypt our own message, this is also non-retryable.
    let invalid_payload_message = PlaintextEnvelope { content: None };
    let invalid_message_bytes = invalid_payload_message.encode_to_vec();
    let message = group
        .load_mls_group_with_lock(storage, |mut mls_group| {
            let m = mls_group
                .create_message(
                    &XmtpOpenMlsProviderRef::new(storage),
                    &alice.context.identity().installation_keys,
                    invalid_message_bytes.as_slice(),
                )
                .unwrap();
            Ok(m)
        })
        .unwrap();

    // what the new cursor should be
    // set cursor to the max u64 value -1_000 to ensure its higher than the cursor in the backend
    // TODO: using u64::MAX here causes an implicit overflow for the i64 comparison (i think),
    // making us actually return the message as already processed, since it loops back to 0,
    // thereby less than group cursor. Thats why we take i64 max before casting to u64, rather than
    // u64::MAX.
    let new_cursor = Cursor::mls_commits((i64::MAX - 1_000) as u64);

    let message = xmtp_proto::types::GroupMessage {
        cursor: new_cursor,
        created_ns: DateTime::from_timestamp_nanos(xmtp_common::time::now_ns()),
        group_id: group.group_id.clone().into(),
        message: MlsMessageIn::tls_deserialize(&mut message.to_bytes().unwrap().as_slice())
            .unwrap()
            .try_into_protocol_message()
            .unwrap(),
        sender_hmac: vec![],
        should_push: false,
        payload_hash: vec![],
        depends_on: Default::default(),
    };

    let res = group.process_message(&message, true).await;
    assert!(res.is_err());
    assert!(!res.unwrap_err().is_retryable());
    let last_cursor = alice
        .context
        .db()
        .get_last_cursor_for_originator(
            &group.group_id,
            EntityKind::ApplicationMessage,
            Originators::MLS_COMMITS,
        )
        .unwrap();
    assert_eq!(new_cursor, last_cursor);
}

#[xmtp_common::test]
async fn test_generate_commit_with_rollback() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None).unwrap();
    group.add_members(&[bo.inbox_id()]).await.unwrap();
    group.sync().await.unwrap();

    let provider = alix.context.mls_storage();
    let hash = || provider.hash_all().map(hex::encode).unwrap();

    let start_hash = hash();
    tracing::info!("start_hash: {start_hash}");

    let group_provider = group.context.mls_storage();
    let installation_keys = group.context.identity().installation_keys.clone();
    let mut in_generate_commit_before_hash = None;
    let mut in_generate_commit_after_hash = None;
    let in_generate_commit_before_hash_mut = &mut in_generate_commit_before_hash;
    let in_generate_commit_after_hash_mut = &mut in_generate_commit_after_hash;
    group
        .load_mls_group_with_lock_async(async |mut mls_group| {
            let extensions = super::build_extensions_for_metadata_update(
                &mls_group,
                "foo".to_string(),
                "bar".to_string(),
            )
            .unwrap();
            // Simulate mutable metadata update
            let (_, _, _) = super::mls_sync::generate_commit_with_rollback(
                group_provider,
                &mut mls_group,
                |group, provider| {
                    use xmtp_db::MlsProviderExt;
                    *in_generate_commit_before_hash_mut =
                        provider.key_store().hash_all().map(hex::encode).ok();
                    let result = group.update_group_context_extensions(
                        provider,
                        extensions,
                        &installation_keys,
                    );
                    *in_generate_commit_after_hash_mut =
                        provider.key_store().hash_all().map(hex::encode).ok();
                    result
                },
            )
            .unwrap();
            Ok::<_, GroupError>(())
        })
        .await
        .unwrap();
    let in_generate_commit_before_hash = in_generate_commit_before_hash.unwrap();
    let in_generate_commit_after_hash = in_generate_commit_after_hash.unwrap();
    tracing::info!("in_generate_commit_before_hash: {in_generate_commit_before_hash}");
    tracing::info!("in_generate_commit_after_hash: {in_generate_commit_after_hash}");
    let end_hash = hash();
    tracing::info!("end_hash: {end_hash}");
    assert_eq!(start_hash, end_hash);
    assert_ne!(
        in_generate_commit_before_hash,
        in_generate_commit_after_hash
    );
    assert_eq!(start_hash, in_generate_commit_before_hash);
    assert_ne!(end_hash, in_generate_commit_after_hash);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_state() {
    tester!(alix);
    tester!(bola);

    // Create a group with alix as creator
    let group = alix.create_group(None, None)?;

    // Alix should have Allowed membership state (creator is immediately Allowed)
    let state = group.membership_state()?;
    assert_eq!(state, GroupMembershipState::Allowed);

    // Add bola to the group
    group.add_members(&[bola.inbox_id()]).await?;

    // Sync so bola receives the welcome
    bola.sync_welcomes().await?;
    let bola_groups = bola.find_groups(GroupQueryArgs::default())?;
    assert_eq!(bola_groups.len(), 1);
    let bola_group = &bola_groups[0];

    // Bola should have Pending membership state when first receiving the welcome
    let bola_state = bola_group.membership_state()?;
    assert_eq!(bola_state, GroupMembershipState::Pending);
}
