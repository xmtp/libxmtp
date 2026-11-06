//! Published configuration through a real client.
//!
//! Each applied field is given a non-default value through the static
//! provider, and the behaviour it changes is asserted. A provider
//! short-circuits the fetch, the store, the refresh, and the identifier check,
//! so these tests name values no deployment has to publish and still run
//! against the shared backend.
use super::distinct_snapshot;
use crate::Client;
use crate::builder::ClientBuilderError;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::server_configuration::{BlockedConnection, test_support::block_connection};
use crate::utils::DefaultTestClientCreator;
use crate::utils::test::identity_setup;
use std::sync::Arc;
use xmtp_configuration::{ConfigProvider, ServerConfiguration, StaticConfigProvider};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::XmtpTestDb;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

fn provider(edit: impl FnOnce(&mut ServerConfiguration)) -> Arc<dyn ConfigProvider> {
    Arc::new(StaticConfigProvider::edited(edit))
}

/// Build against the shared backend with a caller-supplied snapshot, without
/// unwrapping: these cases are about builds that must fail.
async fn build_with(provider: Arc<dyn ConfigProvider>) -> Result<(), ClientBuilderError> {
    let owner = generate_local_wallet();
    Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .config_provider(provider)
        .default_mls_store()
        .unwrap()
        .build()
        .await
        .map(|_| ())
}

// Every field the deployment publishes reaches `serverConfiguration()` unchanged.
// verifies: CONF-061
#[xmtp_common::test(unwrap_try = true)]
async fn every_published_field_round_trips_to_the_client() {
    let expected = distinct_snapshot();
    let snapshot = expected.clone();
    crate::tester!(alix, config_provider: provider(move |c| *c = snapshot));

    assert_eq!(alix.server_configuration(), &expected);
}

// The ceiling is read from the snapshot the client resolved before any
// identity work, and refuses the registration before it publishes.
// verifies: CONF-044
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_installation_limit_refuses_the_registration() {
    // One installation exists after this build.
    crate::tester!(first);
    let owner = first.builder.owner.clone();
    assert_eq!(first.inbox_state(true).await?.installations().len(), 1);

    // A second installation for the same inbox, against a deployment that
    // allows exactly one.
    let error = Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client(DefaultTestClientCreator::create().build()?)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .config_provider(provider(|c| c.mls.max_installations_per_inbox = 1))
        .default_mls_store()?
        .build()
        .await
        .map(|_| ())
        .expect_err("a second installation must be refused");
    assert!(
        format!("{error}").contains("installations"),
        "expected the installation-limit error, got {error}"
    );

    // Nothing was published: the inbox still has one installation.
    assert_eq!(first.inbox_state(true).await?.installations().len(), 1);
}

