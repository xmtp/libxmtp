//! Per-conversation notification rules.

use super::MlsGroup;
use crate::context::XmtpSharedContext;
use crate::core::notifications::{
    NotificationOverride, enabled_from_record, notification_settings_changed,
};
use xmtp_db::consent_record::conversation_consent;
use xmtp_db::{StorageError, TransactionOutcome::Continue, XmtpMlsStorageProvider, prelude::*};

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Set an override after which the task recomputes the desired set.
    pub fn set_notifications(&self, value: NotificationOverride) -> Result<(), StorageError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            tx.storage().db().set_notification_override(
                &self.group_id,
                match value {
                    NotificationOverride::Enabled => Some(1),
                    NotificationOverride::Disabled => Some(0),
                    NotificationOverride::Default => None,
                },
            )?;
            Ok::<_, StorageError>(Continue(()))
        })?;
        notification_settings_changed(&self.context);
        Ok(())
    }

    /// Return the effective local rule for this conversation.
    pub fn notifications_enabled(&self) -> Result<bool, StorageError> {
        let db = self.context.db();
        let record = db.notification_record()?;
        if record.push_state != 1 {
            return Ok(false);
        }
        let group = db
            .find_group(&self.group_id)?
            .ok_or(xmtp_db::NotFound::GroupById(self.group_id))?;
        let consent = conversation_consent(&db, self.context.inbox_id(), &self.group_id)?;
        enabled_from_record(&record, &group, consent)
    }
}
