//! Durable notification configuration and confirmed subscription uploads.

use super::{
    ConnectionExt, DbConnection,
    schema::{groups, push_uploaded_topic, user_preferences},
};
use crate::{StorageError, impl_fetch, impl_store};
use diesel::prelude::*;
use xmtp_proto::types::GroupId;

/// Notification fields on the singleton preferences row.
/// Keep credentials out of debug output.
#[derive(Clone, Default, Queryable, Selectable, AsChangeset)]
#[diesel(table_name = user_preferences, treat_none_as_null = true)]
pub struct StoredNotification {
    pub push_recipient_id: Option<Vec<u8>>,
    pub push_recipient_secret: Option<Vec<u8>>,
    pub push_state: i32,
    pub push_failed_error: Option<Vec<u8>>,
    pub push_config: Option<Vec<u8>>,
    pub push_deadlines: Option<Vec<u8>>,
    pub push_last_state: Option<Vec<u8>>,
    pub push_repairing: bool,
    pub push_generation: i64,
    pub push_suppressed: Option<Vec<u8>>,
}

/// One confirmed topic upload. Stale rows belong to an active repair pass.
#[derive(Clone, Debug, PartialEq, Eq, Queryable, Selectable, Insertable, Identifiable)]
#[diesel(table_name = push_uploaded_topic, primary_key(topic))]
pub struct UploadedTopic {
    pub topic: Vec<u8>,
    pub hmac_epoch_base: Option<i64>,
    pub include_commits: bool,
    pub root_key_fingerprint: Vec<u8>,
    pub stale: bool,
}

impl_fetch!(UploadedTopic, push_uploaded_topic, Vec<u8>);
impl_store!(UploadedTopic, push_uploaded_topic);

pub trait QueryNotifications {
    fn notification_record(&self) -> Result<StoredNotification, StorageError>;
    fn save_notification_record(&self, record: &StoredNotification) -> Result<(), StorageError>;
    /// Atomically disable notifications and return the cleared uploads.
    /// Keep the recipient identity, conversation overrides, and task retry state.
    fn disable_notifications(
        &self,
    ) -> Result<(StoredNotification, Vec<UploadedTopic>), StorageError>;
    fn uploaded_topics(&self) -> Result<Vec<UploadedTopic>, StorageError>;
    fn confirm_uploaded_topics(
        &self,
        adds: &[UploadedTopic],
        removes: &[Vec<u8>],
    ) -> Result<(), StorageError>;
    fn clear_uploaded_topics(&self) -> Result<(), StorageError>;
    fn mark_uploaded_topics_stale(&self) -> Result<(), StorageError>;
    fn notification_groups(&self) -> Result<Vec<super::group::StoredGroup>, StorageError>;
    fn set_notification_override(
        &self,
        group: &GroupId,
        value: Option<i32>,
    ) -> Result<(), StorageError>;
}

impl<T: QueryNotifications + ?Sized> QueryNotifications for &T {
    fn notification_record(&self) -> Result<StoredNotification, StorageError> {
        (**self).notification_record()
    }
    fn save_notification_record(&self, record: &StoredNotification) -> Result<(), StorageError> {
        (**self).save_notification_record(record)
    }
    fn uploaded_topics(&self) -> Result<Vec<UploadedTopic>, StorageError> {
        (**self).uploaded_topics()
    }
    fn disable_notifications(
        &self,
    ) -> Result<(StoredNotification, Vec<UploadedTopic>), StorageError> {
        (**self).disable_notifications()
    }
    fn confirm_uploaded_topics(
        &self,
        adds: &[UploadedTopic],
        removes: &[Vec<u8>],
    ) -> Result<(), StorageError> {
        (**self).confirm_uploaded_topics(adds, removes)
    }
    fn clear_uploaded_topics(&self) -> Result<(), StorageError> {
        (**self).clear_uploaded_topics()
    }
    fn mark_uploaded_topics_stale(&self) -> Result<(), StorageError> {
        (**self).mark_uploaded_topics_stale()
    }
    fn notification_groups(&self) -> Result<Vec<super::group::StoredGroup>, StorageError> {
        (**self).notification_groups()
    }
    fn set_notification_override(
        &self,
        group: &GroupId,
        value: Option<i32>,
    ) -> Result<(), StorageError> {
        (**self).set_notification_override(group, value)
    }
}

