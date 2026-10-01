use super::*;

// verifies: PROC-025, PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn prepared_message_rechecks_consent_before_admission() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let stale = client.conversations().create_group(vec![], None).await?;
    let live = client.conversations().create_group(vec![], None).await?;
    stale.send_text("stale".into(), None).await?;
    let live_id = live.send_text("live".into(), None).await?;
    let reader = stale.message_reader(None).await?;
    reader.update_all_scope_for_test();
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    let reading = reader.clone();
    let task = xmtp_common::spawn(None, async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    stale
        .inner
        .update_consent_state(xmtp_db::consent_record::ConsentState::Denied)?;
    gate.release.notify_one();
    use xmtp_common::StreamHandle;
    let result = xmtp_common::time::timeout(Duration::from_secs(10), task.join()).await???;
    assert_eq!(result.expect("eligible item").0.id, live_id);
    reader.end().await?;
    client.end().await?;
}

// verifies: PROC-025
#[xmtp_common::test(unwrap_try = true)]
async fn prepared_message_rechecks_deletion_before_admission() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let stale_id = group.send_text("stale".into(), None).await?;
    let live_id = group.send_text("live".into(), None).await?;
    let reader = group.message_reader(None).await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    let reading = reader.clone();
    let task = xmtp_common::spawn(None, async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    client
        .conversations()
        .delete_message_locally(stale_id)
        .await?;
    gate.release.notify_one();
    use xmtp_common::StreamHandle;
    let result = xmtp_common::time::timeout(Duration::from_secs(10), task.join()).await???;
    assert_eq!(result.expect("retained item").0.id, live_id);
    reader.end().await?;
    client.end().await?;
}

