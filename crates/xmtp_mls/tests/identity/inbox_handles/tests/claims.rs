//! Claims: the backend's reasons, the claim cap, and no client policy.

use super::{random_handle, scripted_client};
use prost::Message as _;
use xmtp_api::{ApiError, HandleRejection};
use xmtp_db::{
    diesel::{
        RunQueryDsl as _, sql_query,
        sql_types::{BigInt, Binary, Text},
    },
    prelude::*,
};
use xmtp_id::associations::{AssociationState, MAX_HANDLE_CLAIMS, handle::Handle};
use xmtp_mls::{
    client::ClientError, context::CoreContext, identity_updates::IdentityUpdateError,
    inbox_handles::HandleError, tester,
};
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

/// The backend's reason when it refused a claim.
fn rejection(result: Result<(), ClientError>) -> Option<HandleRejection> {
    let Err(ClientError::IdentityUpdate(IdentityUpdateError::Publish(error))) = result else {
        return None;
    };
    match *error {
        IdentityUpdateError::Api(ApiError::HandleRejected(rejection)) => Some(rejection),
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

/// `state` after `count` more claims of one handle.
fn with_claims(state: &AssociationState, count: u32) -> AssociationState {
    let handle = Handle::parse("nick").expect("a handle");
    (0..count).fold(state.clone(), |state, _| state.claim_handle(handle.clone()))
}

/// Put `state` in the client's cache at `sequence_id`, over any state there.
fn replace_cached_state(
    db: &impl xmtp_db::ConnectionExt,
    sequence_id: i64,
    state: &AssociationState,
) -> Result<(), xmtp_db::ConnectionError> {
    let bytes = AssociationStateProto::from(state.clone()).encode_to_vec();
    let inbox_id = state.inbox_id().to_owned();
    db.raw_query(|conn| {
        sql_query(
            "INSERT OR REPLACE INTO association_state (inbox_id, sequence_id, state) \
             VALUES (?, ?, ?)",
        )
        .bind::<Text, _>(inbox_id)
        .bind::<BigInt, _>(sequence_id)
        .bind::<Binary, _>(bytes)
        .execute(conn)
    })?;
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn claim_handle_refreshes_and_refuses_at_the_cap_before_signing() {
    tester!(alix, disable_workers);
    // A second installation grows the log where alix's database cannot see it.
    tester!(alix2, from: alix, disable_workers);
    let (client, script) = scripted_client(&alix).await?;
    let db = alix.context.db();
    let state = alix.inbox_state(true).await?;
    let newest = db.get_latest_sequence_id(&[alix.inbox_id()])?[alix.inbox_id()];

    // The own state holds 64 claims: refused, and nothing is signed or sent.
    replace_cached_state(&db, newest, &with_claims(&state, MAX_HANDLE_CLAIMS))?;
    assert!(matches!(
        client.claim_handle(&random_handle()).await,
        Err(ClientError::IdentityUpdate(IdentityUpdateError::Handle(
            HandleError::LimitReached
        )))
    ));
    assert_eq!(script.lock().publishes, 0);

    // Alix's database holds 63 claims, but the log has a newer update whose
    // state holds 64. The claim refreshes the own log first, so it is refused.
    alix2.claim_handle(&random_handle()).await?;
    let claimed_at = alix2
        .context
        .db()
        .get_latest_sequence_id(&[alix.inbox_id()])?[alix.inbox_id()];
    replace_cached_state(&db, newest, &with_claims(&state, MAX_HANDLE_CLAIMS - 1))?;
    replace_cached_state(&db, claimed_at, &with_claims(&state, MAX_HANDLE_CLAIMS))?;
    assert!(matches!(
        client.claim_handle(&random_handle()).await,
        Err(ClientError::IdentityUpdate(IdentityUpdateError::Handle(
            HandleError::LimitReached
        )))
    ));
    assert_eq!(script.lock().publishes, 0);
}

// verifies: CONF-081
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn client_never_applies_the_minimum() {
    use std::sync::Arc;
    use xmtp_backend::test_support::TestServer as EphemeralBackend;
    use xmtp_configuration::StaticConfigProvider;
    // The backend admits names of one character; the client's snapshot says 5.
    let backend = EphemeralBackend::from_toml(
        r#"
        [handles]
        min_name_length = 1
    "#,
    )
    .await?;
    let snapshot = Arc::new(StaticConfigProvider::edited(|configuration| {
        configuration.handles.min_name_length = 5
    }));
    tester!(alix, backend_url: &backend.url, config_provider: snapshot.clone());
    tester!(bo, backend_url: &backend.url, config_provider: snapshot);
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
