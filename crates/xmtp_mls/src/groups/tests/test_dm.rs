use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::prelude::*;

use crate::context::XmtpSharedContext;
use crate::tester;
use crate::utils::TestMlsGroup;

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
    use xmtp_mls_common::app_data::migration::synthesize_registry_from_policy_set;

    // Build the dictionary directly. DM bootstrap synthesis is a separate task.
    let mut policies = PolicySet::new_dm();
    if allow_add_member {
        policies.add_member_policy = MembershipPolicies::allow();
    }
    let policy_proto = policies.to_proto().unwrap();
    let registry = synthesize_registry_from_policy_set(&policy_proto).unwrap();
    let action_policies = xmtp_proto::xmtp::mls::message_contents::GroupActionPolicies {
        add_member: policy_proto.add_member_policy,
        remove_member: policy_proto.remove_member_policy,
        add_admin: policy_proto.add_admin_policy,
        remove_admin: policy_proto.remove_admin_policy,
        update_permissions: policy_proto.update_permissions_policy,
    };
    dictionary_native_dm_with_action_bytes(
        context,
        added_by_inbox,
        registry,
        prost::Message::encode_to_vec(&action_policies),
    )
}

fn dictionary_native_dm_with_action_bytes(
    context: &impl XmtpSharedContext,
    added_by_inbox: &str,
    registry: xmtp_mls_common::app_data::component_registry::ComponentRegistry,
    action_policies: Vec<u8>,
) -> openmls::group::MlsGroup {
    use openmls::{
        extensions::{AppDataDictionary, AppDataDictionaryExtension, Extension, Extensions},
        prelude::{Capabilities, CredentialWithKey, ExtensionType, MlsGroupCreateConfig},
    };
    use tls_codec::Serialize;
    use xmtp_mls_common::{
        app_data::component_id::ComponentId, inbox_id::InboxId, tls_set::TlsSet,
    };

    let creator = InboxId::from_hex(added_by_inbox).unwrap();
    let recipient = InboxId::from_hex(context.inbox_id()).unwrap();
    let mut dictionary = AppDataDictionary::new();
    for (id, bytes) in [
        (
            ComponentId::COMPONENT_REGISTRY,
            registry.to_bytes().unwrap(),
        ),
        (ComponentId::GROUP_ACTION_POLICIES, action_policies),
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
async fn canonical_dm_action_record_matches_validation_expectation() {
    use crate::groups::group_permissions::PolicySet;
    use prost::Message as _;
    use xmtp_mls_common::app_data::{
        component_id::ComponentId,
        migration::{
            synthesize_canonical_subset_from_extensions, synthesize_registry_from_policy_set,
        },
    };
    use xmtp_proto::xmtp::mls::message_contents::GroupActionPolicies;

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
    let action_bytes = legacy_dm.load_mls_group_with_lock(alix.context.mls_storage(), |group| {
        Ok(
            synthesize_canonical_subset_from_extensions(group.extensions())
                .unwrap()
                .strict
                .get(&ComponentId::GROUP_ACTION_POLICIES)
                .expect("canonical DM bootstrap includes declared actions")
                .1
                .clone(),
        )
    })?;

    let expected = PolicySet::new_dm().to_proto()?;
    let actions = GroupActionPolicies::decode(action_bytes.as_slice())?;
    assert_eq!(actions.add_member, expected.add_member_policy);
    assert_eq!(actions.remove_member, expected.remove_member_policy);
    assert_eq!(actions.add_admin, expected.add_admin_policy);
    assert_eq!(actions.remove_admin, expected.remove_admin_policy);
    assert_eq!(
        actions.update_permissions,
        expected.update_permissions_policy
    );

    let registry = synthesize_registry_from_policy_set(&expected)?;
    let group =
        dictionary_native_dm_with_action_bytes(&alix.context, &added_by, registry, action_bytes);
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
