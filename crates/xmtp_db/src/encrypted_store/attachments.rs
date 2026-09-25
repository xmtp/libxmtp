//! Records for local plaintext files and pending attachment uploads.

use super::{
    ConnectionExt, DbConnection,
    schema::{local_attachments, pending_attachments},
};
use crate::StorageError;
use diesel::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = local_attachments)]
pub struct StoredLocalAttachment {
    pub path: String,
    pub created_at_ns: i64,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = pending_attachments)]
pub struct StoredPendingAttachment {
    pub content_digest: String,
    /// Prost-encoded remote attachment.
    pub remote_attachment: Vec<u8>,
    pub created_at_ns: i64,
}

pub trait QueryLocalAttachment {
    fn insert_or_ignore_local_attachment(
        &self,
        path: &str,
        created_at_ns: i64,
        mime_type: Option<String>,
        filename: Option<String>,
    ) -> Result<(), StorageError>;
    fn get_local_attachment(
        &self,
        path: &str,
    ) -> Result<Option<StoredLocalAttachment>, StorageError>;
    fn delete_local_attachment(&self, path: &str) -> Result<usize, StorageError>;
    fn list_local_attachments(&self) -> Result<Vec<StoredLocalAttachment>, StorageError>;
}

impl<T: QueryLocalAttachment + ?Sized> QueryLocalAttachment for &T {
    fn insert_or_ignore_local_attachment(
        &self,
        path: &str,
        created_at_ns: i64,
        mime_type: Option<String>,
        filename: Option<String>,
    ) -> Result<(), StorageError> {
        (**self).insert_or_ignore_local_attachment(path, created_at_ns, mime_type, filename)
    }

    fn get_local_attachment(
        &self,
        path: &str,
    ) -> Result<Option<StoredLocalAttachment>, StorageError> {
        (**self).get_local_attachment(path)
    }

    fn delete_local_attachment(&self, path: &str) -> Result<usize, StorageError> {
        (**self).delete_local_attachment(path)
    }

    fn list_local_attachments(&self) -> Result<Vec<StoredLocalAttachment>, StorageError> {
        (**self).list_local_attachments()
    }
}

impl<C: ConnectionExt> QueryLocalAttachment for DbConnection<C> {
    #[xmtp_common::db_span]
    fn insert_or_ignore_local_attachment(
        &self,
        path: &str,
        created_at_ns: i64,
        mime_type: Option<String>,
        filename: Option<String>,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::insert_into(local_attachments::table)
                .values((
                    local_attachments::path.eq(path),
                    local_attachments::created_at_ns.eq(created_at_ns),
                    local_attachments::mime_type.eq(mime_type),
                    local_attachments::filename.eq(filename),
                ))
                .on_conflict(local_attachments::path)
                .do_nothing()
                .execute(conn)
        })?;
        Ok(())
    }

    #[xmtp_common::db_span]
    fn get_local_attachment(
        &self,
        path: &str,
    ) -> Result<Option<StoredLocalAttachment>, StorageError> {
        Ok(self.raw_query(|conn| {
            local_attachments::table
                .find(path)
                .select(StoredLocalAttachment::as_select())
                .first(conn)
                .optional()
        })?)
    }

    #[xmtp_common::db_span]
    fn delete_local_attachment(&self, path: &str) -> Result<usize, StorageError> {
        Ok(self
            .raw_query(|conn| diesel::delete(local_attachments::table.find(path)).execute(conn))?)
    }

    #[xmtp_common::db_span]
    fn list_local_attachments(&self) -> Result<Vec<StoredLocalAttachment>, StorageError> {
        Ok(self.raw_query(|conn| {
            local_attachments::table
                .order(local_attachments::created_at_ns.asc())
                .select(StoredLocalAttachment::as_select())
                .load(conn)
        })?)
    }
}

pub trait QueryPendingAttachment {
    fn insert_or_ignore_pending_attachment(
        &self,
        content_digest: &str,
        remote_attachment: &[u8],
        created_at_ns: i64,
    ) -> Result<(), StorageError>;
    fn delete_pending_attachment(&self, content_digest: &str) -> Result<usize, StorageError>;
    fn list_pending_attachments_since(
        &self,
        since_ns: i64,
    ) -> Result<Vec<StoredPendingAttachment>, StorageError>;
    /// Return digests to inspect before expiry. This query does not delete rows.
    fn pending_attachment_sweep_candidates(
        &self,
        older_than_ns: i64,
    ) -> Result<Vec<String>, StorageError>;
}

impl<T: QueryPendingAttachment + ?Sized> QueryPendingAttachment for &T {
    fn insert_or_ignore_pending_attachment(
        &self,
        content_digest: &str,
        remote_attachment: &[u8],
        created_at_ns: i64,
    ) -> Result<(), StorageError> {
        (**self).insert_or_ignore_pending_attachment(
            content_digest,
            remote_attachment,
            created_at_ns,
        )
    }

    fn delete_pending_attachment(&self, content_digest: &str) -> Result<usize, StorageError> {
        (**self).delete_pending_attachment(content_digest)
    }

