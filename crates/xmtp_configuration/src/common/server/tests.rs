use super::*;

// verifies: CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn identifier_rules_reject_empty_oversized_and_unprintable_names() {
    validate_server_identifier("org.example.xmtp")?;
    validate_server_identifier(&"a".repeat(MAX_SERVER_IDENTIFIER_BYTES))?;
    for bad in [
        "",
        "org.example xmtp",
        "org.example\txmtp",
        "org.example\nxmtp",
        "org.\u{0}xmtp",
    ] {
        assert_eq!(
            validate_server_identifier(bad),
            Err(ServerConfigurationError::Identifier),
            "{bad:?} must be rejected"
        );
    }
    assert_eq!(
        validate_server_identifier(&"a".repeat(MAX_SERVER_IDENTIFIER_BYTES + 1)),
        Err(ServerConfigurationError::Identifier)
    );
}

// verifies: CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn caip2_shape_accepts_known_namespaces_and_rejects_malformed_entries() {
    for good in [
        "eip155:1",
        "eip155:31337",
        "solana:mainnet-beta",
        "cosmos:x",
    ] {
        assert!(is_caip2_chain_id(good), "{good:?} must be accepted");
    }
    // The namespace grammar is three to eight characters, so a shorter one is
    // not a chain this deployment could verify on, whatever it looks like.
    for bad in [
        "",
        "eip155",
        ":1",
        "eip155:",
        "EIP155:1",
        "eip155:1:2",
        "1",
        "x:1",
        "ab:1",
        "toolongns:1",
    ] {
        assert!(!is_caip2_chain_id(bad), "{bad:?} must be rejected");
    }
}

// verifies: CONF-050
#[xmtp_common::test(unwrap_try = true)]
fn version_comparison_ignores_the_prerelease_tag() {
    let minimum = semver::Version::parse("1.2.3")?;
    assert!(version_is_below(
        &semver::Version::parse("1.2.2")?,
        &minimum
    ));
    assert!(!version_is_below(
        &semver::Version::parse("1.2.3")?,
        &minimum
    ));
    // Equal on major, minor, and patch with a prerelease tag is not below it.
    assert!(!version_is_below(
        &semver::Version::parse("1.2.3-dev")?,
        &minimum
    ));
    assert!(!version_is_below(
        &semver::Version::parse("1.3.0")?,
        &minimum
    ));
}

// verifies: CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn validation_names_the_field_that_failed() {
    let mut configuration = ServerConfiguration {
        identifier: "org.example.xmtp".to_owned(),
        ..Default::default()
    };
    configuration.validate()?;

    configuration.min_libxmtp_version = "not-a-version".to_owned();
    assert_eq!(
        configuration.validate(),
        Err(ServerConfigurationError::MinimumVersion {
            version: "not-a-version".to_owned()
        })
    );

    configuration.min_libxmtp_version = "2.0.0".to_owned();
    configuration.smart_contract_wallet_chains = vec!["eip155:1".into(), "nonsense".into()];
    assert_eq!(
        configuration.validate(),
        Err(ServerConfigurationError::Chain {
            chain: "nonsense".to_owned()
        })
    );

    configuration.smart_contract_wallet_chains = vec!["eip155:1".into()];
    configuration.identifier = String::new();
    assert_eq!(
        configuration.validate(),
        Err(ServerConfigurationError::Identifier)
    );
}

// A `uint64` above `2^53 - 1` cannot reach a JavaScript app
// intact, so the client refuses the configuration instead of reading a rounded
// value. The bound itself is acceptable.
// verifies: CONF-071
#[xmtp_common::test(unwrap_try = true)]
fn a_value_a_javascript_number_would_round_is_refused() {
    let mut configuration = ServerConfiguration {
        identifier: "org.example.xmtp".to_owned(),
        ..Default::default()
    };
    configuration.retention.group_message_seconds = MAX_PUBLISHED_VALUE;
    configuration.validate()?;

    configuration.retention.group_message_seconds = MAX_PUBLISHED_VALUE + 1;
    assert_eq!(
        configuration.validate(),
        Err(ServerConfigurationError::Magnitude {
            field: "group_message_seconds",
            value: MAX_PUBLISHED_VALUE + 1,
        })
    );
    // The rounding this prevents: the first `f64` that is not itself.
    assert_ne!(
        (MAX_PUBLISHED_VALUE + 2) as f64 as u64,
        MAX_PUBLISHED_VALUE + 2
    );

    configuration.retention.group_message_seconds = 1;
    // Every limit and every MLS value is covered, not just retention.
    #[cfg(target_pointer_width = "64")]
    {
        configuration.limits.max_query_limit = MAX_PUBLISHED_VALUE as usize + 1;
        assert_eq!(
            configuration.validate(),
            Err(ServerConfigurationError::Magnitude {
                field: "max_query_limit",
                value: MAX_PUBLISHED_VALUE + 1,
            })
        );
        configuration.limits.max_query_limit = 1;

        configuration.mls.max_group_members = MAX_PUBLISHED_VALUE as usize + 1;
        assert_eq!(
            configuration.validate(),
            Err(ServerConfigurationError::Magnitude {
                field: "max_group_members",
                value: MAX_PUBLISHED_VALUE + 1,
            })
        );
        configuration.mls.max_group_members = 1;
    }
    configuration.validate()?;
}

