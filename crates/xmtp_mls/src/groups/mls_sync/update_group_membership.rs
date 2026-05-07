use super::*;
use crate::groups::group_membership::GroupMembership;
use crate::groups::{
    GroupError, build_group_membership_extension,
    intents::{PostCommitAction, UpdateGroupMembershipIntentData},
    update_required_capabilities_for_proposals,
    validated_commit::extract_group_membership,
};
use openmls::{
    extensions::Extensions,
    group::GroupContext,
    messages::proposals::{AppDataUpdateOperation, Proposal},
    prelude::{LeafNodeIndex, MlsGroup as OpenMlsGroup, tls_codec::Serialize},
};
use openmls_traits::signatures::Signer;
use prost::Message;
use tls_codec::VLBytes;
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId, components::tls_map_components::GroupMembershipComponent,
        typed::Component,
    },
    inbox_id::InboxId,
    tls_map::TlsMapDelta,
};
use xmtp_proto::xmtp::mls::message_contents::{GroupMembershipEntry, group_membership_entry};

/// Build the wire-level `TlsMapDelta<InboxId, VLBytes>` payload for an
/// `AppDataUpdate(GROUP_MEMBERSHIP)` proposal from the diff between
/// the old and new `GroupMembership` view. Shared between the commit-
/// bundling `apply_update_group_membership_intent` flow and the
/// propose-by-reference `IntentKind::ProposeMemberUpdate` flow so
/// both emit byte-identical payloads for the same diff.
///
/// One mutation per affected inbox:
/// - `Insert(inbox_id, encode(V1 { sequence_id, failed_installations: [] }))`
///   for inboxes added.
/// - `Update(inbox_id, encode(V1 { ... }))` for inboxes whose
///   `sequence_id` changed.
/// - `Delete(inbox_id)` for inboxes removed.
///
/// `failed_installations` is left empty here — per the proto comment
/// it's a sender-authoritative retry-suppression hint and the
/// per-inbox partitioning happens at bootstrap. Steady-state membership
/// updates intentionally don't propagate failed_installations changes
/// over the AppData path; the worst case is a slightly noisier retry
/// loop. Future enhancement once a clearer attribution path exists.
pub(crate) fn build_group_membership_app_data_payload(
    old: &GroupMembership,
    new: &GroupMembership,
) -> Result<Vec<u8>, GroupError> {
    let mut delta = TlsMapDelta::<InboxId, VLBytes>::new();

    // Inserts and updates: walk new.members, classify against old.
    for (inbox_id_str, &sequence_id) in new.members.iter() {
        let entry = encode_membership_entry(sequence_id)?;
        match old.members.get(inbox_id_str) {
            None => {
                // New inbox: Insert.
                let inbox_id = InboxId::from_hex(inbox_id_str)
                    .map_err(|e| GroupError::ComponentSource(e.into()))?;
                delta = delta.insert(inbox_id, VLBytes::new(entry));
            }
            Some(&old_seq) if old_seq != sequence_id => {
                // Existing inbox with bumped sequence_id: Update.
                let inbox_id = InboxId::from_hex(inbox_id_str)
                    .map_err(|e| GroupError::ComponentSource(e.into()))?;
                delta = delta.update(inbox_id, VLBytes::new(entry));
            }
            _ => {
                // Same sequence_id, no change for this inbox.
            }
        }
    }

    // Deletes: in old but not new.
    for inbox_id_str in old.members.keys() {
        if !new.members.contains_key(inbox_id_str) {
            let inbox_id = InboxId::from_hex(inbox_id_str)
                .map_err(|e| GroupError::ComponentSource(e.into()))?;
            delta = delta.delete(inbox_id);
        }
    }

    <GroupMembershipComponent as Component>::encode_mutation(&delta).map_err(|e| {
        GroupError::ComponentSource(
            crate::groups::app_data::component_source::ComponentSourceError::from(e),
        )
    })
}

