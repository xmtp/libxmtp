//! `USER_DISPLAY_NAME` and `GROUP_IMAGE` values, removed-member cleanup, and
//! the single write of an application immutable field.

use crate::groups::test_support::{EnginePeer, EngineTestPeer, EngineTestPeerOps};
use crate::prelude::*;
use std::collections::BTreeMap;

use openmls::{
    component::ComponentData,
    extensions::AppDataDictionary,
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
    group_intent::IntentState,
    group_message::MsgQueryArgs,
    incoming_envelope::{StreamTopic, TerminalRejection},
    prelude::*,
};
use xmtp_mls_group_state::{
    app_data::{
        component_id::ComponentId,
        fields::{
            ComponentMutation, FieldError, FieldKey, FieldValue, MapMutation, MetadataFieldRef,
        },
    },
    inbox_id::InboxId,
};
use xmtp_mls_tls_collections::{
    tls_map::{TlsMap, TlsMapDelta},
    tls_set::TlsSet,
};
use xmtp_proto::xmtp::mls::message_contents::{ComponentType, metadata_policy::MetadataBasePolicy};

use super::test_dictionary_creation::{definition, value};
use crate::groups::{
    GroupError, MlsGroup, UpdateAdminListType,
    app_data::{AppDataBatch, GroupAppDataError, load_component_registry},
    context::XmtpSharedContext,
    intents::{AppDataUpdateIntentData, ProposeMemberUpdateIntentData, QueueIntent},
    mls_sync::generate_prepared_commit,
};
use xmtp_mls_core::state_tx::state_write;

mod dictionary_bounds;
mod pending_app_data;
mod profile_values;
mod removal_cleanup;

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

fn dictionary<C: XmtpSharedContext>(group: &MlsGroup<C>) -> Result<AppDataDictionary, GroupError> {
    group.with_group_snapshot(|group| {
        Ok(group
            .extensions()
            .app_data_dictionary()
            .unwrap()
            .dictionary()
            .clone())
    })
}
