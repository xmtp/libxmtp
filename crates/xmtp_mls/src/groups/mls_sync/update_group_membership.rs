use super::*;
use crate::groups::group_membership::GroupMembership;
use crate::groups::{
    GroupError,
    intents::{PostCommitAction, UpdateGroupMembershipIntentData},
    validated_commit::extract_group_membership,
};
use crate::identity::parse_credential;
use openmls::{
    credentials::BasicCredential,
    key_packages::KeyPackage,
    messages::proposals::AppDataUpdateOperation,
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

/// Inbox ids that received at least one Add proposal in this commit.
pub(crate) fn inbox_ids_from_new_key_packages(
    new_key_packages: &[KeyPackage],
) -> std::collections::HashSet<String> {
    new_key_packages
        .iter()
        .filter_map(|kp| {
            let credential = match BasicCredential::try_from(kp.leaf_node().credential().clone()) {
                Ok(credential) => credential,
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to decode key package leaf credential");
                    return None;
                }
            };
            match parse_credential(credential.identity()) {
                Ok(inbox_id) => Some(inbox_id),
                Err(e) => {
                    tracing::warn!(
                        error = ?e,
                        "failed to parse inbox id from key package credential; \
                         skipping inbox for phantom-member verification"
                    );
                    None
                }
            }
        })
        .collect()
}

/// Build the wire-level `TlsMapDelta<InboxId, VLBytes>` payload for an
/// `AppDataUpdate(GROUP_MEMBERSHIP)` proposal from the diff between
/// the old and new `GroupMembership` view. Shared between the commit-
/// bundling `apply_update_group_membership_intent` flow and the
/// propose-by-reference `IntentKind::ProposeMemberUpdate` flow so
/// both emit byte-identical payloads for the same diff.
///
/// Preserve failed installations in their owning inbox entries. Receivers use
/// these entries to validate why an authenticated installation has no MLS leaf.
pub(crate) fn build_group_membership_app_data_payload(
    conn: &impl xmtp_db::DbQuery,
    group: &OpenMlsGroup,
    old: &GroupMembership,
    new: &GroupMembership,
) -> Result<Vec<u8>, GroupError> {
    use xmtp_mls_common::app_data::component_source::read_from_app_data_dict;
    let old_bytes = read_from_app_data_dict(ComponentId::GROUP_MEMBERSHIP, group)
        .ok_or(GroupError::MissingSequenceId)?;
    build_membership_delta(conn, &old_bytes, old, new)
}

