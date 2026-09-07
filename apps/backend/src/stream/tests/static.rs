use crate::{
    api,
    test_support::{self, TestServer, native::envelope},
};
use api::subscribe_static_response::Response as Frame;

#[xmtp_common::test(unwrap_try = true)]
async fn static_targets_precede_ordered_history_and_live_delivery() {
    let server = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let metas = server
        .publish(vec![envelope(91, 1), envelope(91, 2)])
        .await?;
    let topic = metas[0].topic.clone().unwrap();
    let empty = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[92; 32]);
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let mut stream = client
        .subscribe_static(api::SubscribeStaticRequest {
            topics: vec![
                test_support::query_topic(empty.clone(), 0),
                test_support::query_topic(topic.clone(), 0),
            ],
        })
        .await?
        .into_inner();
    let Some(Frame::Started(started)) = stream.message().await?.unwrap().response else {
        panic!("expected targets first");
    };
    assert_eq!(
        started.targets,
        vec![
            api::CatchupTarget {
                topic: Some(empty),
                through_sequence_id: 0
            },
            api::CatchupTarget {
                topic: Some(topic),
                through_sequence_id: 2
            }
        ]
    );
    let Some(Frame::Messages(history)) = stream.message().await?.unwrap().response else {
        panic!("expected history");
    };
    assert_eq!(
        history
            .envelopes
            .iter()
            .map(|row| row
                .meta
                .as_ref()
                .unwrap()
                .cursor
                .as_ref()
                .unwrap()
                .sequence_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let later = server.publish(vec![envelope(91, 3)]).await?.remove(0);
    let Some(Frame::Messages(live)) = stream.message().await?.unwrap().response else {
        panic!("expected live delivery after unary request completion");
    };
    assert_eq!(live.envelopes[0].meta, Some(later));
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_keepalives_are_one_way_and_do_not_expire_waiting_for_pong() {
    let server = TestServer::new(|config| {
        config.streams.keepalive_interval_ms = 10;
        config.streams.max_pong_wait_ms = 20;
    })
    .await?;
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[93; 32]);
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let mut stream = client
        .subscribe_static(api::SubscribeStaticRequest {
            topics: vec![test_support::query_topic(topic, 0)],
        })
        .await?
        .into_inner();
    assert!(matches!(
        stream.message().await?.unwrap().response,
        Some(Frame::Started(_))
    ));
    for _ in 0..4 {
        assert!(matches!(
            stream.message().await?.unwrap().response,
            Some(Frame::Keepalive(_))
        ));
    }
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_request_validates_its_own_limits_and_unique_topics() {
    let server = TestServer::new(|config| {
        config.limits.max_stream_topics = 1;
        config.limits.max_update_adds = 1;
        config.limits.max_static_topics = 2;
    })
    .await?;
    let a = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[94; 32]);
    let b = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[95; 32]);
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    for topics in [
        vec![],
        vec![test_support::query_topic(a.clone(), 0); 2],
        vec![test_support::query_topic(a.clone(), 0); 3],
        vec![test_support::query_topic(a.clone(), u64::MAX)],
        vec![api::TopicQuery::default()],
    ] {
        assert_eq!(
            client
                .subscribe_static(api::SubscribeStaticRequest { topics })
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }
    let mut stream = client
        .subscribe_static(api::SubscribeStaticRequest {
            topics: vec![
                test_support::query_topic(a, 0),
                test_support::query_topic(b, 0),
            ],
        })
        .await?
        .into_inner();
    assert!(
        matches!(stream.message().await?.unwrap().response, Some(Frame::Started(started)) if started.targets.len() == 2)
    );
    drop(stream);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_initial_log_and_completion_share_request_id_and_body_counts() {
    use prost::Message;
    use tower::Layer;
    use tracing::instrument::WithSubscriber;
    use xmtp_logging::{Level, test_logging::LogCapture};
    let server = TestServer::new(|_| {}).await?;
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[101; 32]);
    let request = api::SubscribeStaticRequest {
        topics: vec![test_support::query_topic(topic, 0)],
    };
    let bytes = request.encoded_len() + 5;
    let inner =
        api::subscription_service_server::SubscriptionServiceServer::new(server.backend.clone());
    let layer = crate::server::request_logger::RequestLoggerLayer(true).layer(inner);
    let mut client = tonic::client::Grpc::new(layer);
    let capture = LogCapture::new(Level::Info);
    let response = client.server_streaming(tonic::Request::new(request), "/xmtp.backend.v1.SubscriptionService/SubscribeStatic".parse()?,
        tonic_prost::ProstCodec::<api::SubscribeStaticRequest, api::SubscribeStaticResponse>::default()).with_subscriber(capture.dispatch()).await?;
    let request_id = response
        .metadata()
        .get("x-request-id")
        .unwrap()
        .to_str()?
        .to_owned();
    let mut stream = response.into_inner();
    assert!(matches!(
        stream.message().await?.unwrap().response,
        Some(Frame::Started(_))
    ));
    drop(stream);
    let logs: Vec<serde_json::Value> = capture
        .output()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0]["message"], "static subscription started");
    assert_eq!(logs[0]["added_topics"], 1);
    assert_eq!(logs[0]["request_id"], request_id);
    assert_eq!(logs[1]["request_id"], request_id);
    assert_eq!(logs[1]["request_size_bytes"], bytes);
    assert!(logs[1]["response_size_bytes"].as_u64().unwrap() > 0);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_started_waits_for_target_capture_before_sending_keepalives() {
    let server = TestServer::new(|config| {
        config.streams.keepalive_interval_ms = 10;
        config.streams.max_pong_wait_ms = 20;
    })
    .await?;
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query!("LOCK TABLE topic_watermark IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await?;
    let topic = test_support::topic(xmtp_proto::types::TopicKind::WelcomeMessagesV1, &[102; 32]);
    let mut client =
        api::subscription_service_client::SubscriptionServiceClient::new(server.channel.clone());
    let mut stream = client
        .subscribe_static(api::SubscribeStaticRequest {
            topics: vec![test_support::query_topic(topic, 0)],
        })
        .await?
        .into_inner();
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE datname = current_database()
            AND wait_event_type = 'Lock' AND query LIKE 'SELECT COALESCE%'
            AND clock_timestamp() - query_start > interval '50 milliseconds') AS "waiting!""#
            )
            .fetch_one(&server.backend.store.primary)
            .await
            .unwrap()
        },
        true,
    )
    .await?;
    blocker.commit().await?;
    assert!(matches!(
        stream.message().await?.unwrap().response,
        Some(Frame::Started(_))
    ));
    drop(stream);
    server.stop().await?;
}
