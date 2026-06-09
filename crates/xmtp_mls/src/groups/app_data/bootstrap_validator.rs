//! Receiver-side bootstrap-commit validator.
//!
//! The one-time AppData-migration bootstrap carries a deterministic,
//! sender-authoritative payload: every honest receiver synthesizes the
//! same canonical subset from the pre-flip group state and byte-
//! compares it against the commit's `AppDataUpdate` proposals.
//! Divergence is rejected.
//!
//! Entry points:
//! - [`is_bootstrap_commit`] — shape-check a staged commit (GCE drops
//!   the four legacy extensions and requires `AppDataDictionary` in
//!   `RequiredCapabilities`, plus a `COMPONENT_REGISTRY` write).
//!   Routes commits into this validator.
//! - [`validate_bootstrap_commit`] — full validation against pre-flip
//!   state. Pure-logic core lives in [`validate_against_canonical_subset`]
//!   so it's unit-testable without a real `StagedCommit`.

use std::collections::BTreeMap;

use openmls::{
    extensions::{Extension, ExtensionType, Extensions},
    framing::Sender,
    group::{GroupContext, MlsGroup as OpenMlsGroup, StagedCommit},
    messages::proposals::{AppDataUpdateOperationType, Proposal},
};
use prost::Message as _;
use tls_codec::Deserialize;
use xmtp_configuration::{
    GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID, MUTABLE_METADATA_EXTENSION_ID,
};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::ComponentRegistry,
        migration::{CanonicalBootstrapExpectation, synthesize_canonical_subset_for_validation},
    },
    group_metadata::GroupMetadata,
    group_mutable_metadata::GroupMutableMetadata,
    inbox_id::InboxId,
    tls_map::{TlsMapDelta, TlsMapMutation},
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentMetadata, GroupMembershipEntry,
    group_membership_entry::Version as GroupMembershipEntryVersion,
};

use crate::groups::validated_commit::{
    CommitParticipant, CommitValidationError, extract_commit_participant,
};

