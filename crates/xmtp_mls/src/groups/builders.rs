//! Extension and group-config builders, plus DM validation.

use super::*;
use xmtp_mls_common::app_data::dm_registry_profile::{
    dm_registry_profiles, matches_dm_registry_profile,
};
use xmtp_mls_validation::commit::{extract_group_membership, inbox_id_from_credential};

// implements: DMS-001, ARCH-020
pub(crate) fn parse_canonical_dm_id(dm_id: Option<&str>) -> Result<DmMembers<String>, GroupError> {
    let invalid = || {
        GroupError::from(MetadataPermissionsError::from(
            DmValidationError::StoredDmIdMismatch,
        ))
    };
    let mut parts = dm_id.ok_or_else(invalid)?.split(':');
    if parts.next() != Some("dm") {
        return Err(invalid());
    }
    let first = parts.next().ok_or_else(invalid)?;
    let second = parts.next().ok_or_else(invalid)?;
    if parts.next().is_some() || first == second {
        return Err(invalid());
    }
    for inbox_id in [first, second] {
        let bytes = hex::decode(inbox_id).map_err(|_| invalid())?;
        if bytes.len() != 32 || hex::encode(bytes) != inbox_id {
            return Err(invalid());
        }
    }
    let members = DmMembers {
        member_one_inbox_id: first.to_string(),
        member_two_inbox_id: second.to_string(),
    };
    if members.to_string() != dm_id.unwrap_or_default() {
        return Err(invalid());
    }
    Ok(members)
}

pub fn build_starting_group_membership_extension(inbox_id: &str, sequence_id: u64) -> Extension {
    let mut group_membership = GroupMembership::new();
    group_membership.add(inbox_id.to_string(), sequence_id);
    build_group_membership_extension(&group_membership)
}

pub fn build_group_membership_extension(group_membership: &GroupMembership) -> Extension {
    let unknown_gc_extension = UnknownExtension(group_membership.into());

    Extension::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID, unknown_gc_extension)
}

// implements: META-002, JOIN-040
pub(crate) fn build_group_config(
    dictionary: openmls::extensions::AppDataDictionary,
) -> Result<MlsGroupCreateConfig, GroupError> {
    // No `LastResort`: key packages carry their last-resort mark in the
    // AppDataDictionary extension.
    let required_extension_types = &[
        ExtensionType::AppDataDictionary,
        ExtensionType::ApplicationId,
    ];
    let required_proposal_types = &[ProposalType::AppDataUpdate];
    let mut supported_extensions = required_extension_types.to_vec();
    supported_extensions.extend([
        ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
        ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
    ]);
    let capabilities = Capabilities::new(
        None,
        None,
        Some(&supported_extensions),
        Some(required_proposal_types),
        None,
    );
    let extensions = Extensions::from_vec(vec![
        Extension::AppDataDictionary(openmls::extensions::AppDataDictionaryExtension::new(
            dictionary,
        )),
        Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
            required_extension_types,
            required_proposal_types,
            &[CredentialType::Basic],
        )),
    ])?;
    Ok(MlsGroupCreateConfig::builder()
        .with_group_context_extensions(extensions)
        .capabilities(capabilities)
        .ciphersuite(CIPHERSUITE)
        .wire_format_policy(WireFormatPolicy::default())
        .max_past_epochs(MAX_PAST_EPOCHS)
        .use_ratchet_tree_extension(true)
        .build())
}

pub fn filter_inbox_ids_needing_updates<'a>(
    conn: &impl DbQuery,
    filters: &[(&'a str, i64)],
) -> Result<Vec<&'a str>, xmtp_db::ConnectionError> {
    let existing_sequence_ids =
        conn.get_latest_sequence_id(&filters.iter().map(|f| f.0).collect::<Vec<&str>>())?;

    let needs_update = filters
        .iter()
        .filter_map(|&(inbox_id, seq)| {
            let existing_sequence_id = existing_sequence_ids.get(inbox_id);
            if existing_sequence_id.is_some_and(|&s| s >= seq) {
                return None;
            }

            Some(inbox_id)
        })
        .collect();
    Ok(needs_update)
}

// implements: DMS-021
pub(in crate::groups) fn validate_dm_group(
    context: impl XmtpSharedContext,
    mls_group: &OpenMlsGroup,
    added_by_inbox: &str,
) -> Result<(), MetadataPermissionsError> {
    // Validate dm specific immutable metadata
    let metadata = extract_group_metadata(mls_group.extensions())?;

    // 1) Check if the conversation type is DM
    if metadata.conversation_type != ConversationType::Dm {
        return Err(DmValidationError::InvalidConversationType.into());
    }

    // 2) If `dm_members` is not set, return an error immediately
    let dm_members = match &metadata.dm_members {
        Some(dm) => dm,
        None => {
            return Err(DmValidationError::MustHaveMembersSet.into());
        }
    };

    let identity = context.identity();
    let pair = [
        dm_members.member_one_inbox_id.as_str(),
        dm_members.member_two_inbox_id.as_str(),
    ];
    if pair[0] == pair[1] || !pair.contains(&identity.inbox_id()) {
        return Err(DmValidationError::OurInboxMustBeMember.into());
    }
    if !pair.contains(&added_by_inbox) {
        return Err(DmValidationError::ExpectedInboxesDoNotMatch.into());
    }

    // A valid identity proof does not make an inbox outside this pair a DM member.
    let membership = extract_group_membership(mls_group.extensions())
        .map_err(|_| DmValidationError::InvalidMembership)?;
    if membership
        .members
        .keys()
        .any(|inbox| !pair.contains(&inbox.as_str()))
    {
        return Err(DmValidationError::MemberOutsidePair.into());
    }
    for member in mls_group.members() {
        let inbox = inbox_id_from_credential(&member.credential)
            .map_err(|_| DmValidationError::InvalidMemberCredential)?;
        if !pair.contains(&inbox.as_str()) {
            return Err(DmValidationError::MemberOutsidePair.into());
        }
    }

    // Validate mutable metadata
    let mutable_metadata =
        xmtp_mls_common::app_data::component_source::extract_group_mutable_metadata_capability_aware(mls_group)
            .map_err(|error| match error {
                xmtp_mls_common::app_data::component_source::ComponentSourceError::GroupMutableMetadata(inner) => {
                    MetadataPermissionsError::Mutable(inner)
                }
                other => MetadataPermissionsError::ComponentSource(other),
            })?;

    // Check if the admin list and super admin list are empty
    if !mutable_metadata.admin_list.is_empty() || !mutable_metadata.super_admin_list.is_empty() {
        return Err(DmValidationError::MustHaveEmptyAdminAndSuperAdmin.into());
    }

    // The entries outside the application range must be one DM registry
    // profile, byte for byte.
    let received = app_data::load_component_registry(mls_group)?;
    if !matches_dm_registry_profile(&received, &dm_registry_profiles()) {
        return Err(DmValidationError::InvalidPermissions.into());
    }

    Ok(())
}
