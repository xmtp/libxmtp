//! `USER_DISPLAY_NAME` and `GROUP_IMAGE` values, removed-member cleanup, and
//! the single write of an application immutable field.

use std::collections::BTreeMap;

use openmls::{
    component::ComponentData,
    framing::MlsMessageOut,
    group::MlsGroup as OpenMlsGroup,
    messages::proposals::{AppDataUpdateOperation, Proposal},
    prelude::CommitMessageBundle,
    storage::OpenMlsProvider,
};
use openmls_traits::signatures::Signer;
use tls_codec::VLBytes;
use tls_codec::{Deserialize, Serialize};
use xmtp_db::{
    TransactionOutcome::Continue,
    incoming_envelope::{StreamTopic, TerminalRejection},
    prelude::*,
};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        fields::{ComponentMutation, FieldValue, MetadataFieldRef},
    },
    inbox_id::InboxId,
    tls_map::{TlsMap, TlsMapDelta},
    tls_set::TlsSet,
};
use xmtp_proto::xmtp::mls::message_contents::{ComponentType, metadata_policy::MetadataBasePolicy};

use super::test_dictionary_creation::{definition, value};
use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, UpdateAdminListType,
        app_data::{AppDataBatch, GroupAppDataError, load_component_registry},
        intents::{AppDataUpdateIntentData, ProposeMemberUpdateIntentData, QueueIntent},
        mls_sync::generate_prepared_commit,
    },
    state_tx::state_write,
    tester,
};

const MAX_ELEMENT: usize = 8192;

fn inbox(id: &str) -> InboxId {
    InboxId::from_hex(id).unwrap()
}

/// A `USER_DISPLAY_NAME` delta that inserts `name` under `owner`.
fn insert_name(owner: &str, name: &[u8]) -> Vec<u8> {
    TlsMapDelta::<InboxId, VLBytes>::new()
        .insert(inbox(owner), VLBytes::new(name.to_vec()))
        .tls_serialize_detached()
        .unwrap()
}

fn display_names<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<BTreeMap<InboxId, Vec<u8>>, GroupError> {
    Ok(value(group, ComponentId::USER_DISPLAY_NAME)?
        .map(|bytes| TlsMap::<InboxId, VLBytes>::tls_deserialize_exact(bytes).unwrap())
        .map(|map| {
            map.iter()
                .map(|(key, value)| (*key, value.as_slice().to_vec()))
                .collect()
        })
        .unwrap_or_default())
}

fn admins<C: XmtpSharedContext>(group: &MlsGroup<C>) -> Result<Vec<InboxId>, GroupError> {
    inboxes(group, ComponentId::ADMIN_LIST)
}

fn inboxes<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ComponentId,
) -> Result<Vec<InboxId>, GroupError> {
    Ok(value(group, id)?
        .map(|bytes| TlsSet::<InboxId>::tls_deserialize_exact(bytes).unwrap())
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default())
}

/// Write through the sender's own checks.
async fn write<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ComponentId,
    payload: Vec<u8>,
) -> Result<(), GroupError> {
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            id.as_u16(),
            payload,
        )))
        .queue(group)?;
    group.sync_until_intent_resolved(intent.id).await.map(drop)
}

fn last_rejection<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<Option<TerminalRejection>, GroupError> {
    Ok(group
        .context
        .db()
        .read_last_rejection(&StreamTopic::group(group.group_id))?)
}

/// Publish the proposals that remove `removed`, without committing them.
async fn propose_removal<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    removed: &str,
) -> Result<(), GroupError> {
    propose_members(group, vec![], vec![removed.to_string()]).await
}

/// Publish the proposals that add `added` and remove `removed`, without
/// committing them.
async fn propose_members<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    added: Vec<String>,
    removed: Vec<String>,
) -> Result<(), GroupError> {
    let intent = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            added, removed,
        ))?)
        .queue(group)?;
    group.sync_until_intent_resolved(intent.id).await.map(drop)
}

/// A commit that is not a membership change but sweeps pending proposals.
enum Sweep {
    MetadataWrite,
    KeyUpdate,
    PendingProposals,
}

