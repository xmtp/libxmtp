//! Pure commit validation rules.
//!
//! These functions read an OpenMLS group, a staged commit, and the group's
//! AppData dictionary. They do not read the client database or fetch
//! identities. `xmtp_mls` runs them inside `ValidatedCommit` and adds the
//! identity and installation checks that need stored state.

use std::collections::{HashMap, HashSet};

use openmls::{
    credentials::{BasicCredential, Credential as OpenMlsCredential, errors::BasicCredentialError},
    extensions::{Extension, Extensions},
    group::{GroupContext, MlsGroup as OpenMlsGroup, StagedCommit},
    messages::proposals::{Proposal, ProposalType},
    prelude::{LeafNodeIndex, Sender},
    treesync::LeafNode,
};
use prost::Message;
use serde::Serialize;
use thiserror::Error;
use xmtp_common::RetryableError;
use xmtp_mls_common::{
    app_data::component_source::ComponentSourceError,
    group_metadata::{DmMembers, GroupMetadata, GroupMetadataError},
    group_mutable_metadata::{GroupMutableMetadata, GroupMutableMetadataError},
    libxmtp_version::{InvalidVersionFormat, LibXMTPVersion},
};
use xmtp_proto::xmtp::{
    identity::MlsCredential,
    mls::message_contents::group_updated::{
        Inbox as InboxProto, MetadataFieldChange as MetadataFieldChangeProto,
    },
};

use crate::{
    group_membership::{GroupMembership, MembershipDiff},
    group_permissions::GroupMutablePermissionsError,
};

