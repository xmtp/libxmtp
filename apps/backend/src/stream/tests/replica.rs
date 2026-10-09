use crate::test_support as support;

use crate::{
    api::{self, subscribe_response::Response as Frame},
    config::Config,
    stream::StreamHub,
};
use support::{
    RunningServer, TestDatabase, TestServer,
    native::{Native, envelope},
    replica::with_paused_replay,
};
use tonic::Code;

static REPLAY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const DEFAULT_REPLICA_PORT: u16 = 55433;
const PAUSED_STARTUP_OBSERVATION: xmtp_common::time::Duration =
    xmtp_common::time::Duration::from_secs(1);
const STARTUP_AFTER_REPLAY_TIMEOUT: xmtp_common::time::Duration =
    xmtp_common::time::Duration::from_secs(5);

fn replica(config: &mut Config) {
    let mut url = url::Url::parse(&config.database.url).unwrap();
    let port = std::env::var("XMTP_BACKEND_REPLICA_PORT").map_or(DEFAULT_REPLICA_PORT, |value| {
        value.parse().expect("replica port must be a valid u16")
    });
    url.set_port(Some(port)).unwrap();
    config.database.replica_url = Some(url.to_string());
    config.streams.poll_interval_ms = 10;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-202, API-252, OPS-009
async fn paused_replica_keeps_fixed_empty_targets_and_recovers_visible_rows() {
    let _replay = REPLAY.lock().await;
    let server = TestServer::new(replica).await?;
    let read = server.backend.store.read.clone();
    let (meta, mut stream) = with_paused_replay(&read, async {
    let meta = server.publish(vec![envelope(21, 1)]).await?.remove(0);
    let primary = server
        .query()
        .query(api::QueryRequest {
            queries: vec![support::query_topic(meta.topic.clone().unwrap(), 0)],
            limit: 1,
        })
        .await?
        .into_inner();
    assert_eq!(primary.envelopes.len(), 1);
    let newest = server.query().query_newest(api::QueryNewestRequest {
        topics: vec![meta.topic.clone().unwrap()],
        include_full_envelope: false,
    }).await?.into_inner();
    assert!(newest.results.is_empty());
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
        Ok((meta, stream))
    }).await?;
    let rows = stream.messages(1).await?;
    assert_eq!(rows[0].meta, Some(meta));
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-005
async fn startup_waits_for_its_boundary_to_reach_the_selected_replica() {
    let _replay = REPLAY.lock().await;
    let first = TestServer::new(replica).await?;
    let read = first.backend.store.read.clone();
    let config = (*first.backend.config).clone();
    // Schema replay has already completed; isolate the tailer's boundary wait.
    let mut initialization = std::pin::pin!(StreamHub::start(
        first.backend.store.primary.clone(),
        read.clone(),
        &config,
    ));
    with_paused_replay(&read, async {
        wait_for_replay_pause(&read).await?;
        first.publish(vec![envelope(22, 1)]).await?;
        let advanced = xmtp_common::wait_for_eq(
            || async {
                sqlx::query_scalar!(
                    "SELECT closed_sequence_id FROM allocation_boundary WHERE singleton"
                )
                .fetch_one(&first.backend.store.primary)
                .await
                .unwrap()
            },
            1,
        );
        tokio::select! {
            result = &mut initialization => {
                let streams = result?;
                streams.stop();
                panic!("stream recovery completed before its boundary reached the replica");
            }
            result = advanced => result?,
        }
        match xmtp_common::time::timeout(PAUSED_STARTUP_OBSERVATION, &mut initialization).await {
            Err(_) => {}
            Ok(result) => {
                let streams = result?;
                streams.stop();
                panic!("stream recovery completed while its boundary was absent on the replica");
            }
        }
        Ok(())
    })
    .await?;
    let streams =
        xmtp_common::time::timeout(STARTUP_AFTER_REPLAY_TIMEOUT, initialization).await??;
    streams.stop();
    drop(streams);
    first.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-005
async fn startup_waits_for_fresh_schema_to_reach_the_selected_replica() {
    let _replay = REPLAY.lock().await;
    let mut database = TestDatabase::new()?;
    let mut config: Config = toml::from_str(&format!(
        "[database]\nurl = {:?}\n[server]\nidentifier = {:?}",
        database.url(),
        support::DEFAULT_TEST_IDENTIFIER,
    ))?;
    replica(&mut config);
    let primary = sqlx::PgPool::connect(database.url()).await?;
    // Let CREATE DATABASE reach the replica, but apply no backend migrations yet.
    let read_url = config.database.replica_url.as_ref().unwrap();
    let read = xmtp_common::wait_for_ok(|| sqlx::PgPool::connect(read_url)).await?;
    assert!(!allocation_schema_exists(&read).await?);
    let mut initialization = std::pin::pin!(RunningServer::new(config));
    with_paused_replay(&read, async {
        wait_for_replay_pause(&read).await?;
        let migrated = xmtp_common::wait_for_eq(
            || async { allocation_schema_exists(&primary).await.unwrap() },
            true,
        );
        tokio::select! {
            result = &mut initialization => {
                let mut server = result?;
                server.stop().await?;
                panic!("backend initialized before its schema reached the replica");
            }
            result = migrated => result?,
        }
        assert!(!allocation_schema_exists(&read).await?);
        // Poll startup long enough to expose the old missing-table failure.
        match xmtp_common::time::timeout(PAUSED_STARTUP_OBSERVATION, &mut initialization).await {
            Err(_) => {}
            Ok(result) => {
                let mut server = result?;
                server.stop().await?;
                panic!("backend initialized while replica replay was paused before migration");
            }
        }
        Ok(())
    })
    .await?;
    let mut server =
        xmtp_common::time::timeout(STARTUP_AFTER_REPLAY_TIMEOUT, initialization).await??;
    assert!(
        sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe("SELECT pg_is_in_recovery()"))
            .fetch_one(&server.backend.store.read)
            .await?,
        "startup must retain the selected replica for reads",
    );
    let meta = server.publish(vec![envelope(26, 1)]).await?.remove(0);
    let response = server
        .query()
        .query(api::QueryRequest {
            queries: vec![support::query_topic(meta.topic.clone().unwrap(), 0)],
            limit: 1,
        })
        .await?
        .into_inner();
    assert_eq!(response.envelopes.len(), 1);
    server.stop().await?;
    primary.close().await;
    read.close().await;
    database.remove()?;
}

async fn allocation_schema_exists(pool: &sqlx::PgPool) -> support::TestResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(
        "SELECT to_regclass('public.allocation_boundary') IS NOT NULL",
    ))
    .fetch_one(pool)
    .await?)
}

async fn wait_for_replay_pause(read: &sqlx::PgPool) -> support::TestResult {
    // A pause request can return before WAL replay actually stops.
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(
                "SELECT pg_get_wal_replay_pause_state() = 'paused'",
            ))
            .fetch_one(read)
            .await
            .unwrap()
        },
        true,
    )
    .await?;
    Ok(())
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
// verifies: API-201, API-254, OPS-010
async fn late_gap_rows_precede_forward_rows_on_the_same_topic() {
    let _replay = REPLAY.lock().await;
    let server = TestServer::new(replica).await?;
    let first = server.publish(vec![envelope(23, 0)]).await?.remove(0);
    xmtp_common::wait_for_eq(
        || async {
            server
                .query()
                .query_newest(api::QueryNewestRequest {
                    topics: vec![first.topic.clone().unwrap()],
                    include_full_envelope: false,
                })
                .await
                .unwrap()
                .into_inner()
                .results
                .len()
        },
        1,
    )
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
// verifies: OPS-006
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
