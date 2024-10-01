use std::collections::HashSet;

use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential, Credential as OpenMlsCredential},
    extensions::{Extension, Extensions, UnknownExtension},
    group::{GroupContext, MlsGroup as OpenMlsGroup, StagedCommit},
    messages::proposals::Proposal,
    prelude::{LeafNodeIndex, Sender},
    treesync::LeafNode,
};
use prost::Message;
use thiserror::Error;
#[cfg(doc)]
use xmtp_id::associations::AssociationState;
use xmtp_id::InboxId;
use xmtp_proto::xmtp::{
    identity::MlsCredential,
    mls::message_contents::{
        group_updated::{Inbox as InboxProto, MetadataFieldChange as MetadataFieldChangeProto},
        GroupMembershipChanges, GroupUpdated as GroupUpdatedProto,
    },
};

use crate::{
    configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
    identity_updates::{InstallationDiff, InstallationDiffError},
    retry::RetryableError,
    retryable,
    storage::db_connection::DbConnection,
    Client, XmtpApi,
};

use super::{
    group_membership::{GroupMembership, MembershipDiff},
    group_metadata::{GroupMetadata, GroupMetadataError},
    group_mutable_metadata::{
        find_mutable_metadata_extension, GroupMutableMetadata, GroupMutableMetadataError,
    },
    group_permissions::{
        extract_group_permissions, GroupMutablePermissions, GroupMutablePermissionsError,
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
}

impl RetryableError for CommitValidationError {
    fn is_retryable(&self) -> bool {
        match self {
            CommitValidationError::InstallationDiff(diff_error) => retryable!(diff_error),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct CommitParticipant {
    pub inbox_id: String,
    pub installation_id: Vec<u8>,
    pub is_creator: bool,
    pub is_admin: bool,
    pub is_super_admin: bool,
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

#[derive(Debug, Clone, Default)]
pub struct MutableMetadataChanges {
    pub metadata_field_changes: Vec<MetadataFieldChange>,
    pub admins_added: Vec<Inbox>,
    pub admins_removed: Vec<Inbox>,
    pub super_admins_added: Vec<Inbox>,
    pub super_admins_removed: Vec<Inbox>,
    pub num_super_admins: u32,
}

impl MutableMetadataChanges {
    pub fn is_empty(&self) -> bool {
        self.metadata_field_changes.is_empty()
            && self.admins_added.is_empty()
            && self.admins_removed.is_empty()
            && self.super_admins_added.is_empty()
            && self.super_admins_removed.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct Inbox {
    pub inbox_id: String,
    #[allow(dead_code)]
    pub is_creator: bool,
    pub is_admin: bool,
    pub is_super_admin: bool,
}

#[derive(Debug, Clone)]
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

/**
 * A [`ValidatedCommit`] is a summary of changes coming from a MLS commit, after all of our validation rules have been applied
 *
 * Commit Validation Rules:
 * 1. If the `sequence_id` for an inbox has changed, it can only increase
 * 2. The client must create an expected diff of installations added and removed based on the difference between the current
 *      [`GroupMembership`] and the [`GroupMembership`] found in the [`StagedCommit`]
 * 3. Installations may only be added or removed in the commit if they were added/removed in the expected diff
 * 4. For updates (either updating a path or via an Update Proposal) clients must verify that the `installation_id` is
 *      present in the [`AssociationState`] for the `inbox_id` presented in the credential at the `to_sequence_id` found in the
 *      new [`GroupMembership`].
 * 5. All proposals in a commit must come from the same installation
 * 6. No PSK proposals will be allowed
 */
#[derive(Debug, Clone)]
pub struct ValidatedCommit {
    pub actor: CommitParticipant,
    pub added_inboxes: Vec<Inbox>,
    pub removed_inboxes: Vec<Inbox>,
    pub metadata_changes: MutableMetadataChanges,
    pub permissions_changed: bool,
}

impl ValidatedCommit {
    pub async fn from_staged_commit<ApiClient: XmtpApi>(
        client: &Client<ApiClient>,
        conn: &DbConnection,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
    ) -> Result<Self, CommitValidationError> {
        // Get the immutable and mutable metadata
        let extensions = openmls_group.extensions();
        let immutable_metadata: GroupMetadata = extensions.try_into()?;
        let mutable_metadata: GroupMutableMetadata = extensions.try_into()?;
        let group_permissions: GroupMutablePermissions = extensions.try_into()?;
        let current_group_members = get_current_group_members(openmls_group);

        let existing_group_context = openmls_group.export_group_context();
        let new_group_context = staged_commit.group_context();

        let metadata_changes = extract_metadata_changes(
            &immutable_metadata,
            &mutable_metadata,
            existing_group_context,
            new_group_context,
        )?;

        let permissions_changed =
            extract_permissions_changed(&group_permissions, new_group_context)?;
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
        let ExpectedDiff {
            new_group_membership,
            expected_installation_diff,
            added_inboxes,
            removed_inboxes,
        } = extract_expected_diff(
            conn,
            client,
            staged_commit,
            existing_group_context,
            &immutable_metadata,
            &mutable_metadata,
        )
        .await?;

        // Ensure that the expected diff matches the added/removed installations in the proposals
        expected_diff_matches_commit(
            &expected_installation_diff,
            added_installations,
            removed_installations,
            current_group_members,
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

            let inbox_state = client
                .get_association_state(
                    conn,
                    participant.inbox_id.clone(),
                    Some(*to_sequence_id as i64),
                )
                .await
                .map_err(InstallationDiffError::from)?;

            if inbox_state
                .get(&participant.installation_id.into())
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
            metadata_changes,
            permissions_changed,
        };

        let policy_set = extract_group_permissions(openmls_group)?;
        if !policy_set.policies.evaluate_commit(&verified_commit) {
            return Err(CommitValidationError::InsufficientPermissions);
        }
        Ok(verified_commit)
    }

    pub fn is_empty(&self) -> bool {
        self.added_inboxes.is_empty()
            && self.removed_inboxes.is_empty()
            && self.metadata_changes.is_empty()
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

/// Generates an expected diff of installations added and removed based on the difference between the current
/// [`GroupMembership`] and the [`GroupMembership`] found in the [`StagedCommit`].
/// This requires loading the Inbox state from the network.
/// Satisfies Rule 2
async fn extract_expected_diff<'diff, ApiClient: XmtpApi>(
    conn: &DbConnection,
    client: &Client<ApiClient>,
    staged_commit: &StagedCommit,
    existing_group_context: &GroupContext,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<ExpectedDiff, CommitValidationError> {
    let old_group_membership = extract_group_membership(existing_group_context.extensions())?;
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

    tracing::debug!("\n\n------------------------GETTING INSTALLATION DIFF ------------------------------------\n\n");
    let expected_installation_diff = client
        .get_installation_diff(
            conn,
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

/// Compare the list of installations added and removed in the commit to the expected diff based on the changes
/// to the inbox state.
/// Satisfies Rule 3
fn expected_diff_matches_commit(
    expected_diff: &InstallationDiff,
    added_installations: HashSet<Vec<u8>>,
    removed_installations: HashSet<Vec<u8>>,
    existing_installation_ids: HashSet<Vec<u8>>,
) -> Result<(), CommitValidationError> {
    // Check and make sure that any added installations are either:
    // 1. In the expected diff
    // 2. Already a member of the group (for example, the group creator is already a member on the first commit)

    // TODO: Replace this logic with something else
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

    if removed_installations.ne(&expected_diff.removed_installations) {
        return Err(CommitValidationError::UnexpectedInstallationsRemoved(
            removed_installations
                .difference(&expected_diff.removed_installations)
                .cloned()
                .collect::<Vec<Vec<u8>>>(),
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
            GROUP_MEMBERSHIP_EXTENSION_ID,
            UnknownExtension(group_membership),
        ) = extension
        {
            return Ok(GroupMembership::try_from(group_membership.clone())?);
        }
    }

    Err(CommitValidationError::MissingGroupMembership)
}

fn extract_metadata_changes(
    immutable_metadata: &GroupMetadata,
    // We already have the old mutable metadata, so save parsing it a second time
    old_mutable_metadata: &GroupMutableMetadata,
    old_group_context: &GroupContext,
    new_group_context: &GroupContext,
) -> Result<MutableMetadataChanges, CommitValidationError> {
    let old_mutable_metadata_ext = find_mutable_metadata_extension(old_group_context.extensions())
        .ok_or(CommitValidationError::MissingMutableMetadata)?;
    let new_mutable_metadata_ext = find_mutable_metadata_extension(new_group_context.extensions())
        .ok_or(CommitValidationError::MissingMutableMetadata)?;

    // Before even decoding the new metadata, make sure that something has changed. Otherwise we know there is
    // nothing to do
    if old_mutable_metadata_ext.eq(new_mutable_metadata_ext) {
        return Ok(MutableMetadataChanges::default());
    }

    let new_mutable_metadata: GroupMutableMetadata = new_mutable_metadata_ext.try_into()?;

    let metadata_field_changes =
        mutable_metadata_field_changes(old_mutable_metadata, &new_mutable_metadata);

    Ok(MutableMetadataChanges {
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
    })
}

// Returns true if the permissions have changed, false otherwise
fn extract_permissions_changed(
    old_group_permissions: &GroupMutablePermissions,
    new_group_context: &GroupContext,
) -> Result<bool, CommitValidationError> {
    let new_group_permissions: GroupMutablePermissions =
        new_group_context.extensions().try_into()?;
    Ok(!old_group_permissions.eq(&new_group_permissions))
}

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
                .metadata_changes
                .metadata_field_changes
                .iter()
                .map(MetadataFieldChangeProto::from)
                .collect(),
        }
    }
}
