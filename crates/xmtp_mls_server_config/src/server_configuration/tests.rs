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
    let ServerConfigError::Blocked(BlockedConnection::ClientVersionTooOld { client, minimum }) =
        error
    else {
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
    for (minimum, client, is_below) in [
        ("1.2.3", "1.2.2", true),
        ("1.2.3", "1.2.3", false),
        ("1.2.3", "1.3.0", false),
        ("1.2.3", "1.2.3-rc.1", false),
        ("1.2.3-rc.4", "1.2.3", false),
        ("1.2.3+build.2", "1.2.3", false),
        ("1.2.3", "1.2.3+build.2", false),
    ] {
        let result = check_minimum_version(&requiring(minimum), &version(client));
        if is_below {
            assert!(
                matches!(
                    &result,
                    Err(ServerConfigError::Blocked(
                        BlockedConnection::ClientVersionTooOld { .. }
                    ))
                ),
                "minimum {minimum}, client {client}: {result:?}"
            );
        } else {
            assert!(
                result.is_ok(),
                "minimum {minimum}, client {client}: {result:?}"
            );
        }
    }
}

// An empty minimum publishes no requirement at all.
#[xmtp_common::test(unwrap_try = true)]
fn an_empty_minimum_requires_nothing() {
    check_minimum_version(&ServerConfiguration::default(), &version("0.0.1")).unwrap();
}

// The first blocked connection cause is reported by later network checks.
#[xmtp_common::test(unwrap_try = true)]
fn the_first_blocked_connection_cause_wins_for_network_checks() {
    let handle = ServerConfigurationHandle::default();
    assert!(handle.blocked_connection().is_none());
    handle.check().unwrap();

    handle.block_connection(BlockedConnection::BackendMismatch {
        stored: "org.example.one".to_owned(),
        received: "org.example.two".to_owned(),
    });
    handle.block_connection(BlockedConnection::ClientVersionTooOld {
        client: "1.0.0".to_owned(),
        minimum: "2.0.0".to_owned(),
    });

    let error = handle.check().unwrap_err();
    let BlockedConnection::BackendMismatch { stored, received } = error else {
        panic!("a later blocked connection cause displaced the first: {error}");
    };
    assert_eq!(stored, "org.example.one");
    assert_eq!(received, "org.example.two");
}

// verifies: EVENT-001, EVENT-008, EVENT-016
#[xmtp_common::test(unwrap_try = true)]
fn a_new_rejection_emits_once_and_keeps_the_subscription_open() {
    use xmtp_events::{ClientEvent, EventBus, EventFilter, EventKind, PublicBusWriter};

    let bus = EventBus::<()>::new();
    let subscription = bus.subscribe_app(EventFilter::new([EventKind::ClientRejectedByServer]))?;
    let handle = ServerConfigurationHandle::default();
    handle.set_event_writer(Arc::new(PublicBusWriter::new(&bus)));
    let reason = BlockedConnection::ClientVersionTooOld {
        client: "1.0.0".into(),
        minimum: "2.0.0".into(),
    };
    handle.block_connection(reason.clone());
    handle.block_connection(reason);
    assert!(matches!(
        subscription.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ClientRejectedByServer(rejection)), ..
        }] if rejection.cause == RejectionCause::VersionTooOld
            && rejection.min_libxmtp_version.as_deref() == Some("2.0.0")
    ));
    assert!(!subscription.is_closed());

    let mismatch = ServerConfigurationHandle::default();
    mismatch.set_event_writer(Arc::new(PublicBusWriter::new(&bus)));
    mismatch.block_connection(BlockedConnection::BackendMismatch {
        stored: "old".into(),
        received: "new".into(),
    });
    assert!(matches!(
        subscription.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(ClientEvent::ClientRejectedByServer(rejection)), ..
        }] if rejection.cause == RejectionCause::BackendMismatch
            && rejection.min_libxmtp_version.is_none()
    ));
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

/// A catalogue entry no client could register makes the whole answer
/// unusable, while tags this build does not know and an unsorted list do not:
/// a newer deployment may publish either, and the group registry decides.
// verifies: CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn a_catalogue_entry_no_client_could_register_is_rejected() {
    use xmtp_proto::xmtp::mls::message_contents::{
        ComponentPermissions, MetadataPolicy, metadata_policy::Kind,
    };
    let policy = || MetadataPolicy {
        kind: Some(Kind::Base(99)),
    };
    let entry = |component_id: u32, name: &str| backend_v1::ApplicationComponentDefinition {
        component_id,
        name: name.to_owned(),
        component_type: 99,
        permissions: Some(ComponentPermissions {
            insert_policy: Some(policy()),
            update_policy: Some(policy()),
            delete_policy: Some(policy()),
        }),
        in_groups: false,
        in_dms: true,
    };
    let response = |application_components| backend_v1::GetConfigurationResponse {
        identifier: "org.example.one".to_owned(),
        application_components,
        ..Default::default()
    };
    let accepted = validated(&response(vec![entry(0xC001, "b"), entry(0xC000, "a")]))?;
    assert_eq!(accepted.application_components[0].component_id, 0xC001);
    for bad_id in [0xBFFF, 0xFF00] {
        assert_eq!(
            validated(&response(vec![entry(0xC000, "a"), entry(bad_id, "b")])),
            Err(ServerConfigurationError::ApplicationComponent {
                index: 1,
                reason: xmtp_configuration::ApplicationComponentError::ComponentId,
            }),
            "component ID {bad_id:#x}"
        );
    }
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod backend_tests {

    use std::sync::Arc;
    use xmtp_api_backend::{BackendClient, TrackedStatsClient};
    use xmtp_backend::test_support::TestServer;

    use xmtp_db::prelude::*;
    use xmtp_db::{TestDb, XmtpTestDb};

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
    // verifies: CONF-034, CONF-076
    #[xmtp_common::test(unwrap_try = true)]
    async fn an_offline_start_uses_the_stored_copy_without_checking_the_url() {
        let backend = TestServer::from_toml(DISTINCT).await?;
        let store = TestDb::create_ephemeral_store().await;
        crate::server_configuration::resolve(
            &xmtp_api::ApiClientWrapper::new(Arc::new(api_at(&backend.url)), Default::default()),
            &store.db(),
            false,
        )
        .await?;
        backend.stop().await?;

        // A different URL, with nothing listening on it.
        let handle =
            crate::server_configuration::resolve(&unreachable_api(), &store.db(), true).await?;
        assert_eq!(handle.configuration().identifier, "org.example.distinct");
        assert_eq!(handle.configuration().limits.max_query_topics, 17);
        assert_eq!(handle.configuration().mls.max_group_members, 23);
        assert_eq!(catalogue(handle.configuration()), DISTINCT_CATALOGUE);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod preflight_controls;
