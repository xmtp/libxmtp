use super::*;

#[test]
fn quorum_percentage_ceiling() {
    let q = Quorum::Percentage(0.5);
    assert_eq!(q.required_count(4), 2);
    assert_eq!(q.required_count(5), 3); // ceil(0.5 * 5) = 3
    assert_eq!(q.required_count(1), 1);
    assert_eq!(q.required_count(0), 0);
}

#[test]
fn quorum_absolute() {
    let q = Quorum::Absolute(3);
    assert_eq!(q.required_count(10), 3);
    assert_eq!(q.required_count(2), 3);
}

#[test]
fn visibility_confirmation_options_defaults() {
    let opts = VisibilityConfirmationOptions::default();
    assert!(matches!(opts.quorum, Quorum::Percentage(p) if (p - 0.5).abs() < f32::EPSILON));
    assert_eq!(opts.timeout_ms, 30_000);
    assert_eq!(opts.sleep_interval_ms, 500);
}

#[xmtp_common::test]
async fn check_node_visibility_times_out_when_no_envelopes() {
    use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
    // This test uses a GrpcClient pointed at a non-existent server
    // to verify the timeout/retry behavior.
    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host("http://localhost:1".parse().unwrap());
    let client = builder.build().unwrap();

    let cursor = Cursor::new(1, 1u32);
    let opts = VisibilityConfirmationOptions {
        timeout_ms: 1_000, // 1 second timeout
        sleep_interval_ms: 200,
        ..Default::default()
    };

    let result =
        check_node_visibility(&client, 1u32, "ab01ab01ab01ab01", &[0u8; 32], cursor, &opts)
            .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        crate::client::ClientError::RegistrationNotVisible { failed_nodes } => {
            assert_eq!(failed_nodes, vec![1u32]);
        }
        other => panic!("Expected RegistrationNotVisible, got: {:?}", other),
    }
}
