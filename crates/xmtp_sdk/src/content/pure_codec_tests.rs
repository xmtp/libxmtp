use super::*;
use xmtp_content_types::ContentCodec;
use xmtp_proto::xmtp::mls::message_contents::content_types as proto;

#[cfg(test)]
#[xmtp_common::test(unwrap_try = true)]
fn encoded_content_requires_complete_type_and_preserves_custom_bytes() {
    let sample = |authority: &str, name: &str| EncodedContent {
        r#type: ContentTypeId {
            authority_id: authority.into(),
            type_id: name.into(),
            version_major: u32::MAX,
            version_minor: 7,
        },
        parameters: HashMap::from([("encoding".into(), "opaque".into())]),
        fallback: Some("custom content".into()),
        content: vec![0, 0xff, 7, 0],
    };
    for (authority, name) in [("", "type"), ("authority", ""), ("", "")] {
        let error = encode_encoded_content(sample(authority, name)).unwrap_err();
        let crate::XmtpError::InvalidArgument(details) = error else {
            panic!("expected InvalidArgument, got {error:?}");
        };
        assert_eq!(details.code, "InvalidArgument");
        assert!(matches!(details.category, crate::ErrorCategory::Input));
        assert!(!details.retryable);
    }
    let content = sample("custom.example", "opaque");
    let decoded = decode_encoded_content(encode_encoded_content(content.clone())?)?;
    assert_eq!(decoded.r#type.authority_id, content.r#type.authority_id);
    assert_eq!(decoded.r#type.type_id, content.r#type.type_id);
    assert_eq!(decoded.r#type.version_major, content.r#type.version_major);
    assert_eq!(decoded.r#type.version_minor, content.r#type.version_minor);
    assert_eq!(decoded.parameters, content.parameters);
    assert_eq!(decoded.fallback, content.fallback);
    assert_eq!(decoded.content, content.content);
}

pub(crate) fn standard_codec_samples()
-> Result<Vec<(StandardContent, ProtoEncodedContent)>, Box<dyn std::error::Error>> {
    let text = xmtp_content_types::text::TextCodec::encode("hello".into())?;
    let remote = proto::RemoteAttachmentInfo {
        url: "https://example.test/file".into(),
        content_digest: "digest".into(),
        secret: vec![1; 32],
        salt: vec![2; 32],
        nonce: vec![3; 12],
        scheme: "https".into(),
        content_length: Some(10),
        filename: Some("file".into()),
    };
    let transaction = xmtp_content_types::transaction_reference::TransactionReference {
        namespace: None,
        network_id: "1".into(),
        reference: "0x1".into(),
        metadata: None,
    };
    let reaction = proto::ReactionV2 {
        reference: "a".repeat(64),
        reference_inbox_id: "inbox".into(),
        action: proto::ReactionAction::Added as i32,
        content: "👍".into(),
        schema: proto::ReactionSchema::Unicode as i32,
    };
    let reply = xmtp_content_types::reply::Reply {
        reference: "a".repeat(64),
        reference_inbox_id: Some("inbox".into()),
        content: text.clone(),
    };
    let update = xmtp_proto::xmtp::mls::message_contents::GroupUpdated {
        initiated_by_inbox_id: "inbox".into(),
        ..Default::default()
    };
    let wallet = WalletSendCalls {
        version: "1".into(),
        chain_id: "0x1".into(),
        from: "0xsender".into(),
        calls: vec![],
        capabilities: None,
    };
    let actions = Actions {
        id: "actions".into(),
        description: "Choose".into(),
        actions: vec![Action {
            id: "one".into(),
            label: "One".into(),
            image_url: None,
            style: None,
            expires_at: None,
        }],
        expires_at: None,
    };
    let intent = Intent {
        id: "actions".into(),
        action_id: "one".into(),
        metadata_json: None,
    };
    let cases = vec![
        (StandardContent::Text("hello".into()), text),
        (
            StandardContent::Markdown("**hello**".into()),
            xmtp_content_types::markdown::MarkdownCodec::encode("**hello**".into())?,
        ),
        (
            StandardContent::ReadReceipt,
            xmtp_content_types::read_receipt::ReadReceiptCodec::encode(
                xmtp_content_types::read_receipt::ReadReceipt {},
            )?,
        ),
        (
            StandardContent::Reaction {
                reference: crate::MessageId::try_from(reaction.reference.clone())?,
                reference_inbox_id: Some(crate::InboxId::try_from(
                    reaction.reference_inbox_id.clone(),
                )?),
                reaction: Reaction::from_proto(reaction.clone()),
            },
            xmtp_content_types::reaction::ReactionCodec::encode(reaction)?,
        ),
        (
            StandardContent::Attachment(Attachment {
                filename: None,
                mime_type: "text/plain".into(),
                content: b"file".to_vec(),
            }),
            xmtp_content_types::attachment::AttachmentCodec::encode(
                xmtp_content_types::attachment::Attachment {
                    filename: None,
                    mime_type: "text/plain".into(),
                    content: b"file".to_vec(),
                },
            )?,
        ),
        (
            StandardContent::RemoteAttachment(remote.clone().into()),
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::encode(remote.clone())?,
        ),
        (
            StandardContent::MultiRemoteAttachment(MultiRemoteAttachment {
                attachments: vec![remote.clone().into()],
            }),
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::encode(
                proto::MultiRemoteAttachment {
                    attachments: vec![remote],
                },
            )?,
        ),
        (
            StandardContent::TransactionReference(transaction.clone().into()),
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::encode(
                transaction,
            )?,
        ),
        (
            StandardContent::WalletSendCalls(wallet.clone()),
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::encode(wallet.into())?,
        ),
        (
            StandardContent::Actions(actions.clone()),
            xmtp_content_types::actions::ActionsCodec::encode(actions.into())?,
        ),
        (
            StandardContent::Intent(intent.clone()),
            xmtp_content_types::intent::IntentCodec::encode(intent.try_into()?)?,
        ),
        (
            StandardContent::Reply {
                reference: crate::MessageId::try_from(reply.reference.clone())?,
                reference_inbox_id: reply
                    .reference_inbox_id
                    .clone()
                    .map(crate::InboxId::try_from)
                    .transpose()?,
                content: reply.content.clone().try_into()?,
            },
            xmtp_content_types::reply::ReplyCodec::encode(reply)?,
        ),
        (
            StandardContent::GroupUpdated(update.clone().try_into()?),
            xmtp_content_types::group_updated::GroupUpdatedCodec::encode(update)?,
        ),
        (
            StandardContent::DeleteMessage {
                message_id: crate::MessageId::try_from("a".repeat(64))?,
            },
            xmtp_content_types::delete_message::DeleteMessageCodec::encode(proto::DeleteMessage {
                message_id: "a".repeat(64),
            })?,
        ),
        (
            StandardContent::LeaveRequest(LeaveRequest {
                authenticated_note: None,
            }),
            xmtp_content_types::leave_request::LeaveRequestCodec::encode(proto::LeaveRequest {
                authenticated_note: None,
            })?,
        ),
    ];
    Ok(cases)
}

#[cfg(test)]
// verifies: CTYPE-026
#[xmtp_common::test(unwrap_try = true)]
fn standard_codec_bytes_match_the_core_send_codecs() {
    let cases = standard_codec_samples()?;
    assert_eq!(cases.len(), 15);
    for (value, core) in cases {
        let facade = encode_standard(value)?;
        let expected: EncodedContent = core.try_into()?;
        assert_eq!(facade.r#type.type_id, expected.r#type.type_id);
        assert_eq!(facade.content, expected.content);
        assert_eq!(facade.parameters, expected.parameters);
        assert_eq!(facade.fallback, expected.fallback);
        let round_trip = encode_standard(decode_standard(facade)?)?;
        assert_eq!(round_trip.content, expected.content);
    }
}

#[cfg(test)]
// verifies: CTYPE-011
// verifies: CTYPE-024
#[xmtp_common::test(unwrap_try = true)]
fn malformed_nested_reply_content_is_rejected() {
    use prost::Message as _;

    let valid = xmtp_content_types::text::TextCodec::encode("valid".into())?;
    let reply = xmtp_content_types::reply::Reply {
        reference: "a".repeat(64),
        reference_inbox_id: None,
        content: valid,
    };
    assert!(matches!(
        xmtp_content_types::reply::ReplyCodec::encode(xmtp_content_types::reply::Reply {
            content: ProtoEncodedContent::default(),
            ..reply.clone()
        }),
        Err(xmtp_content_types::CodecError::InvalidContentType)
    ));
    for nested in [
        ProtoEncodedContent::default(),
        ProtoEncodedContent {
            r#type: Some(xmtp_content_types::text::TextCodec::content_type()),
            compression: Some(99),
            content: b"compressed text".to_vec(),
            ..Default::default()
        },
    ] {
        let mut outer = xmtp_content_types::reply::ReplyCodec::encode(reply.clone())?;
        outer.content = nested.encode_to_vec();
        assert!(decode_standard(outer.try_into()?).is_err());
    }
}

#[cfg(test)]
// verifies: CTYPE-029
#[xmtp_common::test(unwrap_try = true)]
fn malformed_nested_standard_reply_content_is_rejected() {
    let nested = ProtoEncodedContent {
        r#type: Some(xmtp_content_types::text::TextCodec::content_type()),
        content: vec![0xff, 0xfe],
        ..Default::default()
    };
    let outer = xmtp_content_types::reply::ReplyCodec::encode(xmtp_content_types::reply::Reply {
        reference: "a".repeat(64),
        reference_inbox_id: None,
        content: nested,
    })?;
    assert!(decode_standard(outer.try_into()?).is_err());
}

#[cfg(test)]
// verifies: CTYPE-027
#[xmtp_common::test(unwrap_try = true)]
fn nested_custom_reply_content_remains_available() {
    let nested = ProtoEncodedContent {
        r#type: Some(ProtoContentTypeId {
            authority_id: "example.com".into(),
            type_id: "widget".into(),
            version_major: 1,
            version_minor: 0,
        }),
        content: vec![0xff, 0xfe],
        ..Default::default()
    };
    let outer = xmtp_content_types::reply::ReplyCodec::encode(xmtp_content_types::reply::Reply {
        reference: "a".repeat(64),
        reference_inbox_id: None,
        content: nested,
    })?;
    assert!(matches!(
        decode_standard(outer.try_into()?)?,
        StandardContent::Reply { content, .. }
            if content.r#type.authority_id == "example.com"
                && content.r#type.type_id == "widget"
                && content.content == [0xff, 0xfe]
    ));
}

#[cfg(test)]
#[xmtp_common::test(unwrap_try = true)]
fn text_minor_version_decodes() {
    let mut encoded = encode_text("minor version".into())?;
    encoded.r#type.version_minor = 1;
    assert!(matches!(
        decode_standard(encoded)?,
        StandardContent::Text(value) if value == "minor version"
    ));
}

// verifies: CTYPE-026, SEND-021
#[cfg(test)]
#[xmtp_common::test(unwrap_try = true)]
fn standalone_standard_hooks_use_canonical_rules() {
    let leave = encode_standard(StandardContent::LeaveRequest(LeaveRequest {
        authenticated_note: None,
    }))
    .unwrap();
    assert_eq!(
        leave.fallback.as_deref(),
        Some("A member has requested leaving the group")
    );
    assert!(!catalogue_content_type_should_push(leave.r#type));
    for kind in [
        StandardContentKind::ReadReceipt,
        StandardContentKind::Reaction,
        StandardContentKind::GroupUpdated,
        StandardContentKind::DeleteMessage,
        StandardContentKind::LeaveRequest,
    ] {
        assert!(!catalogue_content_type_should_push(standard_content_type(
            kind
        )));
    }
    assert!(catalogue_content_type_should_push(ContentTypeId {
        authority_id: "example.org".into(),
        type_id: "note".into(),
        version_major: 1,
        version_minor: 0,
    }));
}

// Retained LeaveRequest init and decode normalize an empty note to None.
#[cfg(test)]
#[xmtp_common::test(unwrap_try = true)]
fn leave_request_empty_note_is_absent_on_the_wire_and_after_decode() {
    use prost::Message as _;
    let empty = encode_standard(StandardContent::LeaveRequest(LeaveRequest {
        authenticated_note: Some(vec![]),
    }))?;
    let absent = encode_standard(StandardContent::LeaveRequest(LeaveRequest {
        authenticated_note: None,
    }))?;
    assert_eq!(empty.content, absent.content);
    let mut raw: ProtoEncodedContent = absent.into();
    raw.content = proto::LeaveRequest {
        authenticated_note: Some(vec![]),
    }
    .encode_to_vec();
    let StandardContent::LeaveRequest(decoded) = decode_standard(raw.try_into()?)? else {
        panic!("wrong standard content variant");
    };
    assert_eq!(decoded.authenticated_note, None);
}

// verifies: CTYPE-015, CTYPE-026
#[cfg(test)]
#[xmtp_common::test(unwrap_try = true)]
fn remote_attachment_projection_uses_ciphertext_and_shared_url_policy() {
    let encrypted = || EncryptedEncodedContent {
        ciphertext: b"ciphertext".to_vec(),
        keys: EncryptionKeys {
            secret: vec![1; 32],
            salt: vec![2; 32],
            nonce: vec![3; 12],
            digest: "not the ciphertext digest".into(),
            length: 999,
        },
    };
    for url in [
        "https://example.org/file?signature=value",
        "http://localhost/file",
        "http://127.0.0.1/file",
        "http://[::1]/file",
    ] {
        let record =
            remote_attachment_from_encrypted(url.into(), encrypted(), Some("file".into()))?;
        assert_eq!(record.url, url);
        assert_eq!(record.content_length, Some(10));
        assert_eq!(
            record.content_digest,
            "305531dcc50ebca31cf1d5b31e9fc76ed51f66b3b6dd5a030c6539ae6532f979"
        );
        assert_eq!(record.secret, vec![1; 32]);
        assert_eq!(record.salt, vec![2; 32]);
        assert_eq!(record.nonce, vec![3; 12]);
        assert_eq!(record.filename.as_deref(), Some("file"));
        assert_eq!(
            record.scheme,
            if url.starts_with("https:") {
                "https://"
            } else {
                "http://"
            }
        );
    }
    for url in [
        "not a url",
        "ftp://example.org/file",
        "http://example.org/file",
    ] {
        assert!(matches!(
            remote_attachment_from_encrypted(url.into(), encrypted(), None),
            Err(crate::XmtpError::InvalidArgument(_))
        ));
    }
}

// verifies: CTYPE-015
#[cfg(test)]
#[xmtp_common::test(unwrap_try = true)]
fn remote_attachment_projection_rejects_invalid_key_lengths() {
    let mut wrong_results = Vec::new();
    for (field, expected) in [("secret", 32), ("salt", 32), ("nonce", 12)] {
        for length in [0, expected - 1, expected + 1] {
            let mut encrypted = EncryptedEncodedContent {
                ciphertext: b"ciphertext".to_vec(),
                keys: EncryptionKeys {
                    secret: vec![1; 32],
                    salt: vec![2; 32],
                    nonce: vec![3; 12],
                    digest: String::new(),
                    length: 10,
                },
            };
            let key = match field {
                "secret" => &mut encrypted.keys.secret,
                "salt" => &mut encrypted.keys.salt,
                "nonce" => &mut encrypted.keys.nonce,
                _ => unreachable!(),
            };
            *key = vec![7; length];
            if !matches!(
                remote_attachment_from_encrypted(
                    "https://example.org/file".into(),
                    encrypted,
                    None
                ),
                Err(crate::XmtpError::InvalidArgument(_))
            ) {
                wrong_results.push((field, length));
            }
        }
    }
    assert!(
        wrong_results.is_empty(),
        "wrong key validation results: {wrong_results:?}"
    );
}

// verifies: CTYPE-015
#[cfg(all(test, not(feature = "pure-only")))]
#[xmtp_common::test(unwrap_try = true)]
async fn remote_attachment_projection_preserves_decryptable_attachment() {
    use prost::Message as _;
    let attachment = Attachment {
        filename: Some("note.txt".into()),
        mime_type: "text/plain".into(),
        content: b"attachment payload".to_vec(),
    };
    let encoded: ProtoEncodedContent =
        encode_standard(StandardContent::Attachment(attachment.clone()))?.into();
    let plaintext = encoded.encode_to_vec();
    let encrypted = crate::crypto::encrypt_encoded_content(plaintext.clone()).await?;
    let record = remote_attachment_from_encrypted(
        "https://example.org/file".into(),
        encrypted.clone(),
        attachment.filename.clone(),
    )?;
    let decrypted = crate::crypto::decrypt_encoded_content(EncryptedEncodedContent {
        ciphertext: encrypted.ciphertext,
        keys: EncryptionKeys {
            secret: record.secret,
            salt: record.salt,
            nonce: record.nonce,
            digest: record.content_digest,
            length: u64::from(record.content_length?),
        },
    })
    .await?;
    let decrypted = ProtoEncodedContent::decode(decrypted.as_slice())?;
    assert_eq!(decrypted, encoded);
    let StandardContent::Attachment(decoded) = decode_standard(decrypted.try_into()?)? else {
        panic!("wrong standard content variant");
    };
    assert_eq!(decoded.filename, attachment.filename);
    assert_eq!(decoded.mime_type, attachment.mime_type);
    assert_eq!(decoded.content, attachment.content);
}
