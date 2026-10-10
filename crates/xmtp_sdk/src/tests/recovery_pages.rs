use super::*;
use crate::{DeliveryStatus, ListMessagesOptions, MessageKind, MessageOrder, SendOptions};
use xmtp_db::delivery::QueryDelivery;
use xmtp_db::diesel;
use xmtp_db::{
    ConnectionExt,
    diesel::prelude::*,
    schema::{group_messages::dsl, refresh_state, user_preferences},
};

fn selected(direction: MessageOrder, limit: u32) -> Option<ListMessagesOptions> {
    Some(ListMessagesOptions {
        direction: Some(direction),
        limit: Some(limit),
        kind: Some(MessageKind::Application),
        ..Default::default()
    })
}

// verifies: PROC-024, PROC-025, PROC-026, PROC-050
#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_sdk_pending_retry_keeps_actual_reader_and_delivery_state() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let published = group.send_text("held reader item".into(), None).await?;
    let reader = group.message_reader(None).await?;
    let held = xmtp_common::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(message) = reader.next().await?
                && message.0.id == published
            {
                break Ok::<_, XmtpError>(message);
            }
        }
    })
    .await??;
    assert!(held.0.delivery_cursor.is_some());
    let mut ids = Vec::new();
    for index in 0..80 {
        ids.push(
            group
                .send_text(
                    format!("pending {index}"),
                    Some(SendOptions {
                        optimistic: true,
                        ..Default::default()
                    }),
                )
                .await?,
        );
    }
    let bytes = ids
        .iter()
        .map(MessageId::to_bytes)
        .collect::<Result<Vec<_>, _>>()?;
    let db = client.inner.context.db();
    db.raw_query(|conn| {
        diesel::update(dsl::group_messages.filter(dsl::id.eq_any(&bytes)))
            .set(dsl::sent_at_ns.eq(10))
            .execute(conn)?;
        diesel::update(dsl::group_messages.filter(dsl::id.eq_any(&bytes[..40])))
            .set(dsl::delivery_status.eq(xmtp_db::group_message::DeliveryStatus::Failed))
            .execute(conn)
    })?;
    let state = || {
        db.raw_query(|conn| {
            let owner = user_preferences::table
                .select(user_preferences::delivery_owner)
                .first::<Option<Vec<u8>>>(conn)?;
            let progress = refresh_state::table
                .filter(refresh_state::entity_kind.eq_any([1, 2, 9, 10]))
                .order((refresh_state::entity_id, refresh_state::entity_kind))
                .select((
                    refresh_state::entity_id,
                    refresh_state::entity_kind,
                    refresh_state::sequence_id,
                    refresh_state::received_sequence_id,
                ))
                .load::<(Vec<u8>, i32, i64, Option<i64>)>(conn)?;
            let positions = dsl::group_messages
                .filter(dsl::id.eq_any(&bytes))
                .select(dsl::delivery_sequence)
                .load::<Option<i64>>(conn)?;
            Ok::<_, diesel::result::Error>((owner, progress, positions))
        })
    };
    let initial = state()?;
    assert!(initial.0.is_some());
    assert!(initial.2.iter().all(Option::is_none));
    let mut expected: Vec<_> = ids.clone();
    expected.sort_by_key(|id| id.to_bytes().unwrap());
    let first = group
        .message_recovery_page(selected(MessageOrder::Ascending, 50), None, None)
        .await?;
    assert_eq!(
        first
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        expected[..50]
    );
    assert!(first.has_more);
    assert_eq!(first.skipped_count, 0);
    assert_eq!(
        state()?,
        initial,
        "recovery must retain owner, F/P/D, allocator and null pending delivery numbers"
    );
    let second = group
        .message_recovery_page(
            selected(MessageOrder::Ascending, 50),
            None,
            first.last_position.clone(),
        )
        .await?;
    assert_eq!(
        second
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        expected[50..]
    );
    assert!(!second.has_more);
    let backwards = group
        .message_recovery_page(
            selected(MessageOrder::Descending, 50),
            second.first_position,
            None,
        )
        .await?;
    assert_eq!(
        backwards
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        expected[..50].iter().rev().cloned().collect::<Vec<_>>()
    );
    let before_cursor = db.current_delivery_cursor()?;
    let anchor_id = first.messages.last().unwrap().0.id.clone();
    group.publish_message(anchor_id.clone()).await?;
    // Publication can drain other queued intents in this group. Queue a new
    // retained row, then reuse the old boundary without an anchor lookup.
    let newer = group
        .send_text(
            "pending after publication".into(),
            Some(SendOptions {
                optimistic: true,
                ..Default::default()
            }),
        )
        .await?;
    let next = group
        .message_recovery_page(
            selected(MessageOrder::Ascending, 50),
            None,
            first.last_position,
        )
        .await?;
    assert_eq!(
        next.messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        vec![newer]
    );
    let retained = client
        .conversations()
        .get_message_by_id(anchor_id)
        .await?
        .unwrap();
    assert!(matches!(
        retained.0.delivery_status,
        DeliveryStatus::Published
    ));
    assert!(retained.0.delivery_cursor.is_some());
    assert!(db.current_delivery_cursor()?.delivery_sequence > before_cursor.delivery_sequence);
    // Closing does not acknowledge the held row. Recovery did not take its lease.
    reader.end().await?;
    let reopened = group.message_reader(None).await?;
    let replayed = xmtp_common::time::timeout(Duration::from_secs(30), reopened.next())
        .await??
        .unwrap();
    assert_eq!(replayed.0.id, published);
    reopened.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_sdk_unreadable_prefix_raw_id_and_foreign_boundary() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let foreign = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut ids = Vec::new();
    for index in 0..80 {
        ids.push(
            group
                .send_text(
                    format!("raw pending {index}"),
                    Some(SendOptions {
                        optimistic: true,
                        ..Default::default()
                    }),
                )
                .await?,
        );
    }
    let bytes = ids
        .iter()
        .map(MessageId::to_bytes)
        .collect::<Result<Vec<_>, _>>()?;
    client.inner.context.db().raw_query(|conn| {
        diesel::update(dsl::group_messages.filter(dsl::id.eq_any(&bytes[..49])))
            .set((dsl::sent_at_ns.eq(9), dsl::sender_inbox_id.eq("")))
            .execute(conn)?;
        diesel::update(dsl::group_messages.filter(dsl::id.eq(&bytes[49])))
            .set((dsl::sent_at_ns.eq(10), dsl::id.eq(vec![255_u8])))
            .execute(conn)?;
        diesel::update(dsl::group_messages.filter(dsl::id.eq_any(&bytes[50..])))
            .set(dsl::sent_at_ns.eq(11))
            .execute(conn)
    })?;
    let unreadable = group.message_recovery_page(None, None, None).await?;
    assert!(unreadable.messages.is_empty());
    assert_eq!(unreadable.skipped_count, 50);
    assert!(unreadable.has_more);
    let boundary = unreadable.last_position.unwrap();
    assert_eq!(boundary.sent_at.0, 10);
    let next = group
        .message_recovery_page(None, None, Some(boundary.clone()))
        .await?;
    let mut expected = ids[50..].to_vec();
    expected.sort_by_key(|id| id.to_bytes().unwrap());
    assert_eq!(
        next.messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        expected
    );
    let other = foreign.conversations().create_group(vec![], None).await?;
    assert!(matches!(
        other
            .message_recovery_page(None, Some(boundary.clone()), None)
            .await,
        Err(XmtpError::ForeignCursor(_))
    ));
    client.inner.context.db().rotate_stream_database_id()?;
    assert!(matches!(
        group
            .message_recovery_page(None, None, Some(boundary))
            .await,
        Err(XmtpError::ForeignCursor(_))
    ));
    client.end().await?;
    foreign.end().await?;
}

