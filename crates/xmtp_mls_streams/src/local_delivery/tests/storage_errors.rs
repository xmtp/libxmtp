//! Local delivery tests: storage errors.

use super::*;

xmtp_common::if_native! {
// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn read_storage_error_ends_iterator_once_and_new_reader_replays_on_same_client() {
    let alix = EngineTestPeer::persistent().await;
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || LocalDelivery::new(alix.context.clone(), DeliveryScope::All,
        LocalDeliveryFilter::default(), None, LocalDeliveryConfig::default());
    let mut stream = Box::pin(create()?.into_stream());
    alix.context.db().disconnect()?;
    assert!(stream.next().await.unwrap().is_err());
    assert!(stream.next().await.is_none());
    assert!(!alix.context.is_closed());
    alix.context.db().reconnect()?;
    let mut replacement = create()?;
    assert_eq!(replacement.next_delivery().await?.unwrap().message.id, message.id);
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn queued_enrichment_storage_error_is_terminal_and_reopen_retains_the_item() {
    use prost::Message;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    let alix = EngineTestPeer::persistent().await;
    let group = alix.create_group(None, None)?;
    let mut message = generate_stored_msg(Cursor(100), group.group_id);
    message.decrypted_message_bytes = TextCodec::encode("queued message".to_string())?.encode_to_vec();
    message.store(&alix.context.db())?;
    let create = || MessageReader::new(alix.context.clone(), DeliveryScope::All,
        LocalDeliveryFilter::default(), None);
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    let mut pending = Box::pin(reader.next_delivery());
    assert!(pending.as_mut().now_or_never().is_none());
    alix.context.db().disconnect()?;
    let expected = alix.context.db().stream_database_id().unwrap_err();
    let error = item.acknowledgement.enriched_message().unwrap_err();
    let LocalDeliveryError::SessionFailure(original) = &error else { panic!("Expected the shared storage failure") };
    assert!(matches!(original.as_ref(), LocalDeliveryError::Storage(_)));
    assert_eq!(error.error_code(), expected.error_code());
    assert_eq!(error.to_string(), expected.to_string());
    let Err(reader_error) = timeout(Duration::from_secs(1), pending).await? else { panic!("The pending reader lost its storage failure") };
    assert!(matches!(reader_error, LocalDeliveryError::SessionFailure(cause) if Arc::ptr_eq(original, &cause)));
    assert!(reader.next_delivery().await?.is_none());
    alix.context.db().reconnect()?;
    assert!(matches!(item.acknowledgement.enriched_message(), Err(LocalDeliveryError::SessionFailure(cause)) if Arc::ptr_eq(original, &cause)));
    assert!(matches!(item.acknowledgement.acknowledge(), Err(LocalDeliveryError::SessionFailure(cause)) if Arc::ptr_eq(original, &cause)));
    let mut replacement = create()?;
    let replay = replacement.next_delivery().await?.unwrap();
    assert_eq!(replay.cursor, item.cursor);
    assert_eq!(replay.acknowledgement.enriched_message()?.metadata.id, message.id);
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn final_owner_check_storage_error_closes_the_reader_without_advancing_delivery() {
    let alix = EngineTestPeer::persistent().await;
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || MessageReader::new(alix.context.clone(), DeliveryScope::All,
        LocalDeliveryFilter::default(), None);
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    let mut pending = Box::pin(reader.next_delivery());
    assert!(pending.as_mut().now_or_never().is_none());
    alix.context.db().disconnect()?;
    let expected = alix.context.db().stream_database_id().unwrap_err();
    let error = item.acknowledgement.check_owner().unwrap_err();
    let LocalDeliveryError::SessionFailure(original) = &error else { panic!("Expected the shared storage failure") };
    assert!(matches!(original.as_ref(), LocalDeliveryError::Storage(_)));
    assert_eq!(error.error_code(), expected.error_code());
    assert_eq!(error.to_string(), expected.to_string());
    // Cleanup after token failure must not replace its cause with rejection.
    item.acknowledgement.reject();
    let Err(reader_error) = timeout(Duration::from_secs(1), pending).await? else { panic!("The pending reader lost its storage failure") };
    assert!(matches!(reader_error, LocalDeliveryError::SessionFailure(cause) if Arc::ptr_eq(original, &cause)));
    assert!(reader.next_delivery().await?.is_none());
    alix.context.db().reconnect()?;
    let mut replacement = create()?;
    assert!(matches!(item.acknowledgement.acknowledge(), Err(LocalDeliveryError::SessionFailure(cause)) if Arc::ptr_eq(original, &cause)));
    reader.close();
    assert!(create().is_err());
    let replay = replacement.next_delivery().await?.unwrap();
    assert_eq!(replay.cursor, item.cursor);
    assert_eq!(replay.message.id, message.id);
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn acknowledgement_storage_error_reaches_the_pending_reader_and_replays() {
    let alix = EngineTestPeer::persistent().await;
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || MessageReader::new(alix.context.clone(), DeliveryScope::All,
        LocalDeliveryFilter::default(), None);
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    let mut pending = Box::pin(reader.next_delivery());
    assert!(pending.as_mut().now_or_never().is_none());
    alix.context.db().disconnect()?;
    let expected = alix.context.db().stream_database_id().unwrap_err();
    let error = item.acknowledgement.acknowledge().unwrap_err();
    let LocalDeliveryError::SessionFailure(original) = &error else { panic!("Expected the shared storage failure") };
    assert!(matches!(original.as_ref(), LocalDeliveryError::Storage(_)));
    assert_eq!(error.error_code(), expected.error_code());
    assert_eq!(error.to_string(), expected.to_string());
    let Err(reader_error) = timeout(Duration::from_secs(1), pending).await? else { panic!("The pending reader lost its storage failure") };
    assert!(matches!(reader_error, LocalDeliveryError::SessionFailure(cause) if Arc::ptr_eq(original, &cause)));
    assert!(reader.next_delivery().await?.is_none());
    alix.context.db().reconnect()?;
    let mut replacement = create()?;
    assert!(matches!(item.acknowledgement.acknowledge(), Err(LocalDeliveryError::SessionFailure(cause)) if Arc::ptr_eq(original, &cause)));
    reader.close();
    assert!(create().is_err());
    let replay = replacement.next_delivery().await?.unwrap();
    assert_eq!(replay.cursor, item.cursor);
    assert_eq!(replay.message.id, message.id);
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn renewal_storage_error_retains_its_cause_and_allows_a_fresh_reader() {
    use xmtp_common::ErrorCode;
    let alix = EngineTestPeer::persistent().await;
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let create = || LocalDelivery::new(alix.context.clone(), DeliveryScope::All,
        LocalDeliveryFilter::default(), None, LocalDeliveryConfig {
            renew_interval: Duration::from_millis(5), ..Default::default()
        });
    let mut reader = create()?;
    let completed = reader.next_delivery().await?.unwrap();
    completed.acknowledgement.acknowledge()?;
    let pending = reader.next_delivery().await?.unwrap();
    alix.context.db().disconnect()?;
    let expected = alix.context.db().stream_database_id().unwrap_err();
    xmtp_common::wait_for_eq(|| async { reader.session.is_closed() }, true).await?;
    completed.acknowledgement.acknowledge()?;
    let check_error = pending.acknowledgement.check_owner().unwrap_err();
    let ack_error = pending.acknowledgement.acknowledge().unwrap_err();
    assert_eq!(check_error.error_code(), expected.error_code());
    assert_eq!(ack_error.error_code(), check_error.error_code());
    assert_eq!(ack_error.to_string(), check_error.to_string());
    assert!(!alix.context.is_closed());
    alix.context.db().reconnect()?;
    let mut replacement = create()?;
    assert_eq!(replacement.next_delivery().await?.unwrap().message.id, second.id);
}

// verifies: PROC-052, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn sqlite_full_ack_ends_stream_without_next_handoff_and_new_stream_replays() {
    use diesel::{RunQueryDsl, connection::SimpleConnection};
    #[derive(diesel::QueryableByName)]
    struct PageLimit {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        max_page_count: i64,
    }
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let create = || LocalDelivery::new(alix.context.clone(), DeliveryScope::All,
        LocalDeliveryFilter::default(), None, LocalDeliveryConfig::default());
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    let original_limit = alix.context.db().raw_query(|conn| {
        let original = diesel::sql_query("PRAGMA max_page_count").get_result::<PageLimit>(conn)?;
        // Force a real SQLite page-allocation failure inside acknowledgement.
        // The limit applies to this test database and does not fill the host disk.
        conn.batch_execute("CREATE TABLE delivery_fault_payload (payload BLOB);
            CREATE TRIGGER delivery_fault_full BEFORE INSERT ON refresh_state
            WHEN NEW.entity_kind = 10 BEGIN
                INSERT INTO delivery_fault_payload VALUES (zeroblob(1048576));
            END;
            PRAGMA max_page_count = 1;")?;
        Ok::<_, diesel::result::Error>(original.max_page_count)
    })?;
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(error.to_string().contains("database or disk is full"));
    assert!(stream.next().await.is_none());
    alix.context.db().raw_query(|conn| conn.batch_execute(&format!(
        "PRAGMA max_page_count = {original_limit}; DROP TRIGGER delivery_fault_full;"
    )))?;
    let mut replacement = Box::pin(create()?.into_stream());
    assert_eq!(replacement.next().await.unwrap()?.id, first.id);
    assert_eq!(replacement.next().await.unwrap()?.id, second.id);
}
}

/// A second default consumer is refused with the stream-storage code on every
/// route that opens one. The refusal is not retried and needs no reconnect.
#[xmtp_common::test(unwrap_try = true)]
async fn subscribe_error_code_reaches_stream_storage_code() {
    use crate::{stream_all::StreamAllMessages, stream_messages::StreamGroupMessages};
    use xmtp_common::RetryableError;
    use xmtp_mls_core::worker::NeedsDbReconnect;
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let open = || {
        MessageReader::new(
            alix.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter::default(),
            None,
        )
    };
    let _owner = open()?;

    let reader = open().err().expect("a second default reader is refused");
    assert_eq!(reader.error_code(), "StreamStorageError::AlreadyActive");
    assert!(!reader.is_retryable());

    let all = StreamAllMessages::new(&alix.context, None, None)
        .await
        .err()
        .expect("a second message stream is refused");
    let groups = StreamGroupMessages::new(&alix.context, vec![group.group_id])
        .await
        .err()
        .expect("a second group stream is refused");
    for (route, error) in [("stream_all", all), ("stream_messages", groups)] {
        assert_eq!(
            error.error_code(),
            "StreamStorageError::AlreadyActive",
            "{route}"
        );
        assert!(!error.is_retryable(), "{route}");
        assert!(!error.needs_db_reconnect(), "{route}");
    }
}
