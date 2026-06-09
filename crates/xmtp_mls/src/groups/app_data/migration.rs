//! Sender-side bootstrap synthesis that needs async access to identity
//! updates (for the `failed_installations` per-inbox partition).
//!
//! Mirrors the wire-format contract enforced by the receiver-side
//! `synthesize_canonical_subset_for_validation` in
//! `xmtp_mls_common::app_data::migration`. The split is:
//!
//! - sync pure-local synthesis (registry, admin/super-admin lists,
//!   Bytes attrs, sequence-id map, immutable seeds) → `xmtp_mls_common`
//! - async identity-update-dependent synthesis
//!   (`failed_installations` partition) → this module
//!
//! The output is the full `BTreeMap<ComponentId, Vec<u8>>` the
//! bootstrap intent handler fans out as individual `AppDataUpdate`
//! proposals, sorted by ComponentId ascending.

use std::collections::BTreeMap;

use openmls::{
    extensions::Extensions,
    group::{GroupContext, MlsGroup as OpenMlsGroup},
    messages::proposals::{AppDataUpdateProposal, Proposal},
    prelude::CommitMessageBundle,
    storage::OpenMlsProvider,
};
use prost::Message as _;
use tls_codec::{Serialize as _, VLBytes};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentRegistry, ComponentRegistryError},
        migration::{self, CanonicalBootstrapExpectation, MigrationError as CommonMigrationError},
    },
    inbox_id::InboxId,
    tls_map::TlsMapDelta,
};
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMembership as GroupMembershipProto, GroupMembershipEntry,
    group_membership_entry::{
        V1 as GroupMembershipEntryV1, Version as GroupMembershipEntryVersion,
    },
};

use crate::{context::XmtpSharedContext, identity_updates::IdentityUpdates};

