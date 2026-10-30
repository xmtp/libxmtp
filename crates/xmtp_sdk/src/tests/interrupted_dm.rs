use super::*;
use prost::Message as _;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_mls::groups::{MlsGroup, send_message_opts::SendMessageOpts};

type CoreGroup = MlsGroup<xmtp_mls::MlsContext>;

/// Two clients and a DM `alix` stored with `bo` whose creation stopped
/// before `bo` was added.
async fn interrupted_dm() -> (Client, Client, CoreGroup) {
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

/// Queues a text message with the core send, which does not add the peer.
fn plant(orphan: &CoreGroup, text: &str) -> MessageId {
    let bytes = TextCodec::encode(text.into()).unwrap().encode_to_vec();
    let id = orphan
        .send_message_optimistic(&bytes, SendMessageOpts::default())
        .unwrap();
    MessageId::from_bytes(&id).unwrap()
}

/// `bo` joins the DM with `alix` and holds the message `id`.
async fn assert_bo_received(alix: &Client, bo: &Client, id: MessageId) {
    bo.conversations().sync_all(None).await.unwrap();
    assert!(
        bo.conversations()
            .get_dm_by_inbox_id(alix.inbox_id())
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        bo.conversations()
            .get_message_by_id(id)
            .await
            .unwrap()
            .is_some()
    );
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

    assert_bo_received(&alix, &bo, id).await;
    alix.end().await?;
    bo.end().await?;
}

/// Deleting a message in a DM whose creation stopped before the peer was
/// added adds the peer before it queues the deletion, so the peer reads the
/// deletion. Both delete entry points share that step.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn delete_message_completes_an_interrupted_dm() {
    for through_dm in [false, true] {
        let (alix, bo, orphan) = interrupted_dm().await;
        let planted = plant(&orphan, "planted");

        let deletion = if through_dm {
            alix.conversations()
                .get_dm_by_inbox_id(bo.inbox_id())
                .await?
                .expect("stored DM")
                .delete_message(planted)
                .await?
        } else {
            alix.conversations().delete_message(planted).await?
        };
        orphan.publish_messages().await?;

        assert_bo_received(&alix, &bo, deletion).await;
        alix.end().await?;
        bo.end().await?;
    }
}

/// A retry of a failed message in a DM whose creation stopped before the
/// peer was added adds the peer before it queues the message again.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn publish_message_retry_completes_an_interrupted_dm() {
    use xmtp_db::group_intent::{IntentKind, QueryGroupIntent};

    let (alix, bo, orphan) = interrupted_dm().await;
    let planted = plant(&orphan, "retried");
    let db = alix.inner.context.db();
    let [intent] = db
        .find_group_intents(orphan.group_id, None, Some(vec![IntentKind::SendMessage]))?
        .try_into()
        .expect("one send intent");
    db.set_group_intent_error_and_fail_msg(&intent, Some(planted.to_bytes()?))?;

    alix.conversations()
        .get_dm_by_inbox_id(bo.inbox_id())
        .await?
        .expect("stored DM")
        .publish_message(planted.clone())
        .await?;

    assert_bo_received(&alix, &bo, planted).await;
    alix.end().await?;
    bo.end().await?;
}

/// A send that the client refuses locally does no network work for the DM
/// repair: a reserved transcript type fails as such even when the peer of an
/// interrupted DM cannot be added.
// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_send_is_refused_before_the_dm_repair() {
    use xmtp_content_types::group_updated::GroupUpdatedCodec;
    use xmtp_proto::xmtp::mls::message_contents::GroupUpdated;

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    // An inbox with no identity log has no installation to add.
    let peer = hex::encode(xmtp_cryptography::rand::rand_array::<32>());
    MlsGroup::create_dm_without_peer(&alix.inner.context, peer.clone())?;
    let dm = alix
        .conversations()
        .get_dm_by_inbox_id(InboxId::try_from(peer)?)
        .await?
        .expect("stored DM");

    let reserved = GroupUpdatedCodec::encode(GroupUpdated::default())?.try_into()?;
    let XmtpError::InvalidInput(refused) = dm.prepare_message(reserved, None).await.unwrap_err()
    else {
        panic!("expected the reserved content refusal");
    };
    assert_eq!(refused.code, "ReservedTranscriptContentType");
    alix.end().await?;
}

/// An invalid stored publication does no network work to repair an interrupted DM.
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_stored_publish_is_refused_before_dm_repair() {
    use xmtp_content_types::{
        group_updated::GroupUpdatedCodec,
        reply::{Reply, ReplyCodec},
    };
    use xmtp_db::{
        Store,
        group_intent::{IntentKind, QueryGroupIntent},
        group_message::{ContentType as DbContentType, QueryGroupMessage},
    };
    use xmtp_proto::{api::HasStats, xmtp::mls::message_contents::GroupUpdated};

    let (alix, bo, orphan) = interrupted_dm().await;
    let raw = ReplyCodec::encode(Reply {
        reference: "01".repeat(32),
        reference_inbox_id: None,
        content: GroupUpdatedCodec::encode(GroupUpdated::default())?,
    })?
    .encode_to_vec();
    assert!(raw.len() < 1024);
    let template = orphan.prepare_message_for_later_publish(
        &TextCodec::encode("legacy template".into())?.encode_to_vec(),
        false,
        Some("legacy-reserved-dm".into()),
    )?;
    let db = orphan.context.db();
    let mut message = db.get_group_message(&template)?.unwrap();
    message.id =
        xmtp_mls::utils::id::calculate_message_id(orphan.group_id, &raw, &message.idempotency_key);
    message.decrypted_message_bytes = raw;
    message.content_type = DbContentType::Reply;
    message.reference_id = Some(vec![1; 32]);
    message.store(&db)?;
    assert!(
        db.find_group_intents(orphan.group_id, None, Some(vec![IntentKind::SendMessage]))?
            .is_empty()
    );
    let members_before = orphan.members().await?.len();
    let dm = alix
        .conversations()
        .get_dm_by_inbox_id(bo.inbox_id())
        .await?
        .expect("stored DM");
    let published = || {
        alix.inner
            .context
            .api()
            .api_client
            .mls_stats()
            .publish
            .get_count()
    };
    let before = published();
    let error = dm
        .publish_message(MessageId::from_bytes(&message.id)?)
        .await
        .unwrap_err();
    let XmtpError::InvalidInput(refused) = error else {
        panic!("expected reserved refusal, got {error:?}");
    };
    assert_eq!(refused.code, "ReservedTranscriptContentType");
    let after = published();
    let members_after = orphan.members().await?.len();
    eprintln!(
        "requested application refused: {}; publications {} -> {}; members {} -> {}",
        refused.code, before, after, members_before, members_after
    );
    assert_eq!(
        after, before,
        "DM repair published before the reserved stored-message refusal"
    );
    assert_eq!(members_after, members_before);
    alix.end().await?;
    bo.end().await?;
}
