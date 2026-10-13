//! Tests that hold a database lock while requests queue behind it.

use super::{RunningServer, TestResult};
use crate::api;

/// Publish `envelope` on its own task, so a test can hold it behind a lock.
pub(crate) fn publish_task(
    server: &RunningServer,
    envelope: api::ClientEnvelope,
) -> tokio::task::JoinHandle<Result<tonic::Response<api::PublishResponse>, tonic::Status>> {
    let mut client = server.publisher();
    tokio::spawn(async move {
        client
            .publish(api::PublishRequest {
                envelopes: vec![envelope],
            })
            .await
    })
}

/// Wait until a transaction in the test database waits for a lock of
/// `lock_type`, such as `relation` or `advisory`.
pub(crate) async fn wait_for_lock(pool: &sqlx::PgPool, lock_type: &'static str) -> TestResult {
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM pg_locks
                WHERE database = (SELECT oid FROM pg_database WHERE datname = current_database())
                  AND locktype = $1 AND NOT granted) AS "waiting!""#,
                lock_type
            )
            .fetch_one(pool)
            .await
            .unwrap()
        },
        true,
    )
    .await?;
    Ok(())
}
