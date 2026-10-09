use super::*;

// verifies: CTYPE-030
#[xmtp_common::test(unwrap_try = true)]
fn decode_standard_rejects_out_of_range_actions_expiry() {
    use xmtp_content_types::{
        ContentCodec,
        actions::{Actions, ActionsCodec},
    };

    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One","expiresAt":"9999-12-31T23:59:59.999Z"}]}"#,
    )?;
    let encoded = ActionsCodec::encode(actions)?;
    assert!(crate::decode_standard(encoded.try_into()?).is_err());
}

// verifies: CTYPE-030, PROC-052
#[xmtp_common::test(unwrap_try = true)]
async fn out_of_range_actions_remain_readable_and_replay_until_acknowledged() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{Actions, ActionsCodec},
    };
    use xmtp_mls::groups::send_message_opts::SendMessageOpts;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One"}]}"#,
    )?;
    let raw = ActionsCodec::encode(actions)?.encode_to_vec();
    let id = MessageId::from_bytes(
        &group
            .inner
            .send_message(&raw, SendMessageOpts::default())
            .await?,
    )?;

    let reader = group.message_reader(None).await?;
    let delivered = xmtp_common::time::timeout(Duration::from_secs(5), reader.next())
        .await??
        .expect("out-of-range actions handoff");
    assert_eq!(delivered.0.id, id);
    assert!(matches!(&delivered.0.content,
        MessageContent::Unknown { raw_bytes, error, .. }
            if *raw_bytes == raw && error.code == "CodecDecodeFailed"));
    reader.end().await?;

    let replacement = group.message_reader(None).await?;
    let replay = xmtp_common::time::timeout(Duration::from_secs(5), replacement.next())
        .await??
        .expect("unacknowledged actions replay");
    assert_eq!(replay.0.id, id);
    assert!(matches!(&replay.0.content,
        MessageContent::Unknown { raw_bytes, error, .. }
            if *raw_bytes == raw && error.code == "CodecDecodeFailed"));
    replacement.end().await?;
    client.end().await?;
}