/// Bootstrap-commit-specific validation failures.
///
/// Wrapped behind [`CommitValidationError::Bootstrap`] in the steady-state
/// validator surface so the AppData-migration variants don't drown the
/// rest of the enum. Construct these locally; callers higher up convert
/// via the `#[from]` on `CommitValidationError::Bootstrap`.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapValidationError {
    /// The canonical subset says this `ComponentId` should be present
    /// among the bootstrap commit's `AppDataUpdate` proposals, but the
    /// commit didn't include one for it.
    #[error("bootstrap commit is missing seed for component {0}")]
    MissingSeed(ComponentId),
    /// The commit carries an `AppDataUpdate` proposal for this
    /// component but its bytes (or operation type) don't match what
    /// the pre-flip state produces. Signals a malicious or buggy
    /// sender.
    #[error("bootstrap commit payload mismatch for component {0}")]
    Mismatch(ComponentId),
    /// The commit carries an `AppDataUpdate` proposal for a component
    /// outside the canonical subset (and not `GROUP_MEMBERSHIP`).
    /// Bootstrap is not a sender-discretion op — there are no extras.
    #[error("bootstrap commit carries unexpected proposal for component {0}")]
    UnexpectedProposal(ComponentId),
    /// The commit carries two or more `AppDataUpdate` proposals for the
    /// same component. Bootstrap is sender-authoritative byte-compare —
    /// duplicates would let the sender's queue order diverge from
    /// OpenMLS' apply order (proposals are sorted by `(component_id,
    /// op_type)` at apply time but iterated in queue order during
    /// validation), so the validator could green-light bytes that
    /// aren't what gets merged into the dictionary.
    #[error("bootstrap commit carries duplicate proposal for component {0}")]
    DuplicateProposal(ComponentId),
    /// The commit's `GROUP_MEMBERSHIP` payload carries an inbox id
    /// that isn't in the pre-flip membership map.
    #[error("bootstrap commit introduces unknown member inbox {}", hex::encode(.0))]
    ExtraMember(Vec<u8>),
    /// The commit's `GROUP_MEMBERSHIP` payload is missing an inbox
    /// that IS in the pre-flip membership map.
    #[error("bootstrap commit is missing member inbox {}", hex::encode(.0))]
    MissingMember(Vec<u8>),
    /// A per-inbox `sequence_id` in the commit's `GROUP_MEMBERSHIP`
    /// payload doesn't match the pre-flip state. Security-sensitive
    /// (a smuggled-in higher sequence_id would cause honest receivers
    /// to skip legitimate identity updates).
    #[error(
        "bootstrap commit sequence-id mismatch for inbox {}: expected {expected}, got {actual}",
        hex::encode(inbox_id)
    )]
    SequenceIdMismatch {
        inbox_id: Vec<u8>,
        expected: u64,
        actual: u64,
    },
    /// The commit's GCE proposal doesn't have the shape bootstrap
    /// requires: remove exactly the four legacy XMTP extensions and
    /// list `AppDataDictionary` in `RequiredCapabilities`.
    #[error("bootstrap GCE-proposal shape mismatch: {0}")]
    GceMismatch(String),
    /// Failed to decode the on-wire bytes of a bootstrap `AppDataUpdate`
    /// payload under the expected encoding (e.g. a `GROUP_MEMBERSHIP`
    /// payload that isn't a valid `TlsMap<[u8;32], VLBytes>`).
    #[error("bootstrap commit payload decode failure for component {component_id}: {reason}")]
    PayloadDecode {
        component_id: ComponentId,
        reason: String,
    },
    /// The bootstrap commit's proposer is not a super-admin of the
    /// pre-flip group.
    #[error("bootstrap commit proposer is not a super-admin")]
    ProposerNotSuperAdmin,
    /// A `COMPONENT_REGISTRY` entry in the bootstrap commit decoded
    /// to a `ComponentMetadata` that doesn't match the receiver's
    /// expected metadata (different permissions, type, or default
    /// value) for that component id.
    #[error("bootstrap commit registry entry for component {0} doesn't match expected metadata")]
    RegistryEntryMismatch(ComponentId),
    /// A per-inbox `failed_installations` entry references an
    /// installation id that wasn't in the pre-flip legacy
    /// `failed_installations` list. The legacy state bounds the universe
    /// of installations the migrator may legally mark failed.
    #[error(
        "bootstrap commit references unauthorized failed installation: {}",
        hex::encode(.0)
    )]
    UnauthorizedFailedInstallation(Vec<u8>),
    /// The canonical-subset synthesis failed. Almost always means the
    /// pre-flip group state is corrupted (missing a legacy extension
    /// this module's synthesis depends on).
    #[error("bootstrap canonical-subset synthesis failed: {0}")]
    Synthesis(#[from] xmtp_mls_common::app_data::migration::MigrationError),
    /// The bootstrap commit carries a proposal whose type isn't
    /// `GroupContextExtensions` or `AppDataUpdate`. Defense-in-depth:
    /// `is_bootstrap_commit` only checks for the GCE shape plus a
    /// `COMPONENT_REGISTRY` write, so a malicious sender could in
    /// principle bundle in an `Add` / `Remove` / `Update` / `SelfRemove`
    /// / `ReInit` / `ExternalInit` / `AppEphemeral` / `Custom` proposal
    /// and still satisfy the routing predicate. Bootstrap is
    /// membership-neutral and AppData-only — anything outside that pair
    /// is rejected here so OpenMLS never gets a chance to apply the
    /// smuggled proposal at merge time.
    #[error("bootstrap commit carries disallowed proposal type {0}")]
    DisallowedProposalType(&'static str),
}

/// Shape-check a staged commit against the bootstrap signature.
///
/// A bootstrap commit has, in a single MLS commit:
/// 1. A `GroupContextExtensions` proposal that drops
///    `MUTABLE_METADATA_EXTENSION_ID`, `GROUP_PERMISSIONS_EXTENSION_ID`,
///    `GROUP_MEMBERSHIP_EXTENSION_ID`, and the OpenMLS-built-in
///    `ImmutableMetadata` extension.
/// 2. The same GCE proposal's `RequiredCapabilities` lists
///    `ExtensionType::AppDataDictionary` (the standard MLS extension
///    that carries the dict). After commit application this is the
///    invariant pinning the group into migrated state.
/// 3. At least one `AppDataUpdate` proposal writing
///    `COMPONENT_REGISTRY`. Its application populates the
///    `AppDataDictionary` GCE that `RequiredCapabilities` now
///    requires.
///
/// The three conditions together differentiate a bootstrap from any
/// other combinatorial GCE flow.
pub(crate) fn is_bootstrap_commit(
    staged_commit: &StagedCommit,
    existing_extensions: &Extensions<GroupContext>,
) -> bool {
    // Only fire on the pre-flip side — a group that already has the
    // AppDataDictionary GCE can't "bootstrap" again.
    if crate::groups::check_proposals_enabled(existing_extensions) {
        return false;
    }

    let mut writes_component_registry = false;
    let mut gce_matches = false;
    for queued in staged_commit.queued_proposals() {
        match queued.proposal() {
            Proposal::GroupContextExtensions(gce) => {
                let exts = gce.extensions();
                // Symmetric routing: a GCE that strips the legacy
                // extensions without flipping RequiredCapabilities is
                // NOT a bootstrap commit. Routing it to the bootstrap
                // validator would surface a misleading
                // "RequiredCapabilities is missing" error when the
                // real problem is "you can't strip legacy extensions
                // unless you're migrating".
                gce_matches = gce_matches_bootstrap_shape(exts)
                    && gce_required_capabilities_match_bootstrap_shape(exts);
            }
            Proposal::AppDataUpdate(app_data)
                if ComponentId::from(app_data.component_id())
                    == ComponentId::COMPONENT_REGISTRY =>
            {
                writes_component_registry = true;
            }
            _ => {}
        }
    }
    gce_matches && writes_component_registry
}

/// Does `new_extensions` look like the bootstrap GCE output?
/// Drops the four legacy XMTP extensions. The `RequiredCapabilities`
/// shape check is enforced separately by
/// `gce_required_capabilities_match_bootstrap_shape` so the validator
/// can distinguish "no RC at all" from "RC has wrong shape" — pinned
/// by `gce_extension_set_missing_required_capabilities_rejected`.
fn gce_matches_bootstrap_shape(new_extensions: &Extensions<GroupContext>) -> bool {
    let drops_legacy = |id: u16| {
        !new_extensions
            .iter()
            .any(|ext| matches!(ext, Extension::Unknown(candidate, _) if *candidate == id))
    };
    let drops_immutable = !new_extensions
        .iter()
        .any(|ext| matches!(ext, Extension::ImmutableMetadata(_)));

    drops_legacy(MUTABLE_METADATA_EXTENSION_ID)
        && drops_legacy(GROUP_PERMISSIONS_EXTENSION_ID)
        && drops_legacy(GROUP_MEMBERSHIP_EXTENSION_ID)
        && drops_immutable
}

/// Does `new_extensions.required_capabilities()` carry the
/// `AppDataDictionary` flip the bootstrap requires? Used by the router
/// (`is_bootstrap_commit`) so a GCE that drops the legacy extensions
/// but doesn't flip RequiredCapabilities is NOT routed into the
/// bootstrap validator at all — the steady-state validator will
/// reject it for losing the legacy extensions on a non-migrated
/// group, which is the more accurate error mode.
fn gce_required_capabilities_match_bootstrap_shape(
    new_extensions: &Extensions<GroupContext>,
) -> bool {
    new_extensions
        .required_capabilities()
        .map(|rc| {
            rc.extension_types()
                .contains(&ExtensionType::AppDataDictionary)
        })
        .unwrap_or(false)
}

/// Extract the bootstrap commit's single GCE-proposal proposer.
/// Returns `None` if the commit has no GCE proposal (not bootstrap-
/// shaped). Any `Sender` other than `Member` (external, new-member)
/// is rejected as non-member per the existing commit validator's
/// shape.
pub(crate) fn extract_gce_proposer(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    immutable_metadata: &GroupMetadata,
    mutable_metadata: &GroupMutableMetadata,
) -> Result<Option<CommitParticipant>, CommitValidationError> {
    for queued in staged_commit.queued_proposals() {
        if matches!(queued.proposal(), Proposal::GroupContextExtensions(_)) {
            let leaf_index = match queued.sender() {
                Sender::Member(idx) => idx,
                _ => return Err(CommitValidationError::ActorNotMember),
            };
            let participant = extract_commit_participant(
                leaf_index,
                openmls_group,
                immutable_metadata,
                mutable_metadata,
            )?;
            return Ok(Some(participant));
        }
    }
    Ok(None)
}

/// Full receiver-side validation of a bootstrap commit.
///
/// Called from `ValidatedCommit::from_staged_commit` **before**
/// `validate_app_data_update_proposals_in_commit` so the bootstrap
/// path bypasses the deny-by-default registry check (the dict doesn't
/// have a `COMPONENT_REGISTRY` entry until this very commit merges).
///
/// The `proposer` argument is the GCE-proposal proposer extracted by
/// `get_proposal_changes` — bootstrap requires that actor to be a
/// super-admin per the pre-flip GMM. The legacy permission check is
/// still valid at this moment because the legacy extensions are still
/// present on the receiver side (the GCE strip only takes effect
/// after the commit merges).
pub(crate) fn validate_bootstrap_commit(
    staged_commit: &StagedCommit,
    openmls_group: &OpenMlsGroup,
    proposer: &CommitParticipant,
) -> Result<(), BootstrapValidationError> {
    if !proposer.is_super_admin {
        return Err(BootstrapValidationError::ProposerNotSuperAdmin);
    }

    // Defense-in-depth: bootstrap is membership-neutral and AppData-
    // only. Reject any proposal type other than `GroupContextExtensions`
    // and `AppDataUpdate` BEFORE the canonical-subset compare, so a
    // smuggled `Add`/`Remove`/`Update`/`SelfRemove`/`ReInit`/
    // `ExternalInit`/`AppEphemeral`/`Custom` proposal that satisfied
    // `is_bootstrap_commit` (which only checks the GCE shape + a
    // COMPONENT_REGISTRY write) cannot reach OpenMLS' merge step.
    validate_only_allowed_proposal_types(staged_commit)?;

    // Expected bytes: sync synthesis over the pre-flip extensions.
    // No identity-update API lookups; the receiver does NOT call the
    // sender-side async synthesizer.
    let expected = synthesize_canonical_subset_for_validation(openmls_group)?;

    // Actual: the `AppDataUpdate` proposal bag from the staged commit.
    let actual = collect_app_data_updates(staged_commit)?;

    validate_against_canonical_subset(&expected, &actual)?;
    validate_gce_shape(staged_commit)?;

    Ok(())
}

/// True iff `proposal` is a proposal type the bootstrap commit is
/// allowed to carry. Bootstrap is membership-neutral (no `Add`,
/// `Remove`, `Update`, `SelfRemove`) and identity-key-neutral (no
/// `ReInit`, `ExternalInit`, `PreSharedKey`); the canonical-subset
/// compare also presumes there are no `AppEphemeral` or `Custom`
/// payloads. The only legal types are the single GCE that drops the
/// legacy extensions plus requires `AppDataDictionary`, and the
/// `AppDataUpdate` proposals that seed the post-flip dictionary.
fn is_allowed_bootstrap_proposal(proposal: &Proposal) -> bool {
    matches!(
        proposal,
        Proposal::GroupContextExtensions(_) | Proposal::AppDataUpdate(_)
    )
}

/// Static debug name for a `Proposal` variant. Used solely to make the
/// `DisallowedProposalType` error self-describing in logs.
fn proposal_type_name(proposal: &Proposal) -> &'static str {
    match proposal {
        Proposal::Add(_) => "Add",
        Proposal::Update(_) => "Update",
        Proposal::Remove(_) => "Remove",
        Proposal::PreSharedKey(_) => "PreSharedKey",
        Proposal::ReInit(_) => "ReInit",
        Proposal::ExternalInit(_) => "ExternalInit",
        Proposal::GroupContextExtensions(_) => "GroupContextExtensions",
        Proposal::AppDataUpdate(_) => "AppDataUpdate",
        Proposal::SelfRemove => "SelfRemove",
        Proposal::AppEphemeral(_) => "AppEphemeral",
        Proposal::_AppAck => "_AppAck",
        Proposal::Custom(_) => "Custom",
    }
}

