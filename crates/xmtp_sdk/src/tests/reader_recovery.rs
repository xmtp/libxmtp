use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn streamed_reply_has_the_same_context_as_message_by_id() {
    use crate::{Reaction, ReactionAction, ReactionSchema};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent_id = group.send_text("parent".into(), None).await?;
    let reaction = Reaction {
        content: "👍".into(),
        action: ReactionAction::Added,
        schema: ReactionSchema::Unicode,
    };
    client
        .conversations()
        .react_to_message(parent_id.clone(), reaction.clone(), None)
        .await?;
    let reply_id = client
        .conversations()
        .reply_to_message(parent_id.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    let reaction_id = client
        .conversations()
        .react_to_message(reply_id.clone(), reaction, None)
        .await?;
    client
        .conversations()
        .reply_to_message(reply_id.clone(), crate::encode_text("child".into())?, None)
        .await?;

    let reader = group.message_reader(None).await?;
    assert_eq!(
        reader.next().await?.expect("parent handoff").0.id,
        parent_id
    );
    let streamed = xmtp_common::time::timeout(Duration::from_secs(5), async {
        loop {
            let item = reader.next().await?.expect("reply handoff");
            if item.0.id == reply_id {
                break Ok::<_, XmtpError>(item);
            }
        }
    })
    .await??;
    assert_eq!(streamed.0.id, reply_id);
    let direct = client
        .conversations()
        .get_message_by_id(reply_id)
        .await?
        .expect("reply by ID");
    assert_eq!(direct.0.reply_count, 1);
    assert_eq!(direct.0.reactions[0].id, reaction_id);
    assert_eq!(
        direct.0.in_reply_to.as_ref().map(|parent| &parent.id),
        Some(&parent_id)
    );
    assert_eq!(streamed.0.reply_count, direct.0.reply_count);
    assert_eq!(streamed.0.reactions.len(), direct.0.reactions.len());
    assert_eq!(streamed.0.reactions[0].id, direct.0.reactions[0].id);
    assert_eq!(
        streamed.0.in_reply_to.as_ref().map(|parent| &parent.id),
        direct.0.in_reply_to.as_ref().map(|parent| &parent.id)
    );
    reader.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn late_reader_released() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reader = group.message_reader(None).await?;
    let control = reader.control_for_test();
    drop(reader);
    assert_eq!(
        crate::ConnectionState::from(control.catch_up_snapshot().connection),
        crate::ConnectionState::Closed
    );
    let replacement = group.message_reader(None).await?;
    replacement.end().await?;
    client.end().await?;
}

// verifies: CTYPE-008, PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn raw_message_bytes_are_delivered_and_replayed_until_acknowledged() {
    use xmtp_mls::groups::send_message_opts::SendMessageOpts;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let raw = b"\xff\x00 not an EncodedContent protobuf";
    let id = MessageId::from_bytes(
        &group
            .inner
            .send_message(raw, SendMessageOpts::default())
            .await?,
    )?;
    let reader = group.message_reader(None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("raw message handoff");
    assert_eq!(delivered.0.id, id);
    assert!(matches!(&delivered.0.content,
        MessageContent::Unknown { raw_bytes, .. } if raw_bytes == raw));
    reader.end().await?;

    let replacement = group.message_reader(None).await?;
    let replayed = xmtp_common::time::timeout(Duration::from_secs(5), replacement.next())
        .await??
        .expect("unacknowledged raw message replay");
    assert_eq!(replayed.0.id, id);
    assert!(matches!(&replayed.0.content,
        MessageContent::Unknown { raw_bytes, .. } if raw_bytes == raw));
    replacement.end().await?;
    client.end().await?;
}

// verifies: PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn message_decode_error_closes_reader_and_releases_lease() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    group
        .send_text("invalid stored message".into(), None)
        .await?;
    let reader = group.message_reader(None).await?;
    reader.corrupt_next_message_for_test();
    assert!(
        xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
            .await
            .expect("invalid message read timed out")
            .is_err(),
        "invalid ID must fail conversion"
    );
    assert!(reader.is_ended_for_test(), "decode error left reader open");
    let replacement = group.message_reader(None).await?;
    replacement.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_conversion_error_closes_reader() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client.conversations().conversation_reader(None).await?;
    reader.fail_next_conversion_for_test();
    let idle = reader.idle_read_for_test();
    let waiting = reader.clone();
    let read = tokio::spawn(async move { waiting.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), idle.notified()).await?;
    client.conversations().create_group(vec![], None).await?;
    assert!(
        xmtp_common::time::timeout(Duration::from_secs(5), read)
            .await??
            .is_err(),
        "injected conversion must fail the pending read"
    );
    assert_eq!(
        reader.connection_state().await,
        crate::ConnectionState::Closed
    );
    let replacement = client.conversations().conversation_reader(None).await?;
    replacement.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_reader_rereads_after_fall_behind() {
    use std::collections::HashSet;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client.conversations().conversation_reader(None).await?;
    let idle = reader.idle_read_for_test();
    let waiting_reader = reader.clone();
    let pending = tokio::spawn(async move { waiting_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), idle.notified()).await?;
    assert!(
        !pending.is_finished(),
        "reader must start before new groups"
    );
    let first = client.conversations().create_group(vec![], None).await?;
    let first_id = first.id();
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), pending)
        .await???
        .expect("first group");
    assert!(matches!(delivered, crate::Conversation::Group { group } if group.id() == first_id));

    let mut expected = HashSet::new();
    // The core event hint queue holds ten entries. Its overflow must be read
    // after the database scan has delivered all groups.
    for _ in 0..13 {
        let group = client.conversations().create_group(vec![], None).await?;
        expected.insert(group.id().into_checked()?);
    }
    for _ in 0..expected.len() {
        let conversation = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
            .await??
            .expect("stored conversation");
        let id = match conversation {
            crate::Conversation::Group { group } => group.id().into_checked()?,
            crate::Conversation::Dm { dm } => dm.id().into_checked()?,
        };
        assert!(expected.remove(&id), "duplicate or unrequested group");
    }
    assert!(expected.is_empty());

    // Stored reads leave a permit. Consume it before waiting for the next read.
    let _ = idle.notified().now_or_never();
    let waiting_reader = reader.clone();
    let pending = tokio::spawn(async move { waiting_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), idle.notified()).await?;
    assert!(
        !pending.is_finished(),
        "lagged hint must not close the reader"
    );
    let final_group = client.conversations().create_group(vec![], None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), pending)
        .await???
        .expect("group after lagged hint");
    assert!(
        matches!(delivered, crate::Conversation::Group { group } if group.id() == final_group.id())
    );
    reader.end().await?;
    client.end().await?;
}
