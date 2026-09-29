use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::prelude::*;

use crate::context::XmtpSharedContext;
use crate::tester;
use crate::utils::TestMlsGroup;

fn archived_message(
    group_id: xmtp_proto::types::GroupId,
    sender_inbox_id: String,
    marker: u8,
) -> xmtp_db::group_message::StoredGroupMessage {
    use xmtp_db::group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage};

    StoredGroupMessage {
        id: vec![marker; 32],
        group_id,
        decrypted_message_bytes: b"archived message".to_vec(),
        sent_at_ns: xmtp_common::time::now_ns(),
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![marker; 32],
        sender_inbox_id,
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".to_string(),
        reference_id: None,
        sequence_id: 1,
        envelope_hash: None,
        expiry_ns: None,
        expire_at_ns: None,
        inserted_at_ns: 0,
        should_push: false,
        idempotency_key: format!("archived-{marker}"),
    }
}

// verifies: DMS-003, JOIN-080, JOIN-044
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_placeholder_activates_with_archived_history() {
    use xmtp_db::{
        Store,
        group::{ConversationType, GroupMembershipState, QueryGroup, StoredGroup},
    };
    use xmtp_mls_common::group_metadata::DmMembers;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let db = bo.db();
    assert!(db.find_group(&dm.group_id)?.is_none());
    StoredGroup::builder()
        .id(dm.group_id)
        .created_at_ns(xmtp_common::time::now_ns())
        .membership_state(GroupMembershipState::Restored)
        .added_by_inbox_id(alix.inbox_id().to_string())
        .conversation_type(ConversationType::Dm)
        .dm_id(Some(
            DmMembers {
                member_one_inbox_id: alix.inbox_id().to_string(),
                member_two_inbox_id: bo.inbox_id().to_string(),
            }
            .to_string(),
        ))
        .build()?
        .store(&db)?;
    let archived = archived_message(dm.group_id, bo.inbox_id().to_string(), 0x54);
    archived.store(&db)?;
    assert!(
        openmls::group::MlsGroup::load(bo.context.mls_storage(), &dm.group_id.to_openmls())?
            .is_none()
    );

    bo.sync_welcomes().await?;
    let stored = db
        .find_group(&dm.group_id)?
        .expect("Welcome must keep the DM row");
    assert_ne!(stored.membership_state, GroupMembershipState::Restored);
    let welcome_cursor = db.get_last_cursor(
        bo.context.installation_id(),
        xmtp_db::refresh_state::EntityKind::Welcome,
    )?;
    assert_eq!(stored.cursor(), Some(welcome_cursor));
    assert!(db.get_group_message(archived.id)?.is_some());
    assert!(bo.group(&dm.group_id)?.is_active()?);
}

