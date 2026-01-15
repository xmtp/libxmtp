use super::*;
use xmtp_configuration::Originators;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::message_backup::{
    ContentTypeSave, DeliveryStatusSave, GroupMessageKindSave, GroupMessageSave,
};

impl TryFrom<GroupMessageSave> for StoredGroupMessage {
    type Error = ConversionError;
    fn try_from(value: GroupMessageSave) -> Result<Self, Self::Error> {
        let kind = value.kind().try_into()?;
        let delivery_status = value.delivery_status().try_into()?;

        let mut content_type: ContentType = value.content_type_save().into();
        if matches!(content_type, ContentType::Unknown) {
            content_type = value.content_type.into();
        }

        Ok(Self {
            id: value.id,
            group_id: value.group_id,
            decrypted_message_bytes: value.decrypted_message_bytes,
            sent_at_ns: value.sent_at_ns,
            kind,
            sender_installation_id: value.sender_installation_id,
            sender_inbox_id: value.sender_inbox_id,
            delivery_status,
            content_type,
            version_major: value.version_major,
            version_minor: value.version_minor,
            authority_id: value.authority_id,
            reference_id: value.reference_id,
            sequence_id: value.sequence_id.unwrap_or(0),
            originator_id: value
                .originator_id
                .unwrap_or(Originators::APPLICATION_MESSAGES.into()),
            expire_at_ns: None,
            inserted_at_ns: 0,  // Will be set by database
            should_push: false, // Default to false for synced messages
        })
    }
}

impl TryFrom<GroupMessageKindSave> for GroupMessageKind {
    type Error = ConversionError;
    fn try_from(value: GroupMessageKindSave) -> Result<Self, Self::Error> {
        let message_kind = match value {
            GroupMessageKindSave::Application => Self::Application,
            GroupMessageKindSave::MembershipChange => Self::MembershipChange,
            GroupMessageKindSave::Unspecified => {
                return Err(ConversionError::Unspecified("message_kind"));
            }
        };
        Ok(message_kind)
    }
}

impl TryFrom<DeliveryStatusSave> for DeliveryStatus {
    type Error = ConversionError;
    fn try_from(value: DeliveryStatusSave) -> Result<Self, Self::Error> {
        let delivery_status = match value {
            DeliveryStatusSave::Failed => Self::Failed,
            DeliveryStatusSave::Published => Self::Published,
            DeliveryStatusSave::Unpublished => Self::Unpublished,
            DeliveryStatusSave::Unspecified => {
                return Err(ConversionError::Unspecified("delivery_status"));
            }
        };
        Ok(delivery_status)
    }
}

impl From<ContentTypeSave> for ContentType {
    fn from(value: ContentTypeSave) -> Self {
        match value {
            ContentTypeSave::Attachment => Self::Attachment,
            ContentTypeSave::GroupMembershipChange => Self::GroupMembershipChange,
            ContentTypeSave::GroupUpdated => Self::GroupUpdated,
            ContentTypeSave::Reaction => Self::Reaction,
            ContentTypeSave::ReadReceipt => Self::ReadReceipt,
            ContentTypeSave::RemoteAttachment => Self::RemoteAttachment,
            ContentTypeSave::Reply => Self::Reply,
            ContentTypeSave::Text => Self::Text,
            ContentTypeSave::TransactionReference => Self::TransactionReference,
            _ => Self::Unknown,
        }
    }
}

impl From<StoredGroupMessage> for GroupMessageSave {
    fn from(value: StoredGroupMessage) -> Self {
        let kind: GroupMessageKindSave = value.kind.into();
        let delivery_status: DeliveryStatusSave = value.delivery_status.into();

        Self {
            id: value.id,
            group_id: value.group_id,
            decrypted_message_bytes: value.decrypted_message_bytes,
            sent_at_ns: value.sent_at_ns,
            kind: kind as i32,
            sender_installation_id: value.sender_installation_id,
            sender_inbox_id: value.sender_inbox_id,
            delivery_status: delivery_status as i32,
            content_type: value.content_type.to_string(),
            version_major: value.version_major,
            version_minor: value.version_minor,
            authority_id: value.authority_id,
            reference_id: value.reference_id,
            sequence_id: Some(value.sequence_id),
            originator_id: Some(value.originator_id),

            // Deprecated
            #[allow(deprecated)]
            content_type_save: 0,
        }
    }
}
impl From<GroupMessageKind> for GroupMessageKindSave {
    fn from(value: GroupMessageKind) -> Self {
        match value {
            GroupMessageKind::Application => Self::Application,
            GroupMessageKind::MembershipChange => Self::MembershipChange,
        }
    }
}
impl From<DeliveryStatus> for DeliveryStatusSave {
    fn from(value: DeliveryStatus) -> Self {
        match value {
            DeliveryStatus::Failed => Self::Failed,
            DeliveryStatus::Published => Self::Published,
            DeliveryStatus::Unpublished => Self::Unpublished,
        }
    }
}
