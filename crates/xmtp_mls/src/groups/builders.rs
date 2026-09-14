//! Extension and group-config builders, plus DM validation.

use super::*;

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
) -> Result<Extension, GroupError> {
    let mut commit_log_signer = None;
    if xmtp_configuration::ENABLE_COMMIT_LOG {
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
) -> Result<Extension, MetadataPermissionsError> {
    let mut commit_log_signer = None;
    if xmtp_configuration::ENABLE_COMMIT_LOG {
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

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_metadata_update(
    group: &OpenMlsGroup,
    field_name: String,
    field_value: String,
) -> Result<Extensions<GroupContext>, MetadataPermissionsError> {
    let existing_metadata: GroupMutableMetadata = group.try_into()?;
    let mut attributes = existing_metadata.attributes.clone();
    attributes.insert(field_name, field_value);
    let new_mutable_metadata: Vec<u8> = GroupMutableMetadata::new(
        attributes,
        existing_metadata.admin_list,
        existing_metadata.super_admin_list,
    )
    .try_into()?;
    let unknown_gc_extension = UnknownExtension(new_mutable_metadata);
    let extension = Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, unknown_gc_extension);
    let mut extensions = group.extensions().clone();
    extensions.add_or_replace(extension)?;
    Ok(extensions)
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_permissions_update(
    group: &OpenMlsGroup,
    update_permissions_intent: UpdatePermissionIntentData,
) -> Result<Extensions<GroupContext>, MetadataPermissionsError> {
    let existing_permissions: GroupMutablePermissions = group.try_into()?;
    let existing_policy_set = existing_permissions.policies.clone();
    let new_policy_set = match update_permissions_intent.update_type {
        PermissionUpdateType::AddMember => PolicySet::new(
            update_permissions_intent.policy_option.into(),
            existing_policy_set.remove_member_policy,
            existing_policy_set.update_metadata_policy,
            existing_policy_set.add_admin_policy,
            existing_policy_set.remove_admin_policy,
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::RemoveMember => PolicySet::new(
            existing_policy_set.add_member_policy,
            update_permissions_intent.policy_option.into(),
            existing_policy_set.update_metadata_policy,
            existing_policy_set.add_admin_policy,
            existing_policy_set.remove_admin_policy,
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::AddAdmin => PolicySet::new(
            existing_policy_set.add_member_policy,
            existing_policy_set.remove_member_policy,
            existing_policy_set.update_metadata_policy,
            update_permissions_intent.policy_option.into(),
            existing_policy_set.remove_admin_policy,
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::RemoveAdmin => PolicySet::new(
            existing_policy_set.add_member_policy,
            existing_policy_set.remove_member_policy,
            existing_policy_set.update_metadata_policy,
            existing_policy_set.add_admin_policy,
            update_permissions_intent.policy_option.into(),
            existing_policy_set.update_permissions_policy,
        ),
        PermissionUpdateType::UpdateMetadata => {
            let mut metadata_policy = existing_policy_set.update_metadata_policy.clone();
            metadata_policy.insert(
                update_permissions_intent
                    .metadata_field_name
                    .ok_or(GroupMutableMetadataError::MissingMetadataField)?,
                update_permissions_intent.policy_option.into(),
            );
            PolicySet::new(
                existing_policy_set.add_member_policy,
                existing_policy_set.remove_member_policy,
                metadata_policy,
                existing_policy_set.add_admin_policy,
                existing_policy_set.remove_admin_policy,
                existing_policy_set.update_permissions_policy,
            )
        }
    };
    let new_group_permissions: Vec<u8> = GroupMutablePermissions::new(new_policy_set).try_into()?;
    let unknown_gc_extension = UnknownExtension(new_group_permissions);
    let extension = Extension::Unknown(GROUP_PERMISSIONS_EXTENSION_ID, unknown_gc_extension);
    let mut extensions = group.extensions().clone();
    extensions.add_or_replace(extension)?;
    Ok(extensions)
}

#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_admin_lists_update(
    group: &OpenMlsGroup,
    admin_lists_update: UpdateAdminListIntentData,
) -> Result<Extensions<GroupContext>, MetadataPermissionsError> {
    let existing_metadata: GroupMutableMetadata = group.try_into()?;
    let attributes = existing_metadata.attributes.clone();
    let mut admin_list = existing_metadata.admin_list;
    let mut super_admin_list = existing_metadata.super_admin_list;
    match admin_lists_update.action_type {
        AdminListActionType::Add => {
            if !admin_list.contains(&admin_lists_update.inbox_id) {
                admin_list.push(admin_lists_update.inbox_id);
            }
        }
        AdminListActionType::Remove => admin_list.retain(|x| x != &admin_lists_update.inbox_id),
        AdminListActionType::AddSuper => {
            if !super_admin_list.contains(&admin_lists_update.inbox_id) {
                super_admin_list.push(admin_lists_update.inbox_id);
            }
        }
        AdminListActionType::RemoveSuper => {
            super_admin_list.retain(|x| x != &admin_lists_update.inbox_id)
        }
    }
    let new_mutable_metadata: Vec<u8> =
        GroupMutableMetadata::new(attributes, admin_list, super_admin_list).try_into()?;
    let unknown_gc_extension = UnknownExtension(new_mutable_metadata);
    let extension = Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, unknown_gc_extension);
    let mut extensions = group.extensions().clone();
    extensions.add_or_replace(extension)?;
    Ok(extensions)
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

/// Check if the given extensions belong to a migrated group.
///
/// "Migrated" means the bootstrap commit completed: the
/// [`ExtensionType::AppDataDictionary`] group-context extension is
/// present and its dict contains a `COMPONENT_REGISTRY` entry. This is
/// also the canonical predicate used by
/// [`crate::groups::app_data::is_migrated_extensions`]; the two
/// converge so the rest of the codebase can use one name where the
/// proposal-by-reference flow is the question and another where the
/// dict-source-of-truth migration is the question.
pub fn check_proposals_enabled(extensions: &Extensions<GroupContext>) -> bool {
    crate::groups::app_data::is_migrated_extensions(extensions)
}

/// Update the `RequiredCapabilities` extension to add or remove
/// `ExtensionType::AppDataDictionary`. Per MLS RFC 9420 §7.2, unknown
/// extensions in the group context MUST be listed in the required
/// capabilities, so this must be called whenever the AppData
/// dictionary extension is added to or removed from the group context.
pub fn update_required_capabilities_for_proposals(
    extensions: &mut Extensions<GroupContext>,
    add: bool,
) -> Result<(), GroupError> {
    let app_data_ext_type = ExtensionType::AppDataDictionary;

    if let Some(required_caps) = extensions.required_capabilities() {
        let mut ext_types: Vec<ExtensionType> = required_caps.extension_types().to_vec();
        let has_it = ext_types.contains(&app_data_ext_type);

        if add && !has_it {
            ext_types.push(app_data_ext_type);
        } else if !add && has_it {
            ext_types.retain(|t| *t != app_data_ext_type);
        } else {
            return Ok(()); // already in desired state
        }

        let new_required = Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
            &ext_types,
            required_caps.proposal_types(),
            required_caps.credential_types(),
        ));
        extensions.add_or_replace(new_required)?;
    }

    Ok(())
}

/// Update RequiredCapabilities for the bootstrap commit:
///   - Add `ExtensionType::AppDataDictionary` to the required
///     extension types so receivers MUST advertise support for the
///     standard AppData dictionary group-context extension.
///   - Remove the four legacy XMTP extension types
///     (`MUTABLE_METADATA`, `GROUP_PERMISSIONS`, `GROUP_MEMBERSHIP`,
///     `ImmutableMetadata`) since the bootstrap commit also removes
///     those extensions from the group context.
///
/// Per MLS RFC 9420 §7.2, every extension present in the group
/// context must also be in `required_capabilities.extension_types`,
/// so the two changes have to land together.
pub fn update_required_capabilities_for_bootstrap(
    extensions: &mut Extensions<GroupContext>,
) -> Result<(), GroupError> {
    let app_data_ext_type = ExtensionType::AppDataDictionary;
    let to_remove: &[ExtensionType] = &[
        ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
        ExtensionType::ImmutableMetadata,
    ];

    if let Some(required_caps) = extensions.required_capabilities() {
        let mut ext_types: Vec<ExtensionType> = required_caps
            .extension_types()
            .iter()
            .copied()
            .filter(|t| !to_remove.contains(t))
            .collect();
        if !ext_types.contains(&app_data_ext_type) {
            ext_types.push(app_data_ext_type);
        }
        let new_required = Extension::RequiredCapabilities(RequiredCapabilitiesExtension::new(
            &ext_types,
            required_caps.proposal_types(),
            required_caps.credential_types(),
        ));
        extensions.add_or_replace(new_required)?;
    }
    Ok(())
}

/// Build extensions with updated group membership for a GroupContextExtensions proposal.
/// This is used when proposing to add or remove members to include the membership update
/// alongside the Add/Remove proposals.
#[tracing::instrument(level = "trace", skip_all)]
pub fn build_extensions_for_membership_update(
    group: &OpenMlsGroup,
    new_membership: &GroupMembership,
) -> Result<Extensions<GroupContext>, GroupError> {
    let mut extensions: Extensions<GroupContext> = group.extensions().clone();
    extensions.add_or_replace(build_group_membership_extension(new_membership))?;
    Ok(extensions)
}

pub(crate) fn build_group_config(
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
    let mutable_metadata: GroupMutableMetadata = mls_group.try_into()?;

    // Check if the admin list and super admin list are empty
    if !mutable_metadata.admin_list.is_empty() || !mutable_metadata.super_admin_list.is_empty() {
        return Err(DmValidationError::MustHaveEmptyAdminAndSuperAdmin.into());
    }

    // Validate permissions so no one adds us to a dm that they can unexpectedly add another member to
    // Note: we don't validate mutable metadata permissions, because they don't affect group membership
    let permissions = extract_group_permissions(mls_group)?;
    let expected_permissions = GroupMutablePermissions::new(PolicySet::new_dm());

    if permissions.policies.add_member_policy != expected_permissions.policies.add_member_policy
        && permissions.policies.remove_member_policy
            != expected_permissions.policies.remove_member_policy
        && permissions.policies.add_admin_policy != expected_permissions.policies.add_admin_policy
        && permissions.policies.remove_admin_policy
            != expected_permissions.policies.remove_admin_policy
        && permissions.policies.update_permissions_policy
            != expected_permissions.policies.update_permissions_policy
    {
        return Err(DmValidationError::InvalidPermissions.into());
    }

    Ok(())
}
