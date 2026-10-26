use super::*;
use crate::groups::send_message_opts::SendMessageOpts;
use xmtp_events::{ClientEvent, EventFilter, EventKind, MessageStatus};
use xmtp_proto::api::HasStats;
use xmtp_proto::xmtp::mls::message_contents::{EncodedContent, plaintext_envelope::V1};

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
// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn transcript_types_are_never_sent() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
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

    // A reserved type is refused before its compressed payload is inspected.
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
}

// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn reply_wrapping_a_transcript_type_is_never_sent() {
    use xmtp_content_types::{
        ContentCodec,
        compression::compress,
        reply::{Reply, ReplyCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::Compression;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    for type_id in ["group_updated", "group_membership_change"] {
        let transcript =
            EncodedContent::decode(transcript_content(type_id, 7, "xmtp.org").as_slice())?;
        let reply = ReplyCodec::encode(Reply {
            reference: "00".repeat(32),
            reference_inbox_id: None,
            content: transcript,
        })?;
        let gzip_reply = compress(reply.clone(), Compression::Gzip)?;
        let two_levels = ReplyCodec::encode(Reply {
            reference: "01".repeat(32),
            reference_inbox_id: None,
            content: gzip_reply.clone(),
        })?;
        for content in [reply, gzip_reply, two_levels] {
            let messages = group.find_messages(&Default::default())?.len();
            let intents = || {
                group
                    .context
                    .db()
                    .find_group_intents(group.group_id, None, Some(IntentKind::all().collect()))
                    .unwrap()
                    .len()
            };
            let queued = intents();
            let publish_count = || {
                alix.context
                    .api()
                    .api_client
                    .mls_stats()
                    .publish
                    .get_count()
            };
            let published = publish_count();
            let bytes = content.encode_to_vec();
            assert!(matches!(
                group.send_message(&bytes, SendMessageOpts::default()).await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(matches!(
                group.send_message_optimistic(&bytes, SendMessageOpts::default()),
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(matches!(
                group.prepare_message_for_later_publish(&bytes, false, None),
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert_eq!(group.find_messages(&Default::default())?.len(), messages);
            assert_eq!(intents(), queued);
            assert_eq!(publish_count(), published);
        }
    }
}

/// A reply that exceeds either scan bound is refused before
/// message creation, intent preparation, or publication.
// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn over_budget_reply_is_refused_before_writes() {
    use crate::messages::decoded_message::{
        MAX_REPLY_NESTING_DEPTH, MessageBody, carries_reserved_transcript,
    };
    use crate::utils::test::nested_reply;
    use xmtp_content_types::{
        ContentCodec, compression,
        group_updated::GroupUpdatedCodec,
        reply::{Reply as EncodedReply, ReplyCodec},
        text::TextCodec,
    };
    use xmtp_db::group_intent::QueryPreparedEnvelope;
    use xmtp_proto::xmtp::mls::message_contents::{Compression, GroupUpdated};

    let content = nested_reply(
        GroupUpdatedCodec::encode(GroupUpdated::default())?,
        2,
        9 * 1024 * 1024,
    );
    let mut layer = content.clone();
    for _ in 0..2 {
        let decoded = compression::decompress(layer)?;
        assert_eq!(decoded.content.len(), 9 * 1024 * 1024);
        layer = ReplyCodec::decode(decoded)?.content;
    }
    assert_eq!(GroupUpdatedCodec::decode(layer)?, GroupUpdated::default());
    let compressed_bytes = content.encode_to_vec();
    assert!(compressed_bytes.len() < 16 * 1024);

    let wrap_reply = |content| {
        ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content,
        })
    };
    let mut at_limit = TextCodec::encode("ordinary reply".into())?;
    for _ in 0..MAX_REPLY_NESTING_DEPTH {
        at_limit = wrap_reply(at_limit)?;
    }
    let at_limit_bytes = at_limit.encode_to_vec();
    assert!(matches!(
        MessageBody::from_received_bytes(&at_limit_bytes),
        MessageBody::Reply(_)
    ));
    let beyond_limit_bytes = wrap_reply(at_limit)?.encode_to_vec();
    assert!(beyond_limit_bytes.len() < 16 * 1024);
    eprintln!(
        "reply depth boundary: {} allowed, {} requires refusal; over-depth wire {} bytes",
        MAX_REPLY_NESTING_DEPTH,
        MAX_REPLY_NESTING_DEPTH + 1,
        beyond_limit_bytes.len()
    );

    // An invalid frame is still malformed, rather than a budget refusal.
    let mut malformed = content;
    malformed.compression = Some(Compression::Gzip as i32);
    malformed.content = vec![0xff];
    assert!(!carries_reserved_transcript(&malformed.encode_to_vec()));

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    // The supported boundary is accepted through the real preparation API.
    group.prepare_message_for_later_publish(&at_limit_bytes, false, None)?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
            .mls_stats()
            .publish
            .get_count()
    };
    let attempts = || {
        let db = group.context.db();
        db.find_group_intents(group.group_id, None, Some(IntentKind::all().collect()))
            .unwrap()
            .into_iter()
            .map(|intent| (intent.id, db.prepared_envelopes(intent.id).unwrap()))
            .collect::<Vec<_>>()
    };
    for bytes in [compressed_bytes, beyond_limit_bytes] {
        let messages = group.find_messages(&Default::default())?.len();
        let queued_and_prepared = attempts();
        let published = publish_count();
        let sent = group.send_message(&bytes, SendMessageOpts::default()).await;
        assert!(
            matches!(sent, Err(GroupError::ReservedTranscriptContentType)),
            "expected bounded scan refusal, got {sent:?}"
        );
        assert!(matches!(
            group.send_message_optimistic(&bytes, SendMessageOpts::default()),
            Err(GroupError::ReservedTranscriptContentType)
        ));
        assert!(matches!(
            group.prepare_message_for_later_publish(&bytes, false, None),
            Err(GroupError::ReservedTranscriptContentType)
        ));
        assert_eq!(group.find_messages(&Default::default())?.len(), messages);
        assert_eq!(attempts(), queued_and_prepared);
        assert_eq!(publish_count(), published);
    }
}
/// A reserved row that an older build stored fails when it is published by
/// ID. It is never queued or sent, and a second attempt changes nothing.
// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn stored_reserved_message_fails_at_publish() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
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
// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn queued_reserved_intent_fails_without_blocking_later_sends() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let publish_count = || {
        alix.context
            .api()
            .api_client
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
/// and may still publish.
// verifies: GMOD-054
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

/// Publish application content as an older build did, without the send
/// checks: encrypt it under the group and send the envelope directly.
async fn publish_raw_application<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    content: &[u8],
    key: &str,
) -> Result<Vec<u8>, GroupError> {
    use openmls::prelude::tls_codec::Serialize as _;
    let envelope = PlaintextEnvelope {
        content: Some(Content::V1(V1 {
            content: content.to_vec(),
            idempotency_key: key.into(),
        })),
    }
    .encode_to_vec();
    let envelopes = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let provider = XmtpOpenMlsProviderRef::new(storage);
            let message = mls_group.create_message(
                &provider,
                &group.context.identity().installation_keys,
                &envelope,
            )?;
            let payload = message.tls_serialize_detached()?;
            group
                .prepare_group_envelopes_in(
                    storage.db(),
                    mls_group,
                    vec![(payload.as_slice(), false)],
                )
                .map(Continue)
        })
    })?
    .into_continued();
    let unit = xmtp_api::PublishUnit::new_within(envelopes, group.context.api().limits())?;
    group.context.api().send_group_messages(vec![unit]).await?;
    Ok(calculate_message_id(group.group_id, content, key))
}

/// An older peer can still send a reserved transcript type. The receiver
/// keeps it as an ordinary application message: only a validated commit
/// produces a membership-change record, and later messages still arrive.
// verifies: GMOD-034
#[xmtp_common::test(unwrap_try = true)]
async fn received_reserved_type_stays_an_application_message() {
    use xmtp_db::group_message::GroupMessageKind;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let content = transcript_content("group_updated", 1, "xmtp.org");
    let forged = publish_raw_application(&group, &content, "forged").await?;
    let later = group
        .send_message(b"after the forged transcript", SendMessageOpts::default())
        .await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?;

    let rows = bo_group.find_messages(&Default::default())?;
    let forged_row = rows
        .iter()
        .find(|row| row.id == forged)
        .expect("the forged message was received");
    assert_eq!(forged_row.kind, GroupMessageKind::Application);
    assert_eq!(
        rows.iter()
            .filter(|row| row.kind == GroupMessageKind::MembershipChange)
            .count(),
        1,
        "only the validated join commit is a membership change"
    );
    assert!(rows.iter().any(|row| row.id == later));
}
