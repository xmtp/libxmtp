//! Client configuration tests.
//!
//! The pure-logic cases live here unconditionally. The cases that need a
//! deployment answering with values chosen by the test use an ephemeral
//! backend, which is native-only.

use super::*;
use xmtp_configuration::ServerConfiguration;

fn version(text: &str) -> semver::Version {
    semver::Version::parse(text).unwrap()
}

fn requiring(minimum: &str) -> ServerConfiguration {
    ServerConfiguration {
        min_libxmtp_version: minimum.to_owned(),
        ..Default::default()
    }
}

// One patch below the published minimum is too old.
// verifies: CONF-049
#[xmtp_common::test(unwrap_try = true)]
fn a_client_one_patch_below_the_minimum_is_refused() {
    let error = check_minimum_version(&requiring("1.2.4"), &version("1.2.3")).unwrap_err();
    let ClientError::ClientVersionTooOld { client, minimum } = error else {
        panic!("expected ClientVersionTooOld, got {error}");
    };
    assert_eq!(client, "1.2.3");
    assert_eq!(minimum, "1.2.4");
}

// Equal is acceptable — the requirement is "higher than".
// verifies: CONF-049
#[xmtp_common::test(unwrap_try = true)]
fn a_client_exactly_at_the_minimum_is_accepted() {
    check_minimum_version(&requiring("1.2.3"), &version("1.2.3")).unwrap();
}

// Compared on major, minor, and patch only, so a prerelease tag can
// never be the thing that makes a client too old.
// verifies: CONF-050
#[xmtp_common::test(unwrap_try = true)]
fn a_prerelease_tag_never_makes_a_client_too_old() {
    check_minimum_version(&requiring("1.2.3"), &version("1.2.3-rc.1")).unwrap();
    check_minimum_version(&requiring("1.2.3-rc.4"), &version("1.2.3")).unwrap();
}

// An empty minimum publishes no requirement at all.
#[xmtp_common::test(unwrap_try = true)]
fn an_empty_minimum_requires_nothing() {
    check_minimum_version(&ServerConfiguration::default(), &version("0.0.1")).unwrap();
}

// The first latch is the one reported, and it is reported
// by every later call.
#[xmtp_common::test(unwrap_try = true)]
fn the_first_latch_wins_and_fails_every_later_call() {
    let handle = ServerConfigurationHandle::default();
    assert!(handle.latched().is_none());
    handle.check().unwrap();

    handle.latch(ConfigurationLatch::BackendMismatch {
        stored: "org.example.one".to_owned(),
        received: "org.example.two".to_owned(),
    });
    handle.latch(ConfigurationLatch::ClientVersionTooOld {
        client: "1.0.0".to_owned(),
        minimum: "2.0.0".to_owned(),
    });

    let error = handle.check().unwrap_err();
    let ClientError::BackendMismatch { stored, received } = error else {
        panic!("a later latch displaced the first: {error}");
    };
    assert_eq!(stored, "org.example.one");
    assert_eq!(received, "org.example.two");
}

// A stored copy that does not decode is a warning, not a failure. The
// identifier survives so the binding check still works.
#[xmtp_common::test(unwrap_try = true)]
fn an_undecodable_stored_copy_falls_back_to_compiled_defaults() {
    let stored = StoredServerConfiguration {
        id: 0,
        identifier: "org.example.one".to_owned(),
        backend_url: "http://localhost:5050".to_owned(),
        response: vec![0xff, 0xff, 0xff, 0xff],
        fetched_at_ns: 1,
        conflicting_identifier: None,
    };
    let snapshot = snapshot_from(&stored);
    assert_eq!(snapshot.identifier, "org.example.one");
    assert_eq!(
        snapshot.limits.max_query_topics,
        ServerConfiguration::default().limits.max_query_topics
    );
}

// A response that fails validation is never stored or used.
// verifies: CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn an_invalid_response_is_rejected() {
    let response = backend_v1::GetConfigurationResponse {
        identifier: String::new(),
        ..Default::default()
    };
    assert!(validated(&response).is_err());
}

