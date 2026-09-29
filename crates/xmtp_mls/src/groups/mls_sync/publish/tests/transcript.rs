use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use xmtp_events::{ClientEvent, EventFilter, EventKind, MessageStatus};
use xmtp_proto::api::HasStats;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

fn transcript_content(type_id: &str, version_major: u32, authority: &str) -> Vec<u8> {
    use xmtp_proto::xmtp::mls::message_contents::ContentTypeId;
    EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: authority.into(),
            type_id: type_id.into(),
            version_major,
            version_minor: 3,
        }),
        content: b"fixture".to_vec(),
        ..Default::default()
    }
    .encode_to_vec()
}

/// A message row as a build from before the reserved-type check stored it.
fn legacy_stored_message<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    content: &[u8],
    key: &str,
) -> Result<Vec<u8>, GroupError> {
    use xmtp_db::Store;
    let template_id = group.prepare_message_for_later_publish(
        b"legacy fixture template",
        false,
        Some(format!("template-{key}")),
    )?;
    let db = group.context.db();
    let mut row = db.get_group_message(&template_id)?.unwrap();
    row.id = calculate_message_id(group.group_id, content, key);
    row.decrypted_message_bytes = content.to_vec();
    row.idempotency_key = key.into();
    row.store(&db)?;
    Ok(row.id)
}

/// An unprepared send intent as a build from before the reserved-type check queued it.
fn legacy_message_intent<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    content: &[u8],
    key: &str,
) -> Result<StoredGroupIntent, GroupError> {
    let envelope = PlaintextEnvelope {
        content: Some(Content::V1(V1 {
            content: content.to_vec(),
            idempotency_key: key.into(),
        })),
    };
    let data: Vec<u8> = SendMessageIntentData::new(envelope.encode_to_vec()).into();
    QueueIntent::send_message().data(data).queue(group)
}

fn status<Context: XmtpSharedContext>(group: &MlsGroup<Context>, id: &[u8]) -> DeliveryStatus {
    group
        .context
        .db()
        .get_group_message(id)
        .unwrap()
        .unwrap()
        .delivery_status
}

