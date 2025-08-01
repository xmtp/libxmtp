use super::{
    MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH, MAX_GROUP_NAME_LENGTH,
    group_membership::{GroupMembership, MembershipDiff},
    group_permissions::{
        GroupMutablePermissions, GroupMutablePermissionsError, extract_group_permissions,
    },
};
use crate::{
    context::XmtpSharedContext,
    identity_updates::{IdentityUpdates, InstallationDiff, InstallationDiffError},
};
use openmls::{
    credentials::{BasicCredential, Credential as OpenMlsCredential, errors::BasicCredentialError},
    extensions::{Extension, Extensions, UnknownExtension},
    group::{MlsGroup as OpenMlsGroup, StagedCommit},
    messages::proposals::Proposal,
    prelude::{LeafNodeIndex, Sender},
    treesync::LeafNode,
};

use prost::Message;
use serde::Serialize;
use std::collections::HashSet;
use thiserror::Error;
use xmtp_common::{retry::RetryableError, retryable};
use xmtp_db::StorageError;
use xmtp_db::local_commit_log::CommitType;
#[cfg(doc)]
use xmtp_id::associations::AssociationState;
use xmtp_id::{InboxId, associations::MemberIdentifier};
use xmtp_mls_common::{
    group_metadata::{DmMembers, GroupMetadata, GroupMetadataError},
    group_mutable_metadata::{
        GroupMutableMetadata, GroupMutableMetadataError, MetadataField,
        find_mutable_metadata_extension,
    },
};
use xmtp_proto::xmtp::{
    identity::MlsCredential,
    mls::message_contents::{
        GroupMembershipChanges, GroupUpdated as GroupUpdatedProto,
        group_updated::{Inbox as InboxProto, MetadataFieldChange as MetadataFieldChangeProto},
    },
};

