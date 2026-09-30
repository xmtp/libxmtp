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

fn connection() -> xmtp_db::ConnectionError {
    xmtp_db::ConnectionError::Database(xmtp_db::diesel::result::Error::NotFound)
}

// createGroup, createGroupWithIdentities, and findOrCreateDm return a
// ClientError that wraps the GroupError transparently.
#[xmtp_common::test(unwrap_try = true)]
fn a_group_cause_in_a_client_error_keeps_its_code() {
    expect(
        XmtpError::from_client(ClientError::Group(Box::new(GroupError::UserLimitExceeded))),
        "UserLimitExceeded",
        ErrorCategory::Input,
        false,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn a_store_cause_in_a_client_error_keeps_its_code() {
    use xmtp_mls::mls_store::MlsStoreError;
    expect(
        XmtpError::from_client(ClientError::MlsStore(
            MlsStoreError::Storage(head_changed()),
        )),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn a_store_cause_in_a_group_client_error_keeps_its_code() {
    use xmtp_mls::mls_store::MlsStoreError;
    expect(
        XmtpError::from_group(GroupError::Client(ClientError::MlsStore(
            MlsStoreError::Storage(head_changed()),
        ))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn a_group_cause_in_a_builder_error_keeps_its_code() {
    use xmtp_mls::builder::ClientBuilderError;
    expect(
        XmtpError::from_builder(ClientBuilderError::GroupError(Box::new(
            GroupError::UserLimitExceeded,
        ))),
        "UserLimitExceeded",
        ErrorCategory::Input,
        false,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn a_connection_cause_in_a_client_error_is_storage() {
    let error = XmtpError::from_client(ClientError::Db(connection()));
    assert!(matches!(error, XmtpError::Storage(_)), "{error:?}");
}

#[xmtp_common::test(unwrap_try = true)]
fn a_connection_cause_in_a_group_error_is_storage() {
    let error = XmtpError::from_group(GroupError::Db(connection()));
    assert!(matches!(error, XmtpError::Storage(_)), "{error:?}");
}

// A sync that meets a blocked connection fails with the client's
// configuration error inside the group error.
#[xmtp_common::test(unwrap_try = true)]
fn a_configuration_cause_in_a_group_error_keeps_its_code() {
    expect(
        XmtpError::from_group(GroupError::Client(mismatch())),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
}

// Push processing keeps a configuration code before its notification codes.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
fn push_processing_keeps_a_configuration_code() {
    use xmtp_mls::client::notifications::NotificationError;
    expect(
        XmtpError::from_notification(NotificationError::Group(GroupError::Client(mismatch()))),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
    expect(
        XmtpError::from_notification(NotificationError::Api(preflight(mismatch()))),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
}

// Archive export and import, and device sync, keep a storage cause behind
// their transparent wrappers.
#[xmtp_common::test(unwrap_try = true)]
fn a_storage_cause_in_an_archive_error_keeps_its_retry_policy() {
    use xmtp_mls::worker::device_sync::DeviceSyncError;
    expect(
        XmtpError::from_core(xmtp_archive::ArchiveError::Storage(head_changed())),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
    expect(
        XmtpError::from_group(GroupError::DeviceSync(Box::new(DeviceSyncError::Archive(
            xmtp_archive::ArchiveError::Storage(head_changed()),
        )))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn a_storage_cause_in_a_subscribe_error_keeps_its_retry_policy() {
    use xmtp_mls::{subscriptions::SubscribeError, worker::device_sync::DeviceSyncError};
    expect(
        XmtpError::from_core(SubscribeError::Storage(head_changed())),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
    expect(
        XmtpError::from_group(GroupError::DeviceSync(Box::new(
            DeviceSyncError::Subscribe(SubscribeError::Storage(head_changed())),
        ))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
}

// A data directory bound to another deployment has the backend mismatch
// action: select the bound deployment.
#[xmtp_common::test(unwrap_try = true)]
fn a_data_directory_of_another_deployment_is_backend_mismatch() {
    use xmtp_mls::{builder::ClientBuilderError, storage_location::StorageLocationError};
    expect(
        XmtpError::from_builder(ClientBuilderError::StorageLocation(
            StorageLocationError::DeploymentMismatch,
        )),
        "BackendMismatch",
        ErrorCategory::Configuration,
        false,
    );
}

// Client.catchUpToLive returns a CatchUpError that wraps a GroupError
// transparently.
#[xmtp_common::test(unwrap_try = true)]
fn a_group_cause_in_a_catch_up_error_keeps_its_code() {
    use xmtp_mls::{mls_store::MlsStoreError, subscriptions::catch_up::CatchUpError};
    expect(
        XmtpError::from_core(CatchUpError::Group(GroupError::MlsStore(
            MlsStoreError::Storage(head_changed()),
        ))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
    expect(
        XmtpError::from_core(CatchUpError::Group(GroupError::UserLimitExceeded)),
        "UserLimitExceeded",
        ErrorCategory::Input,
        false,
    );
}

// A sync that failed to wait keeps its storage cause.
#[xmtp_common::test(unwrap_try = true)]
fn a_storage_cause_in_a_failed_sync_wait_keeps_its_retry_policy() {
    let mut summary = xmtp_mls::groups::summary::SyncSummary::default();
    summary.add_other(GroupError::Storage(head_changed()));
    expect(
        XmtpError::from_group(GroupError::SyncFailedToWait(Box::new(summary))),
        "Storage",
        ErrorCategory::Storage,
        true,
    );
}
