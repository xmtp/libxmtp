use super::*;
use crate::test_support::nested_reply;
use xmtp_content_types::{
    actions::{Action, Actions},
    attachment::Attachment,
    compression::compress,
    intent::Intent,
    reaction::LegacyReaction,
    reply::{Reply as EncodedReply, ReplyCodec},
};
use xmtp_proto::xmtp::mls::message_contents::Compression;

// verifies: CTYPE-024
#[xmtp_common::test(unwrap_try = true)]
async fn nested_content_decompressed_before_decode() {
    let inner = compress(TextCodec::encode("nested text".into())?, Compression::Gzip)?;
    let outer = ReplyCodec::encode(EncodedReply {
        reference: "0102".into(),
        reference_inbox_id: None,
        content: inner,
    })?;
    let outer = compress(outer, Compression::Deflate)?;
    let MessageBody::Reply(reply) = MessageBody::try_from(outer)? else {
        panic!("expected reply");
    };
    let MessageBody::Text(text) = *reply.content else {
        panic!("expected nested text");
    };
    assert_eq!(text.content, "nested text");
}

fn stored_message(content: EncodedContent) -> StoredGroupMessage {
    StoredGroupMessage {
        id: vec![1, 2, 3],
        group_id: GroupId::ONE,
        decrypted_message_bytes: content.encode_to_vec(),
        sent_at_ns: 1,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![4],
        sender_inbox_id: "inbox".into(),
        delivery_status: DeliveryStatus::Published,
        content_type: xmtp_db::group_message::ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".into(),
        reference_id: None,
        sequence_id: 1,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 0,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: None,
        should_push: false,
        idempotency_key: String::new(),
    }
}

fn unknown_content() -> EncodedContent {
    let mut content = TextCodec::encode("custom payload".into()).unwrap();
    content.r#type.as_mut().unwrap().type_id = "custom".into();
    content
}

