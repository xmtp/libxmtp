use super::*;

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
    for bad in ["", "eip155", ":1", "eip155:", "EIP155:1", "eip155:1:2", "1"] {
        assert!(!is_caip2_chain_id(bad), "{bad:?} must be rejected");
    }
}

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

#[xmtp_common::test(unwrap_try = true)]
fn an_absent_minimum_admits_every_client_version() {
    let configuration = ServerConfiguration {
        identifier: "org.example.xmtp".to_owned(),
        ..Default::default()
    };
    assert!(configuration.minimum_version()?.is_none());
}

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
