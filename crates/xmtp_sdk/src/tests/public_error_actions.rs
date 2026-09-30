//! public_error_actions: each core failure that has a recovery action reaches
//! the app with that action's code, category, and retry policy, through the
//! wrappers that core puts around it.

use crate::{ErrorCategory, ErrorDetails, XmtpError};
use std::sync::Arc;
use xmtp_api::{ApiError, preflight::PreflightError};
use xmtp_db::{StorageError, stream_storage::StreamStorageError};
use xmtp_mls::{
    client::ClientError, groups::GroupError, subscriptions::incoming::IncomingError,
    subscriptions::local_delivery::LocalDeliveryError,
};
use xmtp_proto::api::NetworkError;

fn details(error: &XmtpError) -> &ErrorDetails {
    match error {
        XmtpError::UserLimitExceeded(details)
        | XmtpError::PublishedButUnconfirmed(details)
        | XmtpError::Storage(details)
        | XmtpError::BackendMismatch(details)
        | XmtpError::ClientVersionTooOld(details)
        | XmtpError::ConfigurationUnavailable(details)
        | XmtpError::ClientClosed(details)
        | XmtpError::CredentialMissing(details)
        | XmtpError::Unknown(details) => details,
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[track_caller]
fn expect(error: XmtpError, code: &str, category: ErrorCategory, retryable: bool) {
    let found = details(&error);
    assert_eq!(found.code, code, "{error:?}");
    assert_eq!(
        std::mem::discriminant(&found.category),
        std::mem::discriminant(&category),
        "{error:?}"
    );
    assert_eq!(found.retryable, retryable, "{error:?}");
}

fn mismatch() -> ClientError {
    ClientError::BackendMismatch {
        stored: "deployment-a".into(),
        received: "deployment-b".into(),
    }
}

/// A configuration check before a request, as the API returns it.
fn preflight(cause: ClientError) -> ApiError {
    ApiError::Preflight(PreflightError::new(cause))
}

#[xmtp_common::test(unwrap_try = true)]
fn group_failures_keep_their_actions() {
    expect(
        XmtpError::from_group(GroupError::UserLimitExceeded),
        "UserLimitExceeded",
        ErrorCategory::Input,
        false,
    );
    // Core may retry the confirmation, but the app must not repeat the send.
    expect(
        XmtpError::from_group(GroupError::PublishedButUnconfirmed {
            intent_id: 7,
            cause: None,
        }),
        "PublishedButUnconfirmed",
        ErrorCategory::Conversation,
        false,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn storage_retry_policy_comes_from_the_typed_cause() {
    expect(
        XmtpError::from_group(GroupError::Storage(StorageError::Stream(
            StreamStorageError::HeadChanged,
        ))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
    expect(
        XmtpError::from_group(GroupError::Storage(StorageError::PreTransitionDatabase)),
        "Storage",
        ErrorCategory::Storage,
        false,
    );
    // A storage cause inside a client error inside a group error.
    expect(
        XmtpError::from_group(GroupError::Client(ClientError::Storage(
            StorageError::Stream(StreamStorageError::HeadChanged),
        ))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
}

// verifies: CONF-064, CONF-077
#[xmtp_common::test(unwrap_try = true)]
fn a_failed_check_before_a_request_keeps_its_configuration_code() {
    // A group call whose request failed its configuration check.
    expect(
        XmtpError::from_group(GroupError::WrappedApi(preflight(mismatch()))),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
    // A client call.
    expect(
        XmtpError::from_client(ClientError::Api(preflight(
            ClientError::ClientVersionTooOld {
                client: "1.0.0".into(),
                minimum: "2.0.0".into(),
            },
        ))),
        "ClientVersionTooOld",
        ErrorCategory::Configuration,
        false,
    );
    // An API call, and a closed client found by the check.
    expect(
        XmtpError::from_api(preflight(ClientError::AlreadyClosed)),
        "ClientClosed",
        ErrorCategory::Lifecycle,
        false,
    );
    // A stream whose network recovery failed its configuration check.
    let network = IncomingError::Transport(NetworkError::new(preflight(mismatch())));
    expect(
        crate::delivery::delivery_error(LocalDeliveryError::NetworkFailure(Arc::new(network))),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn a_failure_with_no_typed_cause_is_unknown() {
    expect(
        XmtpError::from_group(GroupError::InvalidGroupMembership),
        "Unknown",
        ErrorCategory::Unknown,
        false,
    );
}

fn head_changed() -> StorageError {
    StorageError::Stream(StreamStorageError::HeadChanged)
}

/// Core wraps a storage cause in `#[error(transparent)]` variants, which
/// forward `source()` past the storage error. The walk still finds it.
#[xmtp_common::test(unwrap_try = true)]
fn a_storage_cause_behind_transparent_wrappers_keeps_its_retry_policy() {
    use xmtp_mls::{
        builder::ClientBuilderError, groups::intents::IntentError, identity::IdentityError,
        mls_store::MlsStoreError,
    };
    let retryable_storage = |error: XmtpError| {
        expect(error, "Storage", ErrorCategory::Storage, true);
    };
    retryable_storage(XmtpError::from_group(GroupError::Intent(
        IntentError::Storage(head_changed()),
    )));
    retryable_storage(XmtpError::from_group(GroupError::MlsStore(
        MlsStoreError::Storage(head_changed()),
    )));
    retryable_storage(XmtpError::from_group(GroupError::Client(
        ClientError::Identity(IdentityError::StorageError(head_changed())),
    )));
    retryable_storage(XmtpError::from_client(ClientError::Identity(
        IdentityError::StorageError(head_changed()),
    )));
    retryable_storage(XmtpError::from_builder(ClientBuilderError::StorageError(
        head_changed(),
    )));
    retryable_storage(XmtpError::from_builder(ClientBuilderError::Identity(
        IdentityError::StorageError(head_changed()),
    )));
}

/// A credential cause behind transparent wrappers keeps its code.
#[xmtp_common::test(unwrap_try = true)]
fn a_credential_cause_behind_transparent_wrappers_keeps_its_code() {
    use xmtp_mls::{builder::ClientBuilderError, identity::IdentityError};
    use xmtp_proto::api::AuthError;
    let missing = || ApiError::Auth(AuthError::MissingCredential);
    expect(
        XmtpError::from_client(ClientError::Identity(IdentityError::ApiClient(missing()))),
        "CredentialMissing",
        ErrorCategory::Callback,
        false,
    );
    expect(
        XmtpError::from_builder(ClientBuilderError::WrappedApiError(missing())),
        "CredentialMissing",
        ErrorCategory::Callback,
        false,
    );
    expect(
        XmtpError::from_group(GroupError::MlsStore(
            xmtp_mls::mls_store::MlsStoreError::Api(missing()),
        )),
        "CredentialMissing",
        ErrorCategory::Callback,
        false,
    );
    // A client error with no source of its own behind a builder error.
    expect(
        XmtpError::from_builder(ClientBuilderError::ClientError(mismatch())),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
}

/// `Unknown` is retryable only when a typed core error says so, and every
/// entry point agrees.
#[xmtp_common::test(unwrap_try = true)]
fn unknown_keeps_the_typed_retry_policy() {
    expect(
        XmtpError::from_group(GroupError::LockUnavailable),
        "Unknown",
        ErrorCategory::Unknown,
        true,
    );
    expect(
        XmtpError::from_core(GroupError::LockUnavailable),
        "Unknown",
        ErrorCategory::Unknown,
        true,
    );
    let client = || ClientError::Group(Box::new(GroupError::LockUnavailable));
    let from_client = XmtpError::from_client(client());
    let from_core = XmtpError::from_core(client());
    assert_eq!(
        details(&from_client).retryable,
        details(&from_core).retryable
    );
    expect(from_client, "Unknown", ErrorCategory::Unknown, true);
    expect(
        XmtpError::from_group(GroupError::InvalidGroupMembership),
        "Unknown",
        ErrorCategory::Unknown,
        false,
    );
}
