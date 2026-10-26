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

// verifies: DMS-021, JOIN-080, JOIN-044
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

// verifies: DMS-021, JOIN-080, JOIN-044
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

// verifies: JOIN-089
#[xmtp_common::test(unwrap_try = true)]
async fn restored_foreign_pair_rejects_different_welcome_pair() {
    use xmtp_db::group::{GroupMembershipState, QueryGroup};
    use xmtp_mls_common::group_metadata::DmMembers;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let foreign = hex::encode([0x43; 32]);
    let historical_pair = DmMembers {
        member_one_inbox_id: alix.inbox_id().to_string(),
        member_two_inbox_id: foreign,
    };
    TestMlsGroup::restore_from_archive(
        &bo.context,
        &xmtp_proto::xmtp::device_sync::group_backup::GroupSave {
            id: dm.group_id.to_vec(),
            conversation_type: 2,
            dm_id: Some(historical_pair.to_string()),
            ..Default::default()
        },
    )?;
    let before = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(
        before.dm_id.as_deref(),
        Some(historical_pair.to_string().as_str())
    );

    bo.wait_for_newest_welcome_rejected(xmtp_proto::types::Cursor(0))
        .await;
    let after = bo.db().find_group(&dm.group_id)?.expect("Restored group");
    assert_eq!(after.membership_state, GroupMembershipState::Restored);
    assert_eq!(after.dm_id, before.dm_id);
    // A rejected Welcome keeps the archived record in use.
}

// verifies: JOIN-089
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
    bo.wait_for_newest_welcome_rejected(xmtp_proto::types::Cursor(0))
        .await;
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

/// A DM that `find_or_create_dm` creates passes the joiner's byte
/// comparison of its registry, so creator and joiner agree.
// verifies: DMS-021
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

/// The registry of the newest DM registry profile, which a new DM writes.
fn newest_dm_registry_profile() -> xmtp_mls_common::app_data::component_registry::ComponentRegistry
{
    xmtp_mls_common::app_data::dm_registry_profile::dm_registry_profiles()
        .pop()
        .unwrap()
}

/// The entries of the newest DM registry profile, as raw `(id, bytes)` pairs.
fn standard_dm_registry_entries() -> Vec<(
    xmtp_mls_common::app_data::component_id::ComponentId,
    Vec<u8>,
)> {
    newest_dm_registry_profile()
        .raw_entries()
        .map(|(id, bytes)| (id, bytes.to_vec()))
        .collect()
}

/// The registry a DM creator writes is the newest DM registry profile, so a
/// change to the creator path fails until a new profile is added.
// verifies: DMS-002
#[xmtp_common::test(unwrap_try = true)]
fn dm_creator_writes_the_newest_registry_profile() {
    use crate::groups::group_permissions::PolicySet;
    use xmtp_mls_common::{
        app_data::{
            component_id::ComponentId,
            component_registry::ComponentRegistry,
            creation::{InitialGroupKind, initial_dictionary},
        },
        group::GroupMetadataOptions,
        group_keys::GroupRoot,
    };

    // The arguments of create_dm_and_insert, with no catalogue.
    let dictionary = initial_dictionary(
        InitialGroupKind::Dm {
            target_inbox_id: &hex::encode([0x42; 32]),
        },
        &PolicySet::new_dm().to_proto()?,
        &GroupMetadataOptions::default(),
        &hex::encode([0x43; 32]),
        Some(&GroupRoot::generate()),
        &[],
    )?;
    let written =
        ComponentRegistry::from_bytes(dictionary.get(&ComponentId::COMPONENT_REGISTRY.as_u16())?)?;
    assert_eq!(written, newest_dm_registry_profile());
}

