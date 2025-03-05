use super::*;
use crate::storage::{
    group::ConversationType,
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    schema::{group_messages, groups},
};
use diesel::prelude::*;
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::device_sync::{
    backup_element::Element,
    message_backup::{ContentTypeSave, DeliveryStatusSave, GroupMessageKindSave, GroupMessageSave},
};

impl BackupRecordProvider for GroupMessageSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let mut query = group_messages::table
            .left_join(groups::table)
            .filter(groups::conversation_type.ne(ConversationType::Sync))
            .filter(group_messages::kind.eq(GroupMessageKind::Application))
            .select(group_messages::all_columns)
            .order_by(group_messages::id)
            .into_boxed();

        if let Some(start_ns) = streamer.start_ns {
            query = query.filter(group_messages::sent_at_ns.gt(start_ns));
        }
        if let Some(end_ns) = streamer.end_ns {
            query = query.filter(group_messages::sent_at_ns.le(end_ns));
        }

        query = query.limit(Self::BATCH_SIZE).offset(streamer.offset);

        let batch = streamer
            .provider
            .conn_ref()
            .raw_query_read(|conn| query.load::<StoredGroupMessage>(conn))
            .expect("Failed to load group records");

        batch
            .into_iter()
            .map(|record| BackupElement {
                element: Some(Element::GroupMessage(record.into())),
            })
            .collect()
    }
}

impl TryFrom<GroupMessageSave> for StoredGroupMessage {
    type Error = DeserializationError;
    fn try_from(value: GroupMessageSave) -> Result<Self, Self::Error> {
        let kind = value.kind().try_into()?;
        let delivery_status = value.delivery_status().try_into()?;
        let content_type = value.content_type().try_into()?;

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
            should_push: false,
        })
    }
}
impl TryFrom<GroupMessageKindSave> for GroupMessageKind {
    type Error = DeserializationError;
    fn try_from(value: GroupMessageKindSave) -> Result<Self, Self::Error> {
        let message_kind = match value {
            GroupMessageKindSave::Application => Self::Application,
            GroupMessageKindSave::MembershipChange => Self::MembershipChange,
            GroupMessageKindSave::Unspecified => {
                return Err(DeserializationError::Unspecified("message_kind"))
            }
        };
        Ok(message_kind)
    }
}
impl TryFrom<DeliveryStatusSave> for DeliveryStatus {
    type Error = DeserializationError;
    fn try_from(value: DeliveryStatusSave) -> Result<Self, Self::Error> {
        let delivery_status = match value {
            DeliveryStatusSave::Failed => Self::Failed,
            DeliveryStatusSave::Published => Self::Published,
            DeliveryStatusSave::Unpublished => Self::Unpublished,
            DeliveryStatusSave::Unspecified => {
                return Err(DeserializationError::Unspecified("delivery_status"))
            }
        };
        Ok(delivery_status)
    }
}
impl TryFrom<ContentTypeSave> for ContentType {
    type Error = DeserializationError;
    fn try_from(value: ContentTypeSave) -> Result<Self, Self::Error> {
        let content_type = match value {
            ContentTypeSave::Attachment => Self::Attachment,
            ContentTypeSave::GroupMembershipChange => Self::GroupMembershipChange,
            ContentTypeSave::GroupUpdated => Self::GroupUpdated,
            ContentTypeSave::Reaction => Self::Reaction,
            ContentTypeSave::ReadReceipt => Self::ReadReceipt,
            ContentTypeSave::RemoteAttachment => Self::RemoteAttachment,
            ContentTypeSave::Reply => Self::Reply,
            ContentTypeSave::Text => Self::Text,
            ContentTypeSave::TransactionReference => Self::TransactionReference,
            ContentTypeSave::Unknown => Self::Unknown,
            ContentTypeSave::Unspecified => {
                return Err(DeserializationError::Unspecified("content_type"))
            }
        };
        Ok(content_type)
    }
}

impl From<StoredGroupMessage> for GroupMessageSave {
    fn from(value: StoredGroupMessage) -> Self {
        let kind: GroupMessageKindSave = value.kind.into();
        let delivery_status: DeliveryStatusSave = value.delivery_status.into();
        let content_type: ContentTypeSave = value.content_type.into();

        Self {
            id: value.id,
            group_id: value.group_id,
            decrypted_message_bytes: value.decrypted_message_bytes,
            sent_at_ns: value.sent_at_ns,
            kind: kind as i32,
            sender_installation_id: value.sender_installation_id,
            sender_inbox_id: value.sender_inbox_id,
            delivery_status: delivery_status as i32,
            content_type: content_type as i32,
            version_major: value.version_major,
            version_minor: value.version_minor,
            authority_id: value.authority_id,
            reference_id: value.reference_id,
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
impl From<ContentType> for ContentTypeSave {
    fn from(value: ContentType) -> Self {
        match value {
            ContentType::Attachment => Self::Attachment,
            ContentType::GroupMembershipChange => Self::GroupMembershipChange,
            ContentType::GroupUpdated => Self::GroupUpdated,
            ContentType::Reaction => Self::Reaction,
            ContentType::ReadReceipt => Self::ReadReceipt,
            ContentType::RemoteAttachment => Self::RemoteAttachment,
            ContentType::Reply => Self::Reply,
            ContentType::Text => Self::Text,
            ContentType::TransactionReference => Self::TransactionReference,
            ContentType::Unknown => Self::Unknown,
        }
    }
}
