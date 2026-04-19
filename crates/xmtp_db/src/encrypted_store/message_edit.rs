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
    /// At most one edit per target message; the latest is selected by
    /// `edited_at_ns` descending, with ties broken by `id` ascending.
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

    /// Overwrite the `edited_at_ns` of an existing edit row.
    ///
    /// Used when a client's own optimistically-stored edit round-trips back
    /// through sync carrying the server-assigned envelope timestamp. Bringing
    /// the local row in line with the server time lets every installation
    /// compute the same "latest edit" winner on cross-device edits.
    fn set_edit_timestamp(
        &self,
        id: &[u8],
        edited_at_ns: i64,
    ) -> Result<(), crate::ConnectionError>;
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

    fn set_edit_timestamp(
        &self,
        id: &[u8],
        edited_at_ns: i64,
    ) -> Result<(), crate::ConnectionError> {
        (**self).set_edit_timestamp(id, edited_at_ns)
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
        message_ids: Vec<Vec<u8>>,
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        if message_ids.is_empty() {
            return Ok(vec![]);
        }
        self.raw_query_read(|conn| {
            // Pull all edits for the target IDs, then keep the latest per target:
            // greatest `edited_at_ns`, tie-break by smallest `id`.
            let mut all: Vec<StoredMessageEdit> = dsl::message_edits
                .filter(dsl::edited_message_id.eq_any(&message_ids))
                .load(conn)?;
            all.sort_by(|a, b| {
                b.edited_at_ns
                    .cmp(&a.edited_at_ns)
                    .then_with(|| a.id.cmp(&b.id))
            });
            let mut by_target: std::collections::HashMap<Vec<u8>, StoredMessageEdit> =
                std::collections::HashMap::new();
            for edit in all.into_iter() {
                by_target
                    .entry(edit.edited_message_id.clone())
                    .or_insert(edit);
            }
            Ok(by_target.into_values().collect())
        })
    }

    fn set_edit_timestamp(
        &self,
        id: &[u8],
        edited_at_ns: i64,
    ) -> Result<(), crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::message_edits.filter(dsl::id.eq(id)))
                .set(dsl::edited_at_ns.eq(edited_at_ns))
                .execute(conn)
                .map(|_| ())
        })
    }

    fn get_group_edits(
        &self,
        group_id: &[u8],
    ) -> Result<Vec<StoredMessageEdit>, crate::ConnectionError> {
        self.raw_query_read(|conn| {
            dsl::message_edits
                .filter(dsl::group_id.eq(group_id))
                .load(conn)
        })
    }
}