/// Sender-side bootstrap synthesis errors.
#[derive(Debug, thiserror::Error, xmtp_common::Retryable)]
pub enum BootstrapSynthesisError {
    #[error(transparent)]
    Common(#[from] CommonMigrationError),

    /// Bootstrap can't fall back silently: a missing installation list
    /// means we can't partition `failed_installations`, so we surface
    /// the lookup error rather than emit incorrect bytes.
    // Most synthesis failures are deterministic over the inputs and
    // not retryable (decode errors, registry-shape mismatches, common
    // migration validation). The exception is `IdentityUpdateLookup`,
    // which wraps a [`crate::client::ClientError`] that can carry a
    // transient API failure (network blip, server 5xx). Delegate
    // retryability to the wrapped client error so a momentary blip
    // during bootstrap doesn't permanently fail the intent.
    #[error("identity-update lookup failed for inbox {inbox_id}: {source}")]
    #[retry(when = source.is_retryable())]
    IdentityUpdateLookup {
        inbox_id: String,
        #[source]
        source: crate::client::ClientError,
    },

    #[error("legacy GroupMembership decode: {0}")]
    LegacyMembershipDecode(#[from] prost::DecodeError),

    /// Re-encoding the canonical-subset's `expected_registry` into the
    /// `COMPONENT_REGISTRY` wire bytes failed. Surfaces a
    /// [`ComponentRegistryError`] from the per-entry `set` round-trip
    /// (rejected `ComponentMetadata`, reserved id, etc.). The
    /// canonical-subset synthesizer already validated these values, so
    /// hitting this path indicates a logic divergence between the two
    /// crates rather than user-visible state.
    #[error("registry re-encode: {0}")]
    RegistryReEncode(#[from] ComponentRegistryError),

    /// A `GROUP_MEMBERSHIP` `sequence_id` exceeded `i64::MAX` and so
    /// can't be passed through to the identity-update API (whose query
    /// surface is signed). Practically unreachable today (sequence ids
    /// don't get within thirteen orders of magnitude of `i64::MAX`),
    /// but a `as i64` cast would silently wrap to a negative value and
    /// quietly query the wrong association state — surface it as a
    /// terminal error rather than risk a deceptive partition of
    /// `failed_installations`.
    #[error(
        "sequence_id {sequence_id} for inbox {inbox_id} exceeds i64::MAX; cannot query identity-update history"
    )]
    SequenceIdOverflow { inbox_id: String, sequence_id: u64 },

    /// TLS-encoding the bootstrap wire delta for COMPONENT_REGISTRY
    /// (or any other inline `TlsMapDelta`) failed. Surfaces a
    /// `tls_codec::Error` from the underlying serializer. Practically
    /// unreachable (we just constructed the delta in memory), but the
    /// `?` operator needs a conversion.
    #[error("wire delta encode: {0}")]
    TlsCodec(#[from] tls_codec::Error),
}

/// Synthesize the full `AppDataUpdate` payload set the bootstrap
/// commit ships, keyed by `ComponentId`.
///
/// Sync parts delegate to [`synthesize_canonical_subset_for_validation`];
/// the async part calls `IdentityUpdates::get_association_state` to
/// partition `failed_installations` by owning inbox. Installations
/// whose owner can't be resolved are dropped — `failed_installations`
/// is a retry-suppression hint, so each costs at most one extra retry.
pub async fn synthesize_initial_component_values<C: XmtpSharedContext>(
    context: &C,
    mls_group: &OpenMlsGroup,
) -> Result<BTreeMap<ComponentId, Vec<u8>>, BootstrapSynthesisError> {
    synthesize_initial_component_values_from_extensions(context, mls_group.extensions()).await
}

/// Extensions-only variant of [`synthesize_initial_component_values`].
/// Lets tests exercise synthesis against synthetic extensions without
/// standing up a real MLS group.
pub async fn synthesize_initial_component_values_from_extensions<C: XmtpSharedContext>(
    context: &C,
    extensions: &Extensions<GroupContext>,
) -> Result<BTreeMap<ComponentId, Vec<u8>>, BootstrapSynthesisError> {
    use xmtp_mls_common::app_data::migration::synthesize_canonical_subset_from_extensions;

    // Sync synthesis produces everything except GROUP_MEMBERSHIP.
    let canonical: CanonicalBootstrapExpectation =
        synthesize_canonical_subset_from_extensions(extensions)?;

    let mut out: BTreeMap<ComponentId, Vec<u8>> = BTreeMap::new();
    for (id, (_op_type, bytes)) in canonical.strict.into_iter() {
        out.insert(id, bytes);
    }

    // COMPONENT_REGISTRY wire payload: a `TlsMapDelta<ComponentId,
    // VLBytes>` of all-`Insert` mutations (bootstrap = delta-from-
    // empty). Built inline from `canonical.expected_registry` —
    // there's no whole-registry wire encoder on `ComponentRegistry`
    // because every steady-state caller emits only the few entries
    // it touches, so a "to_wire_bytes" method would only ever be
    // used here. Keeping it inline avoids adding a one-call-site
    // helper. The receiver decodes via `decode_component_registry_delta`
    // inside the bootstrap validator and via `apply_wire_bytes` for
    // the dict write — both produce the same materialized state.
    //
    // Round-trip-validate each metadata entry through `ComponentRegistry::set`
    // first so we surface a `ComponentRegistryError` here (rather than
    // shipping bytes that fail per-entry validation on the receiver).
    let mut registry = ComponentRegistry::new();
    for (id, meta) in canonical.expected_registry.iter() {
        registry.set(*id, meta.clone())?;
    }
    let mut registry_delta: TlsMapDelta<ComponentId, VLBytes> = TlsMapDelta::new();
    for (id, meta) in canonical.expected_registry.into_iter() {
        registry_delta = registry_delta.insert(id, VLBytes::new(meta.encode_to_vec()));
    }
    out.insert(
        ComponentId::COMPONENT_REGISTRY,
        registry_delta.tls_serialize_detached()?,
    );

    // Async GROUP_MEMBERSHIP: partition failed_installations by
    // owning inbox id via identity-update history.
    let group_membership_bytes =
        build_partitioned_group_membership(context, extensions, &canonical.membership_sequence_ids)
            .await?;
    out.insert(ComponentId::GROUP_MEMBERSHIP, group_membership_bytes);

    Ok(out)
}

/// Walk the legacy `GroupMembership.failed_installations` flat list,
/// query each member's installations from the identity-update store,
/// and bucket the failed installations into a `TlsMap<InboxId, VLBytes>`
/// of per-inbox `GroupMembershipEntryV1`.
async fn build_partitioned_group_membership<C: XmtpSharedContext>(
    context: &C,
    extensions: &Extensions<GroupContext>,
    sequence_ids: &BTreeMap<InboxId, u64>,
) -> Result<Vec<u8>, BootstrapSynthesisError> {
    use openmls::extensions::{Extension, UnknownExtension};
    let legacy_bytes = extensions
        .iter()
        .find_map(|extension| match extension {
            Extension::Unknown(
                xmtp_configuration::GROUP_MEMBERSHIP_EXTENSION_ID,
                UnknownExtension(data),
            ) => Some(data),
            _ => None,
        })
        .ok_or_else(|| {
            BootstrapSynthesisError::Common(CommonMigrationError::MissingGroupMembershipExtension)
        })?;
    let legacy_proto = GroupMembershipProto::decode(legacy_bytes.as_slice())?;

    let identity_updates = IdentityUpdates::new(context);
    let db = context.db();

    // installation_id -> owning inbox_id. HashMap is fine: we only
    // `.get()` it (no iteration), so its undefined order doesn't leak
    // into the serialized output.
    let mut install_owner: std::collections::HashMap<Vec<u8>, InboxId> =
        std::collections::HashMap::new();
    for (inbox_id, seq) in sequence_ids.iter() {
        let inbox_id_hex = inbox_id.to_hex();
        let signed_seq =
            i64::try_from(*seq).map_err(|_| BootstrapSynthesisError::SequenceIdOverflow {
                inbox_id: inbox_id_hex.clone(),
                sequence_id: *seq,
            })?;
        let state = identity_updates
            .get_association_state(&db, &inbox_id_hex, Some(signed_seq))
            .await
            .map_err(|source| BootstrapSynthesisError::IdentityUpdateLookup {
                inbox_id: inbox_id_hex.clone(),
                source,
            })?;
        for install_id in state.installation_ids() {
            install_owner.insert(install_id, *inbox_id);
        }
    }

    // Bucket the flat failed_installations list by owner; drop entries
    // whose owner can't be resolved. The bootstrap commit is produced
    // by one sender and byte-compared by receivers (no re-synthesis),
    // so a missed installation costs at most one retry — documented
    // on the proto as a hint-only field.
    let mut per_inbox_failed: BTreeMap<InboxId, Vec<Vec<u8>>> = BTreeMap::new();
    let mut dropped = 0usize;
    for fi in &legacy_proto.failed_installations {
        if let Some(owner) = install_owner.get(fi) {
            per_inbox_failed.entry(*owner).or_default().push(fi.clone());
        } else {
            dropped += 1;
        }
    }
    if dropped > 0 {
        let total = legacy_proto.failed_installations.len();
        // Drops above half the input usually indicate a systemic
        // identity-update lookup gap (e.g., the inbox of an installation
        // never made it into `sequence_ids`), not the rare "stray entry"
        // case the hint-only contract was designed for. Bump severity so
        // operators see it in dashboards even though it isn't
        // correctness-critical.
        if dropped * 2 > total {
            tracing::warn!(
                dropped,
                total,
                "bootstrap synthesis dropped a majority of failed_installation entries (hint only, but unusual)"
            );
        } else {
            tracing::info!(
                dropped,
                total,
                "bootstrap synthesis dropped unresolvable failed_installation entries (hint only, not correctness-critical)"
            );
        }
    }

    // Build the final per-inbox entries. Wraps each `V1` payload in
    // the `GroupMembershipEntry` envelope so the on-the-wire shape
    // matches what `decode_group_membership_delta` reads back —
    // forward-compat with future `Version` variants comes for free.
    let mut entries: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
    for (inbox_id, seq) in sequence_ids.iter() {
        let failed = per_inbox_failed.remove(inbox_id).unwrap_or_default();
        entries.insert(
            *inbox_id,
            GroupMembershipEntry {
                version: Some(GroupMembershipEntryVersion::V1(GroupMembershipEntryV1 {
                    sequence_id: *seq,
                    failed_installations: failed,
                })),
            },
        );
    }

    Ok(migration::encode_group_membership_delta(&entries)?)
}

/// Errors surfaced by [`stage_bootstrap_commit`].
#[derive(Debug, thiserror::Error)]
pub enum BootstrapCommitError<StorageError: std::error::Error> {
    #[error("commit create error: {0}")]
    CreateCommit(#[from] openmls::group::CreateCommitError),
    #[error("commit stage error: {0}")]
    StageCommit(#[from] openmls::group::CommitBuilderStageError<StorageError>),
    #[error("TLS codec error: {0}")]
    TlsCodec(#[from] tls_codec::Error),
    /// Caller invariant violated: `new_extensions` still carries one of
    /// the four legacy XMTP extensions that bootstrap is supposed to
    /// strip. Failing fast here keeps a malformed sender from publishing
    /// a commit that every honest receiver rejects.
    #[error("bootstrap precondition: new_extensions still carries legacy XMTP extension {0:#06x}")]
    LegacyExtensionPresent(u16),
    /// Caller invariant violated: `new_extensions`'s
    /// `RequiredCapabilities` doesn't list
    /// `ExtensionType::AppDataDictionary`. Every bootstrap commit
    /// MUST require AppDataDictionary so post-flip members can't skip
    /// the support check.
    #[error("bootstrap precondition: RequiredCapabilities doesn't list AppDataDictionary")]
    MissingAppDataDictionaryRequirement,
    /// Sender-side `apply_app_data_update_payload` rejected one of
    /// the synthesized component values when deriving dict bytes from
    /// wire bytes. Indicates a bug in synthesis (the wire bytes don't
    /// decode under the component's own apply rules) — fail loud here
    /// rather than ship a commit with sender/receiver dict divergence.
    #[error("bootstrap precondition: dict apply failed: {0}")]
    DictApply(#[from] super::component_source::ComponentSourceError),
}

/// Build and stage the bootstrap migration commit.
///
/// The commit bundles one `AppDataUpdate(component_id, Update(bytes))`
/// proposal per entry in `component_values` (sorted by ComponentId
/// ascending, enforced by `BTreeMap`) plus the GCE proposal carrying
/// `new_extensions`. `with_app_data_dictionary_updates` is populated
/// with the full set of dict writes so OpenMLS's
/// `apply_app_data_update_proposals` sees a matching bag.
///
/// The caller is responsible for computing `component_values` via the
/// async [`synthesize_initial_component_values`] and for building
/// `new_extensions` with:
/// - `MUTABLE_METADATA_EXTENSION_ID`, `GROUP_PERMISSIONS_EXTENSION_ID`,
///   `GROUP_MEMBERSHIP_EXTENSION_ID`, and the immutable metadata
///   extension (`ExtensionType::ImmutableMetadata`) removed from the
///   group context extensions AND from `RequiredCapabilities`.
/// - `ExtensionType::AppDataDictionary` added to `RequiredCapabilities`
///   so receivers must advertise support for the dict-carrying
///   standard MLS extension. The `AppDataDictionary` GCE itself is
///   populated by OpenMLS when the bundled `AppDataUpdate` proposals
///   apply during commit processing.
pub fn stage_bootstrap_commit<Provider: OpenMlsProvider>(
    mls_group: &mut OpenMlsGroup,
    provider: &Provider,
    signer: &impl openmls_traits::signatures::Signer,
    component_values: &BTreeMap<ComponentId, Vec<u8>>,
    new_extensions: Extensions<GroupContext>,
) -> Result<CommitMessageBundle, BootstrapCommitError<Provider::StorageError>> {
    use openmls::component::ComponentData;
    use openmls::extensions::Extension;
    use xmtp_configuration::{
        GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID,
        MUTABLE_METADATA_EXTENSION_ID,
    };

    // Precondition guard for the contract documented above. A malformed
    // `new_extensions` would build a commit that every honest receiver
    // rejects, so fail loud at the sender — a clear precondition error
    // beats an opaque downstream confirmation-tag mismatch.
    for ext in new_extensions.iter() {
        match ext {
            Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, _) => {
                return Err(BootstrapCommitError::LegacyExtensionPresent(
                    MUTABLE_METADATA_EXTENSION_ID,
                ));
            }
            Extension::Unknown(GROUP_PERMISSIONS_EXTENSION_ID, _) => {
                return Err(BootstrapCommitError::LegacyExtensionPresent(
                    GROUP_PERMISSIONS_EXTENSION_ID,
                ));
            }
            Extension::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID, _) => {
                return Err(BootstrapCommitError::LegacyExtensionPresent(
                    GROUP_MEMBERSHIP_EXTENSION_ID,
                ));
            }
            Extension::ImmutableMetadata(_) => {
                // OpenMLS-assigned IANA value for ExtensionType::ImmutableMetadata.
                return Err(BootstrapCommitError::LegacyExtensionPresent(0xf000));
            }
            _ => {}
        }
    }
    // RequiredCapabilities MUST list AppDataDictionary so post-flip
    // members can't add themselves without supporting the dict. Using
    // `check_proposals_enabled` (which detects the AppDataDictionary
    // GCE itself) wouldn't work here — openmls only adds the dict GCE
    // when the AppDataUpdate proposals apply during commit processing.
    use openmls::extensions::ExtensionType;
    let requires_app_data_dictionary = new_extensions
        .required_capabilities()
        .map(|rc| {
            rc.extension_types()
                .contains(&ExtensionType::AppDataDictionary)
        })
        .unwrap_or(false);
    if !requires_app_data_dictionary {
        return Err(BootstrapCommitError::MissingAppDataDictionaryRequirement);
    }

    // OpenMLS commit-ordering rule (draft-ietf-mls-extensions §4.7-7):
    // the GCE proposal MUST come before any AppDataUpdate proposals.
    let mut builder = mls_group
        .commit_builder()
        .propose_group_context_extensions(new_extensions)
        .map_err(BootstrapCommitError::CreateCommit)?;
    // Each component is encoded twice — once into the proposal payload
    // (`AppDataUpdateProposal::update` takes ownership), and once into
    // the dict updater below (`ComponentData::from_parts` also takes
    // ownership). Both consumers want owned bytes, and the caller passes
    // `&BTreeMap` (the closure in `mls_sync.rs` only sees a borrow), so
    // we clone here and consume on the second pass to keep the
    // sender/receiver byte-bag aligned for confirmation-tag agreement.
    // Component values are bounded (well-known set; largest is
    // GROUP_MEMBERSHIP scaling with member count) so the clones are not
    // a hot-path concern today.
    for (component_id, bytes) in component_values.iter() {
        builder = builder.add_proposal(Proposal::AppDataUpdate(Box::new(
            AppDataUpdateProposal::update(component_id.as_u16(), bytes.clone()),
        )));
    }

    let mut stage = builder.load_psks(provider.storage())?;

    // The wire payload (above) is a delta; the dict stores the
    // materialized state (a snapshot). Sender and receiver must
    // converge on byte-identical dict bytes — the OpenMLS path-
    // encryption AAD covers the post-commit GroupContext, which
    // embeds the serialized AppDataDictionary, so any sender/receiver
    // byte divergence here surfaces on the receiver as
    // `UnableToDecrypt` and the bootstrap commit is rejected.
    //
    // Single source of truth: route each component's wire bytes
    // through `apply_app_data_update_payload(id, wire, None, &reg)` to
    // derive the dict bytes. The receiver runs the same function over
    // the same wire bytes (with prior=None at bootstrap) and gets the
    // same output, so dict byte-equality is guaranteed by construction.
    //
    // The empty `ComponentRegistry` here is intentional: bootstrap
    // only writes well-known components, each of which has a per-id
    // `Component` impl, so the type-aware fallback branch in
    // `apply_app_data_update_payload` is never consulted.
    let empty_registry = xmtp_mls_common::app_data::component_registry::ComponentRegistry::new();
    let mut updater = stage.app_data_dictionary_updater();
    for (component_id, wire_bytes) in component_values.iter() {
        let dict_bytes = super::component_source::apply_app_data_update_payload(
            *component_id,
            wire_bytes,
            None,
            &empty_registry,
        )
        .map_err(BootstrapCommitError::DictApply)?;
        updater.set(ComponentData::from_parts(
            component_id.as_u16(),
            dict_bytes.into(),
        ));
    }
    stage.with_app_data_dictionary_updates(updater.changes());

    let bundle = stage
        .build(provider.rand(), provider.crypto(), signer, |_| true)?
        .stage_commit(provider)?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    //! Unit coverage for the sender-side bootstrap synthesis pipeline.
    //! End-to-end commit-staging (with confirmation-tag agreement)
    //! needs a real `OpenMlsGroup` and lives in integration tests.
    use super::*;
    use std::collections::HashMap;

    use openmls::extensions::{Extension, Extensions, Metadata, UnknownExtension};
    use xmtp_configuration::{
        GROUP_MEMBERSHIP_EXTENSION_ID, GROUP_PERMISSIONS_EXTENSION_ID,
        MUTABLE_METADATA_EXTENSION_ID,
    };
    use xmtp_mls_common::{
        app_data::component_id::ComponentId, group_metadata::GroupMetadata,
        group_mutable_metadata::GroupMutableMetadata,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembership as GroupMembershipProto, GroupMutablePermissionsV1,
        MembershipPolicy as MembershipPolicyProto,
        PermissionsUpdatePolicy as PermissionsUpdatePolicyProto, PolicySet as PolicySetProto,
        membership_policy::{BasePolicy as MembershipBase, Kind as MembershipKind},
        permissions_update_policy::{Kind as PermissionsKind, PermissionsBasePolicy},
    };

    use xmtp_db::identity_update::QueryIdentityUpdates;

    use crate::{identity_updates::load_identity_updates, tester};

    /// Unwrap a `GroupMembershipEntry` envelope to its inner `V1`
    /// payload — the only legal shape today (decode rejects `None`).
    /// Panics on any other variant; tests fail loudly instead of
    /// silently skipping assertions.
    fn unwrap_v1(entry: &GroupMembershipEntry) -> &GroupMembershipEntryV1 {
        match entry.version.as_ref().expect("entry missing version") {
            GroupMembershipEntryVersion::V1(v1) => v1,
        }
    }

    fn minimal_policy_set() -> PolicySetProto {
        let allow = MembershipPolicyProto {
            kind: Some(MembershipKind::Base(MembershipBase::Allow as i32)),
        };
        let admin_only = PermissionsUpdatePolicyProto {
            kind: Some(PermissionsKind::Base(
                PermissionsBasePolicy::AllowIfAdmin as i32,
            )),
        };
        let super_admin_only = PermissionsUpdatePolicyProto {
            kind: Some(PermissionsKind::Base(
                PermissionsBasePolicy::AllowIfSuperAdmin as i32,
            )),
        };
        PolicySetProto {
            add_member_policy: Some(allow.clone()),
            remove_member_policy: Some(allow),
            update_metadata_policy: HashMap::new(),
            add_admin_policy: Some(admin_only.clone()),
            remove_admin_policy: Some(admin_only),
            update_permissions_policy: Some(super_admin_only),
        }
    }

    fn build_test_extensions(
        gmm: GroupMutableMetadata,
        membership: GroupMembershipProto,
        metadata: GroupMetadata,
    ) -> Extensions<openmls::group::GroupContext> {
        let gmm_bytes: Vec<u8> = gmm.try_into().unwrap();
        let permissions_bytes = GroupMutablePermissionsV1 {
            policies: Some(minimal_policy_set()),
        }
        .encode_to_vec();
        let membership_bytes = membership.encode_to_vec();
        let metadata_bytes: Vec<u8> = metadata.try_into().unwrap();
        Extensions::from_vec(vec![
            Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, UnknownExtension(gmm_bytes)),
            Extension::Unknown(
                GROUP_PERMISSIONS_EXTENSION_ID,
                UnknownExtension(permissions_bytes),
            ),
            Extension::Unknown(
                GROUP_MEMBERSHIP_EXTENSION_ID,
                UnknownExtension(membership_bytes),
            ),
            Extension::ImmutableMetadata(Metadata::new(metadata_bytes)),
        ])
        .expect("valid group-context extension set")
    }

    fn default_gmm() -> GroupMutableMetadata {
        GroupMutableMetadata::new(HashMap::new(), Vec::new(), Vec::new())
    }

    fn default_metadata(creator_inbox_id: String) -> GroupMetadata {
        GroupMetadata::new(
            xmtp_db::group::ConversationType::Group,
            creator_inbox_id,
            None,
            None,
        )
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn synthesize_initial_component_values_is_deterministic() {
        // Same inputs → bit-identical output bytes. Load-bearing
        // invariant for cross-peer byte-compare validation: a regression
        // makes honest receivers reject every bootstrap commit with a
        // confirmation-tag mismatch.
        tester!(alix);
        tester!(bo);
        load_identity_updates(
            alix.context.api(),
            &alix.context.db(),
            &[alix.inbox_id(), bo.inbox_id()],
        )
        .await?;
        // GROUP_MEMBERSHIP sequence_ids are no longer synthetic: the
        // synthesizer pins the per-inbox association-state lookup to
        // the GMM's sequence_id, so the value must match an existing
        // identity-update record.
        let alix_seq = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let bo_seq = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(bo.inbox_id())? as u64;

        let mut members = HashMap::new();
        members.insert(alix.inbox_id().to_string(), alix_seq);
        members.insert(bo.inbox_id().to_string(), bo_seq);
        let exts = build_test_extensions(
            default_gmm(),
            GroupMembershipProto {
                members,
                failed_installations: vec![],
            },
            default_metadata(alix.inbox_id().to_string()),
        );

        let a = synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;
        let b = synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;
        assert_eq!(a, b, "bootstrap synthesis must be byte-stable");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn synthesize_partitions_failed_installations_by_owner() {
        // Two inboxes each own one installation; the flat
        // failed_installations list should partition so each
        // per-inbox `GroupMembershipEntryV1` carries only its own.
        tester!(alix);
        tester!(bo);
        load_identity_updates(
            alix.context.api(),
            &alix.context.db(),
            &[alix.inbox_id(), bo.inbox_id()],
        )
        .await?;

        let alix_install = alix.installation_id.to_vec();
        let bo_install = bo.installation_id.to_vec();
        let alix_inbox = InboxId::from_hex(alix.inbox_id())?;
        let bo_inbox = InboxId::from_hex(bo.inbox_id())?;
        let alix_seq = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let bo_seq = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(bo.inbox_id())? as u64;

        let mut members = HashMap::new();
        members.insert(alix.inbox_id().to_string(), alix_seq);
        members.insert(bo.inbox_id().to_string(), bo_seq);
        let exts = build_test_extensions(
            default_gmm(),
            GroupMembershipProto {
                members,
                failed_installations: vec![alix_install.clone(), bo_install.clone()],
            },
            default_metadata(alix.inbox_id().to_string()),
        );

        let out = synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;

        let bytes = out.get(&ComponentId::GROUP_MEMBERSHIP).unwrap();
        let decoded =
            xmtp_mls_common::app_data::migration::decode_group_membership_delta(bytes).unwrap();
        assert_eq!(decoded.len(), 2);
        let v1_a = unwrap_v1(decoded.get(&alix_inbox).unwrap());
        let v1_b = unwrap_v1(decoded.get(&bo_inbox).unwrap());
        assert_eq!(v1_a.sequence_id, alix_seq);
        assert_eq!(v1_a.failed_installations, vec![alix_install]);
        assert_eq!(v1_b.sequence_id, bo_seq);
        assert_eq!(v1_b.failed_installations, vec![bo_install]);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn synthesize_drops_unresolvable_failed_installations() {
        // `failed_installations` is hint-only; an entry whose owning
        // inbox isn't in the lookup is dropped silently.
        tester!(alix);
        load_identity_updates(alix.context.api(), &alix.context.db(), &[alix.inbox_id()]).await?;

        let alix_inbox = InboxId::from_hex(alix.inbox_id())?;
        let alix_seq = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let orphan_install = vec![0xDE; 32]; // not owned by any inbox

        let mut members = HashMap::new();
        members.insert(alix.inbox_id().to_string(), alix_seq);
        let exts = build_test_extensions(
            default_gmm(),
            GroupMembershipProto {
                members,
                failed_installations: vec![orphan_install],
            },
            default_metadata(alix.inbox_id().to_string()),
        );

        let out = synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;
        let bytes = out.get(&ComponentId::GROUP_MEMBERSHIP).unwrap();
        let decoded =
            xmtp_mls_common::app_data::migration::decode_group_membership_delta(bytes).unwrap();
        let v1 = unwrap_v1(decoded.get(&alix_inbox).unwrap());
        assert_eq!(
            v1.failed_installations,
            Vec::<Vec<u8>>::new(),
            "orphan failed_installation must be dropped"
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn synthesize_emits_expected_component_keys() {
        // Every well-known non-optional component shows up. DM_MEMBERS
        // / ONESHOT_MESSAGE are gated on presence — a plain non-DM,
        // non-oneshot group has neither.
        tester!(alix);
        let exts = build_test_extensions(
            default_gmm(),
            GroupMembershipProto::default(),
            default_metadata(alix.inbox_id().to_string()),
        );
        let out = synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;
        assert!(out.contains_key(&ComponentId::COMPONENT_REGISTRY));
        assert!(out.contains_key(&ComponentId::GROUP_MEMBERSHIP));
        assert!(out.contains_key(&ComponentId::ADMIN_LIST));
        assert!(out.contains_key(&ComponentId::SUPER_ADMIN_LIST));
        assert!(out.contains_key(&ComponentId::CREATOR_INBOX_ID));
        assert!(out.contains_key(&ComponentId::CONVERSATION_TYPE));
        assert!(!out.contains_key(&ComponentId::DM_MEMBERS));
        assert!(!out.contains_key(&ComponentId::ONESHOT_MESSAGE));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn synthesize_partitions_against_snapshotted_view_not_latest() {
        // Two members, each with multiple installations; the group's
        // recorded sequence_id captures a snapshot of identity history.
        // After the snapshot, new installations are added to each
        // member's chain WITHOUT a group-side resync. The bootstrap
        // synthesizer pins per-inbox association-state lookups to the
        // group's snapshotted sequence_id, so:
        //  - installations live at the snapshot are partitioned to their
        //    owning inbox
        //  - installations added AFTER the snapshot have no owner at the
        //    snapshotted state and are dropped from `failed_installations`
        //  - bogus install ids that never belonged to anyone are dropped
        //  - cross-inbox leakage doesn't happen
        //  - re-running synthesis with the same input is byte-stable
        //
        // This is the exact "group has not refreshed yet, identity moved
        // on" scenario every honest receiver will hit during a real
        // bootstrap rollout.
        use std::collections::BTreeSet;

        tester!(alix);
        let alix2 = alix.new_installation().await;
        tester!(bo);
        let bo2 = bo.new_installation().await;

        // Refresh once so the synthesizer's local-DB read sees both
        // inboxes' chains up through the second installation.
        load_identity_updates(
            alix.context.api(),
            &alix.context.db(),
            &[alix.inbox_id(), bo.inbox_id()],
        )
        .await?;

        // Snapshot the group's view of each inbox here. The synthetic
        // pre-flip GMM below uses these as its sequence_ids, so the
        // synthesizer will pin its per-inbox lookups to this point in
        // each chain.
        let alix_snapshot_seq =
            alix.context
                .db()
                .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let bo_snapshot_seq = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(bo.inbox_id())? as u64;

        let alix1_install = alix.installation_id.to_vec();
        let alix2_install = alix2.installation_id.to_vec();
        let bo1_install = bo.installation_id.to_vec();
        let bo2_install = bo2.installation_id.to_vec();

        // After the snapshot: each chain extends. The group has NOT
        // resynced, so its recorded sequence_id stays put.
        let alix3 = alix.new_installation().await;
        let bo3 = bo.new_installation().await;
        let alix3_install = alix3.installation_id.to_vec();
        let bo3_install = bo3.installation_id.to_vec();

        // Refresh local DB so post-snapshot updates are visible — the
        // point of this test is that the synthesizer DOESN'T use them
        // because it queries at the snapshotted seq_id, not latest.
        load_identity_updates(
            alix.context.api(),
            &alix.context.db(),
            &[alix.inbox_id(), bo.inbox_id()],
        )
        .await?;

        let alix_inbox = InboxId::from_hex(alix.inbox_id())?;
        let bo_inbox = InboxId::from_hex(bo.inbox_id())?;

        // Bogus install id that has never belonged to any inbox — the
        // legacy `failed_installations` proto is permissive enough that
        // a malicious or buggy prior commit could have stuffed garbage
        // in here.
        let bogus_install = vec![0xCA; 32];

        let mut members = HashMap::new();
        members.insert(alix.inbox_id().to_string(), alix_snapshot_seq);
        members.insert(bo.inbox_id().to_string(), bo_snapshot_seq);
        let exts = build_test_extensions(
            default_gmm(),
            GroupMembershipProto {
                members,
                failed_installations: vec![
                    alix1_install.clone(),
                    alix2_install.clone(),
                    alix3_install.clone(),
                    bo1_install.clone(),
                    bo2_install.clone(),
                    bo3_install.clone(),
                    bogus_install.clone(),
                ],
            },
            default_metadata(alix.inbox_id().to_string()),
        );

        let out = synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;
        let bytes = out.get(&ComponentId::GROUP_MEMBERSHIP).unwrap();
        let decoded =
            xmtp_mls_common::app_data::migration::decode_group_membership_delta(bytes).unwrap();

        let alix_v1 = unwrap_v1(decoded.get(&alix_inbox).unwrap());
        let bo_v1 = unwrap_v1(decoded.get(&bo_inbox).unwrap());

        assert_eq!(alix_v1.sequence_id, alix_snapshot_seq);
        assert_eq!(bo_v1.sequence_id, bo_snapshot_seq);

        let alix_failed: BTreeSet<Vec<u8>> = alix_v1.failed_installations.iter().cloned().collect();
        let bo_failed: BTreeSet<Vec<u8>> = bo_v1.failed_installations.iter().cloned().collect();

        // Snapshot-visible installations partition to their owner.
        assert!(alix_failed.contains(&alix1_install));
        assert!(alix_failed.contains(&alix2_install));
        assert!(bo_failed.contains(&bo1_install));
        assert!(bo_failed.contains(&bo2_install));

        // Post-snapshot installations have no owner at the snapshotted
        // sequence_id and are dropped.
        assert!(!alix_failed.contains(&alix3_install));
        assert!(!bo_failed.contains(&alix3_install));
        assert!(!alix_failed.contains(&bo3_install));
        assert!(!bo_failed.contains(&bo3_install));

        // Bogus install id is dropped — never belonged to any inbox.
        assert!(!alix_failed.contains(&bogus_install));
        assert!(!bo_failed.contains(&bogus_install));

        // No cross-inbox leakage.
        assert!(!alix_failed.contains(&bo1_install));
        assert!(!alix_failed.contains(&bo2_install));
        assert!(!bo_failed.contains(&alix1_install));
        assert!(!bo_failed.contains(&alix2_install));

        // Determinism: byte-identical output across calls. Validation
        // peers byte-compare against this, so any non-determinism here
        // would make every honest receiver reject the bootstrap commit.
        let again =
            synthesize_initial_component_values_from_extensions(&alix.context, &exts).await?;
        assert_eq!(out, again, "bootstrap synthesis must be byte-stable");
    }
}
