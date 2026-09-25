use prost::Message as _;
use xmtp_content_types::{ContentCodec, reply::ReplyCodec};
use xmtp_db::group_message::{
    ContentType as StoredContentType, DeliveryStatus as StoredDeliveryStatus, GroupMessageKind,
    StoredGroupMessage,
};
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use crate::{
    ConversationID, EncodedContent as SdkEncodedContent, InboxID, MessageID, Timestamp, XmtpError,
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
    Reaction(crate::Reaction),
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
        reference_id: MessageID,
        body: MessageBody,
    },
    Custom {
        encoded: SdkEncodedContent,
        raw_bytes: Vec<u8>,
    },
    Unknown {
        encoded: SdkEncodedContent,
        raw_bytes: Vec<u8>,
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
    Custom { encoded: SdkEncodedContent },
    Unknown { encoded: SdkEncodedContent },
}

fn core_decodes_standard(content: &EncodedContent) -> bool {
    let Some(kind) = content.r#type.as_ref() else {
        return false;
    };
    !matches!(
        StoredContentType::from_identifier(&kind.authority_id, &kind.type_id, kind.version_major,),
        StoredContentType::Unknown
            | StoredContentType::GroupMembershipChange
            | StoredContentType::DeleteMessage
    )
}

impl MessageContent {
    pub(crate) fn decode(encoded: Vec<u8>) -> Result<Self, XmtpError> {
        let content = EncodedContent::decode(encoded.as_slice()).map_err(XmtpError::unknown)?;
        Self::decode_proto(content, &encoded)
    }

    fn decode_proto(content: EncodedContent, raw_bytes: &[u8]) -> Result<Self, XmtpError> {
        // The shared bounded decoder validates nested content before any standard codec runs.
        let body = xmtp_mls::messages::decoded_message::MessageBody::try_from(content.clone())
            .map_err(XmtpError::unknown)?;
        Self::from_core(body, content, raw_bytes)
    }

    fn from_core(
        body: xmtp_mls::messages::decoded_message::MessageBody,
        content: EncodedContent,
        raw_bytes: &[u8],
    ) -> Result<Self, XmtpError> {
        use xmtp_mls::messages::decoded_message::MessageBody as CoreBody;
        match body {
            CoreBody::Text(value) => Ok(Self::Text(value.content)),
            CoreBody::Markdown(value) => Ok(Self::Markdown(value.content)),
            CoreBody::ReadReceipt(_) => Ok(Self::ReadReceipt),
            CoreBody::Reaction(value) => Ok(Self::Reaction(crate::Reaction::from_proto(value))),
            CoreBody::Reply(value) => Ok(Self::Reply {
                reference_id: MessageID::try_from(value.reference_id)?,
                body: MessageBody::from_core(
                    *value.content,
                    nested_reply_content(content.into())?,
                )?,
            }),
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
                encoded: value.into(),
                raw_bytes: raw_bytes.to_vec(),
            }),
            _ => Ok(Self::Unknown {
                encoded: content.into(),
                raw_bytes: raw_bytes.to_vec(),
            }),
        }
    }
}

impl MessageBody {
    fn from_core(
        body: xmtp_mls::messages::decoded_message::MessageBody,
        encoded: SdkEncodedContent,
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
            CoreBody::Custom(value) => Self::Custom {
                encoded: value.into(),
            },
            _ => Self::Unknown { encoded },
        })
    }
}

fn nested_reply_content(encoded: SdkEncodedContent) -> Result<SdkEncodedContent, XmtpError> {
    ReplyCodec::decode(encoded.into())
        .map(|reply| reply.content.into())
        .map_err(XmtpError::unknown)
}

impl TryFrom<xmtp_mls::messages::decoded_message::DeletedBy> for crate::DeletedBy {
    type Error = XmtpError;
    fn try_from(
        value: xmtp_mls::messages::decoded_message::DeletedBy,
    ) -> Result<Self, Self::Error> {
        Ok(match value {
            xmtp_mls::messages::decoded_message::DeletedBy::Sender => Self::Sender,
            xmtp_mls::messages::decoded_message::DeletedBy::Admin(inbox_id) => Self::Admin {
                inbox_id: InboxID::try_from(inbox_id)?,
            },
        })
    }
}

