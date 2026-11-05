use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use crate::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
mod disconnect_propagation;
mod test_bad_key_packages;
mod test_change_callbacks;
mod test_commit_log_fork_detection;
mod test_commit_log_readd_requests;
mod test_commit_log_remote;
mod test_commit_log_upgrade;
#[cfg(not(target_arch = "wasm32"))]
mod test_configuration;
mod test_consent;
mod test_delete_message;
mod test_deletion;
mod test_dm;
#[cfg(not(target_arch = "wasm32"))]
mod test_failed_installations;
mod test_group_info;
mod test_group_updated;
mod test_identity_revocation;
mod test_identity_updates;
mod test_leaf_key_binding;
mod test_membership;
mod test_message_disappearing_settings;
mod test_metadata_permissions;
#[cfg(not(target_arch = "wasm32"))]
mod test_metadata_read_amplification;
mod test_min_version;
mod test_prepare_message_for_later_publish;
mod test_proposals;
mod test_proposals_app_data;
mod test_proposals_pause;
mod test_proposals_permissions;
#[cfg(not(target_arch = "wasm32"))]
mod test_publication_durability;
mod test_self_removal;
mod test_send_message_opts;
mod test_send_receive;
mod test_starting_membership_sequence_id;
#[cfg(not(target_arch = "wasm32"))]
mod test_state_processes;
#[cfg(not(target_arch = "wasm32"))]
mod test_state_snapshot;
mod test_state_writes;
mod test_sync_concurrency;
mod test_welcome_pointers;
mod test_welcomes;
use crate::groups::send_message_opts::SendMessageOpts;
use prost::Message;
use xmtp_db::ConnectionExt;
use xmtp_db::XmtpOpenMlsProviderRef;
use xmtp_id::InboxOwner;
use xmtp_proto::types::{Cursor, Topic};

use super::group_permissions::{MembershipPolicies, PolicySet};
use crate::groups::context::XmtpSharedContext;
use crate::groups::intents::QueueIntent;
use crate::groups::{
    DeliveryStatus, GroupError, GroupMetadataOptions, PreconfiguredPolicies, UpdateAdminListType,
    intents::{PermissionPolicyOption, PermissionUpdateType},
    members::{GroupMember, PermissionLevel},
    validate_dm_group,
};
use crate::groups::{DmValidationError, GroupLeaveValidationError, MetadataPermissionsError};
use crate::groups::{
    MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
    MAX_GROUP_NAME_LENGTH,
};
use diesel::connection::SimpleConnection;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use rstest::*;
use xmtp_common::StreamHandle as _;
use xmtp_common::time::now_ns;
use xmtp_common::{assert_err, assert_ok};
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
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
use xmtp_id::associations::test_utils::add_wallet_signature;
use xmtp_mls_core::version::VersionInfo;
use xmtp_mls_group_state::group_mutable_metadata::{MessageDisappearingSettings, MetadataField};
use xmtp_proto::xmtp::mls::message_contents::{EncodedContent, PlaintextEnvelope};

pub(super) use crate::groups::test_support::{assert_blocked_obligation, increment_patch_version};

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

pub(super) async fn receive_group_invite(client: &EngineTestPeer) -> EngineTestGroup {
    client.sync_welcomes().await.unwrap();
    let mut groups = client.find_groups(GroupQueryArgs::default()).unwrap();

    groups.remove(0)
}

pub(super) async fn get_latest_message(group: &EngineTestGroup) -> StoredGroupMessage {
    group.sync().await.unwrap();
    let mut messages = group.find_messages(&MsgQueryArgs::default()).unwrap();
    messages.pop().unwrap()
}

/// Revoke the installation `key` from `member`'s inbox.
pub(in crate::groups) async fn revoke(member: &EngineTestPeer, key: &[u8]) {
    let mut request = member
        .identity_updates()
        .revoke_installations(vec![key.to_vec()])
        .await
        .unwrap();
    add_wallet_signature(&mut request, &member.owner).await;
    member
        .identity_updates()
        .apply_signature_request(request)
        .await
        .unwrap();
}