/// Encode a per-inbox `GroupMembershipEntry::V1` value with the given
/// `sequence_id` and an empty `failed_installations` list.
fn encode_membership_entry(sequence_id: u64) -> Result<Vec<u8>, GroupError> {
    let entry = GroupMembershipEntry {
        version: Some(group_membership_entry::Version::V1(
            group_membership_entry::V1 {
                sequence_id,
                failed_installations: vec![],
            },
        )),
    };
    Ok(entry.encode_to_vec())
}

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[tracing::instrument(level = "trace", skip_all)]
pub(crate) async fn apply_update_group_membership_intent(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError> {
    let extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);
    let membership_diff = old_group_membership.diff(&new_group_membership);

    let changes_with_kps = calculate_membership_changes_with_keypackages(
        context,
        openmls_group.group_id().as_slice(),
        &new_group_membership,
        &old_group_membership,
    )
    .await?;
    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &changes_with_kps.removed_installations);

    if leaf_nodes_to_remove.contains(&openmls_group.own_leaf_index()) {
        tracing::info!("Cannot remove own leaf node");
        return Ok(None);
    }

    if leaf_nodes_to_remove.is_empty()
        && changes_with_kps.new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
    {
        return Ok(None);
    }

    // Detect whether this is a migrated group. On migrated groups the
    // legacy `GROUP_MEMBERSHIP_EXTENSION_ID` is gone and the
    // membership lives in the AppData dictionary; the membership
    // delta travels as an `AppDataUpdate(GROUP_MEMBERSHIP)` proposal
    // rather than a GCE proposal updating the legacy extension.
    //
    // Uses the canonical `is_migrated_extensions` predicate (presence
    // of the `COMPONENT_REGISTRY` entry written by the bootstrap
    // commit) so we agree with every other send/receive/validate gate
    // in the migration.
    let is_migrated = super::super::app_data::is_migrated_extensions(openmls_group.extensions());

    // Update the extensions to have the new GroupMembership.
    // On migrated groups we deliberately skip writing the legacy
    // `GROUP_MEMBERSHIP_EXTENSION_ID` — bootstrap removed it and the
    // AppDataUpdate proposal carries the delta instead.
    let mut new_extensions = extensions.clone();
    if !is_migrated {
        new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership))?;
    }

    // Check if proposals need to be disabled due to new members not supporting them
    let proposal_ext_type =
        openmls::prelude::ExtensionType::Unknown(xmtp_configuration::PROPOSAL_SUPPORT_EXTENSION_ID);
    let mut proposals_currently_enabled = openmls_group
        .extensions()
        .iter()
        .any(|ext| ext.extension_type() == proposal_ext_type);
    let mut downgrade_to_legacy = false;
    if proposals_currently_enabled && !changes_with_kps.new_key_packages.is_empty() {
        let new_members_support_proposals = changes_with_kps.new_key_packages.iter().all(|kp| {
            kp.leaf_node()
                .capabilities()
                .extensions()
                .contains(&proposal_ext_type)
        });
        // TODO: D14N Hammer
        if !new_members_support_proposals {
            tracing::info!("Disabling proposals: new members don't support proposal extension");
            new_extensions.remove(proposal_ext_type);
            update_required_capabilities_for_proposals(&mut new_extensions, false)?;
            proposals_currently_enabled = false;
            // If this group was migrated, downgrading to legacy
            // means we need to re-add the legacy GROUP_MEMBERSHIP
            // extension since the AppDataUpdate path is no longer
            // available. (This is a rare rollback scenario; covered
            // here defensively so the subsequent direct-commit path
            // doesn't lose membership state.)
            if is_migrated {
                new_extensions
                    .add_or_replace(build_group_membership_extension(&new_group_membership))?;
                downgrade_to_legacy = true;
            }
        }
    }

    if proposals_currently_enabled {
        // Batched proposal path: proposals + (AppDataUpdate or GCE) + commit in one publish
        let app_data_payload = if is_migrated {
            Some(build_group_membership_app_data_payload(
                &old_group_membership,
                &new_group_membership,
            )?)
        } else {
            None
        };
        let publish_intent_data = compute_publish_data_for_proposal_based_update(
            context,
            openmls_group,
            changes_with_kps.new_installations,
            changes_with_kps.new_key_packages,
            leaf_nodes_to_remove,
            new_extensions,
            app_data_payload,
            signer,
        )
        .await?;
        let _ = downgrade_to_legacy; // marker so future logic can branch on it; currently unused
        Ok(Some(publish_intent_data))
    } else {
        // Direct commit path (no proposals)
        let publish_intent_data = compute_publish_data_for_group_membership_update(
            context,
            openmls_group,
            changes_with_kps.new_installations,
            changes_with_kps.new_key_packages,
            leaf_nodes_to_remove,
            new_extensions,
            signer,
        )
        .await?;
        Ok(Some(publish_intent_data))
    }
}

