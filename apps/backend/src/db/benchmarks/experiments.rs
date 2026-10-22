//! Benchmark-only paging and index controls for large envelope histories.
//! `PageQuery` selects real Store/EXPLAIN SQL; `IndexProfile` builds a measured
//! metadata-covering replacement after shared corpus seeding. Production never
//! installs these indexes or selects the experimental paging statement.

use super::{Store, TestResult};
use crate::{
    db::{EnvelopePage, StoredEnvelope, TopicCursor},
    error::Error,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub(super) const WINDOW_ROWS: i64 = 4_096;
pub(super) const MIN_WINDOW_TOPICS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PageQuery {
    Lateral,
    Watermarks,
    Bounded,
}

impl PageQuery {
    /// Keep small or heterogeneous requests on production probes; only broad
    /// uniform cursors pay for speculative globally ordered candidates.
    pub(in crate::db) fn uses_window(self, queries: &[TopicCursor]) -> bool {
        self == Self::Bounded
            && queries.len() >= MIN_WINDOW_TOPICS
            && queries
                .iter()
                .all(|query| query.cursor == queries[0].cursor)
    }

    /// Execution and diagnostics must select exactly the same statement.
    pub(super) fn sql(self, queries: &[TopicCursor]) -> &'static str {
        if self.uses_window(queries) {
            include_str!("../../../benchmarks/queries/bounded_page.sql")
        } else if self == Self::Watermarks {
            include_str!("../../../benchmarks/queries/watermark_page.sql")
        } else {
            include_str!("../read/page.sql")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IndexProfile {
    Baseline,
    MetadataCovering,
}

impl IndexProfile {
    /// Replace, rather than duplicate, the topic/sequence index and reestablish
    /// visibility before timing; retain shared column statistics across profiles.
    pub(super) async fn install(self, store: &Store) -> TestResult {
        if self == Self::MetadataCovering {
            let mut transaction = store.primary.begin().await?;
            sqlx::raw_sql(sqlx::AssertSqlSafe(include_str!(
                "../../../benchmarks/migrations/20261022000000_metadata_covering.sql"
            )))
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            for statement in ["VACUUM envelopes", "CHECKPOINT"] {
                sqlx::raw_sql(statement).execute(&store.primary).await?;
            }
        }
        Ok(())
    }
}

/// Run the selected experimental page in one snapshot, including exact has_more.
pub(in crate::db) async fn query(
    store: &Store,
    queries: &[TopicCursor],
    limit: i64,
) -> Result<EnvelopePage, Error> {
    let topics = queries
        .iter()
        .map(|query| query.topic.clone())
        .collect::<Vec<_>>();
    let cursors = queries.iter().map(|query| query.cursor).collect::<Vec<_>>();
    let statement = sqlx::query(sqlx::AssertSqlSafe(store.page_query.sql(queries)))
        .bind(topics)
        .bind(cursors)
        .bind(limit);
    let statement = if store.page_query.uses_window(queries) {
        statement.bind(WINDOW_ROWS)
    } else {
        statement
    };
    let rows = statement.fetch_all(&store.primary).await?;
    let has_more = rows
        .first()
        .map(|row| row.try_get::<bool, _>("has_more"))
        .transpose()?
        .unwrap_or(false);
    let envelopes = rows
        .into_iter()
        .map(|row| {
            Ok(StoredEnvelope {
                sequence_id: row.try_get("sequence_id")?,
                topic: row.try_get("topic")?,
                server_ns: row.try_get("server_ns")?,
                expiry_ns: row.try_get("expiry_ns")?,
                message_hash: row.try_get("message_hash")?,
                is_commit_or_proposal: row.try_get("is_commit_or_proposal")?,
                payload: row.try_get("payload")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(EnvelopePage {
        envelopes,
        has_more,
    })
}

#[cfg(test)]
mod tests;
