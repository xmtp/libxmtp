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
