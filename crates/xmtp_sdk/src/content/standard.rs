/// Content at this boundary is uncompressed. Send options control wire compression.
#[derive(Clone, Debug, uniffi::Record)]
pub struct EncodedContent {
    pub r#type: ContentTypeId,
    #[uniffi(default)]
    pub parameters: HashMap<String, String>,
    #[uniffi(default = None)]
    pub fallback: Option<String>,
    pub content: Vec<u8>,
}

/// A standard value that can be encoded without a client.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum StandardContent {
    Text(String),
    Markdown(String),
    ReadReceipt,
    Reaction {
        reference: crate::MessageId,
        reference_inbox_id: Option<crate::InboxId>,
        reaction: Reaction,
    },
    Attachment(Attachment),
    RemoteAttachment(RemoteAttachment),
    MultiRemoteAttachment(MultiRemoteAttachment),
    TransactionReference(TransactionReference),
    WalletSendCalls(WalletSendCalls),
    Actions(Actions),
    Intent(Intent),
    Reply {
        reference: crate::MessageId,
        reference_inbox_id: Option<crate::InboxId>,
        content: EncodedContent,
    },
    GroupUpdated(GroupUpdated),
    DeleteMessage {
        message_id: crate::MessageId,
    },
    LeaveRequest(LeaveRequest),
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum StandardContentKind {
    Text,
    Markdown,
    ReadReceipt,
    Reaction,
    Attachment,
    RemoteAttachment,
    MultiRemoteAttachment,
    TransactionReference,
    WalletSendCalls,
    Actions,
    Intent,
    Reply,
    GroupUpdated,
    DeleteMessage,
    LeaveRequest,
}

fn codec_error(error: xmtp_content_types::CodecError) -> crate::XmtpError {
    crate::XmtpError::invalid(error.to_string())
}

fn standard_type(kind: StandardContentKind) -> ProtoContentTypeId {
    use xmtp_content_types::ContentCodec;
    match kind {
        StandardContentKind::Text => xmtp_content_types::text::TextCodec::content_type(),
        StandardContentKind::Markdown => {
            xmtp_content_types::markdown::MarkdownCodec::content_type()
        }
        StandardContentKind::ReadReceipt => {
            xmtp_content_types::read_receipt::ReadReceiptCodec::content_type()
        }
        StandardContentKind::Reaction => {
            xmtp_content_types::reaction::ReactionCodec::content_type()
        }
        StandardContentKind::Attachment => {
            xmtp_content_types::attachment::AttachmentCodec::content_type()
        }
        StandardContentKind::RemoteAttachment => {
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::content_type()
        }
        StandardContentKind::MultiRemoteAttachment => {
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::content_type()
        }
        StandardContentKind::TransactionReference => {
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::content_type()
        }
        StandardContentKind::WalletSendCalls => {
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::content_type()
        }
        StandardContentKind::Actions => xmtp_content_types::actions::ActionsCodec::content_type(),
        StandardContentKind::Intent => xmtp_content_types::intent::IntentCodec::content_type(),
        StandardContentKind::Reply => xmtp_content_types::reply::ReplyCodec::content_type(),
        StandardContentKind::GroupUpdated => {
            xmtp_content_types::group_updated::GroupUpdatedCodec::content_type()
        }
        StandardContentKind::DeleteMessage => {
            xmtp_content_types::delete_message::DeleteMessageCodec::content_type()
        }
        StandardContentKind::LeaveRequest => {
            xmtp_content_types::leave_request::LeaveRequestCodec::content_type()
        }
    }
}

#[xmtp_macro::sdk_export(pure)]
pub fn standard_content_type(kind: StandardContentKind) -> ContentTypeId {
    let kind = standard_type(kind);
    ContentTypeId {
        authority_id: kind.authority_id,
        type_id: kind.type_id,
        version_major: kind.version_major,
        version_minor: kind.version_minor,
    }
}

