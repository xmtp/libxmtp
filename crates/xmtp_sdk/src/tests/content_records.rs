use super::*;

/// Each standard codec decodes to its SDK record, and the record keeps the
/// codec's fields, including nested and optional ones.
#[xmtp_common::test(unwrap_try = true)]
fn standard_content_types_decode_to_records() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{Action, ActionStyle, Actions, ActionsCodec},
        attachment::{Attachment, AttachmentCodec},
        group_updated::GroupUpdatedCodec,
        intent::{Intent, IntentCodec},
        leave_request::LeaveRequestCodec,
        multi_remote_attachment::MultiRemoteAttachmentCodec,
        reaction::ReactionCodec,
        read_receipt::{ReadReceipt, ReadReceiptCodec},
        remote_attachment::{RemoteAttachment, RemoteAttachmentCodec},
        reply::{Reply, ReplyCodec},
        text::TextCodec,
        transaction_reference::{
            TransactionMetadata, TransactionReference, TransactionReferenceCodec,
        },
        wallet_send_calls::{WalletSendCalls, WalletSendCallsCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::{
        GroupUpdated,
        content_types::{self as proto, LeaveRequest},
        group_updated::{Inbox, MetadataFieldChange},
    };

    for text in [
        "",
        "Hello 👋 World 🌍! こんにちは 🎉",
        "Line 1\nLine 2\tTabbed\r\nWindows newline",
    ] {
        let encoded = crate::encode_text(text.into())?;
        let decoded = MessageContent::decode(
            xmtp_proto::xmtp::mls::message_contents::EncodedContent::from(encoded).encode_to_vec(),
        )?;
        assert!(matches!(decoded, MessageContent::Text(value) if value == text));
        assert!(
            matches!(MessageContent::decode(TextCodec::encode(text.into())?.encode_to_vec())?, MessageContent::Text(value) if value == text)
        );
    }
    assert!(MessageContent::decode(vec![0xff; 4]).is_err());

    let reference = MessageId::try_from("a".repeat(64))?;
    let reaction = crate::Reaction {
        content: "👍".into(),
        action: crate::ReactionAction::Added,
        schema: crate::ReactionSchema::Unicode,
    };
    let encoded_reaction = ReactionCodec::encode(
        reaction
            .clone()
            .into_proto(reference.checked()?.to_owned(), "inbox".to_owned()),
    )?;
    assert!(
        matches!(MessageContent::decode(encoded_reaction.encode_to_vec())?, MessageContent::Reaction { reaction: value, .. }
        if value.content == reaction.content
            && matches!(value.action, crate::ReactionAction::Added)
            && matches!(value.schema, crate::ReactionSchema::Unicode))
    );
    let proto_reaction = proto::ReactionV2::decode(encoded_reaction.content.as_slice())?;
    assert_eq!(proto_reaction.reference, reference.checked()?);
    assert_eq!(proto_reaction.reference_inbox_id, "inbox");

    let attachment = Attachment {
        filename: Some("a.txt".into()),
        mime_type: "text/plain".into(),
        content: b"attachment".to_vec(),
    };
    assert!(matches!(
        MessageContent::decode(AttachmentCodec::encode(attachment)?.encode_to_vec())?,
        MessageContent::Attachment(value)
            if value.filename.as_deref() == Some("a.txt")
                && value.mime_type == "text/plain"
                && value.content == b"attachment"
    ));

    let remote = RemoteAttachment {
        url: "https://example.org/a".into(),
        content_digest: "ab".repeat(32),
        secret: vec![1; 32],
        salt: vec![3; 32],
        nonce: vec![5; 12],
        scheme: "https://".into(),
        content_length: Some(12),
        filename: Some("a.txt".into()),
    };
    assert!(matches!(
        MessageContent::decode(RemoteAttachmentCodec::encode(remote.clone())?.encode_to_vec())?,
        MessageContent::RemoteAttachment(value)
            if value.url == remote.url
                && value.content_digest == remote.content_digest
                && value.secret == remote.secret
                && value.salt == remote.salt
                && value.nonce == remote.nonce
                && value.scheme == remote.scheme
                && value.content_length == remote.content_length
                && value.filename == remote.filename
    ));
    let multi = proto::MultiRemoteAttachment {
        attachments: vec![remote.clone(), remote],
    };
    assert!(matches!(
        MessageContent::decode(MultiRemoteAttachmentCodec::encode(multi)?.encode_to_vec())?,
        MessageContent::MultiRemoteAttachment(value)
            if value.attachments.len() == 2
                && value.attachments[0].content_digest == "ab".repeat(32)
                && value.attachments[1].url == "https://example.org/a"
    ));

    let transaction = TransactionReference {
        namespace: Some("eip155".into()),
        network_id: "1".into(),
        reference: "0xabc".into(),
        metadata: Some(TransactionMetadata {
            transaction_type: "transfer".into(),
            currency: "ETH".into(),
            amount: 0.42,
            decimals: 18,
            from_address: "0xfrom".into(),
            to_address: "0xto".into(),
        }),
    };
    assert!(matches!(
        MessageContent::decode(TransactionReferenceCodec::encode(transaction)?.encode_to_vec())?,
        MessageContent::TransactionReference(value)
            if value.network_id == "1"
                && value.reference == "0xabc"
                && value.metadata.as_ref().is_some_and(|metadata| metadata.currency == "ETH")
    ));
    let calls = WalletSendCalls {
        version: "1".into(),
        chain_id: "1".into(),
        from: "0x1".into(),
        calls: vec![],
        capabilities: None,
    };
    assert!(
        matches!(MessageContent::decode(WalletSendCallsCodec::encode(calls)?.encode_to_vec())?, MessageContent::WalletSendCalls(value) if value.chain_id == "1")
    );
    assert!(matches!(
        MessageContent::decode(ReadReceiptCodec::encode(ReadReceipt {})?.encode_to_vec())?,
        MessageContent::ReadReceipt
    ));
    let reply = Reply {
        reference: "b".repeat(64),
        reference_inbox_id: Some("inbox".into()),
        content: TextCodec::encode("answer".into())?,
    };
    assert!(matches!(
        MessageContent::decode(ReplyCodec::encode(reply)?.encode_to_vec())?,
        MessageContent::Reply { reference_id, body: crate::MessageBody::Text(value) }
            if reference_id.checked().ok() == Some(&*"b".repeat(64)) && value == "answer"
    ));

    let intent = Intent {
        id: "intent-id".into(),
        action_id: "action-id".into(),
        metadata: Some(serde_json::from_value(
            serde_json::json!({"nested": {"value": 7}}),
        )?),
    };
    assert!(matches!(
        MessageContent::decode(IntentCodec::encode(intent)?.encode_to_vec())?,
        MessageContent::Intent(value)
            if value.id == "intent-id"
                && value.action_id == "action-id"
                && value.metadata_json.as_deref().is_some_and(|json| json.contains("nested"))
    ));
    let actions = Actions {
        id: "actions-id".into(),
        description: "choose".into(),
        actions: vec![Action {
            id: "button".into(),
            label: "Confirm".into(),
            image_url: Some("https://example.org/icon".into()),
            style: Some(ActionStyle::Primary),
            expires_at: None,
        }],
        expires_at: None,
    };
    assert!(matches!(
        MessageContent::decode(ActionsCodec::encode(actions)?.encode_to_vec())?,
        MessageContent::Actions(value)
            if value.id == "actions-id"
                && value.description == "choose"
                && value.actions.len() == 1
                && value.actions[0].id == "button"
                && value.actions[0].label == "Confirm"
                && matches!(value.actions[0].style, Some(crate::ActionStyle::Primary))
    ));

    let inbox = |inbox_id: &str| Inbox {
        inbox_id: inbox_id.into(),
    };
    let update = GroupUpdated {
        initiated_by_inbox_id: "inbox".into(),
        added_inboxes: vec![inbox("added")],
        removed_inboxes: vec![inbox("removed")],
        left_inboxes: vec![inbox("left")],
        metadata_field_changes: vec![MetadataFieldChange {
            field_name: "name".into(),
            old_value: Some("old".into()),
            new_value: Some("new".into()),
        }],
        added_admin_inboxes: vec![inbox("added-admin")],
        removed_admin_inboxes: vec![inbox("removed-admin")],
        added_super_admin_inboxes: vec![inbox("added-super")],
        removed_super_admin_inboxes: vec![inbox("removed-super")],
    };
    let crate::StandardContent::GroupUpdated(update) =
        crate::decode_standard(GroupUpdatedCodec::encode(update)?.try_into()?)?
    else {
        panic!("group update")
    };
    assert_eq!(update.initiated_by_inbox_id.checked()?, "inbox");
    assert_eq!(update.added_inboxes[0].checked()?, "added");
    assert_eq!(update.removed_inboxes[0].checked()?, "removed");
    assert_eq!(update.left_inboxes[0].checked()?, "left");
    assert_eq!(update.metadata_field_changes[0].field_name, "name");
    assert_eq!(
        update.metadata_field_changes[0].old_value.as_deref(),
        Some("old")
    );
    assert_eq!(
        update.metadata_field_changes[0].new_value.as_deref(),
        Some("new")
    );
    assert_eq!(update.added_admin_inboxes[0].checked()?, "added-admin");
    assert_eq!(update.removed_admin_inboxes[0].checked()?, "removed-admin");
    assert_eq!(
        update.added_super_admin_inboxes[0].checked()?,
        "added-super"
    );
    assert_eq!(
        update.removed_super_admin_inboxes[0].checked()?,
        "removed-super"
    );
    for note in [None, Some(b"leaving".to_vec())] {
        let encoded = LeaveRequestCodec::encode(LeaveRequest {
            authenticated_note: note.clone(),
        })?;
        assert!(matches!(
            MessageContent::decode(encoded.encode_to_vec())?,
            MessageContent::LeaveRequest(value) if value.authenticated_note == note
        ));
    }
}