// verifies: GMOD-053, CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn application_transcripts_are_undecodable() {
    for type_id in ["group_updated", "group_membership_change"] {
        for version in [1, 7] {
            for compression in [None, Some(Compression::Gzip as i32)] {
                let mut transcript = GroupUpdatedCodec::encode(GroupUpdated::default())?;
                transcript.r#type.as_mut().unwrap().type_id = type_id.into();
                transcript.r#type.as_mut().unwrap().version_major = version;
                // Neither prost nor gzip can decode this payload. The provenance
                // failure must happen before either one examines it.
                transcript.content = vec![0xff];
                transcript.compression = compression;
                transcript.fallback = Some("untrusted transcript".into());
                let reply = ReplyCodec::encode(EncodedReply {
                    reference: "0102".into(),
                    reference_inbox_id: None,
                    content: transcript.clone(),
                })?;
                let gzip_reply = compress(reply.clone(), Compression::Gzip)?;
                let two_levels = ReplyCodec::encode(EncodedReply {
                    reference: "0304".into(),
                    reference_inbox_id: None,
                    content: gzip_reply.clone(),
                })?;
                for (content, prefix) in [
                    (transcript, ""),
                    (gzip_reply, "nested reply content: "),
                    (two_levels, "nested reply content: nested reply content: "),
                ] {
                    let stored = stored_message(content.clone());
                    let raw = stored.decrypted_message_bytes.clone();
                    let id = stored.id.clone();
                    let decoded = DecodedMessage::from(stored);
                    assert_eq!(decoded.metadata.id, id);
                    assert_eq!(decoded.metadata.content_type, content.r#type);
                    assert_eq!(decoded.fallback_text, content.fallback);
                    let MessageBody::Undecodable(value) = decoded.content else {
                        panic!("application transcript was decoded");
                    };
                    assert_eq!(value.raw_bytes, raw);
                    assert_eq!(value.encoded, Some(content.clone()));
                    assert_eq!(
                        value.failure.kind,
                        ContentDecodeFailureKind::CodecDecodeFailed
                    );
                    assert_eq!(
                        value.failure.message,
                        format!("{prefix}transcript type in application content")
                    );
                    assert!(matches!(
                        MessageBody::try_from(content),
                        Err(CodecError::Decode(_))
                    ));
                }
            }
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn stored_transcripts_still_decode() {
    let transcript = GroupUpdatedCodec::encode(GroupUpdated {
        initiated_by_inbox_id: "committer".into(),
        ..Default::default()
    })?;
    for content in [
        transcript.clone(),
        compress(transcript.clone(), Compression::Gzip)?,
    ] {
        let mut stored = stored_message(content);
        stored.kind = GroupMessageKind::MembershipChange;
        let MessageBody::GroupUpdated(update) = DecodedMessage::from(stored).content else {
            panic!("stored transcript did not decode");
        };
        assert_eq!(update.initiated_by_inbox_id, "committer");
    }
    let mut legacy = transcript.clone();
    legacy.r#type.as_mut().unwrap().type_id = "group_membership_change".into();
    let mut stored = stored_message(legacy.clone());
    stored.kind = GroupMessageKind::MembershipChange;
    assert!(
        matches!(DecodedMessage::from(stored).content, MessageBody::Custom(value) if value.encoded == legacy)
    );

    // A stored transcript kind never grants transcript provenance to a reply body.
    let mut stored = stored_message(ReplyCodec::encode(EncodedReply {
        reference: "0102".into(),
        reference_inbox_id: None,
        content: transcript,
    })?);
    stored.kind = GroupMessageKind::MembershipChange;
    assert!(
        matches!(DecodedMessage::from(stored).content, MessageBody::Undecodable(value)
        if value.failure.message == "nested reply content: transcript type in application content")
    );
}

// verifies: CTYPE-001, CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn standard_codecs_match_authority_for_all_types() {
    macro_rules! check {
        ($content:expr, $standard:pat) => {
            check!($content, $standard, GroupMessageKind::Application)
        };
        ($content:expr, $standard:pat, $kind:expr) => {{
            let mut standard = $content;
            let id = standard.r#type.as_mut().expect("encoded type");
            id.version_minor = 9;
            let name = format!("{}/{}", id.authority_id, id.type_id);
            let mut stored = stored_message(standard.clone());
            stored.kind = $kind;
            let decoded = DecodedMessage::from(stored);
            assert!(matches!(decoded.content, $standard), "{name} did not decode");

            if standard.r#type.as_ref().expect("encoded type").authority_id != "xmtp.org" {
                let mut wrong_standard = standard.clone();
                wrong_standard.r#type.as_mut().expect("encoded type").authority_id =
                    "xmtp.org".into();
                let decoded = DecodedMessage::from(stored_message(wrong_standard.clone()));
                assert!(
                    matches!(decoded.content, MessageBody::Custom(actual) if actual.encoded == wrong_standard),
                    "{name} selected the wrong standard authority"
                );
            }

            let mut custom = standard;
            custom.r#type.as_mut().expect("encoded type").authority_id =
                "custom.example".into();
            let decoded = DecodedMessage::from(stored_message(custom.clone()));
            assert!(
                matches!(decoded.content, MessageBody::Custom(actual) if actual.encoded == custom),
                "{name} selected a codec for custom.example"
            );
        }};
    }

    check!(TextCodec::encode("text".into())?, MessageBody::Text(_));
    check!(
        MarkdownCodec::encode("markdown".into())?,
        MessageBody::Markdown(_)
    );
    check!(
        AttachmentCodec::encode(Attachment {
            filename: None,
            mime_type: "text/plain".into(),
            content: vec![1],
        })?,
        MessageBody::Attachment(_)
    );
    check!(
        RemoteAttachmentCodec::encode(RemoteAttachment {
            url: "https://example.com/file".into(),
            content_digest: "ab".repeat(32),
            secret: vec![1; 32],
            salt: vec![2; 32],
            nonce: vec![3; 12],
            scheme: "https://".into(),
            filename: None,
            content_length: None,
        })?,
        MessageBody::RemoteAttachment(_)
    );
    check!(
        ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: TextCodec::encode("reply".into())?,
        })?,
        MessageBody::Reply(_)
    );
    check!(
        ReactionCodec::encode(ReactionV2::default())?,
        MessageBody::Reaction(_)
    );
    check!(
        LegacyReactionCodec::encode(LegacyReaction {
            action: "added".into(),
            reference: "0102".into(),
            reference_inbox_id: None,
            schema: "unicode".into(),
            content: "👍".into(),
        })?,
        MessageBody::Reaction(_)
    );
    check!(
        MultiRemoteAttachmentCodec::encode(MultiRemoteAttachment::default())?,
        MessageBody::MultiRemoteAttachment(_)
    );
    check!(
        TransactionReferenceCodec::encode(TransactionReference {
            namespace: None,
            network_id: "1".into(),
            reference: "0x123".into(),
            metadata: None,
        })?,
        MessageBody::TransactionReference(_)
    );
    check!(
        GroupUpdatedCodec::encode(GroupUpdated::default())?,
        MessageBody::GroupUpdated(_),
        GroupMessageKind::MembershipChange
    );
    check!(
        ReadReceiptCodec::encode(ReadReceipt {})?,
        MessageBody::ReadReceipt(_)
    );
    check!(
        WalletSendCallsCodec::encode(WalletSendCalls {
            version: "1".into(),
            chain_id: "1".into(),
            from: "0x123".into(),
            calls: vec![],
            capabilities: None,
        })?,
        MessageBody::WalletSendCalls(_)
    );
    check!(
        IntentCodec::encode(Intent {
            id: "one".into(),
            action_id: "two".into(),
            metadata: None,
        })?,
        MessageBody::Intent(_)
    );
    check!(
        ActionsCodec::encode(Actions {
            id: "one".into(),
            description: "actions".into(),
            actions: vec![Action {
                id: "two".into(),
                label: "two".into(),
                image_url: None,
                style: None,
                expires_at: None,
            }],
            expires_at: None,
        })?,
        MessageBody::Actions(_)
    );
    check!(
        LeaveRequestCodec::encode(LeaveRequest::default())?,
        MessageBody::LeaveRequest(_)
    );
}