#[xmtp_macro::sdk_export(pure)]
pub fn encode_standard(value: StandardContent) -> Result<EncodedContent, crate::XmtpError> {
    use xmtp_content_types::ContentCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types as proto;
    let encoded = match value {
        StandardContent::Text(value) => xmtp_content_types::text::TextCodec::encode(value),
        StandardContent::Markdown(value) => {
            xmtp_content_types::markdown::MarkdownCodec::encode(value)
        }
        StandardContent::ReadReceipt => xmtp_content_types::read_receipt::ReadReceiptCodec::encode(
            xmtp_content_types::read_receipt::ReadReceipt {},
        ),
        StandardContent::Reaction {
            reference,
            reference_inbox_id,
            reaction,
        } => xmtp_content_types::reaction::ReactionCodec::encode(reaction.into_proto(
            reference,
            crate::InboxId(reference_inbox_id.map(|id| id.0).unwrap_or_default()),
        )),
        StandardContent::Attachment(value) => {
            xmtp_content_types::attachment::AttachmentCodec::encode(
                xmtp_content_types::attachment::Attachment {
                    filename: value.filename,
                    mime_type: value.mime_type,
                    content: value.content,
                },
            )
        }
        StandardContent::RemoteAttachment(value) => {
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::encode(value.into())
        }
        StandardContent::MultiRemoteAttachment(value) => {
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::encode(
                proto::MultiRemoteAttachment {
                    attachments: value.attachments.into_iter().map(Into::into).collect(),
                },
            )
        }
        StandardContent::TransactionReference(value) => {
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::encode(
                xmtp_content_types::transaction_reference::TransactionReference {
                    namespace: value.namespace,
                    network_id: value.network_id,
                    reference: value.reference,
                    metadata: value.metadata.map(|metadata| {
                        xmtp_content_types::transaction_reference::TransactionMetadata {
                            transaction_type: metadata.transaction_type,
                            currency: metadata.currency,
                            amount: metadata.amount,
                            decimals: metadata.decimals,
                            from_address: metadata.from_address,
                            to_address: metadata.to_address,
                        }
                    }),
                },
            )
        }
        StandardContent::WalletSendCalls(value) => {
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::encode(value.into())
        }
        StandardContent::Actions(value) => {
            xmtp_content_types::actions::ActionsCodec::encode(value.into())
        }
        StandardContent::Intent(value) => {
            xmtp_content_types::intent::IntentCodec::encode(value.try_into()?)
        }
        StandardContent::Reply {
            reference,
            reference_inbox_id,
            content,
        } => xmtp_content_types::reply::ReplyCodec::encode(xmtp_content_types::reply::Reply {
            reference: reference.0,
            reference_inbox_id: reference_inbox_id.map(|id| id.0),
            content: content.into(),
        }),
        StandardContent::GroupUpdated(value) => {
            xmtp_content_types::group_updated::GroupUpdatedCodec::encode(value.into())
        }
        StandardContent::DeleteMessage { message_id } => {
            xmtp_content_types::delete_message::DeleteMessageCodec::encode(proto::DeleteMessage {
                message_id: message_id.0,
            })
        }
        StandardContent::LeaveRequest(value) => {
            xmtp_content_types::leave_request::LeaveRequestCodec::encode(proto::LeaveRequest {
                authenticated_note: value.authenticated_note,
            })
        }
    }
    .map_err(codec_error)?;
    Ok(encoded.into())
}

