use crate::{
    api,
    test_support::{DEFAULT_TEST_IDENTIFIER, TestServer, auth::TestKey},
};

/// Everything an operator sets in TOML must come back on the wire, and the
/// response must be reachable without a credential.
#[xmtp_common::test(unwrap_try = true)]
async fn published_settings_round_trip_from_the_configuration_file() {
    let server = TestServer::from_toml(
        "[server]
identifier = 'org.xmtp.round-trip'
min_libxmtp_version = '1.2.3'
[limits]
default_query_limit = 12
max_query_limit = 37
[mls]
max_group_members = 42
commit_log_enabled = false
[retention]
group_message_seconds = 604800
[chains]
'eip155:1' = 'https://chain.example.com'
'eip155:8453' = 'https://base.example.com'
",
    )
    .await?;
    let published = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(published.identifier, "org.xmtp.round-trip");
    assert_eq!(published.server_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(published.min_libxmtp_version, "1.2.3");
    let limits = published.limits.as_ref().expect("limits are published");
    assert_eq!(limits.max_query_limit, 37);
    assert_eq!(limits.default_query_limit, 12);
    let mls = published.mls.as_ref().expect("mls is published");
    assert_eq!(mls.max_group_members, 42);
    assert_eq!(mls.max_installations_per_inbox, 10);
    assert_eq!(mls.commit_log_enabled, Some(false));
    let retention = published
        .retention
        .as_ref()
        .expect("retention is published");
    assert_eq!(retention.group_message_seconds, 604_800);
    assert_eq!(
        published.smart_contract_wallet_chains,
        ["eip155:1", "eip155:8453"]
    );
    server.stop().await?;
}

/// Zero would tell a client to fall back to a compiled default that the backend
/// does not enforce, so every published number is filled from the configuration.
#[xmtp_common::test(unwrap_try = true)]
async fn every_limit_retention_and_mls_field_is_filled() {
    let server = TestServer::new(|_| {}).await?;
    let published = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(published.identifier, DEFAULT_TEST_IDENTIFIER);
    assert!(published.min_libxmtp_version.is_empty());
    let limits = published.limits.expect("limits are published");
    for value in [
        limits.max_envelope_bytes,
        limits.max_request_bytes,
        limits.max_response_bytes,
    ] {
        assert_ne!(value, 0);
    }
    for value in [
        limits.max_publish_topics,
        limits.max_query_topics,
        limits.max_query_limit,
        limits.default_query_limit,
        limits.max_newest_metadata_topics,
        limits.max_newest_full_topics,
        limits.max_update_adds,
        limits.max_update_removes,
        limits.max_stream_topics,
        limits.max_static_topics,
        limits.max_lookup_identifiers,
        limits.max_scw_signatures,
        limits.max_identity_entries,
        limits.max_update_frames_per_second,
        limits.max_update_burst,
        limits.max_ping_frames_per_second,
        limits.max_ping_burst,
    ] {
        assert_ne!(value, 0);
    }
    let retention = published.retention.expect("retention is published");
    for value in [
        retention.group_message_seconds,
        retention.welcome_seconds,
        retention.key_package_seconds,
    ] {
        assert_ne!(value, 0);
    }
    let mls = published.mls.expect("mls is published");
    assert_eq!(mls.max_group_members, 250);
    assert_eq!(mls.max_installations_per_inbox, 10);
    assert_eq!(mls.commit_log_enabled, Some(true));
    server.stop().await?;
}

/// A deployment that checks no credential still says so explicitly, and says
/// nothing else about auth.
#[xmtp_common::test(unwrap_try = true)]
async fn disabled_auth_publishes_an_empty_summary() {
    let server =
        TestServer::from_toml("[auth]\nenabled = false\naudiences = ['ignored']\n").await?;
    let auth = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner()
        .auth
        .expect("auth is published");
    assert_eq!(auth, api::AuthConfiguration::default());
    server.stop().await?;
}

/// A client must be able to learn what it needs before it holds a credential,
/// so the call succeeds with no authorization header at all.
#[xmtp_common::test(unwrap_try = true)]
async fn enabled_auth_publishes_its_admission_settings_without_a_credential() {
    let key = TestKey::es256();
    let server = TestServer::new(|config| {
        let mut auth = key.auth_config();
        auth.audiences = Some(vec!["xmtp".into()]);
        auth.issuers = Some(vec!["https://issuer.example.com".into()]);
        auth.required_scopes = vec!["publish".into()];
        config.auth = Some(auth);
    })
    .await?;
    let published = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner()
        .auth
        .expect("auth is published");
    assert!(published.enabled);
    assert_eq!(
        published.keys,
        vec![api::auth_configuration::SigningKey {
            kid: key.kid.clone(),
            alg: "ES256".into(),
        }]
    );
    assert_eq!(published.audiences, ["xmtp"]);
    assert_eq!(published.issuers, ["https://issuer.example.com"]);
    assert_eq!(published.required_scopes, ["publish"]);
    // Every other service still demands a credential.
    let rejected = server
        .query()
        .query(api::QueryRequest::default())
        .await
        .expect_err("an uncredentialed query is rejected");
    assert_eq!(rejected.code(), tonic::Code::Unauthenticated);
    server.stop().await?;
}

/// The response is built once at startup, so repeated calls are identical.
#[xmtp_common::test(unwrap_try = true)]
async fn the_same_response_is_returned_for_the_life_of_the_process() {
    let server = TestServer::new(|_| {}).await?;
    let mut client = server.configuration();
    let first = client
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    let second = client
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(first, second);
    server.stop().await?;
}

/// Key material, URLs, and operational timing never leave the process.
#[xmtp_common::test(unwrap_try = true)]
async fn the_response_never_carries_a_url_or_key_material() {
    let key = TestKey::es256();
    let server = TestServer::from_toml(
        "[chains]
'eip155:1' = 'https://secret-chain-endpoint.example.com/private'
",
    )
    .await?;
    let encoded = format!(
        "{:?}",
        server
            .configuration()
            .get_configuration(api::GetConfigurationRequest {})
            .await?
            .into_inner()
    );
    assert!(!encoded.contains("secret-chain-endpoint"));
    assert!(!encoded.contains(&key.public_key));
    assert!(!encoded.contains("leeway"));
    assert!(!encoded.contains("postgres"));
    server.stop().await?;
}