/// Selection and publication use the same delta from the committed dictionary.
pub(super) fn build_membership_delta(
    conn: &impl xmtp_db::DbQuery,
    old_bytes: &[u8],
    old: &GroupMembership,
    new: &GroupMembership,
) -> Result<Vec<u8>, GroupError> {
    use crate::identity_updates::{IdentityRequirement, require_association_state};
    use xmtp_mls_common::app_data::component_source::ComponentSourceError;
    let prior =
        GroupMembershipComponent::decode_value(old_bytes).map_err(ComponentSourceError::from)?;
    let wanted: HashSet<_> = new.failed_installations.iter().cloned().collect();
    let mut owners: HashMap<Vec<u8>, InboxId> = HashMap::new();
    for (inbox, bytes) in prior.iter() {
        let value = GroupMembershipEntry::decode(bytes.as_slice())?;
        if let Some(group_membership_entry::Version::V1(value)) = value.version {
            for installation in value.failed_installations {
                owners.insert(installation, *inbox);
            }
        }
    }
    if !wanted.is_empty() {
        for (inbox, &sequence_id) in &new.members {
            if sequence_id == 0 {
                continue;
            }
            let state = require_association_state(
                conn,
                &IdentityRequirement {
                    inbox_id: inbox.clone(),
                    sequence_id,
                },
            )
            .map_err(crate::groups::validated_commit::CommitValidationError::from)?;
            let inbox = InboxId::from_hex(inbox).map_err(ComponentSourceError::from)?;
            for installation in state.installation_ids() {
                owners.insert(installation, inbox);
            }
        }
    }
    let mut delta = TlsMapDelta::<InboxId, VLBytes>::new();

    // Inserts and updates: walk new.members, classify against old.
    for (inbox_id_str, &sequence_id) in new.members.iter() {
        let inbox = InboxId::from_hex(inbox_id_str).map_err(ComponentSourceError::from)?;
        let mut failed_installations: Vec<_> = wanted
            .iter()
            .filter(|installation| owners.get(*installation) == Some(&inbox))
            .cloned()
            .collect();
        failed_installations.sort_unstable();
        let entry = GroupMembershipEntry {
            version: Some(group_membership_entry::Version::V1(
                group_membership_entry::V1 {
                    sequence_id,
                    failed_installations,
                },
            )),
        }
        .encode_to_vec();
        match old.members.get(inbox_id_str) {
            None => {
                // New inbox: Insert.
                let inbox_id = InboxId::from_hex(inbox_id_str)
                    .map_err(|e| GroupError::ComponentSource(e.into()))?;
                delta = delta.insert(inbox_id, VLBytes::new(entry));
            }
            Some(_)
                if prior
                    .get(&inbox)
                    .is_none_or(|value| value.as_slice() != entry.as_slice()) =>
            {
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
            xmtp_mls_common::app_data::component_source::ComponentSourceError::from(e),
        )
    })
}

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[xmtp_common::mls_span]
pub(crate) fn apply_update_group_membership_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    mut changes_with_kps: MembershipDiffWithKeyPackages,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError> {
    let extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let mut new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);

    // The caller formed this request before the publication fetch. Losing the
    // last usable package for a requested inbox must fail the request, even if
    // pending proposals can still produce another valid commit or a no-op.
    // One usable installation remains sufficient for its inbox.
    let verified_adds = inbox_ids_from_new_key_packages(&changes_with_kps.new_key_packages);
    if new_group_membership
        .inbox_ids()
        .into_iter()
        .any(|inbox| old_group_membership.get(inbox).is_none() && !verified_adds.contains(inbox))
    {
        return Err(if changes_with_kps.failed_installations.is_empty() {
            GroupError::InvalidGroupMembership
        } else {
            GroupError::InvalidPublicKeys(changes_with_kps.failed_installations)
        });
    }

    let membership_diff = old_group_membership.diff(&new_group_membership);

    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &changes_with_kps.removed_installations);

    if leaf_nodes_to_remove.contains(&openmls_group.own_leaf_index()) {
        tracing::info!("Cannot remove own leaf node");
        return Ok(None);
    }

    // Run this guard before the writeback below. A change that only moves
    // `failed_installations` must not make an empty commit.
    if leaf_nodes_to_remove.is_empty()
        && changes_with_kps.new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
        && membership_diff.added_inboxes.is_empty()
        && membership_diff.removed_inboxes.is_empty()
        && openmls_group.pending_proposals().next().is_none()
    {
        return Ok(None);
    }

    // The publish-time fetch decides which Add proposals exist. The intent's
    // list is older, so it can be wrong. Union the two lists; do not assign.
    // The fetch drops ids that this commit also removes.
    // `expected_diff_matches_commit` needs those ids. They explain a removal
    // that the commit could not make. Assign breaks
    // `test_remove_inbox_with_bad_installation_from_group`.
    let mut failed_installations: HashSet<Vec<u8>> =
        std::mem::take(&mut new_group_membership.failed_installations)
            .into_iter()
            .collect();
    failed_installations.extend(std::mem::take(&mut changes_with_kps.failed_installations));
    let mut failed_installations: Vec<Vec<u8>> = failed_installations.into_iter().collect();
    // `PartialEq` compares this field as an ordered `Vec`.
    failed_installations.sort_unstable();
    new_group_membership.failed_installations = failed_installations;

    // New members must support the AppData dictionary that stores all group data.
    let app_data_ext_type = openmls::prelude::ExtensionType::AppDataDictionary;
    if !changes_with_kps.new_key_packages.is_empty() {
        let new_members_support_proposals = changes_with_kps.new_key_packages.iter().all(|kp| {
            kp.leaf_node()
                .capabilities()
                .extensions()
                .contains(&app_data_ext_type)
        });
        if !new_members_support_proposals {
            return Err(GroupError::ProposalsNotSupported(
                "A dictionary-native group requires AppDataDictionary support".into(),
            ));
        }
    }

    let app_data_payload = build_group_membership_app_data_payload(
        &storage.db(),
        openmls_group,
        &old_group_membership,
        &new_group_membership,
    )?;
    let publish_intent_data = compute_publish_data_for_proposal_based_update(
        storage,
        openmls_group,
        changes_with_kps.new_key_packages,
        leaf_nodes_to_remove,
        app_data_payload,
        false,
        signer,
    )?;
    Ok(Some(publish_intent_data))
}

