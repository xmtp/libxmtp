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

// verifies: CTYPE-008, CTYPE-024
#[xmtp_common::test(unwrap_try = true)]
async fn unknown_compression_stays_unknown_on_all_read_paths() {
    use prost::Message as _;
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group.send_text("valid".into(), None).await?;
    let id_bytes = id.to_bytes()?;
    let stored = client.inner.message(id_bytes.clone())?;
    let mut encoded = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
    encoded.compression = Some(99);
    let raw = encoded.encode_to_vec();
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(id_bytes)))
            .set(dsl::decrypted_message_bytes.eq(&raw))
            .execute(conn)
    })?;
    assert_undecodable_standard_read_paths(&client, &group, id, &raw).await?;
    client.end().await?;
}

// verifies: CTYPE-003, CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn empty_content_identifiers_stay_unknown_on_all_read_paths() {
    use prost::Message as _;
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    for empty_authority in [true, false] {
        let id = group.send_text("valid".into(), None).await?;
        let id_bytes = id.to_bytes()?;
        let stored = client.inner.message(id_bytes.clone())?;
        let mut encoded = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
        let kind = encoded.r#type.as_mut().expect("typed text");
        if empty_authority {
            kind.authority_id.clear();
        } else {
            kind.type_id.clear();
        }
        let raw = encoded.encode_to_vec();
        client.inner.context.db().raw_query(|conn| {
            xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&id_bytes)))
                .set(dsl::decrypted_message_bytes.eq(&raw))
                .execute(conn)
        })?;

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
        for (path, message) in [("stored", direct), ("by ID", by_id), ("history", history)] {
            assert!(
                matches!(message.0.content, MessageContent::Unknown { raw_bytes, .. } if raw_bytes == raw),
                "{path} did not preserve an envelope with an empty identifier"
            );
        }
    }
    client.end().await?;
}

// verifies: CTYPE-003, CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn reply_with_empty_nested_identifier_stays_unknown_on_all_read_paths() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        reply::{Reply, ReplyCodec},
        text::TextCodec,
    };
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("parent".into(), None).await?;
    for empty_authority in [true, false] {
        let outer = ReplyCodec::encode(Reply {
            reference: parent.checked()?.to_owned(),
            reference_inbox_id: Some(client.inbox_id().into_checked()?),
            content: TextCodec::encode("nested".into())?,
        })?;
        let id = group.send(outer.try_into()?, None).await?;
        let id_bytes = id.to_bytes()?;
        let stored = client.inner.message(id_bytes.clone())?;
        let mut outer = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
        let mut nested = ProtoEncodedContent::decode(outer.content.as_slice())?;
        let kind = nested.r#type.as_mut().expect("typed text");
        if empty_authority {
            kind.authority_id.clear();
        } else {
            kind.type_id.clear();
        }
        outer.content = nested.encode_to_vec();
        let raw = outer.encode_to_vec();
        client.inner.context.db().raw_query(|conn| {
            xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(&id_bytes)))
                .set(dsl::decrypted_message_bytes.eq(&raw))
                .execute(conn)
        })?;
        let stored = client.inner.message(id_bytes)?;
        let direct = crate::Message::from_stored(stored, client.client_key())?;
        let by_id = client
            .conversations()
            .get_message_by_id(id.clone())
            .await?
            .expect("reply by ID");
        let history = group
            .messages(None)
            .await?
            .into_iter()
            .find(|message| message.0.id == id)
            .expect("reply in history");
        for (path, message) in [("stored", direct), ("by ID", by_id), ("history", history)] {
            assert!(
                matches!(message.0.content, MessageContent::Unknown { raw_bytes, .. } if raw_bytes == raw),
                "{path} did not preserve the outer reply bytes"
            );
        }
    }
    client.end().await?;
}

// verifies: CTYPE-003
#[xmtp_common::test(unwrap_try = true)]
async fn sends_reject_empty_content_identifiers() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("parent".into(), None).await?;
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
