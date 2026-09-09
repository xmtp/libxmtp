use crate::test_support as support;

use crate::{
    api::{self, subscribe_response::Response as Frame},
    config::Config,
    server,
};
use support::{
    TestServer,
    native::{Native, envelope},
    replica::with_paused_replay,
};
use tonic::Code;

static REPLAY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn replica(config: &mut Config) {
    let mut url = url::Url::parse(&config.database.url).unwrap();
    url.set_port(Some(55433)).unwrap();
    config.database.replica_url = Some(url.to_string());
    config.streams.poll_interval_ms = 10;
}

#[xmtp_common::test(unwrap_try = true)]
async fn paused_replica_keeps_fixed_empty_targets_and_recovers_visible_rows() {
    let _replay = REPLAY.lock().await;
    let Some(metrics) = support::metrics::isolated(
        "stream::tests::replica::paused_replica_keeps_fixed_empty_targets_and_recovers_visible_rows",
    ) else {
        return;
    };
    let server = TestServer::new(replica).await?;
    let read = server.backend.store.read.clone();
    let (meta, mut stream) = with_paused_replay(&read, async {
    let meta = server.publish(vec![envelope(21, 1)]).await?.remove(0);
    let id = meta.cursor.as_ref().unwrap().sequence_id;
    let primary = server
        .query()
        .query(api::QueryRequest {
            queries: vec![support::query_topic(meta.topic.clone().unwrap(), 0)],
            limit: 1,
        })
        .await?
        .into_inner();
    assert_eq!(primary.envelopes.len(), 1);
    assert_eq!(
        server
            .query()
            .get(api::GetRequest { sequence_id: id })
            .await
            .unwrap_err()
            .code(),
        Code::NotFound
    );
    let mut stream = Native::open(&server).await?;
    stream
        .update(
            1,
            vec![support::query_topic(meta.topic.clone().unwrap(), 0)],
            vec![],
        )
        .await?;
    assert!(
        matches!(stream.next().await?, Frame::Applied(applied) if applied.added_targets[0].through_sequence_id == 0)
    );
        // The production sampler must observe unapplied WAL within two sample intervals.
        xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(10), xmtp_common::wait_for_some(|| async {
            (support::metrics::value(&metrics, "xmtp_replica_replay_delay_seconds", &[]) > 0.0 && support::metrics::value(&metrics, "xmtp_sequence_id", &[("database", "primary")]) == id as f64).then_some(())
        })).await?.ok_or("replica delay was not sampled")?;
        assert_eq!(support::metrics::value(&metrics, "xmtp_sequence_id", &[("database", "primary")]), id as f64);
        assert_eq!(support::metrics::value(&metrics, "xmtp_sequence_id", &[("database", "read")]), 0.0);
        Ok((meta, stream))
    }).await?;
    let rows = stream.messages(1).await?;
    assert_eq!(rows[0].meta, Some(meta));
    xmtp_common::wait_for_eq(
        || async { support::metrics::value(&metrics, "xmtp_sequence_id", &[("database", "read")]) },
        1.0,
    )
    .await?;
    xmtp_common::wait_for_eq(
        || async { support::metrics::value(&metrics, "xmtp_replica_replay_delay_seconds", &[]) },
        0.0,
    )
    .await?;
    assert!(metrics.render().contains("pool=\"read\""));
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn startup_waits_for_its_boundary_to_reach_the_selected_replica() {
    let _replay = REPLAY.lock().await;
    let first = TestServer::new(replica).await?;
    let read = first.backend.store.read.clone();
    let initialization = with_paused_replay(&read, async {
        first.publish(vec![envelope(22, 1)]).await?;
        let config = (*first.backend.config).clone();
        let initialization = tokio::spawn(server::initialize(config));
        xmtp_common::wait_for_eq(
            || async {
                sqlx::query_scalar!(
                    "SELECT closed_sequence_id FROM allocation_boundary WHERE singleton"
                )
                .fetch_one(&first.backend.store.primary)
                .await
                .unwrap()
            },
            1,
        )
        .await?;
        assert!(!initialization.is_finished());
        Ok(initialization)
    })
    .await?;
    let backend =
        xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), initialization)
            .await???;
    drop(backend);
    first.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn replay_resumes_after_test_error_or_assertion_failure() {
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;
    let _replay = REPLAY.lock().await;
    let server = TestServer::new(replica).await?;
    let read = &server.backend.store.read;
    let error = with_paused_replay(read, async { Err::<(), _>("deliberate test error".into()) })
        .await
        .unwrap_err();
    let paused_after_error =
        sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe("SELECT pg_is_wal_replay_paused()"))
            .fetch_one(read)
            .await?;
    let panic = AssertUnwindSafe(with_paused_replay(read, async {
        let paused =
            sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe("SELECT pg_is_wal_replay_paused()"))
                .fetch_one(read)
                .await?;
        assert!(!paused, "deliberate assertion failure");
        Ok(())
    }))
    .catch_unwind()
    .await;
    let paused =
        sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe("SELECT pg_is_wal_replay_paused()"))
            .fetch_one(read)
            .await?;
    // Repair shared state even when this regression detects broken cleanup.
    sqlx::query!("SELECT pg_wal_replay_resume()")
        .execute(read)
        .await?;
    server.stop().await?;
    assert_eq!(error.to_string(), "deliberate test error");
    assert!(!paused_after_error);
    assert!(panic.is_err());
    assert!(!paused);
}

