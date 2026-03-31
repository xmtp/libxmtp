use super::*;

#[xmtp_common::test]
fn quorum_percentage_ceiling() {
    let q = Quorum::Percentage(0.5);
    assert_eq!(q.required_count(4), 2);
    assert_eq!(q.required_count(5), 3); // ceil(0.5 * 5) = 3
    assert_eq!(q.required_count(1), 1);
    assert_eq!(q.required_count(0), 0);
}

#[xmtp_common::test]
fn quorum_absolute() {
    let q = Quorum::Absolute(3);
    assert_eq!(q.required_count(10), 3);
    assert_eq!(q.required_count(2), 3);
}

#[xmtp_common::test]
fn visibility_confirmation_options_defaults() {
    let opts = VisibilityConfirmationOptions::default();
    assert!(matches!(opts.quorum, Quorum::Absolute(1)));
    assert_eq!(opts.timeout_ms, 30_000);
}

#[xmtp_common::test]
async fn check_node_visibility_returns_not_yet_visible_when_no_envelopes() {
    use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
    use xmtp_proto::types::Topic;

    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host("http://localhost:1".parse().unwrap());
    let client = builder.build().unwrap();

    let cursor = xmtp_proto::types::Cursor::new(1, 1u32);
    let inbox_id_bytes = hex::decode("ab01ab01ab01ab01").unwrap();
    let topics = vec![
        Topic::new_identity_update(&inbox_id_bytes).cloned_vec(),
        Topic::new_key_package([0u8; 32]).cloned_vec(),
    ];

    let result = check_node_visibility(&client, 1u32, &topics, cursor).await;

    assert!(matches!(
        result,
        Err(crate::client::ClientError::EnvelopesNotYetVisible { node_id: 1 })
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_wait_for_registration_visible_after_registration() {
    use crate::tester;
    tester!(alice);
    alice
        .wait_for_registration_visible(VisibilityConfirmationOptions::default())
        .await?;
}

#[cfg(feature = "d14n")]
#[xmtp_common::test(unwrap_try = true)]
async fn test_wait_for_registration_visible_fails_when_network_severed() {
    use crate::tester;
    use xmtp_common::toxiproxy_test;

    toxiproxy_test(async || {
        tester!(alice, proxy);

        // Disable all proxies (both xmtpd and gateway) after registration.
        // We can't selectively disable only the xmtpd proxy here because
        // poll_node_quorum calls get_node_clients(), which queries GetNodes via
        // the gateway and builds fresh GrpcClients that connect directly to the
        // real node addresses — bypassing toxiproxy entirely.
        // TODO: figure out how to proxy the direct node connections created by
        // get_node_clients() so we can test xmtpd failures independently of
        // the gateway.
        // Sleep briefly after disable to let cached HTTP/2 connections drop.
        alice
            .for_each_proxy(async |p| {
                p.disable().await.unwrap();
            })
            .await;
        xmtp_common::time::sleep(xmtp_common::time::Duration::from_millis(500)).await;

        let result = alice
            .wait_for_registration_visible(VisibilityConfirmationOptions {
                quorum: Quorum::Absolute(1),
                timeout_ms: 3_000,
            })
            .await;

        assert!(result.is_err());

        Result::<(), crate::client::ClientError>::Ok(())
    })
    .await?;
}
