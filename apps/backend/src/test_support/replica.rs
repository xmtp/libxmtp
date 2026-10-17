use super::TestResult;
use futures::FutureExt;
use sqlx::PgPool;
use std::{future::Future, panic::AssertUnwindSafe};
use xmtp_common::time::{Duration, timeout};

/// Resume shared replica replay before returning a test error or resuming an
/// assertion panic. Callers serialize tests that change replica replay state.
pub async fn with_paused_replay<T>(
    read: &PgPool,
    work: impl Future<Output = TestResult<T>>,
) -> TestResult<T> {
    sqlx::query!("SELECT pg_wal_replay_pause()")
        .execute(read)
        .await?;
    let result = AssertUnwindSafe(work).catch_unwind().await;
    let resumed = timeout(Duration::from_secs(5), async {
        sqlx::query!("SELECT pg_wal_replay_resume()")
            .execute(read)
            .await
    })
    .await;
    match result {
        Ok(Ok(value)) => {
            resumed??;
            Ok(value)
        }
        failed => {
            if !matches!(resumed, Ok(Ok(_))) {
                tracing::error!("test replica replay cleanup failed");
            }
            match failed {
                Ok(Err(error)) => Err(error),
                Err(panic) => std::panic::resume_unwind(panic),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    }
}