// Adds a member to the group without the usual validations on group membership
// Used for testing adversarial scenarios
#[cfg(not(target_arch = "wasm32"))]
// A test drives an OpenMLS convenience commit; its GroupInfo never leaves the test.
#[allow(clippy::disallowed_methods)]
pub(super) async fn force_add_member(
    sender_client: &EngineTestPeer,
    new_member_client: &EngineTestPeer,
    sender_group: &EngineTestGroup,
    sender_mls_group: &mut openmls::prelude::MlsGroup,
    sender_provider: &impl xmtp_db::MlsProviderExt,
) {
    use crate::groups::mls_ext::WelcomePointersExtension;
    use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
    use xmtp_id::key_package::WrapperEncryptionExtension;

    use super::intents::{Installation, SendWelcomesAction};
    use openmls::prelude::tls_codec::Serialize;
    let new_member_provider = new_member_client.context.mls_provider();

    let key_package_result = xmtp_mls_identity::identity_lifecycle::new_key_package(
        new_member_client.identity(),
        &new_member_provider,
        CREATE_PQ_KEY_PACKAGE_EXTENSION,
    )
    .unwrap();
    let wrapper =
        WrapperEncryptionExtension::advertised_by(&key_package_result.key_package).unwrap();
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
            hpke_public_key: wrapper.pub_key_bytes,
            welcome_wrapper_algorithm: wrapper.algorithm,
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
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

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
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;
    let charlie = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
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

// Create a membership update intent, but don't sync it yet
pub(super) async fn create_membership_update_no_sync(group: &EngineTestGroup) {
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

// verifies: DMS-021
#[xmtp_common::test(unwrap_try = true)]
async fn test_validate_dm_group() {
    use crate::groups::build_group_config;
    use openmls::prelude::{CredentialWithKey, MlsGroup as OpenMlsGroup};
    use tls_codec::Serialize;
    use xmtp_mls_group_keys::group_keys::GroupRoot;
    use xmtp_mls_group_state::{
        app_data::{
            component_id::ComponentId,
            creation::{InitialGroupKind, initial_dictionary},
        },
        inbox_id::InboxId as ComponentInboxId,
    };
    use xmtp_mls_tls_collections::tls_set::TlsSet;

    let client = EngineTestPeer::new().await;
    let added_by_inbox = hex::encode([0x42; 32]);
    // A DM Welcome must carry the group's root.
    let root = GroupRoot::generate();
    let make_group = |policies: PolicySet,
                      changes: Vec<(ComponentId, Option<Vec<u8>>)>,
                      leaf_inbox: Option<&str>| {
        let mut dictionary = initial_dictionary(
            InitialGroupKind::Dm {
                target_inbox_id: &added_by_inbox,
            },
            &policies.to_proto().unwrap(),
            &GroupMetadataOptions::default(),
            client.inbox_id(),
            Some(&root),
            &[],
        )
        .unwrap();
        for (id, value) in changes {
            if let Some(value) = value {
                dictionary.insert(id.as_u16(), value);
            } else {
                dictionary.remove(&id.as_u16());
            }
        }
        let config = build_group_config(dictionary).unwrap();
        let identity = client.context.identity();
        OpenMlsGroup::new(
            &client.context.mls_provider(),
            &identity.installation_keys,
            &config,
            CredentialWithKey {
                credential: leaf_inbox
                    .map(|inbox| xmtp_mls_core::identity::create_credential(inbox).unwrap())
                    .unwrap_or_else(|| identity.credential()),
                signature_key: identity.installation_keys.public_slice().into(),
            },
        )
        .unwrap()
    };
    let validate =
        |group: &OpenMlsGroup| validate_dm_group(&client.context, group, &added_by_inbox);
    assert!(validate(&make_group(PolicySet::new_dm(), vec![], None)).is_ok());
    assert!(
        validate_dm_group(
            &client.context,
            &make_group(PolicySet::new_dm(), vec![], None),
            client.inbox_id(),
        )
        .is_ok()
    );

    // Metadata extraction already rejects a DM pair outside a DM, so drop
    // the pair to reach the type check itself.
    let invalid_type = make_group(
        PolicySet::new_dm(),
        vec![
            (
                ComponentId::CONVERSATION_TYPE,
                Some((ConversationType::Group as i32).to_be_bytes().to_vec()),
            ),
            (ComponentId::DM_MEMBERS, None),
        ],
        None,
    );
    assert!(matches!(
        validate(&invalid_type),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::InvalidConversationType
        ))
    ));

    let missing_members = make_group(
        PolicySet::new_dm(),
        vec![(ComponentId::DM_MEMBERS, None)],
        None,
    );
    assert!(matches!(
        validate(&missing_members),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::MustHaveMembersSet
        ))
    ));

    let wrong_members = TlsSet::from_keys([
        ComponentInboxId::from_hex(client.inbox_id())?,
        ComponentInboxId::from_hex(&hex::encode([0x43; 32]))?,
    ])
    .tls_serialize_detached()?;
    let mismatched_members = make_group(
        PolicySet::new_dm(),
        vec![(ComponentId::DM_MEMBERS, Some(wrong_members))],
        None,
    );
    assert!(matches!(
        validate(&mismatched_members),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::ExpectedInboxesDoNotMatch
        ))
    ));

    for component in [ComponentId::ADMIN_LIST, ComponentId::SUPER_ADMIN_LIST] {
        let admins = TlsSet::from_keys([ComponentInboxId::from_hex(client.inbox_id())?])
            .tls_serialize_detached()?;
        let group = make_group(PolicySet::new_dm(), vec![(component, Some(admins))], None);
        assert!(matches!(
            validate(&group),
            Err(MetadataPermissionsError::DmValidation(
                DmValidationError::MustHaveEmptyAdminAndSuperAdmin
            ))
        ));
    }

    let mut single_slot_invalid_permissions = PolicySet::new_dm();
    single_slot_invalid_permissions.add_member_policy = MembershipPolicies::allow();
    for policies in [PolicySet::default(), single_slot_invalid_permissions] {
        let group = make_group(policies, vec![], None);
        assert!(matches!(
            validate(&group),
            Err(MetadataPermissionsError::DmValidation(
                DmValidationError::InvalidPermissions
            ))
        ));
    }

    let own_inbox_admins = TlsSet::from_keys([ComponentInboxId::from_hex(client.inbox_id())?])
        .tls_serialize_detached()?;
    let own_inbox_invalid_role = make_group(
        PolicySet::new_dm(),
        vec![(ComponentId::ADMIN_LIST, Some(own_inbox_admins))],
        None,
    );
    assert!(matches!(
        validate_dm_group(&client.context, &own_inbox_invalid_role, client.inbox_id()),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::MustHaveEmptyAdminAndSuperAdmin
        ))
    ));

    let mut own_inbox_invalid_policy = PolicySet::new_dm();
    own_inbox_invalid_policy.add_member_policy = MembershipPolicies::allow();
    let own_inbox_invalid_policy = make_group(own_inbox_invalid_policy, vec![], None);
    assert!(matches!(
        validate_dm_group(
            &client.context,
            &own_inbox_invalid_policy,
            client.inbox_id()
        ),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::InvalidPermissions
        ))
    ));

    let third_inbox = hex::encode([0x43; 32]);
    let initial = xmtp_mls_group_state::app_data::creation::initial_dictionary(
        InitialGroupKind::Dm {
            target_inbox_id: &added_by_inbox,
        },
        &PolicySet::new_dm().to_proto()?,
        &GroupMetadataOptions::default(),
        client.inbox_id(),
        None,
        &[],
    )?;
    let mut membership_entries =
        xmtp_mls_group_state::app_data::migration::decode_group_membership_dict(
            initial
                .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
                .unwrap(),
        )?;
    membership_entries.insert(
        ComponentInboxId::from_hex(&third_inbox)?,
        xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry {
            version: Some(
                xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version::V1(
                    xmtp_proto::xmtp::mls::message_contents::group_membership_entry::V1 {
                        sequence_id: 0,
                        failed_installations: vec![],
                    },
                ),
            ),
        },
    );
    let foreign_membership = make_group(
        PolicySet::new_dm(),
        vec![(
            ComponentId::GROUP_MEMBERSHIP,
            Some(
                xmtp_mls_group_state::app_data::migration::encode_group_membership_dict(
                    &membership_entries,
                )?,
            ),
        )],
        None,
    );
    assert!(matches!(
        validate(&foreign_membership),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::MemberOutsidePair
        ))
    ));

    let foreign_leaf = make_group(PolicySet::new_dm(), vec![], Some(&third_inbox));
    assert!(matches!(
        validate(&foreign_leaf),
        Err(MetadataPermissionsError::DmValidation(
            DmValidationError::MemberOutsidePair
        ))
    ));
}