// verifies: CTYPE-024
#[xmtp_common::test(unwrap_try = true)]
async fn compressed_custom_content_is_decompressed_at_both_levels() {
    let inner = compress(unknown_content(), Compression::Gzip)?;
    let MessageBody::Custom(top) = DecodedMessage::from(stored_message(inner.clone())).content
    else {
        panic!("expected top-level custom content");
    };
    assert_eq!(top.encoded.compression, None);
    assert_eq!(top.encoded.content, b"custom payload");
    assert_eq!(top.raw_bytes, inner.encode_to_vec());

    let outer = ReplyCodec::encode(EncodedReply {
        reference: "0102".into(),
        reference_inbox_id: None,
        content: inner,
    })?;
    let MessageBody::Reply(reply) = MessageBody::try_from(outer.clone())? else {
        panic!("expected reply");
    };
    let MessageBody::Custom(nested) = *reply.content else {
        panic!("expected nested custom content");
    };
    assert_eq!(nested.encoded.compression, None);
    assert_eq!(nested.encoded.content, b"custom payload");
    assert_eq!(nested.raw_bytes, outer.content);
}

// verifies: CTYPE-008, CTYPE-025
#[xmtp_common::test(unwrap_try = true)]
async fn nested_compressed_reply_uses_one_budget() {
    let attack = nested_reply(
        TextCodec::encode("inner".into())?,
        20,
        compression::MAX_DECOMPRESSED_BYTES - 4096,
    );
    let bytes = attack.encode_to_vec();
    let mut budget = compression::DecompressionBudget::new();
    let failure =
        MessageBody::decode_bytes(&bytes, &mut budget, 0, Origin::Application).unwrap_err();
    assert_eq!(failure.kind, ContentDecodeFailureKind::CodecDecodeFailed);
    assert!(failure.message.contains("decompressed content exceeds"));
    assert!(budget.used() > 0);
    assert!(budget.peak_capacity() <= compression::MAX_DECOMPRESSED_BYTES);

    let expected = EncodedContent::decode(bytes.as_slice())?;
    // Decode from the exact bytes: re-encoding a parameter map can
    // reorder its entries.
    let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
    let MessageBody::Undecodable(original) = decoded.content else {
        panic!("expected preserved decode failure");
    };
    assert_eq!(original.raw_bytes, bytes);
    assert_eq!(original.encoded, Some(expected));
    assert_eq!(
        original.failure.kind,
        ContentDecodeFailureKind::CodecDecodeFailed
    );
}