/// A registry holding exactly `entries`, including bytes that do not decode.
fn raw_registry(
    entries: Vec<(
        xmtp_mls_common::app_data::component_id::ComponentId,
        Vec<u8>,
    )>,
) -> xmtp_mls_common::app_data::component_registry::ComponentRegistry {
    use tls_codec::{Serialize, VLBytes};
    use xmtp_mls_common::{app_data::component_registry::ComponentRegistry, tls_map::TlsMap};

    let map: TlsMap<_, _> = entries
        .into_iter()
        .map(|(id, bytes)| (id, VLBytes::new(bytes)))
        .collect();
    ComponentRegistry::from_bytes(&map.tls_serialize_detached().unwrap()).unwrap()
}

/// A DM whose `GROUP_MEMBERSHIP` update policy denies would refuse every
/// later sequence bump of its members, and no participant could repair the
/// entry. The joiner rejects it at the Welcome.
// verifies: DMS-021
#[xmtp_common::test(unwrap_try = true)]
async fn dm_welcome_rejects_registry_that_blocks_membership_updates() {
    use crate::groups::{DmValidationError, MetadataPermissionsError};
    use xmtp_mls_common::app_data::component_id::ComponentId;
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind, MetadataBasePolicy},
    };

    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let mut registry = newest_dm_registry_profile();
    let standard = dictionary_native_dm_with_registry(&alix.context, &added_by, registry.clone());
    crate::groups::validate_dm_group(&alix.context, &standard, &added_by)?;

    let mut membership = registry.get(&ComponentId::GROUP_MEMBERSHIP)?.unwrap();
    membership.permissions.as_mut().unwrap().update_policy = Some(MetadataPolicy {
        kind: Some(Kind::Base(MetadataBasePolicy::Deny as i32)),
    });
    registry.set(ComponentId::GROUP_MEMBERSHIP, membership)?;
    let group = dictionary_native_dm_with_registry(&alix.context, &added_by, registry);
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

