use super::*;
use crate::test_support::TestServer;

#[xmtp_common::test(unwrap_try = true)]
async fn signals_during_cooldown_share_one_boundary_attempt() {
    let server = TestServer::new(|_| {}).await?;
    server.backend.streams.as_ref().unwrap().stop();
    let pool = &server.backend.store.primary;
    sqlx::query!("SELECT nextval('envelope_sequence')")
        .fetch_one(pool)
        .await?;
    let signal = Arc::new(Notify::new());
    signal.notify_one();
    let worker = Worker(tokio::spawn(maintain(
        pool.clone(),
        signal.clone(),
        Duration::from_millis(250),
        10,
        Instant::now(),
    )));
    tokio::task::yield_now().await;
    for _ in 0..1_000 {
        signal.notify_one();
    }
    xmtp_common::wait_for_eq(|| boundary(pool), 1).await?;
    sqlx::query!("SELECT nextval('envelope_sequence')")
        .fetch_one(pool)
        .await?;
    let extra = xmtp_common::time::timeout(
        Duration::from_millis(400),
        xmtp_common::wait_for_eq(|| boundary(pool), 2),
    )
    .await;
    assert!(
        extra.is_err(),
        "cooldown signals scheduled a second attempt"
    );
    signal.notify_one();
    xmtp_common::wait_for_eq(|| boundary(pool), 2).await?;
    drop(worker);
    server.stop().await?;
}

async fn boundary(pool: &PgPool) -> i64 {
    sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap()
}
