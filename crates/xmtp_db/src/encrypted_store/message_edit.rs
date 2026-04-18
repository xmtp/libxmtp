use super::ConnectionExt;
use crate::schema::message_edits::dsl;
use crate::{DbConnection, impl_store, impl_store_or_ignore, schema::message_edits};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    Insertable,
    Identifiable,
    Queryable,
    Eq,
    PartialEq,
    QueryableByName,
)]
#[diesel(table_name = message_edits)]
#[diesel(primary_key(id))]
/// Represents an edit record for a message in a group conversation.
///
/// An edit is authored by the original sender of the target message. Each record
/// corresponds to a single `EditMessage` payload stored in `group_messages` (via the
/// `id` FK). Multiple edits of the same target message produce multiple rows that
/// share the same `edited_message_id` but have distinct `id` values; the "current"
/// edit is the one with the latest `edited_at_ns` (ties broken by `id` ascending).
pub struct StoredMessageEdit {
    /// Primary key: the ID of the EditMessage in the `group_messages` table.
    pub id: Vec<u8>,
    /// The group this edit belongs to.
    pub group_id: Vec<u8>,
    /// The ID of the original message being edited.
    pub edited_message_id: Vec<u8>,
    /// The inbox_id of who sent the edit.
    pub edited_by_inbox_id: String,
    /// The replacement `EncodedContent` bytes.
    pub edited_content_bytes: Vec<u8>,
    /// Timestamp when the edit was processed.
    pub edited_at_ns: i64,
}

impl StoredMessageEdit {
    pub fn new(
        id: Vec<u8>,
        group_id: Vec<u8>,
        edited_message_id: Vec<u8>,
        edited_by_inbox_id: String,
        edited_content_bytes: Vec<u8>,
    ) -> Self {
        Self {
            id,
            group_id,
            edited_message_id,
            edited_by_inbox_id,
            edited_content_bytes,
            edited_at_ns: xmtp_common::time::now_ns(),
        }
    }
}

impl_store!(StoredMessageEdit, message_edits);
impl_store_or_ignore!(StoredMessageEdit, message_edits);

/// Trait for querying message edits.
pub trait QueryMessageEdit {
    /// Look up an edit record by the EditMessage's own ID (the row primary key).
    fn get_message_edit(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError>;

    /// Return the latest edit (by `edited_at_ns` desc, tie-break `id` asc) for the
    /// given target message, if any.
    fn get_latest_edit_by_message_id(
        &self,
        message_id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError>;

    /// Check whether any edit exists for the given target message.
    fn is_message_edited(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError>;

    /// Return the latest edit for each of the provided target message IDs.
    /// Stubbed for a future task — not implemented in this tracer-bullet slice.
    fn get_latest_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError>;

    /// Return all edits in a group.
    /// Stubbed for a future task — not implemented in this tracer-bullet slice.
    fn get_group_edits(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError>;
}

impl<T> QueryMessageEdit for &T
where
    T: QueryMessageEdit,
{
    fn get_message_edit(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_message_edit(id)
    }

    fn get_latest_edit_by_message_id(
        &self,
        message_id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_latest_edit_by_message_id(message_id)
    }

    fn is_message_edited(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        (**self).is_message_edited(message_id)
    }

    fn get_latest_edits_for_messages(
        &self,
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_latest_edits_for_messages(message_ids)
    }

    fn get_group_edits(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        (**self).get_group_edits(group_id)
    }
}

impl<C: ConnectionExt> QueryMessageEdit for DbConnection<C> {
    fn get_message_edit(
        &self,
        id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::id.eq(id))
                .first(conn)
                .optional()
        })
    }

    fn get_latest_edit_by_message_id(
        &self,
        message_id: &[u8],
    ) -> Result<Option<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::edited_message_id.eq(message_id))
                .order((dsl::edited_at_ns.desc(), dsl::id.asc()))
                .first(conn)
                .optional()
        })
    }

    fn is_message_edited(&self, message_id: &[u8]) -> Result<bool, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            diesel::dsl::select(diesel::dsl::exists(
                dsl::message_edits.filter(dsl::edited_message_id.eq(message_id)),
            ))
            .get_result::<bool>(conn)
        })
    }

    fn get_latest_edits_for_messages(
        &self,
        _message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        unimplemented!("get_latest_edits_for_messages is implemented in a later task")
    }

    fn get_group_edits(
        &self,
        _group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        unimplemented!("get_group_edits is implemented in a later task")
    }
}