/// Every registry entry outside the application range must be byte-equal to
/// the standard DM entry, and no entry may be missing or extra. Entries in
/// the application range are the participants' to change. Every DM registry
/// profile is admitted.
// verifies: DMS-021
#[xmtp_common::test(unwrap_try = true)]
async fn dm_admission_compares_every_protocol_registry_entry() {
    use crate::groups::{DmValidationError, MetadataPermissionsError};
    use prost::Message;
    use xmtp_common::assert_err;
    use xmtp_mls_common::app_data::{
        component_id::ComponentId, dm_registry_profile::dm_registry_profiles,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentMetadata, ComponentPermissions, ComponentType, MetadataPolicy,
        metadata_policy::{Kind, MetadataBasePolicy},
    };

    type Entries = Vec<(ComponentId, Vec<u8>)>;
    type PolicySlot = fn(&mut ComponentPermissions) -> &mut Option<MetadataPolicy>;
    let standard = standard_dm_registry_entries();
    let metadata_of = |id: ComponentId| {
        let (_, bytes) = standard.iter().find(|(entry, _)| *entry == id).unwrap();
        ComponentMetadata::decode(bytes.as_slice()).unwrap()
    };
    // The standard entries with `id` replaced, added, or (for `None`) removed.
    let with = |id: ComponentId, bytes: Option<Vec<u8>>| -> Entries {
        let mut entries: Entries = standard
            .iter()
            .filter(|(entry, _)| *entry != id)
            .cloned()
            .collect();
        entries.extend(bytes.map(|bytes| (id, bytes)));
        entries
    };
    let base = |policy: MetadataBasePolicy| MetadataPolicy {
        kind: Some(Kind::Base(policy as i32)),
    };
    let valid_entry = ComponentMetadata {
        component_type: ComponentType::Bytes as i32,
        permissions: Some(ComponentPermissions {
            insert_policy: Some(base(MetadataBasePolicy::AllowIfSuperAdmin)),
            update_policy: Some(base(MetadataBasePolicy::Deny)),
            delete_policy: Some(base(MetadataBasePolicy::Deny)),
        }),
        external_committer_permissions: None,
    }
    .encode_to_vec();

    let mut rejected: Vec<(String, Entries)> = Vec::new();
    // Each policy of each entry, changed to a value the constrained
    // ADMIN_LIST also accepts.
    let slots: [(&str, PolicySlot); 3] = [
        ("insert", |p| &mut p.insert_policy),
        ("update", |p| &mut p.update_policy),
        ("delete", |p| &mut p.delete_policy),
    ];
    for (id, _) in &standard {
        for (slot, policy_of) in slots {
            let mut metadata = metadata_of(*id);
            let policy = policy_of(metadata.permissions.as_mut().unwrap());
            *policy = Some(if *policy == Some(base(MetadataBasePolicy::Deny)) {
                base(MetadataBasePolicy::AllowIfSuperAdmin)
            } else {
                base(MetadataBasePolicy::Deny)
            });
            rejected.push((
                format!("{id} {slot} policy"),
                with(*id, Some(metadata.encode_to_vec())),
            ));
        }
    }
    let group_image = ComponentMetadata {
        component_type: ComponentType::Bytes as i32,
        ..metadata_of(ComponentId::GROUP_IMAGE_URL)
    };
    let mut external_committer = metadata_of(ComponentId::GROUP_MEMBERSHIP);
    external_committer.external_committer_permissions = external_committer.permissions.clone();
    // Field 15 as a varint: prost skips it on decode, the stored bytes keep it.
    let mut unknown_field = metadata_of(ComponentId::GROUP_MEMBERSHIP).encode_to_vec();
    unknown_field.extend([15 << 3, 1]);
    let unassigned = ComponentId::new(0x800E);
    for (case, id, bytes) in [
        (
            "GROUP_IMAGE added",
            ComponentId::GROUP_IMAGE,
            Some(group_image.encode_to_vec()),
        ),
        (
            "ONESHOT_MESSAGE added",
            ComponentId::ONESHOT_MESSAGE,
            Some(valid_entry.clone()),
        ),
        (
            "USER_DISPLAY_NAME removed",
            ComponentId::USER_DISPLAY_NAME,
            None,
        ),
        (
            "COMMIT_LOG_SIGNER removed",
            ComponentId::COMMIT_LOG_SIGNER,
            None,
        ),
        (
            "external committer permissions",
            ComponentId::GROUP_MEMBERSHIP,
            Some(external_committer.encode_to_vec()),
        ),
        (
            "unknown field",
            ComponentId::GROUP_MEMBERSHIP,
            Some(unknown_field),
        ),
        (
            "valid entry at 0x800E",
            unassigned,
            Some(valid_entry.clone()),
        ),
        (
            "undecodable entry at 0x800E",
            unassigned,
            Some(vec![0xFF; 3]),
        ),
        (
            "entry at reserved 0xFF00",
            ComponentId::new(0xFF00),
            Some(valid_entry.clone()),
        ),
        (
            "entry below the component space",
            ComponentId::new(0x7FFF),
            Some(valid_entry.clone()),
        ),
        (
            "SUPER_ADMIN_LIST entry",
            ComponentId::SUPER_ADMIN_LIST,
            Some(valid_entry.clone()),
        ),
    ] {
        rejected.push((case.into(), with(id, bytes)));
    }

    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let validate = |entries: Entries| {
        let group =
            dictionary_native_dm_with_registry(&alix.context, &added_by, raw_registry(entries));
        crate::groups::validate_dm_group(&alix.context, &group, &added_by)
    };
    for (case, entries) in rejected {
        let result = validate(entries);
        assert_err!(
            result,
            MetadataPermissionsError::DmValidation(DmValidationError::InvalidPermissions),
            "{case}: {result:?}"
        );
    }
    // Both ends of the application range.
    for id in [0xC000, 0xFEFF] {
        validate(with(ComponentId::new(id), Some(valid_entry.clone())))?;
    }
    // Every profile is admitted, so once a second profile exists this
    // catches an admission that accepts only the newest.
    for profile in dm_registry_profiles() {
        validate(
            profile
                .raw_entries()
                .map(|(id, bytes)| (id, bytes.to_vec()))
                .collect(),
        )?;
    }
}
