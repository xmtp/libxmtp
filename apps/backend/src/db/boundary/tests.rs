use super::*;
use crate::test_support::TestServer;

#[xmtp_common::test(unwrap_try = true)]
async fn statement_timeout_during_barrier_acquisition_preserves_proof_and_allows_retry() {
    let Some(metrics) = crate::test_support::metrics::isolated(
        "db::boundary::tests::statement_timeout_during_barrier_acquisition_preserves_proof_and_allows_retry",
    ) else {
        return;
    };
    let capture = xmtp_logging::test_logging::LogCapture::new(xmtp_logging::Level::Info);
    let server = TestServer::new(|config| {
        config.database.max_statement_timeout_ms = 100;
        config.publishing.max_barrier_wait_ms = 1_000;
    })
    .await?;
    server.backend.streams.as_ref().unwrap().stop();
    let pool = &server.backend.store.primary;
    let mut publisher = pool.begin().await?;
    sqlx::query!(
        "SELECT pg_advisory_xact_lock_shared($1::integer, $2::integer)",
        GLOBAL_LOCK_DOMAIN,
        ALLOCATION_BARRIER
    )
    .execute(&mut *publisher)
    .await?;
    sqlx::query!("SELECT nextval('envelope_sequence')")
        .fetch_one(&mut *publisher)
        .await?;
    let attempt = tracing::instrument::WithSubscriber::with_subscriber(
        advance(pool, 1_000),
        capture.dispatch(),
    )
    .await;
    let previous =
        sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(pool)
            .await?;
    publisher.rollback().await?;
    let retry = advance(pool, 1_000).await?;
    server.stop().await?;
    assert_eq!(attempt.unwrap(), None);
    assert_eq!(previous, 0);
    assert_eq!(retry, Some(1));
    assert_eq!(
        crate::test_support::metrics::value(
            &metrics,
            "xmtp_boundary_advances_total",
            &[("result", "lock_timeout")]
        ),
        1.0
    );
    assert_eq!(
        capture
            .output()
            .matches("allocation barrier lock timed out")
            .count(),
        1
    );
    assert!(capture.output().contains("WARN"));
}

#[xmtp_common::test(unwrap_try = true)]
async fn statement_timeout_after_barrier_acquisition_remains_an_error() {
    let server = TestServer::new(|config| {
        config.database.max_statement_timeout_ms = 100;
    })
    .await?;
    server.backend.streams.as_ref().unwrap().stop();
    let pool = &server.backend.store.primary;
    let mut blocker = pool.begin().await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        "SELECT closed_sequence_id FROM allocation_boundary WHERE singleton FOR UPDATE",
    ))
    .execute(&mut *blocker)
    .await?;
    let attempt = advance(pool, 1_000).await;
    blocker.rollback().await?;
    server.stop().await?;
    assert!(
        matches!(attempt, Err(Error::Database(sqlx::Error::Database(error)))
        if error.code().as_deref() == Some("57014"))
    );
}
