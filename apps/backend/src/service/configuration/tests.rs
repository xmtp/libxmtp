use crate::{
    api,
    test_support::{DEFAULT_TEST_IDENTIFIER, TestServer, auth::TestKey},
};

/// Everything an operator sets in TOML must come back on the wire, and the
/// response must be reachable without a credential.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-011, CONF-012, CONF-069, CONF-070
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
    for value in [
        limits.max_envelope_bytes,
        limits.max_request_bytes,
        limits.max_response_bytes,
        u64::from(limits.max_publish_topics),
        u64::from(limits.max_query_topics),
        u64::from(limits.max_query_limit),
        u64::from(limits.default_query_limit),
        u64::from(limits.max_newest_metadata_topics),
        u64::from(limits.max_newest_full_topics),
        u64::from(limits.max_update_adds),
        u64::from(limits.max_update_removes),
        u64::from(limits.max_stream_topics),
        u64::from(limits.max_static_topics),
        u64::from(limits.max_lookup_identifiers),
        u64::from(limits.max_scw_signatures),
        u64::from(limits.max_identity_entries),
        u64::from(limits.max_update_frames_per_second),
        u64::from(limits.max_update_burst),
        u64::from(limits.max_ping_frames_per_second),
        u64::from(limits.max_ping_burst),
    ] {
        assert_ne!(value, 0);
    }
    let mls = published.mls.as_ref().expect("mls is published");
    assert_eq!(mls.max_group_members, 42);
    assert_eq!(mls.max_installations_per_inbox, 10);
    assert_eq!(mls.commit_log_enabled, Some(false));
    let retention = published
        .retention
        .as_ref()
        .expect("retention is published");
    assert_eq!(retention.group_message_seconds, 604_800);
    for value in [
        retention.group_message_seconds,
        retention.welcome_seconds,
        retention.key_package_seconds,
    ] {
        assert_ne!(value, 0);
    }
    assert_eq!(
        published.smart_contract_wallet_chains,
        ["eip155:1", "eip155:8453"]
    );
    let second = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(published, second);
    let encoded = format!("{published:?}");
    assert!(!encoded.contains("chain.example.com"));
    assert!(!encoded.contains("base.example.com"));
    assert!(!encoded.contains("postgres"));
    server.stop().await?;
}

/// A deployment that checks no credential still says so explicitly, and says
/// nothing else about auth.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-068
async fn disabled_auth_publishes_an_empty_summary() {
    let server =
        TestServer::from_toml("[auth]\nenabled = false\naudiences = ['ignored']\n").await?;
    let published = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(published.identifier, DEFAULT_TEST_IDENTIFIER);
    assert!(published.min_libxmtp_version.is_empty());
    let mls = published.mls.as_ref().expect("mls is published");
    assert_eq!(mls.max_group_members, 250);
    assert_eq!(mls.max_installations_per_inbox, 10);
    assert_eq!(mls.commit_log_enabled, Some(true));
    let auth = published.auth.expect("auth is published");
    assert_eq!(auth, api::AuthConfiguration::default());
    server.stop().await?;
}

/// A client must be able to learn what it needs before it holds a credential,
/// so the call succeeds with no authorization header at all.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-010, CONF-011, CONF-068
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
    let response = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    let encoded = prost::Message::encode_to_vec(&response);
    let public_key = key.public_key.as_bytes();
    assert!(!public_key.is_empty());
    assert!(
        !encoded
            .windows(public_key.len())
            .any(|bytes| bytes == public_key)
    );
    let published = response.auth.expect("auth is published");
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

/// A deployment may publish a response budget smaller than its own
/// configuration response. The startup check bounds this one response at 64
/// KiB; `limits.max_response_bytes` bounds the envelopes clients read, and a
/// small one must not make the configuration itself undeliverable.
#[xmtp_common::test(unwrap_try = true)]
async fn a_small_response_budget_still_delivers_the_configuration() {
    // The smallest response budget a one-byte envelope allows.
    const BUDGET: usize = 1 + crate::config::ENVELOPE_METADATA_AND_FRAMING_BYTES;
    let server = TestServer::from_toml(
        "[limits]
max_envelope_bytes = 1
max_request_bytes = 257
max_response_bytes = 257
[chains]
'eip155:1' = 'https://one.example.com'
'eip155:10' = 'https://ten.example.com'
'eip155:56' = 'https://fifty-six.example.com'
'eip155:100' = 'https://one-hundred.example.com'
'eip155:137' = 'https://one-three-seven.example.com'
'eip155:250' = 'https://two-fifty.example.com'
'eip155:8453' = 'https://base.example.com'
'eip155:42161' = 'https://arbitrum.example.com'
'eip155:43114' = 'https://avalanche.example.com'
'eip155:59144' = 'https://linea.example.com'
'eip155:81457' = 'https://blast.example.com'
'eip155:534352' = 'https://scroll.example.com'
'eip155:7777777' = 'https://zora.example.com'
'eip155:11155111' = 'https://sepolia.example.com'
'eip155:84532' = 'https://base-sepolia.example.com'
'eip155:421614' = 'https://arbitrum-sepolia.example.com'
",
    )
    .await?;
    let published = server
        .configuration()
        .get_configuration(api::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(published.smart_contract_wallet_chains.len(), 16);
    // The call above only means something while the response is larger than the
    // budget the deployment published for everything else.
    let encoded = prost::Message::encoded_len(&published);
    assert!(
        encoded > BUDGET,
        "this deployment's configuration response is {encoded} bytes, so it no \
         longer exercises a response budget smaller than itself"
    );
    server.stop().await?;
}