/// Carol, a named admin, is removed by a pending proposal that `sweep`
/// commits. The commit must delete her display name and admin-list entry,
/// for the committer and for a receiver.
async fn assert_sweep_cleans_up(sweep: Sweep) -> Result<(), GroupError> {
    tester!(alix);
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    let carol_group = carol.sync_welcomes().await?.remove(0);
    write(
        &carol_group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(carol.inbox_id(), b"Carol"),
    )
    .await?;
    group.sync().await?;
    group
        .update_admin_list(UpdateAdminListType::Add, carol.inbox_id().to_string())
        .await?;
    propose_removal(&group, carol.inbox_id()).await?;

    match sweep {
        Sweep::MetadataWrite => group.update_group_name("Renamed".into()).await?,
        Sweep::KeyUpdate => group.key_update().await?,
        Sweep::PendingProposals => {
            let intent = QueueIntent::commit_pending_proposals().queue(&group)?;
            group.sync_until_intent_resolved(intent.id).await?;
        }
    }
    bo_group.sync().await?;
    assert_eq!(bo_group.epoch().await?, group.epoch().await?);
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert!(display_names(member)?.is_empty());
        assert!(admins(member)?.is_empty());
    }
    Ok(())
}

/// Stage `updates` in one commit the way a sender that skips its own
/// checks and adds no membership upkeep would. The commit also sweeps the
/// group's pending proposals.
///
/// A payload the sender's own apply refuses goes into the dictionary
/// verbatim: receivers refuse it before they reach the confirmation tag.
fn stage_unchecked<P: OpenMlsProvider>(
    group: &mut OpenMlsGroup,
    provider: &P,
    signer: &impl Signer,
    updates: &[(ComponentId, Vec<u8>)],
) -> Result<(Vec<MlsMessageOut>, CommitMessageBundle), GroupAppDataError<P::StorageError>> {
    let mut proposals = Vec::new();
    for (id, payload) in updates {
        let operation = AppDataUpdateOperation::Update(payload.clone().into());
        let (proposal, _) = group
            .propose_app_data_update(provider, signer, id.as_u16(), operation)
            .map_err(GroupAppDataError::Propose)?;
        proposals.push(proposal);
    }
    let dictionary = AppDataBatch::pending(group)
        .map(AppDataBatch::updates)
        .unwrap_or_else(|_| {
            let mut updater = group.app_data_dictionary_updater();
            for (id, payload) in updates {
                updater.set(ComponentData::from_parts(
                    id.as_u16(),
                    payload.clone().into(),
                ));
            }
            updater.changes()
        });
    let mut stage =
        crate::groups::mls_ext::commit_without_group_info(group, provider.storage(), |builder| {
            builder.consume_proposal_store(true)
        })?;
    stage.with_app_data_dictionary_updates(dictionary);
    let bundle = stage
        .build(provider.rand(), provider.crypto(), signer, |_| true)?
        .stage_commit(provider)?;
    Ok((proposals, bundle))
}

/// Publish `updates` as one [`stage_unchecked`] commit.
async fn publish_unchecked<C: XmtpSharedContext>(
    author: &MlsGroup<C>,
    updates: Vec<(ComponentId, Vec<u8>)>,
) -> Result<(), GroupError> {
    let signer = &author.context.identity().installation_keys;
    let payloads = state_write(author.context.mls_storage(), |tx| {
        tx.with_group(author.group_id, |mls_group, storage| {
            let ((proposals, bundle), _, _) =
                generate_prepared_commit(storage, mls_group, |group, provider| {
                    Ok::<_, GroupError>(stage_unchecked(group, provider, signer, &updates)?)
                })?;
            let (commit, _, _) = bundle.into_messages();
            let mut payloads = proposals
                .iter()
                .map(Serialize::tls_serialize_detached)
                .collect::<Result<Vec<_>, _>>()?;
            payloads.push(commit.tls_serialize_detached()?);
            Ok::<_, GroupError>(Continue(payloads))
        })
    })?
    .into_continued();
    let messages = author.prepare_group_messages(
        payloads
            .iter()
            .map(|payload| (payload.as_slice(), false))
            .collect(),
    )?;
    author.context.api().send_group_messages(messages).await?;
    Ok(())
}

