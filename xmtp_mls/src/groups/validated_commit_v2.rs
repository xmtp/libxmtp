use std::collections::HashSet;

use openmls::{
    credentials::{errors::BasicCredentialError, BasicCredential, Credential as OpenMlsCredential},
    extensions::{Extension, UnknownExtension},
    group::{GroupContext, MlsGroup as OpenMlsGroup, StagedCommit},
    messages::proposals::Proposal,
    prelude::{LeafNodeIndex, Sender},
    treesync::LeafNode,
};
use prost::Message;
use thiserror::Error;
#[cfg(doc)]
use xmtp_id::associations::AssociationState;
use xmtp_proto::xmtp::identity::MlsCredential;

use crate::{
    configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
    identity_updates::{InstallationDiff, InstallationDiffError},
    storage::db_connection::DbConnection,
    Client, XmtpApi,
};

use super::{
    group_membership::{GroupMembership, MembershipDiff},
    group_metadata::{extract_group_metadata, GroupMetadata, GroupMetadataError},
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
    #[error("Unexpected installations added: {0:?}")]
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
    ProtoDecode(#[from] prost::DecodeError),
    #[error(transparent)]
    InstallationDiff(#[from] InstallationDiffError),
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub(crate) struct CommitParticipant {
    pub inbox_id: String,
    pub installation_id: Vec<u8>,
    pub is_creator: bool,
    // TODO: Add is_admin
}

impl CommitParticipant {
    pub fn from_leaf_node(
        leaf_node: &LeafNode,
        group_metadata: &GroupMetadata,
    ) -> Result<Self, CommitValidationError> {
        let inbox_id = inbox_id_from_credential(leaf_node.credential())?;
        let is_creator = inbox_id == group_metadata.creator_inbox_id;

        Ok(Self {
            inbox_id,
            installation_id: leaf_node.signature_key().as_slice().to_vec(),
            is_creator,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Inbox {
    pub inbox_id: String,
    pub is_creator: bool,
    // TODO: add is_admin support
    // pub is_admin: bool,
}

/**
 * A [`ValidatedCommit`] is a summary of changes coming from a MLS commit, after all of our validation rules have been applied
 *
 * Commit Validation Rules:
 * 1. If the `sequence_id` for an inbox has changed, it can only increase
 * 2. The client must create an expected diff of installations added and removed based on the difference between the current
 * [`GroupMembership`] and the [`GroupMembership`] found in the [`StagedCommit`]
 * 3. Installations may only be added or removed in the commit if they were added/removed in the expected diff
 * 4. For updates (either updating a path or via an Update Proposal) clients must verify that the `installation_id` is
 * present in the [`AssociationState`] for the `inbox_id` presented in the credential at the `to_sequence_id` found in the
 * new [`GroupMembership`].
 * 5. All proposals in a commit must come from the same installation
 */
#[derive(Debug, Clone)]
pub struct ValidatedCommit {
    pub actor: CommitParticipant,
    pub added_inboxes: Vec<Inbox>,
    pub removed_inboxes: Vec<Inbox>,
}

impl ValidatedCommit {
    pub async fn from_staged_commit<ApiClient: XmtpApi>(
        conn: &DbConnection,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
        client: &Client<ApiClient>,
    ) -> Result<Self, CommitValidationError> {
        // Get the group metadata
        let group_metadata = extract_group_metadata(openmls_group)?;
        // Get the actor who created the commit.
        // Because we don't allow for multiple actors in a commit, this will error if two proposals come from different authors.
        let actor = extract_actor(staged_commit, openmls_group, &group_metadata)?;

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
            openmls_group.export_group_context(),
            staged_commit.group_context(),
            &group_metadata,
        )
        .await?;

        // Get the installations actually added and removed in the commit
        let ProposalChanges {
            added_installations,
            removed_installations,
            mut credentials_to_verify,
        } = get_proposal_changes(staged_commit, openmls_group, &group_metadata)?;

        // Ensure that the expected diff matches the added/removed installations in the proposals
        expected_diff_matches_commit(
            &expected_installation_diff,
            &added_installations,
            &removed_installations,
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
        };

        Ok(verified_commit)
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
    group_metadata: &GroupMetadata,
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
                    group_metadata,
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
async fn extract_expected_diff<ApiClient: XmtpApi>(
    conn: &DbConnection,
    client: &Client<ApiClient>,
    existing_group_context: &GroupContext,
    new_group_context: &GroupContext,
    group_metadata: &GroupMetadata,
) -> Result<ExpectedDiff, CommitValidationError> {
    let old_group_membership = extract_group_membership(existing_group_context)?;
    let new_group_membership = extract_group_membership(new_group_context)?;
    let membership_diff = old_group_membership.diff(&new_group_membership);
    let added_inboxes = membership_diff
        .added_inboxes
        .iter()
        .map(|inbox_id| Inbox {
            inbox_id: inbox_id.to_string(),
            is_creator: *inbox_id == &group_metadata.creator_inbox_id,
        })
        .collect::<Vec<Inbox>>();

    let removed_inboxes = membership_diff
        .removed_inboxes
        .iter()
        .map(|inbox_id| Inbox {
            inbox_id: inbox_id.to_string(),
            is_creator: *inbox_id == &group_metadata.creator_inbox_id,
        })
        .collect::<Vec<Inbox>>();

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
    added_installations: &HashSet<Vec<u8>>,
    removed_installations: &HashSet<Vec<u8>>,
) -> Result<(), CommitValidationError> {
    if added_installations.ne(&expected_diff.added_installations) {
        return Err(CommitValidationError::UnexpectedInstallationAdded(
            added_installations
                .difference(&expected_diff.added_installations)
                .cloned()
                .collect::<Vec<Vec<u8>>>(),
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
    group_metadata: &GroupMetadata,
) -> Result<CommitParticipant, CommitValidationError> {
    if let Some(leaf_node) = group.member_at(*leaf_index) {
        let installation_id = leaf_node.signature_key.to_vec();
        let inbox_id = inbox_id_from_credential(&leaf_node.credential)?;
        let is_creator = inbox_id == group_metadata.creator_inbox_id;

        Ok(CommitParticipant {
            inbox_id,
            installation_id,
            is_creator,
        })
    } else {
        // TODO: Handle external joins/commits
        Err(CommitValidationError::ActorNotMember)
    }
}

/// Get the [`GroupMembership`] from a [`GroupContext`] struct by iterating through all extensions
/// until a match is found
pub fn extract_group_membership(
    group_context: &GroupContext,
) -> Result<GroupMembership, CommitValidationError> {
    for extension in group_context.extensions().iter() {
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
    group_metadata: &GroupMetadata,
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
    if path_update_leaf_node.is_some() && proposal_author_leaf_index.is_some() {
        let proposal_author = openmls_group
            .member_at(*proposal_author_leaf_index.unwrap())
            .ok_or(CommitValidationError::ActorCouldNotBeFound)?;

        // Verify that the signature keys are the same
        if path_update_leaf_node
            .unwrap()
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
        return CommitParticipant::from_leaf_node(path_update_leaf_node, group_metadata);
    }

    // Convert the proposal author leaf index to a [`CommitParticipant`]
    if let Some(leaf_index) = proposal_author_leaf_index {
        return extract_commit_participant(leaf_index, openmls_group, group_metadata);
    }

    // To get here there must be no path update and no proposals found. This should actually be impossible
    Err(CommitValidationError::ActorCouldNotBeFound)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_simple_change() {}
}