#[xmtp_macro::sdk_export(pure)]
pub fn decode_standard(encoded: EncodedContent) -> Result<StandardContent, crate::XmtpError> {
    use xmtp_content_types::ContentCodec;
    let kind = [
        StandardContentKind::Text,
        StandardContentKind::Markdown,
        StandardContentKind::ReadReceipt,
        StandardContentKind::Reaction,
        StandardContentKind::Attachment,
        StandardContentKind::RemoteAttachment,
        StandardContentKind::MultiRemoteAttachment,
        StandardContentKind::TransactionReference,
        StandardContentKind::WalletSendCalls,
        StandardContentKind::Actions,
        StandardContentKind::Intent,
        StandardContentKind::Reply,
        StandardContentKind::GroupUpdated,
        StandardContentKind::DeleteMessage,
        StandardContentKind::LeaveRequest,
    ]
    .into_iter()
    .find(|kind| {
        let expected = standard_type(*kind);
        encoded.r#type.authority_id == expected.authority_id
            && encoded.r#type.type_id == expected.type_id
            && encoded.r#type.version_major == expected.version_major
    })
    .ok_or_else(|| crate::XmtpError::invalid("unsupported standard content type"))?;
    let encoded: ProtoEncodedContent = encoded.into();
    Ok(match kind {
        StandardContentKind::Text => StandardContent::Text(
            xmtp_content_types::text::TextCodec::decode(encoded).map_err(codec_error)?,
        ),
        StandardContentKind::Markdown => StandardContent::Markdown(
            xmtp_content_types::markdown::MarkdownCodec::decode(encoded).map_err(codec_error)?,
        ),
        StandardContentKind::ReadReceipt => {
            xmtp_content_types::read_receipt::ReadReceiptCodec::decode(encoded)
                .map_err(codec_error)?;
            StandardContent::ReadReceipt
        }
        StandardContentKind::Reaction => {
            let value = xmtp_content_types::reaction::ReactionCodec::decode(encoded)
                .map_err(codec_error)?;
            let reaction = Reaction::from_proto(value.clone());
            StandardContent::Reaction {
                reference: crate::MessageId::try_from(value.reference)?,
                reference_inbox_id: (!value.reference_inbox_id.is_empty())
                    .then_some(value.reference_inbox_id)
                    .map(crate::InboxId::try_from)
                    .transpose()?,
                reaction,
            }
        }
        StandardContentKind::Attachment => StandardContent::Attachment(
            xmtp_content_types::attachment::AttachmentCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::RemoteAttachment => StandardContent::RemoteAttachment(
            xmtp_content_types::remote_attachment::RemoteAttachmentCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::MultiRemoteAttachment => StandardContent::MultiRemoteAttachment(
            xmtp_content_types::multi_remote_attachment::MultiRemoteAttachmentCodec::decode(
                encoded,
            )
            .map_err(codec_error)?
            .into(),
        ),
        StandardContentKind::TransactionReference => StandardContent::TransactionReference(
            xmtp_content_types::transaction_reference::TransactionReferenceCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::WalletSendCalls => StandardContent::WalletSendCalls(
            xmtp_content_types::wallet_send_calls::WalletSendCallsCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::Actions => StandardContent::Actions(
            xmtp_content_types::actions::ActionsCodec::decode(encoded)
                .map_err(codec_error)?
                .try_into()?,
        ),
        StandardContentKind::Intent => StandardContent::Intent(
            xmtp_content_types::intent::IntentCodec::decode(encoded)
                .map_err(codec_error)?
                .into(),
        ),
        StandardContentKind::Reply => {
            let value =
                xmtp_content_types::reply::ReplyCodec::decode(encoded).map_err(codec_error)?;
            let nested_type = value.content.r#type.as_ref().ok_or_else(|| {
                crate::XmtpError::invalid("nested reply content has no content type")
            })?;
            if nested_type.authority_id.is_empty() || nested_type.type_id.is_empty() {
                return Err(crate::XmtpError::invalid(
                    "nested reply content has an empty content type",
                ));
            }
            // implements: CTYPE-024
            // implements: CTYPE-025
            let nested =
                xmtp_content_types::compression::decompress(value.content).map_err(codec_error)?;
            xmtp_mls::messages::decoded_message::MessageBody::try_from(nested.clone())
                .map_err(|error| crate::XmtpError::invalid(error.to_string()))?;
            StandardContent::Reply {
                reference: crate::MessageId::try_from(value.reference)?,
                reference_inbox_id: value
                    .reference_inbox_id
                    .map(crate::InboxId::try_from)
                    .transpose()?,
                content: nested.into(),
            }
        }
        StandardContentKind::GroupUpdated => StandardContent::GroupUpdated(
            xmtp_content_types::group_updated::GroupUpdatedCodec::decode(encoded)
                .map_err(codec_error)?
                .try_into()?,
        ),
        StandardContentKind::DeleteMessage => StandardContent::DeleteMessage {
            message_id: crate::MessageId::try_from(
                xmtp_content_types::delete_message::DeleteMessageCodec::decode(encoded)
                    .map_err(codec_error)?
                    .message_id,
            )?,
        },
        StandardContentKind::LeaveRequest => StandardContent::LeaveRequest(LeaveRequest {
            authenticated_note: xmtp_content_types::leave_request::LeaveRequestCodec::decode(
                encoded,
            )
            .map_err(codec_error)?
            .authenticated_note,
        }),
    })
}

#[xmtp_macro::sdk_export(pure)]
pub fn encode_text(text: String) -> Result<EncodedContent, crate::XmtpError> {
    encode_standard(StandardContent::Text(text))
}

