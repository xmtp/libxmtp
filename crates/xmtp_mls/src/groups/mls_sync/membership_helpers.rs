//! Membership, key package, and commit-construction helpers.

use super::*;

/// Compare membership intent results without depending on delta mutation order.
pub(super) fn membership_from_app_data_bytes(bytes: &[u8]) -> Result<GroupMembership, GroupError> {
    use xmtp_mls_common::app_data::component_source::ComponentSourceError;
    use xmtp_mls_common::app_data::{
        components::tls_map_components::GroupMembershipComponent, typed::Component,
    };
    let entries =
        GroupMembershipComponent::decode_value(bytes).map_err(ComponentSourceError::from)?;
    let mut membership = GroupMembership::new();
    for (inbox, bytes) in entries.iter() {
        let entry = xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry::decode(
            bytes.as_slice(),
        )?;
        let Some(xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version::V1(
            entry,
        )) = entry.version
        else {
            return Err(GroupError::InvalidGroupMembership);
        };
        membership.add(inbox.to_hex(), entry.sequence_id);
        membership
            .failed_installations
            .extend(entry.failed_installations);
    }
    membership.failed_installations.sort_unstable();
    membership.failed_installations.dedup();
    Ok(membership)
}

// Extracts the message sender, but does not do any validation to ensure that the
// installation_id is actually part of the inbox.
pub(super) fn extract_message_sender(
    openmls_group: &mut OpenMlsGroup,
    decrypted_message: &ProcessedMessage,
    message_created_ns: u64,
) -> Result<(InboxId, Vec<u8>), GroupMessageProcessingError> {
    if let Sender::Member(leaf_node_index) = decrypted_message.sender()
        && let Some(member) = openmls_group.member_at(*leaf_node_index)
        && member.credential.eq(decrypted_message.credential())
    {
        let basic_credential = BasicCredential::try_from(member.credential)?;
        let sender_inbox_id = parse_credential(basic_credential.identity())?;
        return Ok((sender_inbox_id, member.signature_key));
    }

    let basic_credential = BasicCredential::try_from(decrypted_message.credential().clone())?;
    Err(GroupMessageProcessingError::InvalidSender {
        message_time_ns: message_created_ns,
        credential: basic_credential.identity().to_vec(),
    })
}

pub(in crate::groups) async fn calculate_membership_changes_with_keypackages<'a>(
    context: &impl XmtpSharedContext,
    group_id: &GroupId,
    new_group_membership: &'a GroupMembership,
    old_group_membership: &'a GroupMembership,
) -> Result<MembershipDiffWithKeyPackages, GroupError> {
    let membership_diff = old_group_membership.diff(new_group_membership);

    let identity = IdentityUpdates::new(&context);
    let mut installation_diff = identity
        .get_installation_diff(
            &context.db(),
            group_id,
            old_group_membership,
            new_group_membership,
            &membership_diff,
        )
        .await?;

    let mut new_installations = Vec::new();
    let mut new_key_packages = Vec::new();
    let mut new_failed_installations = Vec::new();

    if !installation_diff.added_installations.is_empty() {
        get_keypackages_for_installation_ids(
            context,
            installation_diff.added_installations,
            &mut new_installations,
            &mut new_key_packages,
            &mut new_failed_installations,
        )
        .await?;
    }

    let mut failed_installations: HashSet<Vec<u8>> = old_group_membership
        .failed_installations
        .clone()
        .into_iter()
        .chain(new_failed_installations)
        .collect();

    let common: HashSet<_> = failed_installations
        .intersection(&installation_diff.removed_installations)
        .cloned()
        .collect();

    failed_installations.retain(|item| !common.contains(item));

    installation_diff
        .removed_installations
        .retain(|item| !common.contains(item));

    // This field is a set, but `GroupMembership`'s `PartialEq` compares it as
    // an ordered `Vec`. Sort it. An unchanged set must always compare equal.
    let mut failed_installations: Vec<Vec<u8>> = failed_installations.into_iter().collect();
    failed_installations.sort_unstable();

    Ok(MembershipDiffWithKeyPackages::new(
        new_installations,
        new_key_packages,
        installation_diff.removed_installations,
        failed_installations,
    ))
}

/// Drop the configured installations' packages, as if the backend served none.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn inject_failed_installations_for_test(
    key_packages: &mut HashMap<
        Vec<u8>,
        Result<
            xmtp_id::key_package::VerifiedKeyPackageV2,
            xmtp_id::key_package::KeyPackageVerificationError,
        >,
    >,
) {
    let malformed = crate::utils::test_mocks_helpers::get_test_mode_malformed_installations();
    key_packages.retain(|id, _| !malformed.contains(id));
}

