use super::*;

// verifies: CTYPE-010, SEND-021
#[xmtp_common::test(unwrap_try = true)]
async fn encoded_sends_use_catalogue_push_defaults_and_explicit_override() {
    use crate::{Reaction, ReactionAction, ReactionSchema, SendOptions};
    use xmtp_content_types::{ContentCodec, reaction::ReactionCodec};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("reference".into(), None).await?;
    let reaction = || Reaction {
        content: "👍".into(),
        action: ReactionAction::Added,
        schema: ReactionSchema::Unicode,
    };
    let encoded_reaction = || {
        ReactionCodec::encode(reaction().into_proto(
            parent.checked()?.to_owned(),
            client.inbox_id().into_checked()?,
        ))
        .map_err(XmtpError::from_core)
        .and_then(crate::EncodedContent::try_from)
    };
    let stored_push = |id: &MessageId| {
        client
            .inner
            .message(id.to_bytes().expect("message ID"))
            .expect("stored message")
            .should_push
    };

    let raw = group.send(encoded_reaction()?, None).await?;
    let raw_push = stored_push(&raw);
    let action = client
        .conversations()
        .react_to_message(parent.clone(), reaction(), None)
        .await?;
    let action_push = stored_push(&action);
    let prepared = group.prepare_message(encoded_reaction()?, None).await?;
    let prepared_push = stored_push(&prepared);
    let text = group.send(crate::encode_text("text".into())?, None).await?;
    let text_push = stored_push(&text);
    let override_id = group
        .send(
            encoded_reaction()?,
            Some(SendOptions {
                should_push: Some(true),
                ..Default::default()
            }),
        )
        .await?;
    let override_push = stored_push(&override_id);
    let reply = client
        .conversations()
        .reply_to_message(parent, crate::encode_text("reply".into())?, None)
        .await?;
    let reply_push = stored_push(&reply);
    let observed = [
        raw_push,
        action_push,
        prepared_push,
        text_push,
        override_push,
        reply_push,
    ];
    assert_eq!(
        observed,
        [false, false, false, true, true, true],
        "raw, action, and prepared reactions must use the catalogue push default"
    );
    client.end().await?;
}

// verifies: CTYPE-008, CTYPE-009
#[xmtp_common::test(unwrap_try = true)]
async fn invalid_reply_parent_body_does_not_break_reads() {
    use crate::MessageBody;
    use xmtp_content_types::{
        ContentCodec,
        actions::{Actions, ActionsCodec},
        group_updated::GroupUpdatedCodec,
    };
    use xmtp_db::{Store, group_message::QueryGroupMessage};
    use xmtp_proto::xmtp::mls::message_contents::GroupUpdated;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One"}]}"#,
    )?;
    // An older peer can still send a transcript type as application content.
    // The send checks refuse it here, so that parent is stored directly.
    let parents = [
        (
            "group_updated",
            GroupUpdatedCodec::encode(GroupUpdated {
                initiated_by_inbox_id: String::new(),
                ..Default::default()
            })?,
        ),
        ("actions", ActionsCodec::encode(actions)?),
    ];
    let reader = group.message_reader(None).await?;
    for (kind, content) in parents {
        let parent_id = if kind == "group_updated" {
            let template = group.inner.prepare_message_for_later_publish(
                b"parent template",
                false,
                Some("template".into()),
            )?;
            let db = group.inner.context.db();
            let mut stored = db.get_group_message(&template)?.unwrap();
            let bytes = prost::Message::encode_to_vec(&content);
            stored.id = xmtp_mls::utils::id::calculate_message_id(
                group.inner.group_id,
                &bytes,
                "forged-parent",
            );
            stored.decrypted_message_bytes = bytes;
            stored.idempotency_key = "forged-parent".into();
            stored.store(&db)?;
            MessageId::from_bytes(&stored.id)?
        } else {
            group.send(content.try_into()?, None).await?
        };
        let reply_id = client
            .conversations()
            .reply_to_message(parent_id, crate::encode_text("reply".into())?, None)
            .await?;
        let by_id = client
            .conversations()
            .get_message_by_id(reply_id.clone())
            .await?
            .expect("reply by ID");
        let history = group
            .messages(None)
            .await?
            .into_iter()
            .find(|message| message.0.id == reply_id)
            .expect("reply in history");
        let streamed = xmtp_common::time::timeout(Duration::from_secs(5), async {
            loop {
                let item = reader.next().await?.expect("reply in stream");
                if item.0.id == reply_id {
                    break Ok::<_, XmtpError>(item);
                }
            }
        })
        .await??;
        for (path, message) in [("by ID", by_id), ("history", history), ("stream", streamed)] {
            assert!(
                matches!(&message.0.content, MessageContent::Reply { body: MessageBody::Text(text), .. } if text == "reply"),
                "{path} changed the reply body for {kind}"
            );
            assert!(
                matches!(message.0.in_reply_to.as_ref().map(|parent| &parent.content), Some(MessageBody::Unknown { encoded: Some(encoded), .. }) if encoded.r#type.type_id == kind),
                "{path} did not keep the failed {kind} parent body as Unknown"
            );
        }
    }
    reader.end().await?;
    client.end().await?;
}

