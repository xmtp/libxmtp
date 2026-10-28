//! Membership, key package, and commit-construction helpers.

use super::*;
use xmtp_cryptography::configuration::CIPHERSUITE;

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
    let installation_diff = identity
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

    // A removed id is no longer failed. It stays in the removals: one without
    // a leaf yields no Remove proposal, and an older client can have listed
    // an id that has a leaf, which the commit must still remove.
    failed_installations.retain(|item| !installation_diff.removed_installations.contains(item));

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

/// Drop the configured installations' packages, as if the backend served none,
/// and put each configured unchecked package in place of its installation's.
#[cfg(any(test, feature = "test-utils"))]
pub(super) fn inject_test_key_packages(
    key_packages: &mut HashMap<
        Vec<u8>,
        Result<
            xmtp_id::key_package::VerifiedKeyPackageV2,
            xmtp_id::key_package::KeyPackageVerificationError,
        >,
    >,
) {
    use crate::utils::test_mocks_helpers::{
        get_test_mode_malformed_installations, test_mode_unchecked_key_package,
    };
    let malformed = get_test_mode_malformed_installations();
    key_packages.retain(|id, _| !malformed.contains(id));
    for (id, served) in key_packages.iter_mut() {
        if let Some(package) = test_mode_unchecked_key_package(id) {
            *served = Ok(package);
        }
    }
}

/// Fetch a key package for every requested installation except this one.
/// `requested_installations` maps each installation key to the inboxes that own it.
/// Each is either returned with its verified package or recorded as failed.
/// A package whose credential names another inbox fails, because receivers
/// reject a leaf whose key the credential's inbox does not own.
/// A package under another suite fails every Add, and one whose leaf is
/// another installation's key adds the wrong leaf, so both fail too.
pub(super) async fn get_keypackages_for_installation_ids(
    context: impl XmtpSharedContext,
    requested_installations: HashMap<Vec<u8>, HashSet<String>>,
    fetched_installations: &mut Vec<Installation>,
    fetched_key_packages: &mut Vec<KeyPackage>,
    failed_installations: &mut Vec<Vec<u8>>,
) -> Result<(), GroupError> {
    let my_installation_id = context.installation_id().to_vec();
    let requested: Vec<_> = requested_installations
        .into_iter()
        .filter(|(installation, _)| my_installation_id.ne(installation))
        .collect();
    #[allow(unused_mut)]
    let mut key_packages = MlsStore::new(context.clone())
        .get_key_packages_for_installation_ids(
            requested
                .iter()
                .map(|(installation, _)| installation.clone())
                .collect(),
        )
        .await?;

    #[cfg(any(test, feature = "test-utils"))]
    inject_test_key_packages(&mut key_packages);

    // implements: JOIN-093, JOIN-012, GMOD-009
    for (installation_id, owner_inbox_ids) in requested {
        match key_packages.remove(&installation_id) {
            Some(Ok(verified_key_package))
                if owner_inbox_ids.contains(&verified_key_package.credential.inbox_id)
                    && verified_key_package.inner.ciphersuite() == CIPHERSUITE
                    && verified_key_package.installation_public_key == installation_id =>
            {
                fetched_installations.push(Installation::from_verified_key_package(
                    &verified_key_package,
                ));
                fetched_key_packages.push(verified_key_package.inner);
            }
            // An absent package fails the installation exactly as an invalid
            // one does, including one whose addressing extensions no sender
            // can use.
            Some(_) | None => failed_installations.push(installation_id),
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

    let mut stage =
        crate::groups::mls_ext::commit_without_group_info(group, provider.storage(), |builder| {
            builder.consume_proposal_store(true).add_proposals(upkeep)
        })
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
