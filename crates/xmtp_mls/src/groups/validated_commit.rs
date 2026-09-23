#[cfg(test)]
mod identity_tests;
use crate::{
    context::XmtpSharedContext,
    groups::app_data::bootstrap_validator::BootstrapValidationError,
    identity_updates::{
        IdentityDependencyError, IdentityRequirement, InstallationDiff, InstallationDiffError,
        get_installation_diff_local, require_association_state,
    },
};
use openmls::{
    extensions::Extensions,
    group::{GroupContext, MlsGroup as OpenMlsGroup, QueuedProposal, StagedCommit},
    messages::proposals::{Proposal, ProposalType},
    prelude::{LeafNodeIndex, Sender},
};

use crate::traits::FromWith;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use xmtp_common::{retry::RetryableError, retryable};
use xmtp_db::local_commit_log::CommitType;
use xmtp_db::{DbQuery, StorageError};
#[cfg(doc)]
use xmtp_id::associations::AssociationState;
use xmtp_id::{InboxId, associations::MemberIdentifier};
use xmtp_mls_common::{
    app_data::component_source::ComponentSourceError,
    group_metadata::{DmMembers, GroupMetadata, GroupMetadataError},
    group_mutable_metadata::{GroupMutableMetadata, GroupMutableMetadataError},
    libxmtp_version::{InvalidVersionFormat, LibXMTPVersion},
};
use xmtp_mls_validation::{
    commit::{
        CommitParticipant, CommitRuleError, Inbox, MembershipValidationInfo, MetadataChanges,
        ProposalChanges, build_inbox_with_proposer, extract_commit_participant,
        extract_committer_and_proposers, extract_group_membership, extract_readded_installations,
        get_current_group_members, get_latest_group_membership, get_proposal_changes,
        inbox_id_from_credential, metadata_changes_between, read_committed_metadata,
        read_post_commit_mutable_metadata, reject_psk_proposals,
        validate_app_data_update_proposals_in_commit, validate_identity_sequence_order,
        validate_membership_diff, validate_one_app_data_update,
    },
    group_membership::GroupMembership,
    group_permissions::{GroupMutablePermissionsError, MembershipPolicy, PolicySet},
};
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMembershipChanges, GroupUpdated as GroupUpdatedProto,
    group_updated::{Inbox as InboxProto, MetadataFieldChange as MetadataFieldChangeProto},
};

/// Commit validation failed. [`CommitRuleError`] holds rule violations;
/// the other variants need client state (identity proofs, the database).
#[derive(Debug, Error)]
pub enum CommitValidationError {
    /// Committed local state could not be read. Repair or restore before retry.
    #[error("Committed group state is invalid: {0}")]
    InstalledState(Box<CommitValidationError>),
    /// Resolve this proof outside the state transaction, then reload MLS state.
    #[error(transparent)]
    IdentityDependency(#[from] IdentityDependencyError),
    #[error(transparent)]
    InstallationDiff(#[from] InstallationDiffError),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    /// All bootstrap-commit-validator failures. The bootstrap path runs
    /// only during the one-time AppData migration; isolating its many
    /// failure modes in a sub-enum keeps the steady-state validator's
    /// surface from being dominated by migration-specific noise.
    #[error(transparent)]
    Bootstrap(#[from] BootstrapValidationError),
    /// The commit breaks a validation rule.
    #[error(transparent)]
    Rule(#[from] CommitRuleError),
}

impl crate::worker::NeedsDbReconnect for CommitValidationError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::InstalledState(error) => error.needs_db_reconnect(),
            Self::IdentityDependency(error) => error.needs_db_reconnect(),
            Self::StorageError(error) => error.db_needs_connection(),
            _ => false,
        }
    }
}

impl RetryableError for CommitValidationError {
    fn is_retryable(&self) -> bool {
        match self {
            CommitValidationError::IdentityDependency(error) => retryable!(error),
            CommitValidationError::InstallationDiff(diff_error) => retryable!(diff_error),
            CommitValidationError::Rule(error) => retryable!(error),
            _ => false,
        }
    }
}

impl CommitValidationError {
    /// Mark a local state failure so malformed wire input cannot hide corruption.
    pub(crate) fn installed_state(error: impl Into<Self>) -> Self {
        Self::InstalledState(Box::new(error.into()))
    }