/// Publish `updates` as one [`stage_unchecked`] commit, and
/// require `peer` to reject it and keep its epoch.
async fn assert_peer_rejects<C: XmtpSharedContext>(
    author: &MlsGroup<C>,
    peer: &MlsGroup<C>,
    updates: Vec<(ComponentId, Vec<u8>)>,
) -> Result<(), GroupError> {
    peer.sync().await?;
    let before = last_rejection(peer)?;
    let epoch = peer.epoch().await?;
    publish_unchecked(author, updates).await?;
    let _ = peer.sync().await;
    assert_ne!(last_rejection(peer)?, before, "peer must reject the commit");
    assert_eq!(peer.epoch().await?, epoch);
    Ok(())
}

/// A member sets its own display name, but neither the sender nor a
/// receiver lets it write another member's.
// verifies: PERM-027
#[xmtp_common::test(unwrap_try = true)]
async fn test_display_name_is_self_owned() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    write(
        &group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(alix.inbox_id(), b"Alix"),
    )
    .await?;
    bo_group.sync().await?;
    let expected = BTreeMap::from([(inbox(alix.inbox_id()), b"Alix".to_vec())]);
    assert_eq!(display_names(&bo_group)?, expected);

    let forged = insert_name(bo.inbox_id(), b"Not Bo");
    assert!(
        write(&group, ComponentId::USER_DISPLAY_NAME, forged.clone())
            .await
            .is_err()
    );
    assert_peer_rejects(
        &group,
        &bo_group,
        vec![(ComponentId::USER_DISPLAY_NAME, forged)],
    )
    .await?;
    assert_eq!(display_names(&bo_group)?, expected);
}

/// Receivers reject a display name that is not UTF-8.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
async fn test_display_name_must_be_utf8() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    let invalid = insert_name(alix.inbox_id(), &[0xC3, 0x28]);
    assert_peer_rejects(
        &group,
        &bo_group,
        vec![(ComponentId::USER_DISPLAY_NAME, invalid)],
    )
    .await?;
    assert!(display_names(&bo_group)?.is_empty());
}

/// A display name or group image may hold 8192 bytes, and receivers reject
/// one byte more.
// verifies: META-068
#[xmtp_common::test(unwrap_try = true)]
async fn test_profile_values_are_bounded() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    let name = vec![b'n'; MAX_ELEMENT];
    write(
        &group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(alix.inbox_id(), &name),
    )
    .await?;
    write(&group, ComponentId::GROUP_IMAGE, vec![0xFF; MAX_ELEMENT]).await?;
    for (id, payload) in [
        (
            ComponentId::USER_DISPLAY_NAME,
            TlsMapDelta::<InboxId, VLBytes>::new()
                .update(
                    inbox(alix.inbox_id()),
                    VLBytes::new(vec![b'n'; MAX_ELEMENT + 1]),
                )
                .tls_serialize_detached()?,
        ),
        (ComponentId::GROUP_IMAGE, vec![0xFF; MAX_ELEMENT + 1]),
    ] {
        assert_peer_rejects(&group, &bo_group, vec![(id, payload)]).await?;
    }
    assert_eq!(display_names(&bo_group)?[&inbox(alix.inbox_id())], name);
    assert_eq!(
        value(&bo_group, ComponentId::GROUP_IMAGE)?,
        Some(vec![0xFF; MAX_ELEMENT])
    );
}

/// `GROUP_IMAGE` carries opaque bytes: a value that is neither UTF-8 nor a
/// URL arrives unchanged.
// verifies: META-010
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_image_is_opaque() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    let image = vec![0x00, 0xFF, 0xC3, 0x28, 0x0A];
    write(&group, ComponentId::GROUP_IMAGE, image.clone()).await?;
    bo_group.sync().await?;
    assert_eq!(value(&bo_group, ComponentId::GROUP_IMAGE)?, Some(image));
}

/// The commit that removes a member also deletes its display name and
/// admin-list entry, so no stale profile or role outlives the membership.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_removal_deletes_removed_member_entries() {
    tester!(alix);
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    for (member, inbox_id, name) in [
        (&bo_group, bo.inbox_id(), b"Bo".as_slice()),
        (&carol_group, carol.inbox_id(), b"Carol".as_slice()),
    ] {
        member.sync().await?;
        write(
            member,
            ComponentId::USER_DISPLAY_NAME,
            insert_name(inbox_id, name),
        )
        .await?;
    }
    group.sync().await?;
    group
        .update_admin_list(UpdateAdminListType::Add, carol.inbox_id().to_string())
        .await?;
    assert_eq!(admins(&group)?, vec![inbox(carol.inbox_id())]);

    let epoch = group.epoch().await?;
    group.remove_members(&[carol.inbox_id()]).await?;
    assert_eq!(group.epoch().await?, epoch + 1);
    bo_group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert_eq!(
            display_names(member)?,
            BTreeMap::from([(inbox(bo.inbox_id()), b"Bo".to_vec())])
        );
        assert!(admins(member)?.is_empty());
    }
}

