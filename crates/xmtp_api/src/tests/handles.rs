//! Typed handle rejections and handle lookups.

use super::{publish_refusal, wrapper};
use crate::{ApiError, HandleRejection};
use rstest::rstest;
use xmtp_api_backend::MockBackendClient;
use xmtp_common::RetryableError;
use xmtp_proto::{
    backend_v1 as wire, types::ApiIdentifier, xmtp::identity::associations::IdentifierKind,
};

#[rstest]
#[case(wire::publish_error::Reason::HandleTaken, Some(HandleRejection::Taken))]
#[case(
    wire::publish_error::Reason::HandleNotAllowed,
    Some(HandleRejection::NotAllowed)
)]
#[case(
    wire::publish_error::Reason::HandleTooSoon,
    Some(HandleRejection::TooSoon)
)]
#[case(wire::publish_error::Reason::InvalidIdentityUpdate, None)]
#[xmtp_common::test(unwrap_try = true)]
async fn identity_publish_maps_handle_reasons(
    #[case] reason: wire::publish_error::Reason,
    #[case] expected: Option<HandleRejection>,
) {
    let mut mock = MockBackendClient::new();
    mock.expect_publish()
        .times(1)
        .returning(move |_| Err(publish_refusal(reason)));
    let update = xmtp_proto::xmtp::identity::associations::IdentityUpdate {
        inbox_id: "ab".repeat(32),
        ..Default::default()
    };
    let error = wrapper(mock)
        .publish_identity_update(update)
        .await
        .unwrap_err();
    match expected {
        Some(rejection) => {
            assert!(matches!(error, ApiError::HandleRejected(found) if found == rejection));
            assert!(!error.is_retryable());
        }
        None => assert!(matches!(error, ApiError::Api(_)), "{error:?}"),
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn get_inbox_ids_accepts_handle_kind() {
    let mut mock = MockBackendClient::new();
    mock.expect_get_inbox_ids().times(1).returning(|request| {
        assert_eq!(
            request.requests,
            vec![wire::get_inbox_ids_request::Request {
                identifier: "nick#0042".into(),
                identifier_kind: IdentifierKind::Handle as i32,
            }]
        );
        Ok(wire::GetInboxIdsResponse {
            responses: vec![wire::get_inbox_ids_response::Response {
                identifier: "nick#0042".into(),
                identifier_kind: IdentifierKind::Handle as i32,
                inbox_id: Some("ab".repeat(32)),
            }],
        })
    });
    let inbox_ids = wrapper(mock)
        .get_inbox_ids(vec![ApiIdentifier {
            identifier: "nick#0042".into(),
            identifier_kind: IdentifierKind::Handle,
        }])
        .await?;
    assert_eq!(inbox_ids, vec![Some("ab".repeat(32))]);
}
