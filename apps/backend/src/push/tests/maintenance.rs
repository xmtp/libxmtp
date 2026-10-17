use super::*;
use crate::{
    api,
    test_support::{
        self, TestServer,
        metrics::{isolated, value},
        native::{Native, envelope},
    },
};

#[xmtp_common::test(unwrap_try = true)]
async fn dispatcher_demand_survives_lock_timeout_and_tailer_recovery_without_holding_streams() {
    let Some(metrics) = isolated(
        "push::tests::maintenance::dispatcher_demand_survives_lock_timeout_and_tailer_recovery_without_holding_streams",
    ) else {
        return;
    };
    const POLL_MS: u64 = 100;
    let server = TestServer::new(|config| {
        config.push.http = Some(crate::config::push::HttpConfig::default());
        config.streams.poll_interval_ms = POLL_MS;
        config.publishing.max_barrier_wait_ms = 20;
    })
    .await?;
    let saved_signal = server.backend.streams.as_ref()?.maintenance.clone();
    let pool = &server.backend.store.primary;
    let mut native = Native::open(&server).await?;
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[81; 32]);
    native
        .update(1, vec![test_support::query_topic(topic, 0)], vec![])
        .await?;
    assert!(matches!(
        native.next().await?,
        api::subscribe_response::Response::Applied(_)
    ));
    let mut barrier = pool.begin().await?;
    let barrier_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *barrier)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock_shared(0, 2)")
        .execute(&mut *barrier)
        .await?;
    let began = Instant::now();
    server.publish(vec![envelope(81, 1)]).await?;
    assert_eq!(
        native.messages(1).await?.len(),
        1,
        "visible rows must reach streams above the push boundary"
    );
    let boundary: i64 =
        sqlx::query_scalar("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(pool)
            .await?;
    assert_eq!(boundary, 0);
    xmtp_common::wait_for_ge(
        || async {
            value(
                &metrics,
                "xmtp_boundary_advances_total",
                &[("result", "lock_timeout")],
            )
        },
        3.0,
    )
    .await?;
    let attempts = value(
        &metrics,
        "xmtp_boundary_advances_total",
        &[("result", "lock_timeout")],
    ) as u128;
    assert!(
        attempts <= began.elapsed().as_millis() / u128::from(POLL_MS) + 1,
        "dispatcher bypassed the shared maintenance cooldown"
    );
    // Stop the caller so no later request can hide a lost pending notification.
    stop(server.backend.push.as_ref()?).await;
    sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() AND pid <> $1")
        .bind(barrier_pid).execute(pool).await?;
    xmtp_common::wait_for_ge(
        || async { value(&metrics, "xmtp_tailer_restarts_total", &[]) },
        2.0,
    )
    .await?;
    barrier.rollback().await?;
    xmtp_common::wait_for_eq(|| async { value(&metrics, "xmtp_tailer_ready", &[]) }, 1.0).await?;
    let boundary: i64 =
        sqlx::query_scalar("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(pool)
            .await?;
    assert_eq!(boundary, 1);
    // Dense tailing does not request a boundary. Only the pre-recovery handle
    // below can advance it now that the dispatcher has stopped.
    server.publish(vec![envelope(81, 2)]).await?;
    saved_signal.notify_one();
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT closed_sequence_id FROM allocation_boundary WHERE singleton",
            )
            .fetch_one(pool)
            .await
            .unwrap()
        },
        2,
    )
    .await?;
    drop(native);
    server.stop().await?;
}
