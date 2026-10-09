use super::*;
use crate::{DeliveryStatus, ListMessagesOptions, MessageKind, MessageOrder, SendOptions};
use xmtp_db::group_message::QueryGroupMessage;
use xmtp_db::{ConnectionExt, Store, diesel::prelude::*, schema::group_messages::dsl};

fn selected(direction: MessageOrder, limit: u32) -> Option<ListMessagesOptions> {
    Some(ListMessagesOptions {
        direction: Some(direction),
        limit: Some(limit),
        kind: Some(MessageKind::Application),
        ..Default::default()
    })
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_sdk_equal_time_conversion_and_sentinel() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut ids = Vec::new();
    for index in 0..80 {
        ids.push(
            group
                .send_text(
                    format!("page {index}"),
                    Some(SendOptions {
                        optimistic: true,
                        ..Default::default()
                    }),
                )
                .await?,
        );
    }
    group.publish_messages().await?;
    let bytes = ids
        .iter()
        .map(MessageId::to_bytes)
        .collect::<Result<Vec<_>, _>>()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq_any(&bytes)))
            .set(dsl::sent_at_ns.eq(10))
            .execute(conn)
    })?;
    let first = group.message_history_page(None, None, None).await?;
    assert_eq!(first.messages.len(), 50);
    assert_eq!(
        first
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        ids[..50]
    );
    assert!(first.has_more);
    assert_eq!(first.skipped_count, 0);
    let last = first.last_position.clone().unwrap();
    let second = group.message_history_page(None, None, Some(last)).await?;
    assert_eq!(
        second
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        ids[50..]
    );
    assert!(!second.has_more);
    let reverse = group
        .message_history_page(selected(MessageOrder::Descending, 50), None, None)
        .await?;
    assert_eq!(
        reverse
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        ids.iter().rev().take(50).cloned().collect::<Vec<_>>()
    );
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq_any(&bytes[..50])))
            .set(dsl::sender_inbox_id.eq(""))
            .execute(conn)
    })?;
    let unreadable = group.message_history_page(None, None, None).await?;
    assert!(unreadable.messages.is_empty());
    assert_eq!(unreadable.skipped_count, 50);
    assert!(unreadable.has_more);
    assert_eq!(unreadable.first_position.unwrap().sent_at.0, 10);
    let next = group
        .message_history_page(None, None, unreadable.last_position)
        .await?;
    assert_eq!(
        next.messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        ids[50..]
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_sdk_deleted_anchor_backfill_and_foreign_cursor() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let foreign = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut ids = Vec::new();
    for index in 0..5 {
        ids.push(group.send_text(format!("anchor {index}"), None).await?);
    }
    let first = group
        .message_history_page(selected(MessageOrder::Ascending, 3), None, None)
        .await?;
    let anchor = first.last_position.clone().unwrap();
    client
        .conversations()
        .delete_message_locally(ids[2].clone())
        .await?;
    let after = group
        .message_history_page(
            selected(MessageOrder::Ascending, 50),
            None,
            Some(anchor.clone()),
        )
        .await?;
    assert_eq!(
        after
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        ids[3..]
    );
    let mut imported = client
        .inner
        .context
        .db()
        .get_group_message(ids[0].to_bytes()?)?
        .unwrap();
    imported.id = vec![7; 32];
    imported.sent_at_ns = 1;
    imported.sequence_id = 0;
    imported.store(&client.inner.context.db())?;
    let backfill = group
        .message_history_page(selected(MessageOrder::Ascending, 50), None, None)
        .await?;
    assert_eq!(
        backfill.messages.first().unwrap().0.id.to_bytes()?,
        imported.id
    );
    assert!(
        backfill
            .messages
            .first()
            .unwrap()
            .0
            .delivery_cursor
            .is_some()
    );
    let other = foreign.conversations().create_group(vec![], None).await?;
    other.send_text("foreign".into(), None).await?;
    let position = other
        .message_history_page(None, None, None)
        .await?
        .last_position
        .unwrap();
    assert!(matches!(
        group.message_history_page(None, Some(position), None).await,
        Err(XmtpError::ForeignCursor(_))
    ));
    let mut future = anchor;
    let mut cursor = crate::delivery::cursor::parse(&future.delivery_cursor)?;
    cursor.delivery_sequence = u64::MAX;
    future.delivery_cursor = crate::delivery::cursor::encode(cursor);
    assert!(matches!(
        group.message_history_page(None, Some(future), None).await,
        Err(XmtpError::InvalidCursor(_))
    ));
    foreign.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_sdk_stitched_dm_preserves_defaults_and_filters() {
    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = a.conversations().create_dm(b.inbox_id(), None).await?;
    let first_id = first.send_text("first physical".into(), None).await?;
    let second = b.conversations().create_dm(a.inbox_id(), None).await?;
    let second_id = second.send_text("second physical".into(), None).await?;
    a.conversations().sync_all(None).await?;
    let winner = a
        .conversations()
        .get_dm_by_inbox_id(b.inbox_id())
        .await?
        .unwrap();
    let oldest = winner
        .message_history_page(selected(MessageOrder::Ascending, 1), None, None)
        .await?;
    assert_eq!(oldest.messages[0].0.id, first_id);
    let next = winner
        .message_history_page(
            selected(MessageOrder::Ascending, 1),
            None,
            oldest.last_position,
        )
        .await?;
    assert_eq!(next.messages[0].0.id, second_id);
    let only_peer = winner
        .message_history_page(
            Some(ListMessagesOptions {
                exclude_sender_inbox_ids: Some(vec![a.inbox_id()]),
                kind: Some(MessageKind::Application),
                ..Default::default()
            }),
            None,
            None,
        )
        .await?;
    assert_eq!(
        only_peer
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        vec![second_id]
    );
    assert!(matches!(
        winner
            .message_history_page(
                Some(ListMessagesOptions {
                    delivery_status: Some(DeliveryStatus::Unpublished),
                    ..Default::default()
                }),
                None,
                None
            )
            .await,
        Err(XmtpError::InvalidArgument(_))
    ));
    b.end().await?;
    a.end().await?;
}