/// A commit breaks a validation rule, or its group state cannot be read.
#[derive(Debug, Error)]
pub enum CommitRuleError {
    /// Identity updates must precede the group envelope. Not retryable.
    #[error(
        "Identity sequence {identity_sequence} does not precede group sequence {envelope_sequence}"
    )]
    IdentitySequenceNotBeforeEnvelope {
        /// Identity update sequence `N` named by the proposed membership.
        identity_sequence: u64,
        /// Authenticated group envelope sequence `S`; valid references have `N < S`.
        envelope_sequence: u64,
    },
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
    GroupMutablePermissions(#[from] GroupMutablePermissionsError),
    #[error("PSKs are not supported")]
    NoPSKSupport,
    #[error("Unsupported proposal type: {0:?}")]
    UnsupportedProposalType(ProposalType),
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
    ComponentSource(#[from] ComponentSourceError),
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
}

impl RetryableError for CommitRuleError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl CommitRuleError {
    /// Only authenticated, deterministic invalid input can advance the prefix.
    /// Unsupported versions and proposal types leave the head pending.
    pub fn is_safe_rejection(&self) -> bool {
        match self {
            Self::IdentitySequenceNotBeforeEnvelope { .. }
            | Self::ActorCouldNotBeFound
            | Self::InboxValidationFailed(_)
            | Self::InsufficientPermissions
            | Self::InvalidVersionFormat(_)
            | Self::ActorNotMember
            | Self::SubjectDoesNotExist
            | Self::MultipleActors
            | Self::UnexpectedInstallationAdded(_)
            | Self::SequenceIdDecreased
            | Self::UnexpectedInstallationsRemoved(_)
            | Self::MlsCredential(_)
            | Self::ProtoDecode(_)
            | Self::NoPSKSupport
            | Self::TooManyCharacters { .. }
            | Self::ProposerNotFound
            | Self::ProposalsNotEnabled
            | Self::MinVersionDowngrade { .. }
            | Self::MinVersionRemoveOnExistingFloor { .. }
            | Self::MissingGroupMembership
            | Self::MissingMutableMetadata
            | Self::GroupMetadata(_)
            | Self::GroupMutableMetadata(_)
            | Self::GroupMutablePermissions(_)
            | Self::ComponentSource(_)
            | Self::Conversion(_) => true,
            Self::ProtocolVersionTooLow(_) | Self::UnsupportedProposalType(_) => false,
        }
    }
}

impl From<InvalidVersionFormat> for CommitRuleError {
    fn from(error: InvalidVersionFormat) -> Self {
        Self::InvalidVersionFormat(error.0)
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
    ) -> Result<Self, CommitRuleError> {
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

/// Membership authorization has no metadata or permission diff fields.
/// Each component update is checked against its authenticated proposer.
pub struct MembershipValidationInfo<'a> {
    pub actor: &'a CommitParticipant,
    pub added_inboxes: &'a [Inbox],
    pub removed_inboxes: &'a [Inbox],
    pub dm_members: Option<&'a DmMembers<String>>,
}

/// Actual metadata changes, used only for transcripts and status updates.
/// Component policies authorize these changes before this summary is built.
#[derive(Debug, Clone, Serialize)]
pub struct MetadataChanges {
    pub metadata_field_changes: Vec<MetadataFieldChange>,
    pub admins_added: Vec<Inbox>,
    pub admins_removed: Vec<Inbox>,
    pub super_admins_added: Vec<Inbox>,
    pub super_admins_removed: Vec<Inbox>,
}

impl MetadataChanges {
    pub fn is_empty(&self) -> bool {
        self.metadata_field_changes.is_empty()
            && self.admins_added.is_empty()
            && self.admins_removed.is_empty()
            && self.super_admins_added.is_empty()
            && self.super_admins_removed.is_empty()
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

/// Reject any commit that carries a `PreSharedKey` proposal.
///
pub fn reject_psk_proposals(staged_commit: &StagedCommit) -> Result<(), CommitRuleError> {
    if staged_commit.psk_proposals().any(|_| true) {
        return Err(CommitRuleError::NoPSKSupport);
    }
    Ok(())
}

pub struct ProposalChanges {
    pub added_installations: HashSet<Vec<u8>>,
    pub removed_installations: HashSet<Vec<u8>>,
    pub credentials_to_verify: Vec<CommitParticipant>,
    /// Maps inbox_id to the proposer who proposed adding it
    pub added_inbox_proposers: HashMap<String, CommitParticipant>,
    /// Maps inbox_id to the proposer who proposed removing it
    pub removed_inbox_proposers: HashMap<String, CommitParticipant>,
}

/**
 * Extracts the installations added and removed via proposals in the commit.
 * Also returns a list of credentials from existing members that need verification (caused by update proposals)
 * Tracks which proposer created each proposal for permission validation.
 */
pub fn get_proposal_changes(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<ProposalChanges, CommitRuleError> {
    // The actual installations added and removed via proposals in the commit
    let mut added_installations: HashSet<Vec<u8>> = HashSet::new();
    let mut removed_installations: HashSet<Vec<u8>> = HashSet::new();
    let mut credentials_to_verify: Vec<CommitParticipant> = vec![];
    let mut added_inbox_proposers: HashMap<String, CommitParticipant> = HashMap::new();
    let mut removed_inbox_proposers: HashMap<String, CommitParticipant> = HashMap::new();

    for proposal in staged_commit.queued_proposals() {
        // Extract the proposer for this proposal
        let proposer = match proposal.sender() {
            Sender::Member(leaf_index) => extract_commit_participant(
                leaf_index,
                openmls_group,
                immutable_metadata,
                mutable_metadata,
            )?,
            _ => return Err(CommitRuleError::ActorNotMember),
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
                    .ok_or(CommitRuleError::SubjectDoesNotExist)?;
                let installation_id = leaf_node.signature_key.to_vec();
                let inbox_id = inbox_id_from_credential(&leaf_node.credential)?;
                removed_installations.insert(installation_id);
                removed_inbox_proposers.insert(inbox_id, proposer);
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
    })
}

/**
 * Extracts the latest `GroupMembership` from the staged commit.
 *
 * Returns an error if the extension is not found.
 */
pub fn get_latest_group_membership(
    staged_commit: &StagedCommit,
) -> Result<GroupMembership, CommitRuleError> {
    extract_group_membership(staged_commit.group_context().extensions())
}

/// Read committed metadata once through the active extension representation.
/// Decode the post-commit metadata for messages, callbacks, and admin side effects.
pub fn read_post_commit_mutable_metadata(
    group: &OpenMlsGroup,
    staged_commit: &StagedCommit,
    registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
) -> Result<GroupMutableMetadata, CommitRuleError> {
    use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension};
    use xmtp_mls_common::{
        app_data::component_id::ComponentId,
        group_mutable_metadata::{METADATA_FIELD_COMPONENT_MAP, merge_dict_into_mutable_metadata},
    };
    let mut dictionary = AppDataDictionary::new();
    for id in METADATA_FIELD_COMPONENT_MAP
        .iter()
        .map(|(_, id)| *id)
        .chain([ComponentId::ADMIN_LIST, ComponentId::SUPER_ADMIN_LIST])
    {
        if let Some(bytes) =
            xmtp_mls_common::app_data::component_source::read_post_commit_component_bytes(
                id,
                group,
                staged_commit,
                registry,
            )?
        {
            dictionary.insert(id.as_u16(), bytes);
        }
    }
    let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(dictionary),
    )])
    .expect("one dictionary extension has no duplicate types");
    let mut metadata = GroupMutableMetadata::new(HashMap::new(), Vec::new(), Vec::new());
    merge_dict_into_mutable_metadata(&mut metadata, &extensions)?;
    Ok(metadata)
}

pub fn read_committed_metadata(
    group: &OpenMlsGroup,
) -> Result<(GroupMetadata, GroupMutableMetadata), CommitRuleError> {
    let seed = xmtp_mls_common::app_data::component_source::read_group_metadata_from_dict(group)?
        .ok_or(GroupMetadataError::MissingExtension)?;
    let immutable =
        GroupMetadata::try_from(xmtp_proto::xmtp::mls::message_contents::GroupMetadataV1 {
            conversation_type: seed.conversation_type,
            creator_inbox_id: seed.creator_inbox_id,
            creator_account_address: String::new(),
            dm_members: seed.dm_members,
            oneshot_message: seed.oneshot,
        })?;
    let mut mutable = GroupMutableMetadata::new(HashMap::new(), Vec::new(), Vec::new());
    xmtp_mls_common::app_data::component_source::merge_app_data_into_mutable_metadata(
        &mut mutable,
        group,
    )?;
    Ok((immutable, mutable))
}

/// Require each identity sequence `N` to precede group envelope sequence `S`.
/// `N >= S` is invalid on every receiver, independent of cache or replica state.
// implements: GMOD-007
pub fn validate_identity_sequence_order(
    membership: &GroupMembership,
    envelope_sequence: u64,
) -> Result<(), CommitRuleError> {
    for identity_sequence in membership.members.values() {
        if *identity_sequence >= envelope_sequence {
            return Err(CommitRuleError::IdentitySequenceNotBeforeEnvelope {
                identity_sequence: *identity_sequence,
                envelope_sequence,
            });
        }
    }
    Ok(())
}

/// Superadmins are permitted to readd installations, e.g. for fork recovery
/// We can take these readded installations out of the list of installations to validate
// implements: GMOD-015
pub fn extract_readded_installations(
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

pub fn get_current_group_members(openmls_group: &OpenMlsGroup) -> HashSet<Vec<u8>> {
    openmls_group
        .members()
        .map(|member| member.signature_key)
        .collect()
}

/// Validate that the new group membership is a valid state transition from the old group membership.
/// Enforces Rule 1 from above
// implements: GMOD-008
pub fn validate_membership_diff(
    old_membership: &GroupMembership,
    new_membership: &GroupMembership,
    diff: &MembershipDiff<'_>,
) -> Result<(), CommitRuleError> {
    for inbox_id in diff.updated_inboxes.iter() {
        let old_sequence_id = old_membership
            .get(inbox_id)
            .ok_or(CommitRuleError::SubjectDoesNotExist)?;
        let new_sequence_id = new_membership
            .get(inbox_id)
            .ok_or(CommitRuleError::SubjectDoesNotExist)?;

        if new_sequence_id.lt(old_sequence_id) {
            return Err(CommitRuleError::SequenceIdDecreased);
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
pub fn validate_one_app_data_update(
    component_id: xmtp_mls_common::app_data::component_id::ComponentId,
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    actor: xmtp_mls_common::app_data::validation::ActorAuthority,
    proposer_inbox_id: &str,
    registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
    openmls_group: &OpenMlsGroup,
    dm_members: Option<&DmMembers<String>>,
) -> Result<(), CommitRuleError> {
    use xmtp_mls_common::app_data::component_source::read_from_app_data_dict;

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
        dm_members,
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
// implements: GMOD-026
fn enforce_min_version_monotonicity(
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    old_value: Option<&[u8]>,
) -> Result<(), CommitRuleError> {
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
            let new_str = std::str::from_utf8(new_bytes)
                .map_err(|_| CommitRuleError::InvalidVersionFormat(format!("{:?}", new_bytes)))?;
            let new_v = LibXMTPVersion::parse(new_str)?;
            if new_v < old_v {
                return Err(CommitRuleError::MinVersionDowngrade {
                    requested: new_str.to_string(),
                    current: old_str.to_string(),
                });
            }
            Ok(())
        }
        AppDataUpdateOperation::Remove => Err(CommitRuleError::MinVersionRemoveOnExistingFloor {
            current: old_str.to_string(),
        }),
    }
}

/// Permit exactly one insertion for the other authenticated DM participant.
fn permits_dm_participant_insert(
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    proposer_inbox_id: &str,
    dm_members: Option<&DmMembers<String>>,
) -> bool {
    use openmls::messages::proposals::AppDataUpdateOperation;
    use tls_codec::{Deserialize as _, VLBytes};
    use xmtp_mls_common::{
        inbox_id::InboxId,
        tls_map::{TlsMapDelta, TlsMapMutation},
    };

    let Some(dm) = dm_members else { return false };
    let AppDataUpdateOperation::Update(payload) = operation else {
        return false;
    };
    let Ok(delta) = TlsMapDelta::<InboxId, VLBytes>::tls_deserialize_exact(payload.as_slice())
    else {
        return false;
    };
    let mut inserted = delta
        .mutations
        .iter()
        .filter_map(|mutation| match mutation {
            TlsMapMutation::Insert { key, .. } => Some(key.to_hex()),
            _ => None,
        });
    let Some(inbox_id) = inserted.next() else {
        return false;
    };
    inserted.next().is_none()
        && inbox_id != proposer_inbox_id
        && (inbox_id == dm.member_one_inbox_id.as_ref()
            || inbox_id == dm.member_two_inbox_id.as_ref())
}

/// Pure core of [`validate_one_app_data_update`] with `old_value`
/// passed explicitly so unit tests can exercise the
/// expand → per-change policy loop without a real MLS group.
// implements: PERM-011, PERM-014
pub fn validate_one_app_data_update_with_old_value(
    component_id: xmtp_mls_common::app_data::component_id::ComponentId,
    operation: &openmls::messages::proposals::AppDataUpdateOperation,
    actor: xmtp_mls_common::app_data::validation::ActorAuthority,
    proposer_inbox_id: &str,
    registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
    old_value: Option<&[u8]>,
    dm_members: Option<&DmMembers<String>>,
) -> Result<(), CommitRuleError> {
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
                let wrapped =
                    xmtp_mls_common::app_data::component_source::ComponentSourceError::from(e);
                tracing::warn!(
                    proposer_inbox_id,
                    component_id = %component_id,
                    error = %wrapped,
                    "AppDataUpdate proposal rejected: failed to expand payload"
                );
                CommitRuleError::InsufficientPermissions
            })?
    } else {
        match xmtp_mls_common::app_data::component_source::expand_app_data_update_to_changes(
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
                return Err(CommitRuleError::InsufficientPermissions);
            }
        }
    };

    // A DM may add its other participant even though its add-member policy
    // denies general additions. Use the same exception as MLS Add validation.
    let dm_participant_insert = component_id
        == xmtp_mls_common::app_data::component_id::ComponentId::GROUP_MEMBERSHIP
        && permits_dm_participant_insert(operation, proposer_inbox_id, dm_members);
    for change in &changes {
        if dm_participant_insert
            && change.op == xmtp_mls_common::app_data::component_registry::ComponentOp::Insert
        {
            continue;
        }
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
            return Err(CommitRuleError::InsufficientPermissions);
        }
    }

    // Layer 2: component-local invariants. Run once per proposal with
    // the complete pre- and post-operation values. Collection policies
    // above still run per mutation, but an invariant such as “a non-empty
    // super-admin list cannot become empty” is a transition property and
    // cannot be determined from an individual delta mutation.
    //
    // Only known components have this hook. Unknown ids use the
    // type-aware compatibility path and therefore cannot add invariants
    // beyond their registry policy.
    if let Some(component) = component {
        let post_value = match operation {
            openmls::messages::proposals::AppDataUpdateOperation::Update(payload) => component
                .apply_update_payload(payload.as_slice(), old_value)
                .map_err(|e| {
                    tracing::warn!(
                        proposer_inbox_id,
                        component_id = %component_id,
                        error = %e,
                        "AppDataUpdate proposal rejected: failed to compute post-state"
                    );
                    CommitRuleError::InsufficientPermissions
                })?,
            openmls::messages::proposals::AppDataUpdateOperation::Remove => Vec::new(),
        };
        let invariant_change = ComponentChange::builder()
            .component_id(component_id)
            .op(match operation {
                openmls::messages::proposals::AppDataUpdateOperation::Update(_) => {
                    xmtp_mls_common::app_data::component_registry::ComponentOp::Update
                }
                openmls::messages::proposals::AppDataUpdateOperation::Remove => {
                    xmtp_mls_common::app_data::component_registry::ComponentOp::Delete
                }
            })
            .actor(actor)
            .maybe_old_value(old_value)
            .maybe_new_value(match operation {
                openmls::messages::proposals::AppDataUpdateOperation::Update(_) => {
                    Some(post_value.as_slice())
                }
                openmls::messages::proposals::AppDataUpdateOperation::Remove => None,
            })
            .build();
        if let Err(e) = component.validate_invariant(&invariant_change, registry) {
            tracing::warn!(
                proposer_inbox_id,
                component_id = %component_id,
                error = %e,
                "AppDataUpdate proposal rejected: component invariant violated"
            );
            return Err(CommitRuleError::InsufficientPermissions);
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
pub fn app_data_update_proposer_leaf(sender: &Sender) -> Result<&LeafNodeIndex, CommitRuleError> {
    match sender {
        Sender::Member(leaf_index) => Ok(leaf_index),
        Sender::External(_) | Sender::NewMemberCommit | Sender::NewMemberProposal => {
            Err(CommitRuleError::ActorNotMember)
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
/// `registry` is the committed registry shared with the other component checks.
/// Registry changes and writes that depend on them must use separate commits.
pub fn validate_app_data_update_proposals_in_commit(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
    registry: &xmtp_mls_common::app_data::component_registry::ComponentRegistry,
) -> Result<(), CommitRuleError> {
    use std::collections::HashMap;
    use xmtp_mls_common::app_data::component_source::read_from_app_data_dict;
    use xmtp_mls_common::app_data::{
        component_id::ComponentId, registry_table::lookup_component, validation::ActorAuthority,
    };

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

    // A single commit's bootstrap can carry multiple AppDataUpdate proposals
    // from the same leaf; cache extracted `CommitParticipant`s so we don't
    // re-walk the admin lists and re-parse the credential for every one.
    let mut participants: HashMap<LeafNodeIndex, CommitParticipant> = HashMap::new();
    // Keep post-operation snapshots for known components as proposals are
    // processed. A commit can contain more than one delta for a collection;
    // each later invariant must observe the preceding proposal's result.
    let mut component_post_states: HashMap<ComponentId, Option<Vec<u8>>> = HashMap::new();

    for queued in proposals {
        let app_data = queued.app_data_update_proposal();
        let component_id = ComponentId::from(app_data.component_id());
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

        let old_value = component_post_states
            .entry(component_id)
            .or_insert_with(|| read_from_app_data_dict(component_id, openmls_group))
            .clone();
        validate_one_app_data_update_with_old_value(
            component_id,
            app_data.operation(),
            ActorAuthority::from(proposer),
            &proposer.inbox_id,
            registry,
            old_value.as_deref(),
            immutable_metadata.dm_members.as_ref(),
        )?;

        // Unknown components do not have a per-id invariant. Known component
        // codecs compute the exact post-state for the next proposal in this
        // commit, so transition invariants cannot be bypassed by splitting a
        // destructive delta into sequential proposals.
        if let Some(component) = lookup_component(component_id) {
            let post_value = match app_data.operation() {
                openmls::messages::proposals::AppDataUpdateOperation::Update(payload) => Some(
                    component
                        .apply_update_payload(payload.as_slice(), old_value.as_deref())
                        .map_err(|_| CommitRuleError::InsufficientPermissions)?,
                ),
                openmls::messages::proposals::AppDataUpdateOperation::Remove => None,
            };
            component_post_states.insert(component_id, post_value);
        }
    }

    // Validate the final registry after all deltas. Required action entries
    // must remain present, and every child in each action tree must be valid.
    if let Some(post_registry) = component_post_states.get(&ComponentId::COMPONENT_REGISTRY) {
        let bytes = post_registry
            .as_deref()
            .ok_or(CommitRuleError::InsufficientPermissions)?;
        let registry =
            xmtp_mls_common::app_data::component_registry::ComponentRegistry::from_bytes(bytes)
                .map_err(|_| CommitRuleError::InsufficientPermissions)?;
        xmtp_mls_common::app_data::policy_set::validate_registry_action_policies(&registry)
            .map_err(|error| {
                tracing::warn!(%error, "invalid registry action policy in commit");
                CommitRuleError::InsufficientPermissions
            })?;
    }

    Ok(())
}

/// Extracts the [`CommitParticipant`] from the [`LeafNodeIndex`]
pub fn extract_commit_participant(
    leaf_index: &LeafNodeIndex,
    group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<CommitParticipant, CommitRuleError> {
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
        Err(CommitRuleError::ActorNotMember)
    }
}

/// Get [`GroupMembership`] from the AppData dictionary.
#[tracing::instrument(level = "trace", skip_all)]
pub fn extract_group_membership(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMembership, CommitRuleError> {
    let proto =
        xmtp_mls_common::app_data::component_source::read_group_membership_from_dict(extensions)
            .map_err(|e| {
                CommitRuleError::GroupMutableMetadata(
                    xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::from(e),
                )
            })?
            .ok_or(CommitRuleError::MissingGroupMembership)?;
    Ok(GroupMembership {
        members: proto.members,
        failed_installations: proto.failed_installations,
    })
}

pub fn metadata_changes_between(
    immutable_metadata: &GroupMetadata,
    old_mutable_metadata: &GroupMutableMetadata,
    new_mutable_metadata: &GroupMutableMetadata,
) -> MetadataChanges {
    let metadata_field_changes =
        mutable_metadata_field_changes(old_mutable_metadata, new_mutable_metadata);

    MetadataChanges {
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
    }
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

pub fn build_inbox_with_proposer(
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
pub fn inbox_id_from_credential(credential: &OpenMlsCredential) -> Result<String, CommitRuleError> {
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
pub fn extract_committer_and_proposers(
    staged_commit: &StagedCommit,
    committer_leaf_index: LeafNodeIndex,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<(CommitParticipant, Vec<CommitParticipant>), CommitRuleError> {
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
            _ => return Err(CommitRuleError::ActorNotMember),
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

#[cfg(test)]
mod app_data_update_tests;
#[cfg(test)]
mod readded_installations_tests;

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

    #[xmtp_common::test(unwrap_try = true)]
    fn dm_membership_insert_exception_rejects_other_inboxes() {
        use tls_codec::{Serialize as _, VLBytes};
        use xmtp_mls_common::{inbox_id::InboxId, tls_map::TlsMapDelta};
        let creator = InboxId::from_bytes([1; 32]);
        let peer = InboxId::from_bytes([2; 32]);
        let outsider = InboxId::from_bytes([3; 32]);
        let dm = DmMembers {
            member_one_inbox_id: creator.to_hex(),
            member_two_inbox_id: peer.to_hex(),
        };
        for (keys, allowed) in [
            (vec![peer], true),
            (vec![creator], false),
            (vec![outsider], false),
            (vec![peer, outsider], false),
            (vec![peer, peer], false),
        ] {
            let mut delta = TlsMapDelta::<InboxId, VLBytes>::new();
            for key in keys {
                delta = delta.insert(key, vec![].into());
            }
            let operation = AppDataUpdateOperation::Update(delta.tls_serialize_detached()?.into());
            assert_eq!(
                permits_dm_participant_insert(&operation, &creator.to_hex(), Some(&dm)),
                allowed
            );
            assert!(!permits_dm_participant_insert(
                &operation,
                &creator.to_hex(),
                None
            ));
        }
    }

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
            None,
        )
        .expect_err("non-admin write to super-admin-only component must be rejected");
        assert!(
            matches!(err, CommitRuleError::InsufficientPermissions),
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
            None,
        )
        .expect_err("plain-admin write to super-admin-only component must be rejected");
        assert!(
            matches!(err, CommitRuleError::InsufficientPermissions),
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
            None,
        )
        .expect_err("write to unregistered component must be rejected");
        assert!(
            matches!(err, CommitRuleError::InsufficientPermissions),
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

    // verifies: GMOD-026
    #[xmtp_common::test(unwrap_try = true)]
    fn lower_version_is_rejected() {
        let err = enforce_min_version_monotonicity(&update_op("1.10.0"), Some(b"1.11.0-dev"))
            .expect_err("downgrade must be rejected");
        assert!(
            matches!(
                err,
                CommitRuleError::MinVersionDowngrade { ref requested, ref current }
                if requested == "1.10.0" && current == "1.11.0-dev"
            ),
            "expected MinVersionDowngrade, got {err:?}",
        );
    }

    // verifies: GMOD-026
    #[xmtp_common::test(unwrap_try = true)]
    fn remove_with_prior_floor_is_rejected() {
        let err =
            enforce_min_version_monotonicity(&AppDataUpdateOperation::Remove, Some(b"1.11.0-dev"))
                .expect_err("remove on a set floor must be rejected");
        assert!(
            matches!(
                err,
                CommitRuleError::MinVersionRemoveOnExistingFloor { ref current }
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
            matches!(err, CommitRuleError::InvalidVersionFormat(_)),
            "expected InvalidVersionFormat, got {err:?}",
        );
    }

    // verifies: GMOD-027
    #[xmtp_common::test(unwrap_try = true)]
    fn prerelease_ordering_matches_semver() {
        // semver §11: pre-release sorts BEFORE the release. Bumping
        // from a pre-release to the corresponding release is allowed;
        // going the other way is a downgrade.
        enforce_min_version_monotonicity(&update_op("1.10.0"), Some(b"1.10.0-rc.1"))?;
        let err = enforce_min_version_monotonicity(&update_op("1.10.0-rc.1"), Some(b"1.10.0"))
            .expect_err("rc → release reverse must be rejected");
        assert!(
            matches!(err, CommitRuleError::MinVersionDowngrade { .. }),
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
                CommitRuleError::MinVersionDowngrade { ref requested, ref current }
                if requested == "1.11.0-dev" && current == "1.11.0"
            ),
            "expected MinVersionDowngrade, got {err:?}",
        );
    }
}