/// Reject any queued proposal whose type isn't on the bootstrap
/// allowlist. See [`is_allowed_bootstrap_proposal`] for the allowed set
/// and the security rationale.
fn validate_only_allowed_proposal_types(
    staged_commit: &StagedCommit,
) -> Result<(), BootstrapValidationError> {
    for queued in staged_commit.queued_proposals() {
        let proposal = queued.proposal();
        if !is_allowed_bootstrap_proposal(proposal) {
            return Err(BootstrapValidationError::DisallowedProposalType(
                proposal_type_name(proposal),
            ));
        }
    }
    Ok(())
}

/// Collect every `AppDataUpdate` proposal in a staged commit into a
/// `BTreeMap` keyed by `ComponentId`. Reject duplicate component ids:
/// OpenMLS sorts AppData proposals by `(component_id, op_type)` at
/// apply time but the validator iterates in queue order — a duplicate
/// where one entry happens to byte-equal the canonical subset could
/// green-light a different payload than what gets merged. Fail closed
/// with [`BootstrapValidationError::DuplicateProposal`].
fn collect_app_data_updates(
    staged_commit: &StagedCommit,
) -> Result<BTreeMap<ComponentId, (AppDataUpdateOperationType, Vec<u8>)>, BootstrapValidationError>
{
    use openmls::messages::proposals::AppDataUpdateOperation;
    let mut out = BTreeMap::new();
    for queued in staged_commit.queued_proposals() {
        if let Proposal::AppDataUpdate(p) = queued.proposal() {
            let id = ComponentId::from(p.component_id());
            let (op_type, bytes) = match p.operation() {
                AppDataUpdateOperation::Update(bytes) => (
                    AppDataUpdateOperationType::Update,
                    bytes.as_slice().to_vec(),
                ),
                AppDataUpdateOperation::Remove => (AppDataUpdateOperationType::Remove, Vec::new()),
            };
            if out.insert(id, (op_type, bytes)).is_some() {
                return Err(BootstrapValidationError::DuplicateProposal(id));
            }
        }
    }
    Ok(out)
}