// A deployment that requires a credential refuses a client that has no
// way to produce one, and the error carries the scopes it wanted.
// verifies: CONF-051
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_requiring_authentication_refuses_a_client_with_no_credential_source() {
    let error = build_with(provider(|c| {
        c.auth.enabled = true;
        c.auth.required_scopes = vec!["xmtp:write".to_owned()];
    }))
    .await
    .expect_err("a deployment requiring authentication must refuse this client");
    let ClientBuilderError::ClientError(crate::client::ClientError::AuthRequired {
        required_scopes,
    }) = error
    else {
        panic!("expected AuthRequired, got {error}");
    };
    assert_eq!(required_scopes, vec!["xmtp:write".to_owned()]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn an_sdk_placeholder_does_not_count_as_a_credential_source() {
    let owner = generate_local_wallet();
    let mut transport = xmtp_api_backend::MessageBackendBuilder::new();
    transport
        .host(xmtp_configuration::backend_test_url())
        .maybe_auth_handle(Some(xmtp_api_backend::AuthHandle::sdk_placeholder()));
    let error = Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client_with_streams(transport.build()?)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .config_provider(provider(|c| {
            c.auth.enabled = true;
            c.auth.required_scopes = vec!["xmtp:write".to_owned()];
        }))
        .default_mls_store()?
        .build()
        .await
        .map(|_| ())
        .expect_err("an SDK placeholder must not satisfy CONF-051");
    let ClientBuilderError::ClientError(crate::client::ClientError::AuthRequired {
        required_scopes,
    }) = error
    else {
        panic!("expected AuthRequired, got {error}");
    };
    assert_eq!(required_scopes, vec!["xmtp:write".to_owned()]);
}

// Auth off with a callback configured is not an error. The client
// still builds; the backend simply ignores the credential.
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_with_authentication_off_still_builds() {
    build_with(provider(|c| c.auth.enabled = false)).await?;
}

// The minimum-version check applies to the snapshot the client ends up
// holding, including one a provider supplied.
// verifies: CONF-049
#[xmtp_common::test(unwrap_try = true)]
async fn a_provider_snapshot_requiring_a_newer_client_refuses_the_build() {
    let error = build_with(provider(|c| {
        c.min_libxmtp_version = "9999.0.0".to_owned();
    }))
    .await
    .expect_err("a client below the published minimum must be refused");
    assert!(
        format!("{error}").contains("9999.0.0"),
        "unexpected error: {error}"
    );
}

// Every signature request the client hands back is bound to the chains
// the deployment accepts, revocation included. Installing the default remote
// verifier also ends the app-supplied-verifier exemption, so a caller that sets
// its own verifier and then asks for the remote one is bound like anyone else.
#[xmtp_common::test(unwrap_try = true)]
async fn a_revocation_request_is_bound_to_the_accepted_chains() {
    let owner = generate_local_wallet();
    let client = Client::builder(identity_setup(&owner))
        .store(xmtp_db::TestDb::create_ephemeral_store().await)
        .api_client(DefaultTestClientCreator::create().build().unwrap())
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_remote_verifier()?
        .with_disable_workers(true)
        .config_provider(provider(|c| {
            c.smart_contract_wallet_chains = vec!["eip155:1".to_owned()];
        }))
        .default_mls_store()?
        .build()
        .await?;
    crate::utils::test::register_client(&client, &owner).await;

    let request = client
        .identity_updates()
        .revoke_installations(vec![vec![9u8; 32]])
        .await?;
    assert_eq!(
        request.accepted_chains(),
        Some(&["eip155:1".to_owned()][..]),
        "a revocation request must carry the deployment's chains"
    );
}

// Once the connection is blocked, every later call fails with the reason,
// and the client's cancellation token closes its streams.
// verifies: CONF-075
#[xmtp_common::test(unwrap_try = true)]
async fn a_client_with_blocked_connection_fails_every_later_call() {
    use crate::context::CoreContext;
    use xmtp_common::StreamHandle;

    crate::tester!(alix, config_provider: provider(|_| {}));
    let group = alix.create_group(None, None)?;
    group
        .send_message(b"before", SendMessageOpts::default())
        .await?;

    block_connection(
        alix.context.server_configuration(),
        BlockedConnection::ClientVersionTooOld {
            client: "1.0.0".to_owned(),
            minimum: "9999.0.0".to_owned(),
        },
    );

    // The gates that report the blocked connection cause: group sync and the client-level
    // readiness check every operation passes through.
    for error in [
        group.sync().await.unwrap_err().to_string(),
        alix.create_group(None, None).unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("9999.0.0"),
            "a client with a blocked connection must report its cause, got {error}"
        );
    }

    // Sending still fails; the sync driver reports it as a publish failure
    // rather than re-raising the blocked connection cause, because the intent stays queued for a
    // client that can publish it.
    assert!(
        group
            .send_message(b"after", SendMessageOpts::default())
            .await
            .is_err(),
        "a client with a blocked connection must not send"
    );

    // The streams a blocked connection closes are closed by cancelling the context token.
    // The refresh worker does that; here the assertion is that cancelling is
    // all it takes, that a callback stream reports the reason rather than
    // closing silently, and that the blocked connection cause survives to explain why.
    let reported = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let sink = reported.clone();
    let mut handle = crate::Client::stream_consent_with_callback(
        Arc::new((*alix).clone()),
        move |update| {
            if let Err(error) = update {
                sink.lock().push(error.to_string());
            }
        },
        || {},
    );
    handle.wait_for_ready().await;

    alix.context.cancellation_token().cancel();
    assert!(alix.context.server_configuration().check().is_err());

    let closed = handle
        .join()
        .await?
        .expect_err("a client with a blocked connection must close its streams with the reason");
    assert!(
        closed.to_string().contains("9999.0.0"),
        "a closing stream must report the blocked connection cause, got {closed}"
    );
    let reported = reported.lock().clone();
    assert!(
        reported.iter().any(|error| error.contains("9999.0.0")),
        "the callback must see the blocked connection cause, got {reported:?}"
    );
}
