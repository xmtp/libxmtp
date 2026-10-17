use super::*;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::message_backup::{
    ContentTypeSave, DeliveryStatusSave, GroupMessageKindSave, GroupMessageSave,
};

impl TryFrom<GroupMessageSave> for StoredGroupMessage {
    type Error = ConversionError;
    fn try_from(value: GroupMessageSave) -> Result<Self, Self::Error> {
        let kind = value.kind().try_into()?;
        let delivery_status = value.delivery_status().try_into()?;

        let legacy_type: ContentType = value.content_type_save().into();
        let type_id = if legacy_type == ContentType::Unknown {
            value.content_type.clone()
        } else {
            legacy_type.to_string()
        };
        let content_type = ContentType::from_identifier(
            &value.authority_id,
            &type_id,
            u32::try_from(value.version_major).unwrap_or_default(),
        );

        Ok(Self {
            id: value.id,
            group_id: value.group_id.try_into()?,
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
            envelope_hash: None,
            expiry_ns: None,
            expire_at_ns: None,
            inserted_at_ns: 0,  // Will be set by database
            should_push: false, // Default to false for synced messages
            // GroupMessageSave does not carry the idempotency key; fall back to
            // the historical default (the send timestamp). Restored messages are
            // already published, so this value is only informational.
            idempotency_key: value.sent_at_ns.to_string(),
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
            group_id: value.group_id.into(),
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

            // Deprecated
            #[allow(deprecated)]
            content_type_save: 0,
            ..Default::default()
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

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: CTYPE-001, CTYPE-018
    #[allow(deprecated)]
    #[xmtp_common::test(unwrap_try = true)]
    fn saved_content_type_requires_catalogue_authority_and_major() {
        let base = GroupMessageSave {
            group_id: vec![1; 16],
            kind: GroupMessageKindSave::Application as i32,
            delivery_status: DeliveryStatusSave::Published as i32,
            content_type: "text".into(),
            authority_id: "xmtp.org".into(),
            version_major: 1,
            ..Default::default()
        };

        for old_type in [0, ContentTypeSave::Text as i32] {
            let mut standard = base.clone();
            standard.content_type_save = old_type;
            assert_eq!(
                StoredGroupMessage::try_from(standard.clone())?.content_type,
                ContentType::Text
            );

            standard.authority_id = "custom.example".into();
            assert_eq!(
                StoredGroupMessage::try_from(standard)?.content_type,
                ContentType::Unknown
            );
        }

        let mut legacy = base.clone();
        legacy.content_type.clear();
        legacy.content_type_save = ContentTypeSave::Text as i32;
        assert_eq!(
            StoredGroupMessage::try_from(legacy)?.content_type,
            ContentType::Text
        );

        let mut unsupported = base;
        unsupported.version_major = 99;
        assert_eq!(
            StoredGroupMessage::try_from(unsupported)?.content_type,
            ContentType::Unknown
        );
    }
}
