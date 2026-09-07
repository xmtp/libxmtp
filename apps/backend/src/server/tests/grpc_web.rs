use crate::{
    api,
    test_support::{
        self, TestServer,
        grpc_web::{self, WebStream},
        native::envelope,
    },
};
use api::subscribe_static_response::Response as Frame;

#[xmtp_common::test(unwrap_try = true)]
async fn direct_grpc_web_static_subscription_delivers_incrementally_with_cors_headers() {
    let server = TestServer::new(|config| config.streams.poll_interval_ms = 10).await?;
    let meta = server.publish(vec![envelope(97, 1)]).await?.remove(0);
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("origin", "https://client.example".parse()?);
    headers.insert("authorization", "test-token".parse()?);
    headers.insert("x-client-version", "test-client".parse()?);
    let client = xmtp_common::http::client_builder()
        .default_headers(headers)
        .build()?;
    let mut web: WebStream<api::SubscribeStaticResponse> = grpc_web::open(
        &client,
        &server.url,
        "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        api::SubscribeStaticRequest {
            topics: vec![test_support::query_topic(meta.topic.clone().unwrap(), 0)],
        },
    )
    .await?;
    assert_eq!(web.headers["access-control-allow-origin"], "*");
    assert!(
        web.headers["access-control-expose-headers"]
            .to_str()?
            .contains("x-request-id")
    );
    uuid::Uuid::parse_str(web.headers["x-request-id"].to_str()?)?;
    assert!(
        matches!(web.stream.message().await?.unwrap().response, Some(Frame::Started(started)) if started.targets[0].through_sequence_id == 1)
    );
    assert!(
        matches!(web.stream.message().await?.unwrap().response, Some(Frame::Messages(messages)) if messages.envelopes[0].meta == Some(meta))
    );
    let later = server.publish(vec![envelope(97, 2)]).await?.remove(0);
    let frame = xmtp_common::time::timeout(
        xmtp_common::time::Duration::from_secs(5),
        web.stream.message(),
    )
    .await??
    .unwrap();
    assert!(
        matches!(frame.response, Some(Frame::Messages(messages)) if messages.envelopes[0].meta == Some(later))
    );
    drop(web);
    server.stop().await?;
}
