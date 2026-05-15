use super::{
    MAX_APP_DATA_LENGTH, MAX_GROUP_DESCRIPTION_LENGTH, MAX_GROUP_IMAGE_URL_LENGTH,
    MAX_GROUP_NAME_LENGTH,
    group_membership::{GroupMembership, MembershipDiff},
    group_permissions::{
        GroupMutablePermissions, GroupMutablePermissionsError, MembershipPolicy, MetadataPolicy,
        PermissionsPolicy, PolicySet, extract_group_permissions,
    },
};
use crate::{
    context::XmtpSharedContext,
    identity_updates::{IdentityUpdates, InstallationDiff, InstallationDiffError},
};
use openmls::{
    credentials::{BasicCredential, Credential as OpenMlsCredential, errors::BasicCredentialError},
    extensions::{Extension, Extensions, UnknownExtension},
    group::{GroupContext, MlsGroup as OpenMlsGroup, QueuedProposal, StagedCommit},
    messages::proposals::{Proposal, ProposalType},
    prelude::{LeafNodeIndex, Sender},
    treesync::LeafNode,
};

use crate::traits::FromWith;
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
    #[error("PSKs are not supported")]
    NoPSKSupport,
    #[error("Unsupported proposal type: {0:?}")]
    UnsupportedProposalType(ProposalType),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error("Exceeded max characters for this field. Must be under: {length}")]
    TooManyCharacters { length: usize },
    #[error("Proposer could not be determined for inbox change in proposal-enabled group")]
    ProposerNotFound,
    #[error("Proposals are not enabled on this group")]
    ProposalsNotEnabled,
    /// A well-known component value in the AppData dictionary failed
    /// to decode while validating an AppDataUpdate proposal — most
    /// commonly a malformed `COMPONENT_REGISTRY`. Treated as a
    /// terminal wire-format violation so the offending commit is
    /// rejected rather than silently downgraded to "empty registry"
    /// (which would let a permissive validator state slip in).
    #[error(transparent)]
    ComponentSource(#[from] super::app_data::component_source::ComponentSourceError),

    /// All bootstrap-commit-validator failures. The bootstrap path runs
    /// only during the one-time AppData migration; isolating its many
    /// failure modes in a sub-enum keeps the steady-state validator's
    /// surface from being dominated by migration-specific noise.
    #[error(transparent)]
    Bootstrap(#[from] super::app_data::bootstrap_validator::BootstrapValidationError),
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

    /// Project this participant into the admin/super-admin view that the
    /// component-permission validator consumes.
    fn actor_authority(&self) -> xmtp_mls_common::app_data::validation::ActorAuthority {
        xmtp_mls_common::app_data::validation::ActorAuthority {
            is_admin: self.is_admin,
            is_super_admin: self.is_super_admin,
        }
    }
}

impl From<&CommitParticipant> for xmtp_mls_common::app_data::validation::ActorAuthority {
    fn from(participant: &CommitParticipant) -> Self {
        participant.actor_authority()
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
    /// The proposer who requested this inbox change (if from a proposal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposer: Option<CommitParticipant>,
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

/// Wrapper around [`semver::Version`] used for the
/// `MIN_SUPPORTED_PROTOCOL_VERSION` floor and related min-version checks.
///
/// Delegates parsing and ordering to the [`semver`] crate so behavior
/// matches the semver 2.0 spec — most importantly:
///
/// * Pre-release versions sort *before* the release: `1.0.0-alpha <
///   1.0.0-beta < 1.0.0`. The previous hand-rolled implementation got
///   this backwards (`1.0.0 < 1.0.0-alpha`), which would silently
///   pause clients running release builds against any group floor set
///   by a caller passing a pre-release string.
/// * Pre-release identifiers compare numerically when all-digits, so
///   `rc2 < rc10` instead of lexicographic `rc10 < rc2`.
/// * Multi-segment pre-release tags like `1.0.0-alpha.1` parse cleanly
///   instead of failing with `InvalidVersionFormat`.
/// * Build metadata (after `+`) parses cleanly. Note: the [`semver`]
///   crate's `Ord` impl deliberately *includes* build metadata for
///   total-ordering / `Hash` consistency, deviating from semver 2.0
///   §10 ("build metadata MUST be ignored when determining version
///   precedence"). Irrelevant in practice — `CARGO_PKG_VERSION` and
///   the application-facing `update_group_min_version` callers never
///   pass `+`-suffixed input.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibXMTPVersion(semver::Version);

impl LibXMTPVersion {
    pub fn parse(version_str: &str) -> Result<Self, CommitValidationError> {
        semver::Version::parse(version_str)
            .map(Self)
            .map_err(|_| CommitValidationError::InvalidVersionFormat(version_str.to_string()))
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
    pub metadata_validation_info: MutableMetadataValidationInfo,
    pub installations_changed: bool,
    pub permissions_changed: bool,
    pub dm_members: Option<DmMembers<String>>,
}

/// Reject any commit that carries a `PreSharedKey` proposal.
///
/// Called from both the steady-state and bootstrap commit-validation
/// paths so the rejection rule lives in one place — drift between the
/// two paths is a security risk (a steady-state tightening that misses
/// the bootstrap path would let a sender smuggle a PSK proposal through
/// a bootstrap-shaped commit).
fn reject_psk_proposals(staged_commit: &StagedCommit) -> Result<(), CommitValidationError> {
    if staged_commit.psk_proposals().any(|_| true) {
        return Err(CommitValidationError::NoPSKSupport);
    }
    Ok(())
}

impl ValidatedCommit {
    pub async fn from_staged_commit(
        context: &impl XmtpSharedContext,
        staged_commit: &StagedCommit,
        committer_leaf_index: LeafNodeIndex,
        openmls_group: &OpenMlsGroup,
    ) -> Result<Self, CommitValidationError> {
        let extensions = openmls_group.extensions();
        // Capability-aware reads. On post-bootstrap groups, the
        // legacy `ImmutableMetadata` and `GroupMutableMetadata`
        // extensions are stripped — read from the AppData dictionary
        // instead. Bootstrap commits themselves are detected below
        // and route into `validate_bootstrap_and_build`, which uses
        // these pre-flip values as the canonical source.
        let is_migrated = super::app_data::is_migrated_extensions(extensions);
        let immutable_metadata: GroupMetadata = if is_migrated {
            // ComponentSourceError → GroupMutableMetadataError →
            // CommitValidationError::GroupMutableMetadata is the
            // existing conversion chain. There is no
            // ComponentSourceError → GroupMetadataError From impl
            // (GroupMetadataError predates the AppData layer), so
            // wrap structurally and let the GroupMutableMetadata
            // error variant carry the diagnostic — receivers see
            // the same shape they'd get from a malformed dict on
            // the read side.
            let seed =
                super::app_data::component_source::read_group_metadata_from_dict(openmls_group)
                    .map_err(
                        xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from,
                    )?
                    .ok_or(xmtp_mls_common::group_metadata::GroupMetadataError::MissingExtension)?;
            use xmtp_proto::xmtp::mls::message_contents::GroupMetadataV1 as GroupMetadataProto;
            let proto = GroupMetadataProto {
                conversation_type: seed.conversation_type,
                creator_inbox_id: seed.creator_inbox_id,
                creator_account_address: String::new(),
                dm_members: seed.dm_members,
                oneshot_message: seed.oneshot,
            };
            GroupMetadata::try_from(proto)?
        } else {
            extensions.try_into()?
        };
        let mutable_metadata: GroupMutableMetadata = if is_migrated {
            let mut metadata =
                GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new());
            super::app_data::component_source::merge_app_data_into_mutable_metadata(
                &mut metadata,
                openmls_group,
            )
            .map_err(xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from)?;
            metadata
        } else {
            extensions.try_into()?
        };

        // Bootstrap detection MUST run before the steady-state
        // extractors below — bootstrap commits strip MUTABLE_METADATA,
        // GROUP_PERMISSIONS, and GROUP_MEMBERSHIP from
        // `new_group_extensions`, so `extract_metadata_changes` /
        // `extract_permissions_changed` / membership-diff would all
        // surface MissingExtension errors before bootstrap-specific
        // validation could ever run. The pre-flip extensions still
        // carry the legacy set, so the metadata reads above are safe.
        if super::app_data::bootstrap_validator::is_bootstrap_commit(staged_commit, extensions) {
            return Self::validate_bootstrap_and_build(
                staged_commit,
                committer_leaf_index,
                openmls_group,
                immutable_metadata,
                mutable_metadata,
            );
        }

        let conn = context.db();
        // On migrated groups the legacy `GROUP_PERMISSIONS_EXTENSION_ID`
        // is gone — membership policy lives in the AppData
        // dictionary's COMPONENT_REGISTRY entry under
        // `GROUP_MEMBERSHIP`. Per-component AppDataUpdate enforcement
        // runs separately in
        // `validate_app_data_update_proposals_in_commit`, but the
        // legacy code paths here (extract_permissions_changed +
        // standalone Add/Remove proposer permission checks) still
        // need a `GroupMutablePermissions` instance to evaluate
        // against. We derive the membership-affecting bits from the
        // registry so post-bootstrap commits enforce the same policy
        // a pre-bootstrap GCE-extension lookup would.
        let group_permissions: GroupMutablePermissions = if is_migrated {
            super::app_data::policy::membership_policy_set_from_registry(openmls_group)?
        } else {
            extensions.try_into()?
        };
        let current_group_members = get_current_group_members(openmls_group);

        let existing_group_extensions = openmls_group.extensions();
        let proposals_enabled = super::check_proposals_enabled(existing_group_extensions);
        let new_group_extensions = staged_commit.group_context().extensions();

        // On migrated groups, load the pre-commit COMPONENT_REGISTRY
        // exactly once and thread it through both
        // `read_post_commit_component_bytes` (here) and
        // `validate_app_data_update_proposals_in_commit` (further
        // down). This collapses two independent dict reads on every
        // migrated-commit validation into one.
        //
        // Pre-commit semantics are the documented convention across
        // the migrated commit path — see the doc on
        // `read_post_commit_component_bytes` for the full statement
        // and the bootstrap-commit carve-out.
        //
        // On migrated groups, metadata changes flow as AppDataUpdate
        // proposals — there is no legacy GroupMutableMetadata
        // extension on either side to diff. Per-component policy
        // enforcement happens through
        // `validate_app_data_update_proposals_in_commit` below;
        // character limits are enforced at the sender (host APIs like
        // `update_group_name`) and via Component-level
        // `validate_invariant` hooks. An empty struct is the correct
        // "no legacy metadata changes" view — *except* for
        // `MIN_SUPPORTED_PROTOCOL_VERSION`, where the validator below
        // relies on the post-commit floor being surfaced so old
        // clients reject commits raising the floor above their
        // pkg_version. Compute it capability-aware: pre-commit dict
        // overlaid with any `AppDataUpdate(MIN_SUPPORTED_PROTOCOL_VERSION)`
        // proposals carried by the staged commit, last-write-wins.
        // Mirrors the unmigrated branch's reliance on
        // `extract_metadata_changes` returning the new GMM attribute
        // even when nothing else changed.
        let (metadata_validation_info, migrated_registry) = if is_migrated {
            let registry = super::app_data::load_component_registry(openmls_group)
                .map_err(GroupMutableMetadataError::from)?;
            let min_version_bytes =
                super::app_data::component_source::read_post_commit_component_bytes(
                    xmtp_mls_common::app_data::component_id::ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                    openmls_group,
                    staged_commit,
                    &registry,
                )
                .map_err(xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from)?;
            let minimum_supported_protocol_version = match min_version_bytes {
                Some(bytes) => Some(String::from_utf8(bytes).map_err(|e| {
                    CommitValidationError::GroupMutableMetadata(
                        GroupMutableMetadataError::MalformedComponent {
                            component_id: Some(
                                xmtp_mls_common::app_data::component_id::ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                            ),
                            reason: format!("invalid utf-8: {e}"),
                        },
                    )
                })?),
                None => None,
            };
            (
                MutableMetadataValidationInfo {
                    minimum_supported_protocol_version,
                    ..Default::default()
                },
                Some(registry),
            )
        } else {
            (
                extract_metadata_changes(
                    &immutable_metadata,
                    &mutable_metadata,
                    existing_group_extensions,
                    new_group_extensions,
                )?,
                None,
            )
        };

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
                    val if val == MetadataField::AppData.as_str()
                        && new_value.len() > MAX_APP_DATA_LENGTH =>
                    {
                        return Err(CommitValidationError::TooManyCharacters {
                            length: MAX_APP_DATA_LENGTH,
                        });
                    }
                    _ => {}
                }
            }
        }

        // On migrated groups the legacy GROUP_PERMISSIONS_EXTENSION_ID
        // was stripped at bootstrap and stays absent — permission
        // changes flow as `AppDataUpdate(COMPONENT_REGISTRY)` which
        // `validate_app_data_update_proposals_in_commit` validates.
        // Skip the legacy extension diff to avoid `MissingExtension`.
        let permissions_changed = if is_migrated {
            false
        } else {
            extract_permissions_changed(&group_permissions, new_group_extensions)?
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
        // Bootstrap commits are routed earlier in this function and
        // never reach this path; their dispatch is via
        // `validate_bootstrap_and_build`.
        validate_app_data_update_proposals_in_commit(
            staged_commit,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
            migrated_registry.as_ref(),
        )?;

        // Get the installations actually added and removed in the commit
        let ProposalChanges {
            mut added_installations,
            mut removed_installations,
            mut credentials_to_verify,
            added_inbox_proposers,
            removed_inbox_proposers,
            gce_proposer,
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
            context,
            staged_commit,
            openmls_group,
            proposals_enabled,
            &gce_proposer,
            &added_inbox_proposers,
            &removed_inbox_proposers,
        )
        .await?;

        let ExpectedDiff {
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
            proposers,
            added_inboxes,
            removed_inboxes,
            readded_installations,
            metadata_validation_info,
            installations_changed,
            permissions_changed,
            dm_members: immutable_metadata.dm_members,
        };

        // On migrated groups the legacy GROUP_PERMISSIONS extension
        // is gone — reuse the synthesized stub already built above
        // for the same reason (per-component policy enforcement
        // happens via `validate_app_data_update_proposals_in_commit`,
        // and the legacy commit-level policy_set.evaluate_commit
        // would otherwise reject every commit on a migrated group
        // because there's no extension to extract from).
        let policy_set = if is_migrated {
            group_permissions.clone()
        } else {
            extract_group_permissions(openmls_group)?
        };
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

    /// Build a `ValidatedCommit` for the one-time AppData-migration
    /// bootstrap commit.
    ///
    /// Bootstrap commits don't add or remove members, don't change the
    /// per-inbox sequence ids, and don't change the legacy permissions
    /// (their state is migrated to the AppData dictionary, not
    /// modified). They're validated against the receiver-derived
    /// canonical subset and a super-admin proposer requirement; the
    /// resulting `ValidatedCommit` reports "no diff" on every
    /// steady-state field so downstream policy evaluation and
    /// installation-diff checks see a no-op.
    fn validate_bootstrap_and_build(
        staged_commit: &StagedCommit,
        committer_leaf_index: LeafNodeIndex,
        openmls_group: &OpenMlsGroup,
        immutable_metadata: GroupMetadata,
        mutable_metadata: GroupMutableMetadata,
    ) -> Result<Self, CommitValidationError> {
        reject_psk_proposals(staged_commit)?;

        let (actor, proposers) = extract_committer_and_proposers(
            staged_commit,
            committer_leaf_index,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
        )?;

        let gce_proposer = super::app_data::bootstrap_validator::extract_gce_proposer(
            staged_commit,
            openmls_group,
            &immutable_metadata,
            &mutable_metadata,
        )?
        .ok_or(CommitValidationError::ProposerNotFound)?;

        super::app_data::bootstrap_validator::validate_bootstrap_commit(
            staged_commit,
            openmls_group,
            &gce_proposer,
        )?;

        Ok(Self {
            actor,
            proposers,
            added_inboxes: Vec::new(),
            removed_inboxes: Vec::new(),
            readded_installations: HashSet::new(),
            metadata_validation_info: MutableMetadataValidationInfo::default(),
            installations_changed: false,
            permissions_changed: false,
            dm_members: immutable_metadata.dm_members,
        })
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

use std::collections::HashMap;

struct ProposalChanges {
    added_installations: HashSet<Vec<u8>>,
    removed_installations: HashSet<Vec<u8>>,
    credentials_to_verify: Vec<CommitParticipant>,
    /// Maps inbox_id to the proposer who proposed adding it
    added_inbox_proposers: HashMap<String, CommitParticipant>,
    /// Maps inbox_id to the proposer who proposed removing it
    removed_inbox_proposers: HashMap<String, CommitParticipant>,
    /// The proposer of the GCE proposal (if any) - this affects membership changes
    gce_proposer: Option<CommitParticipant>,
}

/**
 * Extracts the installations added and removed via proposals in the commit.
 * Also returns a list of credentials from existing members that need verification (caused by update proposals)
 * Tracks which proposer created each proposal for permission validation.
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
    let mut added_inbox_proposers: HashMap<String, CommitParticipant> = HashMap::new();
    let mut removed_inbox_proposers: HashMap<String, CommitParticipant> = HashMap::new();
    let mut gce_proposer: Option<CommitParticipant> = None;

    for proposal in staged_commit.queued_proposals() {
        // Extract the proposer for this proposal
        let proposer = match proposal.sender() {
            Sender::Member(leaf_index) => extract_commit_participant(
                leaf_index,
                openmls_group,
                immutable_metadata,
                mutable_metadata,
            )?,
            _ => return Err(CommitValidationError::ActorNotMember),
        };

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
                let inbox_id = inbox_id_from_credential(leaf_node.credential())?;
                added_installations.insert(installation_id);
                added_inbox_proposers.insert(inbox_id, proposer);
            }
            // For Remove Proposals, all we need to do is validate that the installation_id is in the expected diff
            Proposal::Remove(remove_proposal) => {
                let leaf_node = openmls_group
                    .member_at(remove_proposal.removed())
                    .ok_or(CommitValidationError::SubjectDoesNotExist)?;
                let installation_id = leaf_node.signature_key.to_vec();
                let inbox_id = inbox_id_from_credential(&leaf_node.credential)?;
                removed_installations.insert(installation_id);
                removed_inbox_proposers.insert(inbox_id, proposer);
            }
            // For GroupContextExtensions proposals, track the proposer for membership changes
            Proposal::GroupContextExtensions(_) => {
                gce_proposer = Some(proposer);
            }
            _ => continue,
        }
    }

    Ok(ProposalChanges {
        added_installations,
        removed_installations,
        credentials_to_verify,
        added_inbox_proposers,
        removed_inbox_proposers,
        gce_proposer,
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
    pub(super) async fn from_staged_commit_with_proposers(
        context: &impl XmtpSharedContext,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
        proposals_enabled: bool,
        gce_proposer: &Option<CommitParticipant>,
        added_inbox_proposers: &HashMap<String, CommitParticipant>,
        removed_inbox_proposers: &HashMap<String, CommitParticipant>,
    ) -> Result<Self, CommitValidationError> {
        // Get the immutable and mutable metadata. Capability-aware
        // — same dual-source pattern as `from_staged_commit`.
        let extensions = openmls_group.extensions();
        let is_migrated = super::app_data::is_migrated_extensions(extensions);
        let immutable_metadata: GroupMetadata = if is_migrated {
            let seed =
                super::app_data::component_source::read_group_metadata_from_dict(openmls_group)
                    .map_err(
                        xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from,
                    )?
                    .ok_or(xmtp_mls_common::group_metadata::GroupMetadataError::MissingExtension)?;
            use xmtp_proto::xmtp::mls::message_contents::GroupMetadataV1 as GroupMetadataProto;
            let proto = GroupMetadataProto {
                conversation_type: seed.conversation_type,
                creator_inbox_id: seed.creator_inbox_id,
                creator_account_address: String::new(),
                dm_members: seed.dm_members,
                oneshot_message: seed.oneshot,
            };
            GroupMetadata::try_from(proto)?
        } else {
            extensions.try_into()?
        };
        let mutable_metadata: GroupMutableMetadata = if is_migrated {
            let mut metadata =
                GroupMutableMetadata::new(std::collections::HashMap::new(), Vec::new(), Vec::new());
            super::app_data::component_source::merge_app_data_into_mutable_metadata(
                &mut metadata,
                openmls_group,
            )
            .map_err(xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from)?;
            metadata
        } else {
            extensions.try_into()?
        };

        reject_psk_proposals(staged_commit)?;

        let expected_diff = Self::extract_expected_diff_with_proposers(
            context,
            openmls_group.group_id().as_slice(),
            staged_commit,
            extensions,
            &immutable_metadata,
            &mutable_metadata,
            proposals_enabled,
            gce_proposer,
            added_inbox_proposers,
            removed_inbox_proposers,
        )
        .await?;

        Ok(expected_diff)
    }

    /// Generates an expected diff with proposer attribution for each inbox change.
    /// This is used when validating commits with proposals from multiple members.
    #[allow(clippy::too_many_arguments)]
    async fn extract_expected_diff_with_proposers(
        context: &impl XmtpSharedContext,
        group_id: &[u8], // used for logging
        staged_commit: &StagedCommit,
        existing_group_extensions: &Extensions<GroupContext>,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
        proposals_enabled: bool,
        gce_proposer: &Option<CommitParticipant>,
        added_inbox_proposers: &HashMap<String, CommitParticipant>,
        removed_inbox_proposers: &HashMap<String, CommitParticipant>,
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

        // For added inboxes, try to find the proposer from:
        // 1. The original Add proposal proposer for this specific inbox
        // 2. The GCE proposer (if membership changed via GCE proposal without a direct Add proposal)
        // When proposals are enabled, a proposer must always be determinable.
        let added_inboxes = membership_diff
            .added_inboxes
            .iter()
            .map(|inbox_id| {
                // Look up the proposer who proposed adding this specific inbox.
                // Falls back to the GCE proposer if no direct Add proposal was found
                // (e.g., membership changed via a GroupContextExtensions proposal).
                let proposer = added_inbox_proposers
                    .get(inbox_id.as_str())
                    .cloned()
                    .or_else(|| gce_proposer.clone());
                match proposer {
                    Some(p) => Ok(build_inbox_with_proposer(
                        inbox_id,
                        immutable_metadata,
                        mutable_metadata,
                        p,
                    )),
                    None if proposals_enabled => Err(CommitValidationError::ProposerNotFound),
                    None => Ok(build_inbox(inbox_id, immutable_metadata, mutable_metadata)),
                }
            })
            .collect::<Result<Vec<Inbox>, CommitValidationError>>()?;

        // For removed inboxes, try to find the proposer from:
        // 1. The original Remove proposal proposer for this specific inbox
        // 2. The GCE proposer (if membership changed via GCE proposal without a direct Remove proposal)
        // When proposals are enabled, a proposer must always be determinable.
        let removed_inboxes = membership_diff
            .removed_inboxes
            .iter()
            .map(|inbox_id| {
                let proposer = removed_inbox_proposers
                    .get(inbox_id.as_str())
                    .cloned()
                    .or_else(|| gce_proposer.clone());
                match proposer {
                    Some(p) => Ok(build_inbox_with_proposer(
                        inbox_id,
                        immutable_metadata,
                        mutable_metadata,
                        p,
                    )),
                    None if proposals_enabled => Err(CommitValidationError::ProposerNotFound),
                    None => Ok(build_inbox(inbox_id, immutable_metadata, mutable_metadata)),
                }
            })
            .collect::<Result<Vec<Inbox>, CommitValidationError>>()?;

        let identity_updates = IdentityUpdates::new(&context);
        let expected_installation_diff = identity_updates
            .get_installation_diff(
                &conn,
                group_id,
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

/// Superadmins are permitted to readd installations, e.g. for fork recovery
/// We can take these readded installations out of the list of installations to validate
pub(super) fn extract_readded_installations(
    actor: &CommitParticipant,
    added_installations: &mut HashSet<Vec<u8>>,
    removed_installations: &mut HashSet<Vec<u8>>,
    failed_installations: &mut HashSet<Vec<u8>>,
) -> HashSet<Vec<u8>> {
    if !actor.is_super_admin {
        return HashSet::new();
    }
    let successfully_readded = added_installations
        .intersection(removed_installations)
        .cloned()
        .collect::<HashSet<Vec<u8>>>();
    added_installations.retain(|installation_id| !successfully_readded.contains(installation_id));
    removed_installations.retain(|installation_id| !successfully_readded.contains(installation_id));

    // We only want to intersect with *remaining* removed installations here, to avoid double counting
    let unsuccessfully_readded = failed_installations
        .intersection(removed_installations)
        .cloned()
        .collect::<HashSet<Vec<u8>>>();
    failed_installations
        .retain(|installation_id| !unsuccessfully_readded.contains(installation_id));
    removed_installations
        .retain(|installation_id| !unsuccessfully_readded.contains(installation_id));

    successfully_readded
        .union(&unsuccessfully_readded)
        .cloned()
        .collect()
}

/// Compare the list of installations added and removed in the commit to the expected diff based on the changes
/// to the inbox state.
/// Satisfies Rule 3 and Rule 7
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

/// Validate a single `AppDataUpdate` (component_id + operation) against
/// `registry` on behalf of `actor`.
///
/// Shared core for both validator entry points:
/// [`validate_proposal`] (standalone proposal-by-reference messages) and
/// [`validate_app_data_update_proposals_in_commit`] (proposals inside
/// commits, inline or referenced). Both paths must enforce identical
/// permission checks; lifting the loop here keeps them in lockstep so a
/// future change can't drift the two implementations apart.
///
/// Reads the pre-commit stored bytes for `component_id` from the
/// group's AppData dictionary and threads them into the expansion step
/// so `RemoveByHash` mutations can be resolved back to the concrete
/// inbox id being removed. If the component has no prior entry (first
/// write), `read_from_app_data_dict` returns `None`, which
/// [`expand_app_data_update_to_changes`] treats as an empty prior set
/// — `Insert` / `Remove` deltas expand normally, and any `RemoveByHash`
/// surfaces `value: None` (the CRDT apply step later rejects with
/// `KeyNotFound`). This matches the `Bytes` component case where a
/// first-time `Update` has no prior value to diff against.
///
/// Returns `Err(InsufficientPermissions)` on the first failure (expand or
/// per-element check) so the caller can reject the wider message wholesale.
fn validate_one_app_data_update(
    component_id: xmtp_mls_common::app_data::component_id::ComponentId,
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    actor: xmtp_mls_common::app_data::validation::ActorAuthority,
    proposer_inbox_id: &str,
    registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
    openmls_group: &OpenMlsGroup,
) -> Result<(), CommitValidationError> {
    use super::app_data::component_source::read_from_app_data_dict;

    // Pull the pre-commit stored bytes for this component so the expansion
    // step can resolve `RemoveByHash` mutations back to the concrete
    // inbox id being removed. `None` is a legal first-write state — see
    // the fn docstring above for how the expansion handles it.
    let old_value = read_from_app_data_dict(component_id, openmls_group);

    validate_one_app_data_update_with_old_value(
        component_id,
        operation,
        actor,
        proposer_inbox_id,
        registry,
        old_value.as_deref(),
    )
}

/// Pure core of [`validate_one_app_data_update`] with `old_value`
/// passed explicitly so unit tests can exercise the
/// expand → per-change policy loop without a real MLS group.
pub(super) fn validate_one_app_data_update_with_old_value(
    component_id: xmtp_mls_common::app_data::component_id::ComponentId,
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    actor: xmtp_mls_common::app_data::validation::ActorAuthority,
    proposer_inbox_id: &str,
    registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
    old_value: Option<&[u8]>,
) -> Result<(), CommitValidationError> {
    use xmtp_mls_common::app_data::{
        registry_table::lookup_component,
        validation::{ComponentChange, validate_component_write},
    };

    // Two dispatch shapes:
    //
    // - **Known component**: expand via the per-id `Component` impl
    //   (decodes Set/Map deltas into per-element changes) and run both
    //   layers — registry policy AND per-component invariant.
    //
    // - **Unknown component** (no per-id impl on this client; the
    //   sender shipped a newer release): look the component's
    //   registered [`ComponentType`] up in the registry and run the
    //   type-aware expansion. Same per-element change list a typed
    //   client would produce, fed through the same policy loop. The
    //   per-component invariant hook is skipped — there's no per-id
    //   trait method to call — but registry-policy enforcement still
    //   gates the write, so deny-by-default applies.
    let component = lookup_component(component_id);
    let changes = if let Some(component) = component {
        component
            .expand_to_changes(operation, old_value)
            .map_err(|e| {
                let wrapped = super::app_data::component_source::ComponentSourceError::from(e);
                tracing::warn!(
                    proposer_inbox_id,
                    component_id = %component_id,
                    error = %wrapped,
                    "AppDataUpdate proposal rejected: failed to expand payload"
                );
                CommitValidationError::InsufficientPermissions
            })?
    } else {
        match super::app_data::component_source::expand_app_data_update_to_changes(
            component_id,
            operation,
            old_value,
            registry,
        ) {
            Ok(changes) => changes,
            Err(err) => {
                tracing::warn!(
                    proposer_inbox_id,
                    component_id = %component_id,
                    error = %err,
                    "AppDataUpdate proposal rejected"
                );
                return Err(CommitValidationError::InsufficientPermissions);
            }
        }
    };

    for change in &changes {
        let cc = ComponentChange::builder()
            .component_id(component_id)
            .op(change.op)
            .actor(actor)
            .maybe_new_value(change.value.as_deref())
            .build();

        // Layer 1: registry-based policy. Applies to both known and
        // unknown components — every component requires a registry
        // entry (deny by default).
        if let Err(e) = validate_component_write(&cc, registry) {
            tracing::warn!(
                proposer_inbox_id,
                component_id = %component_id,
                op = %change.op,
                error = %e,
                "AppDataUpdate proposal rejected"
            );
            return Err(CommitValidationError::InsufficientPermissions);
        }

        // Layer 2: per-component invariants. Only available for known
        // components — unknown ids have nothing to invoke. Skipping
        // the invariant is the cost of forward compatibility (see
        // module-level docstring).
        if let Some(component) = component
            && let Err(e) = component.validate_invariant(&cc, registry)
        {
            tracing::warn!(
                proposer_inbox_id,
                component_id = %component_id,
                op = %change.op,
                error = %e,
                "AppDataUpdate proposal rejected: component invariant violated"
            );
            return Err(CommitValidationError::InsufficientPermissions);
        }
    }

    Ok(())
}

/// Resolve the proposer leaf index for a proposal sender, rejecting
/// senders that can't legally propose `AppDataUpdate`.
///
/// External senders and new-member proposals can't carry
/// `AppDataUpdate` by design — only an existing leaf can propose one.
/// Pulled out so the rejection reason is a single code path that can
/// be unit-tested without constructing a `StagedCommit`.
pub(super) fn app_data_update_proposer_leaf(
    sender: &Sender,
) -> Result<&LeafNodeIndex, CommitValidationError> {
    match sender {
        Sender::Member(leaf_index) => Ok(leaf_index),
        Sender::External(_) | Sender::NewMemberCommit | Sender::NewMemberProposal => {
            Err(CommitValidationError::ActorNotMember)
        }
    }
}

/// Validate every `AppDataUpdate` proposal carried by `staged_commit`
/// against the group's component registry.
///
/// `staged_commit.app_data_update_proposals()` iterates both inline
/// proposals and references that resolve into the group's proposal
/// store, so this covers both shapes. `validate_proposal()` covers the
/// standalone-proposal-by-reference path (proposals that arrive as
/// their own message), but commits never flow through
/// `validate_proposal` — they go through `from_staged_commit`. Without
/// this helper, `AppDataUpdate` proposals committed alongside a commit
/// would bypass `validate_component_write` entirely, since
/// `extract_metadata_changes` only inspects the legacy mutable-metadata
/// extension.
///
/// Delegates the per-proposal permission check to
/// [`validate_one_app_data_update`] so the core logic stays shared with
/// the standalone-proposal path in [`validate_proposal`].
///
/// # Registry semantics
///
/// `preloaded_registry`, when `Some`, is the **pre-commit**
/// `COMPONENT_REGISTRY` — the same view used by
/// [`super::app_data::component_source::read_post_commit_component_bytes`]
/// and any other per-component check on this commit. Pre-commit (not
/// post-commit) is the documented convention across the migrated commit
/// path: registry mutations and writes that depend on those mutations
/// MUST land in separate commits. Bootstrap commits are the only
/// "register + write in the same commit" legitimate pattern and route
/// through a dedicated validator instead.
///
/// `None` defers the registry load to this function, which only
/// materializes it lazily after a peek confirms at least one
/// `AppDataUpdate` proposal exists. The split lets the caller share a
/// single registry across this helper and
/// `read_post_commit_component_bytes` on the migrated branch, while
/// unmigrated callers (or commits with zero `AppDataUpdate` proposals)
/// pay zero load cost.
fn validate_app_data_update_proposals_in_commit(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
    preloaded_registry: Option<&xmtp_mls_common::app_data::component_registry::ComponentRegistry>,
) -> Result<(), CommitValidationError> {
    use super::app_data::load_component_registry;
    use std::collections::HashMap;
    use xmtp_mls_common::app_data::{component_id::ComponentId, validation::ActorAuthority};

    // Peek first: the common case is zero AppDataUpdate proposals, in
    // which case we skip the registry load and the per-proposer work
    // entirely. This runs on every commit's validation path.
    //
    // Safety of the early-exit against unresolvable references: OpenMLS
    // rejects commits that reference proposals it can't resolve against
    // the group's proposal store *before* `from_staged_commit` is called
    // (see `process_message`'s reference-resolution pass). So
    // `staged_commit.app_data_update_proposals()` iterates only
    // inline-or-successfully-resolved proposals — an attacker can't
    // smuggle in a dangling reference that would `peek()` as `None` and
    // bypass the loop.
    let mut proposals = staged_commit.app_data_update_proposals().peekable();
    if proposals.peek().is_none() {
        return Ok(());
    }

    // Use the caller's pre-loaded registry when available; otherwise
    // load lazily. `owned_registry` keeps the loaded value alive for
    // the `registry` borrow.
    let owned_registry;
    let registry = match preloaded_registry {
        Some(r) => r,
        None => {
            owned_registry = load_component_registry(openmls_group)?;
            &owned_registry
        }
    };

    // A single commit's bootstrap can carry multiple AppDataUpdate proposals
    // from the same leaf; cache extracted `CommitParticipant`s so we don't
    // re-walk the admin lists and re-parse the credential for every one.
    let mut participants: HashMap<LeafNodeIndex, CommitParticipant> = HashMap::new();

    for queued in proposals {
        let app_data = queued.app_data_update_proposal();
        let proposer_leaf = app_data_update_proposer_leaf(queued.sender())?;
        let proposer = match participants.get(proposer_leaf) {
            Some(cached) => cached,
            None => {
                let fresh = extract_commit_participant(
                    proposer_leaf,
                    openmls_group,
                    immutable_metadata,
                    mutable_metadata,
                )?;
                participants.entry(*proposer_leaf).or_insert(fresh)
            }
        };

        validate_one_app_data_update(
            ComponentId::from(app_data.component_id()),
            app_data.operation(),
            ActorAuthority::from(proposer),
            &proposer.inbox_id,
            registry,
            openmls_group,
        )?;
    }

    Ok(())
}

/// Extracts the [`CommitParticipant`] from the [`LeafNodeIndex`]
pub(super) fn extract_commit_participant(
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

/// Get the [`GroupMembership`] from a `GroupContext` struct.
///
/// Post-migration the legacy `GROUP_MEMBERSHIP_EXTENSION_ID` is gone —
/// we reconstruct from the AppData dictionary's `GROUP_MEMBERSHIP`
/// component. Pre-migration the legacy extension is authoritative.
#[tracing::instrument(level = "trace", skip_all)]
pub fn extract_group_membership(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMembership, CommitValidationError> {
    if let Some(proto) = super::app_data::component_source::read_group_membership_from_dict(
        extensions,
    )
    .map_err(|e| {
        CommitValidationError::GroupMutableMetadata(
            xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from(e),
        )
    })? {
        // Proto and `GroupMembership` carry the same two fields; build
        // directly to skip a wasteful `encode → decode` round-trip
        // through `try_from(bytes)`.
        return Ok(GroupMembership {
            members: proto.members,
            failed_installations: proto.failed_installations,
        });
    }

    for extension in extensions.iter() {
        if let Extension::Unknown(
            xmtp_configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
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
    old_group_extensions: &Extensions<GroupContext>,
    new_group_extensions: &Extensions<GroupContext>,
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
    new_group_extensions: &Extensions<GroupContext>,
) -> Result<bool, CommitValidationError> {
    let new_group_permissions: GroupMutablePermissions = new_group_extensions.try_into()?;
    Ok(!old_group_permissions.eq(&new_group_permissions))
}

fn find_unknown_extension(
    extensions: &Extensions<GroupContext>,
    extension_type: u16,
) -> Option<&Vec<u8>> {
    extensions.iter().find_map(|extension| {
        if let Extension::Unknown(id, UnknownExtension(bytes)) = extension
            && *id == extension_type
        {
            return Some(bytes);
        }
        None
    })
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
        proposer: None,
    }
}

fn build_inbox_with_proposer(
    inbox_id: &String,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
    proposer: CommitParticipant,
) -> Inbox {
    Inbox {
        inbox_id: inbox_id.to_string(),
        is_admin: mutable_metadata.is_admin(inbox_id),
        is_super_admin: mutable_metadata.is_super_admin(inbox_id),
        is_creator: immutable_metadata.creator_inbox_id.eq(inbox_id),
        proposer: Some(proposer),
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

/// Takes a [`StagedCommit`] and extracts the committer and all unique proposers.
///
/// `committer_leaf_index` is the verified sender of the commit message — for
/// received commits, that's `ProcessedMessage::sender()` after OpenMLS has
/// validated the framing signature against the leaf at that index; for our
/// own commits being applied from an intent, that's `mls_group.own_leaf_index()`.
/// Either way the cryptographic signature is the source of truth, so we
/// don't need a path update to identify the committer.
///
/// Returns (committer, proposers) where:
/// - `committer` is the actor who created the commit
/// - `proposers` is a list of all unique members who created proposals
///
/// Note: The committer may differ from the proposers — this is valid when one
/// member commits proposals created by other members.
fn extract_committer_and_proposers(
    staged_commit: &StagedCommit,
    committer_leaf_index: LeafNodeIndex,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<(CommitParticipant, Vec<CommitParticipant>), CommitValidationError> {
    // Collect all unique proposers from the proposals
    let mut proposer_leaf_indices: Vec<&LeafNodeIndex> = Vec::new();
    for proposal in staged_commit.queued_proposals() {
        match proposal.sender() {
            Sender::Member(member_leaf_node_index) => {
                // Only add if not already in the list
                if !proposer_leaf_indices.contains(&member_leaf_node_index) {
                    proposer_leaf_indices.push(member_leaf_node_index);
                }
            }
            _ => return Err(CommitValidationError::ActorNotMember),
        }
    }

    // Convert all proposer leaf indices to CommitParticipants
    let mut proposers: Vec<CommitParticipant> = Vec::new();
    for leaf_index in &proposer_leaf_indices {
        let participant = extract_commit_participant(
            leaf_index,
            openmls_group,
            immutable_metadata,
            mutable_metadata,
        )?;
        proposers.push(participant);
    }

    let committer = extract_commit_participant(
        &committer_leaf_index,
        openmls_group,
        immutable_metadata,
        mutable_metadata,
    )?;

    Ok((committer, proposers))
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
            return Err(CommitValidationError::ActorNotMember);
        }
    };

    let unsupported_error =
        || CommitValidationError::UnsupportedProposalType(proposal.proposal().proposal_type());

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
                    return Err(CommitValidationError::InsufficientPermissions);
                }
            }
        }
        Proposal::Remove(remove_proposal) => {
            // Check if the proposer has permission to remove members
            // Get the inbox_id of the member being removed
            let removed_member = openmls_group
                .member_at(remove_proposal.removed())
                .ok_or(CommitValidationError::SubjectDoesNotExist)?;
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
                return Err(CommitValidationError::InsufficientPermissions);
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
                return Err(CommitValidationError::InsufficientPermissions);
            }
        }
        Proposal::GroupContextExtensions(gce_proposal) => {
            let existing_extensions = openmls_group.extensions();
            let new_extensions = gce_proposal.extensions();

            // Check for mutable metadata changes (group name, admin list, etc.)
            let old_meta = find_mutable_metadata_extension(existing_extensions);
            let new_meta = find_mutable_metadata_extension(new_extensions);
            if old_meta.is_some() && new_meta.is_none() {
                tracing::warn!(
                    proposer_inbox_id = %proposer.inbox_id,
                    "GCE proposal rejected: cannot remove mutable metadata extension"
                );
                return Err(CommitValidationError::InsufficientPermissions);
            }
            if let (Some(old_meta), Some(new_meta)) = (old_meta, new_meta)
                && old_meta != new_meta
            {
                let metadata_changes = extract_metadata_changes(
                    immutable_metadata,
                    mutable_metadata,
                    existing_extensions,
                    new_extensions,
                )?;

                for change in &metadata_changes.metadata_field_changes {
                    if let Some(policy) = policy_set.update_metadata_policy.get(&change.field_name)
                        && !policy.evaluate(&proposer, change)
                    {
                        tracing::warn!(
                            proposer_inbox_id = %proposer.inbox_id,
                            field = %change.field_name,
                            "GCE proposal rejected: no permission to update metadata field"
                        );
                        return Err(CommitValidationError::InsufficientPermissions);
                    }
                }

                if !metadata_changes.admins_added.is_empty()
                    && !policy_set.add_admin_policy.evaluate(&proposer)
                {
                    tracing::warn!(
                        proposer_inbox_id = %proposer.inbox_id,
                        "GCE proposal rejected: no permission to add admins"
                    );
                    return Err(CommitValidationError::InsufficientPermissions);
                }
                if !metadata_changes.admins_removed.is_empty()
                    && !policy_set.remove_admin_policy.evaluate(&proposer)
                {
                    tracing::warn!(
                        proposer_inbox_id = %proposer.inbox_id,
                        "GCE proposal rejected: no permission to remove admins"
                    );
                    return Err(CommitValidationError::InsufficientPermissions);
                }

                if (!metadata_changes.super_admins_added.is_empty()
                    || !metadata_changes.super_admins_removed.is_empty())
                    && !proposer.is_super_admin
                {
                    tracing::warn!(
                        proposer_inbox_id = %proposer.inbox_id,
                        "GCE proposal rejected: only super admins can modify super admin list"
                    );
                    return Err(CommitValidationError::InsufficientPermissions);
                }
            }

            // Check for permission changes (only super admin can
            // change permissions). On migrated groups the legacy
            // GROUP_PERMISSIONS, MUTABLE_METADATA, and GROUP_MEMBERSHIP
            // extensions are all stripped at bootstrap; their state
            // lives in the AppData dictionary and changes flow as
            // `AppDataUpdate` proposals validated against the dict's
            // policy entries. A GCE proposal that (re-)introduces any
            // of these legacy extensions on a migrated group is
            // therefore unconditionally rejected — otherwise a
            // non-super-admin peer could smuggle an arbitrary policy
            // set, metadata change, or membership view through the
            // legacy extension because the post-migration check below
            // would have nothing to diff against.
            let migrated_for_perms = super::app_data::is_migrated_extensions(existing_extensions);
            if migrated_for_perms {
                for (ext_id, ext_name) in [
                    (
                        xmtp_configuration::GROUP_PERMISSIONS_EXTENSION_ID,
                        "GROUP_PERMISSIONS",
                    ),
                    (
                        xmtp_configuration::MUTABLE_METADATA_EXTENSION_ID,
                        "MUTABLE_METADATA",
                    ),
                    (
                        xmtp_configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
                        "GROUP_MEMBERSHIP",
                    ),
                ] {
                    if find_unknown_extension(new_extensions, ext_id).is_some() {
                        tracing::warn!(
                            proposer_inbox_id = %proposer.inbox_id,
                            extension = ext_name,
                            "GCE proposal rejected: legacy extension cannot be (re-)added to a migrated group"
                        );
                        return Err(CommitValidationError::InsufficientPermissions);
                    }
                }
            } else {
                let old_permissions: GroupMutablePermissions = existing_extensions.try_into()?;
                if !proposer.is_super_admin
                    && let Ok(true) = extract_permissions_changed(&old_permissions, new_extensions)
                {
                    tracing::warn!(
                        proposer_inbox_id = %proposer.inbox_id,
                        "GCE proposal rejected: only super admins can change permissions"
                    );
                    return Err(CommitValidationError::InsufficientPermissions);
                }
            }
        }
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
                return Err(CommitValidationError::ActorNotMember);
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
            )?;
        }
        Proposal::AppEphemeral(_) => {
            return Err(unsupported_error());
        }
        Proposal::_AppAck => {
            return Err(unsupported_error());
        }
        Proposal::SelfRemove => {
            return Err(unsupported_error());
        }
    }

    Ok(())
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
                .metadata_validation_info
                .metadata_field_changes
                .iter()
                .map(MetadataFieldChangeProto::from)
                .collect(),
            left_inboxes: left_inboxes.iter().map(InboxProto::from).collect(),
            added_admin_inboxes: commit
                .metadata_validation_info
                .admins_added
                .iter()
                .map(InboxProto::from)
                .collect(),
            removed_admin_inboxes: commit
                .metadata_validation_info
                .admins_removed
                .iter()
                .map(InboxProto::from)
                .collect(),
            added_super_admin_inboxes: commit
                .metadata_validation_info
                .super_admins_added
                .iter()
                .map(InboxProto::from)
                .collect(),
            removed_super_admin_inboxes: commit
                .metadata_validation_info
                .super_admins_removed
                .iter()
                .map(InboxProto::from)
                .collect(),
        }
    }
}
