use super::*;
use xmtp_configuration::{
    BACKEND_DEFAULT_MAX_PUBLISH_TOPICS, BACKEND_DEFAULT_MAX_QUERY_LIMIT,
    BACKEND_DEFAULT_MAX_UPLOAD_BYTES, BACKEND_DEFAULT_WELCOME_SECONDS, ENABLE_COMMIT_LOG,
    MAX_GROUP_SIZE,
};

/// A response with every published field set, so a test can prove each one
/// survives the conversion.
fn populated() -> backend_v1::GetConfigurationResponse {
    backend_v1::GetConfigurationResponse {
        identifier: "org.example.xmtp".to_owned(),
        server_version: "1.2.3".to_owned(),
        min_libxmtp_version: "1.0.0".to_owned(),
        auth: Some(backend_v1::AuthConfiguration {
            enabled: true,
            keys: vec![backend_v1::auth_configuration::SigningKey {
                kid: "key-1".to_owned(),
                alg: "EdDSA".to_owned(),
            }],
            audiences: vec!["xmtp".to_owned()],
            issuers: vec!["https://issuer.example".to_owned()],
            required_scopes: vec!["messages:write".to_owned()],
        }),
        retention: Some(backend_v1::RetentionConfiguration {
            group_message_seconds: 11,
            welcome_seconds: 22,
            key_package_seconds: 33,
        }),
        limits: Some(backend_v1::LimitsConfiguration {
            max_envelope_bytes: 1,
            max_request_bytes: 2,
            max_response_bytes: 3,
            max_publish_topics: 4,
            max_query_topics: 5,
            max_query_limit: 6,
            default_query_limit: 7,
            max_newest_metadata_topics: 8,
            max_newest_full_topics: 9,
            max_update_adds: 10,
            max_update_removes: 11,
            max_stream_topics: 12,
            max_static_topics: 13,
            max_lookup_identifiers: 14,
            max_scw_signatures: 15,
            max_identity_entries: 16,
            max_update_frames_per_second: 17,
            max_update_burst: 18,
            max_ping_frames_per_second: 19,
            max_ping_burst: 20,
        }),
        mls: Some(backend_v1::MlsConfiguration {
            max_group_members: 42,
            max_installations_per_inbox: 7,
            commit_log_enabled: Some(false),
        }),
        smart_contract_wallet_chains: vec!["eip155:1".to_owned(), "eip155:8453".to_owned()],
        attachments: None,
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn every_published_field_survives_the_conversion() {
    let configuration = ServerConfiguration::from(populated());

    assert_eq!(configuration.identifier, "org.example.xmtp");
    assert_eq!(configuration.server_version, "1.2.3");
    assert_eq!(configuration.min_libxmtp_version, "1.0.0");

    assert!(configuration.auth.enabled);
    assert_eq!(configuration.auth.keys.len(), 1);
    assert_eq!(configuration.auth.keys[0].kid, "key-1");
    assert_eq!(configuration.auth.keys[0].alg, "EdDSA");
    assert_eq!(configuration.auth.audiences, vec!["xmtp".to_owned()]);
    assert_eq!(
        configuration.auth.issuers,
        vec!["https://issuer.example".to_owned()]
    );
    assert_eq!(
        configuration.auth.required_scopes,
        vec!["messages:write".to_owned()]
    );

    assert_eq!(configuration.retention.group_message_seconds, 11);
    assert_eq!(configuration.retention.welcome_seconds, 22);
    assert_eq!(configuration.retention.key_package_seconds, 33);

    let limits = &configuration.limits;
    assert_eq!(limits.max_envelope_bytes, 1);
    assert_eq!(limits.max_request_bytes, 2);
    assert_eq!(limits.max_response_bytes, 3);
    assert_eq!(limits.max_publish_topics, 4);
    assert_eq!(limits.max_query_topics, 5);
    assert_eq!(limits.max_query_limit, 6);
    assert_eq!(limits.default_query_limit, 7);
    assert_eq!(limits.max_newest_metadata_topics, 8);
    assert_eq!(limits.max_newest_full_topics, 9);
    assert_eq!(limits.max_update_adds, 10);
    assert_eq!(limits.max_update_removes, 11);
    assert_eq!(limits.max_stream_topics, 12);
    assert_eq!(limits.max_static_topics, 13);
    assert_eq!(limits.max_lookup_identifiers, 14);
    assert_eq!(limits.max_scw_signatures, 15);
    assert_eq!(limits.max_identity_entries, 16);
    assert_eq!(limits.max_update_frames_per_second, 17);
    assert_eq!(limits.max_update_burst, 18);
    assert_eq!(limits.max_ping_frames_per_second, 19);
    assert_eq!(limits.max_ping_burst, 20);

    assert_eq!(configuration.mls.max_group_members, 42);
    assert_eq!(configuration.mls.max_installations_per_inbox, 7);
    assert_eq!(configuration.mls.commit_log_enabled, Some(false));
    assert!(!configuration.mls.commit_log_enabled());

    assert_eq!(
        configuration.smart_contract_wallet_chains,
        vec!["eip155:1".to_owned(), "eip155:8453".to_owned()]
    );
}

// verifies: CONF-025
#[xmtp_common::test(unwrap_try = true)]
fn a_zero_field_reads_as_the_compiled_default() {
    let mut response = populated();
    let limits = response.limits.as_mut()?;
    limits.max_publish_topics = 0;
    limits.max_query_limit = 0;
    response.retention.as_mut()?.welcome_seconds = 0;
    response.mls.as_mut()?.max_group_members = 0;

    let configuration = ServerConfiguration::from(response);
    assert_eq!(
        configuration.limits.max_publish_topics,
        BACKEND_DEFAULT_MAX_PUBLISH_TOPICS
    );
    assert_eq!(
        configuration.limits.max_query_limit,
        BACKEND_DEFAULT_MAX_QUERY_LIMIT
    );
    assert_eq!(
        configuration.retention.welcome_seconds,
        BACKEND_DEFAULT_WELCOME_SECONDS
    );
    assert_eq!(configuration.mls.max_group_members, MAX_GROUP_SIZE);
    // The values that were not zeroed still come from the wire.
    assert_eq!(configuration.limits.max_query_topics, 5);
}

// verifies: CONF-025
#[xmtp_common::test(unwrap_try = true)]
fn an_absent_submessage_reads_as_every_compiled_default() {
    let response = backend_v1::GetConfigurationResponse {
        identifier: "org.example.xmtp".to_owned(),
        ..Default::default()
    };
    let configuration = ServerConfiguration::from(response);

    assert_eq!(configuration.limits, LimitsConfiguration::default());
    assert_eq!(configuration.retention, RetentionConfiguration::default());
    assert_eq!(configuration.mls, MlsConfiguration::default());
    assert!(!configuration.auth.enabled);
    // Absent, so the client keeps its compiled commit-log default.
    assert_eq!(configuration.mls.commit_log_enabled, None);
    assert_eq!(configuration.mls.commit_log_enabled(), ENABLE_COMMIT_LOG);
    assert!(configuration.smart_contract_wallet_chains.is_empty());
    configuration.validate()?;
}

#[xmtp_common::test(unwrap_try = true)]
fn the_stored_bytes_round_trip_through_prost() {
    use prost::Message;

    let response = populated();
    let encoded = response.encode_to_vec();
    let decoded = backend_v1::GetConfigurationResponse::decode(encoded.as_slice())?;
    assert_eq!(
        ServerConfiguration::from(decoded),
        ServerConfiguration::from(populated())
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn attachments_absent_is_none() {
    let configuration = ServerConfiguration::from(populated());
    assert!(configuration.attachments.is_none());
    configuration.validate()?;
}

// verifies: CONF-025
#[xmtp_common::test(unwrap_try = true)]
fn attachments_zero_max_reads_default() {
    let mut response = populated();
    response.attachments = Some(backend_v1::AttachmentsConfiguration {
        base_url: "http://localhost/attachments".to_owned(),
        max_upload_bytes: 0,
        retention_seconds: 86_400,
    });

    let configuration = ServerConfiguration::from(response);
    let attachments = configuration.attachments.as_ref()?;
    assert_eq!(
        attachments.base_url.as_str(),
        "http://localhost/attachments"
    );
    assert_eq!(
        attachments.max_upload_bytes,
        BACKEND_DEFAULT_MAX_UPLOAD_BYTES
    );
    assert_eq!(attachments.retention_seconds, 86_400);
    configuration.validate()?;
}

// verifies: ATCH-008
#[xmtp_common::test(unwrap_try = true)]
fn unusable_attachments_is_none() {
    for (base_url, max_upload_bytes) in [
        ("http://example.com/attachments", 1),
        ("https://example.com/attachments?key=value", 1),
        ("https://example.com/attachments#section", 1),
        ("https://example.com/attachments/", 1),
        ("https://example.com/attachments/ ", 1),
        ("https://example.com/attachments/\n", 1),
        ("https://example.com/a/..", 1),
        (" https://example.com/a", 1),
        (r"https:\\example.com\a", 1),
        ("https:example.com/a", 1),
        ("https://example.com/attachments", 4_294_967_296),
    ] {
        let mut response = populated();
        response.attachments = Some(backend_v1::AttachmentsConfiguration {
            base_url: base_url.to_owned(),
            max_upload_bytes,
            retention_seconds: 0,
        });
        let configuration = ServerConfiguration::from(response);
        assert!(configuration.attachments.is_none(), "{base_url}");
        configuration.validate()?;
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn usable_attachments_keep_the_offer() {
    for base_url in [
        "https://example.com/attachments",
        "https://example.com",
        "http://127.0.0.1:9000",
        "http://127.0.0.1/attachments",
        "http://[::1]/attachments",
    ] {
        let mut response = populated();
        response.attachments = Some(backend_v1::AttachmentsConfiguration {
            base_url: base_url.to_owned(),
            max_upload_bytes: u32::MAX as u64,
            retention_seconds: 0,
        });
        let configuration = ServerConfiguration::from(response);
        let attachments = configuration.attachments.as_ref()?;
        assert_eq!(attachments.base_url, base_url);
        assert_eq!(attachments.max_upload_bytes, u32::MAX as u64);
        configuration.validate()?;
    }
}
