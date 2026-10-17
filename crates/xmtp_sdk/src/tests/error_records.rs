use super::*;

#[xmtp_common::test]
fn notification_and_auth_errors_keep_their_kinds() {
    use crate::ErrorCategory;
    use xmtp_mls::client::notifications::NotificationError as N;
    use xmtp_proto::api::AuthError as A;

    macro_rules! notification {
        ($source:expr, $variant:ident, $category:pat, $retryable:expr) => {{
            let mapped = XmtpError::from_notification($source);
            let XmtpError::$variant(details) = mapped else {
                panic!("notification error became {mapped:?}");
            };
            assert_eq!(details.code, stringify!($variant));
            assert!(matches!(details.category, $category));
            assert_eq!(details.retryable, $retryable);
        }};
    }
    notification!(
        N::TaskRunnerDisabled,
        TaskRunnerDisabled,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::PermissionDenied,
        PermissionDenied,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::InvalidArgument,
        InvalidArgument,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::OutOfRange,
        OutOfRange,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::Unimplemented,
        Unimplemented,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::ChannelNotConfigured,
        ChannelNotConfigured,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::ResourceExhausted,
        ResourceExhausted,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::RequestTimeout,
        RequestTimeout,
        ErrorCategory::Notification,
        true
    );
    notification!(
        N::NotFound,
        NotificationNotFound,
        ErrorCategory::Notification,
        true
    );
    notification!(
        N::Api(xmtp_api::ApiError::InvalidRequest("test")),
        NotificationApi,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::Storage(xmtp_db::StorageError::DbDeserialize),
        NotificationStorage,
        ErrorCategory::Storage,
        false
    );
    notification!(
        N::Group(xmtp_mls::groups::GroupError::UserLimitExceeded),
        NotificationGroup,
        ErrorCategory::Conversation,
        false
    );

    let auth_cases = [
        (
            A::CredentialRejected { retryable: true },
            "CredentialRejected",
            true,
        ),
        (
            A::CallbackFailed { retryable: true },
            "CredentialCallbackFailed",
            true,
        ),
        (A::Exhausted, "CredentialExhausted", false),
        (A::ExhaustedAfterAttempt, "CredentialExhausted", false),
        (A::MissingCredential, "CredentialMissing", false),
    ];
    for (source, code, retryable) in auth_cases {
        let mapped = XmtpError::from_api(xmtp_api::ApiError::Auth(source));
        let details = match mapped {
            XmtpError::CredentialRejected(details)
            | XmtpError::CredentialCallbackFailed(details)
            | XmtpError::CredentialExhausted(details)
            | XmtpError::CredentialMissing(details) => details,
            other => panic!("auth error became {other:?}"),
        };
        assert_eq!(details.code, code);
        assert!(matches!(details.category, ErrorCategory::Callback));
        assert_eq!(details.retryable, retryable);
    }
    let nested = xmtp_mls::builder::ClientBuilderError::Identity(
        xmtp_mls::identity::IdentityError::ApiClient(xmtp_api::ApiError::Auth(A::CallbackFailed {
            retryable: true,
        })),
    );
    assert!(matches!(
        XmtpError::from_builder(nested),
        XmtpError::CredentialCallbackFailed(details) if details.retryable
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn out_of_range_installation_time_does_not_fail_inbox_state() {
    use xmtp_id::associations::{AssociationState, Identifier, Member, MemberIdentifier};
    let owner = Identifier::eth("0x1111111111111111111111111111111111111111")?;
    let installation = MemberIdentifier::installation(vec![1; 32]);
    let state = AssociationState::new(owner, 0, None)?.add(Member::new(
        installation,
        None,
        Some(u64::MAX),
        None,
    ));
    let state = crate::InboxState::from_core(state, None)?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(
        state.installations[0].created_at,
        Some(crate::Timestamp(i64::MAX))
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn inbox_state_preserves_member_creation_order() {
    use xmtp_id::associations::{AssociationState, Identifier, Member};

    let owner = Identifier::eth("0x1111111111111111111111111111111111111111")?;
    let early = Identifier::eth("0x2222222222222222222222222222222222222222")?;
    let late = Identifier::eth("0x3333333333333333333333333333333333333333")?;
    let state = AssociationState::new(owner.clone(), 0, None)?
        .add(Member::new(late.clone().into(), None, Some(20), None))
        .add(Member::new(early.clone().into(), None, Some(10), None));
    let exported = crate::InboxState::from_core(state, None)?;
    let identifiers = exported
        .identities
        .into_iter()
        .map(|identity| identity.identifier)
        .collect::<Vec<_>>();
    assert_eq!(
        identifiers,
        [early.to_string(), late.to_string(), owner.to_string()]
    );
}