// verifies: DMS-003, JOIN-080, JOIN-044
#[xmtp_common::test(unwrap_try = true)]
async fn restored_dm_backup_stub_activates_with_archived_history() {
    use xmtp_db::{
        Store,
        group::{GroupMembershipState, QueryGroup},
    };

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    TestMlsGroup::create_dm_and_insert(
        &bo.context,
        GroupMembershipState::Restored,
        alix.inbox_id().to_string(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        Some(dm.group_id.as_ref()),
    )?;
    let db = bo.db();
    assert_eq!(
        db.find_group(&dm.group_id)?
            .expect("backup stub")
            .membership_state,
        GroupMembershipState::Restored
    );
    assert!(
        openmls::group::MlsGroup::load(bo.context.mls_storage(), &dm.group_id.to_openmls())?
            .is_some()
    );
    let archived = archived_message(dm.group_id, bo.inbox_id().to_string(), 0x56);
    archived.store(&db)?;

    bo.sync_welcomes().await?;
    let stored = db.find_group(&dm.group_id)?.expect("activated DM");
    assert_ne!(stored.membership_state, GroupMembershipState::Restored);
    let welcome_cursor = db.get_last_cursor(
        bo.context.installation_id(),
        xmtp_db::refresh_state::EntityKind::Welcome,
    )?;
    assert_eq!(stored.cursor(), Some(welcome_cursor));
    assert!(db.get_group_message(archived.id)?.is_some());
    assert!(bo.group(&dm.group_id)?.is_active()?);
}

// verifies: DMS-015
#[xmtp_common::test(unwrap_try = true)]
async fn restored_foreign_pair_rejects_different_welcome_pair() {
    use xmtp_db::group::{GroupMembershipState, QueryGroup};
    use xmtp_mls_common::{group::GroupMetadataOptions, group_metadata::DmMembers};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let foreign = hex::encode([0x43; 32]);
    let historical_pair = DmMembers {
        member_one_inbox_id: alix.inbox_id().to_string(),
        member_two_inbox_id: foreign,
    };
    TestMlsGroup::create_restored_dm_and_insert(
        &bo.context,
        historical_pair.clone(),
        GroupMetadataOptions::default(),
        dm.group_id.as_ref(),
    )?;
    let before = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(
        before.dm_id.as_deref(),
        Some(historical_pair.to_string().as_str())
    );

    let _ = bo.sync_welcomes().await;
    let after = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(after.membership_state, GroupMembershipState::Restored);
    assert_eq!(after.dm_id, before.dm_id);
}

// verifies: DMS-015
#[xmtp_common::test(unwrap_try = true)]
async fn restored_group_rejects_dm_welcome_kind_change() {
    use xmtp_db::group::{ConversationType, GroupMembershipState, QueryGroup};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    TestMlsGroup::insert(
        &bo.context,
        Some(dm.group_id.as_ref()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        crate::groups::group_permissions::PolicySet::default(),
        xmtp_mls_common::group::GroupMetadataOptions::default(),
        None,
        false,
    )?;
    let _ = bo.sync_welcomes().await;
    let stored = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(stored.membership_state, GroupMembershipState::Restored);
    assert_eq!(stored.conversation_type, ConversationType::Group);
}

/// Test case: If two users are talking in a DM, and one user
/// creates a new installation and creates a new DM before being
/// welcomed into the old DM, that new DM group should be consented.
#[xmtp_common::test(unwrap_try = true)]
async fn auto_consent_dms_for_new_installations() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Bo creates a new installation and immediately creates a new DM with alix
    let bo2_dm = bo2.find_or_create_dm(alix.inbox_id(), None).await?;

    // Alix pulls down the new DM from bo
    alix.sync_welcomes().await?;

    // That DM should be already consented, since alix consented with bo in another DM
    let consent = alix
        .get_consent_state(ConsentType::ConversationId, hex::encode(bo2_dm.group_id))
        .await?;
    assert_eq!(consent, ConsentState::Allowed);
}

/// Test case: If a second installation syncs the consent state for a DM
/// before processing the welcome, the welcome should succeed rather than
/// aborting on a unique constraint error.
#[xmtp_common::test(unwrap_try = true)]
async fn test_dm_welcome_with_preexisting_consent() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    let (a_group, _) = alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Mock device sync - the consent record is processed on Bo2 before
    // the welcome is processed.
    let cr = StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(a_group.group_id),
    );
    bo2.context.db().insert_newer_consent_record(cr)?;
    // Now bo2 processes the welcome
    bo1.find_or_create_dm(alix.inbox_id(), None)
        .await?
        .update_installations()
        .await?;
    bo2.sync_welcomes().await?;

    // The welcome should succeed
    assert_eq!(
        bo2.find_or_create_dm(alix.inbox_id(), None).await?.group_id,
        a_group.group_id
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_group_update_dedupes() {
    tester!(alix);
    tester!(bo);

    let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

    let updates = || {
        dm.find_messages(&MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupUpdated]),
            ..Default::default()
        })?
    };
    assert_eq!(updates().len(), 1);

    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 2);

    // The same event in a row will be deduped
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 2);

    // Different time means different update, will not be deduped.
    dm.update_conversation_message_disappear_from_ns(2).await?;
    assert_eq!(updates().len(), 3);

    // Back to 1, will not be deduped because we set it to 2 and back.
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 4);

    // Continue to dedupe because the field did not change.
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 4);
}

fn dictionary_native_dm(
    context: &impl XmtpSharedContext,
    added_by_inbox: &str,
    allow_add_member: bool,
) -> openmls::group::MlsGroup {
    use crate::groups::group_permissions::{MembershipPolicies, PolicySet};
    use xmtp_mls_common::app_data::creation::synthesize_registry_from_policy_set;

    // Build the dictionary directly. DM bootstrap synthesis is a separate task.
    let mut policies = PolicySet::new_dm();
    if allow_add_member {
        policies.add_member_policy = MembershipPolicies::allow();
    }
    let policy_proto = policies.to_proto().unwrap();
    let registry = synthesize_registry_from_policy_set(&policy_proto).unwrap();
    dictionary_native_dm_with_registry(context, added_by_inbox, registry)
}