    fn list_pending_attachments_since(
        &self,
        since_ns: i64,
    ) -> Result<Vec<StoredPendingAttachment>, StorageError> {
        (**self).list_pending_attachments_since(since_ns)
    }

    fn pending_attachment_sweep_candidates(
        &self,
        older_than_ns: i64,
    ) -> Result<Vec<String>, StorageError> {
        (**self).pending_attachment_sweep_candidates(older_than_ns)
    }
}

impl<C: ConnectionExt> QueryPendingAttachment for DbConnection<C> {
    #[xmtp_common::db_span]
    fn insert_or_ignore_pending_attachment(
        &self,
        content_digest: &str,
        remote_attachment: &[u8],
        created_at_ns: i64,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::insert_into(pending_attachments::table)
                .values((
                    pending_attachments::content_digest.eq(content_digest),
                    pending_attachments::remote_attachment.eq(remote_attachment),
                    pending_attachments::created_at_ns.eq(created_at_ns),
                ))
                .on_conflict(pending_attachments::content_digest)
                .do_nothing()
                .execute(conn)
        })?;
        Ok(())
    }

    #[xmtp_common::db_span]
    fn delete_pending_attachment(&self, content_digest: &str) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::delete(pending_attachments::table.find(content_digest)).execute(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn list_pending_attachments_since(
        &self,
        since_ns: i64,
    ) -> Result<Vec<StoredPendingAttachment>, StorageError> {
        Ok(self.raw_query(|conn| {
            pending_attachments::table
                .filter(pending_attachments::created_at_ns.ge(since_ns))
                .order(pending_attachments::created_at_ns.asc())
                .select(StoredPendingAttachment::as_select())
                .load(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn pending_attachment_sweep_candidates(
        &self,
        older_than_ns: i64,
    ) -> Result<Vec<String>, StorageError> {
        Ok(self.raw_query(|conn| {
            pending_attachments::table
                .filter(pending_attachments::created_at_ns.lt(older_than_ns))
                .select(pending_attachments::content_digest)
                .load(conn)
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TestDb, XmtpTestDb};

    #[xmtp_common::test(unwrap_try = true)]
    async fn insert_or_ignore_local_keeps_first_record() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_local_attachment(
            "key/file",
            3,
            Some("image/png".into()),
            Some("photo.png".into()),
        )?;
        db.insert_or_ignore_local_attachment("key/file", 9, Some("text/plain".into()), None)?;
        assert_eq!(
            db.list_local_attachments()?,
            vec![StoredLocalAttachment {
                path: "key/file".into(),
                created_at_ns: 3,
                mime_type: Some("image/png".into()),
                filename: Some("photo.png".into()),
            }]
        );
        assert_eq!(
            db.get_local_attachment("key/file")?,
            db.list_local_attachments()?.pop()
        );
        assert_eq!(db.get_local_attachment("missing")?, None);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn delete_local_removes_only_matching_path() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_local_attachment("a/file", 1, None, None)?;
        db.insert_or_ignore_local_attachment("b/file", 2, None, None)?;
        assert_eq!(db.delete_local_attachment("a/file")?, 1);
        assert_eq!(db.delete_local_attachment("a/file")?, 0);
        assert_eq!(db.list_local_attachments()?[0].path, "b/file");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn list_local_orders_by_creation_time() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_local_attachment("late", 9, None, None)?;
        db.insert_or_ignore_local_attachment("early", 1, None, None)?;
        let rows = db.list_local_attachments()?;
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            vec!["early", "late"]
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn insert_or_ignore_pending_keeps_first_record() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_pending_attachment("abc", &[1, 2], 3)?;
        db.insert_or_ignore_pending_attachment("abc", &[9], 9)?;
        assert_eq!(
            db.list_pending_attachments_since(0)?,
            vec![StoredPendingAttachment {
                content_digest: "abc".into(),
                remote_attachment: vec![1, 2],
                created_at_ns: 3,
            }]
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn delete_pending_removes_only_matching_digest() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_pending_attachment("a", &[1], 1)?;
        db.insert_or_ignore_pending_attachment("b", &[2], 2)?;
        assert_eq!(db.delete_pending_attachment("a")?, 1);
        assert_eq!(db.delete_pending_attachment("a")?, 0);
        assert_eq!(db.list_pending_attachments_since(0)?[0].content_digest, "b");
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn list_pending_since_includes_boundary_and_orders_by_creation_time() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_pending_attachment("late", &[3], 3)?;
        db.insert_or_ignore_pending_attachment("old", &[1], 1)?;
        db.insert_or_ignore_pending_attachment("boundary", &[2], 2)?;
        let rows = db.list_pending_attachments_since(2)?;
        assert_eq!(
            rows.iter()
                .map(|row| row.content_digest.as_str())
                .collect::<Vec<_>>(),
            vec!["boundary", "late"]
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn sweep_candidates_exclude_boundary_without_deleting_rows() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        db.insert_or_ignore_pending_attachment("old", &[1], 1)?;
        db.insert_or_ignore_pending_attachment("boundary", &[2], 2)?;
        assert_eq!(db.pending_attachment_sweep_candidates(2)?, vec!["old"]);
        assert_eq!(db.list_pending_attachments_since(0)?.len(), 2);
    }
}
