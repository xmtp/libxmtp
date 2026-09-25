use super::*;

#[xmtp_common::test(unwrap_try = true)]
fn logging_defaults_and_overrides_are_typed() {
    let config: Config = toml::from_str(MINIMAL)?;
    assert!(matches!(config.server.log_level, LogLevel::Info));
    assert!(config.server.request_logger);
    for level in ["off", "error", "warn", "info", "debug", "trace"] {
        let config: Config = toml::from_str(&format!(
            "{MINIMAL}log_level = '{level}'\nrequest_logger = false"
        ))?;
        let shared: xmtp_logging::Level = config.server.log_level.into();
        assert_eq!(shared.as_str(), level);
        assert!(!config.server.request_logger);
    }
    assert!(toml::from_str::<Config>(&format!("{MINIMAL}log_level = 'verbose'")).is_err());
}
/// A configuration that starts. The `[server]` section comes last and is left
/// open, so a test appends its own server keys directly and any other section
/// with its own header.
const MINIMAL: &str =
    "[database]\nurl = 'postgres://localhost/xmtp'\n[server]\nidentifier = 'org.xmtp.test'\n";
const IDENTIFIER: &str = "org.xmtp.test";
const RESPONSE_TEST_ENVELOPE_BYTES: usize = 1_000_000;
const RESPONSE_TEST_REQUEST_BYTES: usize = 2_000_000;

// verifies: ATCH-073, CONF-065
#[xmtp_common::test(unwrap_try = true)]
fn invalid_attachment_credential_sources_name_the_key() {
    let source = format!(
        "{MINIMAL}\n[attachments]\nbase_url = 'https://example.com/attachments'\n\
         [attachments.target.S3]\nendpoint = 'https://s3.example.com'\n\
         region = 'us-east-1'\nbucket = 'attachments'\n\
         [attachments.target.S3.credentials]\n"
    );
    for (fields, expected_key) in [
        ("kind = 'unknown'", "attachments.target.S3.credentials.kind"),
        ("kind = 'profile'", "attachments.target.S3.credentials.name"),
    ] {
        let error = Config::load_str(&format!("{source}{fields}\n")).unwrap_err();
        assert!(
            matches!(&error, ConfigError::Invalid { field, .. } if *field == expected_key),
            "{error}"
        );
        assert!(error.to_string().contains(expected_key));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn push_defaults_provider_fields_and_redaction() {
    let config = Config::load_str(MINIMAL)?;
    assert_eq!(config.push.recipient_ttl_seconds, 2_592_000);
    assert_eq!(config.push.max_attempts, 3);
    assert_eq!(config.limits.max_push_topics, 100_000);
    assert!(config.push.apns.is_none() && config.push.fcm.is_none() && config.push.http.is_none());
    let source = format!(
        "{MINIMAL}\n[push.apns]\nkey = 'private-apns-credential'\nkey_id = 'key'\nteam_id = 'team'\nbundle_id = 'bundle'\nenvironment = 'sandbox'\n[push.fcm]\nservice_account = 'private-fcm-credential'\n[push.http]\n"
    );
    let config = Config::load_str(&source)?;
    assert!(config.push.http.is_some());
    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("private-apns-credential"));
    assert!(!debug.contains("private-fcm-credential"));
    let source = source
        .replace("private-apns-credential", "env:CARGO_MANIFEST_DIR")
        .replace("private-fcm-credential", "env:CARGO_MANIFEST_DIR");
    let config = Config::load_str(&source)?;
    let expected = std::env::var("CARGO_MANIFEST_DIR")?;
    assert_eq!(
        config.push.apns.unwrap().key.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(
        config.push.fcm.unwrap().service_account.as_deref(),
        Some(expected.as_str())
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn push_invalid_values_name_the_configuration_key() {
    for (extra, field) in [
        (
            "[push]\nrecipient_ttl_seconds = -1",
            "push.recipient_ttl_seconds",
        ),
        ("[push]\nmax_attempts = -1", "push.max_attempts"),
        ("[push]\nmax_attempts = 4294967296", "push.max_attempts"),
        (
            "[push]\nrecipient_ttl_seconds = 86399",
            "push.recipient_ttl_seconds",
        ),
        ("[push]\nmax_attempts = 0", "push.max_attempts"),
        ("[push]\nmax_attempts = 11", "push.max_attempts"),
        ("[push.apns]", "push.apns.key"),
        ("[push.apns]\nkey = 'key'", "push.apns.key_id"),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'",
            "push.apns.team_id",
        ),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'\nteam_id = 'team'",
            "push.apns.bundle_id",
        ),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'\nteam_id = 'team'\nbundle_id = 'bundle'",
            "push.apns.environment",
        ),
        (
            "[push.apns]\nkey = 'key'\nkey_id = 'id'\nteam_id = 'team'\nbundle_id = 'bundle'\nenvironment = 'invalid'",
            "push.apns.environment",
        ),
        ("[push.fcm]", "push.fcm.service_account"),
        (
            "[push.fcm]\nservice_account = ''",
            "push.fcm.service_account",
        ),
        ("[limits]\nmax_push_topics = 0", "limits.max_push_topics"),
    ] {
        let error = Config::load_str(&format!("{MINIMAL}\n{extra}")).unwrap_err();
        assert!(
            matches!(error, ConfigError::Invalid { field: actual, .. } if actual == field),
            "{error}"
        );
    }
    for domain in [
        "*",
        "a.*.org",
        "a*.org",
        "example..org",
        "",
        ".example.org",
        "example.org.",
        "https://example.org",
        "example.org:443",
        "example.org/path",
        "-example.org",
        "example-.org",
    ] {
        let error = Config::load_str(&format!(
            "{MINIMAL}\n[push.http]\nallowed_domains = ['{domain}']"
        ))
        .unwrap_err();
        assert!(matches!(
            error,
            ConfigError::Invalid {
                field: "push.http.allowed_domains",
                ..
            }
        ));
    }
    Config::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = 86400\nmax_attempts = 10\n[push.http]\nallowed_domains = ['hooks.example.com', '*.EXAMPLE.org']"
    ))?;
}