/// A message value contains records and enums only.
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageData {
    pub id: MessageID,
    pub client_key: u64,
    pub conversation_id: ConversationID,
    pub topic: String,
    pub sender_inbox_id: InboxID,
    pub sent_at: Timestamp,
    pub inserted_at: Timestamp,
    pub expires_at: Option<Timestamp>,
    pub kind: MessageKind,
    pub delivery_status: DeliveryStatus,
    pub content_type: ContentTypeId,
    pub fallback: Option<String>,
    pub encoded: SdkEncodedContent,
    pub content: MessageContent,
    pub reply_count: u64,
    pub reactions: Vec<ReactionMessage>,
    pub in_reply_to: Option<ReplyParent>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReactionMessage {
    pub id: MessageID,
    pub sender_inbox_id: InboxID,
    pub sent_at: Timestamp,
    pub delivery_status: DeliveryStatus,
    pub reaction: crate::Reaction,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReplyParent {
    pub id: MessageID,
    pub sender_inbox_id: InboxID,
    pub sent_at: Timestamp,
    pub kind: MessageKind,
    pub delivery_status: DeliveryStatus,
    pub content_type: ContentTypeId,
    pub fallback: Option<String>,
    pub encoded: SdkEncodedContent,
    pub content: MessageBody,
}

/// The host runtime lifts this value to a Message class.
#[derive(Clone, Debug)]
pub struct Message(pub MessageData);

uniffi::custom_newtype!(Message, MessageData);

impl Message {
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
        let encoded = EncodedContent::decode(value.decrypted_message_bytes.as_slice()).ok();
        let raw_fallback = encoded.is_none().then(|| SdkEncodedContent {
            r#type: ContentTypeId {
                authority_id: String::new(),
                type_id: String::new(),
                version_major: 0,
                version_minor: 0,
            },
            parameters: Default::default(),
            fallback: None,
            content: value.decrypted_message_bytes.clone(),
        });
        let content_type = encoded
            .as_ref()
            .and_then(|content| content.r#type.clone())
            .unwrap_or_default();
        let fallback = encoded
            .as_ref()
            .and_then(|content| content.fallback.clone());
        let content = encoded
            .clone()
            .map(|content| match decoded {
                Some(xmtp_mls::messages::decoded_message::MessageBody::Custom(
                    ref core_content,
                )) if content.r#type.is_none()
                    || core_decodes_standard(&content)
                    || core_content.compression.is_some() =>
                {
                    // Core also uses Custom when a standard decode fails.
                    MessageContent::decode_proto(content, &value.decrypted_message_bytes)
                }
                Some(body) => {
                    MessageContent::from_core(body, content, &value.decrypted_message_bytes)
                }
                None => MessageContent::decode_proto(content, &value.decrypted_message_bytes),
            })
            .transpose()
            .unwrap_or(None)
            .unwrap_or_else(|| MessageContent::Unknown {
                encoded: encoded
                    .clone()
                    .map(Into::into)
                    .or_else(|| raw_fallback.clone())
                    .expect("parsed or raw content"),
                raw_bytes: value.decrypted_message_bytes.clone(),
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
            id: MessageID::from_bytes(&value.id)?,
            client_key,
            conversation_id: value.group_id.into(),
            topic: xmtp_proto::types::Topic::new_group_message(value.group_id).to_string(),
            sender_inbox_id: InboxID::try_from(value.sender_inbox_id)?,
            sent_at: Timestamp(value.sent_at_ns),
            inserted_at: Timestamp(value.inserted_at_ns),
            expires_at: value.expire_at_ns.map(Timestamp),
            kind,
            delivery_status,
            content_type: ContentTypeId {
                authority_id: content_type.authority_id,
                type_id: content_type.type_id,
                version_major: content_type.version_major,
                version_minor: content_type.version_minor,
            },
            fallback,
            encoded: encoded
                .map(Into::into)
                .or(raw_fallback)
                .expect("parsed or raw content"),
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
                Some(ReactionMessage {
                    id: MessageID::from_bytes(&reaction.metadata.id).ok()?,
                    sender_inbox_id: InboxID::try_from(reaction.metadata.sender_inbox_id).ok()?,
                    sent_at: Timestamp(reaction.metadata.sent_at_ns),
                    delivery_status: reaction.metadata.delivery_status.into(),
                    reaction: crate::Reaction::from_proto(value),
                })
            })
            .collect();
        if let CoreBody::Reply(reply) = &enriched.content
            && let Some(parent) = &reply.in_reply_to
        {
            let parent = parent.as_ref();
            if let Some(parent_data) = parent_stored.and_then(|parent_stored| {
                Self::from_stored_with_content(
                    parent_stored,
                    client_key,
                    Some(parent.content.clone()),
                )
                .ok()
                .map(|message| message.0)
            }) {
                let parent_content =
                    MessageBody::from_core(parent.content.clone(), parent_data.encoded.clone())
                        .unwrap_or_else(|_| MessageBody::Unknown {
                            encoded: parent_data.encoded.clone(),
                        });
                message.0.in_reply_to = Some(ReplyParent {
                    id: parent_data.id,
                    sender_inbox_id: parent_data.sender_inbox_id,
                    sent_at: parent_data.sent_at,
                    kind: parent_data.kind,
                    delivery_status: parent_data.delivery_status,
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
