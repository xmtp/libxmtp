use super::transport::encode_frame;
use crate::{
    api,
    test_support::{TestServer, query_topic, topic},
};
use bytes::Bytes;
use xmtp_common::time::{Duration, timeout};
use xmtp_proto::types::TopicKind;

#[xmtp_common::timeout(std::time::Duration::from_secs(20))]
#[xmtp_common::test(unwrap_try = true)]
async fn shutdown_closes_a_connection_with_a_flow_control_blocked_subscription() {
    let mut server = TestServer::new(|config| config.server.max_drain_duration_ms = 100).await?;
    let socket =
        tokio::net::TcpStream::connect(server.url.strip_prefix("http://").unwrap()).await?;
    let (mut client, connection) = h2::client::Builder::new()
        .initial_window_size(0)
        .handshake::<_, Bytes>(socket)
        .await?;
    let connection = tokio::spawn(connection);
    let request = http::Request::post(format!(
        "{}/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        server.url
    ))
    .header("content-type", "application/grpc")
    .body(())?;
    let topics = (0_u64..10_000)
        .map(|id| {
            let mut identifier = [0; 32];
            identifier[..8].copy_from_slice(&id.to_be_bytes());
            query_topic(topic(TopicKind::WelcomeMessagesV1, &identifier), 0)
        })
        .collect();
    let body = encode_frame(api::SubscribeStaticRequest { topics });
    let (response, mut sender) = client.send_request(request, false)?;
    sender.send_data(body.into(), true)?;
    let mut response = timeout(Duration::from_secs(5), response)
        .await??
        .into_body();
    assert!(
        timeout(Duration::from_millis(100), response.data())
            .await
            .is_err(),
        "the response must remain blocked by the zero receive window"
    );
    server.shutdown();
    timeout(Duration::from_secs(1), server.wait_stopped()).await??;
    // Keep both the stream and client alive. Dropping them would let the server
    // finish even if shutdown left its spawned connection task running.
    let ended = timeout(Duration::from_secs(1), connection).await?;
    let _ = ended?;
    drop(response);
    drop(sender);
    drop(client);
    server.stop().await?;
}