#[xmtp_common::test(unwrap_try = true)]
fn an_absent_minimum_admits_every_client_version() {
    let configuration = ServerConfiguration {
        identifier: "org.example.xmtp".to_owned(),
        ..Default::default()
    };
    assert!(configuration.minimum_version()?.is_none());
}

// verifies: CONF-025
#[xmtp_common::test(unwrap_try = true)]
fn commit_log_distinguishes_absent_from_false() {
    let mut mls = MlsConfiguration::default();
    assert_eq!(mls.commit_log_enabled(), ENABLE_COMMIT_LOG);
    mls.commit_log_enabled = Some(false);
    assert!(!mls.commit_log_enabled());
    mls.commit_log_enabled = Some(true);
    assert!(mls.commit_log_enabled());
}

#[xmtp_common::test(unwrap_try = true)]
fn an_empty_chain_list_accepts_nothing() {
    let configuration = ServerConfiguration::default();
    assert!(!configuration.accepts_chain("eip155:1"));
    let configuration = ServerConfiguration {
        smart_contract_wallet_chains: vec!["eip155:1".into()],
        ..Default::default()
    };
    assert!(configuration.accepts_chain("eip155:1"));
    assert!(!configuration.accepts_chain("eip155:8453"));
}

#[xmtp_common::test(unwrap_try = true)]
fn providers_return_the_value_they_were_built_with() {
    let static_provider = StaticConfigProvider::edited(|configuration| {
        configuration.limits.max_query_topics = 7;
    });
    assert_eq!(
        static_provider
            .server_configuration()
            .limits
            .max_query_topics,
        7
    );
    // Every other field keeps the compiled default.
    assert_eq!(
        static_provider
            .server_configuration()
            .limits
            .max_publish_topics,
        BACKEND_DEFAULT_MAX_PUBLISH_TOPICS
    );
    let stored = StoredConfigProvider::new(ServerConfiguration {
        identifier: "org.example.stored".to_owned(),
        ..Default::default()
    });
    assert_eq!(
        stored.server_configuration().identifier,
        "org.example.stored"
    );
}

// verifies: CONF-025
#[xmtp_common::test(unwrap_try = true)]
fn zero_limits_fall_back_to_the_compiled_defaults() {
    // A snapshot an app built in Rust never passes through the wire
    // conversion, so it can carry a zero the transport would divide by.
    let supplied = LimitsConfiguration {
        max_static_topics: 0,
        max_newest_metadata_topics: 0,
        max_query_topics: 9,
        ..Default::default()
    };
    let sanitized = supplied.without_zeroes();
    assert_eq!(
        sanitized.max_static_topics,
        BACKEND_DEFAULT_MAX_STATIC_TOPICS
    );
    assert_eq!(
        sanitized.max_newest_metadata_topics,
        BACKEND_DEFAULT_MAX_NEWEST_METADATA_TOPICS
    );
    // A value the app did supply is kept.
    assert_eq!(sanitized.max_query_topics, 9);
    // An all-zero snapshot comes back as the compiled defaults, whole.
    let empty = LimitsConfiguration {
        max_envelope_bytes: 0,
        max_request_bytes: 0,
        max_response_bytes: 0,
        max_publish_topics: 0,
        max_query_topics: 0,
        max_query_limit: 0,
        default_query_limit: 0,
        max_newest_metadata_topics: 0,
        max_newest_full_topics: 0,
        max_update_adds: 0,
        max_update_removes: 0,
        max_stream_topics: 0,
        max_static_topics: 0,
        max_lookup_identifiers: 0,
        max_scw_signatures: 0,
        max_identity_entries: 0,
        max_update_frames_per_second: 0,
        max_update_burst: 0,
        max_ping_frames_per_second: 0,
        max_ping_burst: 0,
    };
    assert_eq!(empty.without_zeroes(), LimitsConfiguration::default());
}
