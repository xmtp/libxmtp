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
    assert!(matches!(opts.quorum, Quorum::Absolute(1)));
    assert_eq!(opts.timeout_ms, 30_000);
}

#[xmtp_common::test]
async fn check_node_visibility_returns_not_yet_visible_when_no_envelopes() {
    use xmtp_proto::api_client::{ApiBuilder, NetConnectConfig};
    let mut builder = xmtp_api_grpc::GrpcClient::builder();
    builder.set_host("http://localhost:1".parse().unwrap());
    let client = builder.build().unwrap();

    let cursor = xmtp_proto::types::Cursor::new(1, 1u32);

    let result = check_node_visibility(&client, 1u32, "ab01ab01ab01ab01", &[0u8; 32], cursor).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        crate::client::ClientError::EnvelopesNotYetVisible { node_id } => {
            assert_eq!(node_id, 1u32);
        }
        other => panic!("Expected EnvelopesNotYetVisible, got: {:?}", other),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_wait_for_registration_visible_after_registration() {
    use crate::tester;
    tester!(alice);
    alice
        .wait_for_registration_visible(VisibilityConfirmationOptions::default())
        .await?;
}
