//! `USER_DISPLAY_NAME` and `GROUP_IMAGE` values, removed-member cleanup, and
//! the single write of an application immutable field.

use std::collections::BTreeMap;

use openmls::{
    component::ComponentData, framing::MlsMessageOut, group::MlsGroup as OpenMlsGroup,
    messages::proposals::AppDataUpdateOperation, prelude::CommitMessageBundle,
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
    app_data::component_id::ComponentId,
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
        app_data::{GroupAppDataError, pending_app_data_updates},
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
    Ok(value(group, ComponentId::ADMIN_LIST)?
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
    let intent = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![],
            vec![removed.to_string()],
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
    let dictionary = pending_app_data_updates(group).unwrap_or_else(|_| {
        let mut updater = group.app_data_dictionary_updater();
        for (id, payload) in updates {
            updater.set(ComponentData::from_parts(
                id.as_u16(),
                payload.clone().into(),
            ));
        }
        updater.changes()
    });
    let mut stage = group
        .commit_builder()
        .consume_proposal_store(true)
        .load_psks(provider.storage())?;
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