    /// Only authenticated, deterministic invalid input can advance the prefix.
    /// Missing state, failed proofs, unsupported versions, and storage failures
    /// leave the head pending even when their error is not retryable.
    pub(crate) fn is_safe_rejection(&self) -> bool {
        match self {
            Self::IdentityDependency(
                IdentityDependencyError::MissingReference(_)
                | IdentityDependencyError::InvalidSequence(_),
            ) => true,
            Self::Rule(error) => error.is_safe_rejection(),
            Self::Bootstrap(error) => !matches!(
                error,
                BootstrapValidationError::ProtocolVersionTooLow(_)
                    | BootstrapValidationError::Synthesis(_)
            ),
            Self::InstalledState(_)
            | Self::IdentityDependency(_)
            | Self::InstallationDiff(_)
            | Self::StorageError(_) => false,
        }
    }
}

/// Route a rule-layer error type through [`CommitValidationError::Rule`], so
/// `?` keeps working on the calls the validator makes.
macro_rules! from_rule_error {
    ($($source:ty),* $(,)?) => {
        $(impl From<$source> for CommitValidationError {
            fn from(error: $source) -> Self {
                Self::Rule(CommitRuleError::from(error))
            }
        })*
    };
}
from_rule_error!(
    InvalidVersionFormat,
    GroupMetadataError,
    GroupMutableMetadataError,
    GroupMutablePermissionsError,
    ComponentSourceError,
);

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
 * 5. All proposals must come from group members (proposer permissions are validated, not committer)
 * 6. No PSK proposals will be allowed
 * 7. New installations may be missing from the commit but still be present in the expected diff.
 * 8. Confirms metadata character limit is not exceeded
 */
#[derive(Debug, Clone, Serialize)]
pub struct ValidatedCommit {
    /// The actor who created the commit (the committer)
    pub actor: CommitParticipant,
    /// All unique proposers who created proposals in this commit
    pub proposers: Vec<CommitParticipant>,
    pub added_inboxes: Vec<Inbox>,
    pub removed_inboxes: Vec<Inbox>,
    pub readded_installations: HashSet<Vec<u8>>,
    pub metadata_changes: MetadataChanges,
    pub installations_changed: bool,
    pub permissions_changed: bool,
    pub dm_members: Option<DmMembers<String>>,
}

impl ValidatedCommit {
    /// Test helper for commits that have not received an envelope sequence.
    #[cfg(test)]
    pub async fn from_staged_commit(
        context: &impl XmtpSharedContext,
        staged_commit: &StagedCommit,
        committer_leaf_index: LeafNodeIndex,
        openmls_group: &OpenMlsGroup,
    ) -> Result<Self, CommitValidationError> {
        loop {
            match Self::from_staged_commit_local(
                context,
                &context.db(),
                staged_commit,
                committer_leaf_index,
                openmls_group,
                u64::MAX,
            ) {
                Err(CommitValidationError::IdentityDependency(IdentityDependencyError::Need(
                    requirement,
                ))) => {
                    crate::identity_updates::resolve_identity_requirement(context, &requirement)
                        .await?;
                }
                result => return result,
            }
        }
    }