// verifies: CTYPE-008, CTYPE-033
#[xmtp_common::test(unwrap_try = true)]
async fn nested_reply_field_count_is_limited() {
    let mut inner = TextCodec::encode("inner".into())?;
    let fields = xmtp_content_types::decode_limits::MAX_PROTOBUF_FIELDS + 1;
    inner
        .parameters
        .extend((0..fields).map(|i| (i.to_string(), String::new())));
    let outer = ReplyCodec::encode(EncodedReply {
        reference: "0102".into(),
        reference_inbox_id: None,
        content: inner,
    })?;
    let body = MessageBody::from_received_bytes(&outer.encode_to_vec());
    let MessageBody::Undecodable(undecodable) = body else {
        panic!("expected undecodable content");
    };
    assert!(undecodable.failure.message.contains("protobuf fields"));
}

// verifies: CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn reply_depth_limit_preserves_message() {
    let content = nested_reply(
        TextCodec::encode("inner".into())?,
        MAX_REPLY_NESTING_DEPTH + 1,
        0,
    );
    let bytes = content.encode_to_vec();
    let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
    let MessageBody::Undecodable(original) = decoded.content else {
        panic!("expected preserved decode failure");
    };
    assert_eq!(original.raw_bytes, bytes);
    assert_eq!(original.encoded, Some(content));
    assert!(original.failure.message.contains("reply nesting exceeds"));
}