// verifies: ARCH-019
#[xmtp_common::test(unwrap_try = true)]
fn message_data_reports_unknown_expiry() {
    use xmtp_db::group_message::StoredGroupMessage;
    use xmtp_proto::xmtp::device_sync::message_backup::{
        DeliveryStatusSave, GroupMessageKindSave, GroupMessageSave,
    };
    let base = StoredGroupMessage::try_from(GroupMessageSave {
        id: vec![1; 32],
        group_id: vec![2; 32],
        sender_inbox_id: hex::encode([3; 32]),
        kind: GroupMessageKindSave::Application as i32,
        delivery_status: DeliveryStatusSave::Published as i32,
        ..Default::default()
    })?;
    for (deadline, unknown) in [(Some(123_456_789), false), (None, false), (None, true)] {
        let mut stored = base.clone();
        stored.expire_at_ns = deadline;
        stored.expiry_unknown = unknown;
        let message = crate::Message::from_stored(stored, 0)?.0;
        assert_eq!(message.expiry_unknown, unknown);
        assert_eq!(message.expires_at.map(|time| time.0), deadline);
    }
}

fn large_group_update(inbox_id: &str) -> xmtp_proto::xmtp::mls::message_contents::EncodedContent {
    use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
    use xmtp_proto::xmtp::mls::message_contents::{GroupUpdated, group_updated::Inbox};

    const ENTRIES_PER_FIELD: usize = 8_192;
    let inbox = Inbox {
        inbox_id: inbox_id.into(),
    };
    GroupUpdatedCodec::encode(GroupUpdated {
        initiated_by_inbox_id: inbox.inbox_id.clone(),
        added_inboxes: vec![inbox.clone(); ENTRIES_PER_FIELD],
        removed_inboxes: vec![inbox.clone(); ENTRIES_PER_FIELD],
        left_inboxes: vec![inbox.clone(); ENTRIES_PER_FIELD],
        added_admin_inboxes: vec![inbox.clone(); ENTRIES_PER_FIELD],
        removed_admin_inboxes: vec![inbox.clone(); ENTRIES_PER_FIELD],
        added_super_admin_inboxes: vec![inbox.clone(); ENTRIES_PER_FIELD],
        removed_super_admin_inboxes: vec![inbox; ENTRIES_PER_FIELD],
        ..Default::default()
    })
    .unwrap()
}