// verifies: CTYPE-003, CTYPE-007, CTYPE-008, CTYPE-009, CTYPE-024, CTYPE-029, CTYPE-030
#[xmtp_common::test(unwrap_try = true)]
async fn unknown_compression_stays_unknown_on_all_read_paths() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{Actions, ActionsCodec},
        reply::{Reply, ReplyCodec},
        text::TextCodec,
    };
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    #[derive(Clone, Copy)]
    enum Expected {
        Text,
        Actions,
        Raw,
    }

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("parent".into(), None).await?;
    let mut rows: Vec<(&str, MessageId, Vec<u8>, Expected, bool)> = Vec::new();

    let mut invalid_text = crate::encode_text("valid".into())?;
    invalid_text.content = vec![0xff, 0xfe];
    let id = group.send(invalid_text, None).await?;
    let raw = client
        .inner
        .message(id.to_bytes()?)?
        .decrypted_message_bytes;
    rows.push(("invalid UTF-8 text", id, raw, Expected::Text, false));

    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One","expiresAt":"9999-12-31T23:59:59.999Z"}]}"#,
    )?;
    let mut action_only = actions.clone();
    action_only.expires_at = None;
    let mut top_level_only = actions.clone();
    for action in &mut top_level_only.actions {
        action.expires_at = None;
    }
    for (label, value) in [
        ("Actions top-level expiry", top_level_only),
        ("Actions both expiries", actions),
        ("Actions item expiry", action_only),
    ] {
        let encoded = ActionsCodec::encode(value)?;
        let id = group.send(encoded.try_into()?, None).await?;
        let raw = client
            .inner
            .message(id.to_bytes()?)?
            .decrypted_message_bytes;
        rows.push((label, id, raw, Expected::Actions, false));
    }

    let id = group.send_text("valid".into(), None).await?;
    let stored = client.inner.message(id.to_bytes()?)?;
    let mut encoded = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
    encoded.compression = Some(99);
    rows.push((
        "unknown compression",
        id,
        encoded.encode_to_vec(),
        Expected::Text,
        true,
    ));

    for (label, empty_authority) in [("empty outer authority", true), ("empty outer type", false)] {
        let id = group.send_text("valid".into(), None).await?;
        let stored = client.inner.message(id.to_bytes()?)?;
        let mut encoded = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
        let kind = encoded.r#type.as_mut().expect("typed text");
        if empty_authority {
            kind.authority_id.clear();
        } else {
            kind.type_id.clear();
        }
        rows.push((label, id, encoded.encode_to_vec(), Expected::Raw, true));
    }

    for (label, empty_authority) in [
        ("empty nested reply authority", true),
        ("empty nested reply type", false),
    ] {
        let outer = ReplyCodec::encode(Reply {
            reference: parent.checked()?.to_owned(),
            reference_inbox_id: Some(client.inbox_id().into_checked()?),
            content: TextCodec::encode("nested".into())?,
        })?;
        let id = group.send(outer.try_into()?, None).await?;
        let stored = client.inner.message(id.to_bytes()?)?;
        let mut outer = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
        let mut nested = ProtoEncodedContent::decode(outer.content.as_slice())?;
        let kind = nested.r#type.as_mut().expect("typed text");
        if empty_authority {
            kind.authority_id.clear();
        } else {
            kind.type_id.clear();
        }
        outer.content = nested.encode_to_vec();
        rows.push((label, id, outer.encode_to_vec(), Expected::Raw, true));
    }

    assert_eq!(rows.len(), 9);
    let mut message_ids = std::collections::HashSet::new();
    for (label, id, raw, expected, rewrite) in rows {
        let id_bytes = id.to_bytes()?;
        assert!(
            message_ids.insert(id_bytes.clone()),
            "{label}: duplicate message ID"
        );
        if rewrite {
            client.inner.context.db().raw_query(|conn| {
                xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&id_bytes)))
                    .set(dsl::decrypted_message_bytes.eq(&raw))
                    .execute(conn)
            })?;
        }
        if matches!(expected, Expected::Text) {
            assert_undecodable_standard_read_paths(&client, &group, id, &raw, label).await?;
            continue;
        }
        let direct =
            crate::Message::from_stored(client.inner.message(id_bytes)?, client.client_key())?;
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
        for (path, message) in [("direct", direct), ("by ID", by_id), ("history", history)] {
            assert_eq!(message.0.id, id, "{label}: {path} changed the message ID");
            let preserved = match (expected, message.0.content) {
                (
                    Expected::Actions,
                    MessageContent::Unknown {
                        encoded: Some(encoded),
                        raw_bytes,
                        ..
                    },
                ) => encoded.r#type.type_id == "actions" && raw_bytes == raw,
                (Expected::Raw, MessageContent::Unknown { raw_bytes, .. }) => raw_bytes == raw,
                _ => false,
            };
            assert!(
                preserved,
                "{label}: {path} did not preserve raw bytes or type"
            );
        }
    }

    // Rejected calls run after all nine stored-message read checks.
    for empty_authority in [true, false] {
        let mut encoded = crate::encode_text("invalid".into())?;
        if empty_authority {
            encoded.r#type.authority_id.clear();
        } else {
            encoded.r#type.type_id.clear();
        }
        assert!(matches!(
            group.send(encoded.clone(), None).await,
            Err(XmtpError::InvalidInput(_))
        ));
        assert!(matches!(
            group.prepare_message(encoded.clone(), None).await,
            Err(XmtpError::InvalidInput(_))
        ));
        assert!(matches!(
            client
                .conversations()
                .reply_to_message(parent.clone(), encoded, None)
                .await,
            Err(XmtpError::InvalidInput(_))
        ));
    }
    client.end().await?;
}