/// Pure-logic byte-compare between the canonical subset and the
/// actual `AppDataUpdate` proposal bag. Separated from the
/// `StagedCommit`-reading layer so it can be unit-tested directly.
///
/// All four `CanonicalBootstrapExpectation` fields are validated here,
/// in two layers:
/// 1. **Strict byte-compare** — `expected.strict` (sender-authoritative
///    components like `GROUP_NAME`, `ADMIN_LIST`, etc.) is compared
///    op-type and bytes-for-bytes against `actual` (loops 1 + 2 below).
/// 2. **Decoded compares** — `COMPONENT_REGISTRY` and `GROUP_MEMBERSHIP`
///    can't be byte-compared safely (encoder-version drift on the proto
///    payload, sender-discretionary `failed_installations` bounded by
///    the pre-flip set), so they're delegated to
///    [`validate_component_registry_payload`] (consumes
///    `expected.expected_registry`) and
///    [`validate_group_membership_payload`] (consumes both
///    `expected.membership_sequence_ids` and
///    `expected.allowed_failed_installations`). The strict-loop's
///    "unexpected proposal" check explicitly allowlists those two
///    component ids so the byte-compare layer doesn't reject them
///    before the decoded layer gets to validate them.
///
/// Together these cover every field on
/// [`CanonicalBootstrapExpectation`]; if you add a new field there,
/// add a corresponding compare here (or to one of the two delegates).
pub(crate) fn validate_against_canonical_subset(
    expected: &CanonicalBootstrapExpectation,
    actual: &BTreeMap<ComponentId, (AppDataUpdateOperationType, Vec<u8>)>,
) -> Result<(), BootstrapValidationError> {
    // 1. Every strict component must appear with matching op_type +
    //    payload bytes.
    for (id, (expected_op, expected_bytes)) in expected.strict.iter() {
        let Some((actual_op, actual_bytes)) = actual.get(id) else {
            return Err(BootstrapValidationError::MissingSeed(*id));
        };
        if actual_op != expected_op || actual_bytes != expected_bytes {
            return Err(BootstrapValidationError::Mismatch(*id));
        }
    }

    // 2. Nothing outside `expected.strict`, `COMPONENT_REGISTRY`, or
    //    `GROUP_MEMBERSHIP` (the latter two are validated separately
    //    via decoded compare and hybrid-bounds compare respectively).
    for id in actual.keys() {
        if id != &ComponentId::GROUP_MEMBERSHIP
            && id != &ComponentId::COMPONENT_REGISTRY
            && !expected.strict.contains_key(id)
        {
            return Err(BootstrapValidationError::UnexpectedProposal(*id));
        }
    }

    // 3. COMPONENT_REGISTRY: decoded per-entry compare against
    //    `expected.expected_registry`. Byte-compare is brittle to
    //    proto evolution / encoder differences (see
    //    `CanonicalBootstrapExpectation` docs); decode actual and
    //    compare typed `ComponentMetadata`s.
    validate_component_registry_payload(
        actual.get(&ComponentId::COMPONENT_REGISTRY),
        &expected.expected_registry,
    )?;

    // 4. GROUP_MEMBERSHIP hybrid validation: sequence_id must match
    //    the pre-flip state exactly (byte-compare); per-inbox
    //    `failed_installations` is bounded by the pre-flip legacy
    //    list (`allowed_failed_installations`).
    validate_group_membership_payload(
        actual.get(&ComponentId::GROUP_MEMBERSHIP),
        &expected.membership_sequence_ids,
        &expected.allowed_failed_installations,
    )?;

    Ok(())
}

