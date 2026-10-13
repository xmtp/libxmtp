//! Claims: the backend's reasons, the claim cap, and no client policy.

use super::{new_inbox_state, random_handle};
use crate::{
    client::ClientError,
    identity_updates::IdentityUpdateError,
    inbox_handles::{HandleError, ensure_claim_allowed},
    tester,
};
use xmtp_api::{ApiError, HandleRejection};
use xmtp_id::associations::{MAX_HANDLE_CLAIMS, handle::Handle};

/// The backend's reason when it refused a claim.
fn rejection(result: Result<(), ClientError>) -> Option<HandleRejection> {
    match result {
        Err(ClientError::IdentityUpdate(IdentityUpdateError::Api(ApiError::HandleRejected(
            rejection,
        )))) => Some(rejection),
        _ => None,
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn claim_reports_backend_reasons() {
    tester!(alix);
    tester!(bo);
    let handle = random_handle();
    alix.claim_handle(&handle).await?;

    assert_eq!(
        rejection(bo.claim_handle(&handle).await),
        Some(HandleRejection::Taken)
    );
    assert_eq!(
        rejection(bo.claim_handle(&Handle::parse("admin")?).await),
        Some(HandleRejection::NotAllowed)
    );
    assert_eq!(
        rejection(alix.claim_handle(&random_handle()).await),
        Some(HandleRejection::TooSoon)
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn claim_limit_is_checked_before_signing() {
    let handle = Handle::parse("nick")?;
    let mut state = new_inbox_state();
    for _ in 1..MAX_HANDLE_CLAIMS {
        state = state.claim_handle(handle.clone());
    }
    assert_eq!(state.handle_claims(), MAX_HANDLE_CLAIMS - 1);
    assert!(ensure_claim_allowed(&state).is_ok());

    let state = state.claim_handle(handle);
    assert!(matches!(
        ensure_claim_allowed(&state),
        Err(HandleError::LimitReached)
    ));
}

// verifies: CONF-081
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn client_never_applies_the_minimum() {
    use crate::utils::test::backend::EphemeralBackend;
    use std::sync::Arc;
    use xmtp_configuration::StaticConfigProvider;
    // The backend admits names of one character; the client's snapshot says 5.
    let backend = EphemeralBackend::start(
        r#"
        [handles]
        min_name_length = 1
    "#,
    )
    .await?;
    let snapshot = Arc::new(StaticConfigProvider::edited(|configuration| {
        configuration.handles.min_name_length = 5
    }));
    tester!(alix, backend: &backend, config_provider: snapshot.clone());
    tester!(bo, backend: &backend, config_provider: snapshot);
    assert_eq!(alix.server_configuration().handles.min_name_length, 5);
    let short = Handle::parse("ab")?;

    assert_eq!(alix.available_handles("ab").await?.first(), Some(&short));
    alix.claim_handle(&short).await?;
    assert_eq!(
        bo.resolve_handles(std::slice::from_ref(&short)).await?,
        vec![Some(alix.inbox_id().to_owned())]
    );
    let state = bo.inbox_addresses(false, vec![alix.inbox_id()]).await?;
    assert_eq!(state[0].display_handle(), Some(&short));
}
