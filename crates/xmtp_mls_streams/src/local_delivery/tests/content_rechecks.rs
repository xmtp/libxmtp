//! Local delivery tests: content rechecks.

use super::*;

// verifies: PROC-026
#[xmtp_common::test(unwrap_try = true)]
async fn missed_local_wake_is_recovered_by_a_fresh_database_poll() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig {
            poll_interval: Duration::from_millis(20),
            ..Default::default()
        },
    )?;
    assert!(reader.next_delivery().now_or_never().is_none());
    let message = generate_stored_msg(Cursor(100), group.group_id);
    // Direct storage deliberately emits no process-local wake event.
    message.store(&alix.context.db())?;
    let item = timeout(Duration::from_secs(2), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(item.message.id, message.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn queued_content_is_rechecked_after_deletion_and_restore() {
    use diesel::{QueryDsl, RunQueryDsl};
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let start = alix.context.db().current_delivery_cursor()?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        Some(start),
        LocalDeliveryConfig::default(),
    )?;
    let removed = reader.next_delivery().await?.unwrap();
    alix.context.db().raw_query(|conn| {
        diesel::delete(xmtp_db::schema::group_messages::table.find(&first.id)).execute(conn)
    })?;
    assert!(matches!(
        removed.acknowledgement.check_owner(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    drop(removed);
    let retained = reader.next_delivery().await?.unwrap();
    assert_eq!(retained.message.id, second.id);
    alix.context.db().rotate_stream_database_id()?;
    let error = retained.acknowledgement.check_owner().unwrap_err();
    assert!(matches!(
        &error,
        LocalDeliveryError::SessionFailure(cause)
            if matches!(cause.as_ref(), LocalDeliveryError::Storage(StorageError::Stream(
                xmtp_db::stream_storage::StreamStorageError::ForeignCursor
            )))
    ));
    assert_eq!(
        error.error_code(),
        xmtp_db::stream_storage::StreamStorageError::ForeignCursor.error_code()
    );
}

/// A row marked deleted after prefetch is handed out as its current, marked
/// row, never the cached content.
// verifies: PROC-025, PROC-060
#[xmtp_common::test(unwrap_try = true)]
async fn buffered_stream_sees_deletions() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let start = alix.context.db().current_delivery_cursor()?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        Some(start),
        LocalDeliveryConfig {
            batch_size: 2,
            ..Default::default()
        },
    )?;
    let handed = reader.next_delivery().await?.unwrap();
    assert_eq!(handed.message.id, first.id);
    handed.acknowledgement.check_owner()?;
    handed.acknowledgement.acknowledge()?;
    alix.context
        .db()
        .mark_message_deleted(&second.id, &second.sender_inbox_id)?;
    let item = reader.next_delivery().await?.unwrap();
    assert_eq!(item.message.id, second.id);
    assert_eq!(
        item.message.deleted_by_inbox_id.as_deref(),
        Some(second.sender_inbox_id.as_str())
    );
    assert!(item.message.decrypted_message_bytes.is_empty());
}

// verifies: CTYPE-008, PROC-045
#[xmtp_common::test(unwrap_try = true)]
async fn queued_undecodable_content_is_delivered_and_the_stream_continues() {
    use prost::Message;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    use xmtp_mls_messages::decoded_message::{ContentDecodeFailureKind, MessageBody};
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    // The mock row's bytes are not a serialized EncodedContent.
    let malformed = generate_stored_msg(Cursor(100), group.group_id);
    malformed.store(&alix.context.db())?;
    let mut valid = generate_stored_msg(Cursor(200), group.group_id);
    valid.decrypted_message_bytes = TextCodec::encode("after".into())?.encode_to_vec();
    valid.store(&alix.context.db())?;
    let mut reader = MessageReader::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert_eq!(item.message.id, malformed.id);
    let enriched = item.acknowledgement.enriched_message()?;
    assert_eq!(enriched.metadata.id, malformed.id);
    assert_eq!(enriched.metadata.content_type, None);
    let MessageBody::Undecodable(undecodable) = enriched.content else {
        panic!("expected undecodable content, got {:?}", enriched.content);
    };
    assert_eq!(undecodable.raw_bytes, malformed.decrypted_message_bytes);
    assert_eq!(
        undecodable.failure.kind,
        ContentDecodeFailureKind::MalformedEnvelope
    );
    item.acknowledgement.acknowledge()?;
    let next = timeout(Duration::from_secs(5), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(next.message.id, valid.id);
    assert!(matches!(
        next.acknowledgement.enriched_message()?.content,
        MessageBody::Text(text) if text.content == "after"
    ));
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn enrichment_does_not_dispatch_and_stale_selection_remains_nonterminal() {
    use prost::Message;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let mut message = generate_stored_msg(Cursor(100), group.group_id);
    message.decrypted_message_bytes =
        TextCodec::encode("queued message".to_string())?.encode_to_vec();
    message.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert_eq!(
        item.acknowledgement.enriched_message()?.metadata.id,
        message.id
    );
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    assert!(matches!(
        item.acknowledgement.enriched_message(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    assert!(!reader.session.is_closed());
    reader.control().update_scope(DeliveryScope::All);
    assert_eq!(
        reader.next_delivery().await?.unwrap().message.id,
        message.id
    );
}

// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn queued_message_that_expires_before_enrichment_is_not_delivered() {
    use prost::Message;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let mut expiring = generate_stored_msg(Cursor(100), group.group_id);
    expiring.decrypted_message_bytes = TextCodec::encode("expiring".into())?.encode_to_vec();
    expiring.expire_at_ns = Some(i64::MAX);
    expiring.store(&alix.context.db())?;
    let mut retained = generate_stored_msg(Cursor(200), group.group_id);
    retained.decrypted_message_bytes = TextCodec::encode("retained".into())?.encode_to_vec();
    retained.store(&alix.context.db())?;

    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let selected = reader.next_delivery().await?.unwrap();
    assert_eq!(selected.message.id, expiring.id);
    use diesel::prelude::*;
    use xmtp_db::schema::group_messages::dsl as messages;
    alix.context.db().raw_query(|conn| {
        diesel::update(messages::group_messages.find(&expiring.id))
            .set(messages::expire_at_ns.eq(Some(xmtp_common::time::now_ns() - 1)))
            .execute(conn)
    })?;
    assert!(alix.context.db().get_group_message(&expiring.id)?.is_some());
    assert!(matches!(
        selected.acknowledgement.enriched_message(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    assert!(!reader.session.is_closed());
    drop(selected);
    let next = reader.next_delivery().await?.unwrap();
    assert_eq!(next.message.id, retained.id);
    assert_eq!(
        next.acknowledgement.enriched_message()?.metadata.id,
        retained.id
    );
}
