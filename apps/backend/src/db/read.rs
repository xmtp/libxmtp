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
    /// Returns `None` when payload bytes exceed `max_bytes`.
    #[xmtp_common::db_span]
    // implements: API-202, API-240
    pub(crate) async fn query(
        &self,
        queries: &[TopicCursor],
        limit: i64,
        max_bytes: i64,
    ) -> Result<Option<EnvelopePage>, Error> {
        let topics: Vec<_> = queries.iter().map(|query| query.topic.clone()).collect();
        let cursors: Vec<_> = queries.iter().map(|query| query.cursor).collect();
        // Scalar totals become InitPlans, not aggregates rerun for each row.
        let rows = sqlx::query!(
            r#"WITH wanted AS (
                SELECT * FROM unnest($1::bytea[], $2::bigint[]) AS r(topic, cursor)
            ), candidates AS MATERIALIZED (
                SELECT e.sequence_id FROM wanted CROSS JOIN LATERAL (
                    SELECT sequence_id FROM envelopes
                    WHERE topic = wanted.topic AND sequence_id > wanted.cursor
                    ORDER BY sequence_id LIMIT ($3::bigint + 1)
                ) AS e ORDER BY e.sequence_id LIMIT ($3::bigint + 1)
            ), page AS MATERIALIZED (
                SELECT c.sequence_id, octet_length(e.payload) AS bytes
                FROM (SELECT sequence_id FROM candidates ORDER BY sequence_id LIMIT $3::bigint) AS c
                JOIN envelopes AS e USING (sequence_id)
            )
            SELECT e.sequence_id, e.topic, e.server_ns, e.expiry_ns,
                e.message_hash, e.is_commit_or_proposal,
                CASE WHEN (SELECT COALESCE(sum(bytes), 0) FROM page) <= $4::bigint
                    THEN e.payload END AS "payload?",
                (SELECT COALESCE(sum(bytes), 0) FROM page) <= $4::bigint AS "fits!",
                (SELECT count(*) > $3::bigint FROM candidates) AS "has_more!"
            FROM page JOIN envelopes AS e USING (sequence_id) ORDER BY e.sequence_id"#,
            &topics,
            &cursors,
            limit,
            max_bytes
        )
        .fetch_all(&self.primary)
        .await?;
        if rows.first().is_some_and(|row| !row.fits) {
            return Ok(None);
        }
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
                payload: row.payload.expect("payloads fit the byte budget"),
            })
            .collect();
        Ok(Some(EnvelopePage {
            envelopes,
            has_more,
        }))
    }

    /// Read the newest visible envelope for each requested topic.
    ///
    /// Watermarks and payload rows are joined in one query on the read pool.
    /// With a replica, the result can lag the primary but cannot reorder a
    /// topic. Topics without a watermark are omitted. Returns `None` when
    /// payload bytes exceed `max_bytes`.
    #[xmtp_common::db_span]
    pub(crate) async fn newest_envelopes(
        &self,
        topics: &[Vec<u8>],
        max_bytes: i64,
    ) -> Result<Option<Vec<StoredEnvelope>>, Error> {
        let rows = sqlx::query!(
            r#"WITH newest AS MATERIALIZED (
                SELECT e.sequence_id, octet_length(e.payload) AS bytes
                FROM topic_watermark w JOIN envelopes e
                    ON e.sequence_id = w.last_sequence_id AND e.topic = w.topic
                WHERE w.topic = ANY($1::bytea[])
            )
            SELECT e.sequence_id, e.topic, e.server_ns, e.expiry_ns,
                e.message_hash, e.is_commit_or_proposal,
                CASE WHEN (SELECT COALESCE(sum(bytes), 0) FROM newest) <= $2::bigint
                    THEN e.payload END AS "payload?",
                (SELECT COALESCE(sum(bytes), 0) FROM newest) <= $2::bigint AS "fits!"
            FROM newest JOIN envelopes AS e USING (sequence_id)"#,
            topics,
            max_bytes
        )
        .fetch_all(&self.read)
        .await?;
        if rows.first().is_some_and(|row| !row.fits) {
            return Ok(None);
        }
        Ok(Some(
            rows.into_iter()
                .map(|row| StoredEnvelope {
                    sequence_id: row.sequence_id,
                    topic: row.topic,
                    server_ns: row.server_ns,
                    expiry_ns: row.expiry_ns,
                    message_hash: row.message_hash,
                    is_commit_or_proposal: row.is_commit_or_proposal,
                    payload: row.payload.expect("payloads fit the byte budget"),
                })
                .collect(),
        ))
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
        Ok(sqlx::query_as!(
            StoredMeta,
            "SELECT e.sequence_id, e.topic, e.server_ns, e.expiry_ns,
                e.message_hash, e.is_commit_or_proposal
            FROM topic_watermark w JOIN envelopes e
                ON e.sequence_id = w.last_sequence_id AND e.topic = w.topic
            WHERE w.topic = ANY($1::bytea[])",
            topics
        )
        .fetch_all(&self.read)
        .await?)
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
