use super::StoredEnvelope;
use crate::error::Error;
use sqlx::{Connection, PgConnection, PgPool, Postgres, Transaction};

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub sequence_id: i64,
    pub topic: Vec<u8>,
    pub payload_bytes: i32,
}

#[derive(Clone)]
pub(crate) struct Range {
    pub topic: Vec<u8>,
    pub after: i64,
    pub through: i64,
}

/// Close an interrupted history connection instead of returning a still-running
/// query to the pool. SQLx retains its pool permit until connection close ends.
/// PostgreSQL can finish cancellation later, subject to its statement timeout.
pub(crate) struct HistoryConnection(Option<sqlx::pool::PoolConnection<Postgres>>);

impl HistoryConnection {
    /// Reserve one request-pool connection, including during cancellation cleanup.
    pub(crate) async fn acquire(pool: &PgPool) -> Result<Self, Error> {
        Ok(Self(Some(pool.acquire().await?)))
    }

    /// Read candidates and payloads from one read-only snapshot. Commit the
    /// transaction before releasing this guard for normal connection reuse.
    pub(crate) async fn snapshot(&mut self) -> Result<Transaction<'_, Postgres>, Error> {
        snapshot_connection(self.0.as_mut().expect("history connection present")).await
    }

    /// Return a completed snapshot's connection to the pool for reuse.
    pub(crate) fn release(mut self) {
        self.0.take();
    }
}

impl Drop for HistoryConnection {
    fn drop(&mut self) {
        if let Some(connection) = &mut self.0 {
            connection.close_on_drop();
        }
    }
}

/// Keep one tailer connection across snapshots so a disconnect cannot be hidden
/// by pool replacement. Recovery must establish a fresh closed boundary.
pub(crate) async fn snapshot_connection(
    connection: &mut PgConnection,
) -> Result<Transaction<'_, Postgres>, Error> {
    Ok(connection
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await?)
}

/// Read the replicated proof of settled allocations in the caller's snapshot.
pub(crate) async fn boundary(tx: &mut Transaction<'_, Postgres>) -> Result<i64, Error> {
    sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Invariant("allocation boundary missing"))
}

/// Return one visible head per input position, including zero for absent topics.
pub(crate) async fn heads(pool: &PgPool, topics: &[Vec<u8>]) -> Result<Vec<i64>, Error> {
    Ok(sqlx::query!(
        r#"SELECT COALESCE(w.last_sequence_id, 0) AS "head!"
        FROM unnest($1::bytea[]) WITH ORDINALITY AS wanted(topic, ordinal)
        LEFT JOIN topic_watermark w ON w.topic = wanted.topic ORDER BY wanted.ordinal"#,
        topics
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.head)
    .collect())
}

/// Select bounded forward candidates without using the closed boundary as a ceiling.
pub(crate) async fn forward(
    tx: &mut Transaction<'_, Postgres>,
    after: i64,
    limit: i64,
) -> Result<Vec<Candidate>, Error> {
    Ok(sqlx::query_as!(
        Candidate,
        r#"SELECT sequence_id, topic, octet_length(payload) AS "payload_bytes!"
        FROM envelopes WHERE sequence_id > $1 ORDER BY sequence_id LIMIT $2"#,
        after,
        limit
    )
    .fetch_all(&mut **tx)
    .await?)
}

/// Probe bounded missing ranges in sequence order. A full page is not exhaustion.
pub(crate) async fn gaps(
    tx: &mut Transaction<'_, Postgres>,
    ranges: &[(i64, i64)],
    limit: i64,
) -> Result<Vec<Candidate>, Error> {
    let (low, high): (Vec<_>, Vec<_>) = ranges.iter().copied().unzip();
    Ok(sqlx::query_as!(
        Candidate,
        r#"SELECT e.sequence_id, e.topic, octet_length(e.payload) AS "payload_bytes!"
        FROM unnest($1::bigint[], $2::bigint[]) AS r(low, high)
        CROSS JOIN LATERAL (SELECT sequence_id, topic, payload FROM envelopes
            WHERE sequence_id BETWEEN r.low AND r.high ORDER BY sequence_id LIMIT $3) e
        ORDER BY e.sequence_id"#,
        &low,
        &high,
        limit
    )
    .fetch_all(&mut **tx)
    .await?)
}

/// Probe topics in caller-supplied fair order. Return sizes before allocating payloads.
pub(crate) async fn history(
    tx: &mut Transaction<'_, Postgres>,
    ranges: &[Range],
    limit: i64,
) -> Result<Vec<(usize, Candidate)>, Error> {
    let topics: Vec<_> = ranges.iter().map(|range| range.topic.clone()).collect();
    let after: Vec<_> = ranges.iter().map(|range| range.after).collect();
    let through: Vec<_> = ranges.iter().map(|range| range.through).collect();
    Ok(sqlx::query!(r#"SELECT wanted.ordinal AS "ordinal!", e.sequence_id, e.topic, octet_length(e.payload) AS "payload_bytes!"
        FROM unnest($1::bytea[], $2::bigint[], $3::bigint[]) WITH ORDINALITY AS wanted(topic, after, through, ordinal)
        CROSS JOIN LATERAL (SELECT sequence_id, topic, payload FROM envelopes
            WHERE topic = wanted.topic AND sequence_id > wanted.after AND sequence_id <= wanted.through
            ORDER BY sequence_id LIMIT $4) e ORDER BY wanted.ordinal, e.sequence_id"#,
        &topics, &after, &through, limit).fetch_all(&mut **tx).await?.into_iter()
        .map(|row| ((row.ordinal - 1) as usize, Candidate { sequence_id: row.sequence_id, topic: row.topic, payload_bytes: row.payload_bytes })).collect())
}

/// Load only candidates already admitted to the caller's byte budget. The same
/// transaction must own candidate selection, or absence would not be conclusive.
pub(crate) async fn payloads(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[i64],
) -> Result<Vec<StoredEnvelope>, Error> {
    Ok(sqlx::query_as!(StoredEnvelope, "SELECT sequence_id, topic, server_ns, expiry_ns, message_hash,
        is_commit_or_proposal, payload FROM envelopes WHERE sequence_id = ANY($1::bigint[]) ORDER BY sequence_id", ids)
        .fetch_all(&mut **tx).await?)
}