impl<C: ConnectionExt> QueryNotifications for DbConnection<C> {
    #[xmtp_common::db_span]
    fn notification_record(&self) -> Result<StoredNotification, StorageError> {
        Ok(self.raw_query(|conn| {
            user_preferences::table
                .select(StoredNotification::as_select())
                .first(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn save_notification_record(&self, record: &StoredNotification) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(user_preferences::table)
                .set(record)
                .execute(conn)
        })?;
        Ok(())
    }

    #[xmtp_common::db_span]
    fn uploaded_topics(&self) -> Result<Vec<UploadedTopic>, StorageError> {
        Ok(self.raw_query(|conn| {
            push_uploaded_topic::table
                .order(push_uploaded_topic::topic.asc())
                .load(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn disable_notifications(
        &self,
    ) -> Result<(StoredNotification, Vec<UploadedTopic>), StorageError> {
        self.raw_query(|conn| {
            Ok(conn.transaction::<_, StorageError, _>(|conn| {
                let mut record = user_preferences::table
                    .select(StoredNotification::as_select())
                    .first(conn)?;
                record.push_generation = record
                    .push_generation
                    .checked_add(1)
                    .ok_or(StorageError::DbSerialize)?;
                record.push_state = 0;
                record.push_config = None;
                record.push_failed_error = None;
                record.push_deadlines = None;
                record.push_last_state = None;
                record.push_repairing = false;
                record.push_suppressed = None;
                let cleared = push_uploaded_topic::table
                    .order(push_uploaded_topic::topic.asc())
                    .load(conn)?;
                diesel::delete(push_uploaded_topic::table).execute(conn)?;
                diesel::update(user_preferences::table)
                    .set(&record)
                    .execute(conn)?;
                Ok((record, cleared))
            }))
        })?
    }

    #[xmtp_common::db_span]
    fn confirm_uploaded_topics(
        &self,
        adds: &[UploadedTopic],
        removes: &[Vec<u8>],
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::delete(
                push_uploaded_topic::table.filter(push_uploaded_topic::topic.eq_any(removes)),
            )
            .execute(conn)?;
            for row in adds {
                diesel::replace_into(push_uploaded_topic::table)
                    .values(row)
                    .execute(conn)?;
            }
            Ok(())
        })?;
        Ok(())
    }

    #[xmtp_common::db_span]
    fn clear_uploaded_topics(&self) -> Result<(), StorageError> {
        self.raw_query(|conn| diesel::delete(push_uploaded_topic::table).execute(conn))?;
        Ok(())
    }

    #[xmtp_common::db_span]
    fn mark_uploaded_topics_stale(&self) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(push_uploaded_topic::table)
                .set(push_uploaded_topic::stale.eq(true))
                .execute(conn)
        })?;
        Ok(())
    }

    #[xmtp_common::db_span]
    fn notification_groups(&self) -> Result<Vec<super::group::StoredGroup>, StorageError> {
        Ok(self.raw_query(|conn| {
            groups::table
                .select(super::group::StoredGroup::as_select())
                .order(groups::id.asc())
                .load(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn set_notification_override(
        &self,
        group: &GroupId,
        value: Option<i32>,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(groups::table.find(group))
                .set(groups::push_override.eq(value))
                .execute(conn)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, TestDb, XmtpTestDb, group::tests::generate_group};
    use diesel::connection::SimpleConnection;

    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_disable_clears_state_and_keeps_identity_and_overrides() {
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let group = generate_group(None);
        group.store(&db)?;
        db.set_notification_override(&group.id, Some(0))?;
        let before = StoredNotification {
            push_recipient_id: Some(vec![1; 32]),
            push_recipient_secret: Some(vec![2; 32]),
            push_state: 2,
            push_failed_error: Some(vec![3]),
            push_config: Some(vec![4]),
            push_deadlines: Some(vec![5]),
            push_last_state: Some(vec![6]),
            push_repairing: true,
            push_generation: 9,
            push_suppressed: Some(vec![7]),
        };
        db.save_notification_record(&before)?;
        let uploaded = UploadedTopic {
            topic: vec![8],
            hmac_epoch_base: Some(42),
            include_commits: true,
            root_key_fingerprint: vec![9],
            stale: true,
        };
        db.confirm_uploaded_topics(std::slice::from_ref(&uploaded), &[])?;
        let (disabled, cleared) = db.disable_notifications()?;
        assert_eq!(cleared, vec![uploaded]);
        assert!(db.uploaded_topics()?.is_empty());
        assert_eq!(disabled.push_generation, 10);
        let disabled = db.notification_record()?;
        assert_eq!(disabled.push_generation, 10);
        assert_eq!(disabled.push_state, 0);
        assert_eq!(disabled.push_recipient_id, before.push_recipient_id);
        assert_eq!(disabled.push_recipient_secret, before.push_recipient_secret);
        assert!(disabled.push_config.is_none());
        assert!(disabled.push_failed_error.is_none());
        assert!(disabled.push_deadlines.is_none());
        assert!(disabled.push_last_state.is_none());
        assert!(disabled.push_suppressed.is_none());
        assert!(!disabled.push_repairing);
        assert_eq!(db.notification_groups()?[0].push_override, Some(0));
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_disable_rolls_back_cleared_uploads_if_state_write_fails() {
        let store = TestDb::create_persistent_store(None).await;
        let db = store.db();
        let before = StoredNotification {
            push_state: 1,
            push_config: Some(vec![1]),
            push_generation: 9,
            ..Default::default()
        };
        db.save_notification_record(&before)?;
        let uploaded = UploadedTopic {
            topic: vec![8],
            hmac_epoch_base: None,
            include_commits: false,
            root_key_fingerprint: vec![9],
            stale: false,
        };
        db.confirm_uploaded_topics(std::slice::from_ref(&uploaded), &[])?;
        db.raw_query(|conn| {
            conn.batch_execute(
                "CREATE TRIGGER reject_notification_disable BEFORE UPDATE ON user_preferences
                 BEGIN SELECT RAISE(ABORT, 'test state write failure'); END;",
            )
        })?;
        assert!(db.disable_notifications().is_err());
        assert_eq!(db.uploaded_topics()?, vec![uploaded]);
        let after = db.notification_record()?;
        assert_eq!(after.push_state, before.push_state);
        assert_eq!(after.push_config, before.push_config);
        assert_eq!(after.push_generation, before.push_generation);
    }
}