/// Decoded compare of the bootstrap commit's `COMPONENT_REGISTRY`
/// payload against the receiver-derived `expected_registry`.
///
/// Per-entry equality on `ComponentMetadata` (a prost message with
/// `PartialEq`) tolerates encoder-version drift on the inner bytes that
/// a raw byte-compare would not. Missing, extra, or differently-shaped
/// entries surface as [`BootstrapValidationError::RegistryEntryMismatch`]
/// — same variant for all three so the operator-side log says which
/// component is wrong without juggling three error types.
fn validate_component_registry_payload(
    actual: Option<&(AppDataUpdateOperationType, Vec<u8>)>,
    expected_registry: &BTreeMap<
        ComponentId,
        xmtp_proto::xmtp::mls::message_contents::ComponentMetadata,
    >,
) -> Result<(), BootstrapValidationError> {
    let Some((op, bytes)) = actual else {
        // No COMPONENT_REGISTRY payload at all.
        return if expected_registry.is_empty() {
            Ok(())
        } else {
            Err(BootstrapValidationError::MissingSeed(
                ComponentId::COMPONENT_REGISTRY,
            ))
        };
    };
    if *op != AppDataUpdateOperationType::Update {
        return Err(BootstrapValidationError::Mismatch(
            ComponentId::COMPONENT_REGISTRY,
        ));
    }
    // The wire payload for COMPONENT_REGISTRY at bootstrap is a
    // `TlsMapDelta<ComponentId, VLBytes>` of all-`Insert` mutations
    // (delta-from-empty). Walk it directly — `ComponentRegistry::from_bytes`
    // is the dict-storage decoder (snapshot), not the wire decoder.
    let delta = TlsMapDelta::<ComponentId, tls_codec::VLBytes>::tls_deserialize_exact(bytes)
        .map_err(|e| BootstrapValidationError::PayloadDecode {
            component_id: ComponentId::COMPONENT_REGISTRY,
            reason: format!("TlsMapDelta decode: {e}"),
        })?;

    let mut actual_entries: BTreeMap<ComponentId, ComponentMetadata> = BTreeMap::new();
    for mutation in delta.mutations {
        let (id, raw) = match mutation {
            TlsMapMutation::Insert { key, value } => (key, value),
            TlsMapMutation::Update { .. } | TlsMapMutation::Delete { .. } => {
                return Err(BootstrapValidationError::PayloadDecode {
                    component_id: ComponentId::COMPONENT_REGISTRY,
                    reason: "COMPONENT_REGISTRY bootstrap delta carried a non-Insert mutation"
                        .into(),
                });
            }
        };
        ComponentRegistry::validate_entry(id, raw.as_slice()).map_err(|e| {
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: format!("ComponentRegistry entry validate: {e}"),
            }
        })?;
        let meta = ComponentMetadata::decode(raw.as_slice()).map_err(|e| {
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: format!("ComponentMetadata decode for {id}: {e}"),
            }
        })?;
        if actual_entries.insert(id, meta).is_some() {
            return Err(BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::COMPONENT_REGISTRY,
                reason: format!("COMPONENT_REGISTRY bootstrap delta has duplicate Insert for {id}"),
            });
        }
    }

    for (id, expected_meta) in expected_registry.iter() {
        let Some(actual_meta) = actual_entries.get(id) else {
            return Err(BootstrapValidationError::RegistryEntryMismatch(*id));
        };
        if actual_meta != expected_meta {
            return Err(BootstrapValidationError::RegistryEntryMismatch(*id));
        }
    }
    for id in actual_entries.keys() {
        if !expected_registry.contains_key(id) {
            return Err(BootstrapValidationError::RegistryEntryMismatch(*id));
        }
    }
    // Cardinality belt: the two existence loops above imply set
    // equality, but only because `actual_entries` is a `BTreeMap` (so
    // duplicate keys collapsed silently during decode). If a future
    // ComponentRegistry encoder changes that, the entry-mismatch loops
    // wouldn't notice — fail closed on count divergence.
    if actual_entries.len() != expected_registry.len() {
        return Err(BootstrapValidationError::PayloadDecode {
            component_id: ComponentId::COMPONENT_REGISTRY,
            reason: format!(
                "COMPONENT_REGISTRY entry cardinality mismatch: payload has {} entries, \
                 expected {}",
                actual_entries.len(),
                expected_registry.len()
            ),
        });
    }
    Ok(())
}

/// Hybrid validation for `GROUP_MEMBERSHIP`:
/// - Payload must be present with op_type `Update`.
/// - Deserialize as `TlsMapDelta<InboxId, VLBytes>` (the wire is
///   always a delta; bootstrap is the case where every mutation is
///   an `Insert` against an empty prior). Each value prost-decodes
///   as a `GroupMembershipEntry` envelope and we read its `V1`
///   variant.
/// - Inbox-id set must equal the pre-flip membership (no extras,
///   nothing missing); duplicate Inserts surface as decode errors.
/// - Each entry's `sequence_id` must equal the pre-flip value.
/// - `failed_installations` is sender-authoritative on which subset to
///   carry per inbox, but the *universe* of legal install ids is
///   bounded by `allowed_failed_installations` (drawn from the pre-flip
///   legacy `GroupMembership.failed_installations` list). Each entry
///   must be 32 bytes (Ed25519 install key) AND present in that set.
fn validate_group_membership_payload(
    actual: Option<&(AppDataUpdateOperationType, Vec<u8>)>,
    expected_sequence_ids: &BTreeMap<InboxId, u64>,
    allowed_failed_installations: &std::collections::BTreeSet<[u8; 32]>,
) -> Result<(), BootstrapValidationError> {
    let Some((op, bytes)) = actual else {
        // No GROUP_MEMBERSHIP entry at all. If expected_sequence_ids
        // is empty this is fine (an empty membership doesn't need a
        // payload); otherwise every expected inbox is "missing."
        return match expected_sequence_ids.keys().next() {
            Some(missing) => Err(BootstrapValidationError::MissingMember(
                missing.as_bytes().to_vec(),
            )),
            None => Ok(()),
        };
    };
    if *op != AppDataUpdateOperationType::Update {
        return Err(BootstrapValidationError::Mismatch(
            ComponentId::GROUP_MEMBERSHIP,
        ));
    }

    let delta =
        TlsMapDelta::<InboxId, tls_codec::VLBytes>::tls_deserialize_exact(bytes).map_err(|e| {
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::GROUP_MEMBERSHIP,
                reason: format!("TlsMapDelta decode: {e}"),
            }
        })?;

    let mut seen: std::collections::BTreeSet<InboxId> = std::collections::BTreeSet::new();
    for mutation in delta.mutations {
        let (inbox_id, raw_entry) = match mutation {
            TlsMapMutation::Insert { key, value } => (key, value),
            TlsMapMutation::Update { .. } | TlsMapMutation::Delete { .. } => {
                return Err(BootstrapValidationError::PayloadDecode {
                    component_id: ComponentId::GROUP_MEMBERSHIP,
                    reason: "GROUP_MEMBERSHIP bootstrap delta carried a non-Insert mutation".into(),
                });
            }
        };
        if !seen.insert(inbox_id) {
            return Err(BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::GROUP_MEMBERSHIP,
                reason: format!(
                    "GROUP_MEMBERSHIP bootstrap delta has duplicate Insert for inbox {}",
                    inbox_id.to_hex()
                ),
            });
        }
        let Some(expected_seq) = expected_sequence_ids.get(&inbox_id) else {
            return Err(BootstrapValidationError::ExtraMember(
                inbox_id.as_bytes().to_vec(),
            ));
        };
        let envelope = GroupMembershipEntry::decode(raw_entry.as_slice()).map_err(|e| {
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::GROUP_MEMBERSHIP,
                reason: format!(
                    "GroupMembershipEntry decode for inbox {}: {e}",
                    inbox_id.to_hex()
                ),
            }
        })?;
        let entry = match envelope.version {
            Some(GroupMembershipEntryVersion::V1(v1)) => v1,
            None => {
                return Err(BootstrapValidationError::PayloadDecode {
                    component_id: ComponentId::GROUP_MEMBERSHIP,
                    reason: format!(
                        "GroupMembershipEntry envelope for inbox {} has unknown version",
                        inbox_id.to_hex()
                    ),
                });
            }
        };
        if entry.sequence_id != *expected_seq {
            return Err(BootstrapValidationError::SequenceIdMismatch {
                inbox_id: inbox_id.as_bytes().to_vec(),
                expected: *expected_seq,
                actual: entry.sequence_id,
            });
        }
        for fi in &entry.failed_installations {
            // 32-byte length is the Ed25519 install-key contract. A
            // wrong length implies a malformed payload, not a
            // sender-vs-receiver disagreement.
            let key: [u8; 32] =
                fi.as_slice()
                    .try_into()
                    .map_err(|_| BootstrapValidationError::PayloadDecode {
                        component_id: ComponentId::GROUP_MEMBERSHIP,
                        reason: format!(
                            "failed_installation for inbox {} is {} bytes, expected 32",
                            inbox_id.to_hex(),
                            fi.len()
                        ),
                    })?;
            if !allowed_failed_installations.contains(&key) {
                return Err(BootstrapValidationError::UnauthorizedFailedInstallation(
                    fi.clone(),
                ));
            }
        }
    }

    // Every expected inbox must be in the snapshot.
    for inbox_id in expected_sequence_ids.keys() {
        if !seen.contains(inbox_id) {
            return Err(BootstrapValidationError::MissingMember(
                inbox_id.as_bytes().to_vec(),
            ));
        }
    }

    Ok(())
}