/// A remover that may not delete an admin-list entry still removes the
/// member: it omits that cleanup, keeps the display-name cleanup, and
/// receivers accept the commit without it.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_removal_omits_forbidden_cleanup() {
    tester!(alix);
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    for admin in [bo.inbox_id(), carol.inbox_id()] {
        group
            .update_admin_list(UpdateAdminListType::Add, admin.to_string())
            .await?;
    }
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    carol_group.sync().await?;
    write(
        &carol_group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(carol.inbox_id(), b"Carol"),
    )
    .await?;
    bo_group.sync().await?;

    // Bo is an admin, not a super admin, so the admin list is closed to him.
    let epoch = bo_group.epoch().await?;
    bo_group.remove_members(&[carol.inbox_id()]).await?;
    assert_eq!(bo_group.epoch().await?, epoch + 1);
    group.sync().await?;
    assert!(last_rejection(&group)?.is_none());
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert!(display_names(member)?.is_empty());
        assert!(admins(member)?.contains(&inbox(carol.inbox_id())));
    }
}

/// A metadata write that commits a pending removal also cleans up after
/// the removed member.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_metadata_write_cleans_up_a_swept_removal() {
    assert_sweep_cleans_up(Sweep::MetadataWrite).await?;
}

/// A key update that commits a pending removal also cleans up after the
/// removed member.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_key_update_cleans_up_a_swept_removal() {
    assert_sweep_cleans_up(Sweep::KeyUpdate).await?;
}

/// A commit of pending proposals that removes a member also cleans up
/// after it.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_commit_cleans_up_a_swept_removal() {
    assert_sweep_cleans_up(Sweep::PendingProposals).await?;
}

/// Cleanup is the sender's courtesy: a receiver accepts a removal that
/// omits a delete its sender was authorized to include, as an older
/// client's removal would.
// verifies: GMOD-039
#[xmtp_common::test(unwrap_try = true)]
async fn test_removal_without_cleanup_is_accepted() {
    tester!(alix);
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    write(
        &carol_group,
        ComponentId::USER_DISPLAY_NAME,
        insert_name(carol.inbox_id(), b"Carol"),
    )
    .await?;
    group.sync().await?;
    propose_removal(&group, carol.inbox_id()).await?;
    bo_group.sync().await?;
    let before = last_rejection(&bo_group)?;
    let epoch = bo_group.epoch().await?;

    publish_unchecked(&group, vec![]).await?;
    bo_group.sync().await?;
    assert_eq!(last_rejection(&bo_group)?, before);
    assert_eq!(bo_group.epoch().await?, epoch + 1);
    assert_eq!(bo_group.members().await?.len(), 2);
    assert_eq!(
        display_names(&bo_group)?,
        BTreeMap::from([(inbox(carol.inbox_id()), b"Carol".to_vec())])
    );
}

/// An absent application immutable scalar takes one first value. Every later
/// write is refused, including a second Update in the same commit.
// verifies: META-004
#[xmtp_common::test(unwrap_try = true)]
async fn test_immutable_field_is_written_once() {
    const ONCE: ComponentId = ComponentId::new(0xFD01);
    const PAIR: ComponentId = ComponentId::new(0xFD02);
    tester!(alix, configured: |c| {
        c.application_components = [ONCE, PAIR]
            .map(|id| {
                definition(id.as_u16(), ComponentType::String, MetadataBasePolicy::Allow, true, false)
            })
            .into()
    });
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    write(&group, ONCE, b"first".to_vec()).await?;
    assert!(write(&group, ONCE, b"second".to_vec()).await.is_err());
    assert_peer_rejects(&group, &bo_group, vec![(ONCE, b"second".to_vec())]).await?;
    assert_eq!(value(&bo_group, ONCE)?, Some(b"first".to_vec()));

    let pair = vec![(PAIR, b"a".to_vec()), (PAIR, b"b".to_vec())];
    assert_peer_rejects(&group, &bo_group, pair).await?;
    assert_eq!(value(&bo_group, PAIR)?, None);
}

