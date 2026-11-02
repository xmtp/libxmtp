use super::*;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::message_backup::MessageDeletionSave;

/// An archived record is never authenticated: only the topic proves a sender.
impl TryFrom<MessageDeletionSave> for StoredMessageDeletion {
    type Error = ConversionError;
    fn try_from(value: MessageDeletionSave) -> Result<Self, Self::Error> {
        if value.id.is_empty() {
            return Err(ConversionError::Missing {
                item: "id",
                r#type: std::any::type_name::<MessageDeletionSave>(),
            });
        }
        if value.deleted_message_id.is_empty() {
            return Err(ConversionError::Missing {
                item: "deleted_message_id",
                r#type: std::any::type_name::<MessageDeletionSave>(),
            });
        }
        Ok(Self {
            id: value.id,
            group_id: value.group_id.try_into()?,
            deleted_message_id: value.deleted_message_id,
            deleted_by_inbox_id: value.deleted_by_inbox_id,
            is_super_admin_deletion: value.is_super_admin_deletion,
            deleted_at_ns: value.deleted_at_ns,
            authenticated: false,
        })
    }
}

impl From<StoredMessageDeletion> for MessageDeletionSave {
    fn from(value: StoredMessageDeletion) -> Self {
        Self {
            id: value.id,
            group_id: value.group_id.into(),
            deleted_message_id: value.deleted_message_id,
            deleted_by_inbox_id: value.deleted_by_inbox_id,
            is_super_admin_deletion: value.is_super_admin_deletion,
            deleted_at_ns: value.deleted_at_ns,
        }
    }
}