// The refresh worker's three attempts are driven by this
// classification, so a transient failure must read as retryable through the
// `ClientError` the fetch wraps it in, and a permanent one must not.
#[xmtp_common::test(unwrap_try = true)]
fn a_transient_configuration_failure_stays_retryable() {
    use xmtp_common::RetryableError;

    let transient = ClientError::ConfigurationUnavailable(Box::new(
        ConfigurationFetchError::Storage(StorageError::DieselConnect(
            diesel::ConnectionError::BadConnection("the database went away".to_owned()),
        )),
    ));
    assert!(
        transient.is_retryable(),
        "a transient storage failure must be retried"
    );

    let permanent = ClientError::ConfigurationUnavailable(Box::new(
        ConfigurationFetchError::Storage(StorageError::DbSerialize),
    ));
    assert!(
        !permanent.is_retryable(),
        "a permanent failure must not be retried"
    );
}

// The handle binds a signature request to the
// chains the snapshot names, and does not when the app supplied its verifier.
#[xmtp_common::test(unwrap_try = true)]
fn the_handle_binds_a_request_to_the_snapshot_chains() {
    use xmtp_configuration::StaticConfigProvider;
    use xmtp_id::associations::builder::SignatureRequestBuilder;

    let provider = |chains: Vec<&str>| {
        Arc::new(StaticConfigProvider::edited(|configuration| {
            configuration.smart_contract_wallet_chains =
                chains.into_iter().map(str::to_owned).collect();
        })) as Arc<dyn ConfigProvider>
    };
    let request = || SignatureRequestBuilder::new("inbox").build();

    // The app supplied no verifier, so the deployment's list binds.
    let handle =
        ServerConfigurationHandle::new(provider(vec!["eip155:1"])).with_chain_restriction(false);
    let mut bound = request();
    handle.restrict(&mut bound);
    assert_eq!(bound.accepted_chains(), Some(&["eip155:1".to_owned()][..]));

    // An empty list is a list, not an absence.
    let handle = ServerConfigurationHandle::new(provider(vec![])).with_chain_restriction(false);
    let mut bound = request();
    handle.restrict(&mut bound);
    assert_eq!(bound.accepted_chains(), Some(&[][..]));

    // An app-supplied verifier is exempt, so nothing binds.
    let handle =
        ServerConfigurationHandle::new(provider(vec!["eip155:1"])).with_chain_restriction(true);
    let mut unbound = request();
    handle.restrict(&mut unbound);
    assert_eq!(unbound.accepted_chains(), None);
}

// A snapshot an app built in Rust can carry a
// zero the wire conversion would have replaced. The handle sanitizes it once,
// so the wrapper that chunks with these values and the app that reads them
// back both see the compiled default rather than a `chunks(0)` panic.
// verifies: CONF-025
#[xmtp_common::test(unwrap_try = true)]
fn a_zero_limit_from_a_provider_never_reaches_a_consumer() {
    use xmtp_configuration::{BACKEND_DEFAULT_MAX_QUERY_TOPICS, StaticConfigProvider};

    let handle =
        ServerConfigurationHandle::new(Arc::new(StaticConfigProvider::edited(|configuration| {
            configuration.identifier = "org.example.zeroed".to_owned();
            configuration.limits.max_query_topics = 0;
            configuration.limits.max_lookup_identifiers = 0;
        })));

    let limits = &handle.configuration().limits;
    assert_eq!(limits.max_query_topics, BACKEND_DEFAULT_MAX_QUERY_TOPICS);
    assert!(limits.max_lookup_identifiers > 0);
    // Only the zeroes moved.
    assert_eq!(handle.configuration().identifier, "org.example.zeroed");

    // A snapshot with no zero in it is held as supplied.
    let handle =
        ServerConfigurationHandle::new(Arc::new(StaticConfigProvider::edited(|configuration| {
            configuration.limits.max_query_topics = 3
        })));
    assert_eq!(handle.configuration().limits.max_query_topics, 3);
}

