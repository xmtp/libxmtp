use super::*;
use crate::{
    subscriptions::message_reader::MessageReader, test::mock::generate_stored_msg, tester,
};
use futures::{FutureExt, StreamExt};
use xmtp_common::{
    ErrorCode,
    time::{Duration, timeout},
};
use xmtp_db::{ConnectionExt, Store};
use xmtp_proto::types::Cursor;

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true)]
async fn iterator_drop_retains_the_last_item_until_a_later_next_request() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    drop(stream);
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    assert_eq!(stream.next().await.unwrap()?.id, second.id);
    drop(stream);
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, second.id);
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_callback_releases_owner_without_consuming_its_item() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    assert!(create().is_err());
    item.acknowledgement.reject();
    let mut replacement = create()?;
    assert_eq!(
        replacement.next_delivery().await?.unwrap().message.id,
        message.id
    );
    assert!(matches!(
        reader.next_delivery().await,
        Err(LocalDeliveryError::AcknowledgementRejected)
    ));
}

// verifies: PROC-028
#[xmtp_common::test(unwrap_try = true)]
async fn cancelling_a_pending_next_does_not_bypass_explicit_acknowledgement() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert!(reader.next_delivery().now_or_never().is_none());
    assert!(reader.next_delivery().now_or_never().is_none());
    item.acknowledgement.acknowledge()?;
    assert_eq!(reader.next_delivery().await?.unwrap().message.id, second.id);
}

// verifies: CONS-043
#[xmtp_common::test(unwrap_try = true)]
async fn denied_group_with_default_filter_is_delivered_when_scoped() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    group.update_consent_state(ConsentState::Denied)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = timeout(Duration::from_secs(5), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(item.message.id, message.id);
}

// verifies: PROC-032
#[xmtp_common::test(unwrap_try = true)]
async fn excluded_rows_stay_consumed_after_a_filter_change() {
    tester!(alix);
    let denied = alix.create_group(None, None)?;
    let allowed = alix.create_group(None, None)?;
    denied.update_consent_state(ConsentState::Denied)?;
    let excluded = generate_stored_msg(Cursor(100), denied.group_id);
    let selected = generate_stored_msg(Cursor(200), allowed.group_id);
    excluded.store(&alix.context.db())?;
    selected.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter {
            consent_states: Some(vec![ConsentState::Allowed]),
            ..Default::default()
        },
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert_eq!(item.message.id, selected.id);
    item.acknowledgement.acknowledge()?;
    let later = generate_stored_msg(Cursor(300), denied.group_id);
    later.store(&alix.context.db())?;
    let candidate = alix.context.db().default_delivery_messages(
        reader.session.owner().unwrap(),
        &DeliveryScope::All,
        now_ns(),
        1,
    )?[0]
        .clone();
    let revision = reader.control.selection.lock().revision;
    // The filter rejected this candidate before its group left the selected scope.
    reader
        .control()
        .update_scope(DeliveryScope::Groups(vec![allowed.group_id]));
    assert!(!reader.skip_candidate(&candidate, revision)?);
    reader
        .control()
        .update_filter(LocalDeliveryFilter::default());
    reader.control().update_scope(DeliveryScope::All);
    assert_eq!(reader.next_delivery().await?.unwrap().message.id, later.id);
}

// verifies: PROC-026
#[xmtp_common::test(unwrap_try = true)]
async fn missed_local_wake_is_recovered_by_a_fresh_database_poll() {
    tester!(alix);
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

// verifies: PROC-031
#[xmtp_common::test(unwrap_try = true)]
async fn stale_host_queue_token_cannot_dispatch_or_acknowledge() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    generate_stored_msg(Cursor(100), group.group_id).store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    alix.context.db().rotate_stream_database_id()?;
    assert!(item.acknowledgement.check_owner().is_err());
    assert!(item.acknowledgement.acknowledge().is_err());
    assert!(reader.next_delivery().await.is_err());
}