#[derive(Debug, Error)]
pub enum CommitValidationError {
    #[error("Actor could not be found")]
    ActorCouldNotBeFound,
    // Subject of the proposal has an invalid credential
    #[error("Inbox validation failed for {0}")]
    InboxValidationFailed(String),
    // Not used yet, but seems obvious enough to include now
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    #[error("Invalid version format: {0}")]
    InvalidVersionFormat(String),
    #[error("Minimum supported protocol version {0} exceeds current version")]
    ProtocolVersionTooLow(String),
    // TODO: We will need to relax this once we support external joins
    #[error("Actor not a member of the group")]
    ActorNotMember,
    #[error("Subject not a member of the group")]
    SubjectDoesNotExist,
    // Current behaviour is to error out if a Commit includes proposals from multiple actors
    // TODO: We should relax this once we support self remove
    #[error("Multiple actors in commit")]
    MultipleActors,
    #[error("Missing group membership")]
    MissingGroupMembership,
    #[error("Missing mutable metadata")]
    MissingMutableMetadata,
    #[error("Unexpected installations added:")]
    UnexpectedInstallationAdded(Vec<Vec<u8>>),
    #[error("Sequence ID can only increase")]
    SequenceIdDecreased,
    #[error("Unexpected installations removed: {0:?}")]
    UnexpectedInstallationsRemoved(Vec<Vec<u8>>),
    #[error(transparent)]
    GroupMetadata(#[from] GroupMetadataError),
    #[error(transparent)]
    MlsCredential(#[from] BasicCredentialError),
    #[error(transparent)]
    GroupMutableMetadata(#[from] GroupMutableMetadataError),
    #[error(transparent)]
    ProtoDecode(#[from] prost::DecodeError),
    #[error(transparent)]
    InstallationDiff(#[from] InstallationDiffError),
    #[error("Failed to parse group mutable permissions: {0}")]
    GroupMutablePermissions(#[from] GroupMutablePermissionsError),
    #[error("PSKs are not support")]
    NoPSKSupport,
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Exceeded max characters for this field. Must be under: {length}")]
    TooManyCharacters { length: usize },
    #[error("Version part missing")]
    VersionMissing,
}

impl RetryableError for CommitValidationError {
    fn is_retryable(&self) -> bool {
        match self {
            CommitValidationError::InstallationDiff(diff_error) => retryable!(diff_error),
            _ => false,
        }
    }
}

#[derive(Clone, PartialEq, Hash, Serialize)]
pub struct CommitParticipant {
    pub inbox_id: String,
    pub installation_id: Vec<u8>,
    pub is_creator: bool,
    pub is_admin: bool,
    pub is_super_admin: bool,
}

impl std::fmt::Debug for CommitParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            inbox_id,
            installation_id,
            is_creator,
            is_admin,
            is_super_admin,
        } = &self;
        write!(
            f,
            "CommitParticipant {{ inbox_id={}, installation_id={}, is_creator={}, is_admin={}, is_super_admin={} }}",
            inbox_id,
            hex::encode(installation_id),
            is_creator,
            is_admin,
            is_super_admin,
        )
    }
}

impl CommitParticipant {
    pub fn build(
        inbox_id: String,
        installation_id: Vec<u8>,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
    ) -> Self {
        let is_creator = inbox_id == immutable_metadata.creator_inbox_id;
        let is_admin = mutable_metadata.is_admin(&inbox_id);
        let is_super_admin = mutable_metadata.is_super_admin(&inbox_id);

        Self {
            inbox_id,
            installation_id,
            is_creator,
            is_admin,
            is_super_admin,
        }
    }

    pub fn from_leaf_node(
        leaf_node: &LeafNode,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
    ) -> Result<Self, CommitValidationError> {
        let inbox_id = inbox_id_from_credential(leaf_node.credential())?;
        let installation_id = leaf_node.signature_key().as_slice().to_vec();

        Ok(Self::build(
            inbox_id,
            installation_id,
            immutable_metadata,
            mutable_metadata,
        ))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MutableMetadataValidationInfo {
    pub metadata_field_changes: Vec<MetadataFieldChange>,
    pub admins_added: Vec<Inbox>,
    pub admins_removed: Vec<Inbox>,
    pub super_admins_added: Vec<Inbox>,
    pub super_admins_removed: Vec<Inbox>,
    pub num_super_admins: u32,
    pub minimum_supported_protocol_version: Option<String>,
}

impl MutableMetadataValidationInfo {
    pub fn is_empty(&self) -> bool {
        self.metadata_field_changes.is_empty()
            && self.admins_added.is_empty()
            && self.admins_removed.is_empty()
            && self.super_admins_added.is_empty()
            && self.super_admins_removed.is_empty()
            && self.minimum_supported_protocol_version.is_none()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Inbox {
    pub inbox_id: String,
    #[allow(dead_code)]
    pub is_creator: bool,
    pub is_admin: bool,
    pub is_super_admin: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetadataFieldChange {
    pub field_name: String,
    #[allow(dead_code)]
    pub old_value: Option<String>,
    #[allow(dead_code)]
    pub new_value: Option<String>,
}

impl MetadataFieldChange {
    pub fn new(field_name: String, old_value: Option<String>, new_value: Option<String>) -> Self {
        Self {
            field_name,
            old_value,
            new_value,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibXMTPVersion {
    major: u32,
    minor: u32,
    patch: u32,
    suffix: Option<String>,
}

impl LibXMTPVersion {
    pub fn parse(version_str: &str) -> Result<Self, CommitValidationError> {
        let parts: Vec<&str> = version_str.split('.').collect();
        if parts.len() != 3 {
            return Err(CommitValidationError::InvalidVersionFormat(
                version_str.to_string(),
            ));
        }

        let major = parts
            .first()
            .ok_or(CommitValidationError::VersionMissing)?
            .parse()
            .map_err(|_| CommitValidationError::InvalidVersionFormat(version_str.to_string()))?;
        let minor = parts
            .get(1)
            .ok_or(CommitValidationError::VersionMissing)?
            .parse()
            .map_err(|_| CommitValidationError::InvalidVersionFormat(version_str.to_string()))?;

        let patch_and_suffix = parts
            .get(2)
            .ok_or(CommitValidationError::VersionMissing)?
            .split('-')
            .collect::<Vec<_>>();

        let patch = patch_and_suffix
            .first()
            .ok_or(CommitValidationError::VersionMissing)?
            .parse()
            .map_err(|_| CommitValidationError::InvalidVersionFormat(version_str.to_string()))?;

        Ok(LibXMTPVersion {
            major,
            minor,
            patch,
            suffix: patch_and_suffix.get(1).map(ToString::to_string),
        })
    }
}

/**
 * A [`ValidatedCommit`] is a summary of changes coming from a MLS commit, after all of our validation rules have been applied
 *
 * Commit Validation Rules:
 * 1. If the `sequence_id` for an inbox has changed, it can only increase
 * 2. The client must create an expected diff of installations added and removed based on the difference between the current
 *    [`GroupMembership`] and the [`GroupMembership`] found in the [`StagedCommit`]
 * 3. Installations may only be added or removed in the commit if they were added/removed in the expected diff
 * 4. For updates (either updating a path or via an Update Proposal) clients must verify that the `installation_id` is
 *    present in the [`AssociationState`] for the `inbox_id` presented in the credential at the `to_sequence_id` found in the
 *    new [`GroupMembership`].
 * 5. All proposals in a commit must come from the same installation
 * 6. No PSK proposals will be allowed
 * 7. New installations may be missing from the commit but still be present in the expected diff.
 * 8. Confirms metadata character limit is not exceeded
 */
#[derive(Debug, Clone, Serialize)]
pub struct ValidatedCommit {
    pub actor: CommitParticipant,
    pub added_inboxes: Vec<Inbox>,
    pub removed_inboxes: Vec<Inbox>,
    pub metadata_validation_info: MutableMetadataValidationInfo,
    pub installations_changed: bool,
    pub permissions_changed: bool,
    pub dm_members: Option<DmMembers<String>>,
}

impl ValidatedCommit {
    pub async fn from_staged_commit(
        context: &impl XmtpSharedContext,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
    ) -> Result<Self, CommitValidationError> {
        let conn = context.db();
        // Get the immutable and mutable metadata
        let extensions = openmls_group.extensions();
        let immutable_metadata: GroupMetadata = extensions.try_into()?;
        let mutable_metadata: GroupMutableMetadata = extensions.try_into()?;
        let group_permissions: GroupMutablePermissions = extensions.try_into()?;
        let current_group_members = get_current_group_members(openmls_group);

        let existing_group_extensions = openmls_group.extensions();
        let new_group_extensions = staged_commit.group_context().extensions();

        let metadata_validation_info = extract_metadata_changes(
            &immutable_metadata,
            &mutable_metadata,
            existing_group_extensions,
            new_group_extensions,
        )?;

        // Enforce character limits for specific metadata fields
        for field_change in &metadata_validation_info.metadata_field_changes {
            if let Some(new_value) = &field_change.new_value {
                match field_change.field_name.as_str() {
                    val if val == MetadataField::Description.as_str()
                        && new_value.len() > MAX_GROUP_DESCRIPTION_LENGTH =>
                    {
                        return Err(CommitValidationError::TooManyCharacters {
                            length: MAX_GROUP_DESCRIPTION_LENGTH,
                        });
                    }
                    val if val == MetadataField::GroupName.as_str()
                        && new_value.len() > MAX_GROUP_NAME_LENGTH =>
                    {
                        return Err(CommitValidationError::TooManyCharacters {
                            length: MAX_GROUP_NAME_LENGTH,
                        });
                    }
                    val if val == MetadataField::GroupImageUrlSquare.as_str()
                        && new_value.len() > MAX_GROUP_IMAGE_URL_LENGTH =>
                    {
                        return Err(CommitValidationError::TooManyCharacters {
                            length: MAX_GROUP_IMAGE_URL_LENGTH,
                        });
                    }
                    _ => {}
                }
            }
        }

        let permissions_changed =
            extract_permissions_changed(&group_permissions, new_group_extensions)?;
        // Get the actor who created the commit.
        // Because we don't allow for multiple actors in a commit, this will error if two proposals come from different authors.
        let actor = extract_actor(
            staged_commit,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
        )?;

        // Block any psk proposals
        if staged_commit.psk_proposals().any(|_| true) {
            return Err(CommitValidationError::NoPSKSupport);
        }

        // Get the installations actually added and removed in the commit
        let ProposalChanges {
            added_installations,
            removed_installations,
            mut credentials_to_verify,
        } = get_proposal_changes(
            staged_commit,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
        )?;

        // Get the expected diff of installations added and removed based on the difference between the current
        // group membership and the new group membership.
        // Also gets back the added and removed inbox ids from the expected diff
        let expected_diff =
            ExpectedDiff::from_staged_commit(context, staged_commit, openmls_group).await?;

        let ExpectedDiff {
            new_group_membership,
            expected_installation_diff,
            added_inboxes,
            removed_inboxes,
        } = expected_diff;

        let installations_changed =
            !added_installations.is_empty() || !removed_installations.is_empty();

        // Ensure that the expected diff matches the added/removed installations in the proposals
        expected_diff_matches_commit(
            &expected_installation_diff,
            added_installations,
            removed_installations,
            current_group_members,
            &new_group_membership.failed_installations,
        )?;
        credentials_to_verify.push(actor.clone());

        // Verify the credentials of the following entities
        // 1. The actor who created the commit
        // 2. Anyone referenced in an update proposal
        // Satisfies Rule 4
        for participant in credentials_to_verify {
            let to_sequence_id = new_group_membership
                .get(&participant.inbox_id)
                .ok_or(CommitValidationError::SubjectDoesNotExist)?;

            let inbox_state = IdentityUpdates::new(&context)
                .get_association_state(&conn, &participant.inbox_id, Some(*to_sequence_id as i64))
                .await
                .map_err(InstallationDiffError::from)?;

            if inbox_state
                .get(&MemberIdentifier::installation(participant.installation_id))
                .is_none()
            {
                return Err(CommitValidationError::InboxValidationFailed(
                    participant.inbox_id,
                ));
            }
        }

        let verified_commit = Self {
            actor,
            added_inboxes,
            removed_inboxes,
            metadata_validation_info,
            installations_changed,
            permissions_changed,
            dm_members: immutable_metadata.dm_members,
        };

        let policy_set = extract_group_permissions(openmls_group)?;
        if !policy_set.policies.evaluate_commit(&verified_commit) {
            return Err(CommitValidationError::InsufficientPermissions);
        }
        if let Some(min_version) = &verified_commit
            .metadata_validation_info
            .minimum_supported_protocol_version
        {
            let current_version = LibXMTPVersion::parse(context.version_info().pkg_version())?;
            let min_supported_version = LibXMTPVersion::parse(min_version)?;
            tracing::info!(
                "Validating commit with min_supported_version: {:?}, current_version: {:?}",
                min_supported_version,
                current_version
            );

            if min_supported_version > current_version {
                return Err(CommitValidationError::ProtocolVersionTooLow(
                    min_version.clone(),
                ));
            }
        }
        Ok(verified_commit)
    }

    // Reuse intent kind here to represent the commit type, even if it's an external commit
    // This is for debugging purposes only, so an approximation is fine
    pub fn debug_commit_type(&self) -> CommitType {
        let metadata_info = &self.metadata_validation_info;
        if !self.added_inboxes.is_empty()
            || !self.removed_inboxes.is_empty()
            || self.installations_changed
        {
            CommitType::UpdateGroupMembership
        } else if self.permissions_changed {
            CommitType::UpdatePermission
        } else if !metadata_info.admins_added.is_empty()
            || !metadata_info.admins_removed.is_empty()
            || !metadata_info.super_admins_added.is_empty()
            || !metadata_info.super_admins_removed.is_empty()
        {
            CommitType::UpdateAdminList
        } else if !metadata_info.metadata_field_changes.is_empty() {
            CommitType::MetadataUpdate
        } else {
            CommitType::KeyUpdate
        }
    }

    pub fn is_empty(&self) -> bool {
        self.added_inboxes.is_empty()
            && self.removed_inboxes.is_empty()
            && self.metadata_validation_info.is_empty()
    }

    pub fn actor_inbox_id(&self) -> InboxId {
        self.actor.inbox_id.clone()
    }

    pub fn actor_installation_id(&self) -> Vec<u8> {
        self.actor.installation_id.clone()
    }
}

impl From<ValidatedCommit> for GroupMembershipChanges {
    fn from(_commit: ValidatedCommit) -> Self {
        // TODO: Use new GroupMembershipChanges

        GroupMembershipChanges {
            members_added: vec![],
            members_removed: vec![],
            installations_added: vec![],
            installations_removed: vec![],
        }
    }
}

struct ProposalChanges {
    added_installations: HashSet<Vec<u8>>,
    removed_installations: HashSet<Vec<u8>>,
    credentials_to_verify: Vec<CommitParticipant>,
}

/**
 * Extracts the installations added and removed via proposals in the commit.
 * Also returns a list of credentials from existing members that need verification (caused by update proposals)
 */
fn get_proposal_changes(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<ProposalChanges, CommitValidationError> {
    // The actual installations added and removed via proposals in the commit
    let mut added_installations: HashSet<Vec<u8>> = HashSet::new();
    let mut removed_installations: HashSet<Vec<u8>> = HashSet::new();
    let mut credentials_to_verify: Vec<CommitParticipant> = vec![];

    for proposal in staged_commit.queued_proposals() {
        match proposal.proposal() {
            // For update proposals, we need to validate that the credential and installation key
            // are valid for the inbox_id in the current group membership state
            Proposal::Update(update_proposal) => {
                credentials_to_verify.push(CommitParticipant::from_leaf_node(
                    update_proposal.leaf_node(),
                    immutable_metadata,
                    mutable_metadata,
                )?);
            }
            // For Add Proposals, all we need to do is validate that the installation_id is in the expected diff
            Proposal::Add(add_proposal) => {
                // We don't need to validate the credential here, since we've already validated it as part of
                // building the expected installation diff
                let leaf_node = add_proposal.key_package().leaf_node();
                let installation_id = leaf_node.signature_key().as_slice().to_vec();
                added_installations.insert(installation_id);
            }
            // For Remove Proposals, all we need to do is validate that the installation_id is in the expected diff
            Proposal::Remove(remove_proposal) => {
                let leaf_node = openmls_group
                    .member_at(remove_proposal.removed())
                    .ok_or(CommitValidationError::SubjectDoesNotExist)?;
                let installation_id = leaf_node.signature_key.to_vec();
                removed_installations.insert(installation_id);
            }
            _ => continue,
        }
    }

    Ok(ProposalChanges {
        added_installations,
        removed_installations,
        credentials_to_verify,
    })
}

/**
 * Extracts the latest `GroupMembership` from the staged commit.
 *
 * Returns an error if the extension is not found.
 */
fn get_latest_group_membership(
    staged_commit: &StagedCommit,
) -> Result<GroupMembership, CommitValidationError> {
    for proposal in staged_commit.queued_proposals() {
        match proposal.proposal() {
            Proposal::GroupContextExtensions(group_context_extensions) => {
                let new_group_membership: GroupMembership =
                    extract_group_membership(group_context_extensions.extensions())?;
                tracing::info!(
                    "Group context extensions proposal found: {:?}",
                    new_group_membership
                );
                return Ok(new_group_membership);
            }
            _ => continue,
        }
    }

    extract_group_membership(staged_commit.group_context().extensions())
}

struct ExpectedDiff {
    new_group_membership: GroupMembership,
    expected_installation_diff: InstallationDiff,
    added_inboxes: Vec<Inbox>,
    removed_inboxes: Vec<Inbox>,
}

impl ExpectedDiff {
    pub(super) async fn from_staged_commit(
        context: &impl XmtpSharedContext,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
    ) -> Result<Self, CommitValidationError> {
        // Get the immutable and mutable metadata
        let extensions = openmls_group.extensions();
        let immutable_metadata: GroupMetadata = extensions.try_into()?;
        let mutable_metadata: GroupMutableMetadata = extensions.try_into()?;

        // Block any psk proposals
        if staged_commit.psk_proposals().any(|_| true) {
            return Err(CommitValidationError::NoPSKSupport);
        }

        let expected_diff = Self::extract_expected_diff(
            context,
            staged_commit,
            extensions,
            &immutable_metadata,
            &mutable_metadata,
        )
        .await?;

        Ok(expected_diff)
    }

    /// Generates an expected diff of installations added and removed based on the difference between the current
    /// [`GroupMembership`] and the [`GroupMembership`] found in the [`StagedCommit`].
    /// This requires loading the Inbox state from the network.
    /// Satisfies Rule 2
    async fn extract_expected_diff(
        context: &impl XmtpSharedContext,
        staged_commit: &StagedCommit,
        existing_group_extensions: &Extensions,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
    ) -> Result<ExpectedDiff, CommitValidationError> {
        let conn = context.db();
        let old_group_membership = extract_group_membership(existing_group_extensions)?;
        let new_group_membership = get_latest_group_membership(staged_commit)?;
        let membership_diff = old_group_membership.diff(&new_group_membership);

        validate_membership_diff(
            &old_group_membership,
            &new_group_membership,
            &membership_diff,
        )?;

        let added_inboxes = membership_diff
            .added_inboxes
            .iter()
            .map(|inbox_id| build_inbox(inbox_id, immutable_metadata, mutable_metadata))
            .collect::<Vec<Inbox>>();

        let removed_inboxes = membership_diff
            .removed_inboxes
            .iter()
            .map(|inbox_id| build_inbox(inbox_id, immutable_metadata, mutable_metadata))
            .collect::<Vec<Inbox>>();
        let identity_updates = IdentityUpdates::new(&context);
        let expected_installation_diff = identity_updates
            .get_installation_diff(
                &conn,
                &old_group_membership,
                &new_group_membership,
                &membership_diff,
            )
            .await?;

        Ok(ExpectedDiff {
            new_group_membership,
            expected_installation_diff,
            added_inboxes,
            removed_inboxes,
        })
    }
}

/// Compare the list of installations added and removed in the commit to the expected diff based on the changes
/// to the inbox state.
/// Satisfies Rule 3 and Rule 7
fn expected_diff_matches_commit(
    expected_diff: &InstallationDiff,
    added_installations: HashSet<Vec<u8>>,
    removed_installations: HashSet<Vec<u8>>,
    existing_installation_ids: HashSet<Vec<u8>>,
    failed_installation_ids: &[Vec<u8>],
) -> Result<(), CommitValidationError> {
    // Check and make sure that any added installations are either:
    // 1. In the expected diff
    // 2. Already a member of the group (for example, the group creator is already a member on the first commit)

    let unknown_adds = added_installations
        .into_iter()
        .filter(|installation_id| {
            !expected_diff.added_installations.contains(installation_id)
                && !existing_installation_ids.contains(installation_id)
        })
        .collect::<Vec<Vec<u8>>>();
    if !unknown_adds.is_empty() {
        return Err(CommitValidationError::UnexpectedInstallationAdded(
            unknown_adds,
        ));
    }

    let filtered_expected: HashSet<_> = expected_diff
        .removed_installations
        .iter()
        .filter(|id| !failed_installation_ids.contains(*id))
        .cloned()
        .collect();

    if removed_installations != filtered_expected {
        let unexpected: Vec<_> = removed_installations
            .difference(&expected_diff.removed_installations)
            .cloned()
            .collect();

        return Err(CommitValidationError::UnexpectedInstallationsRemoved(
            unexpected,
        ));
    }

    Ok(())
}

fn get_current_group_members(openmls_group: &OpenMlsGroup) -> HashSet<Vec<u8>> {
    openmls_group
        .members()
        .map(|member| member.signature_key)
        .collect()
}

/// Validate that the new group membership is a valid state transition from the old group membership.
/// Enforces Rule 1 from above
fn validate_membership_diff(
    old_membership: &GroupMembership,
    new_membership: &GroupMembership,
    diff: &MembershipDiff<'_>,
) -> Result<(), CommitValidationError> {
    for inbox_id in diff.updated_inboxes.iter() {
        let old_sequence_id = old_membership
            .get(inbox_id)
            .ok_or(CommitValidationError::SubjectDoesNotExist)?;
        let new_sequence_id = new_membership
            .get(inbox_id)
            .ok_or(CommitValidationError::SubjectDoesNotExist)?;

        if new_sequence_id.lt(old_sequence_id) {
            return Err(CommitValidationError::SequenceIdDecreased);
        }
    }

    Ok(())
}

/// Extracts the [`CommitParticipant`] from the [`LeafNodeIndex`]
fn extract_commit_participant(
    leaf_index: &LeafNodeIndex,
    group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<CommitParticipant, CommitValidationError> {
    if let Some(leaf_node) = group.member_at(*leaf_index) {
        let installation_id = leaf_node.signature_key.to_vec();
        let inbox_id = inbox_id_from_credential(&leaf_node.credential)?;
        Ok(CommitParticipant::build(
            inbox_id,
            installation_id,
            immutable_metadata,
            mutable_metadata,
        ))
    } else {
        // TODO: Handle external joins/commits
        Err(CommitValidationError::ActorNotMember)
    }
}

/// Get the [`GroupMembership`] from a [`GroupContext`] struct by iterating through all extensions
/// until a match is found
#[tracing::instrument(level = "trace", skip_all)]
pub fn extract_group_membership(
    extensions: &Extensions,
) -> Result<GroupMembership, CommitValidationError> {
    for extension in extensions.iter() {
        if let Extension::Unknown(
            xmtp_mls_common::config::GROUP_MEMBERSHIP_EXTENSION_ID,
            UnknownExtension(group_membership),
        ) = extension
        {
            return Ok(GroupMembership::try_from(group_membership.clone())?);
        }
    }

    Err(CommitValidationError::MissingGroupMembership)
}

/**
 * Extracts the changes to the mutable metadata in the commit.
 *
 * Returns an error if the extension is not found in either the old or new group context.
 */
fn extract_metadata_changes(
    immutable_metadata: &GroupMetadata,
    // We already have the old mutable metadata, so save parsing it a second time
    old_mutable_metadata: &GroupMutableMetadata,
    old_group_extensions: &Extensions,
    new_group_extensions: &Extensions,
) -> Result<MutableMetadataValidationInfo, CommitValidationError> {
    let old_mutable_metadata_ext = find_mutable_metadata_extension(old_group_extensions)
        .ok_or(CommitValidationError::MissingMutableMetadata)?;
    let new_mutable_metadata_ext = find_mutable_metadata_extension(new_group_extensions)
        .ok_or(CommitValidationError::MissingMutableMetadata)?;

    // Before even decoding the new metadata, make sure that something has changed. Otherwise we know there is
    // nothing to do
    if old_mutable_metadata_ext.eq(new_mutable_metadata_ext) {
        let minimum_supported_protocol_version: Option<String> = old_mutable_metadata
            .attributes
            .get(MetadataField::MinimumSupportedProtocolVersion.as_str())
            .map(|s| s.to_string());
        return Ok(MutableMetadataValidationInfo {
            minimum_supported_protocol_version,
            ..Default::default()
        });
    }

    let new_mutable_metadata: GroupMutableMetadata = new_mutable_metadata_ext.try_into()?;

    let metadata_field_changes =
        mutable_metadata_field_changes(old_mutable_metadata, &new_mutable_metadata);

    Ok(MutableMetadataValidationInfo {
        metadata_field_changes,
        admins_added: get_added_members(
            &old_mutable_metadata.admin_list,
            &new_mutable_metadata.admin_list,
            immutable_metadata,
            old_mutable_metadata,
        ),
        admins_removed: get_removed_members(
            &old_mutable_metadata.admin_list,
            &new_mutable_metadata.admin_list,
            immutable_metadata,
            old_mutable_metadata,
        ),
        super_admins_added: get_added_members(
            &old_mutable_metadata.super_admin_list,
            &new_mutable_metadata.super_admin_list,
            immutable_metadata,
            old_mutable_metadata,
        ),
        super_admins_removed: get_removed_members(
            &old_mutable_metadata.super_admin_list,
            &new_mutable_metadata.super_admin_list,
            immutable_metadata,
            old_mutable_metadata,
        ),
        num_super_admins: new_mutable_metadata.super_admin_list.len() as u32,
        minimum_supported_protocol_version: new_mutable_metadata
            .attributes
            .get(MetadataField::MinimumSupportedProtocolVersion.as_str())
            .map(|s| s.to_string()),
    })
}

// Returns true if the permissions have changed, false otherwise
fn extract_permissions_changed(
    old_group_permissions: &GroupMutablePermissions,
    new_group_extensions: &Extensions,
) -> Result<bool, CommitValidationError> {
    let new_group_permissions: GroupMutablePermissions = new_group_extensions.try_into()?;
    Ok(!old_group_permissions.eq(&new_group_permissions))
}

/**
 * Gets the list of inboxes present in the new group membership that are not present in the old group membership.
 */
fn get_added_members(
    old: &[String],
    new: &[String],
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Vec<Inbox> {
    new.iter()
        .filter(|new_inbox| !old.contains(new_inbox))
        .map(|inbox_id| build_inbox(inbox_id, immutable_metadata, mutable_metadata))
        .collect()
}

/**
 * Gets the list of inboxes present in the old group membership that are not present in the new group membership.
 */
fn get_removed_members(
    old: &[String],
    new: &[String],
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Vec<Inbox> {
    old.iter()
        .filter(|old_inbox| !new.contains(old_inbox))
        .map(|inbox_id| build_inbox(inbox_id, immutable_metadata, mutable_metadata))
        .collect()
}

fn build_inbox(
    inbox_id: &String,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Inbox {
    Inbox {
        inbox_id: inbox_id.to_string(),
        is_admin: mutable_metadata.is_admin(inbox_id),
        is_super_admin: mutable_metadata.is_super_admin(inbox_id),
        is_creator: immutable_metadata.creator_inbox_id.eq(inbox_id),
    }
}

/**
 * Extracts the changes to the mutable metadata in the commit.
 */
fn mutable_metadata_field_changes(
    old_metadata: &GroupMutableMetadata,
    new_metadata: &GroupMutableMetadata,
) -> Vec<MetadataFieldChange> {
    let all_keys = old_metadata
        .attributes
        .keys()
        .chain(new_metadata.attributes.keys())
        .fold(HashSet::new(), |mut key_set, key| {
            key_set.insert(key);
            key_set
        });

    all_keys
        .into_iter()
        .filter_map(|key| {
            let old_val = old_metadata.attributes.get(key);
            let new_val = new_metadata.attributes.get(key);
            if old_val.ne(&new_val) {
                Some(MetadataFieldChange::new(
                    key.clone(),
                    old_val.cloned(),
                    new_val.cloned(),
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Extracts the inbox ID from a credential.
fn inbox_id_from_credential(
    credential: &OpenMlsCredential,
) -> Result<String, CommitValidationError> {
    let basic_credential = BasicCredential::try_from(credential.clone())?;
    let identity_bytes = basic_credential.identity();
    let decoded = MlsCredential::decode(identity_bytes)?;

    Ok(decoded.inbox_id)
}

/// Takes a [`StagedCommit`] and tries to extract the actor who created the commit.
/// In the case of a self-update, which does not contain any proposals, this will come from the update_path.
/// In the case of a commit with proposals, it will be the creator of all the proposals.
/// Satisfies Rule 5 by erroring if any proposals have different actors
fn extract_actor(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<CommitParticipant, CommitValidationError> {
    // If there was a path update, get the leaf node that was updated
    let path_update_leaf_node: Option<&LeafNode> = staged_commit.update_path_leaf_node();

    // Iterate through the proposals and get the sender of the proposal.
    // Error if there are multiple senders found
    let proposal_author_leaf_index = staged_commit
        .queued_proposals()
        .try_fold::<Option<&LeafNodeIndex>, _, _>(
            None,
            |existing_value, proposal| match proposal.sender() {
                Sender::Member(member_leaf_node_index) => match existing_value {
                    Some(existing_member) => {
                        if existing_member.ne(member_leaf_node_index) {
                            return Err(CommitValidationError::MultipleActors);
                        }
                        Ok(existing_value)
                    }
                    None => Ok(Some(member_leaf_node_index)),
                },
                _ => Err(CommitValidationError::ActorNotMember),
            },
        )?;

    // If there is both a path update and there are proposals we need to make sure that they are from the same actor
    if let (Some(path_update_leaf_node), Some(proposal_author_leaf_index)) =
        (path_update_leaf_node, proposal_author_leaf_index)
    {
        let proposal_author = openmls_group
            .member_at(*proposal_author_leaf_index)
            .ok_or(CommitValidationError::ActorCouldNotBeFound)?;

        // Verify that the signature keys are the same
        if path_update_leaf_node
            .signature_key()
            .as_slice()
            .to_vec()
            .ne(&proposal_author.signature_key)
        {
            return Err(CommitValidationError::MultipleActors);
        }
    }

    // Convert the path update leaf node to a [`CommitParticipant`]
    if let Some(path_update_leaf_node) = path_update_leaf_node {
        return CommitParticipant::from_leaf_node(
            path_update_leaf_node,
            immutable_metadata,
            mutable_metadata,
        );
    }

    // Convert the proposal author leaf index to a [`CommitParticipant`]
    if let Some(leaf_index) = proposal_author_leaf_index {
        return extract_commit_participant(
            leaf_index,
            openmls_group,
            immutable_metadata,
            mutable_metadata,
        );
    }

    // To get here there must be no path update and no proposals found. This should actually be impossible
    Err(CommitValidationError::ActorCouldNotBeFound)
}

impl From<&MetadataFieldChange> for MetadataFieldChangeProto {
    fn from(change: &MetadataFieldChange) -> Self {
        MetadataFieldChangeProto {
            field_name: change.field_name.clone(),
            old_value: change.old_value.clone(),
            new_value: change.new_value.clone(),
        }
    }
}

impl From<&Inbox> for InboxProto {
    fn from(inbox: &Inbox) -> Self {
        InboxProto {
            inbox_id: inbox.inbox_id.clone(),
        }
    }
}

impl From<ValidatedCommit> for GroupUpdatedProto {
    fn from(commit: ValidatedCommit) -> Self {
        GroupUpdatedProto {
            initiated_by_inbox_id: commit.actor.inbox_id.clone(),
            added_inboxes: commit.added_inboxes.iter().map(InboxProto::from).collect(),
            removed_inboxes: commit
                .removed_inboxes
                .iter()
                .map(InboxProto::from)
                .collect(),
            metadata_field_changes: commit
                .metadata_validation_info
                .metadata_field_changes
                .iter()
                .map(MetadataFieldChangeProto::from)
                .collect(),
        }
    }
}
