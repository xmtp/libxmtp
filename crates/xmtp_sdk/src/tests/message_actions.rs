use super::*;

// verifies: CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
async fn nested_reaction_reply_body_keeps_nested_envelope() {
    use crate::{EncodedContent, MessageBody, Reaction, ReactionAction, ReactionSchema};
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        compression::compress,
        reaction::ReactionCodec,
        reply::{Reply, ReplyCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        Compression as WireCompression, EncodedContent as ProtoEncodedContent,
    };

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reference = group.send_text("reference".into(), None).await?;
    let nested: EncodedContent = ReactionCodec::encode(
        Reaction {
            content: "👍".into(),
            action: ReactionAction::Added,
            schema: ReactionSchema::Unicode,
        }
        .into_proto(
            reference.checked()?.to_owned(),
            client.inbox_id().into_checked()?,
        ),
    )?
    .into();
    let reply_id = client
        .conversations()
        .reply_to_message(reference.clone(), nested, None)
        .await?;
    let stored = client.inner.message(reply_id.to_bytes()?)?;
    let decoded = client
        .decode_content(
            ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?.into(),
        )
        .await?;
    let MessageContent::Reply {
        reference_id,
        body: MessageBody::Unknown { encoded },
    } = decoded
    else {
        panic!("expected an unknown nested reaction body");
    };
    assert_eq!(reference_id, reference);
    assert_eq!(encoded.r#type.type_id, "reaction");

    let reply = group
        .messages(None)
        .await?
        .into_iter()
        .find(|message| message.0.id == reply_id)
        .expect("reply in history");
    let MessageContent::Reply {
        reference_id,
        body: MessageBody::Unknown { encoded },
    } = reply.0.content
    else {
        panic!("expected an unknown nested reaction body in history");
    };
    assert_eq!(reference_id, reference);
    assert_eq!(encoded.r#type.type_id, "reaction");

    let nested = ReactionCodec::encode(
        Reaction {
            content: "compressed reaction".into(),
            action: ReactionAction::Added,
            schema: ReactionSchema::Unicode,
        }
        .into_proto(
            reference.checked()?.to_owned(),
            client.inbox_id().into_checked()?,
        ),
    )?;
    let expected_content = nested.content.clone();
    let compressed = compress(nested, WireCompression::Gzip)?;
    let outer = ReplyCodec::encode(Reply {
        reference: reference.checked()?.to_owned(),
        reference_inbox_id: None,
        content: compressed,
    })?;
    let MessageContent::Reply {
        reference_id,
        body: MessageBody::Unknown { encoded },
    } = MessageContent::decode(outer.encode_to_vec())?
    else {
        panic!("expected unknown nested compressed reaction");
    };
    assert_eq!(reference_id, reference);
    assert_eq!(encoded.r#type.type_id, "reaction");
    assert_eq!(encoded.content, expected_content);
    client.end().await?;
}

// verifies: CTYPE-023, CTYPE-031
#[xmtp_common::test(unwrap_try = true)]
async fn message_actions_use_ids_and_compression_is_opt_in() {
    use crate::{
        Compression, EncodedContent, Reaction, ReactionAction, ReactionSchema, SendOptions,
    };
    use prost::Message as _;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let text: EncodedContent = TextCodec::encode("plain".into())?.into();
    let plain = group.send(text, None).await?;
    let stored = client.inner.message(plain.to_bytes()?)?;
    assert_eq!(
        ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?.compression,
        None
    );

    let compressed: EncodedContent = TextCodec::encode("compressed".into())?.into();
    let compressed_id = group
        .send(
            compressed,
            Some(SendOptions {
                compression: Some(Compression::Gzip),
                ..Default::default()
            }),
        )
        .await?;
    let stored = client.inner.message(compressed_id.to_bytes()?)?;
    assert!(matches!(
        MessageContent::decode(stored.decrypted_message_bytes.clone())?,
        MessageContent::Text(value) if value == "compressed"
    ));
    assert!(
        ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?
            .compression
            .is_some()
    );
    let read_back = client
        .conversations()
        .get_message_by_id(compressed_id)
        .await?
        .expect("compressed message");
    assert!(matches!(read_back.0.content, MessageContent::Text(value) if value == "compressed"));
    assert!(
        client
            .conversations()
            .get_message_by_id(plain.clone())
            .await?
            .is_some()
    );

    let reaction_id = client
        .conversations()
        .react_to_message(
            plain.clone(),
            Reaction {
                content: "👍".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            None,
        )
        .await?;
    let reply: EncodedContent = TextCodec::encode("answer".into())?.into();
    let reply_id = client
        .conversations()
        .reply_to_message(plain.clone(), reply, None)
        .await?;
    assert_ne!(reaction_id, reply_id);
    assert_ne!(plain, reply_id);
    let enriched = group.messages(None).await?;
    let original = enriched
        .iter()
        .find(|value| value.0.id == plain)
        .expect("original message");
    assert_eq!(original.0.reply_count, 1);
    assert_eq!(original.0.reactions.len(), 1);
    assert_eq!(original.0.reactions[0].id, reaction_id);
    assert_eq!(original.0.reactions[0].reaction.content, "👍");
    assert!(matches!(
        original.0.reactions[0].reaction.action,
        ReactionAction::Added
    ));
    assert!(matches!(
        original.0.reactions[0].reaction.schema,
        ReactionSchema::Unicode
    ));
    let answer = enriched
        .iter()
        .find(|value| value.0.id == reply_id)
        .expect("reply message");
    assert_eq!(
        answer.0.in_reply_to.as_ref().map(|parent| &parent.id),
        Some(&plain)
    );
    assert!(matches!(
        &answer.0.content,
        MessageContent::Reply { reference_id, .. } if reference_id == &plain
    ));
    assert!(
        !answer
            .0
            .in_reply_to
            .as_ref()
            .expect("parent")
            .encoded
            .content
            .is_empty()
    );
    let local_message = group.send_text("delete through group".into(), None).await?;
    assert_ne!(
        group.delete_message(local_message.clone()).await?,
        local_message
    );
    let deleted = client.conversations().delete_message(plain.clone()).await?;
    assert_ne!(deleted, plain);
    client.end().await?;
}

// verifies: CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn unknown_message_bytes_remain_available_to_the_host() {
    use prost::Message as _;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group.send_text("fallback".into(), None).await?;
    let mut stored = client.inner.message(id.to_bytes()?)?;
    let original = vec![0xff, 0x00, 0x80];
    stored.decrypted_message_bytes = original.clone();
    let message = crate::Message::from_stored(stored, client.client_key())?;
    let MessageContent::Unknown { raw_bytes, .. } = &message.0.content else {
        panic!("untyped bytes must remain unknown");
    };
    assert_eq!(raw_bytes, &original);
    assert_eq!(message.0.encoded.content, original);

    let mut stored = client.inner.message(id.to_bytes()?)?;
    let mut proto = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
    proto.compression = Some(12_345);
    let original_content = proto.content.clone();
    let original_bytes = proto.encode_to_vec();
    stored.decrypted_message_bytes = original_bytes.clone();
    let message = crate::Message::from_stored(stored, client.client_key())?;
    let MessageContent::Unknown { encoded, raw_bytes } = &message.0.content else {
        panic!("unknown compression must remain unknown");
    };
    assert_eq!(raw_bytes, &original_bytes);
    assert!(encoded.content.is_empty());
    assert!(message.0.encoded.content.is_empty());
    assert!(!original_content.is_empty());
    client.end().await?;
}

// verifies: CTYPE-024, CTYPE-025
#[xmtp_common::test(unwrap_try = true)]
fn decode_rejects_compression_bomb_with_bounded_output() {
    use flate2::{Compression as FlateCompression, write::ZlibEncoder};
    use prost::Message as _;
    use std::io::Write;
    use xmtp_content_types::{
        ContentCodec,
        compression::{COMPRESSION_CHUNK_BYTES, DecompressionBudget, MAX_DECOMPRESSED_BYTES},
        text::TextCodec,
    };
    use xmtp_proto::xmtp::mls::message_contents::Compression as WireCompression;

    let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
    encoder.write_all(&vec![b'x'; MAX_DECOMPRESSED_BYTES + 1])?;
    let mut content = TextCodec::encode("placeholder".into())?;
    content.content = encoder.finish()?;
    content.compression = Some(WireCompression::Deflate as i32);
    let error = MessageContent::decode(content.clone().encode_to_vec()).unwrap_err();
    assert!(
        error.to_string().contains("decompressed content exceeds"),
        "unexpected decode error: {error}"
    );
    let mut budget = DecompressionBudget::new();
    assert!(xmtp_content_types::compression::decompress_with_budget(content, &mut budget).is_err());
    assert!(budget.peak_capacity() <= MAX_DECOMPRESSED_BYTES + COMPRESSION_CHUNK_BYTES);
}