/// A reply parent that has expired, but that cleanup has not deleted, is
/// omitted. The parent comes from the same relation read as the reply, so no
/// later unrestricted reload can return it.
// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn reply_parent_expired_before_lookup_is_omitted() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent_id = group.send_text("parent".into(), None).await?;
    let reply_id = client
        .conversations()
        .reply_to_message(parent_id.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    let before = client
        .conversations()
        .get_message_by_id(reply_id.clone())
        .await?
        .expect("reply");
    assert!(before.0.in_reply_to.is_some());

    let parent_bytes = parent_id.to_bytes()?;
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&parent_bytes)))
            .set(dsl::expire_at_ns.eq(Some(1_i64)))
            .execute(conn)
    })?;
    let by_id = client
        .conversations()
        .get_message_by_id(reply_id.clone())
        .await?
        .expect("reply");
    assert!(
        by_id.0.in_reply_to.is_none(),
        "lookup returned an expired parent"
    );
    let history = group
        .messages(None)
        .await?
        .into_iter()
        .find(|message| message.0.id == reply_id)
        .expect("reply in history");
    assert!(
        history.0.in_reply_to.is_none(),
        "history returned an expired parent"
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deleted_messages_and_reply_parents_hide_original_content() {
    use crate::MessageBody;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut text = crate::encode_text("secret-deleted".into())?;
    text.fallback = Some("secret-deleted-fallback".into());
    let target = group.send(text, None).await?;
    let reply = client
        .conversations()
        .reply_to_message(target.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    let reader = group.message_reader(None).await?;
    let original = xmtp_common::time::timeout(Duration::from_secs(5), async {
        loop {
            let item = reader.next().await?.expect("original message");
            if item.0.id == target {
                break Ok::<_, XmtpError>(item);
            }
        }
    })
    .await??;
    assert!(
        matches!(original.0.content, MessageContent::Text(ref text) if text == "secret-deleted")
    );
    reader.end().await?;
    client
        .conversations()
        .delete_message(target.clone())
        .await?;

    let replay = group.message_reader(None).await?;
    let replayed = xmtp_common::time::timeout(Duration::from_secs(5), async {
        loop {
            let item = replay.next().await?.expect("deleted message replay");
            if item.0.id == target {
                break Ok::<_, XmtpError>(item);
            }
        }
    })
    .await??;
    replay.end().await?;

    let by_id = client
        .conversations()
        .get_message_by_id(target.clone())
        .await?
        .expect("deleted message by ID");
    let history = group.messages(None).await?;
    let listed = history
        .iter()
        .find(|message| message.0.id == target)
        .expect("deleted message in history");
    for (path, message) in [
        ("by ID", &by_id),
        ("history", listed),
        ("reader replay", &replayed),
    ] {
        assert!(
            matches!(message.0.content, MessageContent::DeletedMessage(_)),
            "{path} did not show deletion"
        );
        assert_eq!(
            message
                .0
                .content_type
                .as_ref()
                .expect("content type")
                .authority_id,
            "xmtp.org",
            "{path}"
        );
        assert_eq!(
            message
                .0
                .content_type
                .as_ref()
                .expect("content type")
                .type_id,
            "deletedMessage",
            "{path}"
        );
        assert_eq!(
            message
                .0
                .content_type
                .as_ref()
                .expect("content type")
                .version_major,
            1,
            "{path}"
        );
        assert_eq!(
            message
                .0
                .content_type
                .as_ref()
                .expect("content type")
                .version_minor,
            0,
            "{path}"
        );
        assert!(message.0.raw_bytes.is_empty(), "{path} kept original bytes");
        assert!(message.0.fallback.is_none(), "{path} kept the fallback");
        assert!(
            message
                .0
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .content
                .is_empty(),
            "{path} kept the payload"
        );
        assert_eq!(
            message
                .0
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .r#type
                .type_id,
            "deletedMessage",
            "{path}"
        );
        assert!(
            message
                .0
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .parameters
                .is_empty(),
            "{path}"
        );
        assert!(
            !format!("{:?}", message.0).contains("secret-deleted"),
            "{path}"
        );
    }

    let reply_by_id = client
        .conversations()
        .get_message_by_id(reply.clone())
        .await?
        .expect("reply by ID");
    let reply_in_history = history
        .iter()
        .find(|message| message.0.id == reply)
        .expect("reply in history");
    for (path, message) in [("by ID", &reply_by_id), ("history", reply_in_history)] {
        let parent = message.0.in_reply_to.as_ref().expect("reply parent");
        assert!(
            matches!(parent.content, MessageBody::DeletedMessage(_)),
            "{path}"
        );
        assert_eq!(
            parent.content_type.as_ref().expect("content type").type_id,
            "deletedMessage",
            "{path}"
        );
        assert!(parent.raw_bytes.is_empty(), "{path} kept parent bytes");
        assert!(parent.fallback.is_none(), "{path} kept the parent fallback");
        assert!(
            parent
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .content
                .is_empty(),
            "{path} kept the parent payload"
        );
        assert_eq!(
            parent
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .r#type
                .type_id,
            "deletedMessage",
            "{path}"
        );
        assert!(
            parent
                .encoded
                .as_ref()
                .expect("usable encoded content")
                .parameters
                .is_empty(),
            "{path}"
        );
        assert!(!format!("{parent:?}").contains("secret-deleted"), "{path}");
    }
    client.end().await?;
}

// verifies: CTYPE-009
#[xmtp_common::test(unwrap_try = true)]
async fn failed_standard_reply_parent_decode_stays_unknown() {
    use crate::MessageBody;
    use prost::Message as _;
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent_id = group.send_text("parent".into(), None).await?;
    let reply_id = client
        .conversations()
        .reply_to_message(parent_id.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    let parent_bytes = parent_id.to_bytes()?;
    let stored = client.inner.message(parent_bytes.clone())?;
    let mut encoded = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
    encoded.content = vec![0xff, 0xfe];
    let raw = encoded.encode_to_vec();
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(parent_bytes)))
            .set(dsl::decrypted_message_bytes.eq(&raw))
            .execute(conn)
    })?;

    let by_id = client
        .conversations()
        .get_message_by_id(reply_id.clone())
        .await?
        .expect("reply by ID");
    let history = group.messages(None).await?;
    let listed = history
        .iter()
        .find(|message| message.0.id == reply_id)
        .expect("reply in history");
    for (path, message) in [("by ID", &by_id), ("history", listed)] {
        let parent = message.0.in_reply_to.as_ref().expect("reply parent");
        assert!(
            matches!(&parent.content, MessageBody::Unknown { encoded: Some(encoded), .. }
                if encoded.r#type.type_id == "text" && encoded.content == vec![0xff, 0xfe]),
            "{path} treated a failed text decode as a custom codec"
        );
    }
    client.end().await?;
}