// verifies: GMOD-054
#[xmtp_common::test(unwrap_try = true)]
async fn sdk_reply_with_nested_group_updated_is_refused() {
    use prost::Message as _;

    let sender = Client::create(crate::generate_local_signer().await, options()).await?;
    let recipient = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = sender
        .conversations()
        .create_group(vec![recipient.inbox_id()], None)
        .await?;
    let reference = group.send_text("reply target".into(), None).await?;
    let nested = large_group_update(&sender.inbox_id().into_checked()?);
    assert!(nested.encoded_len() > xmtp_configuration::BACKEND_DEFAULT_MAX_ENVELOPE_BYTES);
    let result = group
        .send_reply(
            reference.clone(),
            Some(sender.inbox_id()),
            nested.try_into()?,
            Some(crate::SendOptions {
                compression: Some(crate::Compression::Gzip),
                ..Default::default()
            }),
        )
        .await;
    assert!(
        matches!(result, Err(XmtpError::InvalidInput(details)) if details.code == "ReservedTranscriptContentType")
    );
    recipient.conversations().sync_all(None).await?;
    let Some(crate::Conversation::Group { group: received }) =
        recipient.conversations().get_by_id(group.id()).await?
    else {
        panic!("recipient must receive the conversation");
    };
    let application = received
        .messages(None)
        .await?
        .into_iter()
        .filter(|message| matches!(message.0.kind, crate::MessageKind::Application))
        .collect::<Vec<_>>();
    assert_eq!(application.len(), 1, "the refused reply must not arrive");
    assert_eq!(application[0].0.id, reference);
    sender.end().await?;
    recipient.end().await?;
}