/// Every new send refuses both reserved types, under `xmtp.org` only, at any
/// version, before it stores a row, queues an intent, or publishes.
// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn transcript_types_are_never_sent() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count()
    };
    let intents = || {
        group
            .context
            .db()
            .find_group_intents(group.group_id, None, Some(IntentKind::all().collect()))
            .unwrap()
            .len()
    };
    for type_id in ["group_updated", "group_membership_change"] {
        for version in [1, 7] {
            let content = transcript_content(type_id, version, "xmtp.org");
            let messages = group.find_messages(&Default::default())?.len();
            let queued = intents();
            let published = publish_count();
            assert!(matches!(
                group
                    .send_message(&content, SendMessageOpts::default())
                    .await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(matches!(
                group.send_message_optimistic(&content, SendMessageOpts::default()),
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(matches!(
                group.prepare_message_for_later_publish(&content, false, None),
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert_eq!(publish_count(), published);
            assert_eq!(group.find_messages(&Default::default())?.len(), messages);
            assert_eq!(intents(), queued);
        }
    }

    // Only the outer type controls the check, even for compressed content.
    let mut compressed = EncodedContent::decode(
        transcript_content("group_membership_change", 7, "xmtp.org").as_slice(),
    )?;
    compressed.compression = Some(1);
    assert!(matches!(
        group
            .send_message(&compressed.encode_to_vec(), SendMessageOpts::default())
            .await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    // The type id alone is not reserved for another authority.
    let other_authority = transcript_content("group_updated", 1, "example.org");
    group
        .send_message(&other_authority, SendMessageOpts::default())
        .await?;
    // A reply that wraps a reserved type is not itself reserved.
    let mut reply = EncodedContent::decode(transcript_content("reply", 1, "xmtp.org").as_slice())?;
    reply.content = transcript_content("group_updated", 7, "xmtp.org");
    group
        .send_message(&reply.encode_to_vec(), SendMessageOpts::default())
        .await?;
}

/// A reserved row that an older build stored fails when it is published by
/// ID. It is never queued or sent, and a second attempt changes nothing.
// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn stored_reserved_message_fails_at_publish() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count()
    };
    let content = transcript_content("group_updated", 1, "xmtp.org");
    let stored_id = legacy_stored_message(&group, &content, "old-stored")?;
    let statuses = alix.context.events().subscribe(
        EventFilter::new([EventKind::MessageStatusChanged]),
        Some(10),
    );
    let published = publish_count();

    assert!(matches!(
        group.publish_stored_message(&stored_id).await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert_eq!(status(&group, &stored_id), DeliveryStatus::Failed);
    assert!(matches!(
        statuses.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::MessageStatusChanged(change)), ..
        }] if change.message_id == stored_id
            && change.previous == MessageStatus::Unpublished
            && change.current == MessageStatus::Failed
    ));

    assert!(matches!(
        group.publish_stored_message(&stored_id).await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert!(statuses.drain().is_empty());
    assert_eq!(publish_count(), published);
    assert!(
        group
            .context
            .db()
            .find_group_intents(
                group.group_id,
                Some(vec![IntentState::ToPublish]),
                Some(vec![IntentKind::SendMessage]),
            )?
            .is_empty()
    );
}

/// An unprepared reserved intent that an older build queued is failed with
/// its message, is not published, and does not block a later send.
// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn queued_reserved_intent_fails_without_blocking_later_sends() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count()
    };
    let content = transcript_content("group_membership_change", 1, "xmtp.org");
    let queued_id = legacy_stored_message(&group, &content, "old-queued")?;
    let intent = legacy_message_intent(&group, &content, "old-queued")?;
    let published = publish_count();

    assert!(group.publish_intents().await.is_err());
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Error);
    assert_eq!(status(&group, &queued_id), DeliveryStatus::Failed);
    assert_eq!(publish_count(), published);

    let later = group
        .send_message(b"later message", SendMessageOpts::default())
        .await?;
    assert_eq!(status(&group, &later), DeliveryStatus::Published);
}

/// A reserved row whose send intent an older build already prepared keeps
/// its unknown outcome: `publish_stored_message` neither fails the row nor
/// reports a refusal, because the saved bytes may already be on the backend
/// and SEND-007 may publish them.
// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn stored_reserved_message_with_saved_attempt_is_not_refused() {
    use xmtp_db::diesel::prelude::*;
    use xmtp_db::{ConnectionExt, schema::group_intents::dsl};

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let key = "old-saved";
    // Save a real prepared attempt, then give its intent the reserved
    // envelope that an older build would have queued. The creation and
    // preparation checks refuse to build one directly.
    let intent = legacy_message_intent(&group, b"ordinary bytes", key)?;
    let requirements = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, _| {
            PublishRequirements::capture(mls_group, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    group
        .prepare_publish_attempt(&requirements, &mut dependencies)?
        .expect("a saved attempt");
    let content = transcript_content("group_updated", 1, "xmtp.org");
    let stored_id = legacy_stored_message(&group, &content, key)?;
    let reserved = PlaintextEnvelope {
        content: Some(Content::V1(V1 {
            content: content.clone(),
            idempotency_key: key.into(),
        })),
    };
    let data: Vec<u8> = SendMessageIntentData::new(reserved.encode_to_vec()).into();
    group.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_intents.filter(dsl::id.eq(intent.id)))
            .set(dsl::data.eq(&data))
            .execute(conn)
    })?;

    let result = group.publish_stored_message(&stored_id).await;
    assert!(
        !matches!(result, Err(GroupError::ReservedTranscriptContentType)),
        "a saved attempt was reported as a definite refusal"
    );
    assert_ne!(status(&group, &stored_id), DeliveryStatus::Failed);
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_ne!(current.state, IntentState::Error);
}
