//! Client group tests: expired messages.

use super::*;

// verifies: CONS-048
#[xmtp_common::test(unwrap_try = true)]
async fn expired_messages_do_not_set_listing_or_activity_time() {
    use crate::test::mock::generate_stored_msg;
    use xmtp_db::Store;
    use xmtp_proto::types::Cursor;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let expired_only = alix.create_group(None, None)?;
    let db = alix.context.db();
    let now = now_ns();
    for (cursor, group_id, sent_at_ns, expire_at_ns) in [
        (100, group.group_id, 200, now + xmtp_common::NS_IN_DAY),
        (200, group.group_id, 300, now - 1),
        (300, expired_only.group_id, 400, now - 1),
    ] {
        let mut message = generate_stored_msg(Cursor(cursor), group_id);
        message.sent_at_ns = sent_at_ns;
        message.expire_at_ns = Some(expire_at_ns);
        message.store(&db)?;
    }

    let listed = alix.list_conversations(GroupQueryArgs::default())?;
    let listing_time = |group_id| {
        listed
            .iter()
            .find(|item| item.group.group_id == group_id)
            .map(|item| item.listing_time_ns)
            .expect("group remains listed")
    };
    assert_eq!(group.last_activity_ns(None)?, 200);
    assert_eq!(listing_time(group.group_id), 200);
    assert_eq!(
        expired_only.last_activity_ns(None)?,
        expired_only.created_at_ns
    );
    assert_eq!(
        listing_time(expired_only.group_id),
        expired_only.created_at_ns
    );
}

// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn expired_message_is_absent_from_history_and_direct_lookup() {
    use crate::test::mock::generate_stored_msg;
    use xmtp_db::Store;
    use xmtp_proto::types::Cursor;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    let make_message = |cursor,
                        expiry|
     -> Result<
        xmtp_db::group_message::StoredGroupMessage,
        xmtp_content_types::CodecError,
    > {
        let mut message = generate_stored_msg(Cursor(cursor), group.group_id);
        message.decrypted_message_bytes =
            TextCodec::encode(format!("message {cursor}"))?.encode_to_vec();
        message.expire_at_ns = expiry;
        Ok(message)
    };
    let expired = make_message(100, Some(now_ns() - 1))?;
    let future = make_message(200, Some(i64::MAX))?;
    let persistent = make_message(300, None)?;
    for message in [&expired, &future, &persistent] {
        message.store(&db)?;
    }

    assert!(db.get_group_message(&expired.id)?.is_some());
    let history = group.find_messages(&MsgQueryArgs::default())?;
    assert!(!history.iter().any(|message| message.id == expired.id));
    assert!(history.iter().any(|message| message.id == future.id));
    assert!(history.iter().any(|message| message.id == persistent.id));
    assert!(matches!(
        alix.message(expired.id.clone()),
        Err(crate::client::ClientError::Storage(
            xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(_))
        ))
    ));
    assert!(alix.message_with_group(&expired.id).await?.is_none());
    assert!(matches!(
        alix.message_v2(expired.id.clone()),
        Err(crate::client::ClientError::Storage(
            xmtp_db::StorageError::NotFound(xmtp_db::NotFound::MessageById(_))
        ))
    ));
    for id in [&future.id, &persistent.id] {
        assert_eq!(alix.message(id.clone())?.id, *id);
        assert_eq!(alix.message_v2(id.clone())?.metadata.id, *id);
        assert!(alix.message_with_group(id).await?.is_some());
    }
}

// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn expired_reply_parent_is_not_enriched() {
    use crate::messages::decoded_message::MessageBody;
    use crate::test::mock::generate_stored_msg;
    use xmtp_content_types::reply::{Reply as EncodedReply, ReplyCodec};
    use xmtp_db::{Store, group_message::ContentType};
    use xmtp_proto::types::Cursor;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    let mut parent = generate_stored_msg(Cursor(100), group.group_id);
    parent.decrypted_message_bytes = TextCodec::encode("parent".into())?.encode_to_vec();
    parent.expire_at_ns = Some(now_ns() - 1);
    parent.store(&db)?;
    let mut reply = generate_stored_msg(Cursor(200), group.group_id);
    reply.decrypted_message_bytes = ReplyCodec::encode(EncodedReply {
        reference: hex::encode(&parent.id),
        reference_inbox_id: None,
        content: TextCodec::encode("reply".into())?,
    })?
    .encode_to_vec();
    reply.content_type = ContentType::Reply;
    reply.reference_id = Some(parent.id.clone());
    reply.store(&db)?;

    assert!(db.get_group_message(&parent.id)?.is_some());
    let enriched = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
    let actual = enriched
        .iter()
        .find(|item| item.stored.id == reply.id)
        .expect("visible reply");
    let MessageBody::Reply(body) = &actual.decoded.content else {
        panic!("expected reply")
    };
    assert!(body.in_reply_to.is_none());
    assert!(actual.parent_stored.is_none());

    let mut visible_parent = parent.clone();
    visible_parent.id = xmtp_common::rand_vec::<32>();
    visible_parent.sequence_id = 300;
    visible_parent.expire_at_ns = Some(i64::MAX);
    visible_parent.store(&db)?;
    let mut visible_reply = reply.clone();
    visible_reply.id = xmtp_common::rand_vec::<32>();
    visible_reply.sequence_id = 400;
    visible_reply.reference_id = Some(visible_parent.id.clone());
    visible_reply.decrypted_message_bytes = ReplyCodec::encode(EncodedReply {
        reference: hex::encode(&visible_parent.id),
        reference_inbox_id: None,
        content: TextCodec::encode("visible reply".into())?,
    })?
    .encode_to_vec();
    visible_reply.store(&db)?;
    let enriched = group.find_messages_v2_with_stored(&MsgQueryArgs::default())?;
    let actual = enriched
        .iter()
        .find(|item| item.stored.id == visible_reply.id)
        .expect("visible reply with visible parent");
    let MessageBody::Reply(body) = &actual.decoded.content else {
        panic!("expected reply")
    };
    assert!(body.in_reply_to.is_some());
    assert_eq!(
        actual.parent_stored.as_ref().map(|item| &item.id),
        Some(&visible_parent.id)
    );
}
