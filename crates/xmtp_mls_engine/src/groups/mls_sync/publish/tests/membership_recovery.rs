//! Publication after rejected membership changes.

use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use xmtp_db::{
    ConnectionExt, group_message::MsgQueryArgs, key_package_history::QueryKeyPackageHistory,
};
use xmtp_mls_validation::commit::CommitRuleError;

mod guarded_commits;
mod membership_deltas;
mod pending_adds;
mod rejected_removals;

// Receive the prefix that the sender processed, including rejected commits.
pub(super) async fn receive_completed_prefix(
    sender: &EngineTestGroup,
    receiver: &EngineTestGroup,
) -> Result<(), GroupError> {
    let target = sender
        .context
        .db()
        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
            sender.group_id,
        ))?
        .processed;
    crate::subscriptions::barrier::wait_through(
        &receiver.context,
        [(Topic::new_group_message(sender.group_id), target)].into(),
        None,
    )
    .await
    .expect("receiver must process the sender's completed prefix");
    Ok(())
}

// A publication receipt can precede QueryNewest visibility.
async fn wait_for_rotated_package(
    group: &EngineTestGroup,
    installation: xmtp_proto::types::InstallationId,
    previous: &KeyPackage,
) -> KeyPackage {
    xmtp_common::wait_for_some(|| async {
        let fetched = group
            .context
            .api()
            .fetch_key_packages(&[installation])
            .await
            .expect("fetch the rotated key package");
        let package = fetched.get(&installation)?.as_ref()?;
        let current = xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(
            &openmls_rust_crypto::RustCrypto::default(),
            &package.key_package_tls_serialized,
        )
        .expect("verify the rotated key package")
        .inner;
        (&current != previous).then_some(current)
    })
    .await
    .expect("the rotated key package must become visible")
}

pub(super) fn committed_membership(group: &EngineTestGroup) -> GroupMembership {
    group
        .with_group_snapshot(|mls| Ok(extract_group_membership(mls.extensions())?))
        .expect("read the committed membership")
}

pub(super) fn has_leaf(group: &EngineTestGroup, installation: &[u8]) -> bool {
    group
        .with_group_snapshot(|mls| {
            Ok(mls
                .members()
                .any(|member| member.signature_key == installation))
        })
        .expect("read the ratchet tree")
}

// Seed the standalone Add whose recovery is under test. Membership refreshes
// carry this leaf change inline, so a rejected refresh no longer leaves it.
async fn publish_installation_add(
    author: &EngineTestGroup,
    installation: &[u8],
    peers: &[&EngineTestGroup],
) -> Result<(), GroupError> {
    let package = MlsStore::new(author.context.clone())
        .get_key_packages_for_installation_ids(vec![installation.to_vec()])
        .await?
        .remove(installation)
        .expect("installation has a key package")
        .expect("key package verifies")
        .inner;
    let payload = xmtp_mls_core::state_tx::state_write(author.context.mls_storage(), |tx| {
        tx.with_group(author.group_id, |mls, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let (message, _) = mls
                .propose_add_member(
                    &provider,
                    &author.context.identity().installation_keys,
                    &package,
                )
                .map_err(GroupError::ProposeAddMember)?;
            Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
        })
    })?
    .into_continued();
    publish_standalone_proposal(author, payload, peers).await
}

// Publish one signed standalone proposal and wait until each peer processed it.
pub(super) async fn publish_standalone_proposal(
    author: &EngineTestGroup,
    payload: Vec<u8>,
    peers: &[&EngineTestGroup],
) -> Result<(), GroupError> {
    let messages = author.prepare_group_messages(vec![(payload.as_slice(), false)])?;
    let receipts = author.context.api().send_group_messages(messages).await?;
    let topic = Topic::new_group_message(author.group_id);
    let (_, cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    for peer in peers {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(topic.clone(), cursor)].into(),
            None,
        )
        .await?;
    }
    Ok(())
}

// Sign a standalone membership proposal without local checks. Each peer's
// admission decides whether to store it.
pub(super) fn propose_membership_payload(
    author: &EngineTestGroup,
    payload: Vec<u8>,
) -> Result<Vec<u8>, GroupError> {
    use openmls::messages::proposals::AppDataUpdateOperation;
    use xmtp_mls_group_state::app_data::component_id::ComponentId;
    Ok(
        xmtp_mls_core::state_tx::state_write(author.context.mls_storage(), |tx| {
            tx.with_group(author.group_id, |mls, storage| {
                let provider = XmtpOpenMlsProviderRef::new(storage);
                let (message, _) = mls
                    .propose_app_data_update(
                        &provider,
                        &author.context.identity().installation_keys,
                        ComponentId::GROUP_MEMBERSHIP.as_u16(),
                        AppDataUpdateOperation::Update(payload.into()),
                    )
                    .map_err(GroupError::Proposal)?;
                Ok::<_, GroupError>(Continue(message.tls_serialize_detached()?))
            })
        })?
        .into_continued(),
    )
}