// verifies: CTYPE-008, CTYPE-024
#[xmtp_common::test(unwrap_try = true)]
async fn compressed_decode_failure_preserves_message() {
    let mut content = TextCodec::encode("unchanged".into())?;
    content.compression = Some(99);
    let bytes = content.encode_to_vec();
    let message = stored_message(content.clone());
    let decoded = DecodedMessage::from(message);
    assert_eq!(decoded.metadata.id, vec![1, 2, 3]);
    assert_eq!(decoded.metadata.content_type, content.r#type);
    let MessageBody::Undecodable(undecodable) = decoded.content else {
        panic!("expected original undecodable content");
    };
    assert_eq!(undecodable.raw_bytes, bytes);
    assert_eq!(undecodable.encoded, Some(content));
    assert_eq!(
        undecodable.failure.kind,
        ContentDecodeFailureKind::CodecDecodeFailed
    );
    assert!(undecodable.failure.message.contains("unknown compression"));
}

fn stored_bytes(bytes: Vec<u8>) -> StoredGroupMessage {
    let mut message = stored_message(EncodedContent::default());
    message.decrypted_message_bytes = bytes;
    message
}

/// Append an unknown field (number 200, length-delimited) so the exact
/// serialization differs from the parsed value's re-encoding.
fn with_unknown_field(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.extend_from_slice(&[0xc2, 0x0c, 0x03, b'x', b'y', b'z']);
    bytes
}

// verifies: CTYPE-008, CTYPE-009
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_bytes_are_kept_with_their_cause() {
    let decoded = DecodedMessage::from(stored_bytes(vec![0xff]));
    assert_eq!(decoded.metadata.id, vec![1, 2, 3]);
    assert_eq!(decoded.metadata.content_type, None);
    assert_eq!(decoded.fallback_text, None);
    let MessageBody::Undecodable(undecodable) = decoded.content else {
        panic!("expected undecodable content, got {:?}", decoded.content);
    };
    assert_eq!(undecodable.raw_bytes, vec![0xff]);
    assert_eq!(undecodable.encoded, None);
    assert_eq!(
        undecodable.failure.kind,
        ContentDecodeFailureKind::MalformedEnvelope
    );
}

// verifies: CTYPE-008, CTYPE-009
#[xmtp_common::test(unwrap_try = true)]
async fn absent_or_partial_type_is_malformed_and_keeps_the_partial_identifier() {
    let mut untyped = TextCodec::encode("text".into())?;
    untyped.r#type = None;
    untyped.fallback = Some("kept".into());
    let mut partial = untyped.clone();
    partial.r#type = Some(ContentTypeId {
        authority_id: String::new(),
        type_id: "text".into(),
        version_major: 1,
        version_minor: 0,
    });
    // Invalid compression does not change the cause of an untyped envelope.
    let mut untyped_compressed = untyped.clone();
    untyped_compressed.compression = Some(99);
    let mut partial_compressed = partial.clone();
    partial_compressed.compression = Some(99);
    for content in [untyped, partial, untyped_compressed, partial_compressed] {
        let bytes = content.encode_to_vec();
        let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
        assert_eq!(decoded.metadata.content_type, content.r#type);
        assert_eq!(decoded.fallback_text.as_deref(), Some("kept"));
        let MessageBody::Undecodable(undecodable) = decoded.content else {
            panic!("expected undecodable content, got {:?}", decoded.content);
        };
        assert_eq!(undecodable.raw_bytes, bytes);
        assert_eq!(undecodable.encoded, Some(content));
        assert_eq!(
            undecodable.failure.kind,
            ContentDecodeFailureKind::MalformedEnvelope
        );
    }
}

// verifies: CTYPE-008, CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
async fn exact_bytes_survive_at_both_levels() {
    let nested_bytes = with_unknown_field(unknown_content().encode_to_vec());
    let mut outer = ReplyCodec::encode(EncodedReply {
        reference: "0102".into(),
        reference_inbox_id: None,
        content: unknown_content(),
    })?;
    outer.content = nested_bytes.clone();
    let outer_bytes = with_unknown_field(outer.encode_to_vec());

    let MessageBody::Custom(top) = DecodedMessage::from(stored_bytes(nested_bytes.clone())).content
    else {
        panic!("expected custom content");
    };
    assert_eq!(top.raw_bytes, nested_bytes);
    assert_ne!(top.encoded.encode_to_vec(), nested_bytes);

    let MessageBody::Reply(reply) = DecodedMessage::from(stored_bytes(outer_bytes)).content else {
        panic!("expected reply");
    };
    assert_eq!(reply.reference_id, "0102");
    let MessageBody::Custom(nested) = *reply.content else {
        panic!("expected nested custom content");
    };
    assert_eq!(nested.raw_bytes, nested_bytes);
    assert_eq!(nested.encoded.content, b"custom payload");
}