/// Non-default values for every published field, so the round-trip assertion
/// checks each field.
#[cfg(test)]
pub(crate) fn distinct_snapshot() -> ServerConfiguration {
    ServerConfiguration {
        identifier: "org.example.applied".to_owned(),
        server_version: "9.8.7".to_owned(),
        min_libxmtp_version: "0.0.1".to_owned(),
        auth: xmtp_configuration::AuthConfiguration {
            enabled: false,
            keys: vec![xmtp_configuration::SigningKeyDescription {
                kid: "key-1".to_owned(),
                alg: "EdDSA".to_owned(),
            }],
            audiences: vec!["aud-1".to_owned()],
            issuers: vec!["iss-1".to_owned()],
            required_scopes: vec!["scope-1".to_owned()],
        },
        retention: xmtp_configuration::RetentionConfiguration {
            group_message_seconds: 11,
            welcome_seconds: 12,
            key_package_seconds: 13,
        },
        limits: xmtp_configuration::LimitsConfiguration {
            max_envelope_bytes: 65_536,
            max_request_bytes: 1_048_576,
            max_response_bytes: 2_097_152,
            max_publish_topics: 21,
            max_query_topics: 22,
            max_query_limit: 23,
            default_query_limit: 24,
            max_newest_metadata_topics: 25,
            max_newest_full_topics: 26,
            max_update_adds: 27,
            max_update_removes: 28,
            max_stream_topics: 29,
            max_static_topics: 30,
            max_lookup_identifiers: 31,
            max_scw_signatures: 32,
            max_identity_entries: 33,
            max_update_frames_per_second: 34,
            max_update_burst: 35,
            max_ping_frames_per_second: 36,
            max_ping_burst: 37,
        },
        mls: xmtp_configuration::MlsConfiguration {
            max_group_members: 41,
            max_installations_per_inbox: 42,
            commit_log_enabled: Some(false),
        },
        smart_contract_wallet_chains: vec!["eip155:1".to_owned(), "eip155:8453".to_owned()],
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod applied_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod backend_tests {
    use crate::Client;
    use crate::builder::ClientBuilderError;
    use crate::context::XmtpSharedContext;
    use crate::identity::IdentityStrategy;
    use crate::utils::backend::EphemeralBackend;
    use crate::utils::test::identity_setup;
    use alloy::signers::local::PrivateKeySigner;
    use std::sync::Arc;
    use xmtp_api_backend::{BackendClient, TrackedStatsClient};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::prelude::*;
    use xmtp_db::{TestDb, XmtpTestDb};
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_proto::api::ToBoxedClient;

    /// A deployment that publishes values no compiled default would produce.
    const DISTINCT: &str = r#"
[server]
identifier = "org.example.distinct"

[limits]
max_query_topics = 17

[mls]
max_group_members = 23

[chains]
"eip155:1" = "http://127.0.0.1:8545"
"eip155:8453" = "http://127.0.0.1:8545"
"#;

    fn api_at(url: &str) -> xmtp_api_backend::TestClient {
        let transport = xmtp_api_grpc::GrpcClient::create(url.parse().unwrap())
            .unwrap()
            .arced();
        TrackedStatsClient::new(BackendClient::new(transport))
    }

    /// Build a client the way `Tester` does, but surface the error instead of
    /// unwrapping it: these tests are about builds that must fail.
    async fn build_at<Db>(
        url: &str,
        store: Db,
        owner: &PrivateKeySigner,
        strategy: IdentityStrategy,
    ) -> Result<
        Arc<
            crate::context::XmtpMlsLocalContext<
                Arc<xmtp_api_backend::TestClient>,
                Db,
                xmtp_db::sql_key_store::SqlKeyStore<<Db as xmtp_db::XmtpDb>::DbQuery>,
            >,
        >,
        ClientBuilderError,
    >
    where
        Db: xmtp_db::XmtpDb + Send + Sync + 'static,
    {
        let _ = owner;
        Client::builder(strategy)
            .store(store)
            .api_client_with_streams(Arc::new(api_at(url)))
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_disable_workers(true)
            .default_mls_store()
            .unwrap()
            .build()
            .await
            .map(|client| client.context.clone())
    }

    // The client reads the deployment's values
    // before any identity work and stores the copy bound to the URL it used.
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_client_reads_and_stores_what_the_deployment_publishes() {
        let backend = EphemeralBackend::start(DISTINCT).await?;
        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let context = build_at(backend.url(), store, &owner, identity_setup(&owner)).await?;

        let configuration = context.server_configuration.configuration();
        assert_eq!(configuration.identifier, "org.example.distinct");
        assert_eq!(configuration.limits.max_query_topics, 17);
        assert_eq!(configuration.mls.max_group_members, 23);
        assert!(!configuration.auth.enabled);
        // The chain list round-trips too.
        assert_eq!(
            configuration.smart_contract_wallet_chains,
            vec!["eip155:1".to_owned(), "eip155:8453".to_owned()]
        );

        let stored = context.db().server_configuration()?.unwrap();
        assert_eq!(stored.identifier, "org.example.distinct");
        assert_eq!(stored.backend_url, backend.url());
        assert_eq!(stored.conflicting_identifier, None);

        backend.stop().await?;
    }

    // The same database pointed at a
    // different deployment is refused, the conflict is recorded, and every
    // later build fails on the record alone.
    // verifies: CONF-030, CONF-031, CONF-072
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_database_bound_to_one_deployment_refuses_another() {
        let first = EphemeralBackend::start(DISTINCT).await?;
        let second =
            EphemeralBackend::start("[server]\nidentifier = \"org.example.other\"\n").await?;

        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let context = build_at(first.url(), store.clone(), &owner, identity_setup(&owner)).await?;
        drop(context);

        // The URL moved, so the identifier is checked again before
        // anything else happens.
        let Err(error) =
            build_at(second.url(), store.clone(), &owner, identity_setup(&owner)).await
        else {
            panic!("a different deployment must be refused");
        };
        assert!(
            format!("{error}").contains("org.example.other"),
            "unexpected error: {error}"
        );

        // The conflict is on disk and fails the next
        // build even back at the original deployment.
        let stored = store.db().server_configuration()?.unwrap();
        assert_eq!(
            stored.conflicting_identifier,
            Some("org.example.other".to_owned())
        );
        let Err(error) = build_at(first.url(), store.clone(), &owner, identity_setup(&owner)).await
        else {
            panic!("a recorded conflict must fail every later build");
        };
        assert!(
            format!("{error}").contains("org.example.other"),
            "unexpected error: {error}"
        );

        first.stop().await?;
        second.stop().await?;
    }

    // A URL change that keeps the identifier re-reads the
    // configuration and rewrites the stored URL.
    // verifies: CONF-033
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_url_change_that_keeps_the_identifier_is_accepted() {
        let first = EphemeralBackend::start(DISTINCT).await?;
        let second = EphemeralBackend::start(DISTINCT).await?;

        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let context = build_at(first.url(), store.clone(), &owner, identity_setup(&owner)).await?;
        drop(context);

        let context = build_at(second.url(), store.clone(), &owner, identity_setup(&owner)).await?;
        assert_eq!(
            context.server_configuration.configuration().identifier,
            "org.example.distinct"
        );
        let stored = store.db().server_configuration()?.unwrap();
        assert_eq!(stored.backend_url, second.url());
        assert_eq!(stored.conflicting_identifier, None);

        first.stop().await?;
        second.stop().await?;
    }

    // A deployment that requires a newer client refuses this one at
    // build, before any identity work.
    // verifies: CONF-049
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_deployment_requiring_a_newer_client_refuses_the_build() {
        let backend = EphemeralBackend::start(
            "[server]\nidentifier = \"org.example.future\"\nmin_libxmtp_version = \"9999.0.0\"\n",
        )
        .await?;

        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let Err(error) = build_at(backend.url(), store, &owner, identity_setup(&owner)).await
        else {
            panic!("a client below the published minimum must be refused");
        };
        assert!(
            format!("{error}").contains("9999.0.0"),
            "unexpected error: {error}"
        );

        backend.stop().await?;
    }

    /// A wrapper pointed at an address nothing is listening on, so any network
    /// read fails rather than quietly succeeding.
    fn unreachable_api() -> xmtp_api::ApiClientWrapper<Arc<xmtp_api_backend::TestClient>> {
        xmtp_api::ApiClientWrapper::new(Arc::new(api_at("http://127.0.0.1:1")), Default::default())
    }

    // An offline start with no stored copy is the compiled
    // defaults and an empty identifier, and reaches no network.
    // verifies: CONF-034, CONF-027
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_offline_start_with_no_stored_copy_uses_compiled_defaults() {
        let store = TestDb::create_ephemeral_store().await;
        let api = unreachable_api();

        let handle = crate::server_configuration::resolve(&api, &store.db(), true).await?;
        assert_eq!(
            handle.configuration(),
            &xmtp_configuration::ServerConfiguration::default()
        );
        assert!(handle.configuration().identifier.is_empty());
        // Nothing is written until a refresh succeeds.
        assert!(store.db().server_configuration()?.is_none());

        // The same start online does reach the network and fails, so the
        // assertion above is about the offline flag and not about the address.
        assert!(
            crate::server_configuration::resolve(&api, &store.db(), false)
                .await
                .is_err(),
            "an online start must read the deployment"
        );
    }

    // An offline start with a stored copy uses it and
    // skips the URL check, so a client with no network still starts.
    // verifies: CONF-034
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_offline_start_uses_the_stored_copy_without_checking_the_url() {
        let backend = EphemeralBackend::start(DISTINCT).await?;
        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let context =
            build_at(backend.url(), store.clone(), &owner, identity_setup(&owner)).await?;
        drop(context);
        backend.stop().await?;

        // A different URL, with nothing listening on it.
        let handle =
            crate::server_configuration::resolve(&unreachable_api(), &store.db(), true).await?;
        assert_eq!(handle.configuration().identifier, "org.example.distinct");
        assert_eq!(handle.configuration().limits.max_query_topics, 17);
        assert_eq!(handle.configuration().mls.max_group_members, 23);
    }

    // A refresh rewrites the stored copy, and a
    // minimum this build no longer meets latches the client.
    // verifies: CONF-036, CONF-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_refresh_that_raises_the_minimum_latches_the_client() {
        let backend = EphemeralBackend::start(
            "[server]\nidentifier = \"org.example.future\"\nmin_libxmtp_version = \"9999.0.0\"\n",
        )
        .await?;

        // A provider short-circuits the build, so this client starts
        // against a deployment whose published minimum would have refused it.
        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let client = Client::builder(identity_setup(&owner))
            .store(store.clone())
            .api_client_with_streams(Arc::new(api_at(backend.url())))
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_disable_workers(true)
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                |_| {},
            )))
            .default_mls_store()
            .unwrap()
            .build()
            .await?;
        assert!(client.context.server_configuration().latched().is_none());
        assert!(store.db().server_configuration()?.is_none());

        let mut worker =
            crate::server_configuration::worker::ConfigurationWorker::new(client.context.clone());
        worker.tick().await;

        // The run rewrote the stored copy.
        let stored = store.db().server_configuration()?.unwrap();
        assert_eq!(stored.identifier, "org.example.future");
        assert_eq!(stored.backend_url, backend.url());

        // The client latched, and every later call reports why.
        let latch = client.context.server_configuration().latched().unwrap();
        assert!(
            matches!(
                latch,
                crate::server_configuration::ConfigurationLatch::ClientVersionTooOld { .. }
            ),
            "expected a version latch, got {latch:?}"
        );
        let error = client.create_group(None, None).unwrap_err().to_string();
        assert!(error.contains("9999.0.0"), "unexpected error: {error}");

        // The latch covers every later call, including the two client entry
        // points that reach the network without going through a group: a
        // latched client neither looks an identifier up nor publishes a key
        // package.
        let error = client.can_message(&[]).await.unwrap_err().to_string();
        assert!(error.contains("9999.0.0"), "unexpected error: {error}");
        let error = client
            .rotate_and_upload_key_package()
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("9999.0.0"), "unexpected error: {error}");

        backend.stop().await?;
    }

    // A refresh that meets a different deployment latches
    // the client and records the conflict, rather than replacing the copy.
    // verifies: CONF-030
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_refresh_that_meets_another_deployment_latches_the_client() {
        let backend = EphemeralBackend::start(DISTINCT).await?;

        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let client = Client::builder(identity_setup(&owner))
            .store(store.clone())
            .api_client_with_streams(Arc::new(api_at(backend.url())))
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_disable_workers(true)
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                |_| {},
            )))
            .default_mls_store()
            .unwrap()
            .build()
            .await?;

        // A copy this database was bound to by an earlier, different deployment.
        store.db().store_server_configuration(
            "org.example.elsewhere",
            backend.url(),
            &[],
            xmtp_common::time::now_ns(),
        )?;

        let mut worker =
            crate::server_configuration::worker::ConfigurationWorker::new(client.context.clone());
        worker.tick().await;

        // The conflict is recorded, and the bound identifier is kept.
        let stored = store.db().server_configuration()?.unwrap();
        assert_eq!(stored.identifier, "org.example.elsewhere");
        assert_eq!(
            stored.conflicting_identifier,
            Some("org.example.distinct".to_owned())
        );

        // The client latched on the mismatch.
        let latch = client.context.server_configuration().latched().unwrap();
        assert!(
            matches!(
                latch,
                crate::server_configuration::ConfigurationLatch::BackendMismatch { .. }
            ),
            "expected a mismatch latch, got {latch:?}"
        );

        backend.stop().await?;
    }

    // An explicit refresh that meets a different
    // deployment closes the open streams too. The worker cancels after its
    // turn; the refresh has to cancel on its own way out, or a database known
    // to belong elsewhere keeps serving its subscriptions.
    // verifies: CONF-022
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_explicit_refresh_that_meets_another_deployment_cancels_the_client() {
        use crate::subscriptions::SubscribeError;
        use futures::StreamExt;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use xmtp_common::{
            ErrorCode, StreamHandle,
            time::{Duration, timeout},
        };

        const WAIT: Duration = Duration::from_secs(10);
        let assert_mismatch = |error: SubscribeError| {
            assert_eq!(error.error_code(), "ClientError::BackendMismatch");
            let cause = match error {
                SubscribeError::Configuration(cause)
                | SubscribeError::LocalDelivery(
                    crate::subscriptions::local_delivery::LocalDeliveryError::Configuration(cause),
                ) => cause,
                error => panic!("expected the configuration cause, got {error:?}"),
            };
            assert!(matches!(
                *cause,
                crate::client::ClientError::BackendMismatch { stored, received }
                    if stored == "org.example.elsewhere" && received == "org.example.distinct"
            ));
        };
        let backend = EphemeralBackend::start(DISTINCT).await?;

        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let client = Client::builder(identity_setup(&owner))
            .store(store.clone())
            .api_client_with_streams(Arc::new(api_at(backend.url())))
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_disable_workers(true)
            .config_provider(Arc::new(xmtp_configuration::StaticConfigProvider::edited(
                |_| {},
            )))
            .default_mls_store()
            .unwrap()
            .build()
            .await?;
        assert!(!client.context.cancellation_token().is_cancelled());

        let messages = client.stream_all_messages_owned(None, None).await?;
        futures::pin_mut!(messages);
        let conversations = client.stream_conversations_owned(None, false).await?;
        futures::pin_mut!(conversations);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let closed = Arc::new(AtomicUsize::new(0));
        let on_close = Arc::clone(&closed);
        let mut callback = Client::stream_conversations_with_callback_dispatch(
            Arc::new(client.clone()),
            None,
            false,
            move |item| {
                let _ = tx.send(item);
            },
            move || {
                on_close.fetch_add(1, Ordering::SeqCst);
            },
        );
        timeout(WAIT, callback.wait_for_ready()).await?;

        // A copy this database was bound to by an earlier, different deployment.
        store.db().store_server_configuration(
            "org.example.elsewhere",
            backend.url(),
            &[],
            xmtp_common::time::now_ns(),
        )?;

        {
            let pending_message = messages.next();
            let pending_conversation = conversations.next();
            futures::pin_mut!(pending_message, pending_conversation);
            assert!(futures::poll!(&mut pending_message).is_pending());
            assert!(futures::poll!(&mut pending_conversation).is_pending());

            // A real GetConfiguration response sets the latch and wakes each read.
            let error = client.refresh_server_configuration().await.unwrap_err();
            assert!(
                matches!(error, crate::client::ClientError::BackendMismatch { .. }),
                "expected a mismatch error, got {error}"
            );
            assert_mismatch(timeout(WAIT, pending_message).await?.unwrap().unwrap_err());
            assert_mismatch(
                timeout(WAIT, pending_conversation)
                    .await?
                    .unwrap()
                    .unwrap_err(),
            );
        }
        assert!(timeout(WAIT, messages.next()).await?.is_none());
        assert!(timeout(WAIT, conversations.next()).await?.is_none());
        assert_mismatch(timeout(WAIT, rx.recv()).await?.unwrap().unwrap_err());
        assert_mismatch(timeout(WAIT, callback.join()).await??.unwrap_err());
        assert!(timeout(WAIT, rx.recv()).await?.is_none());
        assert_eq!(closed.load(Ordering::SeqCst), 1);

        let reopened = client.stream_conversations_owned(None, false).await;
        assert_mismatch(match reopened {
            Err(error) => error,
            Ok(_) => panic!("a latched client must not open another stream"),
        });

        // Latched, and the context cancelled so every open stream closes.
        let latch = client.context.server_configuration().latched().unwrap();
        assert!(
            matches!(
                latch,
                crate::server_configuration::ConfigurationLatch::BackendMismatch { .. }
            ),
            "expected a mismatch latch, got {latch:?}"
        );
        assert!(client.context.cancellation_token().is_cancelled());

        backend.stop().await?;
    }

    // The configuration read is unauthenticated, so a
    // client with an auth callback never invokes it for this call.
    // verifies: CONF-029
    #[xmtp_common::test(unwrap_try = true)]
    async fn reading_the_configuration_never_invokes_the_auth_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use xmtp_api_backend::{AuthCallback, AuthMiddleware, Credential};

        struct CountingCallback(Arc<AtomicUsize>);

        #[xmtp_common::async_trait]
        impl AuthCallback for CountingCallback {
            async fn on_auth_required(&self) -> Result<Credential, xmtp_common::BoxDynError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Err("this test never supplies a credential".into())
            }
        }

        let backend = EphemeralBackend::start(DISTINCT).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = xmtp_api_grpc::GrpcClient::create(backend.url().parse().unwrap()).unwrap();
        let transport = AuthMiddleware::new(
            transport,
            Some(Arc::new(CountingCallback(calls.clone())) as Arc<dyn AuthCallback>),
            None,
        )
        .arced();
        let api = xmtp_api::ApiClientWrapper::new(
            Arc::new(TrackedStatsClient::new(BackendClient::new(transport))),
            Default::default(),
        );

        let configuration = api.get_configuration().await?;
        assert_eq!(configuration.identifier, "org.example.distinct");
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        backend.stop().await?;
    }
}
