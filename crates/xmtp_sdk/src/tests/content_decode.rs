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
        ReactionCodec::encode(reaction().into_proto(parent.clone(), client.inbox_id()))
            .map(Into::into)
    };
    let stored_push = |id: &MessageId| {
        client
            .inner
            .message(hex::decode(&id.0).expect("message ID"))
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
async fn invalid_text_bytes_stay_unknown_on_all_read_paths() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut encoded = crate::encode_text("valid".into())?;
    encoded.content = vec![0xff, 0xfe];
    let id = group.send(encoded, None).await?;
    let raw = client
        .inner
        .message(hex::decode(&id.0)?)?
        .decrypted_message_bytes;
    assert_undecodable_standard_read_paths(&client, &group, id, &raw).await?;
    client.end().await?;
}

// verifies: CTYPE-007, CTYPE-008, CTYPE-030
#[xmtp_common::test(unwrap_try = true)]
async fn actions_with_out_of_range_expiry_stay_unknown_on_all_read_paths() {
    use xmtp_content_types::{
        ContentCodec,
        actions::{Actions, ActionsCodec},
    };

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One","expiresAt":"9999-12-31T23:59:59.999Z"}]}"#,
    )?;
    let mut action_only = actions.clone();
    action_only.expires_at = None;
    let mut top_level_only = actions.clone();
    for action in &mut top_level_only.actions {
        action.expires_at = None;
    }
    for actions in [top_level_only, actions, action_only] {
        let encoded = ActionsCodec::encode(actions)?;
        let id = group.send(encoded.into(), None).await?;
        let stored = client.inner.message(hex::decode(&id.0)?)?;
        let raw = stored.decrypted_message_bytes.clone();
        let direct = crate::Message::from_stored(stored, client.client_key())?;
        let by_id = client
            .conversations()
            .get_message_by_id(id.clone())
            .await?
            .expect("message by ID");
        let history = group
            .messages(None)
            .await?
            .into_iter()
            .find(|message| message.0.id == id)
            .expect("message in history");
        for (path, message) in [("stored", direct), ("by ID", by_id), ("history", history)] {
            assert!(
                matches!(message.0.content, MessageContent::Unknown { encoded, raw_bytes }
                    if encoded.r#type.type_id == "actions" && raw_bytes == raw),
                "{path} changed an out-of-range Actions expiry"
            );
        }
    }
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
    use xmtp_proto::xmtp::mls::message_contents::GroupUpdated;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One"}]}"#,
    )?;
    let parents = [
        (
            "group_updated",
            GroupUpdatedCodec::encode(GroupUpdated {
                initiated_by_inbox_id: String::new(),
                ..Default::default()
            })?
            .into(),
        ),
        ("actions", ActionsCodec::encode(actions)?.into()),
    ];
    let reader = group.message_reader(None).await?;
    for (kind, content) in parents {
        let parent_id = group.send(content, None).await?;
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
                matches!(message.0.in_reply_to.as_ref().map(|parent| &parent.content), Some(MessageBody::Unknown { encoded }) if encoded.r#type.type_id == kind),
                "{path} did not keep the failed {kind} parent body as Unknown"
            );
        }
    }
    reader.end().await?;
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
        assert_eq!(message.0.content_type.authority_id, "xmtp.org", "{path}");
        assert_eq!(message.0.content_type.type_id, "deletedMessage", "{path}");
        assert_eq!(message.0.content_type.version_major, 1, "{path}");
        assert_eq!(message.0.content_type.version_minor, 0, "{path}");
        assert!(message.0.fallback.is_none(), "{path} kept the fallback");
        assert!(
            message.0.encoded.content.is_empty(),
            "{path} kept the payload"
        );
        assert_eq!(message.0.encoded.r#type.type_id, "deletedMessage", "{path}");
        assert!(message.0.encoded.parameters.is_empty(), "{path}");
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
        assert_eq!(parent.content_type.type_id, "deletedMessage", "{path}");
        assert!(parent.fallback.is_none(), "{path} kept the parent fallback");
        assert!(
            parent.encoded.content.is_empty(),
            "{path} kept the parent payload"
        );
        assert_eq!(parent.encoded.r#type.type_id, "deletedMessage", "{path}");
        assert!(parent.encoded.parameters.is_empty(), "{path}");
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
    let parent_bytes = hex::decode(&parent_id.0)?;
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
            matches!(&parent.content, MessageBody::Unknown { encoded }
                if encoded.r#type.type_id == "text" && encoded.content == vec![0xff, 0xfe]),
            "{path} treated a failed text decode as a custom codec"
        );
    }
    client.end().await?;
}
