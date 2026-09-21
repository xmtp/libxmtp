//! Extension and group-config builders, plus DM validation.

use super::*;

#[cfg(test)]
pub(crate) fn build_protected_metadata_extension(
    creator_inbox_id: &str,
    conversation_type: ConversationType,
    oneshot_message: Option<OneshotMessage>,
) -> Result<Extension, MetadataPermissionsError> {
    assert!(conversation_type != ConversationType::Dm);
    let metadata = GroupMetadata::new(
        conversation_type,
        creator_inbox_id.to_string(),
        None,
        oneshot_message,
    );
    let protected_metadata = Metadata::new(metadata.try_into()?);

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

#[cfg(test)]
pub(in crate::groups) fn build_dm_protected_metadata_extension(
    creator_inbox_id: &str,
    dm_inbox_id: InboxId,
) -> Result<Extension, GroupError> {
    let dm_members = Some(DmMembers {
        member_one_inbox_id: creator_inbox_id.to_string(),
        member_two_inbox_id: dm_inbox_id,
    });

    let metadata = GroupMetadata::new(
        ConversationType::Dm,
        creator_inbox_id.to_string(),
        dm_members,
        None,
    );
    let protected_metadata = Metadata::new(
        metadata
            .try_into()
            .map_err(MetadataPermissionsError::from)?,
    );

    Ok(Extension::ImmutableMetadata(protected_metadata))
}

#[cfg(test)]
pub(crate) fn build_mutable_permissions_extension(
    policies: PolicySet,
) -> Result<Extension, MetadataPermissionsError> {
    let permissions: Vec<u8> = GroupMutablePermissions::new(policies).try_into()?;
    let unknown_gc_extension = UnknownExtension(permissions);

    Ok(Extension::Unknown(
        GROUP_PERMISSIONS_EXTENSION_ID,
        unknown_gc_extension,
    ))
}

pub fn build_mutable_metadata_extension_default(
    creator_inbox_id: &str,
    opts: GroupMetadataOptions,
    commit_log_enabled: bool,
) -> Result<Extension, GroupError> {
    let mut commit_log_signer = None;
    // No signer is minted for a deployment that keeps no commit log.
    if commit_log_enabled {
        // Optional TODO(rich): Plumb in provider and use traits in commit_log_key.rs to generate and store secret
        commit_log_signer = Some(xmtp_cryptography::rand::rand_secret::<ED25519_KEY_LENGTH>());
    }
    let mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new_default(creator_inbox_id.to_string(), commit_log_signer, opts)
            .try_into()
            .map_err(MetadataPermissionsError::from)?;
    let unknown_gc_extension = UnknownExtension(mutable_metadata);

    Ok(Extension::Unknown(
        MUTABLE_METADATA_EXTENSION_ID,
        unknown_gc_extension,
    ))
}

pub fn build_dm_mutable_metadata_extension_default(
    creator_inbox_id: &str,
    dm_target_inbox_id: &str,
    opts: DMMetadataOptions,
    commit_log_enabled: bool,
) -> Result<Extension, MetadataPermissionsError> {
    let mut commit_log_signer = None;
    // No signer is minted for a deployment that keeps no commit log.
    if commit_log_enabled {
        commit_log_signer = Some(xmtp_cryptography::rand::rand_secret::<ED25519_KEY_LENGTH>());
    }
    let mutable_metadata: Vec<u8> = GroupMutableMetadata::new_dm_default(
        creator_inbox_id.to_string(),
        dm_target_inbox_id,
        commit_log_signer,
        opts,
    )
    .try_into()?;
    let unknown_gc_extension = UnknownExtension(mutable_metadata);

    Ok(Extension::Unknown(
        MUTABLE_METADATA_EXTENSION_ID,
        unknown_gc_extension,
    ))
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
    let required_extension_types = &[
        ExtensionType::AppDataDictionary,
        ExtensionType::LastResort,
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

// Legacy extension fixtures remain until the legacy reader tests are removed.
#[cfg(test)]
pub(crate) fn build_legacy_test_group_config(
    protected_metadata_extension: Extension,
    mutable_metadata_extension: Extension,
    group_membership_extension: Extension,
    mutable_permission_extension: Extension,
) -> Result<MlsGroupCreateConfig, GroupError> {
    // Extensions that all group members MUST support (enforced by RequiredCapabilities)
    let required_extension_types = &[
        ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
        ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
        ExtensionType::ImmutableMetadata,
        ExtensionType::LastResort,
        ExtensionType::ApplicationId,
    ];

    // Extensions the creator's leaf node advertises support for (superset of required).
    // `AppDataDictionary` is listed here so the group can be migrated
    // later, but is NOT in required_extension_types so members without
    // support can join the pre-migration legacy group.
    let mut creator_capability_extensions = required_extension_types.to_vec();
    creator_capability_extensions.push(ExtensionType::Unknown(
        WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
    ));
    creator_capability_extensions.push(ExtensionType::Unknown(
        WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID,
    ));
    // AppDataDictionary capability — needed for the bootstrap commit
    // and steady-state AppDataUpdate proposals. Required-vs-supported
    // is enforced by RequiredCapabilities (which adds it only after
    // bootstrap), so advertising here unconditionally is safe and lets
    // a fresh group's creator be the migration trigger.
    creator_capability_extensions.push(ExtensionType::AppDataDictionary);

    let required_proposal_types = &[ProposalType::GroupContextExtensions];

    // Leaf-node-advertised proposal types: a superset of `required_proposal_types`.
    // We advertise `AppDataUpdate` here so that the new path (commit-with-inline-
    // AppDataUpdate-proposal) is supported, but we DO NOT add it to
    // `required_proposal_types` because that would break backwards compatibility
    // with members whose leaf nodes don't yet advertise it. The capability check
    // OpenMLS performs at commit-build time inspects every member leaf node's
    // proposal capabilities — adding it here ensures the creator advertises
    // support; joining clients pick it up via their own key package (see
    // `identity.rs`).
    let creator_capability_proposals = &[
        ProposalType::GroupContextExtensions,
        ProposalType::AppDataUpdate,
    ];

    let capabilities = Capabilities::new(
        None,
        None,
        Some(&creator_capability_extensions),
        Some(creator_capability_proposals),
        None,
    );
    let credentials = &[CredentialType::Basic];

    let required_capabilities =
        Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
            required_extension_types,
            required_proposal_types,
            credentials,
        ));

    let extensions = Extensions::from_vec(vec![
        protected_metadata_extension,
        mutable_metadata_extension,
        group_membership_extension,
        mutable_permission_extension,
        required_capabilities,
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

    // 3) If the inbox that added this group is our inbox, make sure that
    //    one of the `dm_members` is our inbox id
    let identity = context.identity();
    if added_by_inbox == identity.inbox_id() {
        if !(dm_members.member_one_inbox_id == identity.inbox_id()
            || dm_members.member_two_inbox_id == identity.inbox_id())
        {
            return Err(DmValidationError::OurInboxMustBeMember.into());
        }
        return Ok(());
    }

    // 4) Otherwise, make sure one of the `dm_members` is ours, and the other is `added_by_inbox`
    let is_expected_pair = (dm_members.member_one_inbox_id == added_by_inbox
        && dm_members.member_two_inbox_id == identity.inbox_id())
        || (dm_members.member_one_inbox_id == identity.inbox_id()
            && dm_members.member_two_inbox_id == added_by_inbox);

    if !is_expected_pair {
        return Err(DmValidationError::ExpectedInboxesDoNotMatch.into());
    }

    // Validate mutable metadata
    let mutable_metadata =
        app_data::component_source::extract_group_mutable_metadata_capability_aware(mls_group)
            .map_err(|error| match error {
                app_data::component_source::ComponentSourceError::GroupMutableMetadata(inner) => {
                    MetadataPermissionsError::Mutable(inner)
                }
                other => MetadataPermissionsError::ComponentSource(other),
            })?;

    // Check if the admin list and super admin list are empty
    if !mutable_metadata.admin_list.is_empty() || !mutable_metadata.super_admin_list.is_empty() {
        return Err(DmValidationError::MustHaveEmptyAdminAndSuperAdmin.into());
    }

    // Validate permissions so no one adds us to a dm that they can unexpectedly add another member to
    // Note: we don't validate mutable metadata permissions, because they don't affect group membership
    let permissions = group_permissions::policy_set_from_dictionary(mls_group.extensions())?;
    let mut expected_permissions = GroupMutablePermissions::new(PolicySet::new_dm());
    // Dictionary permission updates require a super admin. A DM has none.
    expected_permissions.policies.update_permissions_policy =
        group_permissions::PermissionsPolicies::allow_if_actor_super_admin();

    if permissions.policies.add_member_policy != expected_permissions.policies.add_member_policy
        || permissions.policies.remove_member_policy
            != expected_permissions.policies.remove_member_policy
        || permissions.policies.add_admin_policy != expected_permissions.policies.add_admin_policy
        || permissions.policies.remove_admin_policy
            != expected_permissions.policies.remove_admin_policy
        || permissions.policies.update_permissions_policy
            != expected_permissions.policies.update_permissions_policy
    {
        return Err(DmValidationError::InvalidPermissions.into());
    }

    Ok(())
}
