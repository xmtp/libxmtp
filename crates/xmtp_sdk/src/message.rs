use prost::Message as _;
use xmtp_db::group_message::{
    DeliveryStatus as StoredDeliveryStatus, GroupMessageKind, StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::{
    ConversationId, EncodedContent as SdkEncodedContent, ErrorDetails, InboxId, MessageId,
    Timestamp, XmtpError,
};

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageKind {
    Application,
    MembershipChange,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum DeliveryStatus {
    Unpublished,
    Published,
    Failed,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
    pub version_minor: u32,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageContent {
    Text(String),
    Markdown(String),
    ReadReceipt,
    Reaction {
        reference: MessageId,
        reference_inbox_id: Option<InboxId>,
        reaction: crate::Reaction,
    },
    Attachment(crate::Attachment),
    RemoteAttachment(crate::RemoteAttachment),
    MultiRemoteAttachment(crate::MultiRemoteAttachment),
    TransactionReference(crate::TransactionReference),
    WalletSendCalls(crate::WalletSendCalls),
    Actions(crate::Actions),
    Intent(crate::Intent),
    GroupUpdated(crate::GroupUpdated),
    LeaveRequest(crate::LeaveRequest),
    DeletedMessage(crate::DeletedMessage),
    Reply {
        reference_id: MessageId,
        body: MessageBody,
    },
    Custom {
        encoded: SdkEncodedContent,
        raw_bytes: Vec<u8>,
    },
    Unknown {
        encoded: Option<SdkEncodedContent>,
        raw_bytes: Vec<u8>,
        error: ErrorDetails,
    },
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum MessageBody {
    Text(String),
    Markdown(String),
    ReadReceipt,
    Attachment(crate::Attachment),
    RemoteAttachment(crate::RemoteAttachment),
    MultiRemoteAttachment(crate::MultiRemoteAttachment),
    TransactionReference(crate::TransactionReference),
    WalletSendCalls(crate::WalletSendCalls),
    Actions(crate::Actions),
    Intent(crate::Intent),
    GroupUpdated(crate::GroupUpdated),
    LeaveRequest(crate::LeaveRequest),
    DeletedMessage(crate::DeletedMessage),
    Custom {
        encoded: SdkEncodedContent,
        raw_bytes: Vec<u8>,
    },
    Unknown {
        encoded: Option<SdkEncodedContent>,
        raw_bytes: Vec<u8>,
        error: ErrorDetails,
    },
}

fn received_encoded(content: Option<EncodedContent>) -> Option<SdkEncodedContent> {
    content.and_then(|content| content.try_into().ok())
}

fn failure_details(
    failure: xmtp_mls::messages::decoded_message::ContentDecodeFailure,
) -> ErrorDetails {
    use xmtp_mls::messages::decoded_message::ContentDecodeFailureKind;
    match failure.kind {
        ContentDecodeFailureKind::MalformedEnvelope => {
            XmtpError::malformed_envelope(failure.message)
        }
        ContentDecodeFailureKind::CodecDecodeFailed => {
            XmtpError::codec_decode_failed(failure.message)
        }
    }
    .content_details()
}

impl MessageContent {
    pub(crate) fn decode(encoded: Vec<u8>) -> Result<Self, XmtpError> {
        let content = EncodedContent::decode(encoded.as_slice())
            .map_err(|error| XmtpError::malformed_envelope(error.to_string()))?;
        Self::decode_proto(content, &encoded)
    }

    fn decode_proto(content: EncodedContent, raw_bytes: &[u8]) -> Result<Self, XmtpError> {
        use xmtp_mls::messages::decoded_message::MessageBody as CoreBody;
        // The shared bounded decoder validates nested content before any standard codec runs.
        let body = CoreBody::from_received_bytes(raw_bytes);
        if let CoreBody::Undecodable(value) = body {
            let details = failure_details(value.failure);
            return Err(if details.code == "MalformedEnvelope" {
                XmtpError::MalformedEnvelope(details)
            } else {
                XmtpError::CodecDecodeFailed(details)
            });
        }
        Self::from_core(body, Some(content), raw_bytes)
    }

    fn from_core(
        body: xmtp_mls::messages::decoded_message::MessageBody,
        content: Option<EncodedContent>,
        raw_bytes: &[u8],
    ) -> Result<Self, XmtpError> {
        use xmtp_mls::messages::decoded_message::MessageBody as CoreBody;
        match body {
            CoreBody::Text(value) => Ok(Self::Text(value.content)),
            CoreBody::Markdown(value) => Ok(Self::Markdown(value.content)),
            CoreBody::ReadReceipt(_) => Ok(Self::ReadReceipt),
            CoreBody::Reaction(value) => {
                let reference = MessageId::try_from(value.reference.clone())?;
                let reference_inbox_id = (!value.reference_inbox_id.is_empty())
                    .then(|| InboxId::try_from(value.reference_inbox_id.clone()))
                    .transpose()?;
                Ok(Self::Reaction {
                    reference,
                    reference_inbox_id,
                    reaction: crate::Reaction::from_proto(value),
                })
            }
            CoreBody::Reply(value) => {
                let outer: SdkEncodedContent = content
                    .ok_or_else(|| XmtpError::malformed_envelope("reply has no envelope"))?
                    .try_into()?;
                let nested = EncodedContent::decode(outer.content.as_slice()).ok();
                Ok(Self::Reply {
                    reference_id: MessageId::try_from(value.reference_id)?,
                    body: MessageBody::from_core(
                        *value.content,
                        received_encoded(nested),
                        &outer.content,
                    )?,
                })
            }
            CoreBody::Attachment(value) => Ok(Self::Attachment(value.into())),
            CoreBody::RemoteAttachment(value) => Ok(Self::RemoteAttachment(value.into())),
            CoreBody::MultiRemoteAttachment(value) => Ok(Self::MultiRemoteAttachment(value.into())),
            CoreBody::TransactionReference(value) => Ok(Self::TransactionReference(value.into())),
            CoreBody::WalletSendCalls(value) => Ok(Self::WalletSendCalls(value.into())),
            CoreBody::Actions(Some(value)) => Ok(Self::Actions(value.try_into()?)),
            CoreBody::Intent(Some(value)) => Ok(Self::Intent(value.into())),
            CoreBody::GroupUpdated(value) => Ok(Self::GroupUpdated(value.try_into()?)),
            CoreBody::LeaveRequest(value) => Ok(Self::LeaveRequest(crate::LeaveRequest {
                authenticated_note: value.authenticated_note,
            })),
            CoreBody::DeletedMessage { deleted_by } => {
                Ok(Self::DeletedMessage(crate::DeletedMessage {
                    deleted_by: deleted_by.try_into()?,
                }))
            }
            CoreBody::Custom(value) => Ok(Self::Custom {
                encoded: value.encoded.try_into()?,
                raw_bytes: value.raw_bytes,
            }),
            CoreBody::Undecodable(value) => Ok(Self::Unknown {
                encoded: received_encoded(value.encoded),
                raw_bytes: value.raw_bytes,
                error: failure_details(value.failure),
            }),
            _ => Ok(Self::Unknown {
                encoded: received_encoded(content),
                raw_bytes: raw_bytes.to_vec(),
                error: XmtpError::codec_not_found("content type has no SDK decoder")
                    .content_details(),
            }),
        }
    }
}

impl MessageBody {
    fn from_core(
        body: xmtp_mls::messages::decoded_message::MessageBody,
        encoded: Option<SdkEncodedContent>,
        raw_bytes: &[u8],
    ) -> Result<Self, XmtpError> {
        use xmtp_mls::messages::decoded_message::MessageBody as CoreBody;
        Ok(match body {
            CoreBody::Text(value) => Self::Text(value.content),
            CoreBody::Markdown(value) => Self::Markdown(value.content),
            CoreBody::ReadReceipt(_) => Self::ReadReceipt,
            CoreBody::Attachment(value) => Self::Attachment(value.into()),
            CoreBody::RemoteAttachment(value) => Self::RemoteAttachment(value.into()),
            CoreBody::MultiRemoteAttachment(value) => Self::MultiRemoteAttachment(value.into()),
            CoreBody::TransactionReference(value) => Self::TransactionReference(value.into()),
            CoreBody::WalletSendCalls(value) => Self::WalletSendCalls(value.into()),
            CoreBody::Actions(Some(value)) => Self::Actions(value.try_into()?),
            CoreBody::Intent(Some(value)) => Self::Intent(value.into()),
            CoreBody::GroupUpdated(value) => Self::GroupUpdated(value.try_into()?),
            CoreBody::LeaveRequest(value) => Self::LeaveRequest(crate::LeaveRequest {
                authenticated_note: value.authenticated_note,
            }),
            CoreBody::DeletedMessage { deleted_by } => {
                Self::DeletedMessage(crate::DeletedMessage {
                    deleted_by: deleted_by.try_into()?,
                })
            }
            // Core returns Custom only for a complete typed, decompressed
            // envelope; a nested failure makes the outer body undecodable.
            CoreBody::Custom(value) => Self::Custom {
                encoded: value.encoded.try_into()?,
                raw_bytes: value.raw_bytes,
            },
            CoreBody::Undecodable(value) => Self::Unknown {
                encoded: received_encoded(value.encoded),
                raw_bytes: value.raw_bytes,
                error: failure_details(value.failure),
            },
            _ => Self::Unknown {
                encoded,
                raw_bytes: raw_bytes.to_vec(),
                error: XmtpError::codec_not_found("content type has no SDK reply-body decoder")
                    .content_details(),
            },
        })
    }
}

impl TryFrom<xmtp_mls::messages::decoded_message::DeletedBy> for crate::DeletedBy {
    type Error = XmtpError;
    fn try_from(
        value: xmtp_mls::messages::decoded_message::DeletedBy,
    ) -> Result<Self, Self::Error> {
        Ok(match value {
            xmtp_mls::messages::decoded_message::DeletedBy::Sender => Self::Sender,
            xmtp_mls::messages::decoded_message::DeletedBy::Admin(inbox_id) => Self::Admin {
                inbox_id: InboxId::try_from(inbox_id)?,
            },
        })
    }
}

/// A message value contains records and enums only.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageData {
    pub id: MessageId,
    pub client_key: u64,
    #[uniffi(default = None)]
    pub delivery_cursor: Option<String>,
    pub conversation_id: ConversationId,
    pub topic: String,
    pub sender_inbox_id: InboxId,
    pub sent_at: Timestamp,
    pub inserted_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub kind: MessageKind,
    pub delivery_status: DeliveryStatus,
    /// The original received serialization. Empty after deletion.
    pub raw_bytes: Vec<u8>,
    /// The received type, when the envelope has a readable type field.
    pub content_type: Option<ContentTypeId>,
    pub fallback: Option<String>,
    /// Uncompressed codec input. Absent if conversion or decompression fails.
    pub encoded: Option<SdkEncodedContent>,
    pub content: MessageContent,
    pub reply_count: u64,
    pub reactions: Vec<ReactionMessage>,
    pub in_reply_to: Option<ReplyParent>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReactionMessage {
    pub id: MessageId,
    pub sender_inbox_id: InboxId,
    pub sent_at: Timestamp,
    pub delivery_status: DeliveryStatus,
    pub reaction: crate::Reaction,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReplyParent {
    pub id: MessageId,
    pub sender_inbox_id: InboxId,
    pub sent_at: Timestamp,
    pub kind: MessageKind,
    pub delivery_status: DeliveryStatus,
    /// The original received serialization. Empty after deletion.
    pub raw_bytes: Vec<u8>,
    /// The received type, when the envelope has a readable type field.
    pub content_type: Option<ContentTypeId>,
    pub fallback: Option<String>,
    /// Uncompressed codec input. Absent if conversion or decompression fails.
    pub encoded: Option<SdkEncodedContent>,
    pub content: MessageBody,
}

/// The host runtime lifts this value to a Message class.
#[derive(Clone, Debug)]
pub struct Message(pub MessageData);

uniffi::custom_newtype!(Message, MessageData);

impl Message {
    pub(crate) fn with_delivery_cursor(
        mut self,
        cursor: Option<xmtp_db::delivery::DeliveryCursor>,
    ) -> Self {
        self.0.delivery_cursor = cursor.map(crate::delivery::cursor::encode);
        self
    }

    pub(crate) fn from_stored(
        value: StoredGroupMessage,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        Self::from_stored_with_content(value, client_key, None)
    }

    fn from_stored_with_content(
        value: StoredGroupMessage,
        client_key: u64,
        decoded: Option<xmtp_mls::messages::decoded_message::MessageBody>,
    ) -> Result<Self, XmtpError> {
        let deleted = matches!(
            decoded.as_ref(),
            Some(xmtp_mls::messages::decoded_message::MessageBody::DeletedMessage { .. })
        );
        let message_bytes = if deleted {
            &[][..]
        } else {
            value.decrypted_message_bytes.as_slice()
        };
        let encoded = if deleted {
            Some(EncodedContent {
                r#type: Some(xmtp_mls::messages::enrichment::deleted_message_content_type()),
                ..Default::default()
            })
        } else {
            EncodedContent::decode(message_bytes).ok()
        };
        let content_type = encoded
            .as_ref()
            .and_then(|content| content.r#type.clone())
            .map(|kind| ContentTypeId {
                authority_id: kind.authority_id,
                type_id: kind.type_id,
                version_major: kind.version_major,
                version_minor: kind.version_minor,
            });
        let fallback = encoded
            .as_ref()
            .and_then(|content| content.fallback.clone());
        let body = decoded.unwrap_or_else(|| {
            xmtp_mls::messages::decoded_message::MessageBody::from_received_bytes(message_bytes)
        });
        let content = MessageContent::from_core(body, encoded.clone(), message_bytes)
            .unwrap_or_else(|error| MessageContent::Unknown {
                encoded: received_encoded(encoded.clone()),
                raw_bytes: message_bytes.to_vec(),
                error: error.content_details(),
            });
        let kind = match value.kind {
            GroupMessageKind::Application => MessageKind::Application,
            GroupMessageKind::MembershipChange => MessageKind::MembershipChange,
        };
        let delivery_status = match value.delivery_status {
            StoredDeliveryStatus::Unpublished => DeliveryStatus::Unpublished,
            StoredDeliveryStatus::Published => DeliveryStatus::Published,
            StoredDeliveryStatus::Failed => DeliveryStatus::Failed,
        };
        Ok(Self(MessageData {
            id: MessageId::from_bytes(&value.id)?,
            client_key,
            delivery_cursor: None,
            conversation_id: value.group_id.into(),
            topic: xmtp_proto::types::Topic::new_group_message(value.group_id).to_string(),
            sender_inbox_id: InboxId::try_from(value.sender_inbox_id)?,
            sent_at: Timestamp(value.sent_at_ns),
            inserted_at: Timestamp(value.inserted_at_ns),
            expires_at: value.expire_at_ns.map(Timestamp),
            kind,
            delivery_status,
            raw_bytes: message_bytes.to_vec(),
            content_type,
            fallback,
            encoded: received_encoded(encoded),
            content,
            reply_count: 0,
            reactions: Vec::new(),
            in_reply_to: None,
        }))
    }

    pub(crate) fn from_enriched(
        value: StoredGroupMessage,
        enriched: xmtp_mls::messages::decoded_message::DecodedMessage,
        parent_stored: Option<StoredGroupMessage>,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        use xmtp_mls::messages::decoded_message::MessageBody as CoreBody;
        let mut message =
            Self::from_stored_with_content(value, client_key, Some(enriched.content.clone()))?;
        message.0.reply_count = enriched.num_replies as u64;
        message.0.reactions = enriched
            .reactions
            .into_iter()
            .filter_map(|reaction| {
                let CoreBody::Reaction(value) = reaction.content else {
                    return None;
                };
                let reaction_id = hex::encode(&reaction.metadata.id);
                let converted: Result<ReactionMessage, XmtpError> = (|| {
                    Ok(ReactionMessage {
                        id: MessageId::from_bytes(&reaction.metadata.id)?,
                        sender_inbox_id: InboxId::try_from(reaction.metadata.sender_inbox_id)?,
                        sent_at: Timestamp(reaction.metadata.sent_at_ns),
                        delivery_status: reaction.metadata.delivery_status.into(),
                        reaction: crate::Reaction::from_proto(value),
                    })
                })();
                match converted {
                    Ok(reaction) => Some(reaction),
                    Err(error) => {
                        tracing::warn!(%reaction_id, %error, "skipping stored reaction");
                        None
                    }
                }
            })
            .collect();
        if let CoreBody::Reply(reply) = &enriched.content
            && let Some(parent) = &reply.in_reply_to
        {
            let parent = parent.as_ref();
            if let Some(parent_data) = parent_stored.and_then(|parent_stored| {
                let parent_id = hex::encode(&parent_stored.id);
                match Self::from_stored_with_content(
                    parent_stored,
                    client_key,
                    Some(parent.content.clone()),
                ) {
                    Ok(message) => Some(message.0),
                    Err(error) => {
                        tracing::warn!(%parent_id, %error, "omitting reply parent");
                        None
                    }
                }
            }) {
                let parent_content = MessageBody::from_core(
                    parent.content.clone(),
                    parent_data.encoded.clone(),
                    &parent_data.raw_bytes,
                )
                .unwrap_or_else(|error| MessageBody::Unknown {
                    encoded: parent_data.encoded.clone(),
                    raw_bytes: parent_data.raw_bytes.clone(),
                    error: error.content_details(),
                });
                message.0.in_reply_to = Some(ReplyParent {
                    id: parent_data.id,
                    sender_inbox_id: parent_data.sender_inbox_id,
                    sent_at: parent_data.sent_at,
                    kind: parent_data.kind,
                    delivery_status: parent_data.delivery_status,
                    raw_bytes: parent_data.raw_bytes,
                    content_type: parent_data.content_type,
                    fallback: parent_data.fallback,
                    encoded: parent_data.encoded.clone(),
                    content: parent_content,
                });
            }
        }
        Ok(message)
    }
}

impl From<GroupMessageKind> for MessageKind {
    fn from(value: GroupMessageKind) -> Self {
        match value {
            GroupMessageKind::Application => Self::Application,
            GroupMessageKind::MembershipChange => Self::MembershipChange,
        }
    }
}

impl From<StoredDeliveryStatus> for DeliveryStatus {
    fn from(value: StoredDeliveryStatus) -> Self {
        match value {
            StoredDeliveryStatus::Unpublished => Self::Unpublished,
            StoredDeliveryStatus::Published => Self::Published,
            StoredDeliveryStatus::Failed => Self::Failed,
        }
    }
}