/// Publish `operation` on `id` as a signed standalone proposal with no
/// commit, the way a modified client can.
async fn propose_raw<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ComponentId,
    operation: AppDataUpdateOperation,
) -> Result<(), GroupError> {
    let signer = &group.context.identity().installation_keys;
    let message = state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let (message, _) = mls
                .propose_app_data_update(
                    &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                    signer,
                    id.as_u16(),
                    operation.clone(),
                )
                .map_err(GroupError::Proposal)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = group.prepare_group_messages(vec![(message.as_slice(), false)])?;
    group.context.api().send_group_messages(messages).await?;
    Ok(())
}

/// Pending proposals a member holds that no commit can carry together.
enum Uncommittable {
    /// Bo inserts its display name again.
    CommittedName,
    /// Bo inserts its absent display name, then a second one for the same key.
    PendingName,
    /// Bo proposes one insert of its absent display name twice. Both
    /// proposals have the same reference.
    Replay,
    /// Alix holds two copies of one insert, as a build before receive-time
    /// deduplication stored a replay.
    StoredTwice,
    /// Alix inserts an existing admin. Receivers refuse it, but Alix holds
    /// her own copy, which blocked her own commits.
    CommittedAdmin,
    /// Alix updates the description, then removes it: a list may not
    /// update and remove one component.
    UpdateThenRemove,
    /// Alix removes the absent image URL: a list may only remove a
    /// component with state.
    RemoveAbsent,
    /// Alix and Bo are super admins, and Alix removes Bo, then herself:
    /// together they leave none.
    NoSuperAdminLeft,
    /// Alix deletes the membership's registry entry. It applies, but the
    /// registry it leaves has no policy for a membership change.
    RegistryWithoutMembership,
}