/// A receiver sees who deleted a message: the sender, or the super admin with
/// the super admin's inbox ID.
#[xmtp_common::test(unwrap_try = true)]
async fn deleted_message_reports_sender_or_admin_deleter() {
    use crate::{Conversation, DeletedBy};

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    bo.conversations().sync().await?;
    let Some(Conversation::Group { group: bo_group }) =
        bo.conversations().get_by_id(group.id()).await?
    else {
        panic!("bo must receive the group");
    };
    let by_admin = bo_group.send_text("deleted by admin".into(), None).await?;
    let by_sender = bo_group.send_text("deleted by sender".into(), None).await?;

    group.sync().await?;
    // A deletion is queued like a prepared message, so publish it.
    group.delete_message(by_admin.clone()).await?;
    group.publish_messages().await?;
    bo_group.delete_message(by_sender.clone()).await?;
    bo_group.publish_messages().await?;
    bo_group.sync().await?;

    let deleted_by = |id: MessageId| {
        let bo = &bo;
        async move {
            let message = bo
                .conversations()
                .get_message_by_id(id)
                .await?
                .expect("deleted message by ID");
            let MessageContent::DeletedMessage(deleted) = message.0.content else {
                panic!("expected a deleted message, got {:?}", message.0.content);
            };
            Ok::<_, XmtpError>(deleted.deleted_by)
        }
    };
    let admin = deleted_by(by_admin).await?;
    assert!(
        matches!(&admin, DeletedBy::Admin { inbox_id } if *inbox_id == alix.inbox_id()),
        "{admin:?}"
    );
    let sender = deleted_by(by_sender).await?;
    assert!(matches!(sender, DeletedBy::Sender), "{sender:?}");

    alix.end().await?;
    bo.end().await?;
}
