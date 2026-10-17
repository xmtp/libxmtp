use super::*;
use std::sync::atomic::AtomicUsize;
use xmtp_api_backend::MockBackendClient;
use xmtp_proto::{api_client::XmtpMlsStreams, types::IncomingBatchLimits};

// No mock call has an expectation: an unguarded target dispatch panics.
#[xmtp_common::test(unwrap_try = true)]
async fn preflight_blocks_every_public_dispatch_before_binding() {
    let api = GuardedApi::new(MockBackendClient::new());
    api.require_preflight();
    macro_rules! blocked {
        ($call:expr) => {
            assert!(failure(&$call.await.err().expect("must fail before dispatch")).is_some());
        };
    }
    blocked!(api.publish(Default::default()));
    blocked!(api.query(Default::default()));
    blocked!(api.query_newest(Default::default()));
    blocked!(api.get_inbox_ids(Default::default()));
    blocked!(api.get_configuration(Default::default()));
    blocked!(api.verify_smart_contract_wallet_signatures(Default::default()));
    blocked!(api.register(Default::default()));
    blocked!(api.unregister(Default::default()));
    blocked!(api.update_subscriptions(Default::default()));
    blocked!(api.subscribe_group_messages(&[]));
    blocked!(api.subscribe_group_messages_with_cursors(&Default::default()));
    blocked!(api.subscribe_welcome_messages(&[]));
    blocked!(api.subscribe_welcome_messages_with_cursors(&Default::default()));
    blocked!(api.subscribe_envelopes_with_cursors(
        &Default::default(),
        IncomingBatchLimits {
            max_rows: 1,
            max_bytes: 1024
        }
    ));
    assert!(api.backend_url().is_none());
    assert!(api.has_credential_source());
}

struct FailOnce(AtomicUsize);
#[xmtp_common::async_trait]
impl RequestPreflight for FailOnce {
    async fn check(&self, _: &dyn ConfigurationFetch) -> std::result::Result<(), PreflightError> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(PreflightError::new(
                xmtp_proto::api::AuthError::CallbackFailed { retryable: true },
            ))
        } else {
            Ok(())
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn preflight_failure_is_returned_once_and_a_new_operation_can_retry() {
    let mut raw = MockBackendClient::new();
    raw.expect_query()
        .times(1)
        .returning(|_| Ok(Default::default()));
    let api = crate::ApiClientWrapper::new(raw, xmtp_common::Retry::default());
    let hook = Arc::new(FailOnce(AtomicUsize::new(0)));
    assert!(api.api_client.bind_preflight(|_| hook.clone()));
    let error = api
        .retry_call(|| api.api_client.query(Default::default()), false)
        .await
        .unwrap_err();
    assert!(error.is_retryable());
    assert!(failure(&error).is_some());
    assert_eq!(hook.0.load(Ordering::SeqCst), 1);
    api.retry_call(|| api.api_client.query(Default::default()), false)
        .await?;
    assert_eq!(hook.0.load(Ordering::SeqCst), 2);
}

#[xmtp_common::test(unwrap_try = true)]
fn preflight_keeps_typed_causes_and_does_not_erase_raw_auth() {
    use std::error::Error;
    let marker =
        PreflightError::new(xmtp_proto::api::AuthError::CallbackFailed { retryable: true });
    let boxed = Box::new(ApiError::Preflight(marker));
    let error = dyn_err(boxed);
    assert!(matches!(&error, ApiError::Preflight(_)));
    let marker = failure(&error).expect("typed marker survives");
    assert!(marker.is_retryable());
    assert!(
        marker
            .source()
            .unwrap()
            .source()
            .unwrap()
            .is::<xmtp_proto::api::AuthError>()
    );
    assert!(matches!(
        dyn_err(Box::new(ApiError::Auth(
            xmtp_proto::api::AuthError::MissingCredential
        ))),
        ApiError::Auth(xmtp_proto::api::AuthError::MissingCredential)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn preflight_size_failure_does_not_resize_the_unsent_target() {
    for code in [tonic::Code::ResourceExhausted, tonic::Code::OutOfRange] {
        let status = tonic::Status::new(code, "configuration request failed");
        assert!(crate::chunk::size_error(&status));
        let error = ApiError::Preflight(PreflightError::new(
            xmtp_proto::api::ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                status,
            )),
        ));
        assert!(!crate::chunk::size_error(&error));
    }
}

struct SizeFailOnce(AtomicUsize);
#[xmtp_common::async_trait]
impl RequestPreflight for SizeFailOnce {
    async fn check(&self, _: &dyn ConfigurationFetch) -> std::result::Result<(), PreflightError> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(PreflightError::new(
                xmtp_proto::api::ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                    tonic::Status::resource_exhausted("configuration request failed"),
                )),
            ))
        } else {
            Ok(())
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn preflight_resource_exhausted_does_not_dispatch_or_resize_target() {
    use xmtp_proto::types::{Cursor, Topic};
    let api = crate::ApiClientWrapper::new(MockBackendClient::new(), xmtp_common::Retry::default());
    let hook = Arc::new(SizeFailOnce(AtomicUsize::new(0)));
    assert!(api.api_client.bind_preflight(|_| hook.clone()));
    // The mock has no query expectation: any target dispatch fails this test.
    let error = api
        .query_all(
            std::collections::HashMap::from([(Topic::new_group_message([1; 16]), Cursor(0))]),
            8,
        )
        .await
        .unwrap_err();
    assert!(failure(&error).is_some());
    assert_eq!(hook.0.load(Ordering::SeqCst), 1);
}