/// Fetch a key package for every requested installation except this one.
/// Each is either returned with its verified package or recorded as failed.
pub(super) async fn get_keypackages_for_installation_ids(
    context: impl XmtpSharedContext,
    requested_installations: HashSet<Vec<u8>>,
    fetched_installations: &mut Vec<Installation>,
    fetched_key_packages: &mut Vec<KeyPackage>,
    failed_installations: &mut Vec<Vec<u8>>,
) -> Result<(), GroupError> {
    let my_installation_id = context.installation_id().to_vec();
    let requested: Vec<_> = requested_installations
        .into_iter()
        .filter(|installation| my_installation_id.ne(installation))
        .collect();
    #[allow(unused_mut)]
    let mut key_packages = MlsStore::new(context.clone())
        .get_key_packages_for_installation_ids(requested.clone())
        .await?;

    #[cfg(any(test, feature = "test-utils"))]
    inject_failed_installations_for_test(&mut key_packages);

    // implements: JOIN-012, GMOD-009
    for installation_id in requested {
        match key_packages.remove(&installation_id) {
            Some(Ok(verified_key_package)) => {
                fetched_installations.push(Installation::from_verified_key_package(
                    &verified_key_package,
                )?);
                fetched_key_packages.push(verified_key_package.inner);
            }
            // An absent package fails the installation exactly as an invalid one does.
            Some(Err(_)) | None => failed_installations.push(installation_id),
        }
    }

    Ok(())
}

pub(super) fn get_removed_leaf_nodes(
    openmls_group: &mut OpenMlsGroup,
    removed_installations: &HashSet<Vec<u8>>,
) -> Vec<LeafNodeIndex> {
    openmls_group
        .members()
        .filter(|member| removed_installations.contains(&member.signature_key))
        .map(|member| member.index)
        .collect()
}

/// Prepare a commit without merging it. Keep the new keys and sender ratchets.
/// The caller must supply its writer-scoped group and storage, then persist the
/// exact attempt before that same transaction commits.
pub(in crate::groups) fn generate_prepared_commit<S, R, E, F>(
    storage: &S,
    openmls_group: &mut OpenMlsGroup,
    operation: F,
) -> Result<(R, Option<Vec<u8>>, u64), GroupError>
where
    S: XmtpMlsStorageProvider,
    E: Into<GroupError>,
    F: FnOnce(&mut OpenMlsGroup, &XmtpOpenMlsProviderRef<S>) -> Result<R, E>,
{
    if openmls_group.pending_commit().is_some() {
        return Err(publish::OutgoingPreparationError::UnexpectedPendingCommit.into());
    }
    let provider = XmtpOpenMlsProviderRef::new(storage);
    let group_epoch = openmls_group.epoch().as_u64();
    let result = operation(openmls_group, &provider).map_err(Into::into)?;
    let staged_commit = openmls_group
        .pending_commit()
        .map(xmtp_db::db_serialize)
        .transpose()?;
    openmls_group.clear_pending_commit(storage)?;
    Ok((result, staged_commit, group_epoch))
}

/// Build a commit bundle that consumes all pending proposals and
/// pre-computes any AppData dictionary writes required by queued
/// `AppDataUpdate` proposals. The commit also carries this client's
/// membership upkeep for `catalogue` inline.
///
/// Any commit that consumes the proposal store must route through this
/// helper so OpenMLS's `apply_app_data_update_proposals` sees the dict
/// writes the queued `AppDataUpdate` proposals expect — otherwise the
/// build fails with `MissingAppDataUpdates`. Callers provide a
/// `proposal_filter` so the business logic (e.g. "only include a GCE
/// whose membership matches the one we're about to apply") stays at
/// the call site.
pub(super) fn build_commit_with_pending_app_data_updates<P, F>(
    group: &mut OpenMlsGroup,
    provider: &P,
    signer: &impl openmls_traits::signatures::Signer,
    catalogue: &[xmtp_configuration::ApplicationComponentDefinition],
    proposal_filter: F,
) -> Result<openmls::prelude::CommitMessageBundle, GroupError>
where
    P: OpenMlsProvider,
    P::StorageProvider:
        openmls_traits::storage::StorageProvider<1, Error = sql_key_store::SqlKeyStoreError>,
    F: FnMut(&openmls::group::QueuedProposal) -> bool,
{
    let (upkeep, app_data_updates) =
        crate::groups::app_data::membership_upkeep::membership_upkeep(group, catalogue)?;

    let mut stage = group
        .commit_builder()
        .consume_proposal_store(true)
        .add_proposals(upkeep)
        .load_psks(provider.storage())
        .map_err(CommitToPendingProposalsError::from)?;
    stage.with_app_data_dictionary_updates(app_data_updates);

    let bundle = stage
        .build(provider.rand(), provider.crypto(), signer, proposal_filter)
        .map_err(CommitToPendingProposalsError::from)?
        .stage_commit(provider)
        .map_err(CommitToPendingProposalsError::from)?;

    Ok(bundle)
}

pub(crate) fn decode_staged_commit(
    data: &[u8],
) -> Result<StagedCommit, GroupMessageProcessingError> {
    Ok(xmtp_db::db_deserialize(data)?)
}