// verifies: GMOD-053
#[xmtp_common::test(unwrap_try = true)]
fn application_group_updated_is_not_decoded() {
    use prost::Message as _;

    let wire = large_group_update(&"a".repeat(64)).encode_to_vec();
    assert!(wire.len() < xmtp_configuration::MAX_TRANSPORT_BYTES);
    assert!(matches!(
        MessageContent::decode(wire),
        Err(XmtpError::CodecDecodeFailed(_))
    ));
}

// verifies: GMOD-053, CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
fn stored_message_kind_selects_transcript_decoding() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec, compression::compress, group_updated::GroupUpdatedCodec,
    };
    use xmtp_db::group_message::{GroupMessageKind, StoredGroupMessage};
    use xmtp_proto::xmtp::{
        device_sync::message_backup::{DeliveryStatusSave, GroupMessageKindSave, GroupMessageSave},
        mls::message_contents::{Compression, GroupUpdated},
    };

    let base = StoredGroupMessage::try_from(GroupMessageSave {
        id: vec![1; 32],
        group_id: vec![2; 32],
        sender_inbox_id: "a".repeat(64),
        kind: GroupMessageKindSave::Application as i32,
        delivery_status: DeliveryStatusSave::Published as i32,
        ..Default::default()
    })?;
    let mut transcript = GroupUpdatedCodec::encode(GroupUpdated {
        initiated_by_inbox_id: "committer".into(),
        ..Default::default()
    })?;
    transcript.fallback = Some("group update fallback".into());
    for envelope in [
        transcript.clone(),
        compress(transcript.clone(), Compression::Gzip)?,
    ] {
        let bytes = envelope.encode_to_vec();
        for kind in [
            GroupMessageKind::MembershipChange,
            GroupMessageKind::Application,
        ] {
            for enriched in [false, true] {
                let mut stored = base.clone();
                stored.kind = kind;
                stored.decrypted_message_bytes = bytes.clone();
                let message = if enriched {
                    let decoded =
                        xmtp_mls::messages::decoded_message::DecodedMessage::from(stored.clone());
                    crate::Message::from_enriched(stored, decoded, None, 0)?
                } else {
                    crate::Message::from_stored(stored, 0)?
                }
                .0;
                assert_eq!(message.id.to_bytes()?, base.id);
                assert_eq!(message.raw_bytes, bytes);
                assert_eq!(
                    message.content_type.as_ref().unwrap().type_id,
                    "group_updated"
                );
                assert_eq!(message.fallback, transcript.fallback);
                match (kind, message.content) {
                    (GroupMessageKind::MembershipChange, MessageContent::GroupUpdated(update)) => {
                        assert_eq!(update.initiated_by_inbox_id.checked()?, "committer")
                    }
                    (
                        GroupMessageKind::Application,
                        MessageContent::Unknown {
                            encoded,
                            raw_bytes,
                            error,
                        },
                    ) => {
                        assert_eq!(error.code, "CodecDecodeFailed");
                        assert_eq!(raw_bytes, bytes);
                        let encoded =
                            encoded.expect("unknown content remains available to the app");
                        assert_eq!(encoded.r#type.type_id, "group_updated");
                        assert_eq!(encoded.fallback, transcript.fallback);
                        assert_eq!(encoded.content, transcript.content);
                    }
                    (_, content) => panic!("wrong transcript projection: {content:?}"),
                }
            }
        }
    }
}
