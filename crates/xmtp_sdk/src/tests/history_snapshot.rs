use super::*;
use xmtp_db::delivery::QueryDelivery;

// verifies: DMS-016
#[xmtp_common::test(unwrap_try = true)]
async fn history_snapshot_routes_recent_rows_and_global_atomic_boundary() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let unrelated = client.conversations().create_group(vec![], None).await?;
    group.send_text("older".into(), None).await?;
    let middle = group.send_text("middle".into(), None).await?;
    let newest = group.send_text("newest".into(), None).await?;
    unrelated.send_text("outside group".into(), None).await?;
    let boundary = client.inner.context.db().current_delivery_cursor()?;
    let snapshot = group.message_history_snapshot(2).await?;
    assert_eq!(
        snapshot
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>(),
        [middle, newest]
    );
    assert_eq!(crate::delivery::cursor::parse(&snapshot.cursor)?, boundary);
    let rows = snapshot
        .messages
        .iter()
        .map(|message| {
            crate::delivery::cursor::parse(message.0.delivery_cursor.as_ref().expect("row cursor"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert!(rows[0].delivery_sequence < rows[1].delivery_sequence);
    assert!(rows[1].delivery_sequence < boundary.delivery_sequence);
    let empty = group.message_history_snapshot(0).await?;
    assert!(empty.messages.is_empty());
    assert_eq!(empty.cursor, snapshot.cursor);
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = client
        .conversations()
        .create_dm(peer.inbox_id(), None)
        .await?;
    let dm_id = dm.send_text("dm".into(), None).await?;
    let dm_snapshot = dm.message_history_snapshot(1).await?;
    assert_eq!(dm_snapshot.messages[0].0.id, dm_id);
    assert!(dm_snapshot.messages[0].0.delivery_cursor.is_some());
    client.end().await?;
    peer.end().await?;
}

// verifies: DMS-016
#[xmtp_common::test(unwrap_try = true)]
async fn history_snapshot_preserves_selection_and_rejects_replay_before_query() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let conversations = client.conversations();
    let group = conversations.create_group(vec![], None).await?;
    let id = group.send_text("selected".into(), None).await?;
    let selected = conversations.message_history_snapshot(10, None).await?;
    assert!(selected.messages.iter().any(|message| message.0.id == id));
    let empty = conversations
        .message_history_snapshot(
            10,
            Some(crate::MessageReaderOptions {
                consent_states: Some(vec![]),
                ..Default::default()
            }),
        )
        .await?;
    assert!(empty.messages.is_empty());
    assert_eq!(empty.cursor, selected.cursor);
    let kind = conversations
        .message_history_snapshot(
            10,
            Some(crate::MessageReaderOptions {
                conversation_kind: Some(crate::ConversationKind::Dm),
                ..Default::default()
            }),
        )
        .await?;
    assert!(kind.messages.is_empty());
    assert_eq!(kind.cursor, selected.cursor);
    let (rejected, queries, writes) = xmtp_db::count_sql_queries(|| {
        futures::executor::block_on(conversations.message_history_snapshot(
            10,
            Some(crate::MessageReaderOptions {
                from: Some(selected.cursor),
                ..Default::default()
            }),
        ))
    });
    assert!(matches!(rejected, Err(XmtpError::InvalidArgument(_))));
    assert_eq!((queries, writes), (0, 0));
    client.end().await?;
}
