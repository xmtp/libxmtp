//! Attachment runtime pieces that need no client.

use super::*;

// verifies: ATCH-061
#[xmtp_common::test(unwrap_try = true)]
fn credential_kind_kept() {
    use xmtp_proto::api::AuthError;
    let cases = [
        (
            AuthError::CallbackFailed { retryable: true },
            CredentialFailureKind::CallbackFailed,
            true,
        ),
        (
            AuthError::Exhausted,
            CredentialFailureKind::Exhausted,
            false,
        ),
        (
            AuthError::ExhaustedAfterAttempt,
            CredentialFailureKind::Exhausted,
            false,
        ),
    ];
    for (auth, kind, retryable) in cases {
        let error = api_error(xmtp_api::ApiError::Auth(auth));
        assert_eq!(error.cause, Cause::Credential);
        assert_eq!(error.credential_kind, Some(kind));
        assert_eq!(error.retryable, retryable);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn weak_registry_prunes_dropped_entries() {
    let runtime = AttachmentRuntime::default();
    let live_pending = runtime.shared("live");
    let live_lock = runtime.event_lock("live");
    for number in 0..128 {
        drop(runtime.shared(&format!("pending-{number}")));
        drop(runtime.event_lock(&format!("event-{number}")));
    }
    let new_pending = runtime.shared("new");
    let new_lock = runtime.event_lock("new");
    assert_eq!(runtime.pending.lock().len(), 2);
    assert_eq!(runtime.event_locks.lock().len(), 2);
    assert!(Arc::ptr_eq(&live_pending, &runtime.shared("live")));
    assert!(Arc::ptr_eq(&live_lock, &runtime.event_lock("live")));
    drop((new_pending, new_lock));
}

/// A rate limit is a transient backend condition.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
fn a_rate_limit_maps_to_a_transient_cause() {
    use xmtp_proto::api::ApiClientError;
    let limited = xmtp_api_grpc::error::rate_limited_status(
        "upload",
        xmtp_common::time::Duration::from_secs(5),
    );
    let error = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(limited));
    let mapped = api_error(xmtp_api::dyn_err(error));
    assert_eq!(mapped.cause, Cause::BackendUnavailable);
    assert!(!mapped.missing_scope);
}

// verifies: ATCH-060, ATCH-061
#[xmtp_common::test(unwrap_try = true)]
fn causes_map() {
    use xmtp_proto::api::{ApiClientError, AuthError};
    let credential = api_error(xmtp_api::ApiError::Auth(AuthError::CredentialRejected {
        retryable: true,
    }));
    assert_eq!(credential.cause, Cause::Credential);
    assert_eq!(
        credential.credential_kind,
        Some(CredentialFailureKind::CredentialRejected)
    );
    assert!(credential.retryable);
    let no_credential = api_error(xmtp_api::ApiError::Auth(AuthError::MissingCredential));
    assert_eq!(
        no_credential.credential_kind,
        Some(CredentialFailureKind::MissingCredential)
    );
    assert!(!no_credential.retryable);
    for code in [
        tonic::Code::InvalidArgument,
        tonic::Code::OutOfRange,
        tonic::Code::Unimplemented,
    ] {
        let network = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
            tonic::Status::new(code, "rejected"),
        ));
        assert_eq!(
            api_error(xmtp_api::dyn_err(network)).cause,
            Cause::BackendRejected
        );
    }
    // FAILED_PRECONDITION and INTERNAL clear when the operator corrects the backend.
    for code in [
        tonic::Code::Unavailable,
        tonic::Code::FailedPrecondition,
        tonic::Code::Internal,
    ] {
        let unavailable = ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
            tonic::Status::new(code, "not signed"),
        ));
        assert_eq!(
            api_error(xmtp_api::dyn_err(unavailable)).cause,
            Cause::BackendUnavailable
        );
    }
    let target = AttachmentClientError::from(AttachmentError::new(Cause::TargetRejected));
    assert_eq!(target.cause, Cause::TargetRejected);
}

// verifies: ATCH-061
#[xmtp_common::test(unwrap_try = true)]
fn credential_failure_kind_strings_round_trip() {
    fn index(kind: CredentialFailureKind) -> usize {
        match kind {
            CredentialFailureKind::CredentialRejected => 0,
            CredentialFailureKind::CallbackFailed => 1,
            CredentialFailureKind::Exhausted => 2,
            CredentialFailureKind::MissingCredential => 3,
        }
    }
    for (expected, kind) in CredentialFailureKind::ALL.into_iter().enumerate() {
        assert_eq!(index(kind), expected);
        assert_eq!(CredentialFailureKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(
        index(CredentialFailureKind::MissingCredential) + 1,
        CredentialFailureKind::ALL.len()
    );
    assert_eq!(CredentialFailureKind::parse("unknown"), None);
}