#[xmtp_common::test(unwrap_try = true)]
fn push_expiry_is_checked_at_both_duration_and_clock_boundaries() {
    let config = Config::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = {MAX_RETENTION_SECONDS}"
    ))?;
    assert_eq!(
        config.push.expires_at(0)?,
        MAX_RETENTION_SECONDS as i64 * NS_IN_SEC
    );
    assert!(matches!(
        config.push.expires_at(NS_IN_SEC),
        Err(crate::error::Error::PushExpiryOverflow)
    ));
    assert!(matches!(
        config.push.expires_at(-1),
        Err(crate::error::Error::PushExpiryOverflow)
    ));
    let error = Config::load_str(&format!(
        "{MINIMAL}\n[push]\nrecipient_ttl_seconds = {}",
        MAX_RETENTION_SECONDS + 1
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        ConfigError::Invalid {
            field: "push.recipient_ttl_seconds",
            ..
        }
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn minimal_configuration_uses_defaults() {
    let config: Config = toml::from_str(MINIMAL)?;
    config.validate()?;

    assert_eq!(config.server.listen, DEFAULT_LISTEN);
    assert_eq!(
        config.limits.max_request_bytes,
        BACKEND_DEFAULT_MAX_REQUEST_BYTES
    );
    assert!(config.chains.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
fn unknown_keys_are_rejected() {
    let error =
        toml::from_str::<Config>("[database]\nurl = 'postgres://localhost/xmtp'\nextra = true\n")
            .expect_err("unknown keys must fail");
    assert!(error.to_string().contains("unknown field"));
}

#[xmtp_common::test(unwrap_try = true)]
fn framing_bound_covers_maximum_metadata_and_nested_messages() {
    use crate::api;
    use prost::Message;

    const STORED_TOPIC_BYTES: usize = 128;
    const HASH_BYTES: usize = 32;
    let meta = api::EnvelopeMeta {
        cursor: Some(api::Cursor {
            sequence_id: u64::MAX,
        }),
        server_ns: u64::MAX,
        message_hash: Some(api::MessageHash {
            hash: Some(api::message_hash::Hash::Sha256(vec![0; HASH_BYTES])),
        }),
        topic: Some(api::Topic {
            topic: vec![0; STORED_TOPIC_BYTES],
        }),
        expiry_ns: u64::MAX,
        is_commit_or_proposal: true,
    };
    assert_eq!(meta.encoded_len(), MAX_METADATA_BYTES);
    for size in [
        0,
        127,
        128,
        16_383,
        16_384,
        BACKEND_DEFAULT_MAX_ENVELOPE_BYTES,
    ] {
        let envelope = api::ClientEnvelope {
            payload: Some(api::client_envelope::Payload::GroupMessage(
                api::GroupMessage {
                    data: vec![0; size],
                    ..Default::default()
                },
            )),
        };
        let bound = envelope.encoded_len() + ENVELOPE_METADATA_AND_FRAMING_BYTES;
        let response = api::SubscribeResponse {
            response: Some(api::subscribe_response::Response::Messages(
                api::subscribe_response::Messages {
                    envelopes: vec![api::ServerEnvelope {
                        meta: Some(meta.clone()),
                        envelope: Some(envelope),
                    }],
                },
            )),
        };
        assert!(response.encoded_len() + GRPC_HEADER_BYTES <= bound);
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-021
fn every_cross_field_relationship_is_rejected() {
    let mut config: Config = toml::from_str(MINIMAL)?;

    config.publishing.max_publish_duration_ms = config.database.max_statement_timeout_ms;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "publishing.max_publish_duration_ms",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.streams.max_pong_wait_ms = config.streams.keepalive_interval_ms;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "streams.max_pong_wait_ms",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_query_limit = config.limits.default_query_limit - 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_query_limit",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = config.limits.max_request_bytes + 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_envelope_bytes",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
    config.limits.max_request_bytes = RESPONSE_TEST_REQUEST_BYTES;
    config.limits.max_response_bytes = RESPONSE_TEST_ENVELOPE_BYTES;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_envelope_bytes",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_update_adds = config.limits.max_stream_topics + 1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_update_adds",
            ..
        })
    ));
    config = toml::from_str(MINIMAL)?;
    config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES - ENVELOPE_METADATA_AND_FRAMING_BYTES;
    config.limits.max_request_bytes = DELIVERY_FRAME_BYTES;
    config.limits.max_response_bytes = DELIVERY_FRAME_BYTES;
    config.validate()?;
    config.limits.max_envelope_bytes = DELIVERY_FRAME_BYTES;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Invalid {
            field: "limits.max_envelope_bytes",
            ..
        })
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn missing_and_empty_environment_references_fail_without_values() {
    let missing = write_config(
        "missing",
        "[database]\nurl = 'env:XMTP_BACKEND_CONFIG_MISSING_9F31'\n",
    );
    let missing_error = Config::load(&missing.0).expect_err("missing env must fail");
    assert!(
        missing_error
            .to_string()
            .contains("XMTP_BACKEND_CONFIG_MISSING_9F31")
    );

    let empty = write_config("empty", "[database]\nurl = 'env:'\n");
    let empty_error = Config::load(&empty.0).expect_err("empty env must fail");
    assert!(!empty_error.to_string().contains("env:"));
}

#[xmtp_common::test(unwrap_try = true)]
fn debug_output_redacts_urls() {
    let config: Config =
        toml::from_str("[database]\nurl = 'postgres://user:password@localhost/xmtp'\n")?;
    let debug = format!("{config:?}");
    assert!(!debug.contains("password"));
    assert!(debug.contains("<redacted>"));
}

#[xmtp_common::test(unwrap_try = true)]
fn published_schema_matches_generated_artifact() {
    let generated = serde_json::to_value(Config::schema())?;
    let published: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/schemas/backend-v1.json"))?;
    assert_eq!(generated, published);
    assert_eq!(generated["$id"], SCHEMA_ID);
}

#[xmtp_common::test(unwrap_try = true)]
fn environment_reference_returns_variable_value() {
    let path = std::env::var("PATH")?;
    assert_eq!(resolve_env("env:PATH")?, path);
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-020
fn finite_retention_must_fit_the_authoritative_database_clock() {
    const DATABASE_NS: i64 = 1_700_000_000_000_000_000;
    let maximum_seconds = ((i64::MAX - DATABASE_NS) / NS_IN_SEC) as u64;
    for field in [
        "retention.group_message_seconds",
        "retention.welcome_seconds",
        "retention.key_package_seconds",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        let duration = match field {
            "retention.group_message_seconds" => &mut config.retention.group_message_seconds,
            "retention.welcome_seconds" => &mut config.retention.welcome_seconds,
            _ => &mut config.retention.key_package_seconds,
        };
        *duration = maximum_seconds;
        config.retention.validate_at(DATABASE_NS)?;
        let mut invalid = config.retention.clone();
        match field {
            "retention.group_message_seconds" => invalid.group_message_seconds += 1,
            "retention.welcome_seconds" => invalid.welcome_seconds += 1,
            _ => invalid.key_package_seconds += 1,
        }
        assert!(
            matches!(invalid.validate_at(DATABASE_NS), Err(ConfigError::Invalid { field: actual, .. }) if actual == field)
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn configurable_deadlines_follow_the_host_monotonic_clock_range() {
    for milliseconds in [1, 60_000, u64::MAX] {
        let representable = xmtp_common::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(milliseconds))
            .is_some();
        assert_eq!(
            validate_deadline(milliseconds, "timer").is_ok(),
            representable
        );
    }
    for field in [
        "server.max_drain_duration_ms",
        "streams.poll_interval_ms",
        "streams.max_pong_wait_ms",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        match field {
            "server.max_drain_duration_ms" => config.server.max_drain_duration_ms = u64::MAX,
            "streams.poll_interval_ms" => config.streams.poll_interval_ms = u64::MAX,
            _ => config.streams.max_pong_wait_ms = u64::MAX,
        }
        if validate_deadline(u64::MAX, field).is_err() {
            assert!(
                matches!(config.validate(), Err(ConfigError::Invalid { field: actual, .. }) if actual == field)
            );
        } else {
            config.validate()?;
        }
    }
}

struct ConfigFile(std::path::PathBuf);
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_config(name: &str, source: &str) -> ConfigFile {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!(
        "xmtp-backend-config-{}-{name}.toml",
        xmtp_common::rand_hexstring()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("create test config");
    let guard = ConfigFile(path);
    file.write_all(source.as_bytes())
        .expect("write test config");
    guard
}

#[xmtp_common::test(unwrap_try = true)]
fn telemetry_defaults_endpoint_precedence_and_export_options() {
    let config: Config = toml::from_str(MINIMAL)?;
    assert_eq!(config.telemetry.metrics_listen, "0.0.0.0:9464");
    assert_eq!(config.server.log_format, LogFormat::Text);
    assert!(
        config
            .telemetry
            .with_endpoint_fallback(IDENTIFIER, None)?
            .is_none()
    );
    let fallback = config
        .telemetry
        .with_endpoint_fallback(IDENTIFIER, Some("http://tempo:4317".into()))?
        .unwrap();
    assert_eq!(fallback.endpoint.as_deref(), Some("http://tempo:4317"));
    assert_eq!(fallback.service_name.as_deref(), Some("xmtp-backend"));
    assert!(!fallback.logs);
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}log_format = 'json'\n[telemetry]\nmetrics_listen = ''\notlp_endpoint = 'http://collector:4317'\notlp_logs = true\nservice_name = 'custom'\nsample_ratio = 0.25\nresource_attributes = {{ 'deployment.environment' = 'test' }}"
    ))?;
    config.validate()?;
    assert_eq!(config.server.log_format, LogFormat::Json);
    let logging = config
        .telemetry
        .with_endpoint_fallback(IDENTIFIER, Some("malformed-fallback-secret".into()))?
        .unwrap();
    assert_eq!(logging.endpoint.as_deref(), Some("http://collector:4317"));
    assert_eq!(logging.service_name.as_deref(), Some("custom"));
    assert!(logging.logs);
    assert_eq!(logging.sample_ratio, 0.25);
    // The identifier is added to every export, alongside whatever the operator set.
    assert_eq!(
        logging.resource_attributes,
        vec![
            ("deployment.environment".into(), "test".into()),
            ("xmtp.backend.identifier".into(), IDENTIFIER.into())
        ]
    );
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-022
fn telemetry_rejects_unknown_reserved_and_invalid_values_without_endpoint_contents() {
    assert!(toml::from_str::<Config>(&format!("{MINIMAL}\n[telemetry]\nunknown = true")).is_err());
    for key in ["service.name", "service.version", "xmtp.backend.identifier"] {
        let config: Config = toml::from_str(&format!(
            "{MINIMAL}\n[telemetry.resource_attributes]\n'{key}' = 'override'"
        ))?;
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("telemetry.resource_attributes"));
        assert!(error.contains(key));
    }
    for endpoint in [
        "secret-invalid",
        "ftp://secret",
        "http://[secret",
        "http://host/secret value",
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.telemetry.otlp_endpoint = Some(endpoint.into());
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("telemetry.otlp_endpoint"));
        assert!(!error.contains(endpoint));
        config.telemetry.otlp_endpoint = None;
        let error = config
            .telemetry
            .with_endpoint_fallback(IDENTIFIER, Some(endpoint.into()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("OTEL_EXPORTER_OTLP_ENDPOINT"));
        assert!(!error.contains(endpoint));
    }
    for ratio in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.telemetry.sample_ratio = ratio;
        assert!(config.validate().is_err());
    }
    let bad = write_config(
        "telemetry-endpoint",
        &format!("{MINIMAL}\n[telemetry]\notlp_endpoint = 'env:PATH'"),
    );
    let error = Config::load(&bad.0).unwrap_err().to_string();
    assert!(error.contains("telemetry.otlp_endpoint"));
    assert!(!error.contains(&std::env::var("PATH")?));
}

#[xmtp_common::test(unwrap_try = true)]
fn telemetry_string_values_resolve_environment_references_once() {
    let file = write_config(
        "telemetry-env",
        &format!(
            "{MINIMAL}\n[telemetry]\nservice_name = 'env:PATH'\nresource_attributes = {{ custom = 'env:PATH' }}"
        ),
    );
    let config = Config::load(&file.0)?;
    assert_eq!(config.telemetry.service_name, std::env::var("PATH")?);
    assert_eq!(
        config.telemetry.resource_attributes["custom"],
        std::env::var("PATH")?
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn inline_configuration_resolves_environment_references() {
    let config = Config::load_str(&format!(
        "{MINIMAL}\n[telemetry]\nresource_attributes = {{ path = 'env:PATH' }}\n"
    ))?;
    assert_eq!(
        config.telemetry.resource_attributes["path"],
        std::env::var("PATH")?
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn malformed_inline_configuration_omits_document_contents() {
    // Guard the unit variant: retaining a TOML parser error could expose input text.
    let error = Config::load_str("secret-inline-sentinel = [").unwrap_err();
    assert!(matches!(error, ConfigError::Parse));
    assert!(!format!("{error} {error:?}").contains("secret-inline-sentinel"));
}

/// A deployment that does not name itself cannot be bound to by a client
/// database, so an unnamed or malformed identifier stops the process.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-002
fn the_identifier_is_required_and_shaped() {
    let missing: Config = toml::from_str("[database]\nurl = 'postgres://localhost/xmtp'\n")?;
    let error = missing.validate().unwrap_err();
    assert!(matches!(
        error,
        ConfigError::Invalid {
            field: "server.identifier",
            ..
        }
    ));
    for identifier in [
        String::new(),
        "a".repeat(MAX_IDENTIFIER_BYTES + 1),
        // A single multi-byte character can also push a short name over the
        // byte bound, which is what the rule counts.
        "é".repeat(MAX_IDENTIFIER_BYTES / 2 + 1),
        "org.xmtp has a space".into(),
        "org.xmtp\u{0}dev".into(),
        "org.xmtp\ndev".into(),
    ] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.server.identifier = Some(identifier.clone());
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::Invalid {
                    field: "server.identifier",
                    ..
                })
            ),
            "{identifier:?} must be rejected"
        );
    }
    // The documented convention is accepted at the byte bound.
    let mut config: Config = toml::from_str(MINIMAL)?;
    config.server.identifier = Some("a".repeat(MAX_IDENTIFIER_BYTES));
    config.validate()?;
}

/// An existing file that gained `[auth]` before this rule must state its
/// intent, so auth can never switch off by accident. A section that says it is
/// off is not checked any further and loads no key material.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-005
fn an_auth_section_must_state_whether_it_is_enabled() {
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}\n[auth]\njwks_url = 'https://issuer.example/keys'\n"
    ))?;
    assert!(matches!(
        config.validate().unwrap_err(),
        ConfigError::Invalid {
            field: "auth.enabled",
            ..
        }
    ));
    // Disabled auth ignores every other field, including ones that would fail
    // their own checks, and publishes an empty summary.
    let config: Config = toml::from_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\naudiences = []\nissuers = []\n"
    ))?;
    config.validate()?;
    assert!(!config.auth.as_ref()?.is_enabled());
    assert_eq!(
        config.configuration_response(&[]).auth,
        Some(api::AuthConfiguration::default())
    );
    // Ignoring a field is not the same as never reading it: environment
    // references resolve for the whole document before any section is
    // validated, so a missing variable in a disabled section still fails.
    let error = Config::load_str(&format!(
        "{MINIMAL}\n[auth]\nenabled = false\njwks_url = 'env:XMTP_AUTH_DISABLED_MISSING'\n"
    ))
    .unwrap_err();
    assert!(
        matches!(&error, ConfigError::Environment { name } if name == "XMTP_AUTH_DISABLED_MISSING")
    );
}