// verifies: PROC-033, PROC-034, PROC-050
#[xmtp_common::test(unwrap_try = true)]
async fn delivery_cursor_preserves_large_position_across_full_results() {
    use xmtp_db::{
        ConnectionExt, delivery::QueryDelivery, diesel::prelude::*, refresh_state::EntityKind,
        schema::refresh_state::dsl,
    };
    let path = std::env::temp_dir().join(format!(
        "sdk-large-cursor-{}.db3",
        xmtp_common::time::now_ns()
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let client = Client::create(signer.clone(), settings.clone()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let group_id = group.id();
    let start = client.conversations().beginning_delivery_cursor().await?;
    let db = client.inner.context.db();
    let high = 1_i64 << 53;
    db.raw_query(|conn| {
        xmtp_db::diesel::update(
            dsl::refresh_state.filter(dsl::entity_kind.eq(EntityKind::DeliveryAllocator)),
        )
        .set(dsl::sequence_id.eq(high))
        .execute(conn)
    })?;
    let id = group.send_text("large cursor".into(), None).await?;
    let expected = db.current_delivery_cursor()?;
    assert_eq!(expected.delivery_sequence, high as u64 + 1);
    let history = group
        .messages(None)
        .await?
        .into_iter()
        .find(|message| message.0.id == id)
        .expect("history");
    let cursor = history.0.delivery_cursor.clone().expect("published cursor");
    assert_eq!(crate::delivery::cursor::parse(&cursor)?, expected);
    let lookup = client
        .conversations()
        .get_message_by_id(id.clone())
        .await?
        .expect("lookup");
    assert_eq!(lookup.0.delivery_cursor.as_ref(), Some(&cursor));
    assert_eq!(
        group
            .last_message()
            .await?
            .expect("last")
            .0
            .delivery_cursor
            .as_ref(),
        Some(&cursor)
    );
    let reader = group
        .message_reader(Some(crate::ConversationMessageReaderOptions {
            from: Some(start),
        }))
        .await?;
    assert_eq!(
        reader
            .next()
            .await?
            .expect("replay")
            .0
            .delivery_cursor
            .as_ref(),
        Some(&cursor)
    );
    let default = group.message_reader(None).await?;
    assert_eq!(
        default
            .next()
            .await?
            .expect("replay did not consume default")
            .0
            .id,
        id
    );
    let owner_before: Option<Vec<u8>> = db.raw_query(|conn| {
        use xmtp_db::schema::user_preferences::dsl;
        dsl::user_preferences
            .select(dsl::delivery_owner)
            .first(conn)
    })?;
    assert!(owner_before.is_some());
    use xmtp_db::refresh_state::QueryRefreshState;
    let progress_before = db.get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?;
    let next_id = group.send_text("adjacent position".into(), None).await?;
    assert_eq!(
        db.current_delivery_cursor()?.delivery_sequence,
        expected.delivery_sequence + 1
    );
    let resume = group
        .message_reader(Some(crate::ConversationMessageReaderOptions {
            from: Some(cursor.clone()),
        }))
        .await?;
    assert_eq!(
        resume
            .next()
            .await?
            .expect("exclusive adjacent resume")
            .0
            .id,
        next_id
    );
    resume.end().await?;
    let advanced_replay = reader
        .next()
        .await?
        .expect("replay acknowledgement progress");
    assert_eq!(advanced_replay.0.id, next_id);
    assert_eq!(
        crate::delivery::cursor::parse(
            advanced_replay
                .0
                .delivery_cursor
                .as_ref()
                .expect("adjacent cursor")
        )?
        .delivery_sequence,
        expected.delivery_sequence + 1
    );
    let owner_after: Option<Vec<u8>> = db.raw_query(|conn| {
        use xmtp_db::schema::user_preferences::dsl;
        dsl::user_preferences
            .select(dsl::delivery_owner)
            .first(conn)
    })?;
    assert_eq!(
        owner_after, owner_before,
        "replay must not replace the default owner"
    );
    assert_eq!(
        db.get_last_cursor(&group.inner.group_id, EntityKind::Delivery)?,
        progress_before
    );
    reader.end().await?;
    default.end().await?;
    client.end().await?;
    drop((db, resume, reader, default, group, client));
    let reopened = Client::create(signer, settings).await?;
    let crate::Conversation::Group { group } = reopened
        .conversations()
        .get_by_id(group_id)
        .await?
        .expect("persisted group")
    else {
        panic!("group")
    };
    let replay = group
        .message_reader(Some(crate::ConversationMessageReaderOptions {
            from: Some(cursor.clone()),
        }))
        .await?;
    let adjacent = replay.next().await?.expect("persisted adjacent resume");
    assert_eq!(adjacent.0.id, next_id);
    assert_eq!(
        crate::delivery::cursor::parse(adjacent.0.delivery_cursor.as_ref().expect("cursor"))?
            .delivery_sequence,
        expected.delivery_sequence + 1
    );
    replay.end().await?;
    let default = group.message_reader(None).await?;
    let unchanged = default
        .next()
        .await?
        .expect("default progress remains unacknowledged");
    assert_eq!(unchanged.0.id, id);
    assert_eq!(unchanged.0.delivery_cursor.as_ref(), Some(&cursor));
    default.end().await?;
    reopened.end().await?;
    drop((default, replay, group, reopened));
    std::fs::remove_file(path)?;
}

// verifies: PROC-050
#[xmtp_common::test(unwrap_try = true)]
async fn delivery_cursor_absent_until_publication() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group
        .prepare_message(crate::encode_text("pending".into())?, None)
        .await?;
    let before = client
        .conversations()
        .get_message_by_id(id.clone())
        .await?
        .expect("pending row");
    assert!(before.0.delivery_cursor.is_none());
    assert!(
        group
            .last_message()
            .await?
            .expect("pending history")
            .0
            .delivery_cursor
            .is_none()
    );
    group.publish_messages().await?;
    let after = client
        .conversations()
        .get_message_by_id(id)
        .await?
        .expect("published row");
    assert!(after.0.delivery_cursor.is_some());
    assert!(
        before.0.delivery_cursor.is_none(),
        "old value changed after publication"
    );
    client.end().await?;
}

// verifies: PROC-033
#[xmtp_common::test(unwrap_try = true)]
async fn delivery_cursor_rejects_invalid_and_foreign_before_open() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let foreign = Client::create(crate::generate_local_signer().await, options()).await?;
    let start = client.conversations().beginning_delivery_cursor().await?;
    let mut future = crate::delivery::cursor::parse(&start)?;
    future.delivery_sequence = u64::MAX;
    for value in [
        String::new(),
        "dc2_AAAA".into(),
        format!("{start}="),
        crate::delivery::cursor::encode(future),
    ] {
        let result = client
            .conversations()
            .message_reader(Some(crate::MessageReaderOptions {
                from: Some(value),
                ..Default::default()
            }))
            .await;
        assert!(matches!(result, Err(crate::XmtpError::InvalidCursor(_))));
    }
    let other = foreign.conversations().beginning_delivery_cursor().await?;
    let result = client
        .conversations()
        .message_reader(Some(crate::MessageReaderOptions {
            from: Some(other),
            ..Default::default()
        }))
        .await;
    assert!(matches!(result, Err(crate::XmtpError::ForeignCursor(_))));
    let reader = client.conversations().message_reader(None).await?;
    reader.end().await?;
    client.end().await?;
    foreign.end().await?;
}

// verifies: PROC-040
#[xmtp_common::test(unwrap_try = true)]
async fn delivery_cursor_storage_failure_keeps_typed_cause() {
    use xmtp_db::{
        ConnectionExt,
        diesel::{RunQueryDsl, sql_query},
    };
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let beginning = client.conversations().beginning_delivery_cursor().await?;
    let db = client.inner.context.db();
    db.raw_query(|conn| {
        sql_query("ALTER TABLE user_preferences RENAME TO missing_preferences").execute(conn)
    })?;
    let beginning_error = client.conversations().beginning_delivery_cursor().await;
    let replay_error = client
        .conversations()
        .message_reader(Some(crate::MessageReaderOptions {
            from: Some(beginning),
            ..Default::default()
        }))
        .await;
    db.raw_query(|conn| {
        sql_query("ALTER TABLE missing_preferences RENAME TO user_preferences").execute(conn)
    })?;
    client.end().await?;
    for result in [beginning_error.map(|_| ()), replay_error.map(|_| ())] {
        assert!(
            matches!(result, Err(crate::XmtpError::Storage(ref details)) if details.code == "Storage" && matches!(details.category, crate::ErrorCategory::Storage)),
            "{result:?}"
        );
    }
}