#[xmtp_common::test]
async fn test_update_app_data() {
    let amal = EngineTestPeer::new().await;

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
    let amal = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

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

#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_state() {
    let alix = EngineTestPeer::new().await;
    let bola = EngineTestPeer::new().await;

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

/// Each referencing type stores its reference only when it is a 32-byte
/// message id in hexadecimal, in either case. A custom authority or an
/// unknown major version stores none.
// verifies: CTYPE-001, CTYPE-018
#[xmtp_common::test(unwrap_try = true)]
fn reference_ids_are_canonical_message_ids() {
    use xmtp_content_types::{
        CodecError,
        compression::compress,
        delete_message::DeleteMessageCodec,
        reaction::{LegacyReaction, LegacyReactionCodec, ReactionCodec},
        reply::{Reply, ReplyCodec},
        text::TextCodec,
    };
    use xmtp_db::group_message::ContentType;
    use xmtp_proto::xmtp::mls::message_contents::content_types::{DeleteMessage, ReactionV2};
    use xmtp_proto::xmtp::mls::message_contents::{Compression, EncodedContent};

    type Encode = fn(&str) -> Result<EncodedContent, CodecError>;
    let reply: Encode = |reference| {
        ReplyCodec::encode(Reply {
            reference: reference.into(),
            reference_inbox_id: None,
            content: TextCodec::encode("reply".into())?,
        })
    };
    let reaction: Encode = |reference| {
        ReactionCodec::encode(ReactionV2 {
            reference: reference.into(),
            ..Default::default()
        })
    };
    let legacy_reaction: Encode = |reference| {
        LegacyReactionCodec::encode(LegacyReaction {
            action: "added".into(),
            reference: reference.into(),
            reference_inbox_id: None,
            schema: "unicode".into(),
            content: "👍".into(),
        })
    };
    let delete: Encode = |reference| {
        DeleteMessageCodec::encode(DeleteMessage {
            message_id: reference.into(),
        })
    };
    let reference_id =
        |content| super::QueryableContentFields::try_from(content).map(|f| f.reference_id);

    let id = "ab".repeat(32);
    let arms = [
        ("reply", reply),
        ("reaction", reaction),
        ("legacy reaction", legacy_reaction),
        ("delete", delete),
    ];
    for (arm, encode) in arms {
        for (reference, expected) in [
            (id.clone(), Some(vec![0xab; 32])),
            (id.to_uppercase(), Some(vec![0xab; 32])),
            ("0102".into(), None),
            ("ab".repeat(33), None),
            ("zz".repeat(32), None),
            (format!("0x{}", "ab".repeat(31)), None),
        ] {
            assert_eq!(
                reference_id(encode(&reference)?)?,
                expected,
                "{arm} {reference}"
            );
        }
    }
    let compressed_reply = compress(reply(&id)?, Compression::Deflate)?;
    assert_eq!(reference_id(compressed_reply)?, Some(vec![0xab; 32]));
    // The reaction codec refuses to encode this size, so build the envelope.
    let oversized = EncodedContent {
        r#type: Some(ReactionCodec::content_type()),
        content: prost::Message::encode_to_vec(&ReactionV2 {
            reference: "ab".repeat(512 * 1024),
            ..Default::default()
        }),
        ..Default::default()
    };
    let oversized = compress(oversized, Compression::Deflate)?;
    assert_eq!(
        reference_id(oversized)?,
        None,
        "compressed oversized reaction"
    );

    let standard = reply(&id)?;
    let fields = super::QueryableContentFields::try_from(standard.clone())?;
    assert_eq!(fields.content_type, ContentType::Reply);

    let mut custom = standard.clone();
    custom.r#type.as_mut().unwrap().authority_id = "custom.example".into();
    let fields = super::QueryableContentFields::try_from(custom)?;
    assert_eq!(fields.content_type, ContentType::Unknown);
    assert_eq!(fields.reference_id, None);

    let mut unsupported = standard;
    unsupported.r#type.as_mut().unwrap().version_major = 99;
    let fields = super::QueryableContentFields::try_from(unsupported)?;
    assert_eq!(fields.content_type, ContentType::Unknown);
    assert_eq!(fields.reference_id, None);
}

/// Ingest reads a reaction's reference under the reaction limit. A reaction
/// over it, compressed or not and of either major version, stores no
/// reference, so summaries and their counts skip it.
// verifies: CTYPE-034
#[xmtp_common::test(unwrap_try = true)]
fn oversized_reactions_store_no_reference() {
    use prost::Message;
    use xmtp_content_types::{
        ContentCodec,
        compression::compress,
        reaction::{LegacyReaction, LegacyReactionCodec, MAX_REACTION_BYTES, ReactionCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;
    use xmtp_proto::xmtp::mls::message_contents::{Compression, EncodedContent};

    let id = "ab".repeat(32);
    let reference_id =
        |content| super::QueryableContentFields::try_from(content).map(|f| f.reference_id);
    let v2 = |content: String| EncodedContent {
        r#type: Some(ReactionCodec::content_type()),
        content: ReactionV2 {
            reference: id.clone(),
            content,
            ..Default::default()
        }
        .encode_to_vec(),
        ..Default::default()
    };
    let legacy = |content: String| {
        LegacyReactionCodec::encode(LegacyReaction {
            action: "added".into(),
            reference: id.clone(),
            reference_inbox_id: None,
            schema: "custom".into(),
            content,
        })
    };
    // `ReactionV2` bytes are the reference field (66 bytes) and the content
    // field (3 bytes of framing at this size).
    let at_limit = v2("a".repeat(MAX_REACTION_BYTES - 66 - 3));
    assert_eq!(at_limit.content.len(), MAX_REACTION_BYTES);
    assert_eq!(reference_id(at_limit)?, Some(vec![0xab; 32]));
    for oversized in [
        v2("a".repeat(MAX_REACTION_BYTES)),
        compress(v2("a".repeat(1 << 20)), Compression::Deflate)?,
        legacy("a".repeat(MAX_REACTION_BYTES))?,
        compress(legacy("a".repeat(1 << 20))?, Compression::Deflate)?,
    ] {
        assert_eq!(reference_id(oversized)?, None);
    }
}

mod test_dictionary_creation;
mod test_group_id;
mod test_metadata_fields;
mod test_profile_fields;