/// An operator states a minimum client version as a semantic version, or
/// states none and admits every client version.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-067
fn the_minimum_client_version_is_optional_and_semantic() {
    for version in ["1", "1.2", "v1.2.3", "latest", "1.2.3.4", ""] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        config.server.min_libxmtp_version = Some(version.into());
        assert!(
            matches!(
                config.validate(),
                Err(ConfigError::Invalid {
                    field: "server.min_libxmtp_version",
                    ..
                })
            ),
            "{version:?} must be rejected"
        );
    }
    let config: Config = toml::from_str(MINIMAL)?;
    config.validate()?;
    assert!(
        config
            .configuration_response(&[])
            .min_libxmtp_version
            .is_empty()
    );
    let mut config: Config = toml::from_str(MINIMAL)?;
    config.server.min_libxmtp_version = Some("1.2.3-beta.1".into());
    config.validate()?;
    assert_eq!(
        config.configuration_response(&[]).min_libxmtp_version,
        "1.2.3-beta.1"
    );
}

/// The transport ceiling is fixed, so a budget above it would promise a client
/// something the transport cannot carry.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-008
fn request_and_response_budgets_stop_at_the_transport_ceiling() {
    for field in ["limits.max_request_bytes", "limits.max_response_bytes"] {
        let mut config: Config = toml::from_str(MINIMAL)?;
        match field {
            "limits.max_request_bytes" => config.limits.max_request_bytes = MAX_TRANSPORT_BYTES,
            _ => config.limits.max_response_bytes = MAX_TRANSPORT_BYTES,
        }
        config.validate()?;
        match field {
            "limits.max_request_bytes" => config.limits.max_request_bytes += 1,
            _ => config.limits.max_response_bytes += 1,
        }
        let error = config.validate().unwrap_err();
        assert!(
            matches!(&error, ConfigError::Invalid { field: named, .. } if *named == field),
            "{field} must be named, got {error}"
        );
    }
}

/// The auth lists, the key list, and the chain list have no individual bound,
/// so the assembled response is what is measured.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-009
fn an_oversized_published_response_stops_startup() {
    let mut config: Config = toml::from_str(MINIMAL)?;
    let chains = |config: &mut Config, count: u64| {
        config.chains = (0..count)
            .map(|index| (format!("eip155:{index}"), "https://rpc.example".into()))
            .collect();
    };
    chains(&mut config, 1_000);
    config.validate()?;
    chains(&mut config, 10_000);
    let error = config.validate().unwrap_err();
    assert!(matches!(
        &error,
        ConfigError::Invalid {
            field: "configuration response",
            ..
        }
    ));
    // The bound is on the encoded response, not on any one list.
    assert!(
        prost::Message::encoded_len(&config.configuration_response(&[]))
            > MAX_CONFIGURATION_RESPONSE_BYTES,
        "the fixture must exceed the bound"
    );
}
