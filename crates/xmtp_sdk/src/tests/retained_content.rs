use super::*;
use prost::Message as _;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

fn failed_content_cases() -> Vec<(&'static str, Vec<u8>, &'static str, bool)> {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    use xmtp_content_types::{compression::MAX_DECOMPRESSED_BYTES, reply::ReplyCodec};
    use xmtp_proto::xmtp::mls::message_contents::Compression as WireCompression;

    let mut text = TextCodec::encode("original".into()).unwrap();
    text.fallback = Some("received fallback".into());
    let mut untyped = text.clone();
    untyped.r#type = None;
    let mut incomplete = text.clone();
    incomplete.r#type.as_mut().unwrap().authority_id.clear();
    let mut compressed = text.clone();
    compressed.compression = Some(99);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&vec![b'x'; MAX_DECOMPRESSED_BYTES + 1])
        .unwrap();
    let mut bomb = text.clone();
    bomb.content = encoder.finish().unwrap();
    bomb.compression = Some(WireCompression::Deflate as i32);
    let nested = ProtoEncodedContent {
        content: vec![0xff],
        ..text.clone()
    };
    let reply = ProtoEncodedContent {
        r#type: Some(ReplyCodec::content_type()),
        parameters: [("reference".into(), "a".repeat(64))].into(),
        fallback: Some("received fallback".into()),
        content: nested.encode_to_vec(),
        ..Default::default()
    };
    vec![
        ("malformed protobuf", vec![0xff], "MalformedEnvelope", false),
        (
            "missing type",
            untyped.encode_to_vec(),
            "MalformedEnvelope",
            false,
        ),
        (
            "incomplete type",
            incomplete.encode_to_vec(),
            "MalformedEnvelope",
            false,
        ),
        (
            "unknown compression",
            compressed.encode_to_vec(),
            "CodecDecodeFailed",
            false,
        ),
        (
            "oversized expansion",
            bomb.encode_to_vec(),
            "CodecDecodeFailed",
            false,
        ),
        (
            "failed nested codec",
            reply.encode_to_vec(),
            "CodecDecodeFailed",
            true,
        ),
    ]
}

fn assert_retained(message: &crate::Message, raw: &[u8], code: &str, has_encoded: bool) {
    let parsed = ProtoEncodedContent::decode(raw).ok();
    let expected_type = parsed.as_ref().and_then(|value| value.r#type.as_ref());
    assert_eq!(message.0.raw_bytes, raw, "message raw bytes");
    assert_eq!(
        message
            .0
            .content_type
            .as_ref()
            .map(|value| (&value.authority_id, &value.type_id)),
        expected_type.map(|value| (&value.authority_id, &value.type_id))
    );
    assert_eq!(
        message.0.fallback,
        parsed.as_ref().and_then(|value| value.fallback.clone())
    );
    assert_eq!(
        message.0.encoded.is_some(),
        has_encoded,
        "usable envelope presence"
    );
    let MessageContent::Unknown {
        encoded,
        raw_bytes,
        error,
    } = &message.0.content
    else {
        panic!("failed content was not Unknown: {:?}", message.0.content);
    };
    assert_eq!(raw_bytes, raw, "Unknown raw bytes");
    assert_eq!(encoded.is_some(), has_encoded);
    assert_eq!(error.code, code);
    assert!(matches!(error.category, crate::ErrorCategory::Input));
    assert!(!error.retryable);
    assert!(!error.message.is_empty());
}

// verifies: CTYPE-008, CTYPE-009, CTYPE-024, CTYPE-025, CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
async fn failed_content_keeps_bytes_details_and_stream_progress() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reader = group.message_reader(None).await?;
    for (name, raw, code, has_encoded) in failed_content_cases() {
        let id = group.send_text(name.into(), None).await?;
        let id_bytes = id.to_bytes()?;
        let reply_id = client
            .conversations()
            .reply_to_message(id.clone(), crate::encode_text("valid reply".into())?, None)
            .await?;
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
            .expect("retained lookup");
        let refreshed = client
            .conversations()
            .get_message_by_id(by_id.0.id.clone())
            .await?
            .expect("retained refresh");
        let history = group
            .messages(None)
            .await?
            .into_iter()
            .find(|value| value.0.id == id)
            .expect("retained history");
        let streamed = xmtp_common::time::timeout(Duration::from_secs(10), async {
            loop {
                let item = reader.next().await?.expect("retained stream item");
                if item.0.id == id {
                    break Ok::<_, XmtpError>(item);
                }
            }
        })
        .await??;
        for message in [direct, by_id, refreshed, history, streamed] {
            assert_eq!(message.0.id, id, "{name}");
            assert_retained(&message, &raw, code, has_encoded);
        }
        let reply = client
            .conversations()
            .get_message_by_id(reply_id)
            .await?
            .expect("reply lookup");
        assert!(
            matches!(&reply.0.content, MessageContent::Reply { body: crate::MessageBody::Text(value), .. } if value == "valid reply")
        );
        let parent = reply.0.in_reply_to.as_ref().expect("retained parent");
        assert_eq!(parent.id, id);
        assert_eq!(parent.raw_bytes, raw);
        assert_eq!(parent.encoded.is_some(), has_encoded);
        let crate::MessageBody::Unknown {
            raw_bytes,
            error,
            encoded,
        } = &parent.content
        else {
            panic!("failed parent body was not Unknown");
        };
        assert_eq!(raw_bytes, &raw);
        assert_eq!(encoded.is_some(), has_encoded);
        assert_eq!(error.code, code);
        assert!(!error.retryable);
        assert!(matches!(error.category, crate::ErrorCategory::Input));
    }
    let good_id = group.send_text("stream still works".into(), None).await?;
    let good = xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            let item = reader.next().await?.expect("next valid item");
            if item.0.id == good_id {
                break Ok::<_, XmtpError>(item);
            }
        }
    })
    .await??;
    assert!(matches!(good.0.content, MessageContent::Text(value) if value == "stream still works"));
    reader.end().await?;
    client.end().await?;
}