/// Creates MLS proposals (Add/Remove + AppDataUpdate) and a commit that references them.
/// All payloads are returned together so they can be published in a single
/// `send_group_messages` call, eliminating multiple network roundtrips.
#[tracing::instrument(level = "trace", skip_all)]
fn compute_publish_data_for_proposal_based_update(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    key_packages_to_add: Vec<KeyPackage>,
    leaf_nodes_to_remove: Vec<LeafNodeIndex>,
    app_data_membership_payload: Vec<u8>,
    inline_membership: bool,
    signer: impl Signer,
) -> Result<PublishIntentData, GroupError> {
    let ((proposal_payloads, bundle), staged_commit, group_epoch) =
        generate_prepared_commit(storage, openmls_group, |group, provider| {
            let mut proposal_payloads: Vec<Vec<u8>> = Vec::new();

            // Selection has checked pending Adds against the target membership
            // and current packages. Reuse a selected reference when this intent
            // names the same installation. Keep its authenticated proposer.
            let pending_adds: HashSet<Vec<u8>> = group
                .pending_proposals()
                .filter_map(|proposal| match proposal.proposal() {
                    Proposal::Add(add) => Some(
                        add.key_package()
                            .leaf_node()
                            .signature_key()
                            .as_slice()
                            .to_vec(),
                    ),
                    _ => None,
                })
                .collect();
            let key_packages_to_add: Vec<_> = key_packages_to_add
                .into_iter()
                .filter(|key_package| {
                    !pending_adds.contains(key_package.leaf_node().signature_key().as_slice())
                })
                .collect();
            let pending_removals: HashSet<_> = group
                .pending_proposals()
                .filter_map(|proposal| match proposal.proposal() {
                    Proposal::Remove(remove) => Some(remove.removed()),
                    _ => None,
                })
                .collect();
            let leaf_nodes_to_remove: Vec<_> = leaf_nodes_to_remove
                .into_iter()
                .filter(|leaf| !pending_removals.contains(leaf))
                .collect();

            // 1. Create Add proposals
            for kp in key_packages_to_add.iter().filter(|_| !inline_membership) {
                let (msg, _) = group
                    .propose_add_member(provider, &signer, kp)
                    .map_err(GroupError::ProposeAddMember)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            }

            // 2. Create Remove proposals
            for &leaf_index in leaf_nodes_to_remove.iter().filter(|_| !inline_membership) {
                let (msg, _) = group
                    .propose_remove_member(provider, &signer, leaf_index)
                    .map_err(GroupError::ProposeRemoveMember)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            }

            // 3. Emit the AppDataUpdate(GROUP_MEMBERSHIP) proposal carrying the delta.
            // Receivers walk the proposal alongside the Add/Remove proposals
            // and apply the dictionary update via `accumulate_app_data_updates`.
            if !has_pending_membership_delta(group, &app_data_membership_payload)? {
                let (msg, _) = group
                    .propose_app_data_update(
                        provider,
                        &signer,
                        ComponentId::GROUP_MEMBERSHIP.as_u16(),
                        AppDataUpdateOperation::Update(app_data_membership_payload.clone().into()),
                    )
                    .map_err(GroupError::Proposal)?;
                proposal_payloads.push(msg.tls_serialize_detached()?);
            }

            // 4. Create a commit consuming all proposals. Pre-compute the dictionary
            // updates so the confirmation tag agrees with the receiver's apply path.
            let app_data_updates = crate::groups::app_data::pending_app_data_updates(group)?;
            let mut stage = group
                .commit_builder()
                .consume_proposal_store(true)
                .propose_adds(
                    key_packages_to_add
                        .iter()
                        .filter(|_| inline_membership)
                        .cloned(),
                )
                .propose_removals(
                    leaf_nodes_to_remove
                        .iter()
                        .filter(|_| inline_membership)
                        .copied(),
                )
                .load_psks(provider.storage())
                .map_err(CommitToPendingProposalsError::from)?;
            stage.with_app_data_dictionary_updates(app_data_updates);
            let bundle = stage
                .build(provider.rand(), provider.crypto(), &signer, |_| true)
                .map_err(CommitToPendingProposalsError::from)?
                .stage_commit(provider)
                .map_err(CommitToPendingProposalsError::from)?;

            Ok::<_, GroupError>((proposal_payloads, bundle))
        })?;

    let staged_commit = staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;
    let (commit, maybe_welcome_message, _) = bundle.into_messages();

    // Build all payloads with commit last (for intent hash matching)
    let mut payloads_to_publish = proposal_payloads;
    // The publish unit stores all proposals and the commit atomically.
    payloads_to_publish.push(commit.tls_serialize_detached()?);

    let post_commit_action = match maybe_welcome_message {
        Some(welcome_message) => Some(PostCommitAction::from_welcome(
            welcome_message,
            installations_for_staged_adds(&decode_staged_commit(&staged_commit)?)?,
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

/// Reuse an accepted delta with the same result, including a different mutation order.
pub(super) fn has_pending_membership_delta(
    group: &OpenMlsGroup,
    payload: &[u8],
) -> Result<bool, GroupError> {
    let prior = xmtp_mls_common::app_data::component_source::read_from_app_data_dict(
        ComponentId::GROUP_MEMBERSHIP,
        group,
    )
    .ok_or(GroupError::InvalidGroupMembership)?;
    let desired = GroupMembershipComponent::apply_update_payload(payload, Some(&prior))
        .map_err(xmtp_mls_common::app_data::component_source::ComponentSourceError::from)?;
    let desired = membership_from_app_data_bytes(&desired)?;
    for proposal in group.pending_proposals() {
        if let Proposal::AppDataUpdate(update) = proposal.proposal()
            && update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16()
            && let AppDataUpdateOperation::Update(payload) = update.operation()
        {
            let retained =
                GroupMembershipComponent::apply_update_payload(payload.as_slice(), Some(&prior))
                    .map_err(
                        xmtp_mls_common::app_data::component_source::ComponentSourceError::from,
                    )?;
            if membership_from_app_data_bytes(&retained)? == desired {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Welcome every included Add using its exact selected key package, including
/// accepted proposals from other members.
pub(crate) fn installations_for_staged_adds(
    staged_commit: &StagedCommit,
) -> Result<Vec<Installation>, GroupError> {
    staged_commit
        .add_proposals()
        .map(|proposal| {
            let key_package = xmtp_id::key_package::VerifiedKeyPackageV2::try_from(
                proposal.add_proposal().key_package().clone(),
            )
            .map_err(crate::groups::intents::IntentError::from)?;
            Installation::from_verified_key_package(&key_package).map_err(Into::into)
        })
        .collect()
}

/// Preserve Welcome work for every Add selected by any kind of commit.
pub(crate) fn welcome_post_commit_action(
    welcome: Option<openmls::prelude::MlsMessageOut>,
    staged_commit: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, GroupError> {
    let Some(welcome) = welcome else {
        return Ok(None);
    };
    let staged_commit =
        decode_staged_commit(staged_commit.ok_or(GroupError::MissingPendingCommit)?)?;
    Ok(Some(
        PostCommitAction::from_welcome(welcome, installations_for_staged_adds(&staged_commit)?)?
            .to_bytes(),
    ))
}

#[xmtp_common::mls_span]
pub(crate) fn apply_readd_installations_intent(
    storage: &impl XmtpMlsStorageProvider,
    openmls_group: &mut OpenMlsGroup,
    intent_data: ReaddInstallationsIntentData,
    changes_with_kps: MembershipDiffWithKeyPackages,
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

    if installations_to_readd.is_empty() {
        return Ok(None);
    }
    let key_packages_to_welcome = changes_with_kps
        .new_key_packages
        .into_iter()
        .filter(|key_package| {
            installations_to_readd.contains(key_package.leaf_node().signature_key().as_slice())
        })
        .collect();
    let failed_installations = changes_with_kps
        .failed_installations
        .into_iter()
        .filter(|installation| installations_to_readd.contains(installation));

    // Update group membership to reflect any failed installations.
    let extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let failed_installations: HashSet<Vec<u8>> = old_group_membership
        .failed_installations
        .clone()
        .into_iter()
        .chain(failed_installations)
        .collect();
    let mut failed_installations: Vec<_> = failed_installations.into_iter().collect();
    failed_installations.sort_unstable();
    let new_group_membership = GroupMembership {
        members: old_group_membership.members.clone(),
        failed_installations,
    };
    let payload = build_readd_membership_payload(openmls_group, &new_group_membership)?;
    let publish_intent_data = compute_publish_data_for_proposal_based_update(
        storage,
        openmls_group,
        key_packages_to_welcome,
        leaf_indices_to_remove,
        payload,
        // A super-admin re-add must be checked as one commit. Its Remove
        // alone is forbidden; the matching Add keeps the inbox present.
        true,
        signer,
    )?;

    Ok(Some(publish_intent_data))
}

/// Keep existing failures in their inbox entries. Attribute new failures with
/// the current ratchet tree, before the readd removes those installations.
fn build_readd_membership_payload(
    group: &OpenMlsGroup,
    membership: &GroupMembership,
) -> Result<Vec<u8>, GroupError> {
    use xmtp_mls_common::app_data::component_source::{ComponentSourceError, read_component_bytes};

    let malformed = |reason: &str| ComponentSourceError::MalformedComponentValue {
        component_id: ComponentId::GROUP_MEMBERSHIP,
        reason: reason.to_string(),
    };
    let bytes = read_component_bytes(ComponentId::GROUP_MEMBERSHIP, group.extensions())?
        .ok_or_else(|| malformed("missing membership component"))?;
    let entries =
        GroupMembershipComponent::decode_value(&bytes).map_err(ComponentSourceError::from)?;
    let failed: HashSet<_> = membership.failed_installations.iter().collect();
    let mut failures_by_inbox: HashMap<InboxId, Vec<Vec<u8>>> = HashMap::new();
    for member in group.members() {
        if failed.contains(&member.signature_key) {
            let credential = BasicCredential::try_from(member.credential)?;
            let inbox = InboxId::from_hex(&parse_credential(credential.identity())?)
                .map_err(ComponentSourceError::from)?;
            failures_by_inbox
                .entry(inbox)
                .or_default()
                .push(member.signature_key);
        }
    }

    let mut delta = TlsMapDelta::<InboxId, VLBytes>::new();
    for (inbox, failures) in failures_by_inbox {
        let prior = entries
            .get(&inbox)
            .ok_or_else(|| malformed("failed installation inbox is absent from membership"))?;
        let mut entry = GroupMembershipEntry::decode(prior.as_slice())?;
        let Some(group_membership_entry::Version::V1(value)) = &mut entry.version else {
            return Err(malformed("membership entry has no version").into());
        };
        value.failed_installations.extend(failures);
        value.failed_installations.sort_unstable();
        value.failed_installations.dedup();
        let updated = entry.encode_to_vec();
        if updated != prior.as_slice() {
            delta = delta.update(inbox, VLBytes::new(updated));
        }
    }
    GroupMembershipComponent::encode_mutation(&delta)
        .map_err(|error| ComponentSourceError::from(error).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test(unwrap_try = true)]
    async fn membership_delta_reuse_ignores_mutation_order() {
        use crate::tester;
        use tls_codec::Deserialize;
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);
        tester!(dave, disable_workers);
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        let old =
            group.with_group_snapshot(|mls| Ok(extract_group_membership(mls.extensions())?))?;
        let intent = group
            .get_membership_update_intent(&[caro.inbox_id(), dave.inbox_id()], &[])
            .await?;
        let new = intent.apply_to_group_membership(&old);
        let changes = calculate_membership_changes_with_keypackages(
            &group.context,
            &group.group_id,
            &new,
            &old,
        )
        .await?;
        let outcome: TransactionOutcome<()> =
            crate::state_tx::state_write(group.context.mls_storage(), |tx| {
                tx.with_group(group.group_id, |mls, storage| {
                    let payload =
                        build_group_membership_app_data_payload(&storage.db(), mls, &old, &new)?;
                    let mut reversed =
                        TlsMapDelta::<InboxId, VLBytes>::tls_deserialize_exact(&payload)?;
                    assert_eq!(reversed.mutations.len(), 2);
                    reversed.mutations.reverse();
                    let reversed = reversed.tls_serialize_detached()?;
                    assert_ne!(payload, reversed);
                    let provider = XmtpOpenMlsProviderRef::new(storage);
                    let signer = &group.context.identity().installation_keys;
                    // Seed authenticated pending proposals with an equivalent delta
                    // in the opposite wire order. No mutation may be signed twice.
                    for package in &changes.new_key_packages {
                        mls.propose_add_member(&provider, signer, package)
                            .map_err(GroupError::ProposeAddMember)?;
                    }
                    mls.propose_app_data_update(
                        &provider,
                        signer,
                        ComponentId::GROUP_MEMBERSHIP.as_u16(),
                        AppDataUpdateOperation::Update(reversed.into()),
                    )
                    .map_err(GroupError::Proposal)?;
                    let publish = compute_publish_data_for_proposal_based_update(
                        storage,
                        mls,
                        changes.new_key_packages.clone(),
                        vec![],
                        payload,
                        false,
                        group.context.identity().installation_keys.clone(),
                    )?;
                    assert_eq!(
                        publish.payloads_to_publish.len(),
                        1,
                        "reuse accepted Adds and the complete membership delta"
                    );
                    let staged = decode_staged_commit(publish.staged_commit.as_ref().unwrap())?;
                    assert_eq!(staged.add_proposals().count(), 2);
                    ValidatedCommit::from_staged_commit_local(
                        &group.context,
                        &storage.db(),
                        &staged,
                        mls.own_leaf_index(),
                        mls,
                        u64::MAX,
                    )?;
                    Ok::<_, GroupError>(Rollback)
                })
            })?;
        assert!(matches!(outcome, Rollback));
    }
}