fn dictionary_native_dm_with_registry(
    context: &impl XmtpSharedContext,
    added_by_inbox: &str,
    registry: xmtp_mls_common::app_data::component_registry::ComponentRegistry,
) -> openmls::group::MlsGroup {
    use openmls::{
        extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions},
        prelude::{Capabilities, CredentialWithKey, ExtensionType, MlsGroupCreateConfig},
    };
    use tls_codec::Serialize;
    use xmtp_mls_common::{
        app_data::{component_id::ComponentId, migration::encode_group_membership_dict},
        inbox_id::InboxId,
        tls_set::TlsSet,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };

    let creator = InboxId::from_hex(added_by_inbox).unwrap();
    let recipient = InboxId::from_hex(context.inbox_id()).unwrap();
    let membership = std::collections::BTreeMap::from([(
        recipient,
        GroupMembershipEntry {
            version: Some(Version::V1(V1 {
                sequence_id: 0,
                failed_installations: vec![],
            })),
        },
    )]);
    let mut dictionary = AppDataDictionary::new();
    for (id, bytes) in [
        (
            ComponentId::COMPONENT_REGISTRY,
            registry.to_bytes().unwrap(),
        ),
        (
            ComponentId::CONVERSATION_TYPE,
            (xmtp_proto::types::ConversationType::Dm as i32)
                .to_be_bytes()
                .to_vec(),
        ),
        (
            ComponentId::CREATOR_INBOX_ID,
            creator.tls_serialize_detached().unwrap(),
        ),
        (
            ComponentId::DM_MEMBERS,
            TlsSet::from_keys([creator, recipient])
                .tls_serialize_detached()
                .unwrap(),
        ),
        (
            ComponentId::GROUP_MEMBERSHIP,
            encode_group_membership_dict(&membership).unwrap(),
        ),
        (
            ComponentId::ADMIN_LIST,
            TlsSet::<InboxId>::new().tls_serialize_detached().unwrap(),
        ),
        (
            ComponentId::SUPER_ADMIN_LIST,
            TlsSet::<InboxId>::new().tls_serialize_detached().unwrap(),
        ),
    ] {
        assert!(dictionary.insert(id.as_u16(), bytes).is_none());
    }
    let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .unwrap();
    let config = MlsGroupCreateConfig::builder()
        .with_group_context_extensions(extensions)
        .capabilities(Capabilities::new(
            None,
            None,
            Some(&[ExtensionType::AppDataDictionary]),
            None,
            None,
        ))
        .ciphersuite(xmtp_cryptography::configuration::CIPHERSUITE)
        .build();
    let identity = context.identity();
    openmls::group::MlsGroup::new(
        &context.mls_provider(),
        &identity.installation_keys,
        &config,
        CredentialWithKey {
            credential: identity.credential(),
            signature_key: identity.installation_keys.public_slice().into(),
        },
    )
    .unwrap()
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_dm_accepts_valid_permissions() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    bo.sync_welcomes().await?;
    let received = bo.group(&dm.group_id)?;
    assert!(received.mutable_metadata()?.admin_list.is_empty());
    assert!(received.mutable_metadata()?.super_admin_list.is_empty());
    assert_eq!(received.members().await?.len(), 2);
    dm.send_message(b"dictionary-native DM", Default::default())
        .await?;
    received.sync().await?;
    assert!(
        received
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| { message.decrypted_message_bytes == b"dictionary-native DM" })
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn canonical_dm_registry_matches_validation_expectation() {
    use crate::groups::group_permissions::PolicySet;
    use xmtp_mls_common::app_data::{
        component_registry::ComponentRegistry,
        migration::synthesize_canonical_subset_from_extensions,
    };
    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let legacy_dm = TestMlsGroup::create_test_dm_group(
        alix.context.clone(),
        added_by.clone(),
        None,
        None,
        None,
        None,
        None,
    )?;
    let entries = legacy_dm.load_mls_group_with_lock(alix.context.mls_storage(), |group| {
        Ok(
            synthesize_canonical_subset_from_extensions(group.extensions())
                .unwrap()
                .expected_registry,
        )
    })?;
    let mut registry = ComponentRegistry::new();
    for (id, metadata) in entries {
        registry.set(id, metadata)?;
    }
    let group = dictionary_native_dm_with_registry(&alix.context, &added_by, registry);
    let actual =
        crate::groups::group_permissions::policy_set_from_dictionary(group.extensions())?.policies;
    let expected = PolicySet::new_dm();
    assert_eq!(actual.add_member_policy, expected.add_member_policy);
    assert_eq!(actual.remove_member_policy, expected.remove_member_policy);
    assert_eq!(actual.add_admin_policy, expected.add_admin_policy);
    assert_eq!(actual.remove_admin_policy, expected.remove_admin_policy);
    crate::groups::validate_dm_group(&alix.context, &group, &added_by)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_dictionary_native_dm_rejects_add_member_policy_alone() {
    use crate::groups::{DmValidationError, MetadataPermissionsError};

    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let group = dictionary_native_dm(&alix.context, &added_by, true);
    let result = crate::groups::validate_dm_group(&alix.context, &group, &added_by);
    assert!(
        matches!(
            result,
            Err(MetadataPermissionsError::DmValidation(
                DmValidationError::InvalidPermissions
            ))
        ),
        "unexpected validation result: {result:?}"
    );
}
