//! Handle failures reach the app with public codes, none of them retryable,
//! through every wrapper that core puts around them.

use super::*;
use crate::{ErrorCategory, ErrorDetails};
use xmtp_api::{ApiError, HandleRejection};
use xmtp_mls::{
    client::ClientError, groups::GroupError, identity_updates::IdentityUpdateError,
    inbox_handles::HandleError,
};

/// The details of a handle failure, or a panic naming the variant found.
fn handle_details(error: &XmtpError) -> &ErrorDetails {
    match error {
        XmtpError::HandleTaken(details)
        | XmtpError::HandleNotAllowed(details)
        | XmtpError::HandleTooSoon(details)
        | XmtpError::HandleLimitReached(details)
        | XmtpError::IdentityNotFound(details)
        | XmtpError::InvalidArgument(details) => details,
        other => panic!("expected a handle failure, got {other:?}"),
    }
}

#[track_caller]
fn expect_code<T>(result: Result<T, XmtpError>, code: &str) {
    let Err(error) = result else {
        panic!("the handle call must fail with {code}, but it succeeded");
    };
    let details = handle_details(&error);
    assert_eq!(details.code, code, "{error:?}");
    assert!(!details.retryable, "{code} must not be retryable");
    let category = if code == "InvalidArgument" {
        ErrorCategory::Input
    } else {
        ErrorCategory::Identity
    };
    assert_eq!(
        std::mem::discriminant(&details.category),
        std::mem::discriminant(&category),
        "{error:?}"
    );
    let variant_matches = match code {
        "HandleTaken" => matches!(error, XmtpError::HandleTaken(_)),
        "HandleNotAllowed" => matches!(error, XmtpError::HandleNotAllowed(_)),
        "HandleTooSoon" => matches!(error, XmtpError::HandleTooSoon(_)),
        "HandleLimitReached" => matches!(error, XmtpError::HandleLimitReached(_)),
        "IdentityNotFound" => matches!(error, XmtpError::IdentityNotFound(_)),
        "InvalidArgument" => matches!(error, XmtpError::InvalidArgument(_)),
        other => panic!("unknown code {other}"),
    };
    assert!(variant_matches, "{code} has its own variant: {error:?}");
}

#[xmtp_common::test(unwrap_try = true)]
async fn claim_errors_have_public_kinds() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let handle = random_handle();
    alix.claim_handle(handle.clone()).await?;

    // The backend's refusals.
    expect_code(bo.claim_handle(handle).await, "HandleTaken");
    expect_code(bo.claim_handle("admin".into()).await, "HandleNotAllowed");
    expect_code(alix.claim_handle(random_handle()).await, "HandleTooSoon");

    // Text outside the grammar fails before any request.
    let before = request_counts(&bo).await?;
    expect_code(bo.claim_handle("1nick".into()).await, "InvalidArgument");
    expect_code(
        bo.available_handles("nick.eth".into()).await,
        "InvalidArgument",
    );
    assert_eq!(request_counts(&bo).await?, before);

    // A handle that no inbox holds fails each route that needs its inbox.
    let unclaimed = handle_identity(&random_handle());
    let conversations = bo.conversations();
    expect_code(
        conversations
            .create_dm_with_identity(unclaimed.clone(), None)
            .await,
        "IdentityNotFound",
    );
    expect_code(
        conversations
            .create_group_with_identities(vec![unclaimed.clone()], None)
            .await,
        "IdentityNotFound",
    );
    let group = conversations.create_group(vec![], None).await?;
    expect_code(
        group.add_members_by_identity(vec![unclaimed.clone()]).await,
        "IdentityNotFound",
    );
    expect_code(
        group.remove_members_by_identity(vec![unclaimed]).await,
        "IdentityNotFound",
    );
    alix.end().await?;
    bo.end().await?;
}

/// The claim limit cannot be reached on a backend in a test, and the
/// backend's refusals can arrive inside other wrappers. Each core shape maps
/// to its public code.
#[xmtp_common::test(unwrap_try = true)]
fn handle_failures_keep_their_codes_through_wrappers() {
    let rejected = |rejection| {
        ClientError::IdentityUpdate(IdentityUpdateError::Api(ApiError::HandleRejected(
            rejection,
        )))
    };
    for (rejection, code) in [
        (HandleRejection::Taken, "HandleTaken"),
        (HandleRejection::NotAllowed, "HandleNotAllowed"),
        (HandleRejection::TooSoon, "HandleTooSoon"),
    ] {
        expect_code::<()>(Err(XmtpError::from_client(rejected(rejection))), code);
        expect_code::<()>(
            Err(XmtpError::from_group(GroupError::Client(rejected(
                rejection,
            )))),
            code,
        );
        expect_code::<()>(
            Err(XmtpError::from_group_write(GroupError::WrappedApi(
                ApiError::HandleRejected(rejection),
            ))),
            code,
        );
        expect_code::<()>(
            Err(XmtpError::from_api(ApiError::HandleRejected(rejection))),
            code,
        );
    }
    for (handle, code) in [
        (
            HandleError::Invalid(xmtp_id::associations::handle::InvalidHandle),
            "InvalidArgument",
        ),
        (HandleError::NotFound, "IdentityNotFound"),
        (HandleError::LimitReached, "HandleLimitReached"),
    ] {
        let message = handle.to_string();
        let client = || ClientError::Handle(clone_handle_error(&handle));
        expect_code::<()>(Err(XmtpError::from_client(client())), code);
        expect_code::<()>(
            Err(XmtpError::from_group(GroupError::Client(client()))),
            code,
        );
        expect_code::<()>(
            Err(XmtpError::from_client(ClientError::Group(Box::new(
                GroupError::Client(client()),
            )))),
            code,
        );
        let error = XmtpError::from_client(client());
        assert_eq!(handle_details(&error).message, message);
    }
}

/// `HandleError` is not `Clone`; each variant is a unit or a unit struct.
fn clone_handle_error(error: &HandleError) -> HandleError {
    match error {
        HandleError::Invalid(invalid) => HandleError::Invalid(*invalid),
        HandleError::NotFound => HandleError::NotFound,
        HandleError::LimitReached => HandleError::LimitReached,
    }
}
