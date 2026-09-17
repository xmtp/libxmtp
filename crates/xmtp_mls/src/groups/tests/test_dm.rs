use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::prelude::*;

use crate::context::XmtpSharedContext;
use crate::tester;

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
    use crate::groups::group_permissions::{PermissionsPolicies, PolicySet};
    use openmls::{
        extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions},
        prelude::{Capabilities, CredentialWithKey, ExtensionType, MlsGroupCreateConfig},
    };
    use tls_codec::Serialize;
    use xmtp_mls_common::{
        app_data::{component_id::ComponentId, migration::synthesize_registry_from_policy_set},
        inbox_id::InboxId,
        tls_set::TlsSet,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind, MetadataBasePolicy},
    };

    // Build the registry directly. DM bootstrap synthesis is a separate task.
    let mut policies = PolicySet::new_dm();
    policies.add_admin_policy = PermissionsPolicies::allow_if_actor_super_admin();
    policies.remove_admin_policy = PermissionsPolicies::allow_if_actor_super_admin();
    policies.update_permissions_policy = PermissionsPolicies::allow_if_actor_super_admin();
    let mut registry = synthesize_registry_from_policy_set(&policies.to_proto().unwrap()).unwrap();
    let policy = |base| {
        Some(MetadataPolicy {
            kind: Some(Kind::Base(base as i32)),
        })
    };
    if allow_add_member {
        let mut membership = registry
            .get(&ComponentId::GROUP_MEMBERSHIP)
            .unwrap()
            .unwrap();
        membership.permissions.as_mut().unwrap().insert_policy = policy(MetadataBasePolicy::Allow);
        registry
            .set(ComponentId::GROUP_MEMBERSHIP, membership)
            .unwrap();
    }
    let creator = InboxId::from_hex(added_by_inbox).unwrap();
    let recipient = InboxId::from_hex(context.inbox_id()).unwrap();
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
    tester!(alix);
    let added_by = hex::encode([0x42; 32]);
    let group = dictionary_native_dm(&alix.context, &added_by, false);
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