/// Receivers judge a standalone `AppDataUpdate` alone against the committed
/// dictionary, so they accept proposals that no commit can carry together.
/// Every commit of pending proposals once carried them all and failed, so
/// no member could send or change the group again. A commit omits each
/// proposal it cannot carry instead, keeping the ones before it.
// verifies: GMOD-044, GMOD-045
#[rstest::rstest]
#[case::insert_of_committed_name(Uncommittable::CommittedName)]
#[case::second_pending_insert(Uncommittable::PendingName)]
#[case::replayed_insert(Uncommittable::Replay)]
#[case::insert_stored_twice(Uncommittable::StoredTwice)]
#[case::insert_of_committed_admin(Uncommittable::CommittedAdmin)]
#[case::update_then_remove(Uncommittable::UpdateThenRemove)]
#[case::remove_of_absent_component(Uncommittable::RemoveAbsent)]
#[case::no_super_admin_left(Uncommittable::NoSuperAdminLeft)]
#[case::registry_without_membership(Uncommittable::RegistryWithoutMembership)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_uncommittable_pending_app_data_does_not_block_the_group(
    #[case] uncommittable: Uncommittable,
    #[values(false, true)] field_write: bool,
) -> Result<(), GroupError> {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let update = |payload: Vec<u8>| AppDataUpdateOperation::Update(payload.into());
    let bo_name = || update(insert_name(bo.inbox_id(), b"Bo"));
    let remove_super_admin = |id: &str| {
        update(
            xmtp_mls_common::tls_set::TlsSetDelta::new()
                .remove(inbox(id))
                .tls_serialize_detached()
                .unwrap(),
        )
    };
    let names = ComponentId::USER_DISPLAY_NAME;
    // A member that holds the proposals, and how many.
    let (holder, proposed) = match uncommittable {
        Uncommittable::CommittedName => {
            write(&bo_group, names, insert_name(bo.inbox_id(), b"Bo")).await?;
            propose_raw(&bo_group, names, bo_name()).await?;
            (&group, 1)
        }
        Uncommittable::PendingName => {
            propose_raw(&bo_group, names, bo_name()).await?;
            let other = update(insert_name(bo.inbox_id(), b"Mallory"));
            propose_raw(&bo_group, names, other).await?;
            (&group, 2)
        }
        Uncommittable::Replay => {
            for _ in 0..2 {
                propose_raw(&bo_group, names, bo_name()).await?;
            }
            // Receivers store a replay once.
            (&group, 1)
        }
        Uncommittable::StoredTwice => {
            propose_raw(&bo_group, names, bo_name()).await?;
            group.sync().await?;
            state_write(group.context.mls_storage(), |tx| {
                tx.with_group(group.group_id, |mls, storage| {
                    let queued = mls.pending_proposals().next().cloned().unwrap();
                    mls.store_pending_proposal(storage, queued)?;
                    Ok::<_, GroupError>(Continue(()))
                })
            })?;
            (&group, 2)
        }
        Uncommittable::CommittedAdmin => {
            group
                .update_admin_list(UpdateAdminListType::Add, bo.inbox_id().to_string())
                .await?;
            let payload = xmtp_mls_common::tls_set::TlsSetDelta::new()
                .insert(inbox(bo.inbox_id()))
                .tls_serialize_detached()?;
            propose_raw(&group, ComponentId::ADMIN_LIST, update(payload)).await?;
            (&group, 1)
        }
        Uncommittable::UpdateThenRemove => {
            let description = ComponentId::GROUP_DESCRIPTION;
            propose_raw(&group, description, update(b"kept".to_vec())).await?;
            propose_raw(&group, description, AppDataUpdateOperation::Remove).await?;
            (&bo_group, 2)
        }
        Uncommittable::RemoveAbsent => {
            assert_eq!(value(&group, ComponentId::GROUP_IMAGE_URL)?, None);
            let remove = AppDataUpdateOperation::Remove;
            propose_raw(&group, ComponentId::GROUP_IMAGE_URL, remove).await?;
            (&bo_group, 1)
        }
        Uncommittable::NoSuperAdminLeft => {
            group
                .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
                .await?;
            let super_admins = ComponentId::SUPER_ADMIN_LIST;
            propose_raw(&group, super_admins, remove_super_admin(bo.inbox_id())).await?;
            propose_raw(&group, super_admins, remove_super_admin(alix.inbox_id())).await?;
            (&bo_group, 2)
        }
        Uncommittable::RegistryWithoutMembership => {
            let payload = TlsMapDelta::<ComponentId, VLBytes>::new()
                .delete(ComponentId::GROUP_MEMBERSHIP)
                .tls_serialize_detached()?;
            propose_raw(&group, ComponentId::COMPONENT_REGISTRY, update(payload)).await?;
            (&group, 1)
        }
    };
    holder.sync().await?;
    assert_eq!(
        holder.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        proposed,
        "the member holds each proposal"
    );

    // A field write reads the pending state before it queues its commit.
    if field_write {
        let rename = ComponentMutation::Replace(FieldValue::String("after".into()));
        group
            .update_metadata_field(&MetadataFieldRef::GROUP_NAME, &rename)
            .await?;
    }
    for member in [&group, &bo_group] {
        member.sync().await?;
        member.send_message(b"after", Default::default()).await?;
    }
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(
            member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
        if field_write {
            assert_eq!(member.group_name()?, "after");
        }
        match uncommittable {
            Uncommittable::CommittedAdmin => {
                assert_eq!(admins(member)?, vec![inbox(bo.inbox_id())]);
            }
            Uncommittable::UpdateThenRemove => assert_eq!(
                value(member, ComponentId::GROUP_DESCRIPTION)?,
                Some(b"kept".to_vec())
            ),
            Uncommittable::RemoveAbsent => {
                assert_eq!(value(member, ComponentId::GROUP_IMAGE_URL)?, None);
            }
            Uncommittable::NoSuperAdminLeft => assert_eq!(
                inboxes(member, ComponentId::SUPER_ADMIN_LIST)?,
                vec![inbox(alix.inbox_id())]
            ),
            Uncommittable::RegistryWithoutMembership => {
                assert!(member.with_group_snapshot(|mls| {
                    Ok(load_component_registry(mls)?
                        .get(&ComponentId::GROUP_MEMBERSHIP)
                        .unwrap()
                        .is_some())
                })?)
            }
            _ => assert_eq!(
                display_names(member)?,
                BTreeMap::from([(inbox(bo.inbox_id()), b"Bo".to_vec())])
            ),
        }
    }
    Ok(())
}