// verifies: DMS-009, PROC-037, PROC-050, CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_sdk_stitched_source_deletion_is_redacted() {
    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = a.conversations().create_dm(b.inbox_id(), None).await?;
    let second = b.conversations().create_dm(a.inbox_id(), None).await?;
    assert_ne!(first.inner.group_id, second.inner.group_id);
    let id_a = first
        .send_text("private pending source A".into(), None)
        .await?;
    let id_b = second
        .send_text("private pending source B".into(), None)
        .await?;
    a.conversations().sync_all(None).await?;
    b.conversations().sync_all(None).await?;
    let winner = a
        .conversations()
        .get_dm_by_inbox_id(b.inbox_id())
        .await?
        .unwrap();
    let (source, author, id) = if first.inner.group_id != winner.inner.group_id {
        (&first, &a, &id_a)
    } else {
        (&second, &b, &id_b)
    };
    assert_ne!(source.inner.group_id, winner.inner.group_id);
    author.conversations().delete_message(id.clone()).await?;
    source.publish_messages().await?;
    a.conversations().sync_all(None).await?;
    // Retain a Failed row with a real processed source deletion. This isolates
    // recovery enrichment from the publication path that created the deletion.
    let committed_before = a
        .conversations()
        .get_message_by_id(id.clone())
        .await?
        .unwrap()
        .0
        .delivery_cursor;
    assert!(committed_before.is_some());
    a.inner.context.db().raw_query(|conn| {
        diesel::update(dsl::group_messages.filter(dsl::id.eq(id.to_bytes().unwrap())))
            .set(dsl::delivery_status.eq(xmtp_db::group_message::DeliveryStatus::Failed))
            .execute(conn)
    })?;
    let page = winner
        .message_recovery_page(selected(MessageOrder::Ascending, 50), None, None)
        .await?;
    let deleted = page
        .messages
        .iter()
        .find(|message| &message.0.id == id)
        .unwrap();
    assert!(
        matches!(deleted.0.content, MessageContent::DeletedMessage(_)),
        "recovery must use the physical source deletion"
    );
    assert_eq!(deleted.0.delivery_cursor, committed_before);
    assert!(deleted.0.raw_bytes.is_empty());
    assert!(deleted.0.fallback.is_none());
    assert!(!format!("{:?}", deleted.0).contains("private pending source"));
    a.end().await?;
    b.end().await?;
}
