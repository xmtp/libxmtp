mod test_bad_key_packages;
mod test_change_callbacks;
mod test_commit_log_fork_detection;
mod test_commit_log_local;
mod test_commit_log_readd_requests;
mod test_commit_log_remote;
mod test_consent;
mod test_delete_message;
mod test_dm;
mod test_extract_readded_installations;
#[cfg(not(target_arch = "wasm32"))]
mod test_failed_installations;
mod test_group_updated;
mod test_libxmtp_version;
mod test_membership;
mod test_message_disappearing_settings;
mod test_metadata_permissions;
#[cfg(not(target_arch = "wasm32"))]
mod test_metadata_read_amplification;
mod test_min_version;
#[cfg(not(target_arch = "wasm32"))]
mod test_network;
mod test_prepare_message_for_later_publish;
mod test_proposals;
mod test_proposals_app_data;
mod test_proposals_enablement;
mod test_proposals_pause;
mod test_proposals_permissions;
mod test_self_removal;
mod test_send_message_opts;
mod test_send_receive;
mod test_starting_membership_sequence_id;
#[cfg(not(target_arch = "wasm32"))]
mod test_state_processes;
mod test_state_writes;
mod test_sync_concurrency;
mod test_validate_app_data_update;
mod test_welcome_pointers;
mod test_welcomes;
use crate::groups::send_message_opts::SendMessageOpts;
use prost::Message;
use xmtp_db::ConnectionExt;
use xmtp_db::XmtpOpenMlsProviderRef;
use xmtp_id::InboxOwner;
use xmtp_proto::types::{Cursor, Topic};

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
        validate_dm_group,
    },
    utils::test::FullXmtpClient,
};
use diesel::connection::SimpleConnection;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use futures::future::join_all;
use rstest::*;
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

#[track_caller]
pub(super) fn assert_blocked_obligation(
    error: &crate::subscriptions::barrier::BarrierError,
    topic: &Topic,
    processed: Cursor,
    code: &str,
) -> crate::subscriptions::barrier::BarrierTopic {
    use crate::subscriptions::barrier::{BarrierCause, BarrierError, BarrierFailure};
    let BarrierError::Incomplete { reason, unfinished } = error;
    assert_eq!(*reason, BarrierFailure::Blocked);
    let [status] = unfinished.as_slice() else {
        panic!("expected one blocked obligation, got {unfinished:?}");
    };
    assert_eq!(&status.topic, topic);
    let target = status
        .target
        .expect("blocked work must have a fixed target");
    assert!(status.received >= target);
    assert_eq!(status.processed, processed);
    assert!(target > processed);
    assert!(!status.inactive);
    assert!(matches!(&status.cause, Some(BarrierCause::Blocked(actual)) if actual == code));
    status.clone()
}

#[track_caller]
pub(super) fn assert_version_sync_blocked(error: GroupError, topic: &Topic, processed: Cursor) {
    let GroupError::Sync(summary) = error else {
        panic!("expected a blocked sync summary, got {error:?}");
    };
    let Some(GroupError::StreamBarrier(error)) = summary.other.as_deref() else {
        panic!("expected the processing barrier cause, got {summary:?}");
    };
    let status = assert_blocked_obligation(error, topic, processed, "unsupported_protocol_version");
    assert!(status.unresolved_welcomes.is_empty());
}

#[track_caller]
pub(super) fn assert_paused_sync(error: GroupError, expected_version: &str) {
    let GroupError::Sync(summary) = error else {
        panic!("expected a paused sync summary, got {error:?}");
    };
    assert!(matches!(
        summary.other.as_deref(),
        Some(GroupError::GroupPausedUntilUpdate(version)) if version == expected_version
    ));
}

pub(super) async fn receive_group_invite(client: &FullXmtpClient) -> TestMlsGroup {
    client.sync_welcomes().await.unwrap();
    let mut groups = client.find_groups(GroupQueryArgs::default()).unwrap();

    groups.remove(0)
}

pub(super) async fn get_latest_message(group: &TestMlsGroup) -> StoredGroupMessage {
    group.sync().await.unwrap();
    let mut messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    messages.pop().unwrap()
}

// Adds a member to the group without the usual validations on group membership
// Used for testing adversarial scenarios
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn force_add_member(
    sender_client: &FullXmtpClient,
    new_member_client: &FullXmtpClient,
    sender_group: &TestMlsGroup,
    sender_mls_group: &mut openmls::prelude::MlsGroup,
    sender_provider: &impl xmtp_db::MlsProviderExt,
) {
    use crate::groups::mls_ext::WelcomePointersExtension;
    use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
    use xmtp_id::key_package::WrapperAlgorithm;

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

// Test members function from non group creator

// Amal and Bola will both try and add Charlie from the same epoch.
// The group should resolve to a consistent state

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
        .raw_query(|conn| {
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

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_self_resolve_epoch_mismatch() {
    tester!(amal);
    tester!(bola);
    tester!(charlie);
    tester!(dave);
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
async fn test_dm_creation() {
    tester!(amal);
    tester!(bola);
    tester!(caro);

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

// Create a membership update intent, but don't sync it yet
pub(super) async fn create_membership_update_no_sync(group: &TestMlsGroup) {
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

#[cfg_attr(target_arch = "wasm32", ignore)]
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

#[xmtp_common::test]
async fn test_validate_dm_group() {
    tester!(client);
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
    let non_empty_admin_list = build_mutable_metadata_extension_default(
        creator_inbox_id,
        GroupMetadataOptions::default(),
        xmtp_configuration::ENABLE_COMMIT_LOG,
    )
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
async fn test_update_app_data() {
    tester!(amal);

    let policy_set = Some(PreconfiguredPolicies::AdminsOnly.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    // Update app data with a valid value
    let app_data = "Test application data".to_string();
    amal_group
        .update_app_data(app_data.clone(), None)
        .await
        .unwrap();
    amal_group.sync().await.unwrap();

    // Verify the app data was updated using the getter
    let retrieved_app_data = amal_group.app_data().unwrap();
    assert_eq!(retrieved_app_data, app_data);

    // Update with maximum allowed size (8KB)
    let max_size_data = "x".repeat(MAX_APP_DATA_LENGTH);
    amal_group
        .update_app_data(max_size_data.clone(), None)
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
    let result = dm.update_app_data("test data".to_string(), None).await;
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
        .update_app_data(updated_app_data.clone(), None)
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

pub(super) fn increment_patch_version(version: &str) -> Option<String> {
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
