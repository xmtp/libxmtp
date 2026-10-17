use super::{ErrorCategory, XmtpError};

#[xmtp_common::test(unwrap_try = true)]
fn metadata_character_limits_are_invalid_arguments() {
    use xmtp_mls::{builder::ClientBuilderError, client::ClientError, groups::GroupError};

    for length in [0, 1, 1024, usize::MAX] {
        let core = || GroupError::TooManyCharacters { length };
        let expected_message = core().to_string();
        for error in [
            XmtpError::from_core(core()),
            XmtpError::from_group(core()),
            XmtpError::from_group_write(core()),
            XmtpError::from_core(ClientError::Group(Box::new(core()))),
            XmtpError::from_core(ClientBuilderError::GroupError(Box::new(core()))),
        ] {
            let XmtpError::InvalidArgument(details) = error else {
                panic!("expected InvalidArgument, got {error:?}");
            };
            assert_eq!(details.code, "InvalidArgument");
            assert!(matches!(details.category, ErrorCategory::Input));
            assert!(!details.retryable);
            assert_eq!(details.message, expected_message);
            assert!(details.stream_failure.is_none());
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn invalid_backend_requests_are_invalid_arguments() {
    use xmtp_api::ApiError;
    use xmtp_mls::{client::ClientError, groups::GroupError};

    for field in ["inbox id", "empty publish unit", "query limit"] {
        let core = || ApiError::InvalidRequest(field);
        let expected_message = core().to_string();
        for error in [
            XmtpError::from_api(core()),
            XmtpError::api_cause(&core()),
            XmtpError::from_group(GroupError::WrappedApi(core())),
            XmtpError::from_group_write(GroupError::WrappedApi(core())),
            XmtpError::from_core(ClientError::Group(Box::new(GroupError::WrappedApi(core())))),
        ] {
            let XmtpError::InvalidArgument(details) = error else {
                panic!("expected InvalidArgument, got {error:?}");
            };
            assert_eq!(details.code, "InvalidArgument");
            assert!(matches!(details.category, ErrorCategory::Input));
            assert!(!details.retryable);
            assert_eq!(details.message, expected_message);
            assert!(details.stream_failure.is_none());
        }
    }
    let XmtpError::Unknown(details) = XmtpError::from_api(ApiError::InvalidResponse("inbox id"))
    else {
        panic!("response failures must keep their current category");
    };
    assert!(matches!(details.category, ErrorCategory::Network));
    assert!(!details.retryable);
}

/// A write keeps the credential recovery hint.
/// Storage and unknown transport failures do not permit a whole-call retry.
// verifies: AUTH-026
#[xmtp_common::test(unwrap_try = true)]
fn group_writes_keep_typed_recovery_hints() {
    use xmtp_api::ApiError;
    use xmtp_mls::groups::GroupError;
    use xmtp_proto::api::{AuthError, NetworkError};

    for (auth, retryable) in [
        (AuthError::CredentialRejected { retryable: true }, true),
        (AuthError::CredentialRejected { retryable: false }, false),
        (AuthError::CallbackFailed { retryable: true }, true),
        (AuthError::MissingCredential, false),
        (AuthError::Exhausted, false),
    ] {
        for write in [false, true] {
            let core = GroupError::WrappedApi(ApiError::Auth(auth));
            let error = if write {
                XmtpError::from_group_write(core)
            } else {
                XmtpError::from_group(core)
            };
            let details = match (auth, error) {
                (AuthError::CredentialRejected { .. }, XmtpError::CredentialRejected(details))
                | (
                    AuthError::CallbackFailed { .. },
                    XmtpError::CredentialCallbackFailed(details),
                )
                | (AuthError::MissingCredential, XmtpError::CredentialMissing(details))
                | (AuthError::Exhausted, XmtpError::CredentialExhausted(details)) => details,
                (_, other) => panic!("unexpected error {other:?}"),
            };
            assert!(matches!(details.category, ErrorCategory::Callback));
            assert_eq!(details.retryable, retryable);
        }
    }
    for write in [false, true] {
        let core = GroupError::WrappedApi(ApiError::Api(NetworkError::new(
            GroupError::LockUnavailable,
        )));
        let error = if write {
            XmtpError::from_group_write(core)
        } else {
            XmtpError::from_group(core)
        };
        let XmtpError::Unknown(details) = error else {
            panic!("unexpected error {error:?}");
        };
        assert_eq!(details.code, "Unknown");
        assert!(matches!(details.category, ErrorCategory::Network));
        assert_eq!(details.retryable, !write);
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn storage_busy_matches_browser_bridge_fields() {
    let XmtpError::StorageBusy(details) = XmtpError::storage_busy("busy") else {
        panic!("expected StorageBusy");
    };
    assert_eq!(details.code, "StorageBusy");
    assert!(matches!(details.category, ErrorCategory::Storage));
    assert!(details.retryable);
}

/// Each metadata field error keeps its kind as a stable code and
/// category, and none is retryable. A component-source error reads the
/// same whether core wraps it in a field error or not.
#[xmtp_common::test(unwrap_try = true)]
fn field_errors_keep_their_kind() {
    use xmtp_mls::{
        groups::GroupError,
        mls_common::app_data::{
            component_id::ComponentId,
            component_source::ComponentSourceError,
            fields::{FieldError, MetadataComponentType},
        },
    };
    use xmtp_proto::xmtp::mls::message_contents::ComponentType;

    let id = ComponentId::new(0xC001);
    let immutable = || ComponentSourceError::ImmutableUpdate(id);
    let kinds: Vec<_> = [
        FieldError::UnknownField(id),
        FieldError::NotUserField(id),
        FieldError::DuplicateField(id),
        FieldError::UnsupportedType {
            component_id: id,
            tag: 99,
        },
        FieldError::TypeMismatch(id),
        FieldError::TypeChanged {
            component_id: id,
            expected: ComponentType::String,
            actual: MetadataComponentType::Bytes,
        },
        FieldError::Denied(id),
        FieldError::Component(immutable()),
    ]
    .into_iter()
    .map(GroupError::MetadataField)
    .chain([GroupError::ComponentSource(immutable())])
    .map(|error| {
        let error = XmtpError::from_group(error);
        let (variant, details) = match error {
            XmtpError::UnknownField(details) => ("UnknownField", details),
            XmtpError::NotUserField(details) => ("NotUserField", details),
            XmtpError::DuplicateField(details) => ("DuplicateField", details),
            XmtpError::UnsupportedType(details) => ("UnsupportedType", details),
            XmtpError::TypeMismatch(details) => ("TypeMismatch", details),
            XmtpError::TypeChanged(details) => ("TypeChanged", details),
            XmtpError::PermissionDenied(details) => ("PermissionDenied", details),
            XmtpError::Unknown(details) => ("Unknown", details),
            other => panic!("unexpected error {other:?}"),
        };
        (
            variant.to_owned(),
            details.code,
            format!("{:?}", details.category),
            details.retryable,
        )
    })
    .collect();
    let kind = |variant: &str, code: &str, category: &str| {
        (
            variant.to_owned(),
            code.to_owned(),
            category.to_owned(),
            false,
        )
    };
    assert_eq!(
        kinds,
        [
            kind("UnknownField", "UnknownField", "Input"),
            kind("NotUserField", "NotUserField", "Input"),
            kind("DuplicateField", "DuplicateField", "Input"),
            kind("UnsupportedType", "UnsupportedType", "Conversation"),
            kind("TypeMismatch", "TypeMismatch", "Input"),
            kind("TypeChanged", "TypeChanged", "Conversation"),
            kind("PermissionDenied", "PermissionDenied", "Conversation"),
            kind("Unknown", "Unknown", "Conversation"),
            kind("Unknown", "Unknown", "Conversation"),
        ]
    );
}

/// A database failure is `Storage`. A read keeps the typed cause's
/// retry policy, except that a missing row or a value that cannot be
/// encoded, decoded or built into a query is not retryable, wherever the
/// query error is carried. A storage failure from a send or commit is not
/// retryable, because the call may already have queued or published its work.
/// An unconfirmed publish keeps its typed variant and is not retryable.
#[xmtp_common::test(unwrap_try = true)]
fn group_errors_keep_storage_retry_policy_on_reads_only() {
    use xmtp_db::{
        ConnectionError, PlatformStorageError, StorageError,
        diesel::result::{DatabaseErrorKind, Error as DieselError},
        sql_key_store::SqlKeyStoreError,
    };
    use xmtp_mls::groups::GroupError;

    let unconfirmed = || GroupError::PublishedButUnconfirmed {
        intent_id: 1,
        cause: None,
    };
    for error in [
        XmtpError::from_group(unconfirmed()),
        XmtpError::from_group_write(unconfirmed()),
    ] {
        let XmtpError::PublishedButUnconfirmed(details) = error else {
            panic!("unexpected error {error:?}");
        };
        assert_eq!(details.code, "PublishedButUnconfirmed");
        assert!(matches!(details.category, ErrorCategory::Conversation));
        assert!(!details.retryable);
    }
    let unique =
        || DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, Box::new(String::new()));
    let closed =
        || DieselError::DatabaseError(DatabaseErrorKind::ClosedConnection, Box::new(String::new()));

    // Each case with its read result: `Some(retryable)` is `Storage`,
    // `None` is `Unknown`.
    let cases = || {
        [
            (
                GroupError::Storage(StorageError::PreTransitionDatabase),
                Some(false),
            ),
            (
                GroupError::Storage(StorageError::Connection(
                    ConnectionError::DisconnectInTransaction,
                )),
                Some(true),
            ),
            (
                GroupError::Storage(StorageError::DieselResult(closed())),
                Some(true),
            ),
            (
                GroupError::Storage(StorageError::DieselResult(DieselError::NotFound)),
                Some(false),
            ),
            (
                GroupError::Storage(StorageError::Connection(ConnectionError::Database(
                    DieselError::NotFound,
                ))),
                Some(false),
            ),
            (
                GroupError::Storage(StorageError::Platform(PlatformStorageError::DieselResult(
                    closed(),
                ))),
                Some(true),
            ),
            (
                GroupError::Storage(StorageError::Platform(PlatformStorageError::DieselResult(
                    DieselError::NotFound,
                ))),
                Some(false),
            ),
            (
                GroupError::Storage(StorageError::OpenMlsStorage(SqlKeyStoreError::Storage(
                    closed(),
                ))),
                Some(true),
            ),
            (
                GroupError::Storage(StorageError::OpenMlsStorage(SqlKeyStoreError::Storage(
                    DieselError::NotFound,
                ))),
                Some(false),
            ),
            (
                GroupError::Db(ConnectionError::DisconnectInTransaction),
                Some(true),
            ),
            (
                GroupError::Db(ConnectionError::Platform(
                    PlatformStorageError::DieselResult(DieselError::DeserializationError(
                        "bad row".into(),
                    )),
                )),
                Some(false),
            ),
            (
                GroupError::Db(ConnectionError::Database(unique())),
                Some(false),
            ),
            (
                GroupError::Db(ConnectionError::Database(DieselError::NotFound)),
                Some(false),
            ),
            (GroupError::Diesel(closed()), Some(true)),
            (GroupError::Diesel(DieselError::NotFound), Some(false)),
            (
                GroupError::Diesel(DieselError::DeserializationError("bad row".into())),
                Some(false),
            ),
            (
                GroupError::Diesel(DieselError::SerializationError("bad value".into())),
                Some(false),
            ),
            (
                GroupError::Diesel(DieselError::QueryBuilderError("bad query".into())),
                Some(false),
            ),
            (GroupError::Diesel(unique()), Some(false)),
            (
                GroupError::SqlKeyStore(SqlKeyStoreError::Connection(
                    ConnectionError::DisconnectInTransaction,
                )),
                Some(true),
            ),
            (
                GroupError::SqlKeyStore(SqlKeyStoreError::Connection(ConnectionError::Database(
                    DieselError::NotFound,
                ))),
                Some(false),
            ),
            (
                GroupError::SqlKeyStore(SqlKeyStoreError::Connection(ConnectionError::Database(
                    DieselError::DeserializationError("bad row".into()),
                ))),
                Some(false),
            ),
            (
                GroupError::SqlKeyStore(SqlKeyStoreError::Storage(closed())),
                Some(true),
            ),
            (
                GroupError::SqlKeyStore(SqlKeyStoreError::Storage(unique())),
                Some(false),
            ),
            (
                GroupError::SqlKeyStore(SqlKeyStoreError::Storage(DieselError::NotFound)),
                Some(false),
            ),
            (GroupError::SqlKeyStore(SqlKeyStoreError::NotFound), None),
            (GroupError::SyncFailedToWait(Box::default()), None),
            (GroupError::GroupInactive, None),
        ]
    };
    let kind = |error| match error {
        XmtpError::Storage(details) => ("Storage", details),
        XmtpError::Unknown(details) => ("Unknown", details),
        other => panic!("unexpected error {other:?}"),
    };
    let kind = |error| {
        let (variant, details) = kind(error);
        (
            variant,
            details.code,
            format!("{:?}", details.category),
            details.retryable,
        )
    };
    let expected = |read: Option<bool>| match read {
        Some(retryable) => ("Storage", "Storage".into(), "Storage".into(), retryable),
        None => ("Unknown", "Unknown".into(), "Unknown".into(), false),
    };
    for (error, read) in cases() {
        let case = format!("{error:?}");
        assert_eq!(kind(XmtpError::from_group(error)), expected(read), "{case}");
    }
    for (error, read) in cases() {
        let case = format!("{error:?}");
        assert_eq!(
            kind(XmtpError::from_group_write(error)),
            expected(read.map(|_| false)),
            "{case}"
        );
    }
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
fn reserved_transcript_type_is_a_stable_input_error() {
    use xmtp_mls::groups::GroupError;

    let XmtpError::InvalidInput(refused) =
        XmtpError::from_group(GroupError::ReservedTranscriptContentType)
    else {
        panic!("expected InvalidInput");
    };
    assert_eq!(refused.code, "ReservedTranscriptContentType");
    assert!(matches!(refused.category, ErrorCategory::Input));
    assert!(!refused.retryable);
}

#[xmtp_common::test(unwrap_try = true)]
fn construction_causes_keep_their_typed_action() {
    use xmtp_mls::{
        attachments::AttachmentClientError, builder::ClientBuilderError, identity::IdentityError,
        storage_location::StorageLocationError,
    };
    let error = XmtpError::from_builder(ClientBuilderError::Identity(
        IdentityError::IdentifierNotInInbox {
            inbox_id: "12".repeat(32),
        },
    ));
    let XmtpError::IdentityMismatch(details) = error else {
        panic!("expected IdentityMismatch, got {error:?}");
    };
    assert_eq!(details.code, "IdentityMismatch");
    assert!(matches!(details.category, ErrorCategory::Identity));
    assert!(!details.retryable);

    for error in [
        ClientBuilderError::StorageLocation(StorageLocationError::MissingPath { field: "db_path" }),
        ClientBuilderError::StorageLocation(StorageLocationError::Opfs),
        ClientBuilderError::Attachment(AttachmentClientError {
            cause: xmtp_attachments::AttachmentFailureCause::LocalStorage,
            credential_kind: None,
            retryable: false,
            missing_scope: false,
            http_status: None,
        }),
    ] {
        let error = XmtpError::from_builder(error);
        let XmtpError::StorageLocation(details) = error else {
            panic!("expected StorageLocation, got {error:?}");
        };
        assert_eq!(details.code, "StorageLocation");
        assert!(matches!(details.category, ErrorCategory::Storage));
        assert!(!details.retryable);
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn commit_permission_causes_remain_typed() {
    use xmtp_mls::{
        client::ClientError,
        groups::{GroupError, summary::SyncSummary, validated_commit::CommitValidationError},
        mls_validation::commit::CommitRuleError,
    };

    let core = || {
        GroupError::CommitValidation(CommitValidationError::Rule(
            CommitRuleError::InsufficientPermissions,
        ))
    };
    let mut summary = SyncSummary::default();
    summary.add_publish_err(core());
    for error in [
        XmtpError::from_group(core()),
        XmtpError::from_group_write(core()),
        XmtpError::from_core(ClientError::Group(Box::new(core()))),
        XmtpError::from_group_write(GroupError::Sync(Box::new(summary))),
    ] {
        let XmtpError::PermissionDenied(details) = error else {
            panic!("expected PermissionDenied, got {error:?}");
        };
        assert_eq!(details.code, "PermissionDenied");
        assert!(matches!(details.category, ErrorCategory::Conversation));
        assert!(!details.retryable);
        assert_eq!(details.message, "Insufficient permissions");
    }
}