// verifies: PROC-032
#[xmtp_common::test(unwrap_try = true)]
async fn removed_and_readded_scope_discards_old_queued_tokens_without_acknowledging() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let old = reader.next_delivery().await?.unwrap();
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    reader
        .control()
        .update_scope(DeliveryScope::Groups(vec![group.group_id]));
    assert!(matches!(
        old.acknowledgement.check_owner(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    assert!(matches!(
        old.acknowledgement.acknowledge(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    drop(old);
    let selected_again = reader.next_delivery().await?.unwrap();
    assert_eq!(selected_again.message.id, message.id);
    selected_again.acknowledgement.check_owner()?;
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    // This callback already started. Removing its group cannot undo its successful return.
    selected_again.acknowledgement.acknowledge()?;
    assert!(
        alix.context
            .db()
            .default_delivery_messages(
                reader.session.owner().unwrap(),
                &DeliveryScope::All,
                now_ns(),
                8
            )?
            .is_empty()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn queued_content_is_rechecked_after_deletion_and_restore() {
    use diesel::{QueryDsl, RunQueryDsl};
    tester!(alix);
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

xmtp_common::if_native! {
// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn read_storage_error_ends_iterator_once_and_new_reader_replays_on_same_client() {
    tester!(alix, persistent_db);
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
    tester!(alix, persistent_db);
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
    tester!(alix, persistent_db);
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
    tester!(alix, persistent_db);
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
    tester!(alix, persistent_db);
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

// verifies: PROC-028, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn sqlite_full_ack_ends_stream_without_next_handoff_and_new_stream_replays() {
    use diesel::{RunQueryDsl, connection::SimpleConnection};
    #[derive(diesel::QueryableByName)]
    struct PageLimit {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        max_page_count: i64,
    }
    tester!(alix);
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

// verifies: PROC-031, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn expired_lease_ends_reader_and_only_a_new_stream_can_acquire_delivery() {
    use diesel::RunQueryDsl;
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    item.acknowledgement.check_owner()?;
    let old = reader.session.owner().unwrap();
    alix.context.db().raw_query(|conn| {
        diesel::sql_query("UPDATE user_preferences SET delivery_owner_until_ns = 0").execute(conn)
    })?;
    assert!(item.acknowledgement.acknowledge().is_err());
    assert!(reader.session.is_closed());
    let mut replacement = create()?;
    assert_ne!(replacement.session.owner(), Some(old));
    assert!(item.acknowledgement.check_owner().is_err());
    assert_eq!(
        replacement.next_delivery().await?.unwrap().message.id,
        message.id
    );
}

// verifies: PROC-031, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn failed_old_acknowledgement_cannot_release_a_competing_owner() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    generate_stored_msg(Cursor(100), group.group_id).store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    alix.context
        .db()
        .release_delivery_owner(reader.session.owner().unwrap())?;
    let other = alix.context.db().acquire_delivery_owner_with_clock(
        LocalDeliveryConfig::default().lease_duration_ns()?,
        now_ns,
    )?;
    assert!(item.acknowledgement.acknowledge().is_err());
    alix.context
        .db()
        .check_delivery_owner_with_clock(other, now_ns)?;
    assert!(
        !alix
            .context
            .db()
            .default_delivery_messages(other, &DeliveryScope::All, now_ns(), 1)?
            .is_empty()
    );
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn queued_content_decode_failure_is_terminal_and_does_not_advance_delivery() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || {
        MessageReader::new(
            alix.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter::default(),
            None,
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    let mut pending = Box::pin(reader.next_delivery());
    assert!(pending.as_mut().now_or_never().is_none());
    let error = item.acknowledgement.enriched_message().unwrap_err();
    let LocalDeliveryError::SessionFailure(original) = &error else {
        panic!("Expected the shared enrichment failure")
    };
    assert!(matches!(
        original.as_ref(),
        LocalDeliveryError::Enrichment(_)
    ));
    assert_eq!(error.error_code(), original.error_code());
    assert_eq!(error.to_string(), original.to_string());
    let Err(reader_error) = timeout(Duration::from_secs(1), pending).await? else {
        panic!("The pending reader lost its enrichment failure")
    };
    assert!(
        matches!(reader_error, LocalDeliveryError::SessionFailure(cause) if Arc::ptr_eq(original, &cause))
    );
    assert!(reader.next_delivery().await?.is_none());
    assert!(
        matches!(item.acknowledgement.acknowledge(), Err(LocalDeliveryError::SessionFailure(cause)) if Arc::ptr_eq(original, &cause))
    );
    let replay = create()?.next_delivery().await?.unwrap();
    assert_eq!(replay.cursor, item.cursor);
    assert_eq!(replay.message.id, message.id);
}

// verifies: PROC-032, PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn enrichment_does_not_dispatch_and_stale_selection_remains_nonterminal() {
    use prost::Message;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    tester!(alix);
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