// verifies: CTYPE-008, CTYPE-029
#[xmtp_common::test(unwrap_try = true)]
async fn nested_failures_make_the_outer_reply_undecodable() {
    let reply_with = |nested: Vec<u8>| {
        let mut outer = ReplyCodec::encode(EncodedReply {
            reference: "0102".into(),
            reference_inbox_id: None,
            content: TextCodec::encode("placeholder".into()).unwrap(),
        })
        .unwrap();
        outer.content = nested;
        outer.fallback = Some("outer fallback".into());
        outer
    };
    let mut untyped = TextCodec::encode("nested".into())?;
    untyped.r#type = None;
    let mut bad_compression = TextCodec::encode("nested".into())?;
    bad_compression.compression = Some(99);
    let mut bad_text = TextCodec::encode("nested".into())?;
    bad_text.content = vec![0xff, 0xfe];
    let cases = [
        ("malformed", vec![0xff]),
        ("untyped", untyped.encode_to_vec()),
        ("bad compression", bad_compression.encode_to_vec()),
        ("failed standard", bad_text.encode_to_vec()),
    ];
    for (name, nested) in cases {
        let outer = reply_with(nested);
        let bytes = outer.encode_to_vec();
        let decoded = DecodedMessage::from(stored_bytes(bytes.clone()));
        assert_eq!(
            decoded.fallback_text.as_deref(),
            Some("outer fallback"),
            "{name}"
        );
        assert_eq!(decoded.metadata.content_type, outer.r#type, "{name}");
        let MessageBody::Undecodable(undecodable) = decoded.content else {
            panic!(
                "{name}: expected undecodable outer reply, got {:?}",
                decoded.content
            );
        };
        assert_eq!(undecodable.raw_bytes, bytes, "{name}");
        assert_eq!(undecodable.encoded, Some(outer), "{name}");
        assert_eq!(
            undecodable.failure.kind,
            ContentDecodeFailureKind::CodecDecodeFailed,
            "{name}"
        );
        assert!(undecodable.failure.message.contains("nested"), "{name}");
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn invalid_reply_reference_is_a_reply_with_no_parent() {
    // A sender-controlled reference that is not a message id resolves to
    // no parent; the reply itself is valid content and is never marked
    // undecodable. The reference is kept as received.
    for reference in ["not-valid-hex!@#", ""] {
        let outer = ReplyCodec::encode(EncodedReply {
            reference: reference.into(),
            reference_inbox_id: None,
            content: TextCodec::encode("reply".into())?,
        })?;
        let decoded = DecodedMessage::from(stored_bytes(outer.encode_to_vec()));
        let MessageBody::Reply(reply) = decoded.content else {
            panic!("{reference:?}: expected a reply, got {:?}", decoded.content);
        };
        assert!(reply.in_reply_to.is_none());
        assert_eq!(reply.reference_id, reference);
        assert!(matches!(*reply.content, MessageBody::Text(ref text) if text.content == "reply"));
    }
}

// verifies: CTYPE-028
#[xmtp_common::test(unwrap_try = true)]
async fn nested_type_wins_over_the_outer_content_type_parameter() {
    let mut outer = ReplyCodec::encode(EncodedReply {
        reference: "0102".into(),
        reference_inbox_id: None,
        content: TextCodec::encode("placeholder".into())?,
    })?;
    assert_eq!(outer.parameters["contentType"], "xmtp.org/text:1.0");
    outer.content = ReactionCodec::encode(ReactionV2 {
        reference: "0102".into(),
        content: "👍".into(),
        ..Default::default()
    })?
    .encode_to_vec();
    let MessageBody::Reply(reply) = MessageBody::try_from(outer)? else {
        panic!("expected reply");
    };
    assert!(
        matches!(*reply.content, MessageBody::Reaction(ref reaction) if reaction.content == "👍"),
        "the outer contentType parameter selected the codec: {:?}",
        reply.content
    );
}

// verifies: CTYPE-009
#[xmtp_common::test(unwrap_try = true)]
async fn strict_decode_reports_failures_as_errors() {
    let mut untyped = TextCodec::encode("text".into())?;
    untyped.r#type = None;
    assert!(matches!(
        MessageBody::try_from(untyped),
        Err(CodecError::InvalidContentType)
    ));
    let mut bad_text = TextCodec::encode("text".into())?;
    bad_text.content = vec![0xff, 0xfe];
    assert!(matches!(
        MessageBody::try_from(bad_text),
        Err(CodecError::Decode(_))
    ));
}
