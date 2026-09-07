use super::{ALLOCATION_BARRIER, GLOBAL_LOCK_DOMAIN};
use crate::error::Error;
use sqlx::PgPool;

/// A lock timeout leaves the previous boundary unchanged and requests another attempt.
pub(crate) async fn advance(pool: &PgPool, wait_ms: u64) -> Result<Option<i64>, Error> {
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
        .await?;
    let wait = format!("{wait_ms}ms");
    sqlx::query!("SELECT set_config('lock_timeout', $1, true)", wait)
        .fetch_one(&mut *tx)
        .await?;
    if let Err(error) = sqlx::query!(
        "SELECT pg_advisory_xact_lock($1::integer, $2::integer)",
        GLOBAL_LOCK_DOMAIN,
        ALLOCATION_BARRIER
    )
    .execute(&mut *tx)
    .await
    {
        if matches!(&error, sqlx::Error::Database(error) if error.code().as_deref() == Some("55P03"))
        {
            return Ok(None);
        }
        return Err(error.into());
    }
    let boundary = sqlx::query_scalar!(r#"SELECT CASE WHEN is_called THEN last_value ELSE 0 END AS "boundary!" FROM envelope_sequence"#)
        .fetch_one(&mut *tx).await?;
    let updated = sqlx::query_scalar!("UPDATE allocation_boundary SET closed_sequence_id = $1 WHERE singleton AND closed_sequence_id <= $1 RETURNING closed_sequence_id", boundary)
        .fetch_optional(&mut *tx).await?;
    if updated != Some(boundary) {
        return Err(Error::Invariant("missing or rewound allocation boundary"));
    }
    tx.commit().await?;
    Ok(Some(boundary))
}
