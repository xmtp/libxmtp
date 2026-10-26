use super::*;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_mls::groups::MlsGroup;

/// Two clients and a DM `alix` stored with `bo` whose creation stopped
/// before `bo` was added.
async fn interrupted_dm() -> (Client, Client, MlsGroup<xmtp_mls::MlsContext>) {
    let alix = Client::create(crate::generate_local_signer().await, options())
        .await
        .unwrap();
    let bo = Client::create(crate::generate_local_signer().await, options())
        .await
        .unwrap();
    let orphan =
        MlsGroup::create_dm_without_peer(&alix.inner.context, bo.inner.inbox_id().to_string())
            .unwrap();
    (alix, bo, orphan)
}

/// An optimistic send in a DM whose creation stopped before the peer was
/// added adds the peer first, so the peer joins and reads the message.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn prepare_message_completes_an_interrupted_dm() {
    let (alix, bo, _orphan) = interrupted_dm().await;
    // Reading the DM does not repair it.
    let dm = alix
        .conversations()
        .get_dm_by_inbox_id(bo.inbox_id())
        .await?
        .expect("stored DM");

    let id = dm
        .prepare_message(TextCodec::encode("prepared".into())?.try_into()?, None)
        .await?;
    dm.publish_messages().await?;

    bo.conversations().sync_all(None).await?;
    assert!(
        bo.conversations()
            .get_dm_by_inbox_id(alix.inbox_id())
            .await?
            .is_some()
    );
    assert!(bo.conversations().get_message_by_id(id).await?.is_some());
    alix.end().await?;
    bo.end().await?;
}

/// Deleting a message in a DM whose creation stopped before the peer was
/// added adds the peer first, so the peer joins the DM.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn delete_message_completes_an_interrupted_dm() {
    use prost::Message as _;
    use xmtp_mls::groups::send_message_opts::SendMessageOpts;

    let (alix, bo, orphan) = interrupted_dm().await;
    // The core send queues the message without adding the peer.
    let planted = orphan.send_message_optimistic(
        &TextCodec::encode("planted".into())?.encode_to_vec(),
        SendMessageOpts::default(),
    )?;

    alix.conversations()
        .delete_message(MessageId::from_bytes(&planted)?)
        .await?;
    orphan.publish_messages().await?;

    bo.conversations().sync_all(None).await?;
    assert!(
        bo.conversations()
            .get_dm_by_inbox_id(alix.inbox_id())
            .await?
            .is_some()
    );
    alix.end().await?;
    bo.end().await?;
}