    /// Validate with the caller's write connection and exact cached proofs.
    /// This method cannot fetch identities or call an async signature verifier.
    /// Every referenced identity sequence `N` must precede envelope sequence `S`.
    /// On `Need`, roll back, resolve outside the writer, then reload MLS state.
    pub(crate) fn from_staged_commit_local(
        context: &impl XmtpSharedContext,
        conn: &impl DbQuery,
        staged_commit: &StagedCommit,
        committer_leaf_index: LeafNodeIndex,
        openmls_group: &OpenMlsGroup,
        envelope_sequence: u64,
    ) -> Result<Self, CommitValidationError> {
        // PAUSE BEFORE PARSE: when the group's committed floor already
        // exceeds this client's version, every migrated-state read
        // below (dict-seeded metadata, registry loads, per-proposal
        // dispatch) may encounter wire formats introduced after this
        // version — and any error they raise is a non-retryable
        // rejection, i.e. a fork against above-floor peers. Surface
        // the version gap first so the group pauses and the commit is
        // reprocessed after upgrade. Deliberately reads only the
        // pre-commit dict (committed, already-validated state) — the
        // commit that *raises* the floor is instead paused by the
        // post-policy check at the end of this function, after its
        // super-admin permission has been verified. See
        // `committed_floor_exceeding` for the full rationale.
        if let Some(min_version) =
            xmtp_mls_common::app_data::protocol_floor::committed_floor_exceeding(
                openmls_group,
                context.version_info().pkg_semver(),
            )
        {
            return Err(CommitValidationError::Rule(
                CommitRuleError::ProtocolVersionTooLow(min_version),
            ));
        }
        if staged_commit
            .queued_proposals()
            .any(|queued| matches!(queued.proposal(), Proposal::GroupContextExtensions(_)))
        {
            return Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions),
            ));
        }
        let (immutable_metadata, mutable_metadata) = read_committed_metadata(openmls_group)
            .map_err(CommitValidationError::installed_state)?;

        let group_permissions =
            super::group_permissions::policy_set_from_dictionary(openmls_group.extensions())
                .map_err(|error| {
                    CommitValidationError::installed_state(CommitValidationError::Rule(
                        CommitRuleError::GroupMutablePermissions(error),
                    ))
                })?;
        let current_group_members = get_current_group_members(openmls_group);

        // Reuse the committed registry for all component checks. Apply pending
        // floor updates to this view, then check their policy before pausing.
        let registry = super::app_data::load_component_registry(openmls_group)
            .map_err(CommitValidationError::installed_state)?;
        let min_version_bytes =
                xmtp_mls_common::app_data::component_source::read_post_commit_component_bytes(
                    xmtp_mls_common::app_data::component_id::ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                    openmls_group,
                    staged_commit,
                    &registry,
                )
                .map_err(xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from)?;
        let minimum_supported_protocol_version = match min_version_bytes {
                Some(bytes) => Some(String::from_utf8(bytes).map_err(|e| {
                    CommitValidationError::Rule(CommitRuleError::GroupMutableMetadata(
                        GroupMutableMetadataError::MalformedComponent {
                            component_id: Some(
                                xmtp_mls_common::app_data::component_id::ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                            ),
                            reason: format!("invalid utf-8: {e}"),
                        },
                    ))
                })?),
                None => None,
            };
        // Get the committer who created the commit and all unique proposers.
        // The committer may differ from the proposers (e.g., when one member commits
        // proposals created by other members).
        let (actor, proposers) = extract_committer_and_proposers(
            staged_commit,
            committer_leaf_index,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
        )?;

        reject_psk_proposals(staged_commit)?;

        // AppDataUpdate proposals carried by a commit (inline OR by
        // reference, since `staged_commit.app_data_update_proposals()`
        // iterates both) never flow through `validate_proposal()` —
        // that path only handles standalone proposal-by-reference
        // messages — so this is where their permission check lives.
        validate_app_data_update_proposals_in_commit(
            staged_commit,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
            &registry,
        )?;

        // Get the installations actually added and removed in the commit
        let ProposalChanges {
            mut added_installations,
            mut removed_installations,
            mut credentials_to_verify,
            added_inbox_proposers,
            removed_inbox_proposers,
        } = get_proposal_changes(
            staged_commit,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
        )?;

        // Get the expected diff of installations added and removed based on the difference between the current
        // group membership and the new group membership.
        // Also gets back the added and removed inbox ids from the expected diff
        let expected_diff = ExpectedDiff::from_staged_commit_with_proposers(
            conn,
            staged_commit,
            openmls_group,
            envelope_sequence,
            &added_inbox_proposers,
            &removed_inbox_proposers,
        )?;

        let ExpectedDiff {
            old_group_membership,
            new_group_membership,
            expected_installation_diff,
            added_inboxes,
            removed_inboxes,
        } = expected_diff;

        let installations_changed =
            !added_installations.is_empty() || !removed_installations.is_empty();

        let mut failed_installations: HashSet<Vec<u8>> = new_group_membership
            .failed_installations
            .iter()
            .cloned()
            .collect();

        // A deleted inbox has no post-commit entry to retain its failures.
        // Keep authenticated old failures only when they have no current leaf.
        failed_installations.extend(
            old_group_membership
                .failed_installations
                .iter()
                .filter(|id| !current_group_members.contains(*id))
                .cloned(),
        );

        // Remove readded installations from the added/removed/failed lists before going through validation
        let readded_installations = extract_readded_installations(
            &actor,
            &mut added_installations,
            &mut removed_installations,
            &mut failed_installations,
        );
        // Ensure that the expected diff matches the added/removed installations in the proposals
        expected_diff_matches_commit(
            &expected_installation_diff,
            added_installations,
            removed_installations,
            current_group_members,
            failed_installations,
        )?;
        credentials_to_verify.push(actor.clone());

        // Verify the credentials of the following entities
        // 1. The actor who created the commit
        // 2. Anyone referenced in an update proposal
        // Satisfies Rule 4
        for participant in credentials_to_verify {
            let inbox_id = &participant.inbox_id;
            let sequence_id = match new_group_membership.get(inbox_id) {
                None => {
                    return Err(CommitValidationError::Rule(
                        CommitRuleError::SubjectDoesNotExist,
                    ));
                }
                Some(0) if old_group_membership.get(inbox_id) == Some(&0) => {
                    // An unchanged creation placeholder uses the authenticated
                    // committed leaf. A later identity tip cannot change this
                    // decision. New keys still require an exact nonzero proof.
                    let known_leaf = openmls_group.members().any(|member| {
                        member.signature_key == participant.installation_id
                            && inbox_id_from_credential(&member.credential)
                                .is_ok_and(|known_inbox| known_inbox == *inbox_id)
                    });
                    if !known_leaf {
                        return Err(CommitValidationError::Rule(
                            CommitRuleError::InboxValidationFailed(inbox_id.clone()),
                        ));
                    }
                    continue;
                }
                Some(sequence_id) => *sequence_id,
            };
            let inbox_state = require_association_state(
                conn,
                &IdentityRequirement {
                    inbox_id: inbox_id.clone(),
                    sequence_id,
                },
            )?;

            if inbox_state
                .get(&MemberIdentifier::installation(participant.installation_id))
                .is_none()
            {
                return Err(CommitValidationError::Rule(
                    CommitRuleError::InboxValidationFailed(participant.inbox_id),
                ));
            }
        }

        let membership = MembershipValidationInfo {
            actor: &actor,
            added_inboxes: &added_inboxes,
            removed_inboxes: &removed_inboxes,
            dm_members: immutable_metadata.dm_members.as_ref(),
        };
        if !group_permissions.policies.evaluate_membership(&membership) {
            return Err(CommitValidationError::Rule(
                CommitRuleError::InsufficientPermissions,
            ));
        }
        if let Some(min_version) = &minimum_supported_protocol_version {
            let current_version = context.version_info().pkg_semver();
            let min_supported_version = LibXMTPVersion::parse(min_version)?;
            tracing::info!(
                "Validating commit with min_supported_version: {:?}, current_version: {:?}",
                min_supported_version,
                current_version
            );

            if min_supported_version > *current_version {
                return Err(CommitValidationError::Rule(
                    CommitRuleError::ProtocolVersionTooLow(min_version.clone()),
                ));
            }
        }
        // Component policies have already checked each authenticated proposer.
        // Build the change summary after authorization, so legacy actor-based
        // checks do not reject a valid proposal committed by another member.
        let post_metadata =
            read_post_commit_mutable_metadata(openmls_group, staged_commit, &registry)?;
        let metadata_changes =
            metadata_changes_between(&immutable_metadata, &mutable_metadata, &post_metadata);
        let permissions_changed = staged_commit.app_data_update_proposals().any(|queued| {
            let id = queued.app_data_update_proposal().component_id();
            id == xmtp_mls_common::app_data::component_id::ComponentId::COMPONENT_REGISTRY.as_u16()
        });
        Ok(Self {
            actor,
            proposers,
            added_inboxes,
            removed_inboxes,
            readded_installations,
            metadata_changes,
            installations_changed,
            permissions_changed,
            dm_members: immutable_metadata.dm_members.clone(),
        })
    }

    // Reuse intent kind here to represent the commit type, even if it's an external commit
    // This is for debugging purposes only, so an approximation is fine
    pub fn debug_commit_type(&self) -> CommitType {
        let metadata_info = &self.metadata_changes;
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

/// Membership changes derived from the exact old and proposed identity proofs.
struct ExpectedDiff {
    /// The membership before this commit. The commit cannot change it.
    old_group_membership: GroupMembership,
    /// Proposed inbox sequences. They cannot rewrite the old proof requirements.
    new_group_membership: GroupMembership,
    /// Installation changes authorized by those exact identity snapshots.
    expected_installation_diff: InstallationDiff,
    added_inboxes: Vec<Inbox>,
    removed_inboxes: Vec<Inbox>,
}

impl ExpectedDiff {
    /// Derive installation changes from cached proofs on the caller's connection.
    /// Missing exact proofs return `Need`; this method does not fetch identities.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_staged_commit_with_proposers(
        conn: &impl DbQuery,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
        envelope_sequence: u64,
        added_inbox_proposers: &HashMap<String, CommitParticipant>,
        removed_inbox_proposers: &HashMap<String, CommitParticipant>,
    ) -> Result<Self, CommitValidationError> {
        let extensions = openmls_group.extensions();
        let (immutable_metadata, mutable_metadata) = read_committed_metadata(openmls_group)
            .map_err(CommitValidationError::installed_state)?;

        reject_psk_proposals(staged_commit)?;

        let expected_diff = Self::extract_expected_diff_with_proposers(
            conn,
            staged_commit,
            envelope_sequence,
            extensions,
            &immutable_metadata,
            &mutable_metadata,
            added_inbox_proposers,
            removed_inbox_proposers,
        )?;

        Ok(expected_diff)
    }

    /// Generates an expected diff with proposer attribution for each inbox change.
    /// This is used when validating commits with proposals from multiple members.
    #[allow(clippy::too_many_arguments)]
    fn extract_expected_diff_with_proposers(
        conn: &impl DbQuery,
        staged_commit: &StagedCommit,
        envelope_sequence: u64,
        existing_group_extensions: &Extensions<GroupContext>,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
        added_inbox_proposers: &HashMap<String, CommitParticipant>,
        removed_inbox_proposers: &HashMap<String, CommitParticipant>,
    ) -> Result<ExpectedDiff, CommitValidationError> {
        let old_group_membership = extract_group_membership(existing_group_extensions)
            .map_err(CommitValidationError::installed_state)?;
        let new_group_membership = get_latest_group_membership(staged_commit)?;
        validate_identity_sequence_order(&new_group_membership, envelope_sequence)?;
        let membership_diff = old_group_membership.diff(&new_group_membership);

        validate_membership_diff(
            &old_group_membership,
            &new_group_membership,
            &membership_diff,
        )?;

        let added_inboxes = membership_diff
            .added_inboxes
            .iter()
            .map(|inbox_id| {
                let proposer = added_inbox_proposers
                    .get(inbox_id.as_str())
                    .cloned()
                    .ok_or(CommitValidationError::Rule(
                        CommitRuleError::ProposerNotFound,
                    ))?;
                Ok(build_inbox_with_proposer(
                    inbox_id,
                    immutable_metadata,
                    mutable_metadata,
                    proposer,
                ))
            })
            .collect::<Result<Vec<Inbox>, CommitValidationError>>()?;

        let removed_inboxes = membership_diff
            .removed_inboxes
            .iter()
            .map(|inbox_id| {
                let proposer = removed_inbox_proposers
                    .get(inbox_id.as_str())
                    .cloned()
                    .ok_or(CommitValidationError::Rule(
                        CommitRuleError::ProposerNotFound,
                    ))?;
                Ok(build_inbox_with_proposer(
                    inbox_id,
                    immutable_metadata,
                    mutable_metadata,
                    proposer,
                ))
            })
            .collect::<Result<Vec<Inbox>, CommitValidationError>>()?;

        let expected_installation_diff = get_installation_diff_local(
            conn,
            &old_group_membership,
            &new_group_membership,
            &membership_diff,
        )?;

        Ok(ExpectedDiff {
            old_group_membership,
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
// implements: GMOD-010, GMOD-011
fn expected_diff_matches_commit(
    expected_diff: &InstallationDiff,
    added_installations: HashSet<Vec<u8>>,
    removed_installations: HashSet<Vec<u8>>,
    existing_installation_ids: HashSet<Vec<u8>>,
    failed_installation_ids: HashSet<Vec<u8>>,
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
        return Err(CommitValidationError::Rule(
            CommitRuleError::UnexpectedInstallationAdded(unknown_adds),
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

        return Err(CommitValidationError::Rule(
            CommitRuleError::UnexpectedInstallationsRemoved(unexpected),
        ));
    }

    Ok(())
}

/// Validates a single proposal by checking if the proposer has the required permissions.
/// Returns Ok(()) if the proposal is valid, or an error if validation fails.
///
/// This function should be called when receiving proposals to ensure they are valid
/// before they are stored and later committed.
pub fn validate_proposal(
    proposal: &QueuedProposal,
    openmls_group: &OpenMlsGroup,
    policy_set: &PolicySet,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<(), CommitValidationError> {
    // Extract the proposer from the proposal
    let proposer = match proposal.sender() {
        Sender::Member(leaf_index) => extract_commit_participant(
            leaf_index,
            openmls_group,
            immutable_metadata,
            mutable_metadata,
        )?,
        Sender::External(_) | Sender::NewMemberCommit | Sender::NewMemberProposal => {
            // External and new member proposals are not supported
            return Err(CommitValidationError::Rule(CommitRuleError::ActorNotMember));
        }
    };

    let unsupported_error = || {
        CommitValidationError::Rule(CommitRuleError::UnsupportedProposalType(
            proposal.proposal().proposal_type(),
        ))
    };

    // Validate based on proposal type
    match proposal.proposal() {
        Proposal::Add(add_proposal) => {
            // Check if the proposer has permission to add members
            let added_inbox_id =
                inbox_id_from_credential(add_proposal.key_package().leaf_node().credential())?;
            let inbox = Inbox {
                inbox_id: added_inbox_id.clone(),
                is_creator: false,
                is_admin: false,
                is_super_admin: false,
                proposer: Some(proposer.clone()),
            };
            if !policy_set.add_member_policy.evaluate(&proposer, &inbox) {
                // DM bypass: allow adding the other DM participant even if policy denies
                let is_dm_add = immutable_metadata.dm_members.as_ref().is_some_and(|dm| {
                    (added_inbox_id == dm.member_one_inbox_id.as_ref()
                        || added_inbox_id == dm.member_two_inbox_id.as_ref())
                        && added_inbox_id != proposer.inbox_id
                });
                if !is_dm_add {
                    tracing::warn!(
                        proposer_inbox_id = %proposer.inbox_id,
                        "Proposal rejected: proposer does not have permission to add members"
                    );
                    return Err(CommitValidationError::Rule(
                        CommitRuleError::InsufficientPermissions,
                    ));
                }
            }
        }
        Proposal::Remove(remove_proposal) => {
            // Check if the proposer has permission to remove members
            // Get the inbox_id of the member being removed
            let removed_member = openmls_group.member_at(remove_proposal.removed()).ok_or(
                CommitValidationError::Rule(CommitRuleError::SubjectDoesNotExist),
            )?;
            let removed_inbox_id = inbox_id_from_credential(&removed_member.credential)?;
            let removed_is_admin = mutable_metadata.admin_list.contains(&removed_inbox_id);
            let removed_is_super_admin = mutable_metadata.is_super_admin(&removed_inbox_id);

            // Super admins cannot be removed
            if removed_is_super_admin {
                tracing::warn!(
                    proposer_inbox_id = %proposer.inbox_id,
                    removed_inbox_id = %removed_inbox_id,
                    "Proposal rejected: cannot remove super admin"
                );
                return Err(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions,
                ));
            }

            let removed_inbox = Inbox {
                inbox_id: removed_inbox_id.clone(),
                is_creator: immutable_metadata.creator_inbox_id == removed_inbox_id,
                is_admin: removed_is_admin,
                is_super_admin: removed_is_super_admin,
                proposer: Some(proposer.clone()),
            };

            if !policy_set
                .remove_member_policy
                .evaluate(&proposer, &removed_inbox)
            {
                tracing::warn!(
                    proposer_inbox_id = %proposer.inbox_id,
                    removed_inbox_id = %removed_inbox_id,
                    "Proposal rejected: proposer does not have permission to remove members"
                );
                return Err(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions,
                ));
            }
        }
        Proposal::GroupContextExtensions(_) => return Err(unsupported_error()),
        Proposal::Update(update_proposal) => {
            // Update proposals are allowed for the member themselves, but the new leaf node's
            // credential must match the proposer's identity to prevent identity swaps.
            let new_inbox_id = inbox_id_from_credential(update_proposal.leaf_node().credential())?;
            if new_inbox_id != proposer.inbox_id {
                tracing::warn!(
                    proposer_inbox_id = %proposer.inbox_id,
                    proposer_installation_id = hex::encode(&proposer.installation_id),
                    leaf_index = ?proposal.sender(),
                    new_inbox_id = %new_inbox_id,
                    new_installation_id = hex::encode(update_proposal.leaf_node().signature_key().as_slice()),
                    "Update proposal rejected: new leaf node credential does not match proposer"
                );
                return Err(CommitValidationError::Rule(CommitRuleError::ActorNotMember));
            }
        }
        Proposal::PreSharedKey(_) => {
            return Err(unsupported_error());
        }
        Proposal::ReInit(_) => {
            return Err(unsupported_error());
        }
        Proposal::ExternalInit(_) => {
            return Err(unsupported_error());
        }
        Proposal::Custom(_) => {
            return Err(unsupported_error());
        }
        Proposal::AppDataUpdate(app_data) => {
            use super::app_data::load_component_registry;
            use xmtp_mls_common::app_data::{
                component_id::ComponentId, validation::ActorAuthority,
            };

            let registry = load_component_registry(openmls_group)?;

            // Delegate to the shared helper so the commit-time path
            // (`validate_app_data_update_proposals_in_commit`) and this
            // standalone-proposal-by-reference path can't drift apart.
            validate_one_app_data_update(
                ComponentId::from(app_data.component_id()),
                app_data.operation(),
                ActorAuthority::from(&proposer),
                &proposer.inbox_id,
                &registry,
                openmls_group,
                immutable_metadata.dm_members.as_ref(),
            )?;
        }
        Proposal::AppEphemeral(_) => {
            return Err(unsupported_error());
        }
        Proposal::SelfRemove => {
            return Err(unsupported_error());
        }
    }

    Ok(())
}

// Implement the generic conversion: the TARGET (GroupUpdatedProto) declares what params it needs.
// Here it's `BuildOpts`, but it could be `&dyn Policy`, `&[u8]`, etc.
impl FromWith<ValidatedCommit> for GroupUpdatedProto {
    /// Extra parameter is a list of inbox IDs who requested self-removal (pending removals).
    type Params = Vec<String>;

    fn from_with(commit: ValidatedCommit, pending_removals: &Self::Params) -> Self {
        use std::collections::HashSet;

        // Convert the pending removals list into a set for fast lookup
        let pending_set: HashSet<&str> = pending_removals.iter().map(String::as_str).collect();

        // Partition removed inboxes:
        //  - left_inboxes: those present in pending_removals
        //  - removed_inboxes: all others
        let (left_inboxes, removed_inboxes): (Vec<Inbox>, Vec<Inbox>) = commit
            .removed_inboxes
            .into_iter()
            .partition(|inb| pending_set.contains(inb.inbox_id.as_str()));

        GroupUpdatedProto {
            initiated_by_inbox_id: commit.actor.inbox_id.clone(),
            added_inboxes: commit.added_inboxes.iter().map(InboxProto::from).collect(),
            removed_inboxes: removed_inboxes.iter().map(InboxProto::from).collect(),
            metadata_field_changes: commit
                .metadata_changes
                .metadata_field_changes
                .iter()
                .map(MetadataFieldChangeProto::from)
                .collect(),
            left_inboxes: left_inboxes.iter().map(InboxProto::from).collect(),
            added_admin_inboxes: commit
                .metadata_changes
                .admins_added
                .iter()
                .map(InboxProto::from)
                .collect(),
            removed_admin_inboxes: commit
                .metadata_changes
                .admins_removed
                .iter()
                .map(InboxProto::from)
                .collect(),
            added_super_admin_inboxes: commit
                .metadata_changes
                .super_admins_added
                .iter()
                .map(InboxProto::from)
                .collect(),
            removed_super_admin_inboxes: commit
                .metadata_changes
                .super_admins_removed
                .iter()
                .map(InboxProto::from)
                .collect(),
        }
    }
}

#[cfg(test)]
mod gce_rejection_tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn dictionary_native_group_rejects_gce_proposal_and_commit() {
        use crate::tester;
        tester!(alix, disable_workers);
        let conversation = alix.create_group(None, None)?;
        let provider = alix.context.mls_provider();
        let mut group = OpenMlsGroup::load(
            alix.context.mls_storage(),
            &conversation.group_id.to_openmls(),
        )??;
        let signer = &alix.context.identity().installation_keys;
        group.propose_group_context_extensions(&provider, group.extensions().clone(), signer)?;
        let (immutable, mutable) = read_committed_metadata(&group)?;
        let proposal = group.pending_proposals().next()?;
        assert!(matches!(
            validate_proposal(
                proposal,
                &group,
                &PolicySet::default(),
                &immutable,
                &mutable
            ),
            Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions)
            ))
        ));
        group.commit_to_pending_proposals(&provider, signer)?;
        let staged = group.pending_commit()?;
        assert!(matches!(
            ValidatedCommit::from_staged_commit_local(
                &alix.context,
                &alix.context.db(),
                staged,
                group.own_leaf_index(),
                &group,
                u64::MAX,
            ),
            Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions)
            ))
        ));
    }
}
