//! Records for local plaintext files and pending attachment uploads.

use super::{
    ConnectionExt, DbConnection,
    schema::{local_attachments, pending_attachments},
};
use crate::StorageError;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Binary, Bool, Nullable, Text};

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
    pub status: String,
    pub failure_cause: Option<String>,
    pub failure_credential_kind: Option<String>,
    pub failure_retryable: Option<bool>,
    pub lease_id: Option<Vec<u8>>,
    pub lease_expires_at_ns: Option<i64>,
}

impl StoredPendingAttachment {
    /// An expired upload has no outcome and reads as waiting.
    pub fn effective_status(&self, now_ns: i64) -> &str {
        if self.status == "uploading" && self.lease_expires_at_ns.is_none_or(|end| end < now_ns) {
            "waiting"
        } else {
            &self.status
        }
    }
}

/// The durable result of one upload attempt.
#[derive(Clone, Copy, Debug)]
pub enum PendingAttachmentOutcome {
    Complete,
    Failed {
        cause: &'static str,
        credential_kind: Option<&'static str>,
        retryable: Option<bool>,
    },
}

impl PendingAttachmentOutcome {
    pub const fn failed(
        cause: &'static str,
        credential_kind: Option<&'static str>,
        retryable: Option<bool>,
    ) -> Self {
        Self::Failed {
            cause,
            credential_kind,
            retryable,
        }
    }
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
    fn get_pending_attachment(
        &self,
        content_digest: &str,
    ) -> Result<Option<StoredPendingAttachment>, StorageError>;
    fn list_pending_attachments_since(
        &self,
        since_ns: i64,
    ) -> Result<Vec<StoredPendingAttachment>, StorageError>;
    /// Return digests to inspect before expiry. This query does not delete rows.
    fn pending_attachment_sweep_candidates(
        &self,
        older_than_ns: i64,
    ) -> Result<Vec<String>, StorageError>;
    fn claim_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        lease_duration_ns: i64,
    ) -> Result<usize, StorageError>;
    fn extend_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        lease_duration_ns: i64,
    ) -> Result<usize, StorageError>;
    fn finish_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        outcome: PendingAttachmentOutcome,
    ) -> Result<usize, StorageError>;
    fn sweep_pending_attachment(
        &self,
        content_digest: &str,
        older_than_ns: i64,
        now_ns: i64,
    ) -> Result<usize, StorageError>;
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

    fn get_pending_attachment(
        &self,
        content_digest: &str,
    ) -> Result<Option<StoredPendingAttachment>, StorageError> {
        (**self).get_pending_attachment(content_digest)
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

    fn claim_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        lease_duration_ns: i64,
    ) -> Result<usize, StorageError> {
        (**self).claim_pending_attachment(content_digest, lease_id, now_ns, lease_duration_ns)
    }

    fn extend_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        lease_duration_ns: i64,
    ) -> Result<usize, StorageError> {
        (**self).extend_pending_attachment(content_digest, lease_id, now_ns, lease_duration_ns)
    }

    fn finish_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        outcome: PendingAttachmentOutcome,
    ) -> Result<usize, StorageError> {
        (**self).finish_pending_attachment(content_digest, lease_id, now_ns, outcome)
    }

    fn sweep_pending_attachment(
        &self,
        content_digest: &str,
        older_than_ns: i64,
        now_ns: i64,
    ) -> Result<usize, StorageError> {
        (**self).sweep_pending_attachment(content_digest, older_than_ns, now_ns)
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
    fn get_pending_attachment(
        &self,
        content_digest: &str,
    ) -> Result<Option<StoredPendingAttachment>, StorageError> {
        Ok(self.raw_query(|conn| {
            pending_attachments::table
                .find(content_digest)
                .select(StoredPendingAttachment::as_select())
                .first(conn)
                .optional()
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

    #[xmtp_common::db_span]
    fn claim_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        lease_duration_ns: i64,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::sql_query(
                "UPDATE pending_attachments SET status = 'uploading', failure_cause = NULL, \
                 failure_credential_kind = NULL, failure_retryable = NULL, lease_id = ?, \
                 lease_expires_at_ns = ? + ? WHERE content_digest = ? AND status != 'complete' \
                 AND NOT (status = 'failed' AND failure_cause = 'backend_rejected') \
                 AND (status != 'uploading' OR lease_expires_at_ns < ?)",
            )
            .bind::<Binary, _>(lease_id)
            .bind::<BigInt, _>(now_ns)
            .bind::<BigInt, _>(lease_duration_ns)
            .bind::<Text, _>(content_digest)
            .bind::<BigInt, _>(now_ns)
            .execute(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn extend_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        lease_duration_ns: i64,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::sql_query(
                "UPDATE pending_attachments SET lease_expires_at_ns = ? + ? \
                 WHERE content_digest = ? AND lease_id = ? \
                 AND status = 'uploading' AND lease_expires_at_ns >= ?",
            )
            .bind::<BigInt, _>(now_ns)
            .bind::<BigInt, _>(lease_duration_ns)
            .bind::<Text, _>(content_digest)
            .bind::<Binary, _>(lease_id)
            .bind::<BigInt, _>(now_ns)
            .execute(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn finish_pending_attachment(
        &self,
        content_digest: &str,
        lease_id: &[u8],
        now_ns: i64,
        outcome: PendingAttachmentOutcome,
    ) -> Result<usize, StorageError> {
        let (status, failure_cause, failure_credential_kind, failure_retryable) = match outcome {
            PendingAttachmentOutcome::Complete => ("complete", None, None, None),
            PendingAttachmentOutcome::Failed {
                cause,
                credential_kind,
                retryable,
            } => ("failed", Some(cause), credential_kind, retryable),
        };
        Ok(self.raw_query(|conn| {
            diesel::sql_query(
                "UPDATE pending_attachments SET status = ?, failure_cause = ?, \
                 failure_credential_kind = ?, failure_retryable = ?, lease_id = NULL, \
                 lease_expires_at_ns = NULL WHERE content_digest = ? AND lease_id = ? \
                 AND status = 'uploading' AND lease_expires_at_ns >= ?",
            )
            .bind::<Text, _>(status)
            .bind::<Nullable<Text>, _>(failure_cause)
            .bind::<Nullable<Text>, _>(failure_credential_kind)
            .bind::<Nullable<Bool>, _>(failure_retryable)
            .bind::<Text, _>(content_digest)
            .bind::<Binary, _>(lease_id)
            .bind::<BigInt, _>(now_ns)
            .execute(conn)
        })?)
    }

    #[xmtp_common::db_span]
    fn sweep_pending_attachment(
        &self,
        content_digest: &str,
        older_than_ns: i64,
        now_ns: i64,
    ) -> Result<usize, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::sql_query(
                "DELETE FROM pending_attachments WHERE content_digest = ? AND created_at_ns < ? \
                 AND NOT (status = 'uploading' AND lease_expires_at_ns >= ?)",
            )
            .bind::<Text, _>(content_digest)
            .bind::<BigInt, _>(older_than_ns)
            .bind::<BigInt, _>(now_ns)
            .execute(conn)
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
                status: "waiting".into(),
                failure_cause: None,
                failure_credential_kind: None,
                failure_retryable: None,
                lease_id: None,
                lease_expires_at_ns: None,
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

    // verifies: ATCH-074, ATCH-029, ATCH-034
    #[xmtp_common::test(unwrap_try = true)]
    async fn claim_is_atomic_and_owned() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let a = [1u8; 16];
        let b = [2u8; 16];
        db.insert_or_ignore_pending_attachment("digest", &[1], 1)?;
        assert_eq!(db.claim_pending_attachment("digest", &a, 100, 120)?, 1);
        let row = db.get_pending_attachment("digest")?.unwrap();
        assert_eq!(row.effective_status(100), "uploading");
        assert_eq!(row.lease_expires_at_ns, Some(220));
        assert_eq!(db.claim_pending_attachment("digest", &b, 220, 120)?, 0);
        assert_eq!(row.effective_status(221), "waiting");
        assert_eq!(db.claim_pending_attachment("digest", &b, 221, 120)?, 1);
        assert_eq!(
            db.finish_pending_attachment(
                "digest",
                &a,
                222,
                PendingAttachmentOutcome::failed("network", None, None)
            )?,
            0
        );
        assert_eq!(
            db.finish_pending_attachment(
                "digest",
                &b,
                222,
                PendingAttachmentOutcome::failed("backend_rejected", None, None)
            )?,
            1
        );
        assert_eq!(db.claim_pending_attachment("digest", &a, 223, 120)?, 0);
        assert_eq!(
            db.get_pending_attachment("digest")?
                .unwrap()
                .failure_cause
                .as_deref(),
            Some("backend_rejected")
        );
    }

    // verifies: ATCH-074
    #[xmtp_common::test(unwrap_try = true)]
    async fn extend_requires_a_live_matching_lease() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let a = [1u8; 16];
        let b = [2u8; 16];
        db.insert_or_ignore_pending_attachment("digest", &[1], 1)?;
        db.claim_pending_attachment("digest", &a, 100, 120)?;
        assert_eq!(db.extend_pending_attachment("digest", &b, 150, 120)?, 0);
        assert_eq!(db.extend_pending_attachment("digest", &a, 150, 120)?, 1);
        assert_eq!(
            db.get_pending_attachment("digest")?
                .unwrap()
                .lease_expires_at_ns,
            Some(270)
        );
        assert_eq!(db.extend_pending_attachment("digest", &a, 271, 120)?, 0);
        assert_eq!(
            db.finish_pending_attachment("digest", &a, 271, PendingAttachmentOutcome::Complete)?,
            0
        );
    }

    // verifies: ATCH-025, ATCH-061, ATCH-066
    #[xmtp_common::test(unwrap_try = true)]
    async fn outcome_keeps_failure_details_and_complete() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let token = [7u8; 16];
        db.insert_or_ignore_pending_attachment("digest", &[1], 1)?;
        db.claim_pending_attachment("digest", &token, 100, 120)?;
        assert_eq!(
            db.finish_pending_attachment(
                "digest",
                &token,
                150,
                PendingAttachmentOutcome::failed("credential", Some("callback_failed"), Some(true))
            )?,
            1
        );
        let failed = db.get_pending_attachment("digest")?.unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.failure_cause.as_deref(), Some("credential"));
        assert_eq!(
            failed.failure_credential_kind.as_deref(),
            Some("callback_failed")
        );
        assert_eq!(failed.failure_retryable, Some(true));
        assert_eq!(failed.lease_id, None);
        db.claim_pending_attachment("digest", &token, 160, 120)?;
        assert_eq!(
            db.finish_pending_attachment(
                "digest",
                &token,
                170,
                PendingAttachmentOutcome::Complete
            )?,
            1
        );
        let complete = db.get_pending_attachment("digest")?.unwrap();
        assert_eq!(complete.status, "complete");
        assert_eq!(complete.failure_cause, None);
        assert_eq!(complete.lease_id, None);
        assert_eq!(db.claim_pending_attachment("digest", &token, 180, 120)?, 0);
    }

    // verifies: ATCH-068, ATCH-074
    #[xmtp_common::test(unwrap_try = true)]
    async fn sweep_deletes_only_unleased_old_rows() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let token = [9u8; 16];
        for digest in ["leased", "expired", "complete", "new"] {
            db.insert_or_ignore_pending_attachment(
                digest,
                &[1],
                if digest == "new" { 10 } else { 1 },
            )?;
        }
        db.claim_pending_attachment("leased", &token, 100, 120)?;
        db.claim_pending_attachment("expired", &token, 1, 120)?;
        db.claim_pending_attachment("complete", &token, 100, 120)?;
        db.finish_pending_attachment("complete", &token, 101, PendingAttachmentOutcome::Complete)?;
        assert_eq!(db.sweep_pending_attachment("leased", 5, 150)?, 0);
        assert_eq!(db.sweep_pending_attachment("expired", 5, 150)?, 1);
        assert_eq!(db.sweep_pending_attachment("complete", 5, 150)?, 1);
        assert_eq!(db.sweep_pending_attachment("new", 5, 150)?, 0);
        assert!(db.get_pending_attachment("leased")?.is_some());
        assert!(db.get_pending_attachment("expired")?.is_none());
    }

    // verifies: ATCH-074
    #[xmtp_common::test(unwrap_try = true)]
    async fn schema_rejects_uploading_without_a_lease() {
        let store = TestDb::create_ephemeral_store().await;
        let db = store.db();
        let invalid = db.raw_query(|conn| {
            diesel::sql_query("INSERT INTO pending_attachments (content_digest, remote_attachment, created_at_ns, status) VALUES ('bad', X'01', 1, 'uploading')").execute(conn)
        });
        assert!(invalid.is_err());
        assert!(db.get_pending_attachment("bad")?.is_none());
    }
}