// verifies: CTYPE-009, CTYPE-024, CTYPE-025, CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
fn custom_reply_keeps_pre_decompression_serialization() {
    use xmtp_content_types::{compression::compress, reply::ReplyCodec};
    use xmtp_proto::xmtp::mls::message_contents::Compression as WireCompression;
    let mut nested = TextCodec::encode("custom payload".into())?;
    nested.r#type.as_mut().unwrap().authority_id = "custom.example".into();
    let compressed = compress(nested, WireCompression::Gzip)?;
    let mut raw = compressed.encode_to_vec();
    // Unknown protobuf field 127: a new encoding would drop this evidence.
    raw.extend_from_slice(&[0xf8, 0x07, 0x01]);
    let MessageContent::Custom { encoded, raw_bytes } = MessageContent::decode(raw.clone())? else {
        panic!("custom envelope");
    };
    assert_eq!(raw_bytes, raw);
    assert_eq!(encoded.content, b"custom payload");
    let outer = ProtoEncodedContent {
        r#type: Some(ReplyCodec::content_type()),
        parameters: [("reference".into(), "a".repeat(64))].into(),
        content: raw.clone(),
        ..Default::default()
    };
    let outer = compress(outer, WireCompression::Deflate)?.encode_to_vec();
    let MessageContent::Reply {
        body: crate::MessageBody::Custom { encoded, raw_bytes },
        ..
    } = MessageContent::decode(outer)?
    else {
        panic!("custom reply body");
    };
    assert_eq!(raw_bytes, raw);
    assert_eq!(encoded.content, b"custom payload");
}

// verifies: CTYPE-024, CTYPE-025, CTYPE-029
#[xmtp_common::test(unwrap_try = true)]
fn envelope_conversion_does_not_fabricate_codec_input() {
    for (_, raw, code, has_encoded) in failed_content_cases() {
        let Some(envelope) = ProtoEncodedContent::decode(raw.as_slice()).ok() else {
            continue;
        };
        let result = crate::EncodedContent::try_from(envelope);
        assert_eq!(result.is_ok(), has_encoded);
        if let Err(error) = result {
            let details = error.content_details();
            assert_eq!(details.code, code);
            assert!(matches!(details.category, crate::ErrorCategory::Input));
            assert!(!details.retryable);
        }
    }
    let mut custom = crate::encode_text("custom".into())?;
    custom.r#type.authority_id = "custom.example".into();
    assert!(
        matches!(crate::decode_standard(custom), Err(XmtpError::CodecNotFound(details)) if details.code == "CodecNotFound" && !details.retryable)
    );
    let mut invalid = crate::encode_text("invalid UTF-8".into())?;
    invalid.content = vec![0xff];
    assert!(
        matches!(crate::decode_standard(invalid), Err(XmtpError::CodecDecodeFailed(details)) if details.code == "CodecDecodeFailed" && !details.retryable)
    );
}

