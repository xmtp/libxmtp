use crate::{
    api,
    test_support::{
        TestServer,
        native::{Native, envelope},
    },
};
use xmtp_common::time::{Duration, Instant, timeout};

#[xmtp_common::timeout(std::time::Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn shutdown_marks_aggregate_and_named_health_services_not_serving() {
    use tonic_health::pb::{
        HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
    };
    let mut server = TestServer::new(|config| config.server.max_drain_duration_ms = 2000).await?;
    let mut client = HealthClient::new(server.channel.clone());
    let mut watches = Vec::new();
    for service in [
        "",
        "xmtp.backend.v1.QueryService",
        "xmtp.backend.v1.PublishService",
        "xmtp.backend.v1.IdentityService",
        "xmtp.backend.v1.SubscriptionService",
    ] {
        let mut watch = client
            .watch(HealthCheckRequest {
                service: service.into(),
            })
            .await?
            .into_inner();
        assert_eq!(
            watch.message().await?.unwrap().status(),
            ServingStatus::Serving
        );
        watches.push(watch);
    }
    server.shutdown();
    for mut watch in watches {
        assert_eq!(
            timeout(Duration::from_secs(1), watch.message())
                .await??
                .unwrap()
                .status(),
            ServingStatus::NotServing
        );
    }
    server.stop().await?;
}

async fn blocked_publish(
    server: &TestServer,
) -> crate::test_support::TestResult<(
    sqlx::Transaction<'static, sqlx::Postgres>,
    tokio::task::JoinHandle<Result<tonic::Response<api::PublishResponse>, tonic::Status>>,
)> {
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;
    let mut client = server.publisher();
    let publish = tokio::spawn(async move {
        client
            .publish(api::PublishRequest {
                envelopes: vec![envelope(98, 1)],
            })
            .await
    });
    xmtp_common::wait_for_eq(|| async {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM pg_locks WHERE database = (SELECT oid FROM pg_database WHERE datname = current_database())
            AND locktype = 'relation' AND NOT granted) AS "waiting!""#).fetch_one(&server.backend.store.primary).await.unwrap()
    }, true).await?;
    Ok((blocker, publish))
}

#[xmtp_common::timeout(std::time::Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn shutdown_fails_streams_immediately_and_drains_an_admitted_publish() {
    let mut server = TestServer::new(|config| config.server.max_drain_duration_ms = 2_000).await?;
    let mut stream = Native::open(&server).await?;
    let (blocker, publish) = blocked_publish(&server).await?;
    server.shutdown();
    assert_eq!(
        timeout(Duration::from_millis(500), stream.output.message())
            .await?
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    // HTTP/2 can reject the new stream before it reaches the admission layer.
    assert!(matches!(
        server
            .query()
            .get(api::GetRequest { sequence_id: 1 })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable | tonic::Code::Cancelled
    ));
    blocker.commit().await?;
    assert_eq!(publish.await??.into_inner().envelope_metas.len(), 1);
    timeout(Duration::from_secs(1), server.wait_stopped()).await??;
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(&server.backend.store.primary)
            .await?,
        Some(1)
    );
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::timeout(std::time::Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn shutdown_deadline_cancels_unfinished_unary_work_without_a_commit() {
    let mut server = TestServer::new(|config| config.server.max_drain_duration_ms = 100).await?;
    let (blocker, publish) = blocked_publish(&server).await?;
    let started = Instant::now();
    server.shutdown();
    timeout(Duration::from_secs(1), server.wait_stopped()).await??;
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(timeout(Duration::from_secs(1), publish).await??.is_err());
    blocker.commit().await?;
    xmtp_common::wait_for_eq(|| async {
        sqlx::query_scalar!("SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND database = (SELECT oid FROM pg_database WHERE datname = current_database())")
            .fetch_one(&server.backend.store.primary).await.unwrap()
    }, Some(0)).await?;
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(&server.backend.store.primary)
            .await?,
        Some(0)
    );
    server.stop().await?;
}

#[xmtp_common::timeout(std::time::Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn shutdown_also_ends_a_static_subscription() {
    let mut server = TestServer::new(|_| {}).await?;
    let topic =
        crate::test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[99; 32]);
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let mut stream = client
        .subscribe_static(api::SubscribeStaticRequest {
            topics: vec![crate::test_support::query_topic(topic, 0)],
        })
        .await?
        .into_inner();
    assert!(stream.message().await?.is_some());
    server.shutdown();
    assert_eq!(
        timeout(Duration::from_secs(1), stream.message())
            .await?
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    drop(stream);
    server.stop().await?;
}