#[tracing::instrument(level = "trace", skip_all)]
async fn compute_publish_data_for_group_membership_update(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    installations_to_add: Vec<Installation>,
    key_packages_to_add: Vec<KeyPackage>,
    leaf_nodes_to_remove: Vec<LeafNodeIndex>,
    new_extensions: Extensions<GroupContext>,
    signer: impl Signer,
) -> Result<PublishIntentData, GroupError> {
    // Use savepoint pattern to create commit without persisting state
    let ((commit, maybe_welcome_message, _), staged_commit, group_epoch) =
        generate_commit_with_rollback(context.mls_storage(), openmls_group, |group, provider| {
            group.update_group_membership(
                provider,
                &signer,
                &key_packages_to_add,
                &leaf_nodes_to_remove,
                new_extensions,
            )
        })?;

    let staged_commit = staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;

    let post_commit_action = match maybe_welcome_message {
        Some(welcome_message) => Some(PostCommitAction::from_welcome(
            welcome_message,
            installations_to_add,
        )?),
        None => None,
    };

    Ok(PublishIntentData {
        payloads_to_publish: vec![commit.tls_serialize_detached()?],
        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
        staged_commit: Some(staged_commit),
        should_send_push_notification: false,
        group_epoch,
    })
}

/// Like `compute_publish_data_for_group_membership_update`, but instead of creating a direct
/// commit via `update_group_membership`, creates MLS proposals (Add/Remove + GCE *or*
/// AppDataUpdate) and a commit that references them. All payloads are returned together so
/// they can be published in a single `send_group_messages` call, eliminating multiple network
/// roundtrips.
///
/// `app_data_membership_payload`:
/// - `Some(bytes)` on migrated groups — emit an
///   `AppDataUpdate(GROUP_MEMBERSHIP, Update(bytes))` proposal-by-reference.
///   `bytes` is the wire-encoded `TlsMapDelta<InboxId, VLBytes>` from
///   `build_group_membership_app_data_payload`.
/// - `None` on unmigrated groups — fall back to a GCE proposal that
///   updates the legacy `GROUP_MEMBERSHIP_EXTENSION_ID` extension.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(level = "trace", skip_all)]
async fn compute_publish_data_for_proposal_based_update(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    installations_to_add: Vec<Installation>,
    key_packages_to_add: Vec<KeyPackage>,
    leaf_nodes_to_remove: Vec<LeafNodeIndex>,
    new_extensions: Extensions<GroupContext>,
    app_data_membership_payload: Option<Vec<u8>>,
    signer: impl Signer,
) -> Result<PublishIntentData, GroupError> {
    let is_migrated_path = app_data_membership_payload.is_some();
    // Only used on the legacy path. On the migrated path the
    // membership delta travels via the AppDataUpdate proposal so the
    // GCE-changed check is irrelevant.
    let extensions_changed = if is_migrated_path {
        false
    } else {
        let current_membership = extract_group_membership(openmls_group.extensions())?;
        let new_membership_check = extract_group_membership(&new_extensions)?;
        current_membership != new_membership_check
    };
    let new_extensions_for_filter = new_extensions.clone();

    let ((proposal_payloads, bundle), staged_commit, group_epoch) =
        generate_commit_with_rollback(context.mls_storage(), openmls_group, |group, provider| {
            let mut proposal_payloads: Vec<Vec<u8>> = Vec::new();

            // 1. Create Add proposals
            for kp in &key_packages_to_add {
                let (msg, _) = group
                    .propose_add_member(provider, &signer, kp)
                    .map_err(GroupError::ProposeAddMember)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            }

            // 2. Create Remove proposals
            for &leaf_index in &leaf_nodes_to_remove {
                let (msg, _) = group
                    .propose_remove_member(provider, &signer, leaf_index)
                    .map_err(GroupError::ProposeRemoveMember)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            }

            // 3a. Migrated: emit AppDataUpdate(GROUP_MEMBERSHIP) proposal carrying the delta.
            //     Receivers walk the proposal alongside the Add/Remove proposals
            //     and apply the dict update via `accumulate_app_data_updates`.
            if let Some(payload) = &app_data_membership_payload {
                let (msg, _) = group
                    .propose_app_data_update(
                        provider,
                        &signer,
                        ComponentId::GROUP_MEMBERSHIP.as_u16(),
                        AppDataUpdateOperation::Update(payload.clone().into()),
                    )
                    .map_err(GroupError::Proposal)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            // 3b. Legacy: GCE proposal updating GROUP_MEMBERSHIP_EXTENSION_ID
            //     (only when the membership actually changed).
            } else if extensions_changed {
                let (msg, _) = group
                    .propose_group_context_extensions(provider, new_extensions.clone(), &signer)
                    .map_err(GroupError::Proposal)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            }

            // 4. Create commit consuming all proposals (including the ones just created).
            //    On migrated groups, also pre-compute the dict updates
            //    so the commit's confirmation tag agrees with what the
            //    receiver will compute via its own AppDataUpdate apply path.
            let new_membership = extract_group_membership(&new_extensions_for_filter).ok();
            let app_data_updates = if is_migrated_path {
                Some(crate::groups::app_data::pending_app_data_updates(group)?)
            } else {
                None
            };
            let mut stage = group
                .commit_builder()
                .consume_proposal_store(true)
                .load_psks(provider.storage())
                .map_err(CommitToPendingProposalsError::from)?;
            if let Some(Some(updates)) = app_data_updates {
                stage.with_app_data_dictionary_updates(Some(updates));
            }
            let bundle = stage
                .build(provider.rand(), provider.crypto(), &signer, |qp| {
                    match qp.proposal() {
                        // Always filter GCEs against expected membership.
                        // Accept only if it matches our new_membership; reject stale ones.
                        // Compare the full GroupMembership (members + failed_installations).
                        // On migrated groups `extract_group_membership(new_extensions)`
                        // can't extract (no legacy ext present); we conservatively
                        // accept all GCEs in that case since the membership is
                        // delivered via the AppDataUpdate proposal instead.
                        Proposal::GroupContextExtensions(gce) => match &new_membership {
                            Some(expected) => extract_group_membership(gce.extensions())
                                .map(|m| &m == expected)
                                .unwrap_or(false),
                            None => true,
                        },
                        _ => true,
                    }
                })
                .map_err(CommitToPendingProposalsError::from)?
                .stage_commit(provider)
                .map_err(CommitToPendingProposalsError::from)?;

            Ok::<_, GroupError>((proposal_payloads, bundle))
        })?;

    let staged_commit = staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;
    let (commit, maybe_welcome_message, _) = bundle.into_messages();

    // Build all payloads with commit last (for intent hash matching)
    let mut payloads_to_publish = proposal_payloads;
    // There is currently no feasible way to include dependencies on the previous payloads for the icebox.
    // We may want to revisit this in the future by allowing something like `originator_id = same_as_message` and `sequence_id = this_sequence_id - n`.
    payloads_to_publish.push(commit.tls_serialize_detached()?);

    let post_commit_action = match maybe_welcome_message {
        Some(welcome_message) => Some(PostCommitAction::from_welcome(
            welcome_message,
            installations_to_add,
        )?),
        None => None,
    };

    Ok(PublishIntentData {
        payloads_to_publish,
        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
        staged_commit: Some(staged_commit),
        should_send_push_notification: false,
        group_epoch,
    })
}