// verifies: CTYPE-008, CTYPE-014, CTYPE-029
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_attachment_schemes_keep_received_bytes_and_history() {
    use xmtp_content_types::{
        multi_remote_attachment::MultiRemoteAttachmentCodec,
        remote_attachment::{RemoteAttachment, RemoteAttachmentCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::content_types::MultiRemoteAttachment;
    let sender = Client::create(crate::generate_local_signer().await, options()).await?;
    let receiver = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = sender
        .conversations()
        .create_group(vec![receiver.inbox_id()], None)
        .await?;
    let record = |url: &str, scheme: &str| RemoteAttachment {
        url: url.into(),
        scheme: scheme.into(),
        content_digest: "ab".repeat(32),
        secret: vec![1; 32],
        salt: vec![2; 32],
        nonce: vec![3; 12],
        filename: Some("legacy.txt".into()),
        content_length: Some(0),
    };
    let mut cases = Vec::new();
    for (url, bare, canonical) in [
        ("https://example.com/file", "https", "https://"),
        ("http://localhost/file", "http", "http://"),
    ] {
        let mut wire = RemoteAttachmentCodec::encode(record(url, canonical))?;
        wire.parameters.insert("scheme".into(), bare.into());
        cases.push((wire, Some(canonical), false));
    }
    let multi = MultiRemoteAttachment {
        attachments: vec![
            record("https://example.com/file", "https"),
            record("http://127.0.0.1/file", "http"),
        ],
    };
    cases.push((
        ProtoEncodedContent {
            r#type: Some(MultiRemoteAttachmentCodec::content_type()),
            content: multi.encode_to_vec(),
            fallback: Some("legacy multiple".into()),
            ..Default::default()
        },
        None,
        false,
    ));
    let mut invalid = cases[1].0.clone();
    invalid.content = b"http://example.com/file".to_vec();
    cases.push((invalid, None, true));
    receiver.conversations().sync_all(None).await?;
    let peer = receiver
        .conversations()
        .get_by_id(group.id())
        .await?
        .expect("legacy receiver group");
    let crate::Conversation::Group { group: peer } = peer else {
        panic!("expected legacy group")
    };
    let reader = peer.message_reader(None).await?;
    for (wire, scheme, invalid) in cases {
        let encoded: crate::EncodedContent = wire.clone().try_into()?;
        let id = group.send(encoded.clone(), None).await?;
        receiver.conversations().sync_all(None).await?;
        let sent = sender
            .conversations()
            .get_message_by_id(id.clone())
            .await?
            .expect("legacy sent message");
        let lookup = receiver
            .conversations()
            .get_message_by_id(id.clone())
            .await?
            .expect("legacy lookup");
        let history = peer
            .messages(None)
            .await?
            .into_iter()
            .find(|message| message.0.id == id)
            .expect("legacy history");
        let streamed = xmtp_common::time::timeout(Duration::from_secs(10), async {
            loop {
                let message = reader.next().await?.expect("legacy stream item");
                if message.0.id == id {
                    break Ok::<_, XmtpError>(message);
                }
            }
        })
        .await??;
        for message in [lookup, history, streamed] {
            assert_eq!(message.0.id, id);
            assert_eq!(message.0.raw_bytes, sent.0.raw_bytes);
            assert_eq!(message.0.fallback, encoded.fallback);
            let retained = message
                .0
                .encoded
                .as_ref()
                .expect("retained legacy envelope");
            assert_eq!(retained.parameters, encoded.parameters);
            assert_eq!(retained.content, encoded.content);
            assert_eq!(retained.r#type.authority_id, encoded.r#type.authority_id);
            assert_eq!(retained.r#type.type_id, encoded.r#type.type_id);
            assert_eq!(retained.r#type.version_major, encoded.r#type.version_major);
            assert_eq!(retained.r#type.version_minor, encoded.r#type.version_minor);
            if invalid {
                assert_retained(&message, &sent.0.raw_bytes, "CodecDecodeFailed", true);
                continue;
            }
            let canonical = match message.0.content {
                MessageContent::RemoteAttachment(value) => {
                    assert_eq!(value.scheme, scheme.expect("single legacy scheme"));
                    crate::encode_standard(crate::StandardContent::RemoteAttachment(value))?
                }
                MessageContent::MultiRemoteAttachment(value) => {
                    assert_eq!(value.attachments[0].scheme, "https://");
                    assert_eq!(value.attachments[1].scheme, "http://");
                    crate::encode_standard(crate::StandardContent::MultiRemoteAttachment(value))?
                }
                other => panic!("legacy content did not decode: {other:?}"),
            };
            if scheme.is_some() {
                assert_ne!(canonical.parameters["scheme"], encoded.parameters["scheme"]);
            } else {
                assert_ne!(canonical.content, encoded.content);
            }
        }
    }
    let good = group
        .send_text("after legacy attachments".into(), None)
        .await?;
    receiver.conversations().sync_all(None).await?;
    let next = xmtp_common::time::timeout(Duration::from_secs(10), async {
        loop {
            let message = reader.next().await?.expect("continued stream");
            if message.0.id == good {
                break Ok::<_, XmtpError>(message);
            }
        }
    })
    .await??;
    assert!(
        matches!(next.0.content, MessageContent::Text(value) if value == "after legacy attachments")
    );
    reader.end().await?;
    receiver.end().await?;
    sender.end().await?;
}