/// Verify the commit's single GCE proposal has the bootstrap shape:
/// drops exactly the four legacy extension types and lists
/// `AppDataDictionary` in `RequiredCapabilities`. Any extra add/drop
/// beyond the bootstrap contract → reject.
fn validate_gce_shape(staged_commit: &StagedCommit) -> Result<(), BootstrapValidationError> {
    let mut found_gce = None;
    for queued in staged_commit.queued_proposals() {
        if let Proposal::GroupContextExtensions(gce) = queued.proposal() {
            if found_gce.is_some() {
                return Err(BootstrapValidationError::GceMismatch(
                    "multiple GCE proposals in bootstrap commit".into(),
                ));
            }
            found_gce = Some(gce);
        }
    }
    let Some(gce) = found_gce else {
        return Err(BootstrapValidationError::GceMismatch(
            "no GCE proposal in bootstrap commit".into(),
        ));
    };
    validate_gce_extension_set(gce.extensions())
}

/// Pure-extension-bag validation. Split out of [`validate_gce_shape`]
/// so the `RequiredCapabilities`-presence check (security-critical,
/// see in-body comment) can be unit-tested directly without standing
/// up a real `StagedCommit`.
fn validate_gce_extension_set(
    new_extensions: &Extensions<GroupContext>,
) -> Result<(), BootstrapValidationError> {
    if !gce_matches_bootstrap_shape(new_extensions) {
        return Err(BootstrapValidationError::GceMismatch(
            "GCE extension set does not match bootstrap shape (needs \
             -MUTABLE_METADATA, -GROUP_PERMISSIONS, -GROUP_MEMBERSHIP, -ImmutableMetadata)"
                .into(),
        ));
    }

    // RequiredCapabilities MUST be present and MUST drop the four
    // legacy extension types while adding `AppDataDictionary`. A
    // bootstrap commit with no RequiredCapabilities at all would let
    // post-flip adds skip the AppData support check and rejoin a
    // broken group shape — reject hard.
    let required = new_extensions.required_capabilities().ok_or_else(|| {
        BootstrapValidationError::GceMismatch(
            "RequiredCapabilities extension is missing — bootstrap requires it to enforce \
             AppDataDictionary support and ban the four legacy extension types"
                .into(),
        )
    })?;
    let ext_types: Vec<ExtensionType> = required.extension_types().to_vec();
    let requires_app_data_dictionary = ext_types.contains(&ExtensionType::AppDataDictionary);
    let bans_legacy = ![
        ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID),
        ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID),
        ExtensionType::ImmutableMetadata,
    ]
    .iter()
    .any(|banned| ext_types.contains(banned));
    if !requires_app_data_dictionary || !bans_legacy {
        return Err(BootstrapValidationError::GceMismatch(
            "RequiredCapabilities doesn't match bootstrap shape".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! Pure-logic coverage for [`validate_against_canonical_subset`]
    //! and [`validate_group_membership_payload`]. The `StagedCommit`-
    //! reading helpers need a real MLS group; covered in integration tests.
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use xmtp_mls_common::app_data::migration::{
        CanonicalBootstrapExpectation, encode_group_membership_delta,
    };
    use xmtp_proto::xmtp::mls::message_contents::group_membership_entry::V1 as GroupMembershipEntryV1;

    /// Build an `InboxId` from a single fill byte. Tests use these as
    /// throwaway distinct identifiers — no semantics, just "give me a
    /// new one." Keeps call sites readable (`inbox(0xAA)`) instead of
    /// `InboxId::from_bytes([0xAA; 32])`.
    fn inbox(b: u8) -> InboxId {
        InboxId::from_bytes([b; 32])
    }

    fn expected(
        strict: Vec<(ComponentId, AppDataUpdateOperationType, Vec<u8>)>,
        membership: Vec<(InboxId, u64)>,
    ) -> CanonicalBootstrapExpectation {
        // Empty `expected_registry` + empty `allowed_failed_installations`
        // is the correct baseline for tests focused on `strict` and
        // membership-sequence-id behavior: a commit with no
        // COMPONENT_REGISTRY proposal and no failed_installations is
        // valid against an empty expectation. Tests targeting registry
        // or failed-installation bounds construct the expectation
        // directly with non-empty fields.
        CanonicalBootstrapExpectation {
            strict: strict
                .into_iter()
                .map(|(id, op, b)| (id, (op, b)))
                .collect(),
            expected_registry: BTreeMap::new(),
            membership_sequence_ids: membership.into_iter().collect(),
            allowed_failed_installations: BTreeSet::new(),
        }
    }

    fn seed(op: AppDataUpdateOperationType, bytes: &[u8]) -> (AppDataUpdateOperationType, Vec<u8>) {
        (op, bytes.to_vec())
    }

    fn membership_entry(seq: u64, failed: Vec<Vec<u8>>) -> GroupMembershipEntry {
        GroupMembershipEntry {
            version: Some(GroupMembershipEntryVersion::V1(GroupMembershipEntryV1 {
                sequence_id: seq,
                failed_installations: failed,
                admitted_via_external_group_id: vec![],
            })),
        }
    }

    /// `(inbox_id, sequence_id, failed_installations)` triple used by
    /// [`encoded_membership`] to stamp out test payloads.
    type MembershipInputRow = (InboxId, u64, Vec<Vec<u8>>);

    fn encoded_membership(entries: &[MembershipInputRow]) -> Vec<u8> {
        let mut map: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
        for (inbox, seq, failed) in entries {
            map.insert(*inbox, membership_entry(*seq, failed.clone()));
        }
        encode_group_membership_delta(&map).unwrap()
    }

    #[test]
    fn happy_path_accepts_matching_bag() {
        let exp = expected(
            vec![(
                ComponentId::GROUP_NAME,
                AppDataUpdateOperationType::Update,
                b"test".to_vec(),
            )],
            vec![(inbox(0x11), 1_u64)],
        );
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_NAME,
            seed(AppDataUpdateOperationType::Update, b"test"),
        );
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[(inbox(0x11), 1_u64, vec![])]),
            ),
        );
        validate_against_canonical_subset(&exp, &actual).unwrap();
    }

    #[test]
    fn missing_strict_component_surfaces_missing_seed() {
        let exp = expected(
            vec![(
                ComponentId::GROUP_NAME,
                AppDataUpdateOperationType::Update,
                b"expected".to_vec(),
            )],
            vec![],
        );
        let actual = BTreeMap::new();
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::MissingSeed(ComponentId::GROUP_NAME)
        ));
    }

    #[test]
    fn byte_mismatch_surfaces_mismatch() {
        let exp = expected(
            vec![(
                ComponentId::GROUP_NAME,
                AppDataUpdateOperationType::Update,
                b"expected".to_vec(),
            )],
            vec![],
        );
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_NAME,
            seed(AppDataUpdateOperationType::Update, b"DIFFERENT"),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::Mismatch(ComponentId::GROUP_NAME)
        ));
    }

    #[test]
    fn op_type_mismatch_surfaces_mismatch() {
        let exp = expected(
            vec![(
                ComponentId::GROUP_NAME,
                AppDataUpdateOperationType::Update,
                b"v".to_vec(),
            )],
            vec![],
        );
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_NAME,
            seed(AppDataUpdateOperationType::Remove, b"v"),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::Mismatch(ComponentId::GROUP_NAME)
        ));
    }

    #[test]
    fn unexpected_component_surfaces_unexpected_proposal() {
        // Sender tried to smuggle in an `APP_DATA` seed outside the
        // canonical subset.
        let exp = expected(vec![], vec![]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::APP_DATA,
            seed(AppDataUpdateOperationType::Update, b"malicious"),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::UnexpectedProposal(ComponentId::APP_DATA)
        ));
    }

    #[test]
    fn group_membership_sequence_id_mismatch_rejected() {
        // Security-critical: a forged higher sequence_id would
        // silently skip legitimate identity updates on receivers.
        let exp = expected(vec![], vec![(inbox(0xAA), 7_u64)]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[(inbox(0xAA), 9999_u64, vec![])]),
            ),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::SequenceIdMismatch {
                expected: 7,
                actual: 9999,
                ..
            }
        ));
    }

    #[test]
    fn group_membership_extra_member_rejected() {
        let exp = expected(vec![], vec![(inbox(0xAA), 1_u64)]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[
                    (inbox(0xAA), 1_u64, vec![]),
                    (inbox(0xBB), 2_u64, vec![]), // extra
                ]),
            ),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::ExtraMember(ref id) if id == &vec![0xBB; 32]
        ));
    }

    #[test]
    fn group_membership_missing_member_rejected() {
        let exp = expected(vec![], vec![(inbox(0xAA), 1_u64), (inbox(0xBB), 2_u64)]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[(inbox(0xAA), 1_u64, vec![])]), // BB missing
            ),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::MissingMember(ref id) if id == &vec![0xBB; 32]
        ));
    }

    #[test]
    fn group_membership_failed_installations_accepted_within_allowed_set() {
        // `failed_installations` is sender-authoritative on which subset
        // to carry per inbox, but each entry must be 32 bytes AND in
        // `allowed_failed_installations` (the pre-flip universe).
        let install_a = [0xDE_u8; 32];
        let install_b = [0xAD_u8; 32];
        let mut exp = expected(vec![], vec![(inbox(0xAA), 1_u64)]);
        exp.allowed_failed_installations.insert(install_a);
        exp.allowed_failed_installations.insert(install_b);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[(
                    inbox(0xAA),
                    1_u64,
                    vec![install_a.to_vec(), install_b.to_vec()],
                )]),
            ),
        );
        validate_against_canonical_subset(&exp, &actual).unwrap();
    }

    #[test]
    fn group_membership_unauthorized_failed_installation_rejected() {
        // A `failed_installation` not in `allowed_failed_installations`
        // is rejected: the legacy pre-flip list is the only authoritative
        // source for which install ids may appear.
        let install_legit = [0xDE_u8; 32];
        let install_smuggled = [0xBE_u8; 32];
        let mut exp = expected(vec![], vec![(inbox(0xAA), 1_u64)]);
        exp.allowed_failed_installations.insert(install_legit);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[(inbox(0xAA), 1_u64, vec![install_smuggled.to_vec()])]),
            ),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::UnauthorizedFailedInstallation(ref id) if id == &install_smuggled.to_vec()
        ));
    }

    #[test]
    fn component_registry_empty_round_trips() {
        // Empty `expected_registry` + empty COMPONENT_REGISTRY proposal
        // round-trips through TlsMap snapshot decode and passes. Exercises
        // the registry decode path without needing the full
        // ComponentMetadata-construction infrastructure (covered in
        // integration tests where a real ComponentRegistry is built).
        use xmtp_mls_common::app_data::component_registry::ComponentRegistry;
        let empty_registry_bytes = ComponentRegistry::new().to_bytes().unwrap();
        let exp = expected(vec![], vec![]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::COMPONENT_REGISTRY,
            (AppDataUpdateOperationType::Update, empty_registry_bytes),
        );
        validate_against_canonical_subset(&exp, &actual).unwrap();
    }

    #[test]
    fn component_registry_malformed_bytes_surface_decode() {
        let exp = expected(vec![], vec![]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::COMPONENT_REGISTRY,
            (AppDataUpdateOperationType::Update, vec![0xFF, 0xFE, 0xFD]),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::COMPONENT_REGISTRY,
                ..
            }
        ));
    }

    #[test]
    fn group_membership_wrong_length_failed_installation_surfaces_decode() {
        // Wrong-length entries (not 32 bytes) imply a malformed payload,
        // surfaced as `PayloadDecode` rather than the
        // unauthorized-install variant.
        let mut exp = expected(vec![], vec![(inbox(0xAA), 1_u64)]);
        exp.allowed_failed_installations.insert([0xDE_u8; 32]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                encoded_membership(&[(inbox(0xAA), 1_u64, vec![vec![0xDE; 16]])]),
            ),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::GROUP_MEMBERSHIP,
                ..
            }
        ));
    }

    #[test]
    fn group_membership_empty_both_sides_accepted() {
        // Expected empty membership + no GROUP_MEMBERSHIP proposal →
        // trivially valid. Matches the "empty group" edge case.
        let exp = expected(vec![], vec![]);
        let actual = BTreeMap::new();
        validate_against_canonical_subset(&exp, &actual).unwrap();
    }

    #[test]
    fn group_membership_malformed_tlsmap_bytes_surfaces_decode() {
        let exp = expected(vec![], vec![(inbox(0xAA), 1_u64)]);
        let mut actual = BTreeMap::new();
        actual.insert(
            ComponentId::GROUP_MEMBERSHIP,
            (
                AppDataUpdateOperationType::Update,
                vec![0xDE, 0xAD, 0xBE, 0xEF],
            ),
        );
        let err = validate_against_canonical_subset(&exp, &actual).unwrap_err();
        assert!(matches!(
            err,
            BootstrapValidationError::PayloadDecode {
                component_id: ComponentId::GROUP_MEMBERSHIP,
                ..
            }
        ));
    }

    #[test]
    fn allowed_bootstrap_proposal_predicate_rejects_membership_and_smuggled_types() {
        // SelfRemove and Custom are the trivially-constructible
        // disallowed variants; the rest (Add/Update/Remove/PreSharedKey/
        // ReInit/ExternalInit/AppEphemeral) need full openmls plumbing
        // to construct and are covered indirectly by the same `match`
        // arm in `is_allowed_bootstrap_proposal`. Coverage here proves
        // the predicate fails closed for at least one membership-
        // changing variant and one opaque-payload variant.
        use openmls::messages::proposals::CustomProposal;

        assert!(!is_allowed_bootstrap_proposal(&Proposal::SelfRemove));
        assert_eq!(proposal_type_name(&Proposal::SelfRemove), "SelfRemove");

        let custom = Proposal::Custom(Box::new(CustomProposal::new(0xCAFE, vec![0xBE, 0xEF])));
        assert!(!is_allowed_bootstrap_proposal(&custom));
        assert_eq!(proposal_type_name(&custom), "Custom");
    }

    /// Build a bootstrap-shaped GCE extension set (drops the four
    /// legacy extensions). `with_required_capabilities` controls
    /// whether the synthesized `RequiredCapabilities` extension is
    /// included with `AppDataDictionary` listed — that's the
    /// security-critical bit under test.
    fn bootstrap_gce_extensions(with_required_capabilities: bool) -> Extensions<GroupContext> {
        use openmls::extensions::{Extension, RequiredCapabilitiesExtension};
        use openmls::prelude::ProposalType;
        let mut exts = Extensions::empty();
        if with_required_capabilities {
            let ext_types = [ExtensionType::AppDataDictionary];
            let proposal_types = [ProposalType::AppDataUpdate];
            let required = RequiredCapabilitiesExtension::new(&ext_types, &proposal_types, &[]);
            exts.add(Extension::RequiredCapabilities(required)).unwrap();
        }
        exts
    }

    #[test]
    fn gce_extension_set_missing_required_capabilities_rejected() {
        // Security-critical: a bootstrap commit that strips
        // `RequiredCapabilities` lets post-flip adds skip the
        // `AppDataDictionary` requirement and rejoin a broken group
        // shape. Pure-extension-bag check covers the path that
        // `validate_gce_shape` delegates to.
        let exts = bootstrap_gce_extensions(false);
        let err = validate_gce_extension_set(&exts).unwrap_err();
        match err {
            BootstrapValidationError::GceMismatch(msg) => {
                assert!(
                    msg.contains("RequiredCapabilities extension is missing"),
                    "expected RequiredCapabilities-missing message, got: {msg}"
                );
            }
            other => panic!("expected GceMismatch, got {other:?}"),
        }
    }

    #[test]
    fn gce_extension_set_with_required_capabilities_accepted() {
        let exts = bootstrap_gce_extensions(true);
        validate_gce_extension_set(&exts).unwrap();
    }
}