#[xmtp_common::test(unwrap_try = true)]
async fn late_gap_rows_precede_forward_rows_on_the_same_topic() {
    let _replay = REPLAY.lock().await;
    let server = TestServer::new(replica).await?;
    let first = server.publish(vec![envelope(23, 0)]).await?.remove(0);
    xmtp_common::wait_for_ok(|| async {
        server.query().get(api::GetRequest { sequence_id: 1 }).await
    })
    .await?;
    let topic_a = first.topic.unwrap();
    let topic_b = support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[24; 32]);
    let mut stream = Native::open(&server).await?;
    stream
        .update(
            1,
            vec![
                support::query_topic(topic_a.clone(), 1),
                support::query_topic(topic_b, 0),
            ],
            vec![],
        )
        .await?;
    assert!(matches!(stream.next().await?, Frame::Applied(_)));
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query!(
        "SELECT last_sequence_id FROM topic_watermark WHERE topic = $1 FOR UPDATE",
        &topic_a.topic
    )
    .fetch_one(&mut *blocker)
    .await?;
    let mut publisher = server.publisher();
    let delayed = tokio::spawn(async move {
        publisher
            .publish(api::PublishRequest {
                envelopes: vec![envelope(23, 1)],
            })
            .await
    });
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar!("SELECT last_value FROM envelope_sequence")
                .fetch_one(&server.backend.store.primary)
                .await
                .unwrap()
        },
        2,
    )
    .await?;
    server.publish(vec![envelope(24, 1)]).await?;
    assert_eq!(
        stream.messages(1).await?[0]
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id,
        3
    );
    assert!(
        sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(&server.backend.store.read)
            .await?
            < 3
    );
    blocker.commit().await?;
    delayed.await??;
    server.publish(vec![envelope(23, 2)]).await?;
    let rows = stream.messages(2).await?;
    assert_eq!(
        rows.iter()
            .map(|row| row
                .meta
                .as_ref()
                .unwrap()
                .cursor
                .as_ref()
                .unwrap()
                .sequence_id)
            .collect::<Vec<_>>(),
        vec![2, 4]
    );
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn replica_connection_loss_fails_existing_sessions_before_recovery() {
    let _replay = REPLAY.lock().await;
    let Some(metrics) = support::metrics::isolated(
        "stream::tests::replica::replica_connection_loss_fails_existing_sessions_before_recovery",
    ) else {
        return;
    };
    let server = TestServer::new(replica).await?;
    let mut stream = Native::open(&server).await?;
    let mut connection = server.backend.store.read.acquire().await?;
    sqlx::query!("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid()")
        .fetch_all(&mut *connection).await?;
    assert_eq!(
        xmtp_common::time::timeout(
            xmtp_common::time::Duration::from_secs(5),
            stream.output.message()
        )
        .await?
        .unwrap_err()
        .code(),
        Code::Unavailable
    );
    xmtp_common::wait_for_eq(
        || async {
            support::metrics::value(&metrics, "xmtp_stream_ended_total", &[("reason", "tailer")])
        },
        1.0,
    )
    .await?;
    assert_eq!(
        support::metrics::value(&metrics, "xmtp_stream_sessions", &[("kind", "bidi")]),
        0.0
    );
    assert!(support::metrics::value(&metrics, "xmtp_tailer_restarts_total", &[]) >= 2.0);
    drop(connection);
    drop(stream);
    server.stop().await?;
}
