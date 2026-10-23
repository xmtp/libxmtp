//! Snapshot-consistent paging, newest, and identifier reads for backend services.
//! Test builds can select disposable paging experiments; production
//! query SQL and its primary/replica routing remain unchanged.

use super::{EnvelopePage, Store, StoredEnvelope, StoredMeta, TopicCursor};
use crate::error::Error;

#[cfg(test)]
mod tests;

impl Store {
    /// Read one total page from the primary using per-topic indexed probes.
    ///
    /// Each topic contributes at most `limit + 1` candidates, then the final
    /// page is cut to the total limit. The page and `has_more` are computed by
    /// one SQL statement, so they describe the same read snapshot.
    #[xmtp_common::db_span]
    // implements: API-202, API-240
    pub(crate) async fn query(
        &self,
        queries: &[TopicCursor],
        limit: i64,
    ) -> Result<EnvelopePage, Error> {
        #[cfg(test)]
        if matches!(
            self.page_query,
            super::benchmarks::experiments::PageQuery::Watermarks
                | super::benchmarks::experiments::PageQuery::Frontier
        ) || self.page_query.uses_window(queries)
        {
            return super::benchmarks::experiments::query(self, queries, limit).await;
        }
        let topics: Vec<_> = queries.iter().map(|query| query.topic.clone()).collect();
        let cursors: Vec<_> = queries.iter().map(|query| query.cursor).collect();
        let rows = sqlx::query_file!("src/db/read/page.sql", &topics, &cursors, limit)
            .fetch_all(&self.primary)
            .await?;
        let has_more = rows.first().is_some_and(|row| row.has_more);
        let envelopes = rows
            .into_iter()
            .map(|row| StoredEnvelope {
                sequence_id: row.sequence_id,
                topic: row.topic,
                server_ns: row.server_ns,
                expiry_ns: row.expiry_ns,
                message_hash: row.message_hash,
                is_commit_or_proposal: row.is_commit_or_proposal,
                payload: row.payload,
            })
            .collect();
        Ok(EnvelopePage {
            envelopes,
            has_more,
        })
    }

    /// Read the newest visible envelope for each requested topic.
    ///
    /// Watermarks and payload rows are joined in one query on the read pool.
    /// With a replica, the result can lag the primary but cannot reorder a
    /// topic. Topics without a watermark are omitted.
    #[xmtp_common::db_span]
    pub(crate) async fn newest_envelopes(
        &self,
        topics: &[Vec<u8>],
    ) -> Result<Vec<StoredEnvelope>, Error> {
        Ok(
            sqlx::query_file_as!(StoredEnvelope, "src/db/read/newest.sql", topics)
                .fetch_all(&self.read)
                .await?,
        )
    }

    /// Read newest metadata without loading payload bytes.
    ///
    /// The read pool and watermark join match `newest_envelopes`; this method
    /// only projects the fields needed for a metadata-only response.
    #[xmtp_common::db_span]
    pub(crate) async fn newest_metadata(
        &self,
        topics: &[Vec<u8>],
    ) -> Result<Vec<StoredMeta>, Error> {
        Ok(
            sqlx::query_file_as!(StoredMeta, "src/db/read/newest_metadata.sql", topics)
                .fetch_all(&self.read)
                .await?,
        )
    }

    /// Resolve normalized identifier keys to their latest active inbox IDs.
    ///
    /// The query preserves input order and returns one optional value per input.
    /// It uses the read pool, so a replica can temporarily return an older
    /// projection while replication catches up.
    #[xmtp_common::db_span]
    pub(crate) async fn inbox_ids(
        &self,
        identifiers: &[String],
        kinds: &[i16],
    ) -> Result<Vec<Option<Vec<u8>>>, Error> {
        Ok(sqlx::query!(
            "SELECT active.inbox_id FROM unnest($1::text[], $2::smallint[])
                WITH ORDINALITY AS wanted(identifier, identifier_kind, ordinality)
            LEFT JOIN LATERAL (
                SELECT inbox_id FROM identifier_association
                WHERE identifier = wanted.identifier AND identifier_kind = wanted.identifier_kind
                    AND revocation_sequence_id IS NULL
                ORDER BY association_sequence_id DESC LIMIT 1
            ) active ON true ORDER BY wanted.ordinality",
            identifiers,
            kinds
        )
        .fetch_all(&self.read)
        .await?
        .into_iter()
        .map(|row| row.inbox_id)
        .collect())
    }
}
