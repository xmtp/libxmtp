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
    messages::proposals::{AppDataUpdateOperation, Proposal, ProposalOrRefType, ProposalType},
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
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::{
    identity::MlsCredential,
    mls::message_contents::{
        ExternalCommitPolicyV1, GroupMembershipChanges, GroupUpdated as GroupUpdatedProto,
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
    // External joins do not flow through this variant — they are routed
    // separately into [`ValidatedCommit::from_external_commit`], which
    // builds its own actor/participant view from the joiner's path leaf.
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
    /// Sender published an `AppDataUpdate(Update)` against
    /// `MIN_SUPPORTED_PROTOCOL_VERSION` whose new value is below the
    /// existing floor. Monotonic-only: a downgrade silently unpauses
    /// peers between the new and old floors, defeating XIP §3's gate.
    #[error("min_version {requested} would downgrade existing floor {current}")]
    MinVersionDowngrade { requested: String, current: String },
    /// Sender published an `AppDataUpdate(Remove)` against
    /// `MIN_SUPPORTED_PROTOCOL_VERSION` on a group that already has a
    /// floor set. Explicit unsetting is just a downgrade in disguise,
    /// rejected for the same XIP §3 reason.
    #[error("min_version remove is rejected; existing floor is {current}")]
    MinVersionRemoveOnExistingFloor { current: String },
    /// A well-known component value in the AppData dictionary failed
    /// to decode while validating an AppDataUpdate proposal — most
    /// commonly a malformed `COMPONENT_REGISTRY`. Treated as a
    /// terminal wire-format violation so the offending commit is
    /// rejected rather than silently downgraded to "empty registry"
    /// (which would let a permissive validator state slip in).
    #[error(transparent)]
    ComponentSource(#[from] super::app_data::component_source::ComponentSourceError),

    /// An `EXTERNAL_COMMIT_POLICY` write violates the XIP-82
    /// field-coupling invariants (post-state check: enable requires a
    /// 32-byte key + slot id + the GROUP_MEMBERSHIP external-committer
    /// grant; revoke leaves every per-invite field absent). Convergent:
    /// a pure function of the proposed value and the commit's own
    /// registry write.
    #[error(transparent)]
    ExternalCommitPolicy(#[from] super::external_commit_policy::ExternalCommitPolicyError),

    /// All bootstrap-commit-validator failures. The bootstrap path runs
    /// only during the one-time AppData migration; isolating its many
    /// failure modes in a sub-enum keeps the steady-state validator's
    /// surface from being dominated by migration-specific noise.
    #[error(transparent)]
    Bootstrap(#[from] super::app_data::bootstrap_validator::BootstrapValidationError),
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),

    // ──────────────────────────────────────────────────────────────────
    // External-commit validation failures (L-7).
    //
    // These are surfaced exclusively by
    // [`ValidatedCommit::from_external_commit`] and its helpers. New
    // variants are appended here so unrelated PRs that add other
    // CommitValidationError variants don't conflict on the same line
    // range.
    // ──────────────────────────────────────────────────────────────────
    /// The group's permission policy has `allow_external_commit = false`
    /// — external joins are not accepted on this group.
    #[error("external commits are not allowed on this group")]
    ExternalCommitNotAllowed,
    /// The commit was routed to the external-commit validator but its
    /// framing sender is not `Sender::NewMemberCommit`. Either the
    /// caller dispatched incorrectly or the commit is malformed.
    #[error("external-commit validator invoked on non-NewMemberCommit sender")]
    ExternalCommitNotNewMemberCommit,
    /// An external commit must carry exactly one `ExternalInit`
    /// proposal (RFC 9420 §12.4.3.2). The staged commit had none.
    #[error("external commit is missing the required ExternalInit proposal")]
    ExternalCommitMissingExternalInit,
    /// An external commit must carry exactly one `ExternalInit`
    /// proposal — this commit carried more than one.
    #[error("external commit carried multiple ExternalInit proposals")]
    ExternalCommitMultipleExternalInit,
    /// RFC 9420 §12.4.3.2: external commits MUST NOT include any
    /// proposals by reference.
    #[error("external commit included a proposal by reference")]
    ExternalCommitByReferenceProposalsForbidden,
    /// External commits carry the joiner's leaf in the update path
    /// — this commit had no update path, so we cannot identify the
    /// joiner and refuse to accept the commit.
    #[error("external commit is missing the joiner's update path leaf")]
    ExternalCommitMissingPathLeaf,
    /// An `Add` proposal in the external commit referenced a key
    /// package whose credential inbox id differs from the joiner's
    /// path-leaf inbox id. libxmtp v1 only allows external commits to
    /// add installations belonging to the same inbox as the joiner —
    /// this prevents a joiner from smuggling unrelated members in
    /// under cover of an external commit.
    #[error("Add proposal in external commit references a different inbox id")]
    CrossInboxAddInExternalCommit,
    /// External commits must register the joiner in the AppData
    /// `GROUP_MEMBERSHIP` component via exactly one `AppDataUpdate`
    /// proposal — this commit carried none.
    #[error("external commit is missing the GROUP_MEMBERSHIP AppDataUpdate")]
    ExternalCommitAppDataUpdateMissing,
    /// External commits must register the joiner in the AppData
    /// `GROUP_MEMBERSHIP` component via exactly one `AppDataUpdate`
    /// proposal — this commit carried more than one.
    #[error("external commit carried multiple AppDataUpdate proposals")]
    ExternalCommitAppDataUpdateMultiple,
    /// The single AppDataUpdate proposal in this external commit
    /// targets a component other than `GROUP_MEMBERSHIP`. Only the
    /// membership registration is permitted — broader AppData writes
    /// are not allowed at join time.
    #[error("external commit's AppDataUpdate must target GROUP_MEMBERSHIP")]
    ExternalCommitAppDataUpdateWrongComponent,
    /// The joiner's AppDataUpdate proposal mutates a `GROUP_MEMBERSHIP`
    /// entry that is not their own inbox. Joiners may only insert their
    /// own membership entry through an external commit.
    #[error("external commit's AppDataUpdate is out of scope for the joiner")]
    ExternalCommitAppDataUpdateOutOfScope,
    /// The wire-form payload of the joiner's AppDataUpdate
    /// (`TlsMapDelta<InboxId, VLBytes>`) failed to decode. Treat as a
    /// terminal wire-format violation so the commit is rejected rather
    /// than silently accepted.
    #[error("external commit's AppDataUpdate payload is malformed: {0}")]
    ExternalCommitAppDataUpdatePayloadMalformed(String),
    /// The "resync" flavor of external commit (where the joiner removes
    /// a stale prior leaf with a SelfRemove proposal) is not supported
    /// in v1.
    #[error("resync external commits are not supported in v1")]
    ResyncExternalCommitNotSupported,
    /// External commits must not carry a `GroupContextExtensions`
    /// proposal — post-AppData migration, GCE updates are not a
    /// legitimate join-time operation.
    #[error("external commit must not carry GroupContextExtensions proposals")]
    ExternalCommitGceForbidden,
    /// The external commit carried a proposal type that is never legal
    /// in an external commit (e.g. PreSharedKey, Update, Remove, ReInit,
    /// Custom, AppEphemeral, _AppAck). PSKs are called out by XIP-82
    /// explicitly: a non-member has no pre-shared key it can
    /// legitimately reference with the group, so admitting them is
    /// gratuitous attack surface in v1.
    #[error("external commit carried unsupported proposal type: {0:?}")]
    ExternalCommitUnsupportedProposalType(ProposalType),
    /// The joiner's inbox id is already in the group — an existing
    /// ratchet-tree leaf or an existing `GROUP_MEMBERSHIP` entry
    /// carries it. An existing member cannot re-add itself via external
    /// commit; among other things, that would let it rewrite — or
    /// re-tag — its own membership entry (XIP-82 check 4).
    #[error("external commit joiner {inbox_id} is already a member")]
    ExternalCommitJoinerAlreadyMember { inbox_id: String },
    /// The commit's delivery-service envelope timestamp exceeds the
    /// policy's absolute campaign expiry (`expires_at_ns`, XIP-82
    /// check 8). Envelope-timestamp based so every member reaches the
    /// same verdict regardless of when it syncs the commit.
    #[error("external commit envelope ts {commit_envelope_ns} past invite expiry {expires_at_ns}")]
    ExternalCommitInviteExpired {
        expires_at_ns: u64,
        commit_envelope_ns: u64,
    },
    /// The commit landed more than `expire_in_ns` after the current
    /// epoch's start (XIP-82 check 9) — the blob the joiner used is
    /// stale beyond the group's staleness window. Both timestamps are
    /// delivery-service envelope timestamps.
    #[error("external commit is {age_ns}ns into the epoch, past staleness bound {expire_in_ns}ns")]
    ExternalCommitInviteStale { age_ns: u64, expire_in_ns: u64 },
    /// `GROUP_MEMBERSHIP`'s `external_committer_permissions` block does
    /// not admit the joiner's self-entry insert (XIP-82 check 10). The
    /// enable path is required to establish this grant atomically, so
    /// hitting this on an enabled policy means the group state predates
    /// that invariant or was manipulated.
    #[error("GROUP_MEMBERSHIP external_committer_permissions does not admit the joiner insert")]
    ExternalCommitInsertNotAdmitted,
    /// The joiner's new `GROUP_MEMBERSHIP` entry does not record
    /// `admitted_via_external_group_id` equal to the active
    /// `external_group_id` (XIP-82 check 11). Required on every
    /// external commit — whatever `max_uses` is — so a later policy
    /// change to a finite cap starts from accurate data.
    #[error("external commit's membership entry does not record the active external_group_id")]
    ExternalCommitAdmittedViaTagMismatch,
    /// Admitting this joiner would exceed the policy's concurrent
    /// per-invite cap: `max_uses` entries already carry the active
    /// `external_group_id` tag (XIP-82 check 12). Counted from the
    /// shared pre-commit `GROUP_MEMBERSHIP` state so every member —
    /// including ones that joined after the invite was issued —
    /// computes the same count.
    #[error("invite max_uses ({max_uses}) exhausted")]
    ExternalCommitMaxUsesExhausted { max_uses: u32 },
    /// A member-sender commit set, cleared, or altered
    /// `admitted_via_external_group_id` on a `GROUP_MEMBERSHIP` entry.
    /// The tag is write-once: set exactly once by the admitting
    /// external commit and immutable for the life of the entry —
    /// otherwise an invited member could untag itself and free a
    /// `max_uses` slot at will. Entry rewrites for unrelated reasons
    /// (an installation change, say) must carry the tag through
    /// unchanged; the field disappears only when the entry itself does.
    #[error(
        "admitted_via_external_group_id is write-once; member commit altered it for {inbox_id}"
    )]
    AdmittedViaTagImmutable { inbox_id: String },
    /// A `GROUP_MEMBERSHIP` entry value (the prost-encoded
    /// `GroupMembershipEntry`) failed to decode, or decoded to an
    /// unknown version, while enforcing the XIP-82 tag rules. Fail
    /// closed: an undecodable entry could otherwise smuggle a tag
    /// rewrite past the write-once check (replacing a tagged V1 entry
    /// with an "unknown version" blob frees the max_uses slot from
    /// every V1 reader's perspective).
    #[error("GROUP_MEMBERSHIP entry malformed: {0}")]
    GroupMembershipEntryMalformed(String),
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
/// Delivery-service envelope timestamps backing XIP-82's two
/// external-commit time bounds (checks 8 and 9). Both are envelope
/// timestamps — never a validator's wall clock at processing time — so
/// a member that syncs a commit a week later reaches the same verdict
/// as one that processed it immediately.
#[derive(Debug, Clone, Copy)]
pub struct ExternalCommitTimestamps {
    /// Envelope timestamp of the external commit itself (ns).
    pub commit_envelope_ns: u64,
    /// Envelope timestamp of the message by which this client entered
    /// or observed the current epoch (ns): the epoch's commit for
    /// members that processed it, the corresponding `Welcome` for
    /// members added at that epoch, the group-creation timestamp at
    /// the initial epoch. These differ across members only by publish
    /// latency — which is why `expire_in_ns` is a coarse bound.
    pub epoch_started_at_ns: u64,
}

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

    /// Validate a `Sender::NewMemberCommit`-flavored MLS commit (external
    /// commit) carrying an atomic "join the group" payload, asserting
    /// XIP-82's receive-side check set:
    ///
    /// 1. The framing sender is `Sender::NewMemberCommit`.
    /// 2. Exactly one `ExternalInit` proposal is present (RFC 9420
    ///    §12.4.3.2).
    /// 3. Every `Add` proposal's KeyPackage credential carries the same
    ///    inbox id as the joiner's path leaf (anti-smuggling — a joiner
    ///    can add only its own installations).
    /// 4. The joiner's inbox id is not already in the group: no
    ///    existing ratchet-tree leaf and no existing `GROUP_MEMBERSHIP`
    ///    entry. (Re-adding yourself would let you rewrite — or re-tag
    ///    — your own membership entry.)
    /// 5. Exactly one `AppDataUpdate` proposal is present, inserting
    ///    exactly the joiner's own `GROUP_MEMBERSHIP` entry
    ///    (cross-layer invariant: tree-membership ↔ AppData-membership
    ///    coupled in one commit).
    /// 6. No forbidden proposal types: nothing by reference, no
    ///    `Remove`/`SelfRemove` (the "resync" flavor is not v1), no
    ///    PSKs (a non-member has no PSK it can legitimately reference
    ///    with the group), no `GroupContextExtensions`, nothing else.
    /// 7. `EXTERNAL_COMMIT_POLICY.allow_external_commit` is `true`.
    /// 8. The commit's envelope timestamp does not exceed the policy's
    ///    `expires_at_ns` (when set).
    /// 9. When the policy's `expire_in_ns != 0`: the commit's envelope
    ///    timestamp is within `expire_in_ns` of the current epoch's
    ///    start.
    /// 10. `GROUP_MEMBERSHIP`'s `external_committer_permissions` block
    ///     admits the joiner's self-entry insert.
    /// 11. The inserted entry records `admitted_via_external_group_id`
    ///     equal to the active `external_group_id` — on every external
    ///     commit, whatever `max_uses` is, so a later policy change to
    ///     a finite cap starts from accurate data.
    /// 12. When the policy's `max_uses != 0`: strictly fewer than
    ///     `max_uses` current entries carry the active tag.
    ///
    /// Returns a `ValidatedCommit` whose `actor` is the joiner (built
    /// from the path leaf), `added_inboxes`/`added_installations`
    /// reflect the joiner's additions, and `removed_inboxes` is empty.
    ///
    /// Group-state inputs (policy, membership entries, the permission
    /// grant) are read from `openmls_group`'s **pre-merge** extensions,
    /// which every member shares. The two time bounds come from
    /// `timestamps`, which the caller (L-8 — `mls_sync`) sources from
    /// delivery-service envelopes — never a wall clock — so every
    /// member reaches the same verdict regardless of when it processes
    /// the commit. The accept/reject decision is deterministic and
    /// convergent across the membership.
    pub fn from_external_commit(
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
        sender: &Sender,
        immutable_metadata: &GroupMetadata,
        mutable_metadata: &GroupMutableMetadata,
        timestamps: ExternalCommitTimestamps,
    ) -> Result<Self, CommitValidationError> {
        // Check 1: framing sender must be NewMemberCommit. Defensive
        // double-check; the wider mls_sync dispatch should already have
        // routed by sender, but layering the check here means the
        // validator is safe to call directly from tests and tomorrow's
        // refactors can't accidentally hand us a Sender::Member commit.
        enforce_external_commit_sender(sender)?;

        // Check 7: policy gate — short-circuit before any structural
        // work. An absent component and an unknown future version both
        // come back `None` from the loader and fail closed here; an
        // undecodable entry surfaces as `ComponentSource` (also a
        // rejection).
        let policy = super::external_commit_policy::load_external_commit_policy(openmls_group)?;
        let policy = enforce_external_commit_policy(policy.as_ref())?;

        // Checks 8 + 9: envelope-timestamp time bounds.
        enforce_external_commit_time_bounds(policy, &timestamps)?;

        // Check 10: the GROUP_MEMBERSHIP external-committer grant. The
        // enable path establishes this atomically with the policy
        // write (post-state invariant in
        // `validate_external_commit_policy_post_state`), so an enabled
        // policy without the grant means the group state predates that
        // invariant or was manipulated — reject.
        let perms = super::external_commit_policy::external_committer_permissions_for(
            openmls_group,
            xmtp_mls_common::app_data::component_id::ComponentId::GROUP_MEMBERSHIP,
        )?;
        if !super::external_commit_policy::grant_admits_joiner_insert(perms.as_ref()) {
            return Err(CommitValidationError::ExternalCommitInsertNotAdmitted);
        }

        // Identify the joiner via the path leaf so the remaining rules
        // can assert their inbox-id binding against a single source of
        // truth.
        let joiner_leaf = staged_commit
            .update_path_leaf_node()
            .ok_or(CommitValidationError::ExternalCommitMissingPathLeaf)?;
        let joiner_inbox_id = inbox_id_from_credential(joiner_leaf.credential())?;
        let joiner_participant =
            CommitParticipant::from_leaf_node(joiner_leaf, immutable_metadata, mutable_metadata)?;

        // Pre-commit GROUP_MEMBERSHIP entries, decoded once and shared
        // by the already-member check (4) and the max_uses count (12).
        let membership = super::app_data::component_source::read_group_membership_entries(
            openmls_group.extensions(),
        )?
        .unwrap_or_default();

        // Check 4 (tree half): no current leaf may carry the joiner's
        // inbox id. The pre-merge tree is what `members()` iterates.
        let tree_inbox_ids = openmls_group
            .members()
            .map(|member| inbox_id_from_credential(&member.credential))
            .collect::<Result<HashSet<String>, _>>()?;
        enforce_joiner_not_already_member(&joiner_inbox_id, &tree_inbox_ids, &membership)?;

        // Check 12: concurrent per-invite cap, counted from the shared
        // pre-commit membership state so every member — including one
        // that joined after the invite was issued — computes the same
        // count.
        enforce_max_uses(policy, &membership)?;

        // Walk the proposal set once, categorizing as we go. Returns a
        // summary the per-rule helpers consume; iterating once also
        // means we never have to re-walk for a different lens.
        let summary = collect_external_commit_proposals(staged_commit)?;

        // Checks 2 + 6: structural shape — counts and forbidden
        // proposal types.
        enforce_external_commit_structure(&summary)?;

        // Check 3: Add proposals must bind to the joiner's inbox id.
        let added_installations = enforce_adds_bind_to_joiner(&summary, &joiner_inbox_id)?;

        // Checks 5 + 11: the AppDataUpdate must insert exactly the
        // joiner's own GROUP_MEMBERSHIP entry, tagged with the active
        // external_group_id.
        enforce_app_data_update_scope(
            summary
                .app_data_update
                .expect("structure check guarantees Some when no failure"),
            &joiner_inbox_id,
            &policy.external_group_id,
        )?;

        // `added_inboxes` carries the joiner exactly once. We populate
        // `proposer: None` because the proposer attribution machinery
        // is built around `Sender::Member` leaf indices — the joiner
        // is not a member yet, so the right shape is to elide the
        // proposer rather than to attribute it to themselves with a
        // pre-commit leaf index that doesn't yet exist in the tree.
        let added_inboxes = vec![build_inbox(
            &joiner_inbox_id,
            immutable_metadata,
            mutable_metadata,
        )];

        let installations_changed = !added_installations.is_empty();

        Ok(Self {
            actor: joiner_participant,
            // External commits have no by-reference proposers from
            // existing members — the joiner is the sole authoring
            // party. We surface the same participant in `proposers`
            // for downstream consumers that look there for "who
            // wrote this commit".
            proposers: Vec::new(),
            added_inboxes,
            removed_inboxes: Vec::new(),
            readded_installations: HashSet::new(),
            metadata_validation_info: MutableMetadataValidationInfo::default(),
            installations_changed,
            permissions_changed: false,
            dm_members: immutable_metadata.dm_members.clone(),
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

        let group_id = GroupId::try_from(openmls_group.group_id())?;
        let expected_diff = Self::extract_expected_diff_with_proposers(
            context,
            &group_id,
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
        group_id: &GroupId, // used for logging
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

/// Receive-side enforcement of `MIN_SUPPORTED_PROTOCOL_VERSION`
/// monotonicity. A proposal that lowers the floor (or removes it
/// while one was set) is rejected before it can reach the dict.
/// `Update(new)` with `new >= old` (or `old` absent / unparseable)
/// passes. `Remove` with an existing floor fails — explicit unsetting
/// of the floor is just a downgrade in disguise.
///
/// This is the source-of-truth check; the send-side guard in
/// `update_group_min_version` is a friendlier UX layer over the same
/// invariant. An attacker patching out the send-side gate still hits
/// this one on every receiver.
fn enforce_min_version_monotonicity(
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    old_value: Option<&[u8]>,
) -> Result<(), CommitValidationError> {
    use openmls::messages::proposals::AppDataUpdateOperation;
    // First-set on a group with no prior floor is always allowed —
    // there's nothing to downgrade against.
    let Some(old_bytes) = old_value else {
        return Ok(());
    };
    // If the prior bytes don't parse as semver, we can't compare.
    // Treat as "no prior floor" and accept — refusing every future
    // update on a malformed prior would brick the group.
    let Ok(old_str) = std::str::from_utf8(old_bytes) else {
        return Ok(());
    };
    let Ok(old_v) = LibXMTPVersion::parse(old_str) else {
        return Ok(());
    };
    match operation {
        AppDataUpdateOperation::Update(payload) => {
            let new_bytes = payload.as_slice();
            let new_str = std::str::from_utf8(new_bytes).map_err(|_| {
                CommitValidationError::InvalidVersionFormat(format!("{:?}", new_bytes))
            })?;
            let new_v = LibXMTPVersion::parse(new_str)?;
            if new_v < old_v {
                return Err(CommitValidationError::MinVersionDowngrade {
                    requested: new_str.to_string(),
                    current: old_str.to_string(),
                });
            }
            Ok(())
        }
        AppDataUpdateOperation::Remove => {
            Err(CommitValidationError::MinVersionRemoveOnExistingFloor {
                current: old_str.to_string(),
            })
        }
    }
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

    // Source-of-truth monotonicity for `MIN_SUPPORTED_PROTOCOL_VERSION`.
    // Runs ahead of the per-element policy loop so a downgrade fails
    // fast with a structured error rather than passing the policy
    // check and silently relaxing the pause gate. See
    // `enforce_min_version_monotonicity` for the rule shape.
    if component_id
        == xmtp_mls_common::app_data::component_id::ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION
    {
        enforce_min_version_monotonicity(operation, old_value).inspect_err(|err| {
            tracing::warn!(
                proposer_inbox_id,
                component_id = %component_id,
                error = %err,
                "AppDataUpdate proposal rejected: min_version monotonicity"
            );
        })?;
    }

    // XIP-82 write-once rule: a member-sender write to GROUP_MEMBERSHIP
    // must carry every entry's `admitted_via_external_group_id` through
    // unchanged. Only this path needs the rule — the external-commit
    // validator (`from_external_commit`) never reaches here, and
    // `app_data_update_proposer_leaf` has already rejected non-member
    // senders.
    if component_id == xmtp_mls_common::app_data::component_id::ComponentId::GROUP_MEMBERSHIP {
        enforce_admitted_via_write_once(operation, old_value).inspect_err(|err| {
            tracing::warn!(
                proposer_inbox_id,
                component_id = %component_id,
                error = %err,
                "AppDataUpdate proposal rejected: admitted_via tag is write-once"
            );
        })?;
    }

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

    // Capture the LAST write to each of the two components the XIP-82
    // post-state invariant couples. For a Bytes component the Update
    // payload IS the post-state value, and within one commit the last
    // write wins (matching the accumulate order on apply).
    let mut policy_write: Option<Vec<u8>> = None;
    let mut registry_write: Option<Vec<u8>> = None;

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

        let component_id = ComponentId::from(app_data.component_id());
        validate_one_app_data_update(
            component_id,
            app_data.operation(),
            ActorAuthority::from(proposer),
            &proposer.inbox_id,
            registry,
            openmls_group,
        )?;

        if let openmls::messages::proposals::AppDataUpdateOperation::Update(payload) =
            app_data.operation()
        {
            if component_id == ComponentId::EXTERNAL_COMMIT_POLICY {
                policy_write = Some(payload.as_slice().to_vec());
            } else if component_id == ComponentId::COMPONENT_REGISTRY {
                registry_write = Some(payload.as_slice().to_vec());
            }
        }
    }

    if let Some(policy_bytes) = policy_write {
        validate_external_commit_policy_post_state(
            &policy_bytes,
            registry_write.as_deref(),
            registry,
        )?;
    }

    Ok(())
}

/// XIP-82 post-state invariant on `EXTERNAL_COMMIT_POLICY` writes,
/// enforced on every commit that carries one (member senders; the
/// external-commit validator path has its own checks):
///
/// * The proposed policy value satisfies the field-coupling invariants —
///   enabled ⇒ 32-byte key + ≥4-byte slot id (+ well-formed refresh
///   pointers); disabled ⇒ every per-invite field absent.
/// * Enabled ⇒ the **post-state** `GROUP_MEMBERSHIP`
///   `ComponentMetadata.external_committer_permissions` admits the
///   joiner-self-entry insert. Post-state means a `COMPONENT_REGISTRY`
///   write carried by the same commit counts — the enable commit is
///   REQUIRED to establish the grant when absent.
///
/// Convergent by construction: both checks are pure functions of the
/// commit's own proposal payloads plus the pre-commit registry, which
/// every member shares.
fn validate_external_commit_policy_post_state(
    policy_bytes: &[u8],
    registry_write: Option<&[u8]>,
    pre_registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
) -> Result<(), CommitValidationError> {
    use super::external_commit_policy::{
        ExternalCommitPolicyError, grant_admits_joiner_insert, validate_policy_v1,
    };
    use prost::Message;
    use xmtp_mls_common::app_data::component_id::ComponentId;
    use xmtp_mls_common::tls_map::{TlsMapDelta, TlsMapMutation};
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentMetadata, ExternalCommitPolicyEntry,
        external_commit_policy_entry::Version as ExternalCommitPolicyVersion,
    };

    let malformed = |component_id, reason: String| {
        CommitValidationError::ComponentSource(
            super::app_data::component_source::ComponentSourceError::MalformedComponentValue {
                component_id,
                reason,
            },
        )
    };

    let entry = ExternalCommitPolicyEntry::decode(policy_bytes).map_err(|e| {
        malformed(
            ComponentId::EXTERNAL_COMMIT_POLICY,
            format!("ExternalCommitPolicyEntry decode: {e}"),
        )
    })?;
    // Unknown future variant: older validators cannot check invariants
    // they don't understand — same unknown-variant tolerance as the
    // policy reader (a newer client validates it; this client fails
    // closed at external-commit time regardless).
    let Some(ExternalCommitPolicyVersion::V1(policy)) = entry.version else {
        return Ok(());
    };

    validate_policy_v1(&policy)?;

    if !policy.allow_external_commit {
        return Ok(());
    }

    // Post-state GROUP_MEMBERSHIP metadata: a registry write in the same
    // commit supersedes the pre-commit registry entry (last write wins,
    // matching apply order). A Delete of the GROUP_MEMBERSHIP entry in
    // the same commit counts as post-state ABSENT — it must not fall
    // back to the (stale) pre-commit grant.
    enum InCommit {
        Untouched,
        Written(ComponentMetadata),
        Deleted,
    }
    let in_commit_metadata = match registry_write {
        Some(delta_bytes) => {
            use tls_codec::Deserialize;
            let delta =
                TlsMapDelta::<ComponentId, tls_codec::VLBytes>::tls_deserialize_exact(delta_bytes)
                    .map_err(|e| {
                        malformed(
                            ComponentId::COMPONENT_REGISTRY,
                            format!("registry delta decode: {e}"),
                        )
                    })?;
            delta
                .mutations
                .iter()
                .rev()
                .find_map(|mutation| match mutation {
                    TlsMapMutation::Insert { key, value }
                    | TlsMapMutation::Update { key, value }
                        if *key == ComponentId::GROUP_MEMBERSHIP =>
                    {
                        Some(ComponentMetadata::decode(value.as_slice()).map(InCommit::Written))
                    }
                    TlsMapMutation::Delete { key } if *key == ComponentId::GROUP_MEMBERSHIP => {
                        Some(Ok(InCommit::Deleted))
                    }
                    _ => None,
                })
                .transpose()
                .map_err(|e| {
                    malformed(
                        ComponentId::GROUP_MEMBERSHIP,
                        format!("ComponentMetadata decode: {e}"),
                    )
                })?
                .unwrap_or(InCommit::Untouched)
        }
        None => InCommit::Untouched,
    };
    let membership_metadata = match in_commit_metadata {
        InCommit::Written(meta) => Some(meta),
        InCommit::Deleted => None,
        InCommit::Untouched => pre_registry
            .get(&ComponentId::GROUP_MEMBERSHIP)
            .ok()
            .flatten(),
    };

    if !grant_admits_joiner_insert(
        membership_metadata
            .as_ref()
            .and_then(|m| m.external_committer_permissions.as_ref()),
    ) {
        return Err(ExternalCommitPolicyError::MissingMembershipGrant.into());
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
        // External joins/commits don't flow through this helper. The
        // joiner's leaf is not in the tree at the pre-commit snapshot
        // captured by `member_at`, so callers on the external-commit
        // path build their participant directly from the staged
        // commit's `update_path_leaf_node` via
        // [`CommitParticipant::from_leaf_node`]. Reaching this branch
        // means someone routed a `Sender::NewMemberCommit` proposal
        // through the member-only validator, which is a programmer
        // error rather than a peer-attributable failure — surface it
        // as `ActorNotMember` so the caller treats the commit as
        // rejected.
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

// =============================================================================
// External-commit validator helpers (L-7).
//
// Pure functions extracted from `ValidatedCommit::from_external_commit` so the
// per-rule logic can be unit-tested without the considerable scaffolding
// required to construct a real `StagedCommit`. The orchestrator stays
// readable and the rules stay individually pinned.
// =============================================================================

/// Categorized view of the proposals carried by an external commit.
///
/// Populated by [`collect_external_commit_proposals`]: a single pass over
/// `staged_commit.queued_proposals()` that fans out into typed buckets so
/// downstream rule-checks operate on Rust references rather than re-walking
/// the queue.
struct ExternalCommitProposalSummary<'a> {
    /// Number of `ExternalInit` proposals seen. Must be exactly 1.
    external_init_count: usize,
    /// All Add proposals, by-value.
    adds: Vec<&'a openmls::messages::proposals::AddProposal>,
    /// The single AppDataUpdate proposal, if exactly one was present.
    app_data_update: Option<&'a openmls::messages::proposals::AppDataUpdateProposal>,
    /// Number of AppDataUpdate proposals seen — pulled out separately
    /// so the structure check can distinguish "missing" vs "too many".
    app_data_update_count: usize,
    /// True if a `SelfRemove` proposal was seen — drives the
    /// resync-not-supported rejection.
    saw_self_remove: bool,
    /// True if a `GroupContextExtensions` proposal was seen.
    saw_gce: bool,
    /// The proposal type of the first encountered "other" proposal
    /// (PreSharedKey/Update/Remove/ReInit/Custom/AppEphemeral/_AppAck),
    /// if any. Captured for inclusion in the
    /// `ExternalCommitUnsupportedProposalType` error payload. PSKs are
    /// forbidden per XIP-82: a non-member has no pre-shared key it can
    /// legitimately reference with the group, so admitting them is
    /// gratuitous attack surface in v1.
    first_unsupported: Option<ProposalType>,
    /// True if any proposal carried `ProposalOrRefType::Reference`.
    saw_by_reference: bool,
}

impl<'a> ExternalCommitProposalSummary<'a> {
    fn new() -> Self {
        Self {
            external_init_count: 0,
            adds: Vec::new(),
            app_data_update: None,
            app_data_update_count: 0,
            saw_self_remove: false,
            saw_gce: false,
            first_unsupported: None,
            saw_by_reference: false,
        }
    }
}

/// Reject if the group has not opted into accepting MLS External
/// Commits (XIP-82 check 7). This is the first state gate —
/// short-circuiting here means a denied-policy group never pays for
/// proposal-shape walks.
///
/// `policy` is the decoded AppData-resident `EXTERNAL_COMMIT_POLICY`
/// entry (`None` = absent or unrecognized version). Fails closed on
/// anything but an explicit `allow_external_commit = true`; returns
/// the policy so the caller's later checks (time bounds, tag,
/// max_uses) read the same decoded value.
fn enforce_external_commit_policy(
    policy: Option<&ExternalCommitPolicyV1>,
) -> Result<&ExternalCommitPolicyV1, CommitValidationError> {
    match policy {
        Some(policy) if policy.allow_external_commit => Ok(policy),
        _ => Err(CommitValidationError::ExternalCommitNotAllowed),
    }
}

/// XIP-82 checks 8 + 9: the two envelope-timestamp time bounds.
///
/// Check 8 — absolute campaign expiry: when `expires_at_ns` is set
/// (non-zero), the commit's envelope timestamp must not exceed it.
///
/// Check 9 — per-epoch staleness: when `expire_in_ns` is non-zero, the
/// commit must land within `expire_in_ns` of the current epoch's
/// start. `saturating_sub` covers the benign skew case where the
/// epoch-start envelope was published marginally after the commit's
/// (cross-node clock skew): the age clamps to zero rather than
/// underflowing into an astronomically large value that would reject
/// every join.
fn enforce_external_commit_time_bounds(
    policy: &ExternalCommitPolicyV1,
    timestamps: &ExternalCommitTimestamps,
) -> Result<(), CommitValidationError> {
    if policy.expires_at_ns != 0 && timestamps.commit_envelope_ns > policy.expires_at_ns {
        return Err(CommitValidationError::ExternalCommitInviteExpired {
            expires_at_ns: policy.expires_at_ns,
            commit_envelope_ns: timestamps.commit_envelope_ns,
        });
    }
    if policy.expire_in_ns != 0 {
        let age_ns = timestamps
            .commit_envelope_ns
            .saturating_sub(timestamps.epoch_started_at_ns);
        if age_ns > policy.expire_in_ns {
            return Err(CommitValidationError::ExternalCommitInviteStale {
                age_ns,
                expire_in_ns: policy.expire_in_ns,
            });
        }
    }
    Ok(())
}

/// XIP-82 check 4: the joiner must not already be in the group — on
/// either layer of the cross-coupled membership state. An existing
/// member re-adding itself via external commit could rewrite (or
/// re-tag) its own membership entry; recovering a broken installation
/// is the deferred resync flavor, not v1.
fn enforce_joiner_not_already_member(
    joiner_inbox_id: &str,
    tree_inbox_ids: &HashSet<String>,
    membership: &std::collections::BTreeMap<
        xmtp_mls_common::inbox_id::InboxId,
        xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry,
    >,
) -> Result<(), CommitValidationError> {
    if tree_inbox_ids.contains(joiner_inbox_id) {
        return Err(CommitValidationError::ExternalCommitJoinerAlreadyMember {
            inbox_id: joiner_inbox_id.to_string(),
        });
    }
    // A credential whose inbox id doesn't parse as hex can't be a
    // member of anything — but reject it explicitly rather than
    // treating it as "not present".
    let joiner_key = xmtp_mls_common::inbox_id::InboxId::from_hex(joiner_inbox_id)
        .map_err(|_| CommitValidationError::InboxValidationFailed(joiner_inbox_id.to_string()))?;
    if membership.contains_key(&joiner_key) {
        return Err(CommitValidationError::ExternalCommitJoinerAlreadyMember {
            inbox_id: joiner_inbox_id.to_string(),
        });
    }
    Ok(())
}

/// XIP-82 check 12: when the policy caps concurrent invited members
/// (`max_uses != 0`), the number of current `GROUP_MEMBERSHIP` entries
/// tagged with the active `external_group_id` must be strictly less
/// than the cap — the commit that would create the (`max_uses`+1)-th
/// concurrent invited member is rejected. Counted from the shared
/// pre-commit group state (never replayed from commit history) so
/// every member computes it identically. Removing a tagged member
/// frees its slot; the tag disappears with the entry.
fn enforce_max_uses(
    policy: &ExternalCommitPolicyV1,
    membership: &std::collections::BTreeMap<
        xmtp_mls_common::inbox_id::InboxId,
        xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry,
    >,
) -> Result<(), CommitValidationError> {
    use xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version;

    if policy.max_uses == 0 {
        return Ok(());
    }
    // Defensive: an enabled policy always carries a ≥4-byte
    // external_group_id (write-time invariant), but if state predating
    // that invariant slipped through, matching the empty id would
    // count every Welcome-added member (whose tag is absent ≡ empty).
    if policy.external_group_id.is_empty() {
        return Ok(());
    }
    let used = membership
        .values()
        .filter(|entry| match &entry.version {
            Some(Version::V1(v1)) => v1.admitted_via_external_group_id == policy.external_group_id,
            // `decode_group_membership_dict` rejects version-less
            // entries before we get here; an unmatchable future
            // variant counting as untagged is the conservative
            // direction (undercounts, never blocks legitimate joins
            // spuriously).
            None => false,
        })
        .count();
    if used >= policy.max_uses as usize {
        return Err(CommitValidationError::ExternalCommitMaxUsesExhausted {
            max_uses: policy.max_uses,
        });
    }
    Ok(())
}

/// Reject if the commit's framing sender is not `Sender::NewMemberCommit`.
/// Defensive: the wider message dispatch should route by sender before
/// reaching this validator. We also assert against `Sender::Member` here
/// so an attacker who somehow gets a member-authored commit dispatched
/// to the external path can't bypass the `Sender::Member` validator's
/// stricter membership/permission checks.
fn enforce_external_commit_sender(sender: &Sender) -> Result<(), CommitValidationError> {
    match sender {
        Sender::NewMemberCommit => Ok(()),
        _ => Err(CommitValidationError::ExternalCommitNotNewMemberCommit),
    }
}

/// Single-pass categorization of every proposal in `staged_commit`.
///
/// Returns an `ExternalCommitProposalSummary` whose buckets the
/// downstream rule-checks consume. Surfaces only one error of its own:
/// it never accepts a by-reference proposal (RFC 9420 §12.4.3.2) and
/// flags the first sighting so the caller can reject the commit
/// wholesale.
fn collect_external_commit_proposals(
    staged_commit: &StagedCommit,
) -> Result<ExternalCommitProposalSummary<'_>, CommitValidationError> {
    let mut summary = ExternalCommitProposalSummary::new();

    for queued in staged_commit.queued_proposals() {
        // Rule 4: no by-reference proposals. Captured here rather than
        // in the structure check because the proposal_or_ref_type only
        // exists on `QueuedProposal`, not on the post-categorization
        // typed bucket — so it has to happen during the walk.
        if matches!(queued.proposal_or_ref_type(), ProposalOrRefType::Reference) {
            summary.saw_by_reference = true;
        }

        match queued.proposal() {
            Proposal::ExternalInit(_) => {
                summary.external_init_count += 1;
            }
            Proposal::Add(add) => {
                summary.adds.push(add.as_ref());
            }
            Proposal::AppDataUpdate(app_data) => {
                summary.app_data_update_count += 1;
                if summary.app_data_update.is_none() {
                    summary.app_data_update = Some(app_data.as_ref());
                }
            }
            Proposal::SelfRemove => {
                summary.saw_self_remove = true;
            }
            Proposal::GroupContextExtensions(_) => {
                summary.saw_gce = true;
            }
            other => {
                if summary.first_unsupported.is_none() {
                    summary.first_unsupported = Some(other.proposal_type());
                }
            }
        }
    }

    Ok(summary)
}

/// Apply rules 3, 8, 9, 10 against the categorized proposal summary.
///
/// Order matters: we surface the most-specific failure first so error
/// messages and tests pin a single canonical reason per violation
/// shape. "Missing ExternalInit" is the strongest "this commit is not
/// an external commit at all" signal, so it comes ahead of "wrong
/// proposal type" failures.
fn enforce_external_commit_structure(
    summary: &ExternalCommitProposalSummary<'_>,
) -> Result<(), CommitValidationError> {
    // RFC 9420 §12.4.3.2: no by-reference proposals.
    if summary.saw_by_reference {
        return Err(CommitValidationError::ExternalCommitByReferenceProposalsForbidden);
    }

    // ExternalInit count: exactly one.
    match summary.external_init_count {
        0 => return Err(CommitValidationError::ExternalCommitMissingExternalInit),
        1 => {}
        _ => return Err(CommitValidationError::ExternalCommitMultipleExternalInit),
    }

    // Forbidden proposal kinds, in order of specificity.
    if summary.saw_self_remove {
        return Err(CommitValidationError::ResyncExternalCommitNotSupported);
    }
    if summary.saw_gce {
        return Err(CommitValidationError::ExternalCommitGceForbidden);
    }
    if let Some(unsupported) = summary.first_unsupported {
        return Err(CommitValidationError::ExternalCommitUnsupportedProposalType(unsupported));
    }

    // AppDataUpdate count: exactly one.
    match summary.app_data_update_count {
        0 => return Err(CommitValidationError::ExternalCommitAppDataUpdateMissing),
        1 => {}
        _ => return Err(CommitValidationError::ExternalCommitAppDataUpdateMultiple),
    }

    Ok(())
}

/// Apply rule 6: every Add proposal's KeyPackage credential MUST carry
/// the same inbox id as the joiner's path leaf.
///
/// Returns the set of installation ids added (signature keys from the
/// Add proposals' leaf nodes) for population into the resulting
/// `ValidatedCommit`. The path-leaf itself is the joiner's "primary"
/// installation; whether that signature key is also represented as an
/// Add proposal is up to the sender — we do not deduplicate here.
fn enforce_adds_bind_to_joiner(
    summary: &ExternalCommitProposalSummary<'_>,
    joiner_inbox_id: &str,
) -> Result<HashSet<Vec<u8>>, CommitValidationError> {
    let mut added_installations: HashSet<Vec<u8>> = HashSet::new();
    for add in &summary.adds {
        let leaf = add.key_package().leaf_node();
        let inbox_id = inbox_id_from_credential(leaf.credential())?;
        if inbox_id != joiner_inbox_id {
            return Err(CommitValidationError::CrossInboxAddInExternalCommit);
        }
        added_installations.insert(leaf.signature_key().as_slice().to_vec());
    }
    Ok(added_installations)
}

/// Apply XIP-82 checks 5 + 11: the single AppDataUpdate proposal MUST
/// target the `GROUP_MEMBERSHIP` component, and its
/// `TlsMapDelta<InboxId, VLBytes>` payload MUST consist of exactly one
/// `Insert` of the joiner's own inbox id, whose entry value records
/// `admitted_via_external_group_id` equal to the active
/// `external_group_id`.
///
/// Insert-only is not an extra restriction beyond the XIP's "modifying
/// only the joiner's entry": check 4 guarantees the joiner has no
/// existing entry, and `TlsMapDelta` apply semantics fail an
/// `Update`/`Delete` on a missing key — so any other mutation shape
/// could never merge anyway. Rejecting it here keeps the failure
/// structured and pre-merge on every member. A `Remove` operation
/// (wiping the entire component) is likewise not a join-time
/// operation.
///
/// The joiner's `ActorAuthority` (non-admin, non-super-admin) is *not*
/// checked here against `validate_one_app_data_update`: the joiner's
/// write is admitted by the `external_committer_permissions` block
/// (check 10), the symmetric twin of the member-facing `permissions`,
/// and bounded to their own entry by this scope check.
fn enforce_app_data_update_scope(
    proposal: &openmls::messages::proposals::AppDataUpdateProposal,
    joiner_inbox_id: &str,
    active_external_group_id: &[u8],
) -> Result<(), CommitValidationError> {
    use tls_codec::Deserialize as TlsDeserialize;
    use xmtp_mls_common::app_data::component_id::ComponentId;
    use xmtp_mls_common::inbox_id::InboxId;
    use xmtp_mls_common::tls_map::{TlsMapDelta, TlsMapMutation};
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry, group_membership_entry::Version as GroupMembershipEntryVersion,
    };

    if ComponentId::from(proposal.component_id()) != ComponentId::GROUP_MEMBERSHIP {
        return Err(CommitValidationError::ExternalCommitAppDataUpdateWrongComponent);
    }

    let payload = match proposal.operation() {
        AppDataUpdateOperation::Update(bytes) => bytes,
        AppDataUpdateOperation::Remove => {
            return Err(CommitValidationError::ExternalCommitAppDataUpdateOutOfScope);
        }
    };

    // Parse the delta. Treat any decoding failure as a wire-format
    // violation rather than letting it surface as a silent "no
    // mutations to check".
    let delta =
        TlsMapDelta::<InboxId, tls_codec::VLBytes>::tls_deserialize_exact(payload.as_slice())
            .map_err(|e| {
                CommitValidationError::ExternalCommitAppDataUpdatePayloadMalformed(e.to_string())
            })?;

    // Parse the joiner's inbox-id string once and compare by raw
    // bytes. We avoid re-encoding each mutation's key back to a hex
    // string in the hot path.
    let joiner_id = InboxId::from_hex(joiner_inbox_id).map_err(|e| {
        CommitValidationError::ExternalCommitAppDataUpdatePayloadMalformed(format!(
            "joiner inbox id is not valid hex: {e}"
        ))
    })?;

    // Exactly one mutation, and it must be the joiner's own Insert.
    // (Covers the degenerate empty delta too — a joiner that didn't
    // actually register themselves.)
    let [TlsMapMutation::Insert { key, value }] = delta.mutations.as_slice() else {
        return Err(CommitValidationError::ExternalCommitAppDataUpdateOutOfScope);
    };
    if key != &joiner_id {
        return Err(CommitValidationError::ExternalCommitAppDataUpdateOutOfScope);
    }

    // Check 11: the inserted entry must record the invite it was
    // admitted under. Required on every external commit — whatever
    // `max_uses` is — so a later policy change to a finite cap starts
    // from accurate data. Fail closed on an undecodable or
    // unknown-version entry: we could not verify the tag, and an
    // opaque blob in the membership map would hide the tag from every
    // V1 reader's max_uses count.
    let entry = GroupMembershipEntry::decode(value.as_slice())
        .map_err(|e| CommitValidationError::GroupMembershipEntryMalformed(e.to_string()))?;
    let Some(GroupMembershipEntryVersion::V1(v1)) = entry.version else {
        return Err(CommitValidationError::GroupMembershipEntryMalformed(
            "unknown GroupMembershipEntry version".into(),
        ));
    };
    if v1.admitted_via_external_group_id != active_external_group_id {
        return Err(CommitValidationError::ExternalCommitAdmittedViaTagMismatch);
    }

    Ok(())
}

/// XIP-82 write-once enforcement for `admitted_via_external_group_id`
/// on member-sender `GROUP_MEMBERSHIP` writes: the tag is set exactly
/// once by the admitting external commit and is immutable for the life
/// of the entry. A member commit that sets, clears, or alters it — its
/// own entry included — is rejected; otherwise an invited member could
/// untag itself and free a `max_uses` slot at will. Entry rewrites for
/// unrelated reasons (an installation change bumping `sequence_id`,
/// say) must carry the tag through unchanged. Deleting an entry is the
/// one legal way its tag disappears (member removal frees the slot).
///
/// Runs only on the member-sender validation path
/// ([`validate_one_app_data_update_with_old_value`]); the external
/// commit that legitimately *sets* the tag is validated by
/// [`enforce_app_data_update_scope`] instead and never reaches this
/// check.
fn enforce_admitted_via_write_once(
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    old_value: Option<&[u8]>,
) -> Result<(), CommitValidationError> {
    use tls_codec::Deserialize as TlsDeserialize;
    use xmtp_mls_common::app_data::migration::decode_group_membership_dict;
    use xmtp_mls_common::inbox_id::InboxId;
    use xmtp_mls_common::tls_map::{TlsMapDelta, TlsMapMutation};
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry, group_membership_entry::Version as GroupMembershipEntryVersion,
    };

    let AppDataUpdateOperation::Update(payload) = operation else {
        // `Remove` wipes the whole component — every entry (and its
        // tag) disappears together, the same way a single entry's tag
        // goes away with a Delete mutation. Whether the wipe itself is
        // permitted is the registry `delete_policy`'s call, not this
        // check's.
        return Ok(());
    };

    let delta =
        TlsMapDelta::<InboxId, tls_codec::VLBytes>::tls_deserialize_exact(payload.as_slice())
            .map_err(|e| {
                CommitValidationError::ExternalCommitAppDataUpdatePayloadMalformed(e.to_string())
            })?;

    // Decode the pre-commit entries the mutations rewrite. Fail closed
    // on malformed prior state: this is consensus state produced by
    // validated commits, so a decode failure means something is deeply
    // wrong — skipping the check would be the only way a tag rewrite
    // could slip through.
    let old_entries = match old_value {
        Some(bytes) => decode_group_membership_dict(bytes)
            .map_err(|e| CommitValidationError::GroupMembershipEntryMalformed(e.to_string()))?,
        None => Default::default(),
    };

    for mutation in &delta.mutations {
        let (key, new_value) = match mutation {
            TlsMapMutation::Insert { key, value } | TlsMapMutation::Update { key, value } => {
                (key, value)
            }
            TlsMapMutation::Delete { .. } => continue,
        };
        // Fail closed on an undecodable or unknown-version new value:
        // accepting an opaque blob over a tagged V1 entry would hide
        // the tag from every V1 reader and free the max_uses slot.
        let new_entry = GroupMembershipEntry::decode(new_value.as_slice())
            .map_err(|e| CommitValidationError::GroupMembershipEntryMalformed(e.to_string()))?;
        let Some(GroupMembershipEntryVersion::V1(new_v1)) = new_entry.version else {
            return Err(CommitValidationError::GroupMembershipEntryMalformed(
                "unknown GroupMembershipEntry version".into(),
            ));
        };
        // Absent tag ≡ empty bytes (proto3 scalar default — there is
        // exactly one cleared encoding), so a fresh Insert compares
        // against empty: members may only ever add untagged entries.
        let old_tag: &[u8] = old_entries
            .get(key)
            .and_then(|entry| entry.version.as_ref())
            .map(|GroupMembershipEntryVersion::V1(v1)| v1.admitted_via_external_group_id.as_slice())
            .unwrap_or(&[]);
        if new_v1.admitted_via_external_group_id != old_tag {
            return Err(CommitValidationError::AdmittedViaTagImmutable {
                inbox_id: key.to_hex(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod external_commit_validator_tests {
    //! Pins the rule-by-rule behavior of
    //! [`ValidatedCommit::from_external_commit`]. Each helper is exercised
    //! directly so the tests don't have to construct a real
    //! `StagedCommit` (which requires a full MLS group, identity, and
    //! crypto provider). The orchestrator function itself is exercised
    //! indirectly via integration tests in L-8/L-10/L-11.
    use super::*;
    use openmls::messages::proposals::AppDataUpdateProposal;
    use std::collections::BTreeMap;
    use tls_codec::{Serialize as TlsSerialize, VLBytes};
    use xmtp_mls_common::app_data::component_id::ComponentId as XmtpComponentId;
    use xmtp_mls_common::inbox_id::{INBOX_ID_BYTE_LEN, InboxId};
    use xmtp_mls_common::tls_map::TlsMapDelta;
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry, SymmetricKey,
        group_membership_entry::{V1 as MembershipEntryV1, Version as MembershipEntryVersion},
    };

    /// The active invite slot id used across these tests.
    const ACTIVE_SLOT: &[u8] = b"slot-aaaa";

    /// Build a hex inbox-id string with a stable seed byte so different
    /// test inboxes are easy to compare visually.
    fn make_inbox_id_hex(seed: u8) -> String {
        hex::encode([seed; INBOX_ID_BYTE_LEN])
    }

    fn make_inbox_id(seed: u8) -> InboxId {
        InboxId::from_bytes([seed; INBOX_ID_BYTE_LEN])
    }

    /// An enabled policy with the canonical coordinates these tests
    /// assert against. Field-coupling-valid per `validate_policy_v1`.
    fn enabled_policy() -> ExternalCommitPolicyV1 {
        ExternalCommitPolicyV1 {
            allow_external_commit: true,
            symmetric_key: Some(SymmetricKey {
                material: vec![7u8; 32],
            }),
            external_group_id: ACTIVE_SLOT.to_vec(),
            ..Default::default()
        }
    }

    /// Encode a `GroupMembershipEntry::V1` value with the given
    /// admitted-via tag (empty = untagged / Welcome-added).
    fn entry_bytes(tag: &[u8]) -> Vec<u8> {
        GroupMembershipEntry {
            version: Some(MembershipEntryVersion::V1(MembershipEntryV1 {
                sequence_id: 1,
                failed_installations: vec![],
                admitted_via_external_group_id: tag.to_vec(),
            })),
        }
        .encode_to_vec()
    }

    /// A decoded membership map with one entry per `(seed, tag)`.
    fn membership_map(entries: &[(u8, &[u8])]) -> BTreeMap<InboxId, GroupMembershipEntry> {
        entries
            .iter()
            .map(|(seed, tag)| {
                (
                    make_inbox_id(*seed),
                    GroupMembershipEntry::decode(entry_bytes(tag).as_slice()).expect("decode"),
                )
            })
            .collect()
    }

    /// Encode a `TlsMapDelta` of `(InboxId, VLBytes)` mutations to the
    /// wire payload an `AppDataUpdate::Update` proposal carries.
    fn encode_membership_delta(delta: &TlsMapDelta<InboxId, VLBytes>) -> Vec<u8> {
        delta.tls_serialize_detached().expect("delta serialize")
    }

    /// Build an `AppDataUpdate(GROUP_MEMBERSHIP, Update(<insert joiner>))`
    /// proposal — the canonical shape produced by the L-10/L-11 sender:
    /// one Insert of the joiner's entry, tagged with the active slot.
    fn well_formed_membership_update_for(joiner: InboxId) -> AppDataUpdateProposal {
        let entry: VLBytes = entry_bytes(ACTIVE_SLOT).into();
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(joiner, entry);
        let bytes = encode_membership_delta(&delta);
        AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes)
    }

    // ── enforce_external_commit_policy ───────────────────────────────

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_when_allow_external_commit_is_false() {
        let policy = ExternalCommitPolicyV1::default();
        let err = enforce_external_commit_policy(Some(&policy))
            .expect_err("disabled policy must reject external commits");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitNotAllowed
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_when_policy_absent() {
        let err = enforce_external_commit_policy(None)
            .expect_err("absent policy component must fail closed");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitNotAllowed
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_when_allow_external_commit_is_true() {
        let policy = enabled_policy();
        assert!(enforce_external_commit_policy(Some(&policy)).is_ok());
    }

    // ── enforce_external_commit_time_bounds ──────────────────────────

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_when_no_time_bounds_set() {
        // expires_at_ns == 0 and expire_in_ns == 0: no bound applies,
        // whatever the envelope timestamps say.
        let policy = enabled_policy();
        let ts = ExternalCommitTimestamps {
            commit_envelope_ns: u64::MAX,
            epoch_started_at_ns: 0,
        };
        assert!(enforce_external_commit_time_bounds(&policy, &ts).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_commit_past_absolute_expiry() {
        let policy = ExternalCommitPolicyV1 {
            expires_at_ns: 1_000,
            ..enabled_policy()
        };
        let ts = ExternalCommitTimestamps {
            commit_envelope_ns: 1_001,
            epoch_started_at_ns: 0,
        };
        let err = enforce_external_commit_time_bounds(&policy, &ts)
            .expect_err("commit envelope past expires_at_ns must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitInviteExpired {
                expires_at_ns: 1_000,
                commit_envelope_ns: 1_001,
            }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_commit_at_exact_absolute_expiry() {
        // Bound is "does not exceed": equality is still inside.
        let policy = ExternalCommitPolicyV1 {
            expires_at_ns: 1_000,
            ..enabled_policy()
        };
        let ts = ExternalCommitTimestamps {
            commit_envelope_ns: 1_000,
            epoch_started_at_ns: 0,
        };
        assert!(enforce_external_commit_time_bounds(&policy, &ts).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_commit_past_staleness_window() {
        let policy = ExternalCommitPolicyV1 {
            expire_in_ns: 500,
            ..enabled_policy()
        };
        let ts = ExternalCommitTimestamps {
            commit_envelope_ns: 2_000,
            epoch_started_at_ns: 1_000,
        };
        let err = enforce_external_commit_time_bounds(&policy, &ts)
            .expect_err("commit landing past epoch_start + expire_in_ns must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitInviteStale {
                age_ns: 1_000,
                expire_in_ns: 500,
            }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_commit_within_staleness_window() {
        let policy = ExternalCommitPolicyV1 {
            expire_in_ns: 500,
            ..enabled_policy()
        };
        let ts = ExternalCommitTimestamps {
            commit_envelope_ns: 1_400,
            epoch_started_at_ns: 1_000,
        };
        assert!(enforce_external_commit_time_bounds(&policy, &ts).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn staleness_age_clamps_on_publish_latency_skew() {
        // The epoch-start envelope can postdate the commit's by publish
        // latency; the age clamps to zero instead of underflowing into
        // a rejection.
        let policy = ExternalCommitPolicyV1 {
            expire_in_ns: 500,
            ..enabled_policy()
        };
        let ts = ExternalCommitTimestamps {
            commit_envelope_ns: 999,
            epoch_started_at_ns: 1_000,
        };
        assert!(enforce_external_commit_time_bounds(&policy, &ts).is_ok());
    }

    // ── enforce_joiner_not_already_member ────────────────────────────

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_joiner_with_existing_tree_leaf() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let tree: HashSet<String> = [joiner_hex.clone()].into();
        let membership = membership_map(&[]);
        let err = enforce_joiner_not_already_member(&joiner_hex, &tree, &membership)
            .expect_err("existing tree leaf must reject the re-join");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitJoinerAlreadyMember { .. }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_joiner_with_existing_membership_entry() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let tree: HashSet<String> = HashSet::new();
        let membership = membership_map(&[(0x11, b"")]);
        let err = enforce_joiner_not_already_member(&joiner_hex, &tree, &membership)
            .expect_err("existing GROUP_MEMBERSHIP entry must reject the re-join");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitJoinerAlreadyMember { .. }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_genuinely_new_joiner() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let tree: HashSet<String> = [make_inbox_id_hex(0x22)].into();
        let membership = membership_map(&[(0x22, b"")]);
        assert!(enforce_joiner_not_already_member(&joiner_hex, &tree, &membership).is_ok());
    }

    // ── enforce_max_uses ─────────────────────────────────────────────

    #[xmtp_common::test(unwrap_try = true)]
    fn max_uses_zero_is_unlimited() {
        let policy = enabled_policy();
        let membership = membership_map(&[(0x11, ACTIVE_SLOT), (0x22, ACTIVE_SLOT)]);
        assert!(enforce_max_uses(&policy, &membership).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn max_uses_admits_below_cap() {
        let policy = ExternalCommitPolicyV1 {
            max_uses: 2,
            ..enabled_policy()
        };
        let membership = membership_map(&[(0x11, ACTIVE_SLOT)]);
        assert!(enforce_max_uses(&policy, &membership).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn max_uses_rejects_at_cap() {
        let policy = ExternalCommitPolicyV1 {
            max_uses: 2,
            ..enabled_policy()
        };
        let membership = membership_map(&[(0x11, ACTIVE_SLOT), (0x22, ACTIVE_SLOT)]);
        let err = enforce_max_uses(&policy, &membership)
            .expect_err("the (max_uses+1)-th concurrent invited member must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitMaxUsesExhausted { max_uses: 2 }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn max_uses_ignores_untagged_and_other_slot_entries() {
        // Welcome-added members (empty tag) and members admitted under
        // a previous, rotated slot don't consume the active cap.
        let policy = ExternalCommitPolicyV1 {
            max_uses: 2,
            ..enabled_policy()
        };
        let membership = membership_map(&[(0x11, b""), (0x22, b"slot-old"), (0x33, ACTIVE_SLOT)]);
        assert!(enforce_max_uses(&policy, &membership).is_ok());
    }

    // ── enforce_external_commit_sender ───────────────────────────────

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_new_member_commit_sender() {
        assert!(enforce_external_commit_sender(&Sender::NewMemberCommit).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_member_sender() {
        let err = enforce_external_commit_sender(&Sender::Member(LeafNodeIndex::new(0)))
            .expect_err("Sender::Member must be rejected on the external path");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitNotNewMemberCommit
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_new_member_proposal_sender() {
        let err = enforce_external_commit_sender(&Sender::NewMemberProposal)
            .expect_err("Sender::NewMemberProposal must be rejected on the external path");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitNotNewMemberCommit
        ));
    }

    // ── enforce_external_commit_structure ────────────────────────────

    fn summary_for_structure_test() -> ExternalCommitProposalSummary<'static> {
        // We don't need real proposal references for the structure
        // check — the counts and flags are what gate the verdict. The
        // `adds` and `app_data_update` fields stay empty; the
        // structure check only looks at counts.
        ExternalCommitProposalSummary::new()
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_without_external_init_proposal() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 0;
        summary.app_data_update_count = 1;
        let err = enforce_external_commit_structure(&summary).expect_err("0 ExternalInit");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitMissingExternalInit
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_with_two_external_init_proposals() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 2;
        summary.app_data_update_count = 1;
        let err = enforce_external_commit_structure(&summary).expect_err("2 ExternalInit");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitMultipleExternalInit
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_with_by_reference_proposals() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        summary.saw_by_reference = true;
        let err = enforce_external_commit_structure(&summary)
            .expect_err("by-reference proposals must be rejected (RFC 9420 §12.4.3.2)");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitByReferenceProposalsForbidden
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_self_remove_proposal_resync_flavor() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        summary.saw_self_remove = true;
        let err = enforce_external_commit_structure(&summary)
            .expect_err("resync flavor must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ResyncExternalCommitNotSupported
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_group_context_extensions_proposal() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        summary.saw_gce = true;
        let err = enforce_external_commit_structure(&summary)
            .expect_err("GCE proposals not allowed in external commits");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitGceForbidden
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_update_proposal() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        summary.first_unsupported = Some(ProposalType::Update);
        let err = enforce_external_commit_structure(&summary)
            .expect_err("Update proposals not allowed in external commits");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitUnsupportedProposalType(ProposalType::Update)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_remove_proposal() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        summary.first_unsupported = Some(ProposalType::Remove);
        let err = enforce_external_commit_structure(&summary)
            .expect_err("Remove proposals not allowed in external commits");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitUnsupportedProposalType(ProposalType::Remove)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_when_no_app_data_update() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 0;
        let err = enforce_external_commit_structure(&summary)
            .expect_err("missing AppDataUpdate must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateMissing
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_with_two_app_data_update_proposals() {
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 2;
        let err = enforce_external_commit_structure(&summary)
            .expect_err("multiple AppDataUpdates must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateMultiple
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_canonical_external_commit_shape() {
        // 1 ExternalInit + 1 AppDataUpdate + 0 unsupported + 0 ref → ok
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        assert!(enforce_external_commit_structure(&summary).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_psk_proposal() {
        // XIP-82: PSKs are forbidden in external commits — a non-member
        // has no pre-shared key it can legitimately reference with the
        // group. They land in `first_unsupported` like any other
        // illegal proposal type.
        let mut summary = summary_for_structure_test();
        summary.external_init_count = 1;
        summary.app_data_update_count = 1;
        summary.first_unsupported = Some(ProposalType::PreSharedKey);
        let err = enforce_external_commit_structure(&summary)
            .expect_err("PSK proposals not allowed in external commits");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitUnsupportedProposalType(
                ProposalType::PreSharedKey
            )
        ));
    }

    // ── enforce_app_data_update_scope ────────────────────────────────

    #[xmtp_common::test(unwrap_try = true)]
    fn accepts_app_data_update_scoped_to_joiner() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let proposal = well_formed_membership_update_for(make_inbox_id(0x11));
        assert!(enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_for_other_inbox() {
        let joiner_hex = make_inbox_id_hex(0x11);
        // Insert someone else's entry — scope mismatch.
        let proposal = well_formed_membership_update_for(make_inbox_id(0x22));
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("delta keyed by a different inbox must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateOutOfScope
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_wrong_component() {
        let joiner_hex = make_inbox_id_hex(0x11);
        // Same delta payload but on COMPONENT_REGISTRY — wrong component.
        let entry: VLBytes = entry_bytes(ACTIVE_SLOT).into();
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(make_inbox_id(0x11), entry);
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::COMPONENT_REGISTRY.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("non-GROUP_MEMBERSHIP target must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateWrongComponent
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_remove_op() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let proposal = AppDataUpdateProposal::remove(XmtpComponentId::GROUP_MEMBERSHIP.as_u16());
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("Remove op on GROUP_MEMBERSHIP must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateOutOfScope
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_empty_delta() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new();
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("empty delta must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateOutOfScope
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_malformed_payload() {
        let joiner_hex = make_inbox_id_hex(0x11);
        // Two truncation bytes — not a valid TlsMapDelta.
        let proposal = AppDataUpdateProposal::update(
            XmtpComponentId::GROUP_MEMBERSHIP.as_u16(),
            vec![0xffu8, 0x00],
        );
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("malformed payload must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdatePayloadMalformed(_)
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_mixed_inbox_mutations() {
        let joiner_hex = make_inbox_id_hex(0x11);
        let entry: VLBytes = entry_bytes(ACTIVE_SLOT).into();
        // Mutation 1: joiner's own entry (legal in isolation).
        // Mutation 2: someone else's entry. Two mutations also trip the
        // exactly-one rule on their own.
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .insert(make_inbox_id(0x11), entry.clone())
            .insert(make_inbox_id(0x99), entry);
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("any non-joiner mutation must trip the scope check");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateOutOfScope
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_app_data_update_non_insert_mutation() {
        let joiner_hex = make_inbox_id_hex(0x11);
        // An Update mutation could never merge anyway (check 4
        // guarantees no existing entry, and TlsMap Update fails on a
        // missing key) — the validator rejects it structurally.
        let entry: VLBytes = entry_bytes(ACTIVE_SLOT).into();
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().update(make_inbox_id(0x11), entry);
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("non-Insert mutation must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAppDataUpdateOutOfScope
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_untagged_joiner_entry() {
        // Check 11: the tag is required on every external commit, even
        // when max_uses is 0 — an untagged entry would start a future
        // finite-cap policy from an undercount.
        let joiner_hex = make_inbox_id_hex(0x11);
        let entry: VLBytes = entry_bytes(b"").into();
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(make_inbox_id(0x11), entry);
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("entry without the admitted_via tag must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAdmittedViaTagMismatch
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_wrong_slot_tag() {
        // A tag naming a rotated / different slot doesn't satisfy
        // check 11 — only the active external_group_id does.
        let joiner_hex = make_inbox_id_hex(0x11);
        let entry: VLBytes = entry_bytes(b"slot-old").into();
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(make_inbox_id(0x11), entry);
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("entry tagged with a non-active slot must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::ExternalCommitAdmittedViaTagMismatch
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_unknown_version_joiner_entry() {
        // An empty value decodes as GroupMembershipEntry { version:
        // None } — an unverifiable entry must fail closed.
        let joiner_hex = make_inbox_id_hex(0x11);
        let entry: VLBytes = Vec::<u8>::new().into();
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().insert(make_inbox_id(0x11), entry);
        let bytes = encode_membership_delta(&delta);
        let proposal =
            AppDataUpdateProposal::update(XmtpComponentId::GROUP_MEMBERSHIP.as_u16(), bytes);
        let err = enforce_app_data_update_scope(&proposal, &joiner_hex, ACTIVE_SLOT)
            .expect_err("version-less membership entry must fail closed");
        assert!(matches!(
            err,
            CommitValidationError::GroupMembershipEntryMalformed(_)
        ));
    }

    // ── enforce_admitted_via_write_once ──────────────────────────────

    /// Serialize a full membership map to the stored-bytes shape
    /// `old_value` carries (TLS-serialized `TlsMap<InboxId, VLBytes>`).
    fn membership_map_bytes(entries: &[(u8, &[u8])]) -> Vec<u8> {
        use xmtp_mls_common::tls_map::TlsMap;
        let map: TlsMap<InboxId, VLBytes> = entries
            .iter()
            .map(|(seed, tag)| (make_inbox_id(*seed), VLBytes::new(entry_bytes(tag))))
            .collect();
        map.tls_serialize_detached().expect("map serialize")
    }

    fn member_update_op(mutations: TlsMapDelta<InboxId, VLBytes>) -> AppDataUpdateOperation {
        AppDataUpdateOperation::Update(encode_membership_delta(&mutations).into())
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_accepts_tag_carried_through() {
        // The canonical sequence-bump rewrite: same tag, new payload.
        let old = membership_map_bytes(&[(0x11, ACTIVE_SLOT)]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(make_inbox_id(0x11), entry_bytes(ACTIVE_SLOT).into());
        assert!(enforce_admitted_via_write_once(&member_update_op(delta), Some(&old)).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_accepts_untagged_rewrite() {
        let old = membership_map_bytes(&[(0x11, b"")]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(make_inbox_id(0x11), entry_bytes(b"").into());
        assert!(enforce_admitted_via_write_once(&member_update_op(delta), Some(&old)).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_rejects_tag_clear() {
        // The headline attack: an invited member untagging itself to
        // free a max_uses slot.
        let old = membership_map_bytes(&[(0x11, ACTIVE_SLOT)]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(make_inbox_id(0x11), entry_bytes(b"").into());
        let err = enforce_admitted_via_write_once(&member_update_op(delta), Some(&old))
            .expect_err("clearing the tag must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::AdmittedViaTagImmutable { .. }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_rejects_tag_set_by_member() {
        // Members may only ever add untagged entries — tagging is the
        // external commit's exclusive move.
        let old = membership_map_bytes(&[(0x11, b"")]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .insert(make_inbox_id(0x22), entry_bytes(ACTIVE_SLOT).into());
        let err = enforce_admitted_via_write_once(&member_update_op(delta), Some(&old))
            .expect_err("a member setting a tag on a fresh entry must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::AdmittedViaTagImmutable { .. }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_rejects_tag_alteration() {
        let old = membership_map_bytes(&[(0x11, ACTIVE_SLOT)]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(make_inbox_id(0x11), entry_bytes(b"slot-bbbb").into());
        let err = enforce_admitted_via_write_once(&member_update_op(delta), Some(&old))
            .expect_err("altering the tag must be rejected");
        assert!(matches!(
            err,
            CommitValidationError::AdmittedViaTagImmutable { .. }
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_allows_entry_delete() {
        // Member removal is the one legal way a tag disappears — it
        // frees the max_uses slot by design.
        let old = membership_map_bytes(&[(0x11, ACTIVE_SLOT)]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new().delete(make_inbox_id(0x11));
        assert!(enforce_admitted_via_write_once(&member_update_op(delta), Some(&old)).is_ok());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn write_once_rejects_unknown_version_rewrite() {
        // Replacing a tagged V1 entry with an opaque unknown-version
        // blob would hide the tag from every V1 reader — fail closed.
        let old = membership_map_bytes(&[(0x11, ACTIVE_SLOT)]);
        let delta = TlsMapDelta::<InboxId, VLBytes>::new()
            .update(make_inbox_id(0x11), Vec::<u8>::new().into());
        let err = enforce_admitted_via_write_once(&member_update_op(delta), Some(&old))
            .expect_err("version-less rewrite of a tagged entry must fail closed");
        assert!(matches!(
            err,
            CommitValidationError::GroupMembershipEntryMalformed(_)
        ));
    }
}

#[cfg(test)]
mod permission_on_receive_tests {
    //! Pins the receive-side permission check on `AppDataUpdate`
    //! proposals. Every proposal that reaches
    //! [`validate_one_app_data_update_with_old_value`] runs through
    //! `validate_component_write` (registry policy + hardcoded
    //! super-admin gating). Without this guarantee an attacker who
    //! patched out the send-side permission check could still poison
    //! the dictionary as long as their proposal landed in a commit.
    use super::*;
    use openmls::messages::proposals::AppDataUpdateOperation;
    use xmtp_mls_common::app_data::{
        component_id::ComponentId, component_registry::ComponentRegistry,
        validation::ActorAuthority,
    };

    fn non_admin_actor() -> ActorAuthority {
        ActorAuthority {
            is_admin: false,
            is_super_admin: false,
        }
    }

    fn admin_actor() -> ActorAuthority {
        ActorAuthority {
            is_admin: true,
            is_super_admin: false,
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn non_admin_writing_super_admin_only_component_is_rejected() {
        let operation = AppDataUpdateOperation::Update(vec![0u8; 16].into());
        let registry = ComponentRegistry::new();
        let err = validate_one_app_data_update_with_old_value(
            ComponentId::COMPONENT_REGISTRY,
            &operation,
            non_admin_actor(),
            "test-inbox",
            &registry,
            None,
        )
        .expect_err("non-admin write to super-admin-only component must be rejected");
        assert!(
            matches!(err, CommitValidationError::InsufficientPermissions),
            "expected InsufficientPermissions, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn plain_admin_writing_super_admin_only_component_is_rejected() {
        let operation = AppDataUpdateOperation::Update(vec![0u8; 16].into());
        let registry = ComponentRegistry::new();
        let err = validate_one_app_data_update_with_old_value(
            ComponentId::COMPONENT_REGISTRY,
            &operation,
            admin_actor(),
            "test-inbox",
            &registry,
            None,
        )
        .expect_err("plain-admin write to super-admin-only component must be rejected");
        assert!(
            matches!(err, CommitValidationError::InsufficientPermissions),
            "expected InsufficientPermissions, got {err:?}"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn non_admin_writing_component_with_no_registry_entry_is_rejected() {
        // Unknown component in well-known range with no registry
        // entry → deny by default at the registry-policy layer
        // (validate_component_write Layer 3), regardless of actor role.
        let unknown_id = ComponentId::new(0x80FF);
        let operation = AppDataUpdateOperation::Update(vec![0u8; 16].into());
        let registry = ComponentRegistry::new();
        let err = validate_one_app_data_update_with_old_value(
            unknown_id,
            &operation,
            non_admin_actor(),
            "test-inbox",
            &registry,
            None,
        )
        .expect_err("write to unregistered component must be rejected");
        assert!(
            matches!(err, CommitValidationError::InsufficientPermissions),
            "expected InsufficientPermissions, got {err:?}"
        );
    }
}

#[cfg(test)]
mod min_version_monotonicity_tests {
    use super::*;
    use openmls::messages::proposals::AppDataUpdateOperation;

    fn update_op(s: &str) -> AppDataUpdateOperation {
        AppDataUpdateOperation::Update(s.as_bytes().to_vec().into())
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn first_set_with_no_prior_floor_is_allowed() {
        enforce_min_version_monotonicity(&update_op("1.11.0-dev"), None)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn equal_version_is_allowed() {
        enforce_min_version_monotonicity(&update_op("1.11.0-dev"), Some(b"1.11.0-dev"))?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn higher_version_is_allowed() {
        enforce_min_version_monotonicity(&update_op("1.11.0"), Some(b"1.11.0-dev"))?;
        enforce_min_version_monotonicity(&update_op("1.12.0"), Some(b"1.11.0-dev"))?;
        enforce_min_version_monotonicity(&update_op("2.0.0"), Some(b"1.11.0-dev"))?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn lower_version_is_rejected() {
        let err = enforce_min_version_monotonicity(&update_op("1.10.0"), Some(b"1.11.0-dev"))
            .expect_err("downgrade must be rejected");
        assert!(
            matches!(
                err,
                CommitValidationError::MinVersionDowngrade { ref requested, ref current }
                if requested == "1.10.0" && current == "1.11.0-dev"
            ),
            "expected MinVersionDowngrade, got {err:?}",
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn remove_with_prior_floor_is_rejected() {
        let err =
            enforce_min_version_monotonicity(&AppDataUpdateOperation::Remove, Some(b"1.11.0-dev"))
                .expect_err("remove on a set floor must be rejected");
        assert!(
            matches!(
                err,
                CommitValidationError::MinVersionRemoveOnExistingFloor { ref current }
                if current == "1.11.0-dev"
            ),
            "expected MinVersionRemoveOnExistingFloor, got {err:?}",
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn remove_with_no_prior_floor_is_allowed() {
        enforce_min_version_monotonicity(&AppDataUpdateOperation::Remove, None)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_prior_skips_check() {
        // Lenient on unparseable prior bytes — refusing every future
        // update on a malformed floor would brick the group.
        enforce_min_version_monotonicity(&update_op("1.11.0-dev"), Some(b"not-a-version"))?;
        enforce_min_version_monotonicity(&update_op("1.11.0-dev"), Some(&[0xff, 0xfe, 0xfd]))?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn malformed_new_value_surfaces_parse_error() {
        let err =
            enforce_min_version_monotonicity(&update_op("not-a-version"), Some(b"1.11.0-dev"))
                .expect_err("malformed new value must error");
        assert!(
            matches!(err, CommitValidationError::InvalidVersionFormat(_)),
            "expected InvalidVersionFormat, got {err:?}",
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn prerelease_ordering_matches_semver() {
        // semver §11: pre-release sorts BEFORE the release. Bumping
        // from a pre-release to the corresponding release is allowed;
        // going the other way is a downgrade.
        enforce_min_version_monotonicity(&update_op("1.10.0"), Some(b"1.10.0-rc.1"))?;
        let err = enforce_min_version_monotonicity(&update_op("1.10.0-rc.1"), Some(b"1.10.0"))
            .expect_err("rc → release reverse must be rejected");
        assert!(
            matches!(err, CommitValidationError::MinVersionDowngrade { .. }),
            "expected MinVersionDowngrade, got {err:?}",
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn dev_prerelease_is_lower_than_release() {
        // The default `PROPOSALS_MIN_PROTOCOL_VERSION` is the
        // `-dev` pre-release of the workspace version, so by
        // semver §11 the release of the same x.y.z must sort
        // above it. Lock both directions:
        //   - LibXMTPVersion comparator agrees,
        //   - bumping the floor from `-dev` to the release is allowed,
        //   - the reverse is a downgrade.
        let dev = LibXMTPVersion::parse("1.11.0-dev")?;
        let release = LibXMTPVersion::parse("1.11.0")?;
        assert!(
            release > dev,
            "expected 1.11.0 > 1.11.0-dev per semver §11, got release={release:?} dev={dev:?}",
        );
        assert!(dev < release, "expected 1.11.0-dev < 1.11.0 per semver §11");

        enforce_min_version_monotonicity(&update_op("1.11.0"), Some(b"1.11.0-dev"))?;

        let err = enforce_min_version_monotonicity(&update_op("1.11.0-dev"), Some(b"1.11.0"))
            .expect_err("release → -dev reverse must be rejected");
        assert!(
            matches!(
                err,
                CommitValidationError::MinVersionDowngrade { ref requested, ref current }
                if requested == "1.11.0-dev" && current == "1.11.0"
            ),
            "expected MinVersionDowngrade, got {err:?}",
        );
    }
}
