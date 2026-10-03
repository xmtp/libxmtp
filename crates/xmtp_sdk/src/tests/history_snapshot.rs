use super::*;
use futures::FutureExt;
use xmtp_db::delivery::QueryDelivery;

#[xmtp_common::test(unwrap_try = true)]
async fn history_snapshot_fails_before_returning_cursor_for_bad_row() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let good = group.send_text("valid row".into(), None).await?;
    let bad = group
        .send_text("sensitive-history-content".into(), None)
        .await?;
    let unknown = group.send_text("malformed content".into(), None).await?;
    let unknown_bytes = unknown.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&unknown_bytes)))
            .set(dsl::decrypted_message_bytes.eq(vec![0xff]))
            .execute(conn)
    })?;
    let snapshot = group.message_history_snapshot(128).await?;
    assert!(snapshot.messages.iter().any(|message| message.0.id == good));
    assert!(snapshot.messages.iter().any(|message| message.0.id == bad));
    let malformed = snapshot
        .messages
        .iter()
        .find(|message| message.0.id == unknown)
        .expect("malformed content must remain in the snapshot");
    assert!(matches!(
        malformed.0.content,
        MessageContent::Unknown { .. }
    ));
    assert_eq!(malformed.0.raw_bytes, [0xff]);
    let boundary = client.inner.context.db().current_delivery_cursor()?;
    assert_eq!(crate::delivery::cursor::parse(&snapshot.cursor)?, boundary);

    let bad_bytes = bad.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&bad_bytes)))
            .set(dsl::sender_inbox_id.eq(""))
            .execute(conn)
    })?;
    let result = group.message_history_snapshot(128).await;
    assert!(result.is_err(), "an omitted row must not advance the cursor");
    assert!(!format!("{result:?}").contains("sensitive-history-content"));
    client.end().await?;
}

// verifies: DMS-009, DMS-016, PROC-034
#[xmtp_common::test(unwrap_try = true)]
async fn dm_history_snapshot_reads_both_physical_groups_and_replays_after_boundary() {
    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first = a.conversations().create_dm(b.inbox_id(), None).await?;
    let first_id = first.send_text("first physical group".into(), None).await?;
    let second = b.conversations().create_dm(a.inbox_id(), None).await?;
    let second_id = second
        .send_text("second physical group".into(), None)
        .await?;
    assert_ne!(first.id(), second.id());
    a.conversations().sync_all(None).await?;
    b.conversations().sync_all(None).await?;
    let winner = a
        .conversations()
        .get_dm_by_inbox_id(b.inbox_id())
        .await?
        .expect("stitched DM");
    let mut physical = winner.duplicate_dms().await?;
    physical.push(winner.clone());
    assert_eq!(physical.len(), 2);
    let boundary = a.inner.context.db().current_delivery_cursor()?;
    let mut previous = None;
    for dm in &physical {
        let all = dm.message_history_snapshot(128).await?;
        let all_ids = all
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>();
        assert!(
            all_ids.contains(&first_id),
            "first physical group's text is in the union"
        );
        assert!(
            all_ids.contains(&second_id),
            "second physical group's text is in the union"
        );
        assert_eq!(crate::delivery::cursor::parse(&all.cursor)?, boundary);
        let snapshot = dm.message_history_snapshot(2).await?;
        let ids = snapshot
            .messages
            .iter()
            .map(|message| message.0.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids, all_ids[all_ids.len() - 2..]);
        assert_eq!(crate::delivery::cursor::parse(&snapshot.cursor)?, boundary);
        let rows = snapshot
            .messages
            .iter()
            .map(|message| {
                crate::delivery::cursor::parse(
                    message.0.delivery_cursor.as_ref().expect("row cursor"),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert!(rows[0].delivery_sequence < rows[1].delivery_sequence);
        if let Some(expected) = &previous {
            assert_eq!(&rows, expected);
        }
        previous = Some(rows);
        let latest = dm.message_history_snapshot(1).await?;
        assert_eq!(latest.messages.len(), 1);
        assert_eq!(
            &latest.messages[0].0.id,
            all_ids.last().expect("eligible tail")
        );
    }
    let late = winner.send_text("after the boundary".into(), None).await?;
    for dm in &physical {
        let reader = dm
            .message_reader(Some(crate::ConversationMessageReaderOptions {
                from: Some(crate::delivery::cursor::encode(boundary)),
            }))
            .await?;
        let message = tokio::time::timeout(Duration::from_secs(2), reader.next())
            .await??
            .expect("new row after the snapshot");
        assert_eq!(message.0.id, late);
        let cursor = crate::delivery::cursor::parse(
            message.0.delivery_cursor.as_ref().expect("row cursor"),
        )?;
        assert!(cursor.delivery_sequence > boundary.delivery_sequence);
        reader.end().await?;
    }
    a.end().await?;
    b.end().await?;
}

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