#[tracing::instrument(level = "trace", skip_all)]
pub(crate) async fn apply_readd_installations_intent(
    context: &impl XmtpSharedContext,
    openmls_group: &mut OpenMlsGroup,
    intent_data: ReaddInstallationsIntentData,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError> {
    let readded_installations: HashSet<Vec<u8>> =
        intent_data.readded_installations.into_iter().collect();

    // Filter out installations not in the ratchet tree. Do not readd installations:
    // 1. That have since been removed
    // 2. Are in the failed installations list (these should be retried by group members some other way)
    let mut installations_to_readd = HashSet::new();
    let mut leaf_indices_to_remove = Vec::new();
    for member in openmls_group.members() {
        if readded_installations.contains(&member.signature_key)
            && member.index != openmls_group.own_leaf_index()
        {
            installations_to_readd.insert(member.signature_key);
            leaf_indices_to_remove.push(member.index);
        }
    }

    let mut installations_to_welcome = Vec::new();
    let mut key_packages_to_welcome = Vec::new();
    let mut failed_installations = Vec::new();
    get_keypackages_for_installation_ids(
        context,
        installations_to_readd,
        &mut installations_to_welcome,
        &mut key_packages_to_welcome,
        &mut failed_installations,
    )
    .await?;

    // Update the group membership extension to reflect any failed installations
    let extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let failed_installations: HashSet<Vec<u8>> = old_group_membership
        .failed_installations
        .clone()
        .into_iter()
        .chain(failed_installations)
        .collect();
    let new_group_membership = GroupMembership {
        members: old_group_membership.members.clone(),
        failed_installations: failed_installations.into_iter().collect(),
    };
    let mut new_extensions = extensions.clone();
    new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership))?;

    let publish_intent_data = compute_publish_data_for_group_membership_update(
        context,
        openmls_group,
        installations_to_welcome,
        key_packages_to_welcome,
        leaf_indices_to_remove,
        new_extensions,
        signer,
    )
    .await?;

    Ok(Some(publish_intent_data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    use crate::{
        groups::{
            build_group_config, build_mutable_metadata_extension_default,
            build_mutable_permissions_extension, build_protected_metadata_extension,
            build_starting_group_membership_extension,
        },
        identity::create_credential,
        test::mock::{NewMockContext, context},
    };
    use openmls::{group::MlsGroupCreateConfig, prelude::CredentialWithKey};
    use rstest::*;
    use xmtp_cryptography::XmtpInstallationCredential;
    use xmtp_cryptography::configuration::CIPHERSUITE;
    use xmtp_db::mock::MockDbQuery;

    fn generate_config(
        creator_inbox: &str,
        members: &[&str],
    ) -> Result<MlsGroupCreateConfig, GroupError> {
        let mut membership = GroupMembership::new();
        membership.add(creator_inbox.to_string(), 0);
        members
            .iter()
            .for_each(|m| membership.add(m.to_string(), 0));
        let _group_membership = build_group_membership_extension(&membership);
        let protected_metadata =
            build_protected_metadata_extension(creator_inbox, ConversationType::Group, None)?;
        let mutable_metadata =
            build_mutable_metadata_extension_default(creator_inbox, Default::default())?;
        let group_membership = build_starting_group_membership_extension(creator_inbox, 0);
        let mutable_permissions = build_mutable_permissions_extension(Default::default())?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permissions,
        )?;
        Ok(group_config)
    }

    #[rstest]
    #[xmtp_common::test]
    #[allow(clippy::readonly_write_lock, clippy::await_holding_lock)]
    async fn applies_group_membership_intent(mut context: NewMockContext) {
        let mut credentials = HashMap::new();
        let installation_key = XmtpInstallationCredential::new();
        let key_pair = openmls_basic_credential::SignatureKeyPair::from(installation_key.clone());
        key_pair.store(&context.mls_storage).unwrap();
        let signature_key = installation_key.clone().into();
        let credential = CredentialWithKey {
            credential: create_credential("alice").unwrap(),
            signature_key,
        };
        credentials.insert(CIPHERSUITE, credential);
        // create a mocked, MLS client + group using openmls test framework
        let client = openmls::test_utils::test_framework::client::Client::<_> {
            identity: b"alice".to_vec(),
            credentials,
            provider: XmtpOpenMlsProviderRef::new(&context.mls_storage),
            groups: RwLock::new(HashMap::new()),
        };
        let config = generate_config("alice", &["bob", "caro", "eve"]).unwrap();
        let id = client.create_group(config, CIPHERSUITE).unwrap();
        let installation = XmtpInstallationCredential::new();

        let db_calls = || {
            let mut mock_db = MockDbQuery::new();
            mock_db
                .expect_get_latest_sequence_id()
                .returning(|_ids| Ok(HashMap::new()));
            mock_db
        };
        context.store.expect_db().returning(db_calls);

        let mut groups = client.groups.write().unwrap();
        let g = groups.get_mut(&id).unwrap();

        // once context is in an arc, can no longer set expectations
        let context = Arc::new(context);
        let intent = apply_update_group_membership_intent(
            context.as_ref(),
            g,
            UpdateGroupMembershipIntentData {
                membership_updates: HashMap::new(),
                removed_members: Vec::new(),
                failed_installations: Vec::new(),
            },
            installation,
        )
        .await
        .unwrap();
        assert!(intent.is_none());
    }
}
