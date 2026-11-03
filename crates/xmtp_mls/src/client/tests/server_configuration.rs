//! Client builds and refreshes against a deployment's configuration: what a
//! build reads, stores and refuses, and what a refresh changes for a running
//! client.

#[cfg(all(test, not(target_arch = "wasm32")))]
mod applied_tests;

use crate::server_configuration::{ConfigurationFetchError, ServerConfigError};
use xmtp_configuration::ServerConfiguration;
use xmtp_db::StorageError;

// The refresh worker's three attempts are driven by this
// classification, so a transient failure must read as retryable through the
// `ServerConfigError` the fetch wraps it in and the `ClientError` a client
// call returns, and a permanent one must not.
#[xmtp_common::test(unwrap_try = true)]
fn a_transient_configuration_failure_stays_retryable() {
    use crate::client::ClientError;
    use xmtp_common::RetryableError;

    let transient = || {
        ServerConfigError::from(ConfigurationFetchError::Storage(
            StorageError::DieselConnect(diesel::ConnectionError::BadConnection(
                "the database went away".to_owned(),
            )),
        ))
    };
    assert!(
        transient().is_retryable(),
        "a transient storage failure must be retried"
    );
    assert!(
        ClientError::from(transient()).is_retryable(),
        "a transient storage failure must be retried"
    );

    let permanent =
        || ServerConfigError::from(ConfigurationFetchError::Storage(StorageError::DbSerialize));
    assert!(
        !permanent().is_retryable(),
        "a permanent failure must not be retried"
    );
    assert!(
        !ClientError::from(permanent()).is_retryable(),
        "a permanent failure must not be retried"
    );
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
        handles: xmtp_configuration::HandlesConfiguration { min_name_length: 5 },
        smart_contract_wallet_chains: vec!["eip155:1".to_owned(), "eip155:8453".to_owned()],
        attachments: None,
        application_components: vec![xmtp_configuration::ApplicationComponentDefinition {
            component_id: 0xC001,
            name: "USER_PRONOUNS".to_owned(),
            component_type: 7,
            permissions: xmtp_configuration::ComponentPermissions {
                insert: Some(xmtp_configuration::MetadataPolicy::Base(5)),
                update: Some(xmtp_configuration::MetadataPolicy::Base(5)),
                delete: Some(xmtp_configuration::MetadataPolicy::Base(3)),
            },
            in_groups: true,
            in_dms: false,
        }],
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod backend_tests {
    use crate::Client;
    use crate::builder::ClientBuilderError;
    use crate::context::CoreContext;
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

[handles]
min_name_length = 5

[chains]
"eip155:1" = "http://127.0.0.1:8545"
"eip155:8453" = "http://127.0.0.1:8545"

[[application_components]]
component_id = 0xC001
name = "USER_PRONOUNS"
component_type = "tls_map_inbox_id_string"
insert_policy = "allow_if_self_or_non_member"
update_policy = "allow_if_self_or_non_member"
delete_policy = "allow_if_admin"
in_groups = true
in_dms = true

[[application_components]]
component_id = 0xC000
name = "topic"
component_type = "string"
insert_policy = "allow"
update_policy = "allow_if_admin"
delete_policy = "deny"
in_groups = true
in_dms = false
"#;

    /// The catalogue `DISTINCT` publishes, as `(component_id, name)` in the
    /// published order.
    fn catalogue(configuration: &xmtp_configuration::ServerConfiguration) -> Vec<(u16, &str)> {
        configuration
            .application_components
            .iter()
            .map(|definition| (definition.component_id, definition.name.as_str()))
            .collect()
    }

    const DISTINCT_CATALOGUE: [(u16, &str); 2] = [(0xC000, "topic"), (0xC001, "USER_PRONOUNS")];

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
    // verifies: CONF-061, CONF-079
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_client_reads_and_stores_what_the_deployment_publishes() {
        let backend = EphemeralBackend::start(DISTINCT).await?;
        let owner = generate_local_wallet();
        let store = TestDb::create_ephemeral_store().await;
        let context = build_at(backend.url(), store, &owner, identity_setup(&owner)).await?;

        let configuration = context.server_configuration().configuration();
        assert_eq!(configuration.identifier, "org.example.distinct");
        assert_eq!(configuration.limits.max_query_topics, 17);
        assert_eq!(configuration.mls.max_group_members, 23);
        assert_eq!(configuration.handles.min_name_length, 5);
        assert!(!configuration.auth.enabled);
        // The chain list round-trips too.
        assert_eq!(
            configuration.smart_contract_wallet_chains,
            vec!["eip155:1".to_owned(), "eip155:8453".to_owned()]
        );
        assert_eq!(catalogue(configuration), DISTINCT_CATALOGUE);
        assert_eq!(configuration.application_components.len(), 2);
        let topic = &configuration.application_components[0];
        assert_eq!(topic.component_id, 0xC000);
        assert_eq!(topic.name, "topic");
        assert_eq!(topic.component_type, 2);
        assert_eq!(
            topic.permissions.insert,
            Some(xmtp_configuration::MetadataPolicy::Base(1))
        );
        assert_eq!(
            topic.permissions.update,
            Some(xmtp_configuration::MetadataPolicy::Base(3))
        );
        assert_eq!(
            topic.permissions.delete,
            Some(xmtp_configuration::MetadataPolicy::Base(2))
        );
        assert!(topic.in_groups);
        assert!(!topic.in_dms);
        let pronouns = &configuration.application_components[1];
        assert_eq!(pronouns.component_id, 0xC001);
        assert_eq!(pronouns.name, "USER_PRONOUNS");
        assert_eq!(pronouns.component_type, 7);
        assert_eq!(
            pronouns.permissions.insert,
            Some(xmtp_configuration::MetadataPolicy::Base(5))
        );
        assert_eq!(
            pronouns.permissions.update,
            Some(xmtp_configuration::MetadataPolicy::Base(5))
        );
        assert_eq!(
            pronouns.permissions.delete,
            Some(xmtp_configuration::MetadataPolicy::Base(3))
        );
        assert!(pronouns.in_groups);
        assert!(pronouns.in_dms);

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
            context.server_configuration().configuration().identifier,
            "org.example.distinct"
        );
        let stored = store.db().server_configuration()?.unwrap();
        assert_eq!(stored.backend_url, second.url());
        assert_eq!(stored.conflicting_identifier, None);

        first.stop().await?;
        second.stop().await?;
    }

    /// The default verifier sizes its verify requests by the limits the
    /// deployment publishes, not by the compiled defaults: five signatures at
    /// a published maximum of two take three requests.
    // verifies: CONF-073
    #[xmtp_common::test(unwrap_try = true)]
    async fn the_default_verifier_sizes_requests_by_the_published_limits() {
        use alloy::signers::SignerSync;
        use xmtp_id::{
            associations::AccountId,
            scw_verifier::{ScwInput, SmartContractSignatureVerifier},
        };
        let backend = EphemeralBackend::start(&format!(
            "[limits]\nmax_scw_signatures = 2\n[chains]\n\"eip155:31337\" = {:?}\n",
            xmtp_configuration::DockerUrls::anvil()
        ))
        .await?;
        let owner = generate_local_wallet();
        let client = Client::builder(identity_setup(&owner))
            .store(TestDb::create_ephemeral_store().await)
            .api_client_with_streams(Arc::new(api_at(backend.url())))
            .with_remote_verifier()?
            .with_disable_workers(true)
            .default_mls_store()?
            .build()
            .await?;
        // An account without code verifies as the key that signed the hash.
        let signer = generate_local_wallet();
        let account = AccountId::new_evm(31337, signer.address().to_string().to_lowercase());
        let inputs = (0..5u8)
            .map(|index| {
                let hash = [index; 32];
                let signature = signer.sign_hash_sync(&hash.into())?;
                Ok(ScwInput::checked(
                    account.clone(),
                    hash,
                    signature.as_bytes().to_vec().into(),
                    None,
                )?)
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        client.clear_stats();
        let verdicts = client.scw_verifier().verify_many(&inputs).await?;
        assert!(verdicts.iter().all(|verdict| verdict.is_valid));
        assert_eq!(
            client
                .identity_api_stats()
                .verify_smart_contract_wallet_signatures
                .get_count(),
            3
        );
        backend.stop().await?;
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

    // A refresh rewrites the stored copy, and a
    // minimum this build no longer meets blocks the connection.
    // verifies: CONF-036, CONF-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_refresh_that_raises_the_minimum_blocks_the_connection() {
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
        assert!(
            client
                .context
                .server_configuration()
                .blocked_connection()
                .is_none()
        );
        assert!(store.db().server_configuration()?.is_none());

        let mut worker =
            crate::core::config_refresh::ConfigurationWorker::new(client.context.clone());
        crate::core::config_refresh::test_support::tick(&mut worker).await;

        // The run rewrote the stored copy.
        let stored = store.db().server_configuration()?.unwrap();
        assert_eq!(stored.identifier, "org.example.future");
        assert_eq!(stored.backend_url, backend.url());

        // The connection is blocked, and every later call reports why.
        let blocked_connection = client
            .context
            .server_configuration()
            .blocked_connection()
            .unwrap();
        assert!(
            matches!(
                blocked_connection,
                crate::server_configuration::BlockedConnection::ClientVersionTooOld { .. }
            ),
            "expected a version block, got {blocked_connection:?}"
        );
        let error = client.create_group(None, None).unwrap_err().to_string();
        assert!(error.contains("9999.0.0"), "unexpected error: {error}");

        // The blocked connection covers every later call, including the two client entry
        // points that reach the network without going through a group: a
        // client with a blocked connection neither looks an identifier up nor publishes a key
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

    // An explicit refresh that meets a different
    // deployment closes the open streams too. The worker cancels after its
    // turn; the refresh has to cancel on its own way out, or a database known
    // to belong elsewhere keeps serving its subscriptions.
    // verifies: CONF-075
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
            assert_eq!(error.error_code(), "BlockedConnection::BackendMismatch");
            let cause = match error {
                SubscribeError::Blocked(cause)
                | SubscribeError::LocalDelivery(
                    crate::subscriptions::local_delivery::LocalDeliveryError::Blocked(cause),
                ) => cause,
                error => panic!("expected the configuration cause, got {error:?}"),
            };
            assert!(matches!(
                cause,
                crate::server_configuration::BlockedConnection::BackendMismatch { stored, received }
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

            // A real GetConfiguration response blocks the connection and wakes each read.
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
            Ok(_) => panic!("a client with a blocked connection must not open another stream"),
        });

        // The connection is blocked, and the context is cancelled so every open stream closes.
        let blocked_connection = client
            .context
            .server_configuration()
            .blocked_connection()
            .unwrap();
        assert!(
            matches!(
                blocked_connection,
                crate::server_configuration::BlockedConnection::BackendMismatch { .. }
            ),
            "expected a mismatch block, got {blocked_connection:?}"
        );
        assert!(client.context.cancellation_token().is_cancelled());

        backend.stop().await?;
    }
}