/// A build before receive-time deduplication could store one membership
/// delta twice. The membership selection omits the second copy, and
/// removing it by reference deleted the proposal both copies share, so
/// reloading the group failed and no member could send again. The
/// selection rebuilds the store instead.
#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_delta_stored_twice_does_not_block_the_group() -> Result<(), GroupError> {
    tester!(alix);
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.remove(0);
    propose_removal(&group, carol.inbox_id()).await?;
    state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let delta = mls
                .pending_proposals()
                .find(|queued| {
                    matches!(queued.proposal(), Proposal::AppDataUpdate(update)
                        if update.component_id() == ComponentId::GROUP_MEMBERSHIP.as_u16())
                })
                .cloned()
                .unwrap();
            mls.store_pending_proposal(storage, delta)?;
            Ok::<_, GroupError>(Continue(()))
        })
    })?;

    group.send_message(b"after", Default::default()).await?;
    bo_group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
    }
    Ok(())
}

/// Receivers check only that the leaf in a standalone Update proposal
/// names its proposer's inbox, and a commit checks that the leaf's key is
/// an installation of that inbox. Every commit of pending proposals carried
/// one with an unregistered key and was refused, so no member could send
/// again. No client proposes a leaf Update, so a commit never carries one.
// verifies: GMOD-044
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_leaf_update_does_not_block_the_group() -> Result<(), GroupError> {
    use openmls::prelude::{CredentialWithKey, LeafNodeParameters, NewSignerBundle};
    use xmtp_cryptography::XmtpInstallationCredential;
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let identity = bo_group.context.identity();
    let unregistered = XmtpInstallationCredential::new();
    let message = state_write(bo_group.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls, storage| {
            let new_signer = NewSignerBundle {
                signer: &unregistered,
                credential_with_key: CredentialWithKey {
                    credential: identity.credential(),
                    signature_key: unregistered.public_slice().into(),
                },
            };
            let (message, _) = mls
                .propose_self_update_with_new_signer(
                    &xmtp_db::XmtpOpenMlsProviderRef::new(storage),
                    &identity.installation_keys,
                    new_signer,
                    LeafNodeParameters::default(),
                )
                .unwrap();
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    let messages = bo_group.prepare_group_messages(vec![(message.as_slice(), false)])?;
    bo_group.context.api().send_group_messages(messages).await?;
    group.sync().await?;
    assert_eq!(
        group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        1
    );

    group.send_message(b"after", Default::default()).await?;
    bo_group.sync().await?;
    bo_group.send_message(b"after", Default::default()).await?;
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(
            member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    Ok(())
}

/// A receiver judges a standalone Delete of a non-member's display name
/// against the committed membership, which allows it. A pending proposal
/// that adds the owner back makes the owner a member after the commit,
/// where the Delete is refused. The commit judges each proposal against
/// the membership the kept proposals leave, so it omits the Delete.
// verifies: GMOD-044
#[xmtp_common::test(unwrap_try = true)]
async fn test_pending_readd_omits_a_delete_it_forbids() -> Result<(), GroupError> {
    tester!(alix);
    tester!(bo);
    tester!(carol);
    tester!(dave);
    let group = alix
        .create_group_with_members(
            &[bo.inbox_id(), carol.inbox_id(), dave.inbox_id()],
            None,
            None,
        )
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let carol_group = carol.wait_for_welcomes().await?.remove(0);
    let dave_group = dave.wait_for_welcomes().await?.remove(0);
    let names = ComponentId::USER_DISPLAY_NAME;
    write(&carol_group, names, insert_name(carol.inbox_id(), b"Carol")).await?;
    group.sync().await?;
    // A removal without cleanup leaves Carol's name behind.
    propose_removal(&group, carol.inbox_id()).await?;
    publish_unchecked(&group, vec![]).await?;
    for member in [&bo_group, &dave_group] {
        member.sync().await?;
    }

    propose_members(&bo_group, vec![carol.inbox_id().to_string()], vec![]).await?;
    let delete = TlsMapDelta::<InboxId, VLBytes>::new()
        .delete(inbox(carol.inbox_id()))
        .tls_serialize_detached()?;
    let delete = AppDataUpdateOperation::Update(delete.into());
    propose_raw(&dave_group, names, delete).await?;
    bo_group.sync().await?;
    assert_eq!(
        bo_group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
        3,
        "Bo holds the Add, its membership delta, and Dave's Delete"
    );

    bo_group.send_message(b"after", Default::default()).await?;
    dave_group.sync().await?;
    for member in [&bo_group, &dave_group] {
        assert_eq!(member.members().await?.len(), 4);
        assert_eq!(
            display_names(member)?,
            BTreeMap::from([(inbox(carol.inbox_id()), b"Carol".to_vec())])
        );
    }
    Ok(())
}

/// A standalone membership delta with no Add or Remove of its inbox.
enum Orphan {
    /// Bo inserts Carol, who is not a member.
    Insert,
    /// Alix deletes Bo, who stays in the tree.
    Delete,
}

/// Receivers check a standalone membership delta's policy and bounds, not
/// that an Add or Remove of its inbox comes with it. A commit that carries
/// one without them is refused (`ProposerNotFound`): the pre-publish check
/// failed every commit of pending proposals, and no member could send
/// again. The membership selection omits the delta instead.
// verifies: GMOD-038, GMOD-044
#[rstest::rstest]
#[case::insert(Orphan::Insert)]
#[case::delete(Orphan::Delete)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_orphan_membership_delta_does_not_block_the_group(
    #[case] orphan: Orphan,
) -> Result<(), GroupError> {
    use prost::Message as _;
    use xmtp_mls_common::app_data::{
        components::tls_map_components::GroupMembershipComponent, typed::Component,
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupMembershipEntry,
        group_membership_entry::{V1, Version},
    };
    tester!(alix);
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let (proposer, delta) = match orphan {
        Orphan::Insert => {
            let sequence_id = carol
                .context
                .db()
                .get_latest_sequence_id_for_inbox(carol.inbox_id())?
                as u64;
            let entry = GroupMembershipEntry {
                version: Some(Version::V1(V1 {
                    sequence_id,
                    failed_installations: vec![],
                })),
            };
            let delta = TlsMapDelta::<InboxId, VLBytes>::new()
                .insert(inbox(carol.inbox_id()), VLBytes::new(entry.encode_to_vec()));
            (&bo_group, delta)
        }
        Orphan::Delete => (
            &group,
            TlsMapDelta::<InboxId, VLBytes>::new().delete(inbox(bo.inbox_id())),
        ),
    };
    let payload = <GroupMembershipComponent as Component>::encode_mutation(&delta).unwrap();
    let update = AppDataUpdateOperation::Update(payload.into());
    propose_raw(proposer, ComponentId::GROUP_MEMBERSHIP, update).await?;

    for member in [&group, &bo_group] {
        member.sync().await?;
        member.send_message(b"after", Default::default()).await?;
    }
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(member.members().await?.len(), 2);
        assert_eq!(
            member.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?,
            0
        );
    }
    Ok(())
}

/// Omitting a proposal no commit can carry does not judge the rest before
/// publication: receivers judge them on read-back, and a rejection fails
/// only the intent that carried them. The omission once also ran the
/// membership selection's pre-publish check, whose refusal left the intent
/// to retry forever, so every later intent of that member waited behind it.
// verifies: GMOD-044
#[xmtp_common::test(unwrap_try = true)]
async fn test_omission_does_not_stall_a_conflicting_intent() -> Result<(), GroupError> {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    group.update_group_description("before".into()).await?;
    let bo_group = bo.wait_for_welcomes().await?.remove(0);
    let description = ComponentId::GROUP_DESCRIPTION;
    propose_raw(&group, description, AppDataUpdateOperation::Remove).await?;
    // Receivers accept both; a commit omits the second.
    for name in [b"Alix".as_slice(), b"Mallory"] {
        let insert = insert_name(alix.inbox_id(), name);
        let insert = AppDataUpdateOperation::Update(insert.into());
        propose_raw(&group, ComponentId::USER_DISPLAY_NAME, insert).await?;
    }
    bo_group.sync().await?;

    // A list may not remove and update one component, so receivers refuse
    // the commit that carries the Remove and Bo's own update.
    assert!(
        bo_group
            .update_group_description("after".into())
            .await
            .is_err()
    );
    bo_group.send_message(b"after", Default::default()).await?;
    group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(value(member, description)?, None);
        assert_eq!(
            display_names(member)?,
            BTreeMap::from([(inbox(alix.inbox_id()), b"Alix".to_vec())])
        );
    }
    Ok(())
}
