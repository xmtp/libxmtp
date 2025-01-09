use super::*;
use crate::storage::{
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    schema::group_messages,
};
use diesel::prelude::*;
use xmtp_proto::xmtp::device_sync::message_backup::{
    ContentTypeSave, DeliveryStatusSave, GroupMessageKindSave, GroupMessageSave,
};

impl BackupRecordProvider for GroupMessageSave {
    const BATCH_SIZE: i64 = 100;
    fn backup_records(streamer: &BackupRecordStreamer<Self>) -> Vec<BackupElement>
    where
        Self: Sized,
    {
        let mut query = group_messages::table
            .order_by(group_messages::id)
            .into_boxed();

        if let Some(start_ns) = streamer.start_ns {
            query = query.filter(group_messages::sent_at_ns.gt(start_ns as i64));
        }
        if let Some(end_ns) = streamer.end_ns {
            query = query.filter(group_messages::sent_at_ns.le(end_ns as i64));
        }

        query = query.limit(BATCH_SIZE).offset(streamer.offset);

        let batch = streamer
            .conn
            .raw_query(|conn| query.load::<StoredGroupMessage>(conn))
            .expect("Failed to load group records");

        batch
            .into_iter()
            .map(|record| BackupElement::Message(record.into()))
            .collect()
    }
}

impl From<GroupMessageSave> for StoredGroupMessage {
    fn from(value: GroupMessageSave) -> Self {
        let kind = value.kind().into();
        let delivery_status = value.delivery_status().into();
        let content_type = value.content_type().into();

        Self {
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
        }
    }
}
impl From<GroupMessageKindSave> for GroupMessageKind {
    fn from(value: GroupMessageKindSave) -> Self {
        match value {
            GroupMessageKindSave::Application => Self::Application,
            GroupMessageKindSave::MembershipChange => Self::MembershipChange,
        }
    }
}
impl From<DeliveryStatusSave> for DeliveryStatus {
    fn from(value: DeliveryStatusSave) -> Self {
        match value {
            DeliveryStatusSave::Failed => Self::Failed,
            DeliveryStatusSave::Published => Self::Published,
            DeliveryStatusSave::Unpublished => Self::Unpublished,
        }
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
            ContentTypeSave::Unknown => Self::Unknown,
        }
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
