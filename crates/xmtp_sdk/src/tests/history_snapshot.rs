use super::*;
use futures::FutureExt;
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
        conversations
            .message_history_snapshot(
                10,
                Some(crate::MessageReaderOptions {
                    from: Some(selected.cursor),
                    ..Default::default()
                }),
            )
            .now_or_never()
    });
    assert!(matches!(rejected, Some(Err(XmtpError::InvalidArgument(_)))));
    assert_eq!((queries, writes), (0, 0));
    client.end().await?;
}

// verifies: DMS-016, PROC-037, CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
async fn history_snapshots_keep_relations_and_redact_deleted_messages_and_parents() {
    use crate::{Conversation, MessageBody, Reaction, ReactionAction, ReactionSchema};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let conversations = client.conversations();
    let group = conversations.create_group(vec![], None).await?;
    let dm = conversations.create_dm(peer.inbox_id(), None).await?;
    for conversation in [Conversation::Group { group }, Conversation::Dm { dm }] {
        let mut content = crate::encode_text("snapshot-private-parent".into())?;
        content.fallback = Some("snapshot-private-fallback".into());
        let parent = match &conversation {
            Conversation::Group { group } => group.send(content, None).await?,
            Conversation::Dm { dm } => dm.send(content, None).await?,
        };
        let reply = conversations
            .reply_to_message(parent.clone(), crate::encode_text("reply".into())?, None)
            .await?;
        let reaction = conversations
            .react_to_message(
                parent.clone(),
                Reaction {
                    content: "👍".into(),
                    action: ReactionAction::Added,
                    schema: ReactionSchema::Unicode,
                },
                None,
            )
            .await?;
        let selected = match &conversation {
            Conversation::Group { group } => group.message_history_snapshot(128).await?,
            Conversation::Dm { dm } => dm.message_history_snapshot(128).await?,
        };
        let all = conversations.message_history_snapshot(128, None).await?;
        for snapshot in [&selected, &all] {
            let original = snapshot
                .messages
                .iter()
                .find(|row| row.0.id == parent)
                .expect("parent");
            assert_eq!(original.0.reply_count, 1);
            assert_eq!(original.0.reactions.len(), 1);
            assert_eq!(original.0.reactions[0].id, reaction);
            let answer = snapshot
                .messages
                .iter()
                .find(|row| row.0.id == reply)
                .expect("reply");
            assert_eq!(
                answer.0.in_reply_to.as_ref().expect("reply parent").id,
                parent
            );
            assert!(!snapshot.cursor.is_empty());
            assert!(original.0.delivery_cursor.is_some());
            assert!(answer.0.delivery_cursor.is_some());
        }
        let recent = match &conversation {
            Conversation::Group { group } => group.message_history_snapshot(2).await?,
            Conversation::Dm { dm } => dm.message_history_snapshot(2).await?,
        };
        assert_eq!(
            recent
                .messages
                .iter()
                .map(|row| row.0.id.clone())
                .collect::<Vec<_>>(),
            [reply.clone(), reaction]
        );
        assert_eq!(recent.cursor, selected.cursor);
        conversations.delete_message(parent.clone()).await?;
        let selected = match &conversation {
            Conversation::Group { group } => group.message_history_snapshot(128).await?,
            Conversation::Dm { dm } => dm.message_history_snapshot(128).await?,
        };
        let all = conversations.message_history_snapshot(128, None).await?;
        for snapshot in [&selected, &all] {
            let deleted = snapshot
                .messages
                .iter()
                .find(|row| row.0.id == parent)
                .expect("deleted parent");
            assert!(matches!(
                deleted.0.content,
                MessageContent::DeletedMessage(_)
            ));
            assert!(deleted.0.raw_bytes.is_empty());
            assert!(deleted.0.fallback.is_none());
            assert!(
                deleted
                    .0
                    .encoded
                    .as_ref()
                    .expect("tombstone")
                    .content
                    .is_empty()
            );
            assert_eq!(deleted.0.reply_count, 0);
            assert!(deleted.0.reactions.is_empty());
            let answer = snapshot
                .messages
                .iter()
                .find(|row| row.0.id == reply)
                .expect("reply");
            let embedded = answer.0.in_reply_to.as_ref().expect("deleted reply parent");
            assert!(matches!(embedded.content, MessageBody::DeletedMessage(_)));
            assert!(embedded.raw_bytes.is_empty());
            assert!(embedded.fallback.is_none());
            assert!(
                embedded
                    .encoded
                    .as_ref()
                    .expect("parent tombstone")
                    .content
                    .is_empty()
            );
            assert!(!format!("{:?}", deleted.0).contains("snapshot-private"));
            assert!(!format!("{embedded:?}").contains("snapshot-private"));
        }
    }
    client.end().await?;
    peer.end().await?;
}
